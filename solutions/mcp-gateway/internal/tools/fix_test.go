package tools

import (
	"context"
	"errors"
	"strings"
	"testing"
	"time"

	"ricardo.local/monorepo/solutions/mcp-gateway/internal/kubernetes"
	"ricardo.local/monorepo/solutions/mcp-gateway/internal/pending"
)

func prepareScheduler(t *testing.T, d *Deps) (string, prepareDetail, *fakeKube, *fakeTimers) {
	t.Helper()
	kube := d.Kube.(*fakeKube)
	kube.workloads[kubeKey("home", "Deployment", "scheduler")] = healthyScheduler()
	timers, _ := d.Timers.(*fakeTimers)
	res, out, err := d.infraPrepareRestart(context.Background(), nil, restartInput{Name: "планировщик"})
	if err != nil || res == nil {
		t.Fatalf("prepare: %v", err)
	}
	result := resultOf(t, out)
	detail, ok := result.Details.(prepareDetail)
	if !ok {
		t.Fatalf("details = %T", result.Details)
	}
	return result.Speech, detail, kube, timers
}

func TestPrepareRestartAllowed(t *testing.T) {
	d := testDeps(t)
	speech, detail, _, _ := prepareScheduler(t, d)
	if !detail.Allowed {
		t.Fatalf("detail = %+v", detail)
	}
	if !strings.Contains(speech, "?") {
		t.Fatalf("speech is not a question: %q", speech)
	}
	if !strings.Contains(speech, "Будильники в ближайшие минуты") {
		t.Fatalf("warning missing: %q", speech)
	}
	action, ok := d.Pending.Peek()
	if !ok || action.ServiceID != "scheduler" || action.Kind != ActionRolloutRestart {
		t.Fatalf("pending = %+v, %v", action, ok)
	}
	if action.Namespace != "home" || action.WorkloadKind != "Deployment" || action.WorkloadName != "scheduler" {
		t.Fatalf("pending target = %+v", action)
	}
}

func TestPrepareRestartReplacesPrevious(t *testing.T) {
	d := testDeps(t)
	kube := d.Kube.(*fakeKube)
	kube.workloads[kubeKey("home", "Deployment", "scheduler")] = healthyScheduler()
	kube.workloads[kubeKey("home", "Deployment", "hermes")] = kubernetes.Workload{Namespace: "home", Kind: "Deployment", Name: "hermes", Desired: 1, Ready: 1, Available: 1, Updated: 1}
	prepareScheduler(t, d)
	_, _, err := d.infraPrepareRestart(context.Background(), nil, restartInput{Name: "гермес"})
	if err != nil {
		t.Fatalf("prepare hermes: %v", err)
	}
	action, ok := d.Pending.Peek()
	if !ok || action.ServiceID != "hermes" {
		t.Fatalf("pending = %+v, want hermes", action)
	}
}

func TestPrepareRestartNotAllowed(t *testing.T) {
	d := testDeps(t)
	_, out, err := d.infraPrepareRestart(context.Background(), nil, restartInput{Name: "readonly"})
	if err != nil {
		t.Fatalf("prepare: %v", err)
	}
	result := resultOf(t, out)
	if !strings.Contains(result.Speech, "запрещён") {
		t.Fatalf("speech = %q", result.Speech)
	}
	if _, ok := d.Pending.Peek(); ok {
		t.Fatal("pending action stored for a forbidden restart")
	}
}

func TestPrepareRestartExternal(t *testing.T) {
	d := testDeps(t)
	_, out, _ := d.infraPrepareRestart(context.Background(), nil, restartInput{Name: "дом"})
	result := resultOf(t, out)
	if !strings.Contains(result.Speech, "недоступен") {
		t.Fatalf("speech = %q", result.Speech)
	}
}

func TestPrepareRestartUnknownService(t *testing.T) {
	d := testDeps(t)
	_, out, _ := d.infraPrepareRestart(context.Background(), nil, restartInput{Name: "нет такого"})
	if !strings.Contains(resultOf(t, out).Speech, "Не нашёл") {
		t.Fatal("unknown service not rejected")
	}
}

