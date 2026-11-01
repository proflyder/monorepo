package tools

import (
	"context"
	"errors"
	"strings"
	"testing"
	"time"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"ricardo.local/monorepo/solutions/mcp-gateway/internal/alertmanager"
	"ricardo.local/monorepo/solutions/mcp-gateway/internal/checks"
	"ricardo.local/monorepo/solutions/mcp-gateway/internal/kubernetes"
	"ricardo.local/monorepo/solutions/mcp-gateway/internal/loki"
	"ricardo.local/monorepo/solutions/mcp-gateway/internal/prometheus"
)

func healthyScheduler() kubernetes.Workload {
	return kubernetes.Workload{
		Namespace: "home", Kind: "Deployment", Name: "scheduler",
		Desired: 1, Ready: 1, Available: 1, Updated: 1,
		Generation: 2, ObservedGeneration: 2,
		Containers: []string{"scheduler"},
		Selector:   map[string]string{"app.kubernetes.io/name": "scheduler"},
	}
}

func TestInfraStatusHealthy(t *testing.T) {
	d := testDeps(t)
	kube := d.Kube.(*fakeKube)
	kube.workloads[kubeKey("home", "Deployment", "scheduler")] = healthyScheduler()
	kube.workloads[kubeKey("home", "Deployment", "hermes")] = kubernetes.Workload{
		Namespace: "home", Kind: "Deployment", Name: "hermes", Desired: 1, Ready: 1, Available: 1, Updated: 1,
	}
	kube.workloads[kubeKey("home", "StatefulSet", "readonly")] = kubernetes.Workload{
		Namespace: "home", Kind: "StatefulSet", Name: "readonly", Desired: 1, Ready: 1, Available: 1, Updated: 1,
	}

	res, out, err := d.infraStatus(context.Background(), nil, struct{}{})
	if err != nil {
		t.Fatalf("infraStatus: %v", err)
	}
	if res == nil {
		t.Fatal("nil result")
	}
	result := resultOf(t, out)
	if !strings.Contains(result.Speech, "Всё в порядке") {
		t.Fatalf("speech = %q", result.Speech)
	}
	detail, ok := result.Details.(statusDetail)
	if !ok {
		t.Fatalf("details = %T", result.Details)
	}
	if detail.Degraded || detail.Partial {
		t.Fatalf("detail = %+v", detail)
	}
	if len(detail.Workloads) != 3 {
		t.Fatalf("workloads = %d", len(detail.Workloads))
	}
}

func TestInfraStatusAggregatesProblems(t *testing.T) {
	d := testDeps(t)
	kube := d.Kube.(*fakeKube)
	kube.workloads[kubeKey("home", "Deployment", "scheduler")] = kubernetes.Workload{
		Namespace: "home", Kind: "Deployment", Name: "scheduler", Desired: 1, Ready: 0, Available: 0, Updated: 1,
	}
	kube.workloads[kubeKey("home", "Deployment", "hermes")] = healthyScheduler()
	kube.workloads[kubeKey("home", "StatefulSet", "readonly")] = healthyScheduler()
	kube.pods["home"] = []kubernetes.Pod{{
		Namespace: "home", Name: "scheduler-abc",
		Containers: []kubernetes.ContainerStatus{{Name: "scheduler", RestartCount: 12, WaitingReason: "CrashLoopBackOff"}},
	}}
	alerts := d.Alerts.(*fakeAlerts)
	alerts.alerts = []alertmanager.Alert{{
		Labels:      map[string]string{"alertname": "RicardoEndpointDown", "severity": "critical", "deployment": "scheduler"},
		Annotations: map[string]string{"summary": "Планировщик недоступен"},
		StartsAt:    time.Date(2026, 11, 1, 11, 0, 0, 0, time.UTC),
	}}
	prom := d.Prom.(*fakeProm)
	prom.results = map[string][]prometheus.Sample{
		`ricardo_endpoint_up{endpoint="scheduler"} == 0`: {{Value: 1}},
	}

	_, out, err := d.infraStatus(context.Background(), nil, struct{}{})
	if err != nil {
		t.Fatalf("infraStatus: %v", err)
	}
	result := resultOf(t, out)
	if !strings.Contains(result.Speech, "Проблемы") {
		t.Fatalf("speech = %q", result.Speech)
	}
	detail := result.Details.(statusDetail)
	if !detail.Degraded || detail.Partial {
		t.Fatalf("detail = %+v", detail)
	}
	if len(detail.Alerts) != 1 || len(detail.Signals) != 2 || len(detail.Restarts) != 1 {
		t.Fatalf("alerts=%d signals=%d restarts=%d", len(detail.Alerts), len(detail.Signals), len(detail.Restarts))
	}
	firing := 0
	for _, signal := range detail.Signals {
		if signal.Firing {
			firing++
		}
	}
	if firing != 1 {
		t.Fatalf("firing signals = %d", firing)
	}
}

