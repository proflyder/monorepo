package hermes

import (
	"bytes"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"testing"
	"time"

	yaml "go.yaml.in/yaml/v2"
)

const (
	hermesDigest = "ghcr.io/proflyder/hermes@sha256:" + "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
	proxyDigest  = "ghcr.io/proflyder/voice-proxy@sha256:" + "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
	exportDigest = "ghcr.io/proflyder/health-exporter@sha256:" + "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
	smokeImage   = "ghcr.io/proflyder/home-deploy@sha256:" + "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"
)

const valuesYAML = `nodeName: lab-host
storage:
  hermes: {size: 10Gi, path: /data}
workloads:
  hermes:
    replicas: 1
    containers:
    - name: gateway
      image: ghcr.io/proflyder/hermes@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
    - name: voice-proxy
      image: ghcr.io/proflyder/voice-proxy@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
`

func fakeRunner(handler func(Command) Result, calls *[]Command) Runner {
	return func(command Command) Result {
		*calls = append(*calls, command)
		return handler(command)
	}
}

func allUpMetrics() string {
	var lines []string
	for _, name := range ExpectedEndpoints {
		lines = append(lines, `ricardo_endpoint_up{endpoint="`+name+`"} 1`)
	}
	return strings.Join(lines, "\n")
}

func testOptions(t *testing.T) Options {
	t.Helper()
	return Options{
		ValuesFile:         filepath.Join(t.TempDir(), "values.yaml"),
		Deployment:         "hermes",
		ExporterDeployment: ExporterDeployment,
		Namespace:          "home-assistant",
		HermesDigest:       hermesDigest,
		VoiceProxyDigest:   proxyDigest,
		ExporterDigest:     exportDigest,
		SmokeImage:         smokeImage,
		Chart:              "/chart",
		StateDir:           t.TempDir(),
		RetryWindow:        90 * time.Second,
		RetryDelay:         5 * time.Second,
	}
}

func writeValuesFile(t *testing.T, options *Options) {
	t.Helper()
	if err := os.WriteFile(options.ValuesFile, []byte(valuesYAML), 0o600); err != nil {
		t.Fatal(err)
	}
}

// --- digests -------------------------------------------------------------

func TestAcceptsBareSHA256(t *testing.T) {
	got, err := NormalizeDigest("hermes", "sha256:"+strings.Repeat("c", 64))
	if err != nil || got != "ghcr.io/proflyder/hermes@sha256:"+strings.Repeat("c", 64) {
		t.Fatalf("got %q, %v", got, err)
	}
}

func TestAcceptsPinnedReference(t *testing.T) {
	got, err := NormalizeDigest("voice-proxy", proxyDigest)
	if err != nil || got != proxyDigest {
		t.Fatalf("got %q, %v", got, err)
	}
}

func TestRejectsTag(t *testing.T) {
	for _, value := range []string{"develop", "ghcr.io/proflyder/hermes:develop"} {
		if _, err := NormalizeDigest("hermes", value); err == nil {
			t.Fatalf("%q must be rejected", value)
		}
	}
}

func TestImageDigest(t *testing.T) {
	if got := ImageDigest(hermesDigest); got != "sha256:"+strings.Repeat("a", 64) {
		t.Fatalf("image digest = %q", got)
	}
}

// --- values --------------------------------------------------------------

func TestWorkloadValuesDropsStorage(t *testing.T) {
	values, err := LoadValues(writeTempValues(t))
	if err != nil {
		t.Fatal(err)
	}
	workloads := WorkloadValues(values)
	for _, item := range workloads {
		if item.Key == "storage" {
			t.Fatal("storage survived")
		}
	}
	if len(workloads) != 2 {
		t.Fatalf("unexpected keys: %v", workloads)
	}
}

func writeTempValues(t *testing.T) string {
	t.Helper()
	path := filepath.Join(t.TempDir(), "values.yaml")
	if err := os.WriteFile(path, []byte(valuesYAML), 0o600); err != nil {
		t.Fatal(err)
	}
	return path
}

