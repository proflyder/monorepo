package tools

import (
	"context"
	"os"
	"strings"
	"testing"
	"time"
	"unicode/utf8"

	"ricardo.local/monorepo/solutions/mcp-gateway/internal/catalog"
	"ricardo.local/monorepo/solutions/mcp-gateway/internal/checks"
	"ricardo.local/monorepo/solutions/mcp-gateway/internal/kubernetes"
)

func healthyReportDeps(t *testing.T) *Deps {
	t.Helper()
	d := testDeps(t)
	kube := d.Kube.(*fakeKube)
	for _, name := range []string{"scheduler", "hermes"} {
		kube.workloads[kubeKey("home", "Deployment", name)] = kubernetes.Workload{
			Namespace: "home", Kind: "Deployment", Name: name,
			Desired: 1, Ready: 1, Available: 1, Updated: 1,
			Selector: map[string]string{"app": name},
		}
	}
	kube.workloads[kubeKey("home", "StatefulSet", "readonly")] = kubernetes.Workload{
		Namespace: "home", Kind: "StatefulSet", Name: "readonly",
		Desired: 1, Ready: 1, Available: 1, Updated: 1,
	}
	kube.pods["home"] = []kubernetes.Pod{{
		Namespace: "home", Name: "scheduler-1", Ready: true,
		StartTime: time.Date(2026, 10, 30, 12, 0, 0, 0, time.UTC),
		Containers: []kubernetes.ContainerStatus{{
			Name: "scheduler", Image: "ghcr.io/example/scheduler@sha256:abcdef0123456789", Ready: true,
		}},
	}}
	d.Checks = &fakeChecks{byService: map[string][]checks.Result{
		"ha": {
			{Service: "ha", Type: "tcp", Label: "HA API", OK: true, Value: "192.0.2.28:8123"},
			{Service: "ha", Type: "ha_template", Label: "Pulsar", OK: true, Value: "1.2.3"},
		},
	}}
	return d
}

func TestInfraReportHealthy(t *testing.T) {
	d := healthyReportDeps(t)
	_, out, err := d.infraReport(context.Background(), nil, infraReportInput{})
	if err != nil {
		t.Fatalf("infraReport: %v", err)
	}
	result := resultOf(t, out)
	if !strings.Contains(result.Speech, "Всё работает") || !strings.Contains(result.Speech, "Telegram") {
		t.Fatalf("speech = %q", result.Speech)
	}
	detail := result.Details.(infraReportDetail)
	if detail.Problems != 0 || detail.OK != len(d.Catalog.Services) {
		t.Fatalf("detail = %+v", detail)
	}
	if detail.Text == "" || len([]rune(detail.Text)) > reportTextCap {
		t.Fatalf("text length = %d", len([]rune(detail.Text)))
	}
	if !strings.HasPrefix(detail.Text, "Отчёт по инфраструктуре, ") {
		t.Fatalf("text must start with the header:\n%s", detail.Text)
	}
	// details.text is the ready Telegram message: no duplicated verdict with
	// the offer question that only makes sense for the voice path.
	if strings.Contains(detail.Text, "Прислать подробности") {
		t.Fatalf("text contains the speech offer:\n%s", detail.Text)
	}
	for _, want := range []string{
		"✅ Всё работает: 4 из 4",
		"В норме",
		"✅ K3s / home: Планировщик, Гермес, Readonly",
		"✅ LAN / 192.0.2.28: Дом",
	} {
		if !strings.Contains(detail.Text, want) {
			t.Fatalf("text missing %q:\n%s", want, detail.Text)
		}
	}
	// Images, age and versions stay in the structured details by default.
	for _, unwanted := range []string{"scheduler@sha256", "Pulsar: 1.2.3", "Версии"} {
		if strings.Contains(detail.Text, unwanted) {
			t.Fatalf("default text unexpectedly contains %q:\n%s", unwanted, detail.Text)
		}
	}
}

func TestInfraReportVerboseAddsVersions(t *testing.T) {
	d := healthyReportDeps(t)
	_, out, _ := d.infraReport(context.Background(), nil, infraReportInput{Verbose: true})
	detail := resultOf(t, out).Details.(infraReportDetail)
	for _, want := range []string{"Версии", "scheduler@sha256:abcdef01", "возраст 2д", "Pulsar: 1.2.3"} {
		if !strings.Contains(detail.Text, want) {
			t.Fatalf("verbose text missing %q:\n%s", want, detail.Text)
		}
	}
}