func TestInfraStatusIgnoreAlerts(t *testing.T) {
	d := testDeps(t)
	kube := d.Kube.(*fakeKube)
	kube.workloads[kubeKey("home", "Deployment", "scheduler")] = healthyScheduler()
	kube.workloads[kubeKey("home", "Deployment", "hermes")] = healthyScheduler()
	kube.workloads[kubeKey("home", "StatefulSet", "readonly")] = healthyScheduler()
	d.Alerts.(*fakeAlerts).alerts = []alertmanager.Alert{
		{
			Labels:      map[string]string{"alertname": "Watchdog"},
			Annotations: map[string]string{"summary": "Always firing"},
			StartsAt:    time.Date(2026, 11, 1, 11, 0, 0, 0, time.UTC),
		},
		{
			Labels:      map[string]string{"alertname": "InfoInhibitor", "severity": "warning"},
			Annotations: map[string]string{"summary": "Info level inhibiting"},
		},
	}

	_, out, err := d.infraStatus(context.Background(), nil, struct{}{})
	if err != nil {
		t.Fatalf("infraStatus: %v", err)
	}
	result := resultOf(t, out)
	if !strings.Contains(result.Speech, "Всё в порядке") {
		t.Fatalf("speech = %q, want a healthy verdict", result.Speech)
	}
	detail := result.Details.(statusDetail)
	if detail.Degraded {
		t.Fatalf("degraded, ignored alerts leaked into problems: %+v", detail)
	}
	if len(detail.Alerts) != 0 {
		t.Fatalf("alerts = %+v, want none", detail.Alerts)
	}
	if len(detail.IgnoredAlerts) != 2 {
		t.Fatalf("ignoredAlerts = %+v, want 2", detail.IgnoredAlerts)
	}
	if detail.IgnoredAlerts[0].Name != "InfoInhibitor" || detail.IgnoredAlerts[1].Name != "Watchdog" {
		t.Fatalf("ignored alerts are not sorted by name: %+v", detail.IgnoredAlerts)
	}
}

func TestInfraStatusIgnoredAlertsDoNotHideRealProblems(t *testing.T) {
	d := testDeps(t)
	kube := d.Kube.(*fakeKube)
	kube.workloads[kubeKey("home", "Deployment", "scheduler")] = healthyScheduler()
	kube.workloads[kubeKey("home", "Deployment", "hermes")] = healthyScheduler()
	kube.workloads[kubeKey("home", "StatefulSet", "readonly")] = healthyScheduler()
	d.Alerts.(*fakeAlerts).alerts = []alertmanager.Alert{
		{Labels: map[string]string{"alertname": "Watchdog"}, Annotations: map[string]string{"summary": "Always firing"}},
		{
			Labels:      map[string]string{"alertname": "RicardoEndpointDown", "severity": "critical"},
			Annotations: map[string]string{"summary": "Планировщик недоступен"},
		},
	}

	_, out, err := d.infraStatus(context.Background(), nil, struct{}{})
	if err != nil {
		t.Fatalf("infraStatus: %v", err)
	}
	result := resultOf(t, out)
	if !strings.Contains(result.Speech, "Проблемы") || strings.Contains(result.Speech, "Always firing") {
		t.Fatalf("speech = %q", result.Speech)
	}
	detail := result.Details.(statusDetail)
	if !detail.Degraded {
		t.Fatal("real alert did not degrade the status")
	}
	if len(detail.Alerts) != 1 || len(detail.IgnoredAlerts) != 1 {
		t.Fatalf("alerts=%+v ignored=%+v", detail.Alerts, detail.IgnoredAlerts)
	}
}

