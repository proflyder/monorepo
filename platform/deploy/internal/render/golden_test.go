package render

import (
	"bytes"
	"os"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
	"testing"

	"ricardo.local/monorepo/platform/deploy/internal/envfile"
)

// repoRoot walks up from this test file to the module root (the directory that
// holds go.mod). Under Bazel the test runs from a runfiles tree, so locate()
// prefers TEST_SRCDIR.
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

// locate returns a repository-relative path as an absolute path, using the
// Bazel runfiles tree when present.
func locate(t *testing.T, relative string) string {
	t.Helper()
	if sourceRoot := os.Getenv("TEST_SRCDIR"); sourceRoot != "" {
		// Bzlmod names the main repository `_main` (TEST_WORKSPACE); older
		// layouts use the module name. Try both before falling back.
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

// goldenCase is one unit's rendering fixture. The inputs, the `--set` overrides
// and the env files are committed under platform/deploy/testdata so the test
// renders exactly what the Python renderer produced before the cutover.
type goldenCase struct {
	unit   string
	preset string
	env    string
	sets   []string
	files  []string
}

// image is a zero-filled digest reference: TEST-NET-style placeholder data,
// never a real image.
func image(name string, repeated int) string {
	return name + "@sha256:" + strings.Repeat(strconv.Itoa(repeated), 64)
}

var goldenCases = []goldenCase{
	{
		unit:   "scheduler",
		preset: "scheduler",
		env:    "platform/deploy/testdata/env/scheduler.env.example",
		sets: []string{
			"BUILD_ID=0123456789ab",
			"SCHEDULER_IMAGE=" + image("ghcr.io/proflyder/scheduler", 0),
			"HA_BRIDGE_IMAGE=" + image("ghcr.io/proflyder/ha-bridge", 1),
			"HA_CONTROL_IMAGE=" + image("ghcr.io/proflyder/ha-control", 2),
			"PREFLIGHT_IMAGE=" + image("ghcr.io/proflyder/home-deploy", 3),
		},
		files: []string{
			"projects/home/kubernetes/scheduler/manifests/scheduler.yaml",
			"projects/home/kubernetes/scheduler/manifests/bridge.yaml",
			"projects/home/kubernetes/scheduler/manifests/control.yaml",
			"projects/home/kubernetes/scheduler/manifests/control-service.yaml",
			"projects/home/kubernetes/scheduler/manifests/preflight-job.yaml",
		},
	},
	{
		unit:   "currency-bot",
		preset: "currency-bot",
		env:    "platform/deploy/testdata/env/currency-bot.env.example",
		sets: []string{
			"BUILD_ID=0123456789ab",
			"CURRENCY_BOT_IMAGE=" + image("ghcr.io/proflyder/currency-bot", 4),
			"CLOUDFLARED_IMAGE=" + image("cloudflare/cloudflared", 5),
			"CURRENCY_WEBHOOK_URL=https://bot.example.test/telegram/webhook",
			"CURRENCY_BOT_NODE_PORT=30100",
		},
		files: []string{
			"projects/home/kubernetes/currency-bot/manifests/currency-bot.yaml",
			"projects/home/kubernetes/currency-bot/manifests/cloudflared.yaml",
			"projects/home/kubernetes/currency-bot/manifests/network-policies.yaml",
			"projects/home/kubernetes/currency-bot/manifests/servicemonitor.yaml",
		},
	},
	{
		unit:   "mcp-gateway",
		preset: "mcp-gateway",
		env:    "platform/deploy/testdata/env/mcp-gateway.env.example",
		sets: []string{
			"BUILD_ID=0123456789ab",
			"MCP_GATEWAY_IMAGE=" + image("ghcr.io/proflyder/mcp-gateway", 6),
			"BB_CT_IP=192.0.2.30",
			"BB_PROJECT_STATION_LAB=proj_0000000000000001",
			"BB_PROJECT_ENGLISH_NOTES=proj_0000000000000003",
			"BB_DISPATCHER_THREAD_ID=thr_0000000000000002",
		},
		files: []string{
			"projects/home/kubernetes/mcp-gateway/manifests/deployment.yaml",
			"projects/home/kubernetes/mcp-gateway/manifests/network-policies.yaml",
			"projects/home/kubernetes/mcp-gateway/manifests/servicemonitor.yaml",
			"projects/home/kubernetes/mcp-gateway/manifests/catalog.yaml",
			"projects/home/kubernetes/mcp-gateway/manifests/bb-targets.yaml",
		},
	},
	{
		unit:   "wakegrant",
		preset: "wakegrant",
		env:    "platform/deploy/testdata/env/wakegrant.env.example",
		sets: []string{
			"BUILD_ID=0123456789ab",
			"WAKE_GRANT_IMAGE=" + image("ghcr.io/proflyder/wakegrant", 7),
			"WAKE_GRANT_NODE_PORT=30101",
		},
		files: []string{
			"projects/home/kubernetes/wakegrant/manifests/wakegrant.yaml",
		},
	},
	{
		unit:   "nft",
		preset: "scheduler",
		env:    "platform/deploy/testdata/env/scheduler.env.example",
		sets: []string{
			"BUILD_ID=0123456789ab",
			"SCHEDULER_IMAGE=" + image("ghcr.io/proflyder/scheduler", 0),
			"HA_BRIDGE_IMAGE=" + image("ghcr.io/proflyder/ha-bridge", 1),
			"HA_CONTROL_IMAGE=" + image("ghcr.io/proflyder/ha-control", 2),
			"PREFLIGHT_IMAGE=" + image("ghcr.io/proflyder/home-deploy", 3),
		},
		files: []string{
			"projects/home/kubernetes/scheduler/nft/scheduler-nodeport.nft",
			"projects/home/kubernetes/currency-bot/nft/currency-bot-nodeport.nft",
		},
	},
}

// ingressValues is the fixed key set of the ingress deploy script, identical to
// the values the golden fixtures were generated with.
var ingressValues = map[string]string{
	"HOME_DOMAIN":   "home.example.test",
	"LAN_CIDR":      "192.0.2.0/24",
	"HAOS_ADDRESS":  "192.0.2.28",
	"HA_HOST":       "ha.home.example.test",
	"GRAFANA_HOST":  "grafana.home.example.test",
	"ALARMS_HOST":   "alarms.home.example.test",
	"CONTROL_HOST":  "control.home.example.test",
	"CURRENCY_HOST": "currency.home.example.test",
}

var ingressFiles = []string{
	"projects/home/kubernetes/ingress/manifests/issuers.yaml",
	"projects/home/kubernetes/ingress/manifests/certificate.yaml",
	"projects/home/kubernetes/ingress/manifests/certificate-staging.yaml",
	"projects/home/kubernetes/ingress/manifests/middleware.yaml",
	"projects/home/kubernetes/ingress/manifests/backends.yaml",
	"projects/home/kubernetes/ingress/manifests/routes.yaml",
}

func valuesFor(t *testing.T, tc goldenCase) map[string]string {
	t.Helper()
	values, err := envfile.Parse(locate(t, tc.env))
	if err != nil {
		t.Fatalf("%s: parse env: %v", tc.unit, err)
	}
	for _, item := range tc.sets {
		key, value, _ := strings.Cut(item, "=")
		values[key] = value
	}
	return values
}

func TestRenderMatchesPythonGolden(t *testing.T) {
	for _, tc := range goldenCases {
		t.Run(tc.unit, func(t *testing.T) {
			preset, err := PresetFor(tc.preset)
			if err != nil {
				t.Fatalf("preset %s: %v", tc.preset, err)
			}
			values := valuesFor(t, tc)
			for _, file := range tc.files {
				raw, err := os.ReadFile(locate(t, file))
				if err != nil {
					t.Fatalf("read %s: %v", file, err)
				}
				got, err := Render(string(raw), values, file, preset)
				if err != nil {
					t.Fatalf("render %s: %v", file, err)
				}
				golden := locate(t, filepath.Join("platform/deploy/testdata/golden", tc.unit, filepath.Base(file)))
				want, err := os.ReadFile(golden)
				if err != nil {
					t.Fatalf("read golden %s: %v", golden, err)
				}
				if got != string(want) {
					t.Errorf("%s does not match the Python golden output\n%s", file, firstDifference(got, string(want)))
				}
			}
		})
	}
}

func TestIngressMatchesPythonGolden(t *testing.T) {
	preset, err := PresetFor("ingress")
	if err != nil {
		t.Fatalf("preset ingress: %v", err)
	}
	for _, file := range ingressFiles {
		raw, err := os.ReadFile(locate(t, file))
		if err != nil {
			t.Fatalf("read %s: %v", file, err)
		}
		got, err := Render(string(raw), ingressValues, file, preset)
		if err != nil {
			t.Fatalf("render %s: %v", file, err)
		}
		golden := locate(t, filepath.Join("platform/deploy/testdata/golden/ingress", filepath.Base(file)))
		want, err := os.ReadFile(golden)
		if err != nil {
			t.Fatalf("read golden %s: %v", golden, err)
		}
		if got != string(want) {
			t.Errorf("%s does not match the Python golden output\n%s", file, firstDifference(got, string(want)))
		}
	}
}

// firstDifference describes the first mismatching line, which is enough to
// debug a rendering regression without dumping whole manifests.
func firstDifference(got, want string) string {
	gotLines := strings.Split(got, "\n")
	wantLines := strings.Split(want, "\n")
	for index := 0; index < len(gotLines) && index < len(wantLines); index++ {
		if gotLines[index] != wantLines[index] {
			return "line " + strconv.Itoa(index+1) + ":\n  got:  " + gotLines[index] + "\n  want: " + wantLines[index]
		}
	}
	return "length differs: got " + strconv.Itoa(len(gotLines)) + " lines, want " + strconv.Itoa(len(wantLines))
}

func TestRenderSubstitutionAndRefusals(t *testing.T) {
	preset, err := PresetFor("scheduler")
	if err != nil {
		t.Fatal(err)
	}
	values := map[string]string{"LAN_CIDR": "192.0.2.0/24", "NODE_LAN_IP": "192.0.2.27"}

	rendered, err := Render("cidr=${LAN_CIDR} ip=${NODE_LAN_IP}\n", values, "t.yaml", preset)
	if err != nil {
		t.Fatalf("render: %v", err)
	}
	if rendered != "cidr=192.0.2.0/24 ip=192.0.2.27\n" {
		t.Fatalf("unexpected output %q", rendered)
	}

	cases := []struct {
		name   string
		text   string
		values map[string]string
		want   string
	}{
		{"unknown placeholder", "${NOT_ALLOWED}\n", values, "t.yaml: placeholder ${NOT_ALLOWED} is not allowed"},
		{"missing value", "${BUILD_ID}\n", values, "t.yaml: missing value for ${BUILD_ID}"},
		{"empty value", "${BUILD_ID}\n", map[string]string{"BUILD_ID": ""}, "t.yaml: missing value for ${BUILD_ID}"},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			_, err := Render(tc.text, tc.values, "t.yaml", preset)
			if err == nil || err.Error() != tc.want {
				t.Fatalf("got error %v, want %q", err, tc.want)
			}
		})
	}

	// Lower-case placeholders are not placeholders and stay literal.
	untouched, err := Render("${lower}\n", values, "t.yaml", preset)
	if err != nil {
		t.Fatalf("lower-case placeholder should be left alone: %v", err)
	}
	if untouched != "${lower}\n" {
		t.Fatalf("unexpected output %q", untouched)
	}

	// A value is inserted literally and is not re-expanded, so a value that
	// itself looks like a placeholder trips the leftover check (the Python
	// renderer refuses the same way instead of recursively substituting).
	if _, err := Render("${BUILD_ID}\n", map[string]string{"BUILD_ID": "${LAN_CIDR}"}, "t.yaml", preset); err == nil {
		t.Fatal("a placeholder-looking value must be refused, not re-expanded")
	}
}