func TestWriteValuesRoundTripsWithoutStorage(t *testing.T) {
	values, err := LoadValues(writeTempValues(t))
	if err != nil {
		t.Fatal(err)
	}
	path, err := WriteValues(WorkloadValues(values))
	if err != nil {
		t.Fatal(err)
	}
	defer os.Remove(path)
	info, err := os.Stat(path)
	if err != nil {
		t.Fatal(err)
	}
	if info.Mode().Perm() != 0o600 {
		t.Fatalf("mode = %v", info.Mode())
	}
	raw, _ := os.ReadFile(path)
	var loaded yaml.MapSlice
	if err := yaml.Unmarshal(raw, &loaded); err != nil {
		t.Fatal(err)
	}
	if _, present := mappingValue(loaded, "storage"); present {
		t.Fatal("storage survived the round trip")
	}
	workloads, _ := mappingValue(loaded, "workloads")
	hermesWorkload, _ := mappingValue(workloads, "hermes")
	if _, ok := sequenceValue(hermesWorkload, "containers"); !ok {
		t.Fatal("containers lost")
	}
}

func TestLoadValuesRequiresTwoContainers(t *testing.T) {
	path := filepath.Join(t.TempDir(), "values.yaml")
	os.WriteFile(path, []byte("workloads:\n  hermes:\n    containers:\n    - name: gateway\n"), 0o600)
	if _, err := LoadValues(path); err == nil {
		t.Fatal("expected a two-container requirement")
	}
}

// --- rollout decision ----------------------------------------------------

func TestNoNewDigestSkips(t *testing.T) {
	if RolloutNeeded(map[string]string{"gateway": hermesDigest, "voice-proxy": proxyDigest}, hermesDigest, proxyDigest) {
		t.Fatal("unchanged digests must skip the rollout")
	}
}

func TestOneNewDigestRolls(t *testing.T) {
	newer := "ghcr.io/proflyder/voice-proxy@sha256:" + strings.Repeat("d", 64)
	if !RolloutNeeded(map[string]string{"gateway": hermesDigest, "voice-proxy": proxyDigest}, hermesDigest, newer) {
		t.Fatal("a changed digest must roll")
	}
}

func TestSetArgsTargetContainerIndexes(t *testing.T) {
	args := strings.Join(SetArgs(hermesDigest, proxyDigest), " ")
	if !strings.Contains(args, "workloads.hermes.containers[0].image="+hermesDigest) {
		t.Fatalf("gateway index missing: %s", args)
	}
	if !strings.Contains(args, "workloads.hermes.containers[1].image="+proxyDigest) {
		t.Fatalf("voice-proxy index missing: %s", args)
	}
	if strings.Contains(args, "--reuse-values") {
		t.Fatal("--reuse-values must not be used")
	}
}

func TestExporterArgsPinImageAndDropTheScriptMount(t *testing.T) {
	args := strings.Join(ExporterSetArgs("runtime-health-exporter", exportDigest), " ")
	for _, want := range []string{
		"workloads.runtime-health-exporter.containers[0].image=" + exportDigest,
		`command=["/usr/local/bin/health-exporter"]`,
		"workloads.runtime-health-exporter.containers[0].volumeMounts=[]",
		"workloads.runtime-health-exporter.volumes=[]",
	} {
		if !strings.Contains(args, want) {
			t.Fatalf("missing %q in %s", want, args)
		}
	}
}

// --- health --------------------------------------------------------------

func TestHealthAllEndpointsUp(t *testing.T) {
	if err := CheckHealth(allUpMetrics()); err != nil {
		t.Fatal(err)
	}
}

func TestHealthDownEndpointFails(t *testing.T) {
	text := strings.Replace(allUpMetrics(), `endpoint="hermes"} 1`, `endpoint="hermes"} 0`, 1)
	if err := CheckHealth(text); err == nil {
		t.Fatal("a down endpoint must fail")
	}
}

func TestHealthMissingEndpointFails(t *testing.T) {
	text := strings.Replace(allUpMetrics(), `ricardo_endpoint_up{endpoint="voice_proxy"} 1`, "", 1)
	if err := CheckHealth(text); err == nil {
		t.Fatal("a missing endpoint must fail")
	}
}

