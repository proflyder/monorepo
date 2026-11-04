// Package hermes promotes new Hermes/voice-proxy/health-exporter digests into
// the `runtime` Helm release.
//
// It is the Go port of `projects/home-assistant/deploy/hermes_rollout.py` and
// keeps its contract:
//
//  1. read the private runtime values file materialised from OpenBao
//     `secret/ci/deploy-home` key `runtime_values`;
//  2. skip the upgrade when no live image differs from the requested digest (a
//     build that did not change an image must not restart the contour);
//  3. drop the `storage` block and hand the workload values to `helm upgrade`,
//     so the CI ServiceAccount never needs cluster-scoped RBAC;
//  4. remember the deployed revision *before* the upgrade;
//  5. wait for the rollout and run the smoke checks (health exporter, a Hermes
//     Assist turn through the in-cluster HA API, the `voice-proxy -healthcheck`
//     exec, voice-proxy error log scan); the Assist turn retries a bounded
//     warm-up window because a just-started Hermes can answer 500
//     `runtime_draining` before its turn machinery is ready;
//  6. on any failure after a revision was created, `helm rollback` to the
//     remembered revision and re-check the voice circuit.
//
// The HA token is never passed on a command line: the smoke Job mounts the
// existing `ha-bridge-ha` Secret. The Assist request uses the cluster-internal
// HA Service, so no private HA address or token enters GitHub. The Job runs the
// same `home-deploy` image, so no Python image is needed in the cluster.
package hermes

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"os/exec"
	"regexp"
	"strconv"
	"strings"
	"time"

	yaml "go.yaml.in/yaml/v2"
)

// Constants mirrored from the Python rollout.
const (
	Release               = "runtime"
	Namespace             = "home-assistant"
	HAURL                 = "http://home-assistant-external.monitoring.svc.cluster.local:8123"
	HASecret              = "ha-bridge-ha"
	HAAgent               = "conversation.hermes_agent"
	SmokeText             = "Представься одним коротким предложением. Ничего не изменяй."
	ExporterDeployment    = "runtime-health-exporter"
	DefaultTimeout        = "8m"
	DefaultRolloutTimeout = "240s"
)

// ExpectedEndpoints is the health exporter's endpoint set that must be up.
var ExpectedEndpoints = []string{"ha", "music_assistant", "piper", "whisper", "gigaam", "hermes", "voice_proxy"}

var (
	// containerIndex is the position of the container in workloads.hermes.
	containerIndex = map[string]int{"hermes": 0, "voice-proxy": 1}
	// repositories map an image name to its GHCR repository.
	repositories = map[string]string{
		"hermes":          "ghcr.io/proflyder/hermes",
		"voice-proxy":     "ghcr.io/proflyder/voice-proxy",
		"health-exporter": "ghcr.io/proflyder/health-exporter",
	}
	digestPattern  = regexp.MustCompile(`^[^@]+@sha256:[0-9a-f]{64}$`)
	errorPattern   = regexp.MustCompile(`(?m)\b(ERROR|CRITICAL|Traceback)\b|panic:`)
	healthPattern  = regexp.MustCompile(`^ricardo_endpoint_up\{endpoint="([^"]+)"\}\s+([0-9.]+)`)
	transientWords = []string{"URLError", "TimeoutError", "ConnectionError", "ConnectionResetError", "RemoteDisconnected", "timed out", "connection refused"}
)

// Command is one subprocess invocation; Stdin is fed to the process.
type Command struct {
	Name  string
	Args  []string
	Stdin string
}

// Result is what one invocation produced.
type Result struct {
	Stdout string
	Stderr string
	Code   int
}

// Runner executes a Command. It is injectable so the tests never touch a
// cluster or a Helm release.
type Runner func(Command) Result

// Exec returns a Runner that starts the real binary. A failed start is reported
// as exit status 1 with the error on stderr, like the shell would.
func Exec() Runner {
	return func(command Command) Result {
		process := exec.Command(command.Name, command.Args...)
		if command.Stdin != "" {
			process.Stdin = strings.NewReader(command.Stdin)
		}
		var stdout, stderr bytes.Buffer
		process.Stdout = &stdout
		process.Stderr = &stderr
		err := process.Run()
		result := Result{Stdout: stdout.String(), Stderr: stderr.String()}
		if err == nil {
			return result
		}
		var exit *exec.ExitError
		if errors.As(err, &exit) {
			result.Code = exit.ExitCode()
			return result
		}
		result.Code = 1
		if result.Stderr == "" {
			result.Stderr = fmt.Sprint(err)
		}
		return result
	}
}

