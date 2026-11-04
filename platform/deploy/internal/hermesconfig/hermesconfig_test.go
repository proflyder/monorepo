package hermesconfig

import (
	"bytes"
	"os"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
	"testing"

	yaml "go.yaml.in/yaml/v2"
)

// repoRoot walks up from this test file to the module root.
func repoRoot(t *testing.T) string {
	t.Helper()
	_, file, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatal("cannot resolve the test file path")
	}
	directory := filepath.Dir(file)
	for {
		if _, err := os.Stat(filepath.Join(directory, "go.mod")); err == nil {
			return directory
		}
		parent := filepath.Dir(directory)
		if parent == directory {
			t.Fatal("cannot find the module root above the test file")
		}
		directory = parent
	}
}

// locate returns a repository-relative path, preferring the Bazel runfiles tree.
func locate(t *testing.T, relative string) string {
	t.Helper()
	if sourceRoot := os.Getenv("TEST_SRCDIR"); sourceRoot != "" {
		for _, workspace := range []string{os.Getenv("TEST_WORKSPACE"), "_main", "yandex_station_lab"} {
			if workspace == "" {
				continue
			}
			candidate := filepath.Join(sourceRoot, workspace, relative)
			if _, err := os.Stat(candidate); err == nil {
				return candidate
			}
		}
		t.Fatalf("runfiles do not contain %s; add it to the go_test data", relative)
	}
	return filepath.Join(repoRoot(t), relative)
}

const hermesDir = "projects/home-assistant/hermes"

func sourceFiles(t *testing.T) Options {
	t.Helper()
	directory := locate(t, hermesDir)
	return Options{
		Fragment:       filepath.Join(directory, "config.fragment.yaml"),
		Context:        filepath.Join(directory, "HERMES.md"),
		Soul:           filepath.Join(directory, "SOUL.md"),
		VoicePrompt:    filepath.Join(directory, "voice_prompt.txt"),
		TelegramPrompt: filepath.Join(directory, "telegram_prompt.txt"),
		Skills:         filepath.Join(directory, "skills"),
	}
}

const existingConfig = `model:
  provider: "openai-codex"
  default: "gpt-5.6-luna"
gateway:
  write_sessions_json: false
sessions_dir: /opt/data/sessions
platforms:
  telegram:
    token: "123456:placeholder-secret-value"
agent:
  personalities:
    voice: "старый текст"
platform_toolsets:
  api_server: [web]
`

const liveConfig = `model:
  provider: "openai-codex"
  default: "gpt-5.6-luna"
mcp_servers:
  homeassistant:
    url: "http://10.1.2.3:8123/api/mcp"
    trust: full
  music-assistant:
    url: "http://10.1.2.3:8095/mcp/v1"
    trust: full
  manual-extra:
    url: "http://10.1.2.3:9999/mcp"
`

const privateValues = `mcp_servers:
  homeassistant:
    url: "http://10.1.2.3:8123/api/mcp"
  music-assistant:
    url: "http://10.1.2.3:8095/mcp/v1"
`

// --- helpers -------------------------------------------------------------

func planFor(t *testing.T, home, private string, allow bool) map[string]string {
	t.Helper()
	options := sourceFiles(t)
	options.Home = home
	options.PrivateValues = private
	options.AllowPlaceholders = allow
	plan, err := BuildPlan(options)
	if err != nil {
		t.Fatalf("build plan: %v", err)
	}
	result := make(map[string]string, len(plan))
	for _, file := range plan {
		result[file.Path] = file.Content
	}
	return result
}

func parseMapping(t *testing.T, text string) yaml.MapSlice {
	t.Helper()
	var mapping yaml.MapSlice
	if err := yaml.Unmarshal([]byte(text), &mapping); err != nil {
		t.Fatalf("parse rendered config: %v", err)
	}
	return mapping
}

func child(t *testing.T, mapping yaml.MapSlice, keys ...string) interface{} {
	t.Helper()
	var current interface{} = mapping
	for _, key := range keys {
		parent, ok := current.(yaml.MapSlice)
		if !ok {
			t.Fatalf("%v: not a mapping", keys)
		}
		value, ok := valueOf(parent, key)
		if !ok {
			t.Fatalf("%v: key %q is absent", keys, key)
		}
		current = value
	}
	return current
}

