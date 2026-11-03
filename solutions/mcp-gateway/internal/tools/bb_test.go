package tools

import (
	"context"
	"encoding/json"
	"strings"
	"testing"
	"time"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"ricardo.local/monorepo/solutions/mcp-gateway/internal/agentmode"
	"ricardo.local/monorepo/solutions/mcp-gateway/internal/bb"
	"ricardo.local/monorepo/solutions/mcp-gateway/internal/pending"
)

type fakeBB struct {
	targets     bb.Targets
	status      bb.StatusResult
	validateErr error
	spawnErr    error
	spawnResult bb.SpawnResult
	spawnCalls  []bb.SpawnRequest
	sendCalls   []bb.SendRequest
}

func (f *fakeBB) Targets(context.Context) (bb.Targets, error) { return f.targets, nil }
func (f *fakeBB) Status(context.Context, int) (bb.StatusResult, error) {
	return f.status, nil
}
func (f *fakeBB) ValidateSpawn(context.Context, bb.SpawnRequest) error { return f.validateErr }
func (f *fakeBB) Spawn(_ context.Context, req bb.SpawnRequest) (bb.SpawnResult, error) {
	f.spawnCalls = append(f.spawnCalls, req)
	if f.spawnErr != nil {
		return bb.SpawnResult{}, f.spawnErr
	}
	return f.spawnResult, nil
}
func (f *fakeBB) Send(_ context.Context, req bb.SendRequest) (bb.SendResult, error) {
	f.sendCalls = append(f.sendCalls, req)
	return bb.SendResult{ThreadID: req.ThreadID}, nil
}
func (f *fakeBB) TurnResult(context.Context, bb.TurnResultRequest) (bb.TurnResultResult, error) {
	return bb.TurnResultResult{Status: "idle"}, nil
}