// RolloutError is one failed step.
type RolloutError struct{ message string }

func (e *RolloutError) Error() string { return e.message }

func failf(format string, args ...interface{}) error {
	return &RolloutError{message: fmt.Sprintf(format, args...)}
}

// Options configures one rollout.
type Options struct {
	ValuesFile         string
	HermesDigest       string
	VoiceProxyDigest   string
	ExporterDigest     string
	SmokeImage         string
	Release            string
	Deployment         string
	ExporterDeployment string
	Namespace          string
	Chart              string
	StateDir           string
	Timeout            string
	Force              bool
	SimulateFailure    bool
	RollbackOnly       bool
	Kubectl            string
	Helm               string
	RetryWindow        time.Duration
	RetryDelay         time.Duration
	Stdout             io.Writer
	Stderr             io.Writer

	// Test hooks; nil means the real implementation.
	Sleep         func(time.Duration)
	Clock         func() time.Time
	AssistAttempt func() (map[string]interface{}, error)
}

func (o *Options) defaults() {
	if o.Release == "" {
		o.Release = Release
	}
	if o.Deployment == "" {
		o.Deployment = "hermes"
	}
	if o.ExporterDeployment == "" {
		o.ExporterDeployment = ExporterDeployment
	}
	if o.Namespace == "" {
		o.Namespace = Namespace
	}
	if o.Timeout == "" {
		o.Timeout = DefaultTimeout
	}
	if o.Kubectl == "" {
		o.Kubectl = "kubectl"
	}
	if o.Helm == "" {
		o.Helm = "helm"
	}
	if o.RetryWindow == 0 {
		o.RetryWindow = 90 * time.Second
	}
	if o.RetryDelay == 0 {
		o.RetryDelay = 5 * time.Second
	}
	if o.Sleep == nil {
		o.Sleep = time.Sleep
	}
	if o.Clock == nil {
		o.Clock = time.Now
	}
	if o.Stdout == nil {
		o.Stdout = io.Discard
	}
	if o.Stderr == nil {
		o.Stderr = io.Discard
	}
}

// NormalizeDigest returns a digest-pinned image reference for name
// (hermes/voice-proxy).
func NormalizeDigest(name, value string) (string, error) {
	repository, known := repositories[name]
	if !known {
		return "", fmt.Errorf("%s: unknown image", name)
	}
	value = strings.TrimSpace(value)
	if value == "" {
		return "", fmt.Errorf("%s: empty digest", name)
	}
	if strings.HasPrefix(value, "sha256:") {
		return repository + "@" + value, nil
	}
	if digestPattern.MatchString(value) {
		return value, nil
	}
	return "", fmt.Errorf("%s: expected sha256:<64 hex> or <repo>@sha256:<64 hex>, got %q", name, value)
}

// ImageDigest returns the `sha256:…` part of a pinned image reference.
func ImageDigest(image string) string {
	_, digest, _ := strings.Cut(image, "@")
	return digest
}

// LoadValues reads the private runtime values and requires at least two
// containers under workloads.hermes (gateway and voice-proxy).
func LoadValues(path string) (yaml.MapSlice, error) {
	raw, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	values := yaml.MapSlice{}
	if len(bytes.TrimSpace(raw)) > 0 {
		if err := yaml.Unmarshal(raw, &values); err != nil {
			return nil, fmt.Errorf("%s: %w", path, err)
		}
	}
	workloads, ok := mappingValue(values, "workloads")
	if !ok {
		return nil, fmt.Errorf("%s: workloads.hermes.containers must hold gateway and voice-proxy", path)
	}
	hermes, ok := mappingValue(workloads, "hermes")
	if !ok {
		return nil, fmt.Errorf("%s: workloads.hermes.containers must hold gateway and voice-proxy", path)
	}
	containers, ok := sequenceValue(hermes, "containers")
	if !ok || len(containers) < 2 {
		return nil, fmt.Errorf("%s: workloads.hermes.containers must hold gateway and voice-proxy", path)
	}
	return values, nil
}