// --- voice logs ----------------------------------------------------------

func TestCleanLog(t *testing.T) {
	if err := CheckVoiceLogs("listening on 0.0.0.0:8643\nrequest ok"); err != nil {
		t.Fatal(err)
	}
}

func TestErrorLogFails(t *testing.T) {
	if err := CheckVoiceLogs("Traceback (most recent call last):\nValueError"); err == nil {
		t.Fatal("a traceback must fail")
	}
}

func TestWordNotMatchedInsideOtherWords(t *testing.T) {
	if got := VoiceProxyLogErrors("errors=0; no problem"); len(got) != 0 {
		t.Fatalf("unexpected matches: %v", got)
	}
}

// --- smoke job -----------------------------------------------------------

func TestJobMountsExistingSecretAndHasNoToken(t *testing.T) {
	job := SmokeJob("hermes-smoke-1", "home-assistant", smokeImage)
	raw, _ := yaml.Marshal(job)
	text := string(raw)
	if !strings.Contains(text, "secretName: ha-bridge-ha") {
		t.Fatal("the smoke Job must mount the existing ha-bridge-ha Secret")
	}
	if strings.Contains(text, "HA_TOKEN") {
		t.Fatal("the Job must not take an HA token value")
	}
	if !strings.Contains(text, "conversation.hermes_agent") {
		t.Fatal("the Job must name the Hermes agent")
	}
}

func TestJobDoesNotDependOnTheVoiceProxyImage(t *testing.T) {
	job := SmokeJob("x", "home-assistant", smokeImage)
	spec := job["spec"].(map[string]interface{})["template"].(map[string]interface{})["spec"].(map[string]interface{})
	container := spec["containers"].([]interface{})[0].(map[string]interface{})
	if container["image"] != smokeImage {
		t.Fatalf("image = %v", container["image"])
	}
	if strings.Contains(container["image"].(string), "voice-proxy") {
		t.Fatal("the smoke Job must not use the distroless proxy image")
	}
	if !regexp.MustCompile(`@sha256:[0-9a-f]{64}$`).MatchString(container["image"].(string)) {
		t.Fatal("the smoke image must be digest-pinned")
	}
	command := strings.Join(toStrings(container["command"]), " ")
	if !strings.Contains(command, "home-deploy") || !strings.Contains(command, "assist-smoke") {
		t.Fatalf("command = %q", command)
	}
}

func toStrings(value interface{}) []string {
	items, _ := value.([]interface{})
	result := make([]string, 0, len(items))
	for _, item := range items {
		result = append(result, item.(string))
	}
	return result
}

func TestJobIsHardened(t *testing.T) {
	job := SmokeJob("x", "home-assistant", smokeImage)
	spec := job["spec"].(map[string]interface{})["template"].(map[string]interface{})["spec"].(map[string]interface{})
	if spec["automountServiceAccountToken"] != false {
		t.Fatal("the Job must not automount a service account token")
	}
	container := spec["containers"].([]interface{})[0].(map[string]interface{})
	security := container["securityContext"].(map[string]interface{})
	if security["allowPrivilegeEscalation"] != false || security["readOnlyRootFilesystem"] != true {
		t.Fatalf("container is not hardened: %v", security)
	}
}

// --- healthcheck and smoke composition -----------------------------------

func TestHealthcheckExecsTheDistrolessSubcommand(t *testing.T) {
	var calls []Command
	options := testOptions(t)
	options.Namespace = "home-assistant"
	run := fakeRunner(func(Command) Result { return Result{} }, &calls)
	if err := VoiceProxyHealthcheck(run, options); err != nil {
		t.Fatal(err)
	}
	want := strings.Join([]string{
		"-n", "home-assistant", "exec", "deployment/hermes",
		"-c", "voice-proxy", "--", "/usr/local/bin/voice-proxy", "-healthcheck",
	}, " ")
	if got := strings.Join(calls[0].Args, " "); got != want {
		t.Fatalf("argv = %q, want %q", got, want)
	}
}