func stringChild(t *testing.T, mapping yaml.MapSlice, keys ...string) string {
	t.Helper()
	value, _ := child(t, mapping, keys...).(string)
	return value
}

func writeFile(t *testing.T, path, content string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
		t.Fatal(err)
	}
}

// --- DeepMerge -----------------------------------------------------------

func TestNestedMappingMergesAndScalarWins(t *testing.T) {
	merged := DeepMerge(
		parseMapping(t, "a:\n  x: 1\n  y: 2\nkeep: true\n"),
		parseMapping(t, "a:\n  y: 3\n  z: 4\n"),
	)
	want, _ := DumpConfig(parseMapping(t, "a:\n  x: 1\n  y: 3\n  z: 4\nkeep: true\n"))
	got, _ := DumpConfig(merged)
	if got != want {
		t.Fatalf("merged:\n%s\nwant:\n%s", got, want)
	}
}

func TestListsAreReplacedNotConcatenated(t *testing.T) {
	merged := DeepMerge(
		parseMapping(t, "list: [1, 2, 3]\n"),
		parseMapping(t, "list: [web]\n"),
	)
	output, _ := DumpConfig(merged)
	if !strings.Contains(output, "- web") || strings.Contains(output, "- 1") {
		t.Fatalf("lists must be replaced:\n%s", output)
	}
}

func TestInputsAreNotMutated(t *testing.T) {
	base := parseMapping(t, "a:\n  b: 1\n")
	overlay := parseMapping(t, "a:\n  c: 2\n")
	DeepMerge(base, overlay)
	if _, present := valueOf(childMap(t, base, "a"), "c"); present {
		t.Fatal("overlay leaked into base")
	}
	if _, present := valueOf(childMap(t, overlay, "a"), "b"); present {
		t.Fatal("base leaked into overlay")
	}
}

func childMap(t *testing.T, mapping yaml.MapSlice, key string) yaml.MapSlice {
	t.Helper()
	value, _ := valueOf(mapping, key)
	return value.(yaml.MapSlice)
}

// --- redaction -----------------------------------------------------------

func TestTokenShapedValuesAreMasked(t *testing.T) {
	text := "platforms:\n  telegram:\n    token: \"123456:placeholder\"\n"
	redacted := Redact(text)
	if !strings.Contains(redacted, "token: <redacted>") || strings.Contains(redacted, "placeholder") {
		t.Fatalf("not redacted: %q", redacted)
	}
}

func TestOrdinaryLinesSurvive(t *testing.T) {
	if got := Redact("mcp-infra: enabled\n"); got != "mcp-infra: enabled\n" {
		t.Fatalf("unexpected redaction: %q", got)
	}
}

// --- placeholder guard ---------------------------------------------------

func TestRefusesToPutTestNetAddressesOverLiveEndpoints(t *testing.T) {
	home := t.TempDir()
	writeFile(t, filepath.Join(home, "config.yaml"), liveConfig)
	options := sourceFiles(t)
	options.Home = home
	_, err := BuildPlan(options)
	if err == nil {
		t.Fatal("expected a placeholder refusal")
	}
	for _, key := range []string{"mcp_servers.homeassistant.url", "mcp_servers.music-assistant.url", "--private-values"} {
		if !strings.Contains(err.Error(), key) {
			t.Fatalf("message does not name %q: %v", key, err)
		}
	}
}

func TestRefusesPlaceholdersInAFreshHome(t *testing.T) {
	home := t.TempDir()
	options := sourceFiles(t)
	options.Home = home
	if _, err := BuildPlan(options); err == nil || !strings.Contains(err.Error(), "mcp_servers.homeassistant.url") {
		t.Fatalf("expected refusal, got %v", err)
	}
}

func TestPrivateValuesMakeTheRenderSafe(t *testing.T) {
	home := t.TempDir()
	writeFile(t, filepath.Join(home, "config.yaml"), liveConfig)
	private := filepath.Join(home, "private.yaml")
	writeFile(t, private, privateValues)
	plan := planFor(t, home, private, false)
	merged := parseMapping(t, plan[filepath.Join(home, "config.yaml")])
	if got := stringChild(t, merged, "mcp_servers", "homeassistant", "url"); got != "http://10.1.2.3:8123/api/mcp" {
		t.Fatalf("homeassistant url = %q", got)
	}
	if got := stringChild(t, merged, "mcp_servers", "music-assistant", "url"); got != "http://10.1.2.3:8095/mcp/v1" {
		t.Fatalf("music-assistant url = %q", got)
	}
	if strings.Contains(plan[filepath.Join(home, "config.yaml")], "192.0.2.") {
		t.Fatal("rendered config still holds a placeholder")
	}
}