func TestPrepareRestartRolloutInProgress(t *testing.T) {
	d := testDeps(t)
	kube := d.Kube.(*fakeKube)
	kube.workloads[kubeKey("home", "Deployment", "scheduler")] = kubernetes.Workload{
		Namespace: "home", Kind: "Deployment", Name: "scheduler", Desired: 1, Ready: 1, Available: 1, Updated: 0,
		Generation: 3, ObservedGeneration: 2,
	}
	_, out, _ := d.infraPrepareRestart(context.Background(), nil, restartInput{Name: "scheduler"})
	result := resultOf(t, out)
	if !strings.Contains(result.Speech, "уже обновляется") {
		t.Fatalf("speech = %q", result.Speech)
	}
}

func TestPrepareRestartCooldown(t *testing.T) {
	d := testDeps(t)
	w := healthyScheduler()
	w.Annotations = map[string]string{"mcp-gateway/last-restart": d.Now().Add(-90 * time.Second).UTC().Format(time.RFC3339)}
	d.Kube.(*fakeKube).workloads[kubeKey("home", "Deployment", "scheduler")] = w
	_, out, _ := d.infraPrepareRestart(context.Background(), nil, restartInput{Name: "scheduler"})
	result := resultOf(t, out)
	if !strings.Contains(result.Speech, "подождите") {
		t.Fatalf("speech = %q", result.Speech)
	}
	if _, ok := d.Pending.Peek(); ok {
		t.Fatal("pending action stored during cooldown")
	}
}

func TestPrepareRestartCooldownExpired(t *testing.T) {
	d := testDeps(t)
	w := healthyScheduler()
	w.Annotations = map[string]string{"mcp-gateway/last-restart": d.Now().Add(-10 * time.Minute).UTC().Format(time.RFC3339)}
	d.Kube.(*fakeKube).workloads[kubeKey("home", "Deployment", "scheduler")] = w
	_, out, err := d.infraPrepareRestart(context.Background(), nil, restartInput{Name: "scheduler"})
	if err != nil {
		t.Fatalf("prepare: %v", err)
	}
	result := resultOf(t, out)
	detail := result.Details.(prepareDetail)
	if !detail.Allowed || !strings.Contains(result.Speech, "?") {
		t.Fatalf("prepare after cooldown = %+v, %q", detail, result.Speech)
	}
}

func TestPrepareRestartBackendError(t *testing.T) {
	d := testDeps(t)
	d.Kube.(*fakeKube).errs[kubeKey("home", "Deployment", "scheduler")] = errors.New("api down")
	_, out, _ := d.infraPrepareRestart(context.Background(), nil, restartInput{Name: "scheduler"})
	if !strings.Contains(resultOf(t, out).Speech, "Не нашёл") {
		t.Fatal("backend error not reported")
	}
}

func TestConfirmExecutesPending(t *testing.T) {
	d := testDeps(t)
	prepareScheduler(t, d)
	_, out, err := d.infraConfirm(context.Background(), nil, struct{}{})
	if err != nil {
		t.Fatalf("confirm: %v", err)
	}
	result := resultOf(t, out)
	detail := result.Details.(confirmDetail)
	if !detail.Executed || detail.Scheduled {
		t.Fatalf("detail = %+v", detail)
	}
	kube := d.Kube.(*fakeKube)
	if len(kube.patches) != 1 {
		t.Fatalf("patches = %d", len(kube.patches))
	}
	patch := kube.patches[0]
	if patch.namespace != "home" || patch.kind != "Deployment" || patch.name != "scheduler" || !patch.at.Equal(d.Now()) {
		t.Fatalf("patch = %+v", patch)
	}
	if _, ok := d.Pending.Peek(); ok {
		t.Fatal("pending action was not cleared")
	}
}

func TestConfirmWithoutPending(t *testing.T) {
	d := testDeps(t)
	_, out, _ := d.infraConfirm(context.Background(), nil, struct{}{})
	if !strings.Contains(resultOf(t, out).Speech, "Нет отложенного") {
		t.Fatal("missing pending not reported")
	}
}

func TestConfirmAfterTTL(t *testing.T) {
	d := testDeps(t)
	now := d.Now()
	d.Pending = pending.New(90*time.Second, func() time.Time { return now })
	prepareScheduler(t, d)
	now = now.Add(91 * time.Second)
	d.Now = func() time.Time { return now }
	_, out, _ := d.infraConfirm(context.Background(), nil, struct{}{})
	if !strings.Contains(resultOf(t, out).Speech, "Нет отложенного") {
		t.Fatal("expired action was executed")
	}
	if len(d.Kube.(*fakeKube).patches) != 0 {
		t.Fatal("expired action patched the cluster")
	}
}

