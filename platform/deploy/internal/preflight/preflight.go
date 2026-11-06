// Package preflight runs the read-only safety checks for `deploy.sh --execute`
// (start the executor).
//
// Checks (all read-only), ported from
// `projects/home/kubernetes/scheduler/preflight.py`:
//
//  1. retired legacy HA schedule writers are absent or off (never on/unknown);
//  2. no alarm is currently ringing locally;
//  3. the scheduler API answers authenticated and ready;
//  4. HA's scheduler endpoint points at this scheduler.
//
// The HA token is read from the token file and never printed. The command exits
// 1 on any failed check, with the same messages as the Python script.
package preflight

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"
	"strings"
	"time"
)

// LegacyWriters are the retired HA schedule writers. A missing entity is safe;
// an entity that exists must be exactly "off".
var LegacyWriters = []string{
	"automation.alarm_scheduler",
	"automation.schedules_alarm_dispatcher",
	"automation.schedules_reminder_dispatcher",
	"automation.schedules_timer_finished",
}

// Options is one preflight run.
type Options struct {
	HAURL              string
	HATokenFile        string
	SchedulerURL       string
	SchedulerTokenFile string
	ExpectedEndpoint   string
	// Client overrides the HTTP client (tests). Defaults to a 15s-timeout client.
	Client *http.Client
	Stdout io.Writer
}

// LegacyWriterFailures returns failures for writers that could still execute
// schedules. A missing entity is safe: the retired writer was removed from the
// shipped HA YAML on an upgraded install. An entity that exists must be exactly
// "off"; "on", "unknown", "unavailable" and any other value fail closed.
func LegacyWriterFailures(states map[string]string) []string {
	failures := []string{}
	for _, entity := range LegacyWriters {
		state, ok := states[entity]
		if !ok || state == "off" {
			continue
		}
		failures = append(failures, fmt.Sprintf(
			"legacy writer %s is '%s', expected absent or 'off'", entity, state))
	}
	return failures
}

// Run executes every check and returns the process exit status. Diagnostics go
// to opts.Stdout, matching the Python print() calls.
func Run(options Options) int {
	stdout := options.Stdout
	if stdout == nil {
		stdout = os.Stdout
	}
	client := options.Client
	if client == nil {
		client = &http.Client{Timeout: 15 * time.Second}
	}
	haToken := readToken(options.HATokenFile)
	schedulerToken := readToken(options.SchedulerTokenFile)

	code, states := get(client, strings.TrimRight(options.HAURL, "/")+"/api/states", haToken)
	list, ok := states.([]interface{})
	if code != 200 || !ok {
		fmt.Fprintf(stdout, "FAIL: cannot read HA states (HTTP %d)\n", code)
		return 1
	}
	byID := map[string]string{}
	for _, item := range list {
		entity, ok := item.(map[string]interface{})
		if !ok {
			continue
		}
		id, _ := entity["entity_id"].(string)
		value, _ := entity["state"].(string)
		byID[id] = value
	}

	failures := LegacyWriterFailures(byID)

	if byID["input_boolean.alarm_ringing"] == "on" {
		failures = append(failures, "an alarm is ringing in HA; stop it before starting the executor")
	}

	if options.ExpectedEndpoint != "" {
		actual, present := byID["input_text.scheduler_endpoint"]
		if !present || actual != options.ExpectedEndpoint {
			// Python reports the missing entity as None; keep a readable form.
			shown := "None"
			if present {
				shown = "'" + actual + "'"
			}
			failures = append(failures, fmt.Sprintf(
				"input_text.scheduler_endpoint is %s, expected '%s'", shown, options.ExpectedEndpoint))
		}
	}

	code, status := post(client,
		strings.TrimRight(options.SchedulerURL, "/")+"/home.v1.SchedulerService/GetStatus",
		schedulerToken, "{}")
	statusObject, isObject := status.(map[string]interface{})
	if code != 200 || !isObject {
		failures = append(failures, fmt.Sprintf("scheduler GetStatus failed (HTTP %d)", code))
	} else if ready, _ := statusObject["ready"].(bool); !ready {
		failures = append(failures, "scheduler reports ready=false")
	}

	if len(failures) > 0 {
		for _, failure := range failures {
			fmt.Fprintf(stdout, "FAIL: %s\n", failure)
		}
		return 1
	}

	fmt.Fprintln(stdout, "preflight ok: retired writers absent/off, no ringing alarm, scheduler ready")
	return 0
}

func readToken(path string) string {
	raw, err := os.ReadFile(path)
	if err != nil {
		return ""
	}
	return strings.TrimSpace(string(raw))
}

func get(client *http.Client, url, token string) (int, interface{}) {
	request, err := http.NewRequest(http.MethodGet, url, nil)
	if err != nil {
		return 0, nil
	}
	request.Header.Set("Authorization", "Bearer "+token)
	return perform(client, request)
}

func post(client *http.Client, url, token, payload string) (int, interface{}) {
	request, err := http.NewRequest(http.MethodPost, url, bytes.NewBufferString(payload))
	if err != nil {
		return 0, nil
	}
	request.Header.Set("Authorization", "Bearer "+token)
	request.Header.Set("Content-Type", "application/json")
	return perform(client, request)
}

func perform(client *http.Client, request *http.Request) (int, interface{}) {
	response, err := client.Do(request)
	if err != nil {
		return 0, nil
	}
	defer response.Body.Close()
	if response.StatusCode != http.StatusOK {
		// urllib raises HTTPError and the Python helper drops the body.
		return response.StatusCode, nil
	}
	body, err := io.ReadAll(response.Body)
	if err != nil {
		return response.StatusCode, nil
	}
	text := strings.TrimSpace(string(body))
	if text == "" {
		return response.StatusCode, nil
	}
	var decoded interface{}
	if err := json.Unmarshal([]byte(text), &decoded); err != nil {
		return 0, nil
	}
	return response.StatusCode, decoded
}