func TestPrivateValuesAreMergedAfterTheFragment(t *testing.T) {
	home := t.TempDir()
	writeFile(t, filepath.Join(home, "config.yaml"), liveConfig)
	private := filepath.Join(home, "private.yaml")
	writeFile(t, private, privateValues+"model:\n  default: \"private-model\"\n")
	merged := parseMapping(t, planFor(t, home, private, true)[filepath.Join(home, "config.yaml")])
	if got := stringChild(t, merged, "model", "default"); got != "private-model" {
		t.Fatalf("model.default = %q", got)
	}
	if got := stringChild(t, merged, "model", "provider"); got != "openai-codex" {
		t.Fatalf("model.provider = %q", got)
	}
}

func TestMissingPrivateValuesFileIsAnError(t *testing.T) {
	home := t.TempDir()
	options := sourceFiles(t)
	options.Home = home
	options.PrivateValues = filepath.Join(home, "nope.yaml")
	if _, err := BuildPlan(options); err == nil || !strings.Contains(err.Error(), "private values file not found") {
		t.Fatalf("expected a missing-file error, got %v", err)
	}
}

func TestEscapeHatchAllowsPlaceholders(t *testing.T) {
	home := t.TempDir()
	plan := planFor(t, home, "", true)
	if !strings.Contains(plan[filepath.Join(home, "config.yaml")], "192.0.2.") {
		t.Fatal("escape hatch should keep the fragment placeholders")
	}
}

func TestPlaceholderAlreadyLiveIsNotAViolation(t *testing.T) {
	same := parseMapping(t, "mcp_servers:\n  homeassistant:\n    url: \"http://192.0.2.28:8123/api/mcp\"\n")
	if violations := PlaceholderViolations(same, same); len(violations) != 0 {
		t.Fatalf("unexpected violations: %v", violations)
	}
}

// --- plan ----------------------------------------------------------------

func TestMergeKeepsUnrelatedKeysAndInstallsContext(t *testing.T) {
	home := t.TempDir()
	config := filepath.Join(home, "config.yaml")
	writeFile(t, config, existingConfig)
	plan := planFor(t, home, "", true)
	merged := parseMapping(t, plan[config])
	if got := stringChild(t, merged, "sessions_dir"); got != "/opt/data/sessions" {
		t.Fatalf("sessions_dir = %q", got)
	}
	if got := stringChild(t, merged, "platforms", "telegram", "token"); got != "123456:placeholder-secret-value" {
		t.Fatalf("token = %q", got)
	}
	toolsets, _ := child(t, merged, "platform_toolsets", "api_server").([]interface{})
	if len(toolsets) != 4 || toolsets[3] != "mcp-infra" {
		t.Fatalf("api_server toolset = %v", toolsets)
	}
	if _, present := valueOf(childMap(t, merged, "mcp_servers"), "x"); present {
		t.Fatal("unexpected key")
	}
	context, err := os.ReadFile(locate(t, hermesDir+"/HERMES.md"))
	if err != nil {
		t.Fatal(err)
	}
	if plan[filepath.Join(home, ".hermes.md")] != string(context) {
		t.Fatal(".hermes.md was not installed")
	}
	soul, _ := os.ReadFile(locate(t, hermesDir+"/SOUL.md"))
	if plan[filepath.Join(home, "SOUL.md")] != string(soul) {
		t.Fatal("SOUL.md was not installed")
	}
}

func TestVoicePersonalityComesFromVoicePrompt(t *testing.T) {
	home := t.TempDir()
	writeFile(t, filepath.Join(home, "config.yaml"), existingConfig)
	plan := planFor(t, home, "", true)
	merged := parseMapping(t, plan[filepath.Join(home, "config.yaml")])
	prompt, _ := os.ReadFile(locate(t, hermesDir+"/voice_prompt.txt"))
	if got := stringChild(t, merged, "agent", "personalities", "voice"); got != strings.TrimSpace(string(prompt)) {
		t.Fatalf("voice personality is not the prompt")
	}
	if got := stringChild(t, merged, "display", "personality"); got != "voice" {
		t.Fatalf("display.personality = %q", got)
	}
	if strings.Contains(plan[filepath.Join(home, "config.yaml")], "старый текст") {
		t.Fatal("stale personality survived")
	}
}