func TestInfraStatusTimeoutIsBounded(t *testing.T) {
	d := testDeps(t)
	d.StatusTimeout = 120 * time.Millisecond
	d.QueryTimeout = 50 * time.Millisecond
	d.Prom.(*fakeProm).block = true
	kube := d.Kube.(*fakeKube)
	kube.workloads[kubeKey("home", "Deployment", "scheduler")] = healthyScheduler()
	kube.workloads[kubeKey("home", "Deployment", "hermes")] = healthyScheduler()
	kube.workloads[kubeKey("home", "StatefulSet", "readonly")] = healthyScheduler()

	start := time.Now()
	_, out, err := d.infraStatus(context.Background(), nil, struct{}{})
	elapsed := time.Since(start)
	if err != nil {
		t.Fatalf("infraStatus: %v", err)
	}
	if elapsed > time.Second {
		t.Fatalf("aggregate took %v", elapsed)
	}
	result := resultOf(t, out)
	detail := result.Details.(statusDetail)
	if !detail.Partial {
		t.Fatalf("partial = false, detail = %+v", detail)
	}
	if !strings.Contains(result.Speech, "неполная") {
		t.Fatalf("speech = %q", result.Speech)
	}
}

func TestInfraStatusBackendErrorsBecomePartial(t *testing.T) {
	d := testDeps(t)
	d.Alerts.(*fakeAlerts).err = errors.New("alertmanager down")
	d.Prom = nil
	d.Kube = nil

	_, out, err := d.infraStatus(context.Background(), nil, struct{}{})
	if err != nil {
		t.Fatalf("infraStatus: %v", err)
	}
	result := resultOf(t, out)
	detail := result.Details.(statusDetail)
	if !detail.Partial {
		t.Fatalf("partial = false")
	}
	if len(detail.Errors) == 0 {
		t.Fatal("no backend errors recorded")
	}
}

func TestInfraServiceResolvesAliases(t *testing.T) {
	d := testDeps(t)
	kube := d.Kube.(*fakeKube)
	kube.workloads[kubeKey("home", "Deployment", "hermes")] = kubernetes.Workload{
		Namespace: "home", Kind: "Deployment", Name: "hermes", Desired: 1, Ready: 1, Available: 1, Updated: 1,
		Containers: []string{"hermes", "voice-proxy"},
		Selector:   map[string]string{"app.kubernetes.io/name": "hermes"},
	}
	kube.pods["home"] = []kubernetes.Pod{{Name: "hermes-1", Ready: true,
		Containers: []kubernetes.ContainerStatus{{Name: "hermes", Ready: true}, {Name: "voice-proxy", Ready: true}}}}
	kube.events["home/hermes"] = []kubernetes.Event{{Type: "Normal", Reason: "Pulled", Message: "image pulled"}}

	for _, name := range []string{"ГЕРМЁС", "гермес", "ассистент", "Hermes"} {
		res, out, err := d.infraService(context.Background(), nil, serviceInput{Name: name})
		if err != nil || res == nil {
			t.Fatalf("infraService(%q): %v", name, err)
		}
		result := resultOf(t, out)
		if !strings.Contains(result.Speech, "Гермес") {
			t.Fatalf("infraService(%q) speech = %q", name, result.Speech)
		}
		detail := result.Details.(serviceDetail)
		if detail.Service.ID != "hermes" {
			t.Fatalf("resolved %q to %q", name, detail.Service.ID)
		}
		if len(detail.Pods) != 1 || len(detail.Events) != 1 {
			t.Fatalf("pods=%d events=%d", len(detail.Pods), len(detail.Events))
		}
	}
}