func smokeHandler() func(Command) Result {
	return func(command Command) Result {
		switch {
		case contains(command.Args, "rollout"):
			return Result{}
		case strings.Contains(strings.Join(command.Args, " "), "health-exporter"):
			return Result{Stdout: allUpMetrics()}
		case contains(command.Args, "logs"):
			return Result{}
		default:
			return Result{}
		}
	}
}

func contains(args []string, value string) bool {
	for _, item := range args {
		if item == value {
			return true
		}
	}
	return false
}

func TestRunSmokeProbesTheProxyAfterTheRollout(t *testing.T) {
	var calls []Command
	options := testOptions(t)
	options.Namespace = "home-assistant"
	options.AssistAttempt = func() (map[string]interface{}, error) {
		return map[string]interface{}{"ok": true, "speech": "Готов."}, nil
	}
	run := fakeRunner(smokeHandler(), &calls)
	if err := RunSmoke(run, options, false, true); err != nil {
		t.Fatal(err)
	}
	found := false
	for _, call := range calls {
		if strings.Join(call.Args, " ") == "-n home-assistant exec deployment/hermes -c voice-proxy -- /usr/local/bin/voice-proxy -healthcheck" {
			found = true
		}
	}
	if !found {
		t.Fatal("the proxy healthcheck did not run")
	}
}

func TestRollbackVerificationSkipsTheGoOnlyProbe(t *testing.T) {
	var calls []Command
	options := testOptions(t)
	options.Namespace = "home-assistant"
	options.AssistAttempt = func() (map[string]interface{}, error) {
		return map[string]interface{}{"ok": true, "speech": "Готов."}, nil
	}
	run := fakeRunner(smokeHandler(), &calls)
	if err := RunSmoke(run, options, false, false); err != nil {
		t.Fatal(err)
	}
	for _, call := range calls {
		if strings.Contains(strings.Join(call.Args, " "), "voice-proxy -healthcheck") {
			t.Fatalf("rollback verification must not run the Go-only probe: %v", call.Args)
		}
	}
}

// --- assist retry --------------------------------------------------------

func assistOptions(t *testing.T, now *time.Time) Options {
	t.Helper()
	options := testOptions(t)
	options.Namespace = "home-assistant"
	options.Clock = func() time.Time { return *now }
	options.Sleep = func(duration time.Duration) { *now = now.Add(duration) }
	return options
}

func TestTransient500IsRetried(t *testing.T) {
	now := time.Unix(0, 0)
	options := assistOptions(t, &now)
	attempts := 0
	speech, err := AssistSmoke(options, func() (map[string]interface{}, error) {
		attempts++
		if attempts == 1 {
			return map[string]interface{}{"ok": false, "status": 500, "error": "HTTPError: 500 runtime_draining"}, nil
		}
		return map[string]interface{}{"ok": true, "speech": "Готов."}, nil
	})
	if err != nil || speech != "Готов." || attempts != 2 {
		t.Fatalf("speech=%q attempts=%d err=%v", speech, attempts, err)
	}
	if now.Sub(time.Unix(0, 0)) != 5*time.Second {
		t.Fatalf("slept %s", now.Sub(time.Unix(0, 0)))
	}
}

func TestConnectionErrorIsRetried(t *testing.T) {
	now := time.Unix(0, 0)
	options := assistOptions(t, &now)
	attempts := 0
	speech, err := AssistSmoke(options, func() (map[string]interface{}, error) {
		attempts++
		if attempts == 1 {
			return map[string]interface{}{"ok": false, "error": "URLError: [Errno 111] Connection refused"}, nil
		}
		return map[string]interface{}{"ok": true, "speech": "Готов."}, nil
	})
	if err != nil || speech != "Готов." || attempts != 2 {
		t.Fatalf("speech=%q attempts=%d err=%v", speech, attempts, err)
	}
}