func TestExistingMCPServersAreNotDeleted(t *testing.T) {
	home := t.TempDir()
	writeFile(t, filepath.Join(home, "config.yaml"), liveConfig)
	private := filepath.Join(home, "private.yaml")
	writeFile(t, private, privateValues)
	merged := parseMapping(t, planFor(t, home, private, true)[filepath.Join(home, "config.yaml")])
	if got := stringChild(t, merged, "mcp_servers", "manual-extra", "url"); got != "http://10.1.2.3:9999/mcp" {
		t.Fatalf("manual server url = %q", got)
	}
}

func TestSecondRenderIsANoOp(t *testing.T) {
	home := t.TempDir()
	options := sourceFiles(t)
	options.Home = home
	options.AllowPlaceholders = true
	first, err := BuildPlan(options)
	if err != nil {
		t.Fatal(err)
	}
	for _, file := range first {
		writeFile(t, file.Path, file.Content)
	}
	second, err := BuildPlan(options)
	if err != nil {
		t.Fatal(err)
	}
	if len(second) != 0 {
		t.Fatalf("second render is not a no-op: %v", paths(second))
	}
}

func paths(files []File) []string {
	result := make([]string, 0, len(files))
	for _, file := range files {
		result = append(result, file.Path)
	}
	return result
}

func TestMissingConfigIsCreatedFromTheFragment(t *testing.T) {
	home := t.TempDir()
	plan := planFor(t, home, "", true)
	if _, present := valueOf(parseMapping(t, plan[filepath.Join(home, "config.yaml")]), "mcp_servers"); !present {
		t.Fatal("config was not created")
	}
}

func TestNonMappingConfigIsRejected(t *testing.T) {
	home := t.TempDir()
	writeFile(t, filepath.Join(home, "config.yaml"), "- not\n- a mapping\n")
	options := sourceFiles(t)
	options.Home = home
	if _, err := BuildPlan(options); err == nil {
		t.Fatal("a sequence config must be rejected")
	}
}

func TestSkillDirectoriesInstallWithoutDeletingOthers(t *testing.T) {
	home := t.TempDir()
	existing := filepath.Join(home, "skills", "manual", "SKILL.md")
	writeFile(t, existing, "manual skill\n")
	plan := planFor(t, home, "", true)
	skill := filepath.Join(home, "skills", "report", "SKILL.md")
	source, _ := os.ReadFile(locate(t, hermesDir+"/skills/report/SKILL.md"))
	if plan[skill] != string(source) {
		t.Fatal("report skill was not installed")
	}
	if _, present := plan[existing]; present {
		t.Fatal("an unrelated skill must stay untouched")
	}
	if raw, _ := os.ReadFile(existing); string(raw) != "manual skill\n" {
		t.Fatal("unrelated skill content changed")
	}
}

func TestTelegramPromptIsInstalledAsAChannelOverride(t *testing.T) {
	home := t.TempDir()
	writeFile(t, filepath.Join(home, "config.yaml"), existingConfig)
	private := filepath.Join(home, "private.yaml")
	writeFile(t, private, privateValues+"telegram_owner_chat_id: \"100000000\"\n")
	merged := parseMapping(t, planFor(t, home, private, true)[filepath.Join(home, "config.yaml")])
	prompt, _ := os.ReadFile(locate(t, hermesDir+"/telegram_prompt.txt"))
	if got := stringChild(t, merged, "platforms", "telegram", "channel_overrides", "100000000", "system_prompt"); got != strings.TrimSpace(string(prompt)) {
		t.Fatalf("telegram override = %q", got)
	}
	homeChannel := child(t, merged, "platforms", "telegram", "home_channel").(yaml.MapSlice)
	if stringChild(t, homeChannel, "platform") != "telegram" || stringChild(t, homeChannel, "chat_id") != "100000000" || stringChild(t, homeChannel, "name") != "Home" {
		t.Fatalf("unexpected home_channel: %v", homeChannel)
	}
	if _, present := valueOf(merged, "telegram_owner_chat_id"); present {
		t.Fatal("renderer-only key leaked into config")
	}
	if got := stringChild(t, merged, "display", "personality"); got != "voice" {
		t.Fatalf("display.personality = %q", got)
	}
}