func personalTargets() bb.Targets {
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

func newBBDeps(t *testing.T, client *fakeBB) *Deps {
	t.Helper()
	resolver, err := bb.ParseTargets([]byte(`targets:
  - project: proj_personal
    aliases: ["стан лэб", "station lab"]
    provider: claude-code
    title_prefix: "Голос"
  - project: proj_work
    aliases: ["работа"]
`))
	if err != nil {
		t.Fatalf("ParseTargets: %v", err)
	}
	return &Deps{
		BB:        client,
		BBTargets: resolver,
		BBPending: pending.NewKeyed[bb.SpawnAction](90*time.Second, nil),
		BBMode:    agentmode.New(),
	}
}

func decodeResult(t *testing.T, res *mcp.CallToolResult) Result {
	t.Helper()
	if len(res.Content) != 1 {
		t.Fatalf("content = %+v", res.Content)
	}
	text, ok := res.Content[0].(*mcp.TextContent)
	if !ok {
		t.Fatalf("content type = %T", res.Content[0])
	}
	var parsed struct {
		Speech  string          `json:"speech"`
		Details json.RawMessage `json:"details"`
	}
	if err := json.Unmarshal([]byte(text.Text), &parsed); err != nil {
		t.Fatalf("decode %q: %v", text.Text, err)
	}
	return Result{Speech: parsed.Speech, Details: parsed.Details}
}

func TestBBPrepareConfirmEntersAgentMode(t *testing.T) {
	client := &fakeBB{
		targets:     personalTargets(),
		spawnResult: bb.SpawnResult{ThreadID: "thr_new"},
	}
	deps := newBBDeps(t, client)

	res, _, err := deps.bbPrepareSpawn(context.Background(), nil, bbSpawnInput{Project: "стан лэб", Task: "почини тесты"})
	if err != nil {
		t.Fatalf("prepare: %v", err)
	}
	result := decodeResult(t, res)
	if result.Speech == "" {
		t.Fatal("prepare has no speech")
	}
	if !strings.HasSuffix(result.Speech, "Подтверждаете?") {
		t.Fatalf("confirmation speech does not end with a question: %q", result.Speech)
	}
	action, ok := deps.BBPending.Peek(BBPendingKey)
	if !ok {
		t.Fatal("no pending spawn")
	}
	if action.ProjectID != "proj_personal" || action.ProviderID != "claude-code" || action.Model != "sonnet" {
		t.Fatalf("action = %+v", action)
	}
	if action.IdempotencyKey == "" {
		t.Fatal("missing idempotency key")
	}

	res, _, err = deps.bbConfirm(context.Background(), nil, struct{}{})
	if err != nil {
		t.Fatalf("confirm: %v", err)
	}
	decodeResult(t, res)
	if len(client.spawnCalls) != 1 {
		t.Fatalf("spawn calls = %d", len(client.spawnCalls))
	}
	if client.spawnCalls[0].IdempotencyKey != action.IdempotencyKey {
		t.Fatal("confirm did not reuse the pending idempotency key")
	}
	if got, ok := deps.BBMode.Active(); !ok || got != "thr_new" {
		t.Fatalf("agent mode = %q, %v", got, ok)
	}
	if _, ok := deps.BBPending.Peek(BBPendingKey); ok {
		t.Fatal("pending survived confirm")
	}
}

func TestBBConfirmWithoutPending(t *testing.T) {
	deps := newBBDeps(t, &fakeBB{targets: personalTargets()})
	res, _, err := deps.bbConfirm(context.Background(), nil, struct{}{})
	if err != nil {
		t.Fatal(err)
	}
	if decodeResult(t, res).Speech != "Нет отложенного запуска." {
		t.Fatalf("speech = %q", decodeResult(t, res).Speech)
	}
}

func TestBBCancel(t *testing.T) {
	client := &fakeBB{targets: personalTargets()}
	deps := newBBDeps(t, client)
	if _, _, err := deps.bbPrepareSpawn(context.Background(), nil, bbSpawnInput{Project: "стан лэб", Task: "x"}); err != nil {
		t.Fatal(err)
	}
	res, _, err := deps.bbCancel(context.Background(), nil, struct{}{})
	if err != nil {
		t.Fatal(err)
	}
	if decodeResult(t, res).Speech != "Отменил запуск агента." {
		t.Fatalf("speech = %q", decodeResult(t, res).Speech)
	}
	if _, ok := deps.BBPending.Peek(BBPendingKey); ok {
		t.Fatal("cancel kept the pending spawn")
	}
	if len(client.spawnCalls) != 0 {
		t.Fatal("cancel spawned a thread")
	}
}

func TestBBPrepareRefusesWorkProject(t *testing.T) {
	deps := newBBDeps(t, &fakeBB{targets: personalTargets()})
	res, _, err := deps.bbPrepareSpawn(context.Background(), nil, bbSpawnInput{Project: "работа", Task: "x"})
	if err != nil {
		t.Fatal(err)
	}
	if decodeResult(t, res).Speech != "Не нашёл такой личный проект." {
		t.Fatalf("speech = %q", decodeResult(t, res).Speech)
	}
	if _, ok := deps.BBPending.Peek(BBPendingKey); ok {
		t.Fatal("work project was prepared")
	}
}

func TestBBPreparePropagatesValidationFailure(t *testing.T) {
	client := &fakeBB{targets: personalTargets(), validateErr: &bb.Error{Code: "thread_limit", Speech: "Лимит тредов."}}
	deps := newBBDeps(t, client)
	res, _, err := deps.bbPrepareSpawn(context.Background(), nil, bbSpawnInput{Project: "стан лэб", Task: "x"})
	if err != nil {
		t.Fatal(err)
	}
	if decodeResult(t, res).Speech != "Лимит тредов." {
		t.Fatalf("speech = %q", decodeResult(t, res).Speech)
	}
	if _, ok := deps.BBPending.Peek(BBPendingKey); ok {
		t.Fatal("failed validation left a pending action")
	}
}

func TestBBStatusHidesWorkThreads(t *testing.T) {
	client := &fakeBB{targets: personalTargets(), status: bb.StatusResult{Threads: []bb.Thread{
		{ID: "thr_1", Title: "Личный", Profile: "personal", Status: "active"},
		{ID: "thr_2", Title: "Рабочий", Profile: "work", Status: "active"},
		{ID: "thr_3", Title: "Без профиля", Profile: "", Status: "active"},
		{ID: "thr_4", Title: "Неизвестный", Profile: "unknown", Status: "active"},
	}}}
	deps := newBBDeps(t, client)
	res, _, err := deps.bbStatus(context.Background(), nil, struct{}{})
	if err != nil {
		t.Fatal(err)
	}
	result := decodeResult(t, res)
	var details bbStatusDetail
	raw, ok := result.Details.(json.RawMessage)
	if !ok {
		t.Fatalf("details type = %T", result.Details)
	}
	if err := json.Unmarshal(raw, &details); err != nil {
		t.Fatal(err)
	}
	if details.Total != 1 || details.Threads[0].ID != "thr_1" {
		t.Fatalf("details = %+v", details)
	}
	if result.Speech != "Работают 1 тредов: Личный." {
		t.Fatalf("speech = %q", result.Speech)
	}
}

func TestBBConfirmFailureDoesNotEnterAgentMode(t *testing.T) {
	client := &fakeBB{
		targets:  personalTargets(),
		spawnErr: &bb.Error{Code: "thread_limit", Speech: "Лимит тредов."},
	}
	deps := newBBDeps(t, client)
	if _, _, err := deps.bbPrepareSpawn(context.Background(), nil, bbSpawnInput{Project: "стан лэб", Task: "x"}); err != nil {
		t.Fatal(err)
	}
	res, _, err := deps.bbConfirm(context.Background(), nil, struct{}{})
	if err != nil {
		t.Fatal(err)
	}
	if decodeResult(t, res).Speech != "Лимит тредов." {
		t.Fatalf("speech = %q", decodeResult(t, res).Speech)
	}
	if _, ok := deps.BBMode.Active(); ok {
		t.Fatal("failed spawn entered agent mode")
	}
}

func TestBBPendingExpires(t *testing.T) {
	now := time.Date(2026, 11, 3, 10, 0, 0, 0, time.UTC)
	deps := newBBDeps(t, &fakeBB{targets: personalTargets()})
	deps.BBPending = pending.NewKeyed[bb.SpawnAction](90*time.Second, func() time.Time { return now })
	if _, _, err := deps.bbPrepareSpawn(context.Background(), nil, bbSpawnInput{Project: "стан лэб", Task: "x"}); err != nil {
		t.Fatal(err)
	}
	now = now.Add(91 * time.Second)
	res, _, err := deps.bbConfirm(context.Background(), nil, struct{}{})
	if err != nil {
		t.Fatal(err)
	}
	if decodeResult(t, res).Speech != "Нет отложенного запуска." {
		t.Fatalf("speech = %q", decodeResult(t, res).Speech)
	}
}

func TestBBToolsReportUnconfigured(t *testing.T) {
	deps := &Deps{}
	res, _, err := deps.bbStatus(context.Background(), nil, struct{}{})
	if err != nil {
		t.Fatal(err)
	}
	if decodeResult(t, res).Speech != bb.SpeechNotReady {
		t.Fatalf("speech = %q", decodeResult(t, res).Speech)
	}
	if _, _, err := deps.bbPrepareSpawn(context.Background(), nil, bbSpawnInput{Project: "x", Task: "y"}); err != nil {
		t.Fatal(err)
	}
}