// WorkloadValues drops cluster-scoped storage; apply-runtime.sh owns PV/PVC.
func WorkloadValues(values yaml.MapSlice) yaml.MapSlice {
	result := make(yaml.MapSlice, 0, len(values))
	for _, item := range values {
		if fmt.Sprint(item.Key) == "storage" {
			continue
		}
		result = append(result, item)
	}
	return result
}

// WriteValues writes the workload values to a mode-600 temporary file.
func WriteValues(values yaml.MapSlice) (string, error) {
	raw, err := yaml.Marshal(values)
	if err != nil {
		return "", err
	}
	handle, err := os.CreateTemp("", "hermes-values-*.yaml")
	if err != nil {
		return "", err
	}
	path := handle.Name()
	if _, err := handle.Write(raw); err != nil {
		handle.Close()
		os.Remove(path)
		return "", err
	}
	if err := handle.Close(); err != nil {
		os.Remove(path)
		return "", err
	}
	if err := os.Chmod(path, 0o600); err != nil {
		os.Remove(path)
		return "", err
	}
	return path, nil
}

// LiveImages returns the container name to image map of one Deployment.
func LiveImages(run Runner, kubectl, namespace, deployment string) (map[string]string, error) {
	output, err := checked(run, Command{Name: kubectl, Args: []string{"-n", namespace, "get", "deployment", deployment, "-o", "json"}})
	if err != nil {
		return nil, err
	}
	var payload struct {
		Spec struct {
			Template struct {
				Spec struct {
					Containers []struct {
						Name  string `json:"name"`
						Image string `json:"image"`
					} `json:"containers"`
				} `json:"spec"`
			} `json:"template"`
		} `json:"spec"`
	}
	if err := json.Unmarshal([]byte(output), &payload); err != nil {
		return nil, err
	}
	images := make(map[string]string)
	for _, container := range payload.Spec.Template.Spec.Containers {
		images[container.Name] = container.Image
	}
	return images, nil
}

// RolloutNeeded reports whether any live image differs from the requested
// digest.
func RolloutNeeded(current map[string]string, hermes, voiceProxy string) bool {
	return current["gateway"] != hermes || current["voice-proxy"] != voiceProxy
}

// SetArgs returns the `--set` arguments that pin the Hermes containers.
func SetArgs(hermes, voiceProxy string) []string {
	return []string{
		"--set", fmt.Sprintf("workloads.hermes.containers[%d].image=%s", containerIndex["hermes"], hermes),
		"--set", fmt.Sprintf("workloads.hermes.containers[%d].image=%s", containerIndex["voice-proxy"], voiceProxy),
	}
}

// ExporterSetArgs pins the Go health exporter and drops the old ConfigMap
// script mount, so the private values do not need to change.
func ExporterSetArgs(exporterDeployment, image string) []string {
	container := fmt.Sprintf("workloads.%s.containers[0]", exporterDeployment)
	workload := fmt.Sprintf("workloads.%s", exporterDeployment)
	return []string{
		"--set", container + ".image=" + image,
		"--set-json", container + `.command=["/usr/local/bin/health-exporter"]`,
		"--set-json", container + ".volumeMounts=[]",
		"--set-json", workload + ".volumes=[]",
	}
}

// LatestRevision returns the newest Helm revision, or 0 when history is
// unavailable.
func LatestRevision(run Runner, helm, namespace, release string) int {
	result := run(Command{Name: helm, Args: []string{"-n", namespace, "history", release, "-o", "json"}})
	if result.Code != 0 {
		return 0
	}
	var revisions []struct {
		Revision int `json:"revision"`
	}
	if err := json.Unmarshal([]byte(result.Stdout), &revisions); err != nil {
		return 0
	}
	latest := 0
	for _, revision := range revisions {
		if revision.Revision > latest {
			latest = revision.Revision
		}
	}
	return latest
}