func TestInfraReportProblemsAndSpeech(t *testing.T) {
	d := healthyReportDeps(t)
	kube := d.Kube.(*fakeKube)
	kube.workloads[kubeKey("home", "Deployment", "scheduler")] = kubernetes.Workload{
		Namespace: "home", Kind: "Deployment", Name: "scheduler",
		Desired: 1, Ready: 0, Available: 0, Updated: 1,
	}
	d.Checks.(*fakeChecks).byService["ha"] = []checks.Result{{
		Service: "ha", Type: "tcp", Label: "HA API", OK: false, Error: "connection refused",
	}}

	_, out, _ := d.infraReport(context.Background(), nil, infraReportInput{})
	result := resultOf(t, out)
	detail := result.Details.(infraReportDetail)
	if detail.Problems != 2 {
		t.Fatalf("problems = %d (%+v)", detail.Problems, detail.Services)
	}
	if !strings.Contains(result.Speech, "Проблемы") {
		t.Fatalf("speech = %q", result.Speech)
	}
	for _, want := range []string{
		"⚠️ Есть проблемы: 2 из 4",
		"Проблемы",
		"❌ Планировщик — готовы 0 из 1",
		"❌ Дом — HA API: connection refused",
		"В норме",
	} {
		if !strings.Contains(detail.Text, want) {
			t.Fatalf("text missing %q:\n%s", want, detail.Text)
		}
	}
	if strings.Contains(detail.Text, "Прислать подробности") {
		t.Fatalf("text contains the speech offer:\n%s", detail.Text)
	}
}

const reportLayoutCatalog = `
group_order: [Голос, Дом, Мониторинг]
services:
  - id: hermes
    title: Hermes
    group: Голос
    aliases: {ru: [гермес], en: [hermes]}
    kind: k8s
    namespace: home
    workload: {kind: Deployment, name: hermes}
    restart: {allowed: false}
  - id: prometheus
    title: Prometheus
    group: Мониторинг
    aliases: {ru: [прометеус], en: [prometheus]}
    kind: k8s
    namespace: home
    containers: [prometheus]
    workload: {kind: StatefulSet, name: prometheus-monitoring-prometheus}
    restart: {allowed: false}
  - id: home-assistant
    title: Home Assistant
    group: Дом
    host_title: HAOS VM
    aliases: {ru: [дом, ха], en: [home-assistant]}
    kind: external
    checks:
      - {type: ha_unavailable_summary, label: недоступные сущности, domains: [media_player]}
    restart: {allowed: false}
  - id: music-assistant
    title: Music Assistant
    group: Дом
    host_title: HAOS VM
    aliases: {ru: [музыка], en: [music-assistant]}
    kind: external
    checks:
      - {type: tcp, label: API, host: 192.0.2.10, port: 8095}
    restart: {allowed: false}
`

func layoutReportDeps(t *testing.T, healthy bool) *Deps {
	t.Helper()
	d := testDeps(t)
	cat, err := catalog.Parse([]byte(reportLayoutCatalog))
	if err != nil {
		t.Fatalf("catalog: %v", err)
	}
	d.Catalog = cat
	kube := d.Kube.(*fakeKube)
	kube.workloads[kubeKey("home", "Deployment", "hermes")] = kubernetes.Workload{
		Namespace: "home", Kind: "Deployment", Name: "hermes",
		Desired: 1, Ready: 1, Available: 1, Updated: 1,
	}
	prometheusReady := int64(1)
	if !healthy {
		prometheusReady = 0
	}
	kube.workloads[kubeKey("home", "StatefulSet", "prometheus-monitoring-prometheus")] = kubernetes.Workload{
		Namespace: "home", Kind: "StatefulSet", Name: "prometheus-monitoring-prometheus",
		Desired: 1, Ready: prometheusReady, Available: prometheusReady, Updated: 1,
	}
	// containerStatuses are alphabetical; the config-reloader sidecar must not
	// become the reported image.
	kube.pods["home"] = []kubernetes.Pod{{
		Namespace: "home", Name: "prometheus-1", Ready: true,
		Containers: []kubernetes.ContainerStatus{
			{Name: "prometheus-config-reloader", Image: "quay.io/prometheus-operator/prometheus-config-reloader:v0.94.2"},
			{Name: "prometheus", Image: "quay.io/prometheus/prometheus:v3.8.0"},
		},
	}}
	haCheck := checks.Result{
		Service: "home-assistant", Type: catalog.CheckHAUnavailable,
		Label: "недоступные сущности", OK: healthy, Value: "0",
	}
	if !healthy {
		haCheck.OK = false
		haCheck.Value = "1"
		haCheck.Error = "недоступно сущностей: 1"
		haCheck.Details = []string{"media_player.kitchen"}
	}
	d.Checks = &fakeChecks{byService: map[string][]checks.Result{
		"hermes":          {},
		"home-assistant":  {haCheck},
		"music-assistant": {{Service: "music-assistant", Type: "tcp", Label: "API", OK: true}},
	}}
	return d
}