func TestNoOwnerChatIDLeavesExistingOverrideUntouched(t *testing.T) {
	home := t.TempDir()
	writeFile(t, filepath.Join(home, "config.yaml"),
		"platforms:\n  telegram:\n    channel_overrides:\n      \"42\":\n        system_prompt: \"old\"\n")
	merged := parseMapping(t, planFor(t, home, "", true)[filepath.Join(home, "config.yaml")])
	if got := stringChild(t, merged, "platforms", "telegram", "channel_overrides", "42", "system_prompt"); got != "old" {
		t.Fatalf("existing override = %q", got)
	}
}

// --- CLI -----------------------------------------------------------------

func runCLI(t *testing.T, home string, args ...string) (int, string) {
	t.Helper()
	options := sourceFiles(t)
	options.Home = home
	dryRun, check := false, false
	for index := 0; index < len(args); index++ {
		switch args[index] {
		case "--dry-run":
			dryRun = true
		case "--check":
			check = true
		case "--allow-placeholders":
			options.AllowPlaceholders = true
		case "--private-values":
			index++
			options.PrivateValues = args[index]
		}
	}
	var stdout bytes.Buffer
	code, err := Run(options, dryRun, check, &stdout)
	if err != nil {
		t.Fatalf("run: %v", err)
	}
	return code, stdout.String()
}

func TestDryRunWritesNothingAndRedactsSecrets(t *testing.T) {
	home := t.TempDir()
	config := filepath.Join(home, "config.yaml")
	writeFile(t, config, existingConfig)
	before, _ := os.ReadFile(config)
	code, output := runCLI(t, home, "--dry-run", "--allow-placeholders")
	if code != 0 {
		t.Fatalf("exit code %d", code)
	}
	if after, _ := os.ReadFile(config); string(after) != string(before) {
		t.Fatal("dry-run changed config.yaml")
	}
	if _, err := os.Stat(filepath.Join(home, ".hermes.md")); err == nil {
		t.Fatal("dry-run installed .hermes.md")
	}
	if !strings.Contains(output, "+") || strings.Contains(output, "placeholder-secret-value") || !strings.Contains(output, "<redacted>") {
		t.Fatalf("unexpected dry-run output: %q", output)
	}
}

func TestCheckReportsPendingRenderWithoutWriting(t *testing.T) {
	home := t.TempDir()
	writeFile(t, filepath.Join(home, "config.yaml"), existingConfig)
	code, _ := runCLI(t, home, "--check", "--allow-placeholders")
	if code != 1 {
		t.Fatalf("check exit code = %d", code)
	}
	if _, err := os.Stat(filepath.Join(home, "SOUL.md")); err == nil {
		t.Fatal("check wrote SOUL.md")
	}
}

func TestRenderKeepsOneStepBackupAndThenReportsClean(t *testing.T) {
	home := t.TempDir()
	config := filepath.Join(home, "config.yaml")
	writeFile(t, config, existingConfig)
	code, output := runCLI(t, home, "--allow-placeholders")
	if code != 0 || !strings.Contains(output, "rendered") {
		t.Fatalf("render failed: %d %q", code, output)
	}
	previous, _ := os.ReadFile(config + ".prev")
	if string(previous) != existingConfig {
		t.Fatal(".prev does not hold the previous config")
	}
	if info, err := os.Stat(config + ".prev"); err != nil || info.Mode().Perm() != 0o600 {
		t.Fatalf(".prev mode = %v", info.Mode())
	}
	if _, err := os.Stat(filepath.Join(home, ".hermes.md")); err != nil {
		t.Fatal(".hermes.md was not installed")
	}
	code, output = runCLI(t, home, "--check", "--allow-placeholders")
	if code != 0 || !strings.Contains(output, "up to date") {
		t.Fatalf("second check: %d %q", code, output)
	}
}