// Upgrade runs the Helm upgrade with the digests pinned by --set.
func Upgrade(run Runner, options Options, chart, valuesPath, hermes, voiceProxy string) Result {
	args := []string{
		"upgrade", "--install", options.Release, chart,
		"-n", options.Namespace, "-f", valuesPath,
	}
	args = append(args, SetArgs(hermes, voiceProxy)...)
	if options.ExporterDigest != "" {
		args = append(args, ExporterSetArgs(options.ExporterDeployment, options.ExporterDigest)...)
	}
	args = append(args, "--wait=legacy", "--timeout="+options.Timeout)
	return run(Command{Name: options.Helm, Args: args})
}

// Rollback restores one revision.
func Rollback(run Runner, options Options, revision int) error {
	_, err := checked(run, Command{Name: options.Helm, Args: []string{
		"rollback", options.Release, strconv.Itoa(revision),
		"-n", options.Namespace, "--wait=legacy", "--timeout=" + options.Timeout,
	}})
	return err
}

// HealthMetrics reads the exporter's /metrics through the API server's service
// proxy: no exec, so it works for the Go exporter, for a rolled-back older
// image and for a distroless container alike.
func HealthMetrics(run Runner, options Options) (string, error) {
	return checked(run, Command{Name: options.Kubectl, Args: []string{
		"get", "--raw", ExporterMetricsPath(options.Namespace, options.ExporterDeployment),
	}})
}

// ExporterMetricsPath is the service-proxy path of the exporter's /metrics.
func ExporterMetricsPath(namespace, service string) string {
	return "/api/v1/namespaces/" + namespace + "/services/" + service + ":9116/proxy/metrics"
}

// ParseHealth parses `ricardo_endpoint_up{endpoint="…"} N`.
func ParseHealth(text string) map[string]int {
	values := make(map[string]int)
	for _, line := range strings.Split(text, "\n") {
		match := healthPattern.FindStringSubmatch(line)
		if match == nil {
			continue
		}
		value, err := strconv.ParseFloat(match[2], 64)
		if err != nil {
			continue
		}
		values[match[1]] = int(value)
	}
	return values
}

// CheckHealth requires every expected endpoint to be present and up.
func CheckHealth(text string) error {
	values := ParseHealth(text)
	var missing []string
	for _, name := range ExpectedEndpoints {
		if _, present := values[name]; !present {
			missing = append(missing, name)
		}
	}
	if len(missing) > 0 {
		return failf("health exporter is missing endpoints: %v", missing)
	}
	down := make(map[string]int)
	for _, name := range ExpectedEndpoints {
		if values[name] != 1 {
			down[name] = values[name]
		}
	}
	if len(down) > 0 {
		return failf("health exporter endpoints are down: %v", down)
	}
	return nil
}

// VoiceProxyLogErrors returns the lines matching the error pattern.
func VoiceProxyLogErrors(text string) []string {
	var errors []string
	for _, line := range strings.Split(text, "\n") {
		if errorPattern.MatchString(line) {
			errors = append(errors, line)
		}
	}
	return errors
}

// CheckVoiceLogs fails when the voice-proxy log contains errors.
func CheckVoiceLogs(text string) error {
	errors := VoiceProxyLogErrors(text)
	if len(errors) == 0 {
		return nil
	}
	sample := errors
	if len(sample) > 10 {
		sample = sample[:10]
	}
	return failf("voice-proxy logs contain errors:\n%s", strings.Join(sample, "\n"))
}