func TestInfraServiceUnknown(t *testing.T) {
	d := testDeps(t)
	_, out, err := d.infraService(context.Background(), nil, serviceInput{Name: "нет такого"})
	if err != nil {
		t.Fatalf("infraService: %v", err)
	}
	result := resultOf(t, out)
	if !strings.Contains(result.Speech, "Не нашёл") {
		t.Fatalf("speech = %q", result.Speech)
	}
}

func TestInfraServiceReportsRelatedAlerts(t *testing.T) {
	d := testDeps(t)
	kube := d.Kube.(*fakeKube)
	kube.workloads[kubeKey("home", "Deployment", "scheduler")] = healthyScheduler()
	d.Alerts.(*fakeAlerts).alerts = []alertmanager.Alert{
		{Labels: map[string]string{"alertname": "SchedulerDown", "deployment": "scheduler"}, Annotations: map[string]string{"summary": "Планировщик недоступен"}},
		{Labels: map[string]string{"alertname": "Other", "job": "unrelated"}, Annotations: map[string]string{"summary": "Другое"}},
	}
	_, out, _ := d.infraService(context.Background(), nil, serviceInput{Name: "scheduler"})
	result := resultOf(t, out)
	detail := result.Details.(serviceDetail)
	if len(detail.Alerts) != 1 || detail.Alerts[0].Name != "SchedulerDown" {
		t.Fatalf("alerts = %+v", detail.Alerts)
	}
	if !strings.Contains(result.Speech, "Планировщик недоступен") {
		t.Fatalf("speech = %q", result.Speech)
	}
}

func TestInfraLogsLokiRedactsAndGreps(t *testing.T) {
	d := testDeps(t)
	d.Loki.(*fakeLoki).lines = []loki.Line{
		{Timestamp: time.Now(), Text: "started ok"},
		{Timestamp: time.Now(), Text: "Authorization: Bearer supersecrettoken123 error"},
	}
	_, out, err := d.infraLogs(context.Background(), nil, logsInput{Name: "гермес", Grep: "error", Limit: 10})
	if err != nil {
		t.Fatalf("infraLogs: %v", err)
	}
	result := resultOf(t, out)
	detail := result.Details.(logsDetail)
	if detail.Source != "loki" {
		t.Fatalf("source = %q", detail.Source)
	}
	if strings.Contains(detail.Text, "supersecrettoken123") {
		t.Fatalf("secret leaked: %q", detail.Text)
	}
	if !strings.Contains(detail.Text, "[REDACTED]") {
		t.Fatalf("no redaction: %q", detail.Text)
	}
	if strings.Contains(detail.Text, "started ok") {
		t.Fatalf("grep did not filter: %q", detail.Text)
	}
	if !strings.Contains(result.Speech, "Логи Гермес") {
		t.Fatalf("speech = %q", result.Speech)
	}
}

func TestInfraLogsPodsFallback(t *testing.T) {
	d := testDeps(t)
	kube := d.Kube.(*fakeKube)
	kube.workloads[kubeKey("home", "Deployment", "scheduler")] = healthyScheduler()
	kube.pods["home"] = []kubernetes.Pod{{Name: "scheduler-1", Ready: true}}
	kube.logs[kubeKey("home", "scheduler-1", "scheduler")] = "tick ok\nERROR failed to fire\ntick ok\n"

	_, out, err := d.infraLogs(context.Background(), nil, logsInput{Name: "scheduler", Limit: 10})
	if err != nil {
		t.Fatalf("infraLogs: %v", err)
	}
	result := resultOf(t, out)
	detail := result.Details.(logsDetail)
	if detail.Source != "pods" {
		t.Fatalf("source = %q", detail.Source)
	}
	if !strings.Contains(detail.Text, "failed to fire") || !strings.Contains(detail.Text, "scheduler-1/scheduler") {
		t.Fatalf("text = %q", detail.Text)
	}
	if detail.Lines != 3 {
		t.Fatalf("lines = %d", detail.Lines)
	}
}

func TestInfraLogsNotConfigured(t *testing.T) {
	d := testDeps(t)
	_, out, _ := d.infraLogs(context.Background(), nil, logsInput{Name: "ha"})
	result := resultOf(t, out)
	if !strings.Contains(result.Speech, "не настроены") {
		t.Fatalf("speech = %q", result.Speech)
	}
}

