package tools

import (
	"context"
	"fmt"
	"sync"
	"testing"
	"time"

	"ricardo.local/monorepo/solutions/mcp-gateway/internal/alertmanager"
	"ricardo.local/monorepo/solutions/mcp-gateway/internal/catalog"
	"ricardo.local/monorepo/solutions/mcp-gateway/internal/checks"
	"ricardo.local/monorepo/solutions/mcp-gateway/internal/kubernetes"
	"ricardo.local/monorepo/solutions/mcp-gateway/internal/loki"
	"ricardo.local/monorepo/solutions/mcp-gateway/internal/metrics"
	"ricardo.local/monorepo/solutions/mcp-gateway/internal/pending"
	"ricardo.local/monorepo/solutions/mcp-gateway/internal/prometheus"
)

const testCatalog = `
overview: |
  ## Тестовая топология
  Два узла и один кластер.
ignore_alerts: [Watchdog, InfoInhibitor]
services:
  - id: scheduler
    aliases:
      ru: [планировщик, будильники]
      en: [scheduler]
    kind: k8s
    namespace: home
    workload: {kind: Deployment, name: scheduler}
    containers: [scheduler]
    signals:
      - promql: ricardo_endpoint_up{endpoint="scheduler"} == 0
        meaning: планировщик недоступен
        status: true
    logs: {source: pods, containers: [scheduler]}
    restart:
      allowed: true
      warning: Будильники в ближайшие минуты могут не сработать.
  - id: hermes
    aliases:
      ru: [гермес, ассистент]
      en: [hermes]
    kind: k8s
    namespace: home
    workload: {kind: Deployment, name: hermes}
    containers: [hermes, voice-proxy]
    signals:
      - promql: ricardo_endpoint_up{endpoint="hermes"} == 0
        meaning: гермес недоступен
    logs: {source: loki, selector: '{namespace="home"}'}
    restart:
      allowed: true
      self_disrupting: true
      warning: Текущий разговор прервётся.
  - id: ha
    aliases:
      ru: [дом, home assistant]
      en: [ha]
    kind: external
    signals:
      - promql: ricardo_endpoint_up{endpoint="ha"} == 0
        meaning: home assistant недоступен
        status: true
    checks:
      - {type: tcp, label: HA API, host: 192.0.2.28, port: 8123}
    restart: {allowed: false}
  - id: readonly
    aliases:
      en: [readonly]
    kind: k8s
    namespace: home
    workload: {kind: StatefulSet, name: readonly}
    restart: {allowed: false}
`

func testDeps(t *testing.T) *Deps {
	t.Helper()
	cat, err := catalog.Parse([]byte(testCatalog))
	if err != nil {
		t.Fatalf("catalog: %v", err)
	}
	now := time.Date(2026, 11, 1, 12, 0, 0, 0, time.UTC)
	return &Deps{
		Catalog:       cat,
		Prom:          &fakeProm{},
		Alerts:        &fakeAlerts{},
		Loki:          &fakeLoki{},
		Kube:          newFakeKube(),
		Report:        &fakeReport{},
		Pending:       pending.New(90*time.Second, func() time.Time { return now }),
		Timers:        &fakeTimers{},
		Metrics:       metrics.New(),
		Now:           func() time.Time { return now },
		Cooldown:      5 * time.Minute,
		QueryTimeout:  time.Second,
		StatusTimeout: 2 * time.Second,
		LogByteCap:    64 << 10,
	}
}

type fakeProm struct {
	mu      sync.Mutex
	results map[string][]prometheus.Sample
	errs    map[string]error
	block   bool
	queries []string
}

func (f *fakeProm) Query(ctx context.Context, promql string, _ int) ([]prometheus.Sample, error) {
	f.mu.Lock()
	f.queries = append(f.queries, promql)
	err := f.errs[promql]
	block := f.block
	result := f.results[promql]
	f.mu.Unlock()
	if block {
		<-ctx.Done()
		return nil, ctx.Err()
	}
	if err != nil {
		return nil, err
	}
	return result, nil
}

type fakeAlerts struct {
	mu     sync.Mutex
	alerts []alertmanager.Alert
	err    error
}