// SmokeJob is the hardened Assist smoke Job manifest.
func SmokeJob(name, namespace, image string) map[string]interface{} {
	return map[string]interface{}{
		"apiVersion": "batch/v1",
		"kind":       "Job",
		"metadata": map[string]interface{}{
			"name":      name,
			"namespace": namespace,
			"labels":    map[string]interface{}{"app.kubernetes.io/name": "hermes-smoke"},
		},
		"spec": map[string]interface{}{
			"backoffLimit":            0,
			"activeDeadlineSeconds":   300,
			"ttlSecondsAfterFinished": 600,
			"template": map[string]interface{}{
				"metadata": map[string]interface{}{
					"labels": map[string]interface{}{"app.kubernetes.io/name": "hermes-smoke"},
				},
				"spec": map[string]interface{}{
					"restartPolicy":                "Never",
					"automountServiceAccountToken": false,
					"enableServiceLinks":           false,
					"securityContext": map[string]interface{}{
						"runAsNonRoot":   true,
						"runAsUser":      65534,
						"runAsGroup":     65534,
						"seccompProfile": map[string]interface{}{"type": "RuntimeDefault"},
					},
					"containers": []interface{}{map[string]interface{}{
						"name":            "smoke",
						"image":           image,
						"imagePullPolicy": "IfNotPresent",
						"command": []interface{}{
							"/usr/local/bin/home-deploy", "hermes", "assist-smoke",
						},
						"env": []interface{}{
							map[string]interface{}{"name": "HA_URL", "value": HAURL},
							map[string]interface{}{"name": "HA_AGENT", "value": HAAgent},
							map[string]interface{}{"name": "SMOKE_TEXT", "value": SmokeText},
						},
						"resources": map[string]interface{}{
							"requests": map[string]interface{}{"cpu": "10m", "memory": "32Mi"},
							"limits":   map[string]interface{}{"memory": "128Mi"},
						},
						"securityContext": map[string]interface{}{
							"allowPrivilegeEscalation": false,
							"readOnlyRootFilesystem":   true,
							"capabilities":             map[string]interface{}{"drop": []interface{}{"ALL"}},
						},
						"volumeMounts": []interface{}{map[string]interface{}{
							"name":      "ha-token",
							"mountPath": "/run/secrets/ha",
							"readOnly":  true,
						}},
					}},
					"volumes": []interface{}{map[string]interface{}{
						"name":   "ha-token",
						"secret": map[string]interface{}{"secretName": HASecret, "defaultMode": 0o444},
					}},
				},
			},
		},
	}
}

func jobCondition(run Runner, options Options, name string) string {
	result := run(Command{Name: options.Kubectl, Args: []string{
		"-n", options.Namespace, "get", "job", name, "-o",
		`jsonpath={range .status.conditions[*]}{.type}={.status}{"\n"}{end}`,
	}})
	if result.Code != 0 {
		return ""
	}
	for _, line := range strings.Split(result.Stdout, "\n") {
		if line == "Complete=True" || line == "Failed=True" {
			kind, _, _ := strings.Cut(line, "=")
			return kind
		}
	}
	return ""
}

func waitForJob(run Runner, options Options, name string) (string, error) {
	deadline := options.Clock().Add(200 * time.Second)
	for options.Clock().Before(deadline) {
		condition := jobCondition(run, options, name)
		if condition == "Complete" || condition == "Failed" {
			return condition, nil
		}
		options.Sleep(2 * time.Second)
	}
	return "", failf("smoke job %s did not finish within 200s", name)
}

// smokeResult parses the JSON line the smoke container prints, if there is one.
func smokeResult(output string) map[string]interface{} {
	lines := strings.Split(strings.TrimSpace(output), "\n")
	if len(lines) == 0 {
		return nil
	}
	var parsed map[string]interface{}
	if err := json.Unmarshal([]byte(lines[len(lines)-1]), &parsed); err != nil {
		return nil
	}
	return parsed
}

// AssistAttempt runs one smoke Job and returns the JSON result it printed.
//
// It returns an error only when the Job produced no parseable result (it did
// not start, or was killed). A structured failure is returned so the caller can
// decide whether it is transient warm-up.
func AssistAttempt(run Runner, options Options) (map[string]interface{}, error) {
	name := "hermes-smoke-" + strconv.FormatInt(options.Clock().UnixNano()/1_000_000, 10)
	manifest, err := yaml.Marshal(SmokeJob(name, options.Namespace, options.SmokeImage))
	if err != nil {
		return nil, err
	}
	if _, err := checked(run, Command{Name: options.Kubectl, Args: []string{
		"-n", options.Namespace, "apply", "-f", "-",
	}, Stdin: string(manifest)}); err != nil {
		return nil, err
	}
	defer func() {
		run(Command{Name: options.Kubectl, Args: []string{
			"-n", options.Namespace, "delete", "job", name, "--ignore-not-found",
		}})
	}()
	condition, err := waitForJob(run, options, name)
	if err != nil {
		return nil, err
	}
	logs := run(Command{Name: options.Kubectl, Args: []string{
		"-n", options.Namespace, "logs", "job/" + name, "--tail=20",
	}})
	output := strings.TrimSpace(logs.Stdout)
	if result := smokeResult(output); result != nil {
		return result, nil
	}
	if condition == "Complete" {
		return nil, failf("Assist smoke produced unexpected output: %q", output)
	}
	detail := run(Command{Name: options.Kubectl, Args: []string{
		"-n", options.Namespace, "describe", "job", name, "--tail=40",
	}}).Stdout
	stderr := strings.TrimSpace(logs.Stderr)
	if stderr == "" {
		stderr = output
	}
	return nil, failf("Assist smoke job failed: %s\n%s", stderr, strings.TrimSpace(detail))
}