func TestCLIAcceptsPrivateValues(t *testing.T) {
	home := t.TempDir()
	writeFile(t, filepath.Join(home, "config.yaml"), liveConfig)
	private := filepath.Join(home, "private.yaml")
	writeFile(t, private, privateValues)
	code, output := runCLI(t, home, "--private-values", private, "--check")
	if code != 1 || !strings.Contains(output, "would update") {
		t.Fatalf("check: %d %q", code, output)
	}
	code, output = runCLI(t, home, "--dry-run", "--private-values", private)
	if code != 0 || strings.Contains(output, "192.0.2.") {
		t.Fatalf("dry-run: %d %q", code, output)
	}
}

func TestCLIRefusesPlaceholdersWithoutEscapeHatch(t *testing.T) {
	home := t.TempDir()
	writeFile(t, filepath.Join(home, "config.yaml"), liveConfig)
	options := sourceFiles(t)
	options.Home = home
	var stdout bytes.Buffer
	_, err := Run(options, true, false, &stdout)
	if err == nil || !strings.Contains(err.Error(), "placeholder") {
		t.Fatalf("expected a placeholder refusal, got %v", err)
	}
}

// --- golden parity with the Python renderer ------------------------------

// TestGoldenMatchesPython proves the Go renderer produces the same document
// as render_hermes_config.py for the committed TEST-NET fixtures. Both texts
// are parsed and re-serialised with the same emitter, so emitter style
// differences (long-scalar folding, single vs double quotes for numeric-looking
// strings) cannot hide a value or ordering change.
func TestGoldenMatchesPython(t *testing.T) {
	cases := []struct {
		name    string
		private string
		golden  string
	}{
		{"fresh home", "", "config.yaml"},
		{"private values", "platform/deploy/testdata/hermes/private.yaml", "config-private.yaml"},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			home := t.TempDir()
			options := sourceFiles(t)
			options.Home = home
			options.AllowPlaceholders = true
			if tc.private != "" {
				options.PrivateValues = locate(t, tc.private)
			}
			plan, err := BuildPlan(options)
			if err != nil {
				t.Fatalf("render: %v", err)
			}
			var got string
			for _, file := range plan {
				if file.Path == filepath.Join(home, "config.yaml") {
					got = file.Content
				}
			}
			if got == "" {
				t.Fatal("no config.yaml in the plan")
			}
			goldenBytes, err := os.ReadFile(locate(t, "platform/deploy/testdata/golden/hermes-config/"+tc.golden))
			if err != nil {
				t.Fatal(err)
			}
			normalise := func(text string) string {
				output, err := DumpConfig(parseMapping(t, text))
				if err != nil {
					t.Fatal(err)
				}
				return output
			}
			if normalise(got) != normalise(string(goldenBytes)) {
				t.Errorf("rendered config differs from the Python golden\n--- go ---\n%s\n--- python ---\n%s", got, goldenBytes)
			}
			// The remaining byte differences are emitter style only: go-yaml
			// double-quotes numeric-looking strings where PyYAML uses single
			// quotes, and folds a multi-line quoted scalar differently. The
			// parsed documents (above) are identical.
			if got != string(goldenBytes) {
				t.Logf("formatting differs from the Python golden (values match):\n%s",
					firstLineDifference(got, string(goldenBytes)))
			}
		})
	}
}

// --- context files -------------------------------------------------------

func TestContextStaysSmall(t *testing.T) {
	raw, err := os.ReadFile(locate(t, hermesDir+"/HERMES.md"))
	if err != nil {
		t.Fatal(err)
	}
	if len(raw) >= 2048 {
		t.Fatalf("HERMES.md is %d bytes; every byte lands in the cached system prompt", len(raw))
	}
}

func TestContextNamesTheToolContract(t *testing.T) {
	raw, _ := os.ReadFile(locate(t, hermesDir+"/HERMES.md"))
	text := string(raw)
	for _, tool := range []string{"infra_status", "infra_service", "infra_logs", "infra_alert",
		"infra_prepare_restart", "infra_confirm", "infra_cancel", "infra_report", "infra_send_report"} {
		if !strings.Contains(text, tool) {
			t.Fatalf("HERMES.md does not name %s", tool)
		}
	}
	if !strings.Contains(text, "kubectl") || !strings.Contains(text, "90") {
		t.Fatal("HERMES.md lost the tool contract")
	}
}