func TestRenderSetsAndPresets(t *testing.T) {
	preset, err := PresetFor("scheduler")
	if err != nil {
		t.Fatal(err)
	}
	if _, err := Render("${MCP_GATEWAY_IMAGE}", nil, "t.yaml", preset); err == nil {
		t.Fatal("scheduler preset must not allow a gateway-only placeholder")
	}
	gateway, err := PresetFor("mcp-gateway")
	if err != nil {
		t.Fatal(err)
	}
	if _, err := Render("${MCP_GATEWAY_IMAGE}", map[string]string{"MCP_GATEWAY_IMAGE": "image"}, "t.yaml", gateway); err != nil {
		t.Fatalf("gateway preset must allow its own placeholder: %v", err)
	}
	if _, err := PresetFor("nope"); err == nil {
		t.Fatal("unknown unit must be rejected")
	}
}

func TestIngressLeftoverCheck(t *testing.T) {
	preset, err := PresetFor("ingress")
	if err != nil {
		t.Fatal(err)
	}
	_, err = Render("${HOME_DOMAIN}\n${lower}\n", ingressValues, "issuers.yaml", preset)
	if err == nil || !strings.Contains(err.Error(), "unrendered placeholder in issuers.yaml") {
		t.Fatalf("ingress must refuse any leftover `${`: %v", err)
	}
}