func Test4xxIsNotRetried(t *testing.T) {
	now := time.Unix(0, 0)
	options := assistOptions(t, &now)
	attempts := 0
	if _, err := AssistSmoke(options, func() (map[string]interface{}, error) {
		attempts++
		return map[string]interface{}{"ok": false, "status": 401}, nil
	}); err == nil {
		t.Fatal("401 must fail")
	}
	if attempts != 1 {
		t.Fatalf("attempts = %d", attempts)
	}
}

func TestEmptySpeechIsNotRetried(t *testing.T) {
	now := time.Unix(0, 0)
	options := assistOptions(t, &now)
	attempts := 0
	if _, err := AssistSmoke(options, func() (map[string]interface{}, error) {
		attempts++
		return map[string]interface{}{"ok": false, "status": 200, "speech": ""}, nil
	}); err == nil {
		t.Fatal("empty speech must fail")
	}
	if attempts != 1 {
		t.Fatalf("attempts = %d", attempts)
	}
}

func TestAssistWindowIsBounded(t *testing.T) {
	now := time.Unix(0, 0)
	options := assistOptions(t, &now)
	options.RetryWindow = 10 * time.Second
	attempts := 0
	_, err := AssistSmoke(options, func() (map[string]interface{}, error) {
		attempts++
		return map[string]interface{}{"ok": false, "status": 503}, nil
	})
	if err == nil || !strings.Contains(err.Error(), "still failing after 10s") {
		t.Fatalf("error = %v", err)
	}
	if attempts != 3 {
		t.Fatalf("attempts = %d", attempts)
	}
}

func TestTransientClassification(t *testing.T) {
	if !TransientSmokeResult(map[string]interface{}{"ok": false, "status": 502}) {
		t.Fatal("502 is transient")
	}
	if !TransientSmokeResult(map[string]interface{}{"ok": false, "error": "TimeoutError: timed out"}) {
		t.Fatal("a timeout is transient")
	}
	if TransientSmokeResult(map[string]interface{}{"ok": false, "status": 404}) {
		t.Fatal("404 is not transient")
	}
	if TransientSmokeResult(map[string]interface{}{"ok": true, "speech": "ok"}) {
		t.Fatal("a success is not transient")
	}
}

// --- rollback ------------------------------------------------------------

func TestRollbackOnlyIsNoopWithoutAppliedMarker(t *testing.T) {
	var calls []Command
	options := testOptions(t)
	options.RollbackOnly = true
	run := fakeRunner(func(Command) Result { return Result{} }, &calls)
	code, err := Run(run, options)
	if err != nil || code != 0 {
		t.Fatalf("code=%d err=%v", code, err)
	}
	if len(calls) != 0 {
		t.Fatalf("unexpected commands: %v", calls)
	}
}

func TestRollbackOnlyUsesRecordedRevision(t *testing.T) {
	options := testOptions(t)
	options.RollbackOnly = true
	os.WriteFile(filepath.Join(options.StateDir, "upgrade-applied"), []byte("1"), 0o600)
	os.WriteFile(filepath.Join(options.StateDir, "previous-revision"), []byte("14"), 0o600)
	var calls []Command
	var stdout bytes.Buffer
	options.Stdout = &stdout
	run := fakeRunner(func(Command) Result { return Result{} }, &calls)
	code, err := Run(run, options)
	if err != nil || code != 0 {
		t.Fatalf("code=%d err=%v", code, err)
	}
	if len(calls) != 2 {
		t.Fatalf("calls = %v", calls)
	}
	if got := strings.Join(calls[0].Args, " "); got != "rollback runtime 14 -n home-assistant --wait=legacy --timeout=8m" {
		t.Fatalf("rollback argv = %q", got)
	}
}

// --- full rollout --------------------------------------------------------

type helmScenario struct {
	liveImages   map[string]string
	exporterLive map[string]string
	revision     int
	smokeFails   bool
}

