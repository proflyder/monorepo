package tools

import (
	"context"
	"io"
	"log/slog"
	"net/http"
	"strings"
	"testing"
	"time"
	_ "time/tzdata" // report fixtures pin the Asia/Almaty header

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/proflyder/monorepo/packages/go/contracttest"
	"github.com/proflyder/monorepo/solutions/mcp-gateway/internal/agentmode"
	"github.com/proflyder/monorepo/solutions/mcp-gateway/internal/alertmanager"
	"github.com/proflyder/monorepo/solutions/mcp-gateway/internal/bb"
	"github.com/proflyder/monorepo/solutions/mcp-gateway/internal/catalog"
	"github.com/proflyder/monorepo/solutions/mcp-gateway/internal/checks"
	"github.com/proflyder/monorepo/solutions/mcp-gateway/internal/group"
	"github.com/proflyder/monorepo/solutions/mcp-gateway/internal/kubernetes"
	"github.com/proflyder/monorepo/solutions/mcp-gateway/internal/loki"
	"github.com/proflyder/monorepo/solutions/mcp-gateway/internal/metrics"
	"github.com/proflyder/monorepo/solutions/mcp-gateway/internal/pending"
	"github.com/proflyder/monorepo/solutions/mcp-gateway/internal/prometheus"
	"github.com/proflyder/monorepo/solutions/mcp-gateway/internal/server"
)

// contractCatalog is the deployment data of every tool-call fixture. It must
// stay language-neutral: a Rust implementation loads the same YAML.
const contractCatalog = `
overview: |
  ## Фикстурная топология
  Один кластер и один внешний узел.
ignore_alerts: [Watchdog]
group_order: [Голос, Дом, Мониторинг]
services:
  - id: scheduler
    title: Планировщик
    group: Мониторинг
    aliases:
      ru: [планировщик, будильники]
      en: [scheduler]
    kind: k8s
    namespace: home
    workload: {kind: Deployment, name: scheduler}
    containers: [scheduler]
    signals:
      - promql: 'scheduler_up{endpoint="scheduler"} == 0'
        meaning: планировщик недоступен
        status: true
    logs: {source: pods, containers: [scheduler]}
    restart:
      allowed: true
      warning: Будильники в ближайшие минуты могут не сработать.
  - id: hermes
    title: Гермес
    group: Голос
    aliases:
      ru: [гермес, ассистент]
      en: [hermes]
    kind: k8s
    namespace: home
    workload: {kind: Deployment, name: hermes}
    containers: [hermes, voice-proxy]
    signals:
      - promql: 'hermes_up{endpoint="hermes"} == 0'
        meaning: гермес недоступен
    logs: {source: loki, selector: '{namespace="home"}'}
    restart:
      allowed: true
      self_disrupting: true
      warning: Текущий разговор прервётся.
  - id: ha
    title: Home Assistant
    group: Дом
    host_title: HAOS VM
    aliases:
      ru: [дом, ха]
      en: [ha]
    kind: external
    signals:
      - promql: 'ha_up{endpoint="ha"} == 0'
        meaning: home assistant недоступен
        status: true
    checks:
      - {type: tcp, label: HA API, host: 192.0.2.28, port: 8123}
      - {type: ha_template, label: Pulsar, template: "{{ states('sensor.pulsar_version') }}"}
    restart: {allowed: false}
  - id: readonly
    title: Readonly
    group: Мониторинг
    aliases:
      en: [readonly]
    kind: k8s
    namespace: home
    workload: {kind: StatefulSet, name: readonly}
    restart: {allowed: false}
`

const contractTargets = `
targets:
  - project: proj_personal
    aliases: ["стан лэб", "station lab"]
    provider: claude-code
    title_prefix: "Голос"
  - project: proj_work
    aliases: ["работа"]
`

// contractClock is the single fake clock of the fixture replay. Tests are
// sequential, so one process-wide value is enough and lets one step expire a
// pending action without racing another scenario.
var contractClock = time.Date(2026, 11, 1, 12, 0, 0, 0, time.UTC)