func TestInfraLogsInvalidSince(t *testing.T) {
	d := testDeps(t)
	_, out, _ := d.infraLogs(context.Background(), nil, logsInput{Name: "scheduler", Since: "soon"})
	result := resultOf(t, out)
	if !strings.Contains(result.Speech, "период") {
		t.Fatalf("speech = %q", result.Speech)
	}
}

func TestInfraQuery(t *testing.T) {
	d := testDeps(t)
	d.Prom.(*fakeProm).results = map[string][]prometheus.Sample{
		"up": {{Labels: map[string]string{"job": "scheduler"}, Value: 1, Raw: "1"}},
	}
	_, out, err := d.infraQuery(context.Background(), nil, queryInput{PromQL: "up"})
	if err != nil {
		t.Fatalf("infraQuery: %v", err)
	}
	result := resultOf(t, out)
	detail := result.Details.(queryDetail)
	if len(detail.Samples) != 1 || detail.Samples[0].Labels["job"] != "scheduler" {
		t.Fatalf("detail = %+v", detail)
	}
	if _, out, _ := d.infraQuery(context.Background(), nil, queryInput{PromQL: ""}); !strings.Contains(resultOf(t, out).Speech, "Пустой") {
		t.Fatal("empty query not rejected")
	}
}

func TestInfraOverview(t *testing.T) {
	d := testDeps(t)
	_, out, err := d.infraOverview(context.Background(), nil, struct{}{})
	if err != nil {
		t.Fatalf("infraOverview: %v", err)
	}
	result := resultOf(t, out)
	details := result.Details.(map[string]string)
	if !strings.Contains(details["overview"], "Тестовая топология") {
		t.Fatalf("overview = %q", details["overview"])
	}
	if !strings.Contains(details["catalog"], "## scheduler") {
		t.Fatalf("catalog = %q", details["catalog"])
	}
}

func TestInfraAlertByFingerprint(t *testing.T) {
	d := testDeps(t)
	kube := d.Kube.(*fakeKube)
	kube.workloads[kubeKey("home", "Deployment", "scheduler")] = healthyScheduler()
	kube.pods["home"] = []kubernetes.Pod{{Name: "scheduler-1", Ready: true,
		Containers: []kubernetes.ContainerStatus{{Name: "scheduler", Ready: true}}}}
	d.Alerts.(*fakeAlerts).alerts = []alertmanager.Alert{{
		Fingerprint: "a1b2c3d4e5f60718",
		Labels:      map[string]string{"alertname": "RicardoEndpointDown", "severity": "critical", "namespace": "home", "deployment": "scheduler"},
		Annotations: map[string]string{"summary": "Планировщик недоступен", "description": "probe failed for 3m"},
		StartsAt:    time.Date(2026, 11, 1, 11, 0, 0, 0, time.UTC),
	}}

	_, out, err := d.infraAlert(context.Background(), nil, alertInput{Ref: "a1b2c3"})
	if err != nil {
		t.Fatalf("infraAlert: %v", err)
	}
	result := resultOf(t, out)
	if !strings.Contains(result.Speech, "Планировщик недоступен") {
		t.Fatalf("speech = %q", result.Speech)
	}
	detail := result.Details.(alertQueryDetail)
	if detail.Count != 1 || len(detail.Alerts) != 1 {
		t.Fatalf("detail = %+v", detail)
	}
	alert := detail.Alerts[0]
	if alert.Fingerprint != "a1b2c3d4e5f60718" || alert.Description != "probe failed for 3m" {
		t.Fatalf("alert = %+v", alert)
	}
	if alert.Labels["deployment"] != "scheduler" || alert.Annotations["summary"] == "" {
		t.Fatalf("labels/annotations missing: %+v", alert)
	}
	if len(detail.Services) == 0 || detail.Services[0].Service.ID != "scheduler" {
		t.Fatalf("services = %+v", detail.Services)
	}
	if detail.Services[0].Workload == nil || detail.Services[0].Workload.Ready != 1 {
		t.Fatalf("related workload = %+v", detail.Services[0].Workload)
	}
}