func rolloutRunner(t *testing.T, scenario *helmScenario, calls *[]Command) Runner {
	t.Helper()
	return fakeRunner(func(command Command) Result {
		joined := strings.Join(command.Args, " ")
		switch {
		case strings.Contains(joined, "get pod prepull-"):
			// Every prepulled image is already on the node.
			return Result{Stdout: `{"status":{"containerStatuses":[{"imageID":"sha256:on-node","state":{}}]}}`}
		case strings.Contains(joined, "get deployment hermes"):
			payload := map[string]interface{}{
				"spec": map[string]interface{}{
					"template": map[string]interface{}{
						"spec": map[string]interface{}{
							"containers": []interface{}{},
						},
					},
				},
			}
			containers := payload["spec"].(map[string]interface{})["template"].(map[string]interface{})["spec"].(map[string]interface{})["containers"].([]interface{})
			for name, image := range scenario.liveImages {
				containers = append(containers, map[string]interface{}{"name": name, "image": image})
			}
			payload["spec"].(map[string]interface{})["template"].(map[string]interface{})["spec"].(map[string]interface{})["containers"] = containers
			raw, _ := json.Marshal(payload)
			return Result{Stdout: string(raw)}
		case strings.Contains(joined, "get deployment runtime-health-exporter"):
			payload := map[string]interface{}{
				"spec": map[string]interface{}{
					"template": map[string]interface{}{
						"spec": map[string]interface{}{
							"containers": []interface{}{
								map[string]interface{}{"name": "health-exporter", "image": scenario.exporterLive["health-exporter"]},
							},
						},
					},
				},
			}
			raw, _ := json.Marshal(payload)
			return Result{Stdout: string(raw)}
		case strings.Contains(joined, "history"):
			raw, _ := json.Marshal([]map[string]int{{"revision": scenario.revision}})
			return Result{Stdout: string(raw)}
		case strings.Contains(joined, "upgrade"):
			scenario.revision++
			return Result{}
		case strings.Contains(joined, "rollback"):
			return Result{}
		case strings.Contains(joined, "health-exporter"):
			if scenario.smokeFails {
				return Result{Stdout: strings.Replace(allUpMetrics(), `endpoint="hermes"} 1`, `endpoint="hermes"} 0`, 1)}
			}
			return Result{Stdout: allUpMetrics()}
		default:
			return Result{}
		}
	}, calls)
}

func TestRunSkipsWhenDigestsAreUnchanged(t *testing.T) {
	options := testOptions(t)
	writeValuesFile(t, &options)
	scenario := &helmScenario{
		liveImages:   map[string]string{"gateway": hermesDigest, "voice-proxy": proxyDigest},
		exporterLive: map[string]string{"health-exporter": exportDigest},
		revision:     3,
	}
	var calls []Command
	var stdout bytes.Buffer
	options.Stdout = &stdout
	code, err := Run(rolloutRunner(t, scenario, &calls), options)
	if err != nil || code != 0 {
		t.Fatalf("code=%d err=%v", code, err)
	}
	if !strings.Contains(stdout.String(), "nothing to do") {
		t.Fatalf("output = %q", stdout.String())
	}
	for _, call := range calls {
		if contains(call.Args, "upgrade") {
			t.Fatal("an unchanged build must not upgrade")
		}
	}
}

func TestRunUpgradesMarksStateAndClearsIt(t *testing.T) {
	options := testOptions(t)
	writeValuesFile(t, &options)
	options.AssistAttempt = func() (map[string]interface{}, error) {
		return map[string]interface{}{"ok": true, "speech": "Готов."}, nil
	}
	scenario := &helmScenario{
		liveImages:   map[string]string{"gateway": proxyDigest, "voice-proxy": proxyDigest},
		exporterLive: map[string]string{"health-exporter": exportDigest},
		revision:     3,
	}
	var calls []Command
	var stdout bytes.Buffer
	options.Stdout = &stdout
	code, err := Run(rolloutRunner(t, scenario, &calls), options)
	if err != nil || code != 0 {
		t.Fatalf("code=%d err=%v", code, err)
	}
	if _, err := os.Stat(filepath.Join(options.StateDir, "upgrade-applied")); err == nil {
		t.Fatal("a successful rollout must remove the upgrade-applied marker")
	}
	previous, _ := os.ReadFile(filepath.Join(options.StateDir, "previous-revision"))
	if strings.TrimSpace(string(previous)) != "3" {
		t.Fatalf("previous-revision = %q", previous)
	}
	var upgrade Command
	for _, call := range calls {
		if contains(call.Args, "upgrade") {
			upgrade = call
		}
	}
	if upgrade.Name == "" {
		t.Fatal("no helm upgrade ran")
	}
	joined := strings.Join(upgrade.Args, " ")
	if !strings.Contains(joined, "containers[0].image="+hermesDigest) ||
		!strings.Contains(joined, "containers[0].image="+exportDigest) {
		t.Fatalf("upgrade argv = %q", joined)
	}
	if !strings.Contains(joined, "workloads.runtime-health-exporter.containers[0].volumeMounts=[]") {
		t.Fatalf("exporter mount not cleared: %q", joined)
	}
}