func contractNow() time.Time { return contractClock }

// contractDeps builds the fake world of one scenario file.
func contractDeps(t *testing.T, scenario string) *Deps {
	t.Helper()
	cat, err := catalog.Parse([]byte(contractCatalog))
	if err != nil {
		t.Fatalf("catalog: %v", err)
	}
	resolver, err := bb.ParseTargets([]byte(contractTargets))
	if err != nil {
		t.Fatalf("targets: %v", err)
	}
	location, err := time.LoadLocation("Asia/Almaty")
	if err != nil {
		t.Fatalf("location: %v", err)
	}
	contractClock = time.Date(2026, 11, 1, 12, 0, 0, 0, time.UTC)
	d := &Deps{
		Catalog:       cat,
		Prom:          &fakeProm{results: map[string][]prometheus.Sample{}},
		Alerts:        &fakeAlerts{},
		Loki:          &fakeLoki{},
		Kube:          newFakeKube(),
		Report:        &fakeReport{},
		Pending:       pending.New(90*time.Second, contractNow),
		Timers:        &fakeTimers{},
		Metrics:       metrics.New(),
		Now:           contractNow,
		Location:      location,
		Cooldown:      5 * time.Minute,
		QueryTimeout:  time.Second,
		StatusTimeout: 2 * time.Second,
		ReportTimeout: 2 * time.Second,
		LogByteCap:    64 << 10,
		BB:            &contractBB{},
		BBTargets:     resolver,
		BBPending:     pending.NewKeyed[bb.SpawnAction](90*time.Second, contractNow),
		BBMode:        agentmode.New(),
	}
	seedContract(t, scenario, d)
	return d
}

type contractBB struct {
	// fields let the seed functions control one scenario.
	targets     bb.Targets
	status      bb.StatusResult
	statusErr   error
	validateErr error
	spawnErr    error
	spawnResult bb.SpawnResult
}

func (f *contractBB) Targets(context.Context) (bb.Targets, error) {
	if f.targets.Projects == nil {
		return contractTargetsValue(), nil
	}
	return f.targets, nil
}

func (f *contractBB) Status(context.Context, int) (bb.StatusResult, error) {
	return f.status, f.statusErr
}

func (f *contractBB) ValidateSpawn(context.Context, bb.SpawnRequest) error { return f.validateErr }

func (f *contractBB) Spawn(context.Context, bb.SpawnRequest) (bb.SpawnResult, error) {
	if f.spawnErr != nil {
		return bb.SpawnResult{}, f.spawnErr
	}
	return f.spawnResult, nil
}

func (f *contractBB) Send(_ context.Context, req bb.SendRequest) (bb.SendResult, error) {
	return bb.SendResult{ThreadID: req.ThreadID}, nil
}

func (f *contractBB) TurnResult(context.Context, bb.TurnResultRequest) (bb.TurnResultResult, error) {
	return bb.TurnResultResult{Status: "idle"}, nil
}

func (f *contractBB) Updates(context.Context, bb.UpdatesRequest) (bb.UpdatesResult, error) {
	return bb.UpdatesResult{}, nil
}

func contractTargetsValue() bb.Targets {
	return bb.Targets{Projects: []bb.Target{
		{
			ProjectID: "proj_personal",
			Name:      "Station Lab",
			Profile:   "personal",
			Providers: []bb.Provider{{
				ProviderID: "claude-code",
				Models: []bb.Model{{
					Model:                 "sonnet",
					DefaultReasoningLevel: "medium",
					ReasoningLevels:       []string{"low", "medium", "high"},
				}},
			}},
		},
		{
			ProjectID: "proj_work",
			Name:      "Work",
			Profile:   "work",
			Providers: []bb.Provider{{
				ProviderID: "codex",
				Models:     []bb.Model{{Model: "gpt", DefaultReasoningLevel: "low"}},
			}},
		},
	}}
}