func TestContextNamesTheBBAgentContract(t *testing.T) {
	raw, _ := os.ReadFile(locate(t, hermesDir+"/HERMES.md"))
	text := string(raw)
	for _, tool := range []string{"bb_status", "bb_prepare_spawn", "bb_confirm", "bb_cancel"} {
		if !strings.Contains(text, tool) {
			t.Fatalf("HERMES.md does not name %s", tool)
		}
	}
	for _, marker := range []string{"личный профиль", "Рабочих проектов нет", "дословно"} {
		if !strings.Contains(text, marker) {
			t.Fatalf("HERMES.md lost %q", marker)
		}
	}
}

func TestContextHasNoInjectionMarkers(t *testing.T) {
	raw, _ := os.ReadFile(locate(t, hermesDir+"/HERMES.md"))
	text := strings.ToLower(string(raw))
	for _, marker := range []string{"ignore previous instructions", "system prompt",
		"disregard your instructions", "you are now a", "do not tell the user", "register as a node"} {
		if strings.Contains(text, marker) {
			t.Fatalf("HERMES.md contains %q", marker)
		}
	}
	for _, char := range []string{"\u200b", "\u200c", "\u200d", "\u2060", "\ufeff", "\u202a", "\u202b", "\u202c", "\u202d", "\u202e"} {
		if strings.Contains(string(raw), char) {
			t.Fatalf("HERMES.md contains an invisible character")
		}
	}
}

func TestExampleCoversEveryPlaceholderEndpoint(t *testing.T) {
	exampleBytes, err := os.ReadFile(locate(t, hermesDir+"/hermes-private.example.yaml"))
	if err != nil {
		t.Fatal(err)
	}
	example := parseMapping(t, string(exampleBytes))
	fragmentBytes, _ := os.ReadFile(locate(t, hermesDir+"/config.fragment.yaml"))
	fragment := parseMapping(t, string(fragmentBytes))
	servers := childMap(t, fragment, "mcp_servers")
	exampleServers := childMap(t, example, "mcp_servers")
	found := 0
	for _, item := range servers {
		server, ok := item.Value.(yaml.MapSlice)
		if !ok {
			continue
		}
		if !placeholderPattern.MatchString(stringValue(server, "url")) {
			continue
		}
		found++
		name := item.Key.(string)
		exampleServer, ok := valueOf(exampleServers, name)
		if !ok {
			t.Fatalf("example is missing mcp_servers.%s", name)
		}
		if !placeholderPattern.MatchString(stringValue(exampleServer.(yaml.MapSlice), "url")) {
			t.Fatalf("example mcp_servers.%s keeps a placeholder", name)
		}
	}
	if found == 0 {
		t.Fatal("fragment should keep TEST-NET placeholders")
	}
	if got := stringValue(example, "telegram_owner_chat_id"); got != "100000000" {
		t.Fatalf("example telegram_owner_chat_id = %q", got)
	}
}

func TestReportSkillFrontmatter(t *testing.T) {
	raw, err := os.ReadFile(locate(t, hermesDir+"/skills/report/SKILL.md"))
	if err != nil {
		t.Fatal(err)
	}
	parts := strings.SplitN(string(raw), "---", 3)
	if len(parts) != 3 {
		t.Fatal("SKILL.md has no frontmatter")
	}
	meta := parseMapping(t, parts[1])
	if got := stringValue(meta, "name"); got != "report" {
		t.Fatalf("skill name = %q", got)
	}
	if len([]rune(stringValue(meta, "description"))) > 40 {
		t.Fatal("skill description is too long")
	}
	body := strings.ToLower(parts[2])
	if !strings.Contains(body, "infra_report") || !strings.Contains(body, "details.text") {
		t.Fatal("skill lost the report contract")
	}
	for _, forbidden := range []string{"terminal", "shell", "kubectl"} {
		if strings.Contains(body, forbidden) {
			t.Fatalf("skill asks for %q", forbidden)
		}
	}
}

// firstLineDifference reports the first mismatching line of two documents.
func firstLineDifference(got, want string) string {
	gotLines := strings.Split(got, "\n")
	wantLines := strings.Split(want, "\n")
	for index := 0; index < len(gotLines) && index < len(wantLines); index++ {
		if gotLines[index] != wantLines[index] {
			return "line " + strconv.Itoa(index+1) + ":\n  got:  " + gotLines[index] + "\n  want: " + wantLines[index]
		}
	}
	return "length differs: got " + strconv.Itoa(len(gotLines)) + ", want " + strconv.Itoa(len(wantLines))
}