func TestRunRollsBackOnSmokeFailure(t *testing.T) {
	options := testOptions(t)
	writeValuesFile(t, &options)
	options.AssistAttempt = func() (map[string]interface{}, error) {
		return map[string]interface{}{"ok": true, "speech": "Готов."}, nil
	}
	scenario := &helmScenario{
		liveImages:   map[string]string{"gateway": proxyDigest, "voice-proxy": proxyDigest},
		exporterLive: map[string]string{"health-exporter": exportDigest},
		revision:     3,
		smokeFails:   true,
	}
	var calls []Command
	var stderr bytes.Buffer
	options.Stderr = &stderr
	run := rolloutRunner(t, scenario, &calls)
	code, err := Run(run, options)
	if code != 2 || err != nil {
		t.Fatalf("code=%d err=%v", code, err)
	}
	rolledBack := false
	for _, call := range calls {
		if contains(call.Args, "rollback") {
			rolledBack = true
		}
	}
	if !rolledBack {
		t.Fatal("a failed smoke must roll back")
	}
	if !strings.Contains(stderr.String(), "rolling back to revision 3") {
		t.Fatalf("stderr = %q", stderr.String())
	}
}

func TestRunSimulatedFailureRollsBack(t *testing.T) {
	options := testOptions(t)
	writeValuesFile(t, &options)
	options.SimulateFailure = true
	options.AssistAttempt = func() (map[string]interface{}, error) {
		return map[string]interface{}{"ok": true, "speech": "Готов."}, nil
	}
	scenario := &helmScenario{
		liveImages:   map[string]string{"gateway": proxyDigest, "voice-proxy": proxyDigest},
		exporterLive: map[string]string{"health-exporter": exportDigest},
		revision:     3,
	}
	var calls []Command
	code, err := Run(rolloutRunner(t, scenario, &calls), options)
	if code != 1 || err != nil {
		t.Fatalf("code=%d err=%v", code, err)
	}
}

// TestSmokeClientReportsHTTPStatus keeps the structured result the rollout
// retries on: a warm-up 500 must carry its status code.
func TestSmokeClientReportsHTTPStatus(t *testing.T) {
	server := newTestServer(t, 500, `{}`)
	token := filepath.Join(t.TempDir(), "token")
	os.WriteFile(token, []byte("secret-token\n"), 0o600)
	var stdout bytes.Buffer
	code := AssistSmokeClient(server, "conversation.hermes_agent", "hi", token, 5*time.Second, &stdout)
	if code != 1 {
		t.Fatalf("exit code = %d", code)
	}
	if !strings.Contains(stdout.String(), `"status":500`) {
		t.Fatalf("output = %q", stdout.String())
	}
	if strings.Contains(stdout.String(), "secret-token") {
		t.Fatal("the token leaked into the result")
	}
}

func TestSmokeClientAcceptsSpeech(t *testing.T) {
	server := newTestServer(t, 200, `{"response_type":"action_done","response":{"speech":{"plain":{"speech":"Готов."}}}}`)
	token := filepath.Join(t.TempDir(), "token")
	os.WriteFile(token, []byte("secret-token"), 0o600)
	var stdout bytes.Buffer
	if code := AssistSmokeClient(server, "conversation.hermes_agent", "hi", token, 5*time.Second, &stdout); code != 0 {
		t.Fatalf("exit code = %d (%s)", code, stdout.String())
	}
	if !strings.Contains(stdout.String(), `"ok":true`) || !strings.Contains(stdout.String(), "Готов.") {
		t.Fatalf("output = %q", stdout.String())
	}
}