func fakeKubeOf(d *Deps) *fakeKube { return d.Kube.(*fakeKube) }

func readyWorkload(namespace, kind, name string) kubernetes.Workload {
	return kubernetes.Workload{
		Namespace: namespace, Kind: kind, Name: name,
		Desired: 1, Ready: 1, Available: 1, Updated: 1,
		Generation: 2, ObservedGeneration: 2,
		Selector: map[string]string{"app.kubernetes.io/name": name},
	}
}

// seedContract fills the backends according to the fixture file name.
func seedContract(t *testing.T, scenario string, d *Deps) {
	t.Helper()
	kube := fakeKubeOf(d)
	kube.workloads[kubeKey("home", "Deployment", "scheduler")] = readyWorkload("home", "Deployment", "scheduler")
	kube.workloads[kubeKey("home", "Deployment", "hermes")] = readyWorkload("home", "Deployment", "hermes")
	kube.workloads[kubeKey("home", "StatefulSet", "readonly")] = readyWorkload("home", "StatefulSet", "readonly")
	kube.pods["home"] = []kubernetes.Pod{
		{
			Namespace: "home", Name: "scheduler-1", Ready: true,
			StartTime: time.Date(2026, 10, 30, 12, 0, 0, 0, time.UTC),
			Containers: []kubernetes.ContainerStatus{{
				Name: "scheduler", Image: "ghcr.io/proflyder/scheduler:1.2.3", Ready: true,
			}},
		},
		{
			Namespace: "home", Name: "hermes-7d9f8c6b5-x2x4z", Ready: true,
			StartTime: time.Date(2026, 10, 31, 12, 0, 0, 0, time.UTC),
			Containers: []kubernetes.ContainerStatus{
				{Name: "hermes", Image: "ghcr.io/proflyder/hermes:2.0.0", Ready: true},
				{Name: "voice-proxy", Image: "ghcr.io/proflyder/voice-proxy:1.1.0", Ready: true},
			},
		},
	}
	kube.logs[kubeKey("home", "scheduler-1", "scheduler")] = "tick ok\nERROR alarm not fired\ntick ok\n"
	kube.events["home/scheduler"] = []kubernetes.Event{{
		Reason: "Pulled", Message: "Image pulled", LastSeen: contractNow().Add(-time.Hour),
	}}
	d.Checks = &fakeChecks{byService: map[string][]checks.Result{
		"ha": {
			{Service: "ha", Type: catalog.CheckTCP, Label: "HA API", OK: true, Value: "192.0.2.28:8123"},
			{Service: "ha", Type: catalog.CheckHATemplate, Label: "Pulsar", OK: true, Value: "1.2.3"},
		},
	}}
	d.Loki.(*fakeLoki).lines = []loki.Line{
		{Timestamp: contractNow(), Text: "hermes started"},
		{Timestamp: contractNow(), Text: "Authorization: Bearer supersecrettoken123"},
	}

	switch {
	case strings.HasPrefix(scenario, "tools_call_bb_status"):
		bbClient := d.BB.(*contractBB)
		bbClient.status = bb.StatusResult{Threads: []bb.Thread{
			{ID: "thr_personal", Title: "Личный тред", Project: "Station Lab", ProjectID: "proj_personal", Profile: "personal", Status: "running"},
			{ID: "thr_work", Title: "Рабочий тред", Project: "Work", ProjectID: "proj_work", Profile: "work", Status: "running"},
		}}
	case strings.HasPrefix(scenario, "tools_call_bb_confirm"):
		bbClient := d.BB.(*contractBB)
		bbClient.spawnResult = bb.SpawnResult{ThreadID: "thr_new", Idempotent: false}
	case strings.HasPrefix(scenario, "tools_call_bb_prepare"):
		// default targets are enough
	case strings.HasPrefix(scenario, "tools_call_infra_prepare"):
		// Cooldown is exercised on a second prepare after the seeded annotation.
	case strings.HasPrefix(scenario, "tools_call_infra_confirm"):
		bbClient := d.BB.(*contractBB)
		bbClient.spawnResult = bb.SpawnResult{ThreadID: "thr_new"}
	}
}