// TransientSmokeResult reports whether a failed Assist smoke may succeed once
// Hermes finishes warming up.
func TransientSmokeResult(result map[string]interface{}) bool {
	if ok, _ := result["ok"].(bool); ok {
		return false
	}
	switch status := result["status"].(type) {
	case int:
		if status >= 500 && status < 600 {
			return true
		}
		if status != 200 {
			return false
		}
	case float64:
		if status >= 500 && status < 600 {
			return true
		}
		if status != 200 {
			return false
		}
	}
	errorText := fmt.Sprint(result["error"])
	for _, marker := range transientWords {
		if strings.Contains(errorText, marker) {
			return true
		}
	}
	return false
}

// AssistSmoke asks Hermes one Assist turn, retrying a bounded transient window.
func AssistSmoke(options Options, attempt func() (map[string]interface{}, error)) (string, error) {
	options.defaults()
	deadline := options.Clock().Add(options.RetryWindow)
	for {
		result, err := attempt()
		if err != nil {
			return "", err
		}
		if ok, _ := result["ok"].(bool); ok {
			if speech := strings.TrimSpace(fmt.Sprint(result["speech"])); speech != "" {
				return speech, nil
			}
		}
		if !TransientSmokeResult(result) {
			return "", failf("Assist smoke failed: %v", result)
		}
		if !options.Clock().Before(deadline) {
			return "", failf("Assist smoke still failing after %s: %v", options.RetryWindow, result)
		}
		reason := result["status"]
		if reason == nil {
			reason = result["error"]
		}
		if reason == nil {
			reason = "transient failure"
		}
		fmt.Fprintf(options.Stdout, "Assist smoke is not ready yet (%v); retrying in %s\n", reason, options.RetryDelay)
		options.Sleep(options.RetryDelay)
	}
}

// VoiceProxyHealthcheck runs the distroless proxy's own probe inside the live
// container. The image has no shell, so `-healthcheck` replaces curl/sh.
func VoiceProxyHealthcheck(run Runner, options Options) error {
	_, err := checked(run, Command{Name: options.Kubectl, Args: []string{
		"-n", options.Namespace, "exec", "deployment/" + options.Deployment,
		"-c", "voice-proxy", "--", "/usr/local/bin/voice-proxy", "-healthcheck",
	}})
	return err
}

// RunSmoke waits for the rollout and runs every post-upgrade check.
func RunSmoke(run Runner, options Options, simulateFailure, probeProxy bool) error {
	options.defaults()
	if _, err := checked(run, Command{Name: options.Kubectl, Args: []string{
		"-n", options.Namespace, "rollout", "status", "deployment/" + options.Deployment, "--timeout=240s",
	}}); err != nil {
		return err
	}
	if simulateFailure {
		return failf("simulated smoke failure requested")
	}
	metrics, err := HealthMetrics(run, options)
	if err != nil {
		return err
	}
	if err := CheckHealth(metrics); err != nil {
		return err
	}
	attempt := options.AssistAttempt
	if attempt == nil {
		attempt = func() (map[string]interface{}, error) { return AssistAttempt(run, options) }
	}
	speech, err := AssistSmoke(options, attempt)
	if err != nil {
		return err
	}
	fmt.Fprintf(options.Stdout, "Assist reply: %s\n", speech)
	if probeProxy {
		if err := VoiceProxyHealthcheck(run, options); err != nil {
			return err
		}
		fmt.Fprintln(options.Stdout, "voice-proxy -healthcheck: ok")
	}
	logs, err := checked(run, Command{Name: options.Kubectl, Args: []string{
		"-n", options.Namespace, "logs", "deployment/" + options.Deployment, "-c", "voice-proxy", "--since=10m",
	}})
	if err != nil {
		return err
	}
	if err := CheckVoiceLogs(logs); err != nil {
		return err
	}
	fmt.Fprintln(options.Stdout, "voice-proxy logs: no errors")
	return nil
}