func TestSmokeClientFailsClosedWithoutToken(t *testing.T) {
	var stdout bytes.Buffer
	if code := AssistSmokeClient("http://127.0.0.1:1", "agent", "hi", "/missing/token", time.Second, &stdout); code != 1 {
		t.Fatalf("exit code = %d", code)
	}
	if !strings.Contains(stdout.String(), `"ok":false`) {
		t.Fatalf("output = %q", stdout.String())
	}
}

// newTestServer starts an HTTP server that answers every request with status and
// body, then returns its URL.
func newTestServer(t *testing.T, status int, body string) string {
	t.Helper()
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, _ *http.Request) {
		writer.WriteHeader(status)
		_, _ = writer.Write([]byte(body))
	}))
	t.Cleanup(server.Close)
	return server.URL
}

func TestHealthMetricsUsesServiceProxyNotExec(t *testing.T) {
	var got []string
	run := func(c Command) Result {
		got = c.Args
		return Result{Code: 0, Stdout: "ricardo_endpoint_up{endpoint=\"ha\"} 1\n"}
	}
	if _, err := HealthMetrics(run, Options{Kubectl: "kubectl", Namespace: "home-assistant", ExporterDeployment: "runtime-health-exporter"}); err != nil {
		t.Fatal(err)
	}
	want := []string{"get", "--raw", "/api/v1/namespaces/home-assistant/services/runtime-health-exporter:9116/proxy/metrics"}
	if strings.Join(got, " ") != strings.Join(want, " ") {
		t.Fatalf("args = %q, want %q", got, want)
	}
}

func TestLogScanIgnoresLowercaseRetryNotes(t *testing.T) {
	text := "voice-proxy: queue retry backoff=2.0s reason=\"scheduler unavailable\"\nvoice-proxy: error=transient\n"
	if errs := VoiceProxyLogErrors(text); len(errs) != 0 {
		t.Fatalf("informational lines flagged: %q", errs)
	}
	for _, line := range []string{"voice-proxy: ERROR announce failed", "panic: runtime error", "Traceback (most recent call last):"} {
		if len(VoiceProxyLogErrors(line)) != 1 {
			t.Fatalf("real error not flagged: %q", line)
		}
	}
}

func TestRunPrepullsEveryImageBeforeTheUpgrade(t *testing.T) {
	options := testOptions(t)
	writeValuesFile(t, &options)
	options.AssistAttempt = func() (map[string]interface{}, error) {
		return map[string]interface{}{"ok": true, "speech": "Готов."}, nil
	}
	scenario := &helmScenario{
		liveImages:   map[string]string{"gateway": proxyDigest, "voice-proxy": proxyDigest},
		exporterLive: map[string]string{"health-exporter": exportDigest},
		revision:     3,
	}
	var calls []Command
	if code, err := Run(rolloutRunner(t, scenario, &calls), options); err != nil || code != 0 {
		t.Fatalf("code=%d err=%v", code, err)
	}
	created, cleanup, upgrade := 0, -1, -1
	for i, call := range calls {
		joined := strings.Join(call.Args, " ")
		switch {
		case contains(call.Args, "run") && strings.Contains(joined, "prepull-"):
			if upgrade >= 0 {
				t.Fatal("prepull ran after the upgrade")
			}
			created++
		case strings.Contains(joined, "delete pod -l home-deploy/prepull="):
			cleanup = i
		case contains(call.Args, "upgrade") && upgrade < 0:
			upgrade = i
		}
	}
	if created != 4 {
		t.Fatalf("prepull pods = %d, want hermes, voice-proxy, exporter and smoke", created)
	}
	if cleanup < 0 || upgrade < 0 || cleanup > upgrade {
		t.Fatalf("cleanup=%d upgrade=%d: prepull pods must be gone before the upgrade", cleanup, upgrade)
	}
}