// mutateContract changes the fake world before a specific step. The hook is
// the only way to change state between two calls of one fixture file.
func mutateContract(scenario string, step int, d *Deps) {
	kube := fakeKubeOf(d)
	switch scenario {
	case "tools_call_infra_status":
		switch step {
		case 4: // alerts, including an ignored one
			d.Alerts.(*fakeAlerts).alerts = []alertmanager.Alert{
				{
					Fingerprint: "abcdef123456", Labels: map[string]string{"alertname": "SchedulerDown", "severity": "critical", "deployment": "scheduler"},
					Annotations: map[string]string{"summary": "Планировщик недоступен"}, StartsAt: contractNow().Add(-10 * time.Minute),
				},
				{
					Fingerprint: "watchdog01", Labels: map[string]string{"alertname": "Watchdog", "severity": "none"},
					Annotations: map[string]string{"summary": "Watchdog"}, StartsAt: contractNow().Add(-time.Hour),
				},
			}
		case 5: // a firing catalog signal
			d.Alerts.(*fakeAlerts).alerts = nil
			d.Prom.(*fakeProm).results[`scheduler_up{endpoint="scheduler"} == 0`] = []prometheus.Sample{{Value: 1}}
		case 6: // rollout in progress and a pod restart
			d.Prom.(*fakeProm).results = map[string][]prometheus.Sample{}
			kube.workloads[kubeKey("home", "Deployment", "scheduler")] = kubernetes.Workload{
				Namespace: "home", Kind: "Deployment", Name: "scheduler",
				Desired: 1, Ready: 0, Available: 0, Updated: 1, Generation: 3, ObservedGeneration: 2,
			}
			kube.pods["home"] = []kubernetes.Pod{{
				Namespace: "home", Name: "scheduler-2", Ready: false,
				Containers: []kubernetes.ContainerStatus{{
					Name: "scheduler", RestartCount: 2, WaitingReason: "CrashLoopBackOff",
					LastTerminatedAt: contractNow().Add(-5 * time.Minute),
				}},
			}}
		case 7: // Prometheus and Alertmanager are not configured
			d.Prom = nil
			d.Alerts = nil
			kube.workloads[kubeKey("home", "Deployment", "scheduler")] = readyWorkload("home", "Deployment", "scheduler")
			kube.pods["home"] = nil
		case 8: // a failed and a skipped external check
			d.Checks = &fakeChecks{byService: map[string][]checks.Result{
				"ha": {
					{Service: "ha", Type: catalog.CheckTCP, Label: "HA API", OK: false, Error: "connection refused"},
					{Service: "ha", Type: catalog.CheckHATemplate, Label: "Pulsar", OK: true, Skipped: true, Value: "Home Assistant не настроен"},
				},
			}}
		}
	case "tools_call_infra_service":
		switch step {
		case 4: // external service
			// nothing to change
		case 7: // Kubernetes API error
			kube.errs[kubeKey("home", "Deployment", "scheduler")] = io.ErrUnexpectedEOF
		}
	case "tools_call_infra_alert":
		if step == 3 {
			d.Alerts.(*fakeAlerts).alerts = contractAlerts()
		}
	case "tools_call_infra_logs":
		switch step {
		case 5: // byte-cap truncation in the middle of a rune
			d.LogByteCap = 9
			d.Loki.(*fakeLoki).lines = []loki.Line{{Text: "абвгдеж"}}
		case 6: // pods/log fallback
			d.LogByteCap = 64 << 10
		case 7: // no pods
			kube.pods["home"] = nil
		case 8: // loki error
			d.Loki.(*fakeLoki).err = io.ErrUnexpectedEOF
		}
	case "tools_call_infra_query":
		switch step {
		case 4: // empty result
			d.Prom.(*fakeProm).results = map[string][]prometheus.Sample{}
		case 5: // backend error
			d.Prom.(*fakeProm).errs = map[string]error{"up": io.ErrUnexpectedEOF}
		}
	case "tools_call_infra_report":
		switch step {
		case 4: // problems
			kube.workloads[kubeKey("home", "Deployment", "scheduler")] = kubernetes.Workload{
				Namespace: "home", Kind: "Deployment", Name: "scheduler",
				Desired: 1, Ready: 0, Available: 0, Updated: 1, Generation: 3, ObservedGeneration: 2,
			}
			d.Checks.(*fakeChecks).byService["ha"] = []checks.Result{{
				Service: "ha", Type: catalog.CheckTCP, Label: "HA API", OK: false, Error: "connection refused",
			}}
		case 5: // verbose, healthy again
			kube.workloads[kubeKey("home", "Deployment", "scheduler")] = readyWorkload("home", "Deployment", "scheduler")
			d.Checks = &fakeChecks{byService: map[string][]checks.Result{
				"ha": {
					{Service: "ha", Type: catalog.CheckTCP, Label: "HA API", OK: true, Value: "192.0.2.28:8123"},
					{Service: "ha", Type: catalog.CheckHATemplate, Label: "Pulsar", OK: true, Value: "1.2.3"},
				},
			}}
		case 6: // Alertmanager unavailable
			d.Alerts = nil
		}
	case "tools_call_infra_prepare_restart":
		switch step {
		case 5: // rollout in progress
			kube.workloads[kubeKey("home", "Deployment", "scheduler")] = kubernetes.Workload{
				Namespace: "home", Kind: "Deployment", Name: "scheduler",
				Desired: 1, Ready: 1, Available: 1, Updated: 1, Generation: 3, ObservedGeneration: 2,
			}
		case 6: // cooldown annotation
			kube.workloads[kubeKey("home", "Deployment", "scheduler")] = kubernetes.Workload{
				Namespace: "home", Kind: "Deployment", Name: "scheduler",
				Desired: 1, Ready: 1, Available: 1, Updated: 1, Generation: 2, ObservedGeneration: 2,
				Annotations: map[string]string{"mcp-gateway/last-restart": contractNow().Add(-time.Minute).Format(time.RFC3339)},
			}
		case 7: // no annotation and no rollout
			kube.workloads[kubeKey("home", "Deployment", "scheduler")] = readyWorkload("home", "Deployment", "scheduler")
		}
	case "tools_call_infra_confirm":
		switch step {
		case 10: // the pending action from step 9 is now older than the TTL
			contractClock = contractClock.Add(3 * time.Minute)
		}
	case "tools_call_bb_status":
		switch step {
		case 4:
			d.BB.(*contractBB).status = bb.StatusResult{}
		case 5:
			d.BB.(*contractBB).statusErr = &bb.Error{Code: "bridge_error", Speech: bb.SpeechUnavailable}
		}
	case "tools_call_bb_prepare_spawn":
		switch step {
		case 9: // validate_spawn refuses
			d.BB.(*contractBB).validateErr = &bb.Error{Code: "thread_limit", Speech: "Лимит тредов."}
		case 10: // no bridge at all
			d.BB = nil
		}
	}
}