func TestInfraReportLayoutUsesTitlesGroupsAndMainImage(t *testing.T) {
	d := layoutReportDeps(t, false)

	_, out, _ := d.infraReport(context.Background(), nil, infraReportInput{})
	detail := resultOf(t, out).Details.(infraReportDetail)
	text := detail.Text
	for _, want := range []string{
		"⚠️ Есть проблемы: 2 из 4",
		"❌ Home Assistant — недоступно сущностей: 1 (media_player.kitchen)",
		"❌ Prometheus — готовы 0 из 1",
		"✅ Голос: Hermes",
		"✅ Дом: Music Assistant",
	} {
		if !strings.Contains(text, want) {
			t.Fatalf("text missing %q:\n%s", want, text)
		}
	}
	// The label appears once, not as "недоступные сущности: недоступно ...".
	if strings.Contains(text, "недоступные сущности: недоступно") {
		t.Fatalf("duplicated unavailable label:\n%s", text)
	}
	if strings.Index(text, "✅ Голос") > strings.Index(text, "✅ Дом") {
		t.Fatalf("group order is not fixed by group_order:\n%s", text)
	}
	for _, service := range detail.Services {
		if service.ID == "prometheus" {
			if service.Image != "prometheus:v3.8.0" {
				t.Fatalf("image = %q, want the main container", service.Image)
			}
		}
	}
	if strings.Contains(text, "config-reloader") || strings.Contains(text, "v0.94.2") {
		t.Fatalf("default text leaked a sidecar image:\n%s", text)
	}
}

func TestInfraReportGoldenSamples(t *testing.T) {
	cases := []struct {
		name    string
		healthy bool
		golden  string
	}{
		{name: "all ok", healthy: true, golden: "report_ok.txt"},
		{name: "two problems", healthy: false, golden: "report_problems.txt"},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			d := layoutReportDeps(t, tc.healthy)
			_, out, _ := d.infraReport(context.Background(), nil, infraReportInput{})
			detail := resultOf(t, out).Details.(infraReportDetail)
			want := readGolden(t, tc.golden)
			if got := strings.TrimSpace(detail.Text); got != strings.TrimSpace(want) {
				t.Fatalf("report text mismatch\n--- got ---\n%s\n--- want ---\n%s", got, want)
			}
		})
	}
}

func readGolden(t *testing.T, name string) string {
	t.Helper()
	for _, candidate := range []string{
		"../../testdata/" + name,
		"solutions/mcp-gateway/testdata/" + name,
	} {
		data, err := os.ReadFile(candidate)
		if err == nil {
			return string(data)
		}
	}
	t.Fatalf("golden %s not found in runfiles", name)
	return ""
}

func TestPickImagePrefersCatalogThenWorkloadThenSpecOrder(t *testing.T) {
	pod := kubernetes.Pod{Containers: []kubernetes.ContainerStatus{
		{Name: "prometheus-config-reloader", Image: "reloader:v1"},
		{Name: "prometheus", Image: "prometheus:v3"},
	}}
	cases := []struct {
		name    string
		service catalog.Service
		want    string
	}{
		{
			name:    "catalog container",
			service: catalog.Service{Containers: []string{"prometheus"}, Workload: catalog.Workload{Name: "prometheus-monitoring-prometheus"}},
			want:    "prometheus:v3",
		},
		{
			name:    "workload suffix",
			service: catalog.Service{Workload: catalog.Workload{Name: "prometheus-monitoring-prometheus"}},
			want:    "prometheus:v3",
		},
		{
			name:    "spec order fallback",
			service: catalog.Service{Workload: catalog.Workload{Name: "unknown"}},
			want:    "reloader:v1",
		},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			if got := pickImage(pod, tc.service); got != tc.want {
				t.Fatalf("pickImage = %q, want %q", got, tc.want)
			}
		})
	}
}