func TestInfraAlertByFullFingerprintAndName(t *testing.T) {
	d := testDeps(t)
	d.Alerts.(*fakeAlerts).alerts = []alertmanager.Alert{
		{Fingerprint: "fff000111222333444", Labels: map[string]string{"alertname": "DiskFull"}, Annotations: map[string]string{"summary": "Мало места"}},
	}
	for _, ref := range []string{"fff000111222333444", "diskfull", "DISKFULL"} {
		_, out, err := d.infraAlert(context.Background(), nil, alertInput{Ref: ref})
		if err != nil {
			t.Fatalf("infraAlert(%q): %v", ref, err)
		}
		result := resultOf(t, out)
		detail := result.Details.(alertQueryDetail)
		if detail.Count != 1 {
			t.Fatalf("ref %q matched %d alerts", ref, detail.Count)
		}
	}
}

func TestInfraAlertShortFingerprintIsNotUsed(t *testing.T) {
	d := testDeps(t)
	d.Alerts.(*fakeAlerts).alerts = []alertmanager.Alert{
		{Fingerprint: "a1b2c3d4e5f6", Labels: map[string]string{"alertname": "Other"}},
	}
	_, out, _ := d.infraAlert(context.Background(), nil, alertInput{Ref: "a1b2"})
	result := resultOf(t, out)
	if !strings.Contains(result.Speech, "не найден") {
		t.Fatalf("speech = %q", result.Speech)
	}
	detail := result.Details.(alertQueryDetail)
	if detail.Count != 0 || len(detail.Alerts) != 0 {
		t.Fatalf("detail = %+v", detail)
	}
}

func TestInfraAlertNoMatch(t *testing.T) {
	d := testDeps(t)
	_, out, _ := d.infraAlert(context.Background(), nil, alertInput{Ref: "unknown-alert"})
	if !strings.Contains(resultOf(t, out).Speech, "не найден") {
		t.Fatal("missing alert not reported")
	}
}

func TestInfraAlertMatchesEndpointLabelForExternalService(t *testing.T) {
	d := testDeps(t)
	d.Alerts.(*fakeAlerts).alerts = []alertmanager.Alert{{
		Fingerprint: "beef12345678",
		Labels:      map[string]string{"alertname": "EndpointDown", "endpoint": "ha"},
		Annotations: map[string]string{"summary": "Home Assistant недоступен"},
	}}
	_, out, err := d.infraAlert(context.Background(), nil, alertInput{Ref: "beef12"})
	if err != nil {
		t.Fatalf("infraAlert: %v", err)
	}
	result := resultOf(t, out)
	detail := result.Details.(alertQueryDetail)
	if len(detail.Services) == 0 || detail.Services[0].Service.ID != "ha" {
		t.Fatalf("services = %+v", detail.Services)
	}
	if !strings.Contains(result.Speech, "ha") {
		t.Fatalf("speech = %q", result.Speech)
	}
}

func TestInfraAlertMultipleMatches(t *testing.T) {
	d := testDeps(t)
	d.Alerts.(*fakeAlerts).alerts = []alertmanager.Alert{
		{Fingerprint: "111111111111", Labels: map[string]string{"alertname": "Dup", "namespace": "home"}},
		{Fingerprint: "222222222222", Labels: map[string]string{"alertname": "Dup", "namespace": "home"}},
	}
	_, out, _ := d.infraAlert(context.Background(), nil, alertInput{Ref: "dup"})
	result := resultOf(t, out)
	detail := result.Details.(alertQueryDetail)
	if detail.Count != 2 || len(detail.Alerts) != 2 {
		t.Fatalf("detail = %+v", detail)
	}
	if !strings.Contains(result.Speech, "2") {
		t.Fatalf("speech = %q", result.Speech)
	}
}