func TestConfirmPatchFailure(t *testing.T) {
	d := testDeps(t)
	prepareScheduler(t, d)
	d.Kube.(*fakeKube).errs["patch"] = errors.New("forbidden")
	_, out, _ := d.infraConfirm(context.Background(), nil, struct{}{})
	result := resultOf(t, out)
	if !strings.Contains(result.Speech, "Не удалось") {
		t.Fatalf("speech = %q", result.Speech)
	}
}

func TestConfirmSelfDisruptingDelaysPatch(t *testing.T) {
	d := testDeps(t)
	d.SelfRestartDelay = 20 * time.Second
	kube := d.Kube.(*fakeKube)
	kube.workloads[kubeKey("home", "Deployment", "hermes")] = kubernetes.Workload{
		Namespace: "home", Kind: "Deployment", Name: "hermes", Desired: 1, Ready: 1, Available: 1, Updated: 1,
	}
	if _, _, err := d.infraPrepareRestart(context.Background(), nil, restartInput{Name: "гермес"}); err != nil {
		t.Fatalf("prepare: %v", err)
	}
	_, out, err := d.infraConfirm(context.Background(), nil, struct{}{})
	if err != nil {
		t.Fatalf("confirm: %v", err)
	}
	result := resultOf(t, out)
	detail := result.Details.(confirmDetail)
	if !detail.Scheduled || detail.Executed {
		t.Fatalf("detail = %+v", detail)
	}
	if !strings.Contains(result.Speech, "20") {
		t.Fatalf("speech = %q", result.Speech)
	}
	if len(kube.patches) != 0 {
		t.Fatal("self-disrupting restart patched immediately")
	}
	timers := d.Timers.(*fakeTimers)
	if len(timers.delays) != 1 || timers.delays[0] != 20*time.Second {
		t.Fatalf("timers = %+v", timers.delays)
	}
	timers.runAll()
	if len(kube.patches) != 1 || kube.patches[0].name != "hermes" {
		t.Fatalf("patches after timer = %+v", kube.patches)
	}
}

func TestConfirmSelfDisruptingPatchFailureIsAudited(t *testing.T) {
	d := testDeps(t)
	d.Kube.(*fakeKube).errs["patch"] = errors.New("forbidden")
	d.Kube.(*fakeKube).workloads[kubeKey("home", "Deployment", "hermes")] = kubernetes.Workload{
		Namespace: "home", Kind: "Deployment", Name: "hermes", Desired: 1, Ready: 1, Available: 1, Updated: 1,
	}
	if _, _, err := d.infraPrepareRestart(context.Background(), nil, restartInput{Name: "гермес"}); err != nil {
		t.Fatalf("prepare: %v", err)
	}
	if _, _, err := d.infraConfirm(context.Background(), nil, struct{}{}); err != nil {
		t.Fatalf("confirm: %v", err)
	}
	d.Timers.(*fakeTimers).runAll() // must not panic
}

func TestCancel(t *testing.T) {
	d := testDeps(t)
	prepareScheduler(t, d)
	_, out, _ := d.infraCancel(context.Background(), nil, struct{}{})
	result := resultOf(t, out)
	if !strings.Contains(result.Speech, "Отменил") {
		t.Fatalf("speech = %q", result.Speech)
	}
	if _, ok := d.Pending.Peek(); ok {
		t.Fatal("pending action survived cancel")
	}
	_, out, _ = d.infraCancel(context.Background(), nil, struct{}{})
	if !strings.Contains(resultOf(t, out).Speech, "Нечего") {
		t.Fatal("empty cancel not reported")
	}
}

func TestAuditMetricCountsActions(t *testing.T) {
	d := testDeps(t)
	prepareScheduler(t, d)
	if _, _, err := d.infraConfirm(context.Background(), nil, struct{}{}); err != nil {
		t.Fatalf("confirm: %v", err)
	}
	families, err := d.Metrics.Registry().Gather()
	if err != nil {
		t.Fatalf("Gather: %v", err)
	}
	found := false
	for _, family := range families {
		if family.GetName() != "mcp_gateway_actions_total" {
			continue
		}
		for _, metric := range family.Metric {
			labels := map[string]string{}
			for _, label := range metric.Label {
				labels[label.GetName()] = label.GetValue()
			}
			if labels["action"] == "restart" && labels["target"] == "scheduler" && labels["outcome"] == "success" {
				found = true
			}
		}
	}
	if !found {
		t.Fatal("mcp_gateway_actions_total{action=restart,target=scheduler,outcome=success} missing")
	}
}