func writeState(stateDir, name, value string) error {
	if err := os.MkdirAll(stateDir, 0o755); err != nil {
		return err
	}
	return os.WriteFile(stateDir+"/"+name, []byte(value), 0o644)
}

// RollbackOnly is the workflow safety net: restore the recorded revision only
// when this job actually applied an upgrade.
func RollbackOnly(run Runner, options Options) (int, error) {
	applied := options.StateDir + "/upgrade-applied"
	previous := options.StateDir + "/previous-revision"
	_, appliedErr := os.Stat(applied)
	_, previousErr := os.Stat(previous)
	if appliedErr != nil || previousErr != nil {
		fmt.Fprintln(options.Stdout, "hermes rollout: no upgrade was applied, nothing to roll back")
		return 0, nil
	}
	raw, err := os.ReadFile(previous)
	if err != nil {
		return 0, err
	}
	revision, err := strconv.Atoi(strings.TrimSpace(string(raw)))
	if err != nil {
		return 0, fmt.Errorf("previous-revision: %w", err)
	}
	fmt.Fprintf(options.Stdout, "hermes rollout: rolling back to revision %d\n", revision)
	if err := Rollback(run, options, revision); err != nil {
		return 1, err
	}
	if _, err := checked(run, Command{Name: options.Kubectl, Args: []string{
		"-n", options.Namespace, "rollout", "status", "deployment/" + options.Deployment, "--timeout=240s",
	}}); err != nil {
		return 1, err
	}
	return 0, nil
}

// Run performs the rollout and returns the process exit status: 0 healthy,
// 1 failure (rolled back when a revision was created), 2 rollback verification
// failed.
func Run(run Runner, options Options) (int, error) {
	options.defaults()
	if options.RollbackOnly {
		return RollbackOnly(run, options)
	}
	if options.ValuesFile == "" || options.HermesDigest == "" || options.VoiceProxyDigest == "" {
		return 0, errors.New("--values-file, --hermes-digest and --voice-proxy-digest are required")
	}
	if options.SmokeImage == "" {
		return 0, errors.New("--smoke-image is required (the home-deploy image digest)")
	}
	if options.ExporterDigest == "" {
		// The chart no longer ships the Python exporter script/ConfigMap, so an
		// upgrade without the pinned Go image would leave the pod broken.
		return 0, errors.New("--exporter-digest is required (the health-exporter image digest)")
	}
	if options.Chart == "" {
		return 0, errors.New("--chart is required")
	}
	if options.StateDir == "" {
		return 0, errors.New("--state-dir is required")
	}
	hermes, err := NormalizeDigest("hermes", options.HermesDigest)
	if err != nil {
		return 0, err
	}
	voiceProxy, err := NormalizeDigest("voice-proxy", options.VoiceProxyDigest)
	if err != nil {
		return 0, err
	}
	var exporter string
	if options.ExporterDigest != "" {
		exporter, err = NormalizeDigest("health-exporter", options.ExporterDigest)
		if err != nil {
			return 0, err
		}
		options.ExporterDigest = exporter
	}
	values, err := LoadValues(options.ValuesFile)
	if err != nil {
		return 0, err
	}

	current, err := LiveImages(run, options.Kubectl, options.Namespace, options.Deployment)
	if err != nil {
		return 0, err
	}
	needRollout := RolloutNeeded(current, hermes, voiceProxy)
	if exporter != "" {
		liveExporter, exporterErr := LiveImages(run, options.Kubectl, options.Namespace, options.ExporterDeployment)
		if exporterErr != nil || !exporterMatches(liveExporter, exporter) {
			needRollout = true
		}
	}
	if !options.Force && !needRollout {
		fmt.Fprintln(options.Stdout, "hermes rollout: live images already match the built digests; nothing to do")
		return 0, nil
	}

	previous := LatestRevision(run, options.Helm, options.Namespace, options.Release)
	if err := writeState(options.StateDir, "previous-revision", strconv.Itoa(previous)); err != nil {
		return 0, err
	}
	fmt.Fprintf(options.Stdout, "hermes rollout: revision %d -> new digests\n", previous)

	valuesPath, err := WriteValues(WorkloadValues(values))
	if err != nil {
		return 0, err
	}
	defer os.Remove(valuesPath)

	applied := false
	result := Upgrade(run, options, options.Chart, valuesPath, hermes, voiceProxy)
	applied = LatestRevision(run, options.Helm, options.Namespace, options.Release) > previous
	if applied {
		if err := writeState(options.StateDir, "upgrade-applied", "1"); err != nil {
			return 0, err
		}
	}
	if result.Code != 0 {
		return failRollout(run, options, previous, applied, failf("helm upgrade failed: %s",
			strings.TrimSpace(firstNonEmpty(result.Stderr, result.Stdout))))
	}
	if err := RunSmoke(run, options, options.SimulateFailure, true); err != nil {
		return failRollout(run, options, previous, applied, err)
	}

	// A successful rollout leaves no pending rollback for the workflow safety net.
	os.Remove(options.StateDir + "/upgrade-applied")
	fmt.Fprintf(options.Stdout, "hermes rollout: revision %d is healthy\n",
		LatestRevision(run, options.Helm, options.Namespace, options.Release))
	return 0, nil
}