func TestDisplayNameUsesTitleThenAlias(t *testing.T) {
	titled := catalog.Service{Title: "GigaAM", Aliases: catalog.Aliases{RU: []string{"гигаам"}}}
	if got := DisplayName(titled); got != "GigaAM" {
		t.Fatalf("DisplayName = %q, want the title", got)
	}
	legacy := catalog.Service{Aliases: catalog.Aliases{RU: []string{"гигаам"}}}
	if got := DisplayName(legacy); got != "Гигаам" {
		t.Fatalf("DisplayName fallback = %q", got)
	}
}

func TestUnavailableIssueFormatsLabelOnceAndCapsEntities(t *testing.T) {
	result := checks.Result{
		Type: catalog.CheckHAUnavailable, Label: "недоступные сущности",
		Error: "недоступно сущностей: 7", Value: "7",
		Details: []string{"media_player.1", "media_player.2", "media_player.3",
			"media_player.4", "media_player.5", "media_player.6", "media_player.7"},
	}
	got := checkIssue(result)
	want := "недоступно сущностей: 7 (media_player.1, media_player.2, media_player.3, media_player.4, media_player.5)"
	if got != want {
		t.Fatalf("checkIssue = %q, want %q", got, want)
	}
	if strings.Contains(got, "недоступные сущности:") {
		t.Fatalf("label duplicated: %q", got)
	}
}

func TestInfraReportNotConfiguredChecksAreNotFailures(t *testing.T) {
	d := healthyReportDeps(t)
	d.Checks.(*fakeChecks).byService["ha"] = []checks.Result{{
		Service: "ha", Type: "ha_state", Label: "satellite", OK: true, Skipped: true, Value: "Home Assistant не настроен",
	}}
	_, out, _ := d.infraReport(context.Background(), nil, infraReportInput{})
	result := resultOf(t, out)
	detail := result.Details.(infraReportDetail)
	if detail.Problems != 0 || detail.Unknown != 1 {
		t.Fatalf("problems=%d unknown=%d", detail.Problems, detail.Unknown)
	}
	if !strings.Contains(result.Speech, "не проверено 1") {
		t.Fatalf("speech = %q", result.Speech)
	}
	if !strings.Contains(detail.Text, "не проверено 1") {
		t.Fatalf("text = %s", detail.Text)
	}
}

func TestInfraReportTimeoutIsBounded(t *testing.T) {
	d := testDeps(t)
	d.ReportTimeout = 50 * time.Millisecond
	d.Checks = &blockingChecks{}
	start := time.Now()
	_, out, err := d.infraReport(context.Background(), nil, infraReportInput{})
	if err != nil {
		t.Fatalf("infraReport: %v", err)
	}
	if elapsed := time.Since(start); elapsed > time.Second {
		t.Fatalf("report took %v", elapsed)
	}
	if resultOf(t, out).Speech == "" {
		t.Fatal("empty speech")
	}
}

func TestCapReport(t *testing.T) {
	long := strings.Repeat("строка отчёта\n", 1000)
	capped := capReport(long)
	if !utf8.ValidString(capped) {
		t.Fatal("capped text is not valid UTF-8")
	}
	if len([]rune(capped)) > reportTextCap+50 {
		t.Fatalf("capped length = %d", len([]rune(capped)))
	}
	if !strings.Contains(capped, "отчёт сокращён") {
		t.Fatal("missing truncation marker")
	}
}

func TestShortImage(t *testing.T) {
	cases := map[string]string{
		"ghcr.io/example/hermes@sha256:0123456789abcdef": "hermes@sha256:01234567",
		"ghcr.io/example/hermes:1.2.3":                   "hermes:1.2.3",
		"":                                               "",
	}
	for input, want := range cases {
		if got := shortImage(input); got != want {
			t.Fatalf("shortImage(%q) = %q, want %q", input, got, want)
		}
	}
}

type blockingChecks struct{}

func (blockingChecks) RunAll(ctx context.Context, _ []catalog.Service) []checks.Result {
	<-ctx.Done()
	return nil
}