func contractAlerts() []alertmanager.Alert {
	return []alertmanager.Alert{
		{
			Fingerprint: "abcdef123456", Labels: map[string]string{
				"alertname": "SchedulerDown", "severity": "critical", "deployment": "scheduler", "namespace": "home",
			},
			Annotations: map[string]string{"summary": "Планировщик недоступен", "description": "Нет метрик 5 минут"},
			StartsAt:    contractNow().Add(-10 * time.Minute),
		},
		{
			Fingerprint: "abcdef999999", Labels: map[string]string{
				"alertname": "SchedulerDown", "severity": "warning", "deployment": "scheduler", "namespace": "home",
			},
			Annotations: map[string]string{"summary": "Планировщик недоступен"},
			StartsAt:    contractNow().Add(-5 * time.Minute),
		},
		{
			Fingerprint: "fedcba654321", Labels: map[string]string{
				"alertname": "HermesHighLatency", "severity": "warning", "job": "hermes", "namespace": "home",
			},
			Annotations: map[string]string{"summary": "Гермес тормозит"},
			StartsAt:    contractNow().Add(-3 * time.Minute),
		},
	}
}

// newContractHandler builds the real HTTP surface (server + MCP) around the
// fake backends. It is shared by every tool-call fixture file.
func newContractHandler(t *testing.T, scenario string, deps **Deps) http.Handler {
	t.Helper()
	d := contractDeps(t, scenario)
	*deps = d

	mcpServer := mcp.NewServer(&mcp.Implementation{
		Name:    "mcp-gateway",
		Title:   "Infra gateway",
		Version: "contract",
	}, &mcp.ServerOptions{Logger: discardLogger()})
	registry := group.NewRegistry(
		&ReadGroup{Deps: d},
		&FixGroup{Deps: d},
		&ReportGroup{Deps: d},
		&BBReadGroup{Deps: d},
		&BBActGroup{Deps: d},
	)
	selected, err := registry.Select(contractGroups(scenario))
	if err != nil {
		t.Fatalf("select groups: %v", err)
	}
	registry.RegisterAll(mcpServer, selected)
	stream := mcp.NewStreamableHTTPHandler(
		func(*http.Request) *mcp.Server { return mcpServer },
		&mcp.StreamableHTTPOptions{Logger: discardLogger()},
	)
	return server.New(server.Options{
		AuthToken:  "mcp-contract-token",
		VoiceToken: "voice-contract-token",
		MCP:        stream,
		Ready:      func(context.Context) error { return nil },
		Metrics:    metrics.New(),
		Logger:     discardLogger(),
	}).Handler()
}

