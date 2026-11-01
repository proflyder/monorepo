package tools

import (
	"context"
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
	_, out, err := d.infraReport(context.Background(), nil, struct{}{})
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
	for _, want := range []string{"K3s / home", "LAN / 192.0.2.28", "Планировщик: ok", "scheduler@sha256:abcdef01", "Pulsar: 1.2.3"} {
		if !strings.Contains(detail.Text, want) {
			t.Fatalf("text missing %q:\n%s", want, detail.Text)
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

	_, out, _ := d.infraReport(context.Background(), nil, struct{}{})
	result := resultOf(t, out)
	detail := result.Details.(infraReportDetail)
	if detail.Problems != 2 {
		t.Fatalf("problems = %d (%+v)", detail.Problems, detail.Services)
	}
	if !strings.Contains(result.Speech, "Проблемы") {
		t.Fatalf("speech = %q", result.Speech)
	}
	for _, want := range []string{"Планировщик: down", "Дом: down", "connection refused"} {
		if !strings.Contains(detail.Text, want) {
			t.Fatalf("text missing %q:\n%s", want, detail.Text)
		}
	}
}

func TestInfraReportRestartsAndAge(t *testing.T) {
	d := healthyReportDeps(t)
	kube := d.Kube.(*fakeKube)
	kube.pods["home"] = []kubernetes.Pod{{
		Namespace: "home", Name: "scheduler-1", Ready: true,
		StartTime: d.now().Add(-50 * time.Hour),
		Containers: []kubernetes.ContainerStatus{{
			Name: "scheduler", Image: "ghcr.io/example/scheduler@sha256:abcdef0123456789", Ready: true,
			LastTerminatedAt: d.now().Add(-10 * time.Minute),
		}},
	}}
	_, out, _ := d.infraReport(context.Background(), nil, struct{}{})
	detail := resultOf(t, out).Details.(infraReportDetail)
	for _, service := range detail.Services {
		if service.ID != "scheduler" && service.ID != "hermes" {
			continue
		}
		if service.Restarts != 1 || service.Age != "2д" {
			t.Fatalf("%s restarts=%d age=%q", service.ID, service.Restarts, service.Age)
		}
	}
}

func TestInfraReportNotConfiguredChecksAreNotFailures(t *testing.T) {
	d := healthyReportDeps(t)
	d.Checks.(*fakeChecks).byService["ha"] = []checks.Result{{
		Service: "ha", Type: "ha_state", Label: "satellite", OK: true, Skipped: true, Value: "Home Assistant не настроен",
	}}
	_, out, _ := d.infraReport(context.Background(), nil, struct{}{})
	result := resultOf(t, out)
	detail := result.Details.(infraReportDetail)
	if detail.Problems != 0 || detail.Unknown != 1 {
		t.Fatalf("problems=%d unknown=%d", detail.Problems, detail.Unknown)
	}
	if !strings.Contains(result.Speech, "не проверено 1") {
		t.Fatalf("speech = %q", result.Speech)
	}
	if !strings.Contains(detail.Text, "проверки не настроены") {
		t.Fatalf("text = %s", detail.Text)
	}
}

func TestInfraReportTimeoutIsBounded(t *testing.T) {
	d := testDeps(t)
	d.ReportTimeout = 50 * time.Millisecond
	d.Checks = &blockingChecks{}
	start := time.Now()
	_, out, err := d.infraReport(context.Background(), nil, struct{}{})
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
