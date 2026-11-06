package prepull

import (
	"encoding/json"
	"strings"
	"testing"
	"time"

	"github.com/proflyder/monorepo/platform/deploy/internal/kube"
)

type fakeClock struct{ now time.Time }

func (c *fakeClock) Now() time.Time          { return c.now }
func (c *fakeClock) Sleep(d time.Duration)   { c.now = c.now.Add(d) }
func newClock() *fakeClock                   { return &fakeClock{now: time.Unix(1_800_000_000, 0)} }
func podJSON(imageID, waiting string) string { return podJSONMsg(imageID, waiting, "") }

func podJSONMsg(imageID, waiting, message string) string {
	state := map[string]any{}
	if waiting != "" {
		state["waiting"] = map[string]string{"reason": waiting, "message": message}
	}
	body, _ := json.Marshal(map[string]any{"status": map[string]any{
		"containerStatuses": []any{map[string]any{"imageID": imageID, "state": state}},
	}})
	return string(body)
}

// script answers `get pod` calls per pod name from a queue of bodies; the last
// body repeats.
func script(bodies map[string][]string) func([]string) kube.Result {
	return func(args []string) kube.Result {
		if len(args) > 3 && args[2] == "get" {
			name := args[4]
			queue := bodies[name]
			body := queue[0]
			if len(queue) > 1 {
				bodies[name] = queue[1:]
			}
			return kube.Result{Stdout: body}
		}
		return kube.Result{}
	}
}

func verbs(calls [][]string) []string {
	out := make([]string, 0, len(calls))
	for _, call := range calls {
		out = append(out, call[2])
	}
	return out
}

func TestRunWaitsForEveryImageAndCleansUp(t *testing.T) {
	var calls [][]string
	clock := newClock()
	runner := kube.Fake(script(map[string][]string{
		"prepull-abc-0": {podJSON("", "ContainerCreating"), podJSON("sha256:1", "")},
		"prepull-abc-1": {podJSON("", ""), podJSON("", "RunContainerError")},
	}), &calls)
	var logged []string
	err := Run(runner, Options{
		Namespace: "ns", Images: []string{"img/a@sha256:1", "img/b@sha256:2", "img/a@sha256:1", ""},
		Timeout: time.Minute, RunID: "abc", Now: clock.Now, Sleep: clock.Sleep,
		Log: func(line string) { logged = append(logged, line) },
	})
	if err != nil {
		t.Fatalf("Run: %v", err)
	}
	got := strings.Join(verbs(calls), ",")
	if !strings.HasPrefix(got, "run,run,") || !strings.HasSuffix(got, ",delete") {
		t.Fatalf("calls = %s", got)
	}
	if len(logged) != 2 {
		t.Fatalf("logged = %v", logged)
	}
	create := strings.Join(calls[0], " ")
	for _, want := range []string{"--image=img/a@sha256:1", "--restart=Never", "--image-pull-policy=IfNotPresent", "--labels=home-deploy/prepull=abc", `"name":"prepull-abc-0"`, `"runAsNonRoot":true`, `"readOnlyRootFilesystem":true`} {
		if !strings.Contains(create, want) {
			t.Errorf("create call misses %s: %s", want, create)
		}
	}
	last := strings.Join(calls[len(calls)-1], " ")
	if last != "-n ns delete pod -l home-deploy/prepull=abc --ignore-not-found --wait=false" {
		t.Errorf("cleanup = %s", last)
	}
}

func TestRunTimesOutWithThePullReasonAndStillCleansUp(t *testing.T) {
	var calls [][]string
	clock := newClock()
	runner := kube.Fake(script(map[string][]string{
		"prepull-x-0": {podJSONMsg("", "ImagePullBackOff", "Back-off pulling image")},
	}), &calls)
	err := Run(runner, Options{Namespace: "ns", Images: []string{"img/slow"}, Timeout: 10 * time.Second, RunID: "x", Now: clock.Now, Sleep: clock.Sleep})
	if err == nil || !strings.Contains(err.Error(), "img/slow (ImagePullBackOff: Back-off pulling image)") {
		t.Fatalf("err = %v", err)
	}
	if verbs(calls)[len(calls)-1] != "delete" {
		t.Fatalf("no cleanup: %v", verbs(calls))
	}
}

func TestRunCleansUpWhenCreateFails(t *testing.T) {
	var calls [][]string
	runner := kube.Fake(func(args []string) kube.Result {
		if args[2] == "run" && strings.Contains(strings.Join(args, " "), "img/bad") {
			return kube.Result{Code: 1, Stderr: "forbidden"}
		}
		return kube.Result{}
	}, &calls)
	err := Run(runner, Options{Namespace: "ns", Images: []string{"img/ok", "img/bad"}, Timeout: time.Minute, RunID: "r"})
	if err == nil || !strings.Contains(err.Error(), "img/bad: forbidden") {
		t.Fatalf("err = %v", err)
	}
	if got := strings.Join(verbs(calls), ","); got != "run,run,delete" {
		t.Fatalf("calls = %s", got)
	}
}

func TestRunWithoutImagesDoesNothing(t *testing.T) {
	var calls [][]string
	if err := Run(kube.Fake(func([]string) kube.Result { return kube.Result{} }, &calls), Options{Images: []string{"", " "}}); err != nil {
		t.Fatal(err)
	}
	if len(calls) != 0 {
		t.Fatalf("calls = %v", calls)
	}
}

func TestStatusTreatsPostPullStatesAsPulled(t *testing.T) {
	for _, tc := range []struct {
		body   string
		pulled bool
	}{
		{podJSON("", "ContainerCreating"), false},
		{podJSON("", "ErrImagePull"), false},
		{podJSON("", "CreateContainerError"), true},
		{podJSON("docker-pullable://img@sha256:1", ""), true},
		{`{"status":{"containerStatuses":[{"state":{"terminated":{"exitCode":128}}}]}}`, true},
		{`{"status":{}}`, false},
	} {
		runner := func([]string) kube.Result { return kube.Result{Stdout: tc.body} }
		pulled, _, err := status(runner, "ns", "p")
		if err != nil || pulled != tc.pulled {
			t.Errorf("status(%s) = %v, %v; want %v", tc.body, pulled, err, tc.pulled)
		}
	}
}