func TestInfraAlertErrors(t *testing.T) {
	d := testDeps(t)
	if _, out, _ := d.infraAlert(context.Background(), nil, alertInput{Ref: ""}); !strings.Contains(resultOf(t, out).Speech, "Нужен") {
		t.Fatal("empty ref not rejected")
	}

	d.Alerts = nil
	if _, out, _ := d.infraAlert(context.Background(), nil, alertInput{Ref: "x"}); !strings.Contains(resultOf(t, out).Speech, "не настроен") {
		t.Fatal("unconfigured alertmanager not reported")
	}

	d = testDeps(t)
	d.Alerts.(*fakeAlerts).err = errors.New("alertmanager down")
	if _, out, _ := d.infraAlert(context.Background(), nil, alertInput{Ref: "x"}); !strings.Contains(resultOf(t, out).Speech, "Не удалось") {
		t.Fatal("backend error not reported")
	}
}

func TestInfraStatusIncludesExternalChecks(t *testing.T) {
	d := testDeps(t)
	kube := d.Kube.(*fakeKube)
	kube.workloads[kubeKey("home", "Deployment", "scheduler")] = healthyScheduler()
	kube.workloads[kubeKey("home", "Deployment", "hermes")] = healthyScheduler()
	kube.workloads[kubeKey("home", "StatefulSet", "readonly")] = healthyScheduler()
	d.Checks = &fakeChecks{byService: map[string][]checks.Result{
		"ha": {{Service: "ha", Type: "tcp", Label: "HA API", OK: false, Error: "connection refused"}},
	}}

	_, out, err := d.infraStatus(context.Background(), nil, struct{}{})
	if err != nil {
		t.Fatalf("infraStatus: %v", err)
	}
	result := resultOf(t, out)
	detail := result.Details.(statusDetail)
	if len(detail.ExternalChecks) != 1 || detail.ExternalChecks[0].OK {
		t.Fatalf("checks = %+v", detail.ExternalChecks)
	}
	if !detail.Degraded || !strings.Contains(result.Speech, "Проблемы") {
		t.Fatalf("degraded=%v speech=%q", detail.Degraded, result.Speech)
	}
}

func TestReadGroupRegistersAllTools(t *testing.T) {
	d := testDeps(t)
	server := mcp.NewServer(&mcp.Implementation{Name: "tools-test", Version: "0"}, nil)
	(&ReadGroup{Deps: d}).Register(server)

	clientTransport, serverTransport := mcp.NewInMemoryTransports()
	if _, err := server.Connect(context.Background(), serverTransport, nil); err != nil {
		t.Fatalf("server connect: %v", err)
	}
	client := mcp.NewClient(&mcp.Implementation{Name: "test-client", Version: "0"}, nil)
	session, err := client.Connect(context.Background(), clientTransport, nil)
	if err != nil {
		t.Fatalf("client connect: %v", err)
	}
	defer func() { _ = session.Close() }()

	tools, err := session.ListTools(context.Background(), nil)
	if err != nil {
		t.Fatalf("ListTools: %v", err)
	}
	got := map[string]bool{}
	for _, tool := range tools.Tools {
		got[tool.Name] = true
	}
	for _, want := range []string{"infra_status", "infra_service", "infra_alert", "infra_logs", "infra_query", "infra_overview", "infra_report"} {
		if !got[want] {
			t.Errorf("tool %q not registered (got %v)", want, got)
		}
	}

	resources, err := session.ListResources(context.Background(), nil)
	if err != nil {
		t.Fatalf("ListResources: %v", err)
	}
	found := false
	for _, resource := range resources.Resources {
		if resource.URI == "infra://catalog" {
			found = true
		}
	}
	if !found {
		t.Fatal("infra://catalog resource not registered")
	}
}

func TestSpeechNormalization(t *testing.T) {
	long := strings.Repeat("слово ", 60)
	got := Speech("**Жирный** текст\nс переносом https://example.com/x")
	if strings.Contains(got, "*") || strings.Contains(got, "https://") {
		t.Fatalf("speech = %q", got)
	}
	got = Speech(long)
	if len([]rune(got)) > MaxSpeechRunes {
		t.Fatalf("speech = %d runes", len([]rune(got)))
	}
}