func TestRunWritesRenderedFiles(t *testing.T) {
	envPath := filepath.Join(t.TempDir(), "scheduler.env")
	if err := os.WriteFile(envPath, []byte("# comment\nLAN_CIDR=192.0.2.0/24\nexport NODE_LAN_IP='192.0.2.27'\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	template := filepath.Join(t.TempDir(), "manifest.yaml")
	if err := os.WriteFile(template, []byte("cidr: ${LAN_CIDR}\nip: ${NODE_LAN_IP}\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	outDir := filepath.Join(t.TempDir(), "out")
	var stdout bytes.Buffer
	err := Run(Options{
		EnvFile: envPath,
		Sets:    []string{"BUILD_ID=abc"},
		OutDir:  outDir,
		Files:   []string{template},
		Unit:    "scheduler",
		Stdout:  &stdout,
	})
	if err != nil {
		t.Fatalf("run: %v", err)
	}
	rendered, err := os.ReadFile(filepath.Join(outDir, "manifest.yaml"))
	if err != nil {
		t.Fatal(err)
	}
	if string(rendered) != "cidr: 192.0.2.0/24\nip: 192.0.2.27\n" {
		t.Fatalf("unexpected output %q", rendered)
	}
	if !strings.Contains(stdout.String(), "rendered "+template) {
		t.Fatalf("missing progress line: %q", stdout.String())
	}

	// A `--set` for a key outside the preset fails before writing anything.
	err = Run(Options{
		OutDir: filepath.Join(t.TempDir(), "out2"),
		Sets:   []string{"NOT_ALLOWED=1"},
		Files:  []string{template},
		Unit:   "scheduler",
	})
	if err == nil || err.Error() != "--set NOT_ALLOWED: not an allowed placeholder" {
		t.Fatalf("unexpected error: %v", err)
	}
}