func contractGroups(scenario string) []string {
	switch scenario {
	case "mcp_tools_list_read":
		return []string{GroupRead}
	case "mcp_tools_list_bb_read":
		return []string{GroupBBRead}
	case "mcp_tools_list_all":
		return []string{GroupRead, GroupFix, GroupReport, GroupBBRead, GroupBBAct}
	default:
		if strings.HasPrefix(scenario, "tools_call_bb_") {
			return []string{GroupRead, GroupBBRead, GroupBBAct}
		}
		return []string{GroupRead, GroupFix, GroupReport}
	}
}

func discardLogger() *slog.Logger { return slog.New(slog.NewTextHandler(io.Discard, nil)) }

// TestToolContractFixtures replays every tool-level MCP fixture: tools/list
// per group set and tools/call for every tool with fake backends.
func TestToolContractFixtures(t *testing.T) {
	var deps *Deps
	contracttest.Run(t,
		"solutions/mcp-gateway/internal/tools/testdata/contract",
		func(t *testing.T, scenario string) http.Handler {
			return newContractHandler(t, scenario, &deps)
		},
		contracttest.WithBeforeStep(func(t *testing.T, scenario string, step int) {
			if deps != nil {
				mutateContract(scenario, step, deps)
			}
		}),
		contracttest.WithNormalizer(dropSDKFields),
	)
}

// dropSDKFields removes fields that only the Go MCP SDK emits, so -update
// records a language-neutral expectation. Semantic comparison accepts the
// fields when the implementation still sends them.
func dropSDKFields(_ string, value any) any {
	switch typed := value.(type) {
	case map[string]any:
		for _, key := range []string{"ttlMs", "cacheScope", "structuredContent"} {
			delete(typed, key)
		}
		if idempotent, ok := typed["idempotentHint"].(bool); ok && !idempotent {
			delete(typed, "idempotentHint")
		}
		for key, item := range typed {
			typed[key] = dropSDKFields("", item)
		}
		return typed
	case []any:
		for index, item := range typed {
			typed[index] = dropSDKFields("", item)
		}
		return typed
	default:
		return value
	}
}