// failRollout rolls back when a revision was created and verifies the old
// circuit. It returns the Python rollout's exit status (1, or 2 when the
// rollback verification itself failed).
func failRollout(run Runner, options Options, previous int, applied bool, cause error) (int, error) {
	fmt.Fprintf(options.Stderr, "hermes rollout failed: %v\n", cause)
	if !applied {
		return 1, nil
	}
	fmt.Fprintf(options.Stderr, "hermes rollout: rolling back to revision %d\n", previous)
	if err := Rollback(run, options, previous); err != nil {
		fmt.Fprintf(options.Stderr, "hermes rollout: rollback verification failed: %v\n", err)
		return 2, nil
	}
	if err := RunSmoke(run, options, false, false); err != nil {
		fmt.Fprintf(options.Stderr, "hermes rollout: rollback verification failed: %v\n", err)
		return 2, nil
	}
	fmt.Fprintf(options.Stderr, "hermes rollout: rollback to revision %d verified\n", previous)
	return 1, nil
}

// exporterMatches reports whether every container of the live exporter
// deployment already runs the requested digest. The deployment has exactly one
// container, and its name comes from the private values.
func exporterMatches(images map[string]string, digest string) bool {
	if len(images) == 0 {
		return false
	}
	for _, image := range images {
		if image != digest {
			return false
		}
	}
	return true
}

func checked(run Runner, command Command) (string, error) {
	result := run(command)
	if result.Code != 0 {
		detail := strings.TrimSpace(firstNonEmpty(result.Stderr, result.Stdout))
		return "", failf("%s %s failed: %s", command.Name, strings.Join(command.Args, " "), detail)
	}
	return result.Stdout, nil
}

func firstNonEmpty(values ...string) string {
	for _, value := range values {
		if strings.TrimSpace(value) != "" {
			return value
		}
	}
	return ""
}

// mappingValue returns a mapping child.
func mappingValue(parent yaml.MapSlice, key string) (yaml.MapSlice, bool) {
	for _, item := range parent {
		if fmt.Sprint(item.Key) != key {
			continue
		}
		value, ok := item.Value.(yaml.MapSlice)
		return value, ok
	}
	return nil, false
}

// sequenceValue returns a sequence child.
func sequenceValue(parent yaml.MapSlice, key string) ([]interface{}, bool) {
	for _, item := range parent {
		if fmt.Sprint(item.Key) != key {
			continue
		}
		value, ok := item.Value.([]interface{})
		return value, ok
	}
	return nil, false
}