func (f *fakeAlerts) Active(context.Context, int) ([]alertmanager.Alert, error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	return append([]alertmanager.Alert(nil), f.alerts...), f.err
}

type fakeLoki struct {
	lines     []loki.Line
	err       error
	selectors []string
}

func (f *fakeLoki) Query(_ context.Context, selector string, _ time.Duration, limit int) ([]loki.Line, error) {
	f.selectors = append(f.selectors, selector)
	if f.err != nil {
		return nil, f.err
	}
	if limit > 0 && len(f.lines) > limit {
		return f.lines[:limit], nil
	}
	return f.lines, nil
}

type fakeKube struct {
	mu        sync.Mutex
	workloads map[string]kubernetes.Workload
	pods      map[string][]kubernetes.Pod
	events    map[string][]kubernetes.Event
	logs      map[string]string
	patches   []patchCall
	errs      map[string]error
}

type patchCall struct {
	namespace string
	kind      string
	name      string
	at        time.Time
}

func newFakeKube() *fakeKube {
	return &fakeKube{
		workloads: map[string]kubernetes.Workload{},
		pods:      map[string][]kubernetes.Pod{},
		events:    map[string][]kubernetes.Event{},
		logs:      map[string]string{},
		errs:      map[string]error{},
	}
}

func kubeKey(namespace, kind, name string) string {
	return namespace + "/" + kind + "/" + name
}

func (f *fakeKube) GetWorkload(_ context.Context, namespace, kind, name string) (kubernetes.Workload, error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	key := kubeKey(namespace, kind, name)
	if err := f.errs[key]; err != nil {
		return kubernetes.Workload{}, err
	}
	w, ok := f.workloads[key]
	if !ok {
		return kubernetes.Workload{}, fmt.Errorf("workload not found")
	}
	return w, nil
}

func (f *fakeKube) ListPods(_ context.Context, namespace string, _ map[string]string) ([]kubernetes.Pod, error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	if err := f.errs["pods/"+namespace]; err != nil {
		return nil, err
	}
	return append([]kubernetes.Pod(nil), f.pods[namespace]...), nil
}

func (f *fakeKube) ListEvents(_ context.Context, namespace, objectName string, _ int) ([]kubernetes.Event, error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	return append([]kubernetes.Event(nil), f.events[namespace+"/"+objectName]...), nil
}

func (f *fakeKube) PodLogs(_ context.Context, namespace, pod, container string, _ int, _ time.Duration) (string, error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	return f.logs[kubeKey(namespace, pod, container)], nil
}

func (f *fakeKube) PatchRestart(_ context.Context, namespace, kind, name string, at time.Time) error {
	f.mu.Lock()
	defer f.mu.Unlock()
	if err := f.errs["patch"]; err != nil {
		return err
	}
	f.patches = append(f.patches, patchCall{namespace: namespace, kind: kind, name: name, at: at})
	return nil
}

type fakeReport struct {
	title string
	text  string
	err   error
}

func (f *fakeReport) Send(_ context.Context, title, text string) error {
	f.title, f.text = title, text
	return f.err
}

type fakeChecks struct {
	byService map[string][]checks.Result
}

func (f *fakeChecks) RunAll(_ context.Context, services []catalog.Service) []checks.Result {
	var out []checks.Result
	for _, service := range services {
		out = append(out, f.byService[service.ID]...)
	}
	return out
}

type fakeTimers struct {
	delays []time.Duration
	funcs  []func()
}

func (f *fakeTimers) AfterFunc(d time.Duration, fn func()) {
	f.delays = append(f.delays, d)
	f.funcs = append(f.funcs, fn)
}

func (f *fakeTimers) runAll() {
	for _, fn := range f.funcs {
		fn()
	}
}

func resultOf(t *testing.T, out any) Result {
	t.Helper()
	result, ok := out.(Result)
	if !ok {
		t.Fatalf("output = %T, want Result", out)
	}
	if result.Speech == "" {
		t.Fatal("empty speech")
	}
	if len([]rune(result.Speech)) > MaxSpeechRunes {
		t.Fatalf("speech has %d runes: %q", len([]rune(result.Speech)), result.Speech)
	}
	return result
}
