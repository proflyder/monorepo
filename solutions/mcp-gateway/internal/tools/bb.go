package tools

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"errors"
	"fmt"
	"strings"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"ricardo.local/monorepo/solutions/mcp-gateway/internal/bb"
)

// BB tool-group names. They mirror the infra.read/infra.fix split: reading
// threads is separate from starting one.
const (
	GroupBBRead = "bb.read"
	GroupBBAct  = "bb.act"
)

// BBPendingKey is the pending key of bb.act; infra's restart store is separate.
const BBPendingKey = "bb.spawn"

// MaxBBTaskRunes bounds the task text accepted from speech.
const MaxBBTaskRunes = 2000

func (d *Deps) bbReady() bool {
	return d.BB != nil && d.BBTargets != nil
}

// BBReadGroup is the bb.read tool group.
type BBReadGroup struct{ Deps *Deps }

// Name implements group.Group.
func (g *BBReadGroup) Name() string { return GroupBBRead }

// Register implements group.Group.
func (g *BBReadGroup) Register(r *mcp.Server) {
	d := g.Deps
	mcp.AddTool(r, &mcp.Tool{
		Name:        "bb_status",
		Description: "Short summary of running agent threads in the personal profile. Work-profile threads and projects are never listed. Returns a short Russian speech line plus structured thread details.",
		Annotations: readOnly(),
	}, instrument(d, "bb_status", d.bbStatus))
}

// BBActGroup is the bb.act tool group.
type BBActGroup struct{ Deps *Deps }

// Name implements group.Group.
func (g *BBActGroup) Name() string { return GroupBBAct }

// Register implements group.Group.
func (g *BBActGroup) Register(r *mcp.Server) {
	d := g.Deps
	mcp.AddTool(r, &mcp.Tool{
		Name:        "bb_prepare_spawn",
		Description: "Validate that an agent may be started in one personal project and store exactly one pending spawn for 90 seconds. Project is resolved only among personal targets, by spoken name or alias; provider, model and reasoning level must be allowed by the dispatcher. Nothing is started yet: returns a short Russian confirmation question.",
		Annotations: destructive(),
	}, instrument(d, "bb_prepare_spawn", d.bbPrepareSpawn))

	mcp.AddTool(r, &mcp.Tool{
		Name:        "bb_confirm",
		Description: "Start the agent prepared by bb_prepare_spawn. Takes no arguments, uses the stored idempotency key, and turns on agent conversation mode for the created thread.",
		Annotations: destructive(),
	}, instrument(d, "bb_confirm", d.bbConfirm))

	mcp.AddTool(r, &mcp.Tool{
		Name:        "bb_cancel",
		Description: "Drop the pending agent spawn without starting it.",
		Annotations: destructive(),
	}, instrument(d, "bb_cancel", d.bbCancel))
}

// --- bb_status ------------------------------------------------------------

type bbStatusDetail struct {
	Threads []bb.Thread `json:"threads"`
	Total   int         `json:"total"`
}

func (d *Deps) bbStatus(ctx context.Context, _ *mcp.CallToolRequest, _ struct{}) (*mcp.CallToolResult, any, error) {
	if d.BB == nil {
		return Failure(bb.SpeechNotReady, nil)
	}
	status, err := d.BB.Status(ctx, 20)
	if err != nil {
		return Failure(bb.SpeechForErr(err), map[string]string{"code": bb.AsError(err).Code})
	}
	personal := make([]bb.Thread, 0, len(status.Threads))
	for _, thread := range status.Threads {
		if thread.Profile != bb.ProfilePersonal {
			continue
		}
		personal = append(personal, thread)
	}
	return respond(Result{Speech: bbStatusSpeech(personal), Details: bbStatusDetail{Threads: personal, Total: len(personal)}})
}

func bbStatusSpeech(threads []bb.Thread) string {
	if len(threads) == 0 {
		return "Работающих тредов нет."
	}
	names := make([]string, 0, 2)
	for _, thread := range threads {
		if len(names) >= 2 {
			break
		}
		names = append(names, bb.FormatThread(thread))
	}
	speech := fmt.Sprintf("Работают %d тредов: %s", len(threads), strings.Join(names, ", "))
	if len(threads) > len(names) {
		speech += " и другие"
	}
	return speech + "."
}

// --- bb_prepare_spawn -----------------------------------------------------

type bbSpawnInput struct {
	Project string `json:"project" jsonschema:"spoken project name or alias from the personal allowlist"`
	Model   string `json:"model,omitempty" jsonschema:"optional model id allowed by the dispatcher"`
	Effort  string `json:"effort,omitempty" jsonschema:"optional reasoning level allowed by the dispatcher"`
	Task    string `json:"task" jsonschema:"task text for the agent"`
}

type bbPrepareDetail struct {
	Allowed   bool   `json:"allowed"`
	Project   string `json:"project"`
	Provider  string `json:"provider,omitempty"`
	Model     string `json:"model,omitempty"`
	Effort    string `json:"effort,omitempty"`
	Task      string `json:"task,omitempty"`
	ExpiresAt string `json:"expiresAt,omitempty"`
	Reason    string `json:"reason,omitempty"`
}

func (d *Deps) bbPrepareSpawn(ctx context.Context, _ *mcp.CallToolRequest, in bbSpawnInput) (*mcp.CallToolResult, any, error) {
	if !d.bbReady() || d.BBPending == nil {
		return Failure(bb.SpeechNotReady, nil)
	}
	task := strings.TrimSpace(in.Task)
	if task == "" {
		return Failure("Пустая задача для агента.", bbPrepareDetail{Reason: "empty task"})
	}
	if len([]rune(task)) > MaxBBTaskRunes {
		return Failure(fmt.Sprintf("Задача длиннее %d символов.", MaxBBTaskRunes), bbPrepareDetail{Reason: "task too long"})
	}
	spoken := strings.TrimSpace(in.Project)
	if spoken == "" {
		return Failure("Не указан проект.", bbPrepareDetail{Reason: "empty project"})
	}

	targets, err := d.BB.Targets(ctx)
	if err != nil {
		return Failure(bb.SpeechForErr(err), bbPrepareDetail{Reason: bb.AsError(err).Code})
	}
	resolved, err := d.BBTargets.Resolve(targets, spoken, in.Model, in.Effort)
	if err != nil {
		return Failure(bbResolveSpeech(err), bbPrepareDetail{Reason: err.Error()})
	}

	action := bb.SpawnAction{
		Project:        resolved.Project,
		ProjectID:      resolved.ProjectID,
		ProviderID:     resolved.ProviderID,
		Model:          resolved.Model,
		ReasoningLevel: resolved.Effort,
		Title:          bbTitle(resolved.TitlePrefix, task),
		Prompt:         task,
		IdempotencyKey: newIdempotencyKey(),
	}
	if err := d.BB.ValidateSpawn(ctx, action.Request()); err != nil {
		return Failure(bb.SpeechForErr(err), bbPrepareDetail{Reason: bb.AsError(err).Code})
	}
	createdAt := d.BBPending.Put(BBPendingKey, action)
	detail := bbPrepareDetail{
		Allowed:   true,
		Project:   resolved.Project,
		Provider:  resolved.ProviderID,
		Model:     resolved.Model,
		Effort:    resolved.Effort,
		Task:      task,
		ExpiresAt: createdAt.Add(d.BBPending.TTL()).UTC().Format("2006-01-02T15:04:05Z07:00"),
	}
	d.logger().Info("bb spawn prepared", "event", "action_prepared", "action", "bb_spawn",
		"target", resolved.ProjectID, "provider", resolved.ProviderID, "model", resolved.Model)
	speech := fmt.Sprintf("Запустить агента в %s: %s. Подтверждаете?", resolved.Project, shorten(task, 90))
	return respond(Result{Speech: speech, Details: detail})
}

func bbResolveSpeech(err error) string {
	switch {
	case errors.Is(err, bb.ErrTargetNotFound):
		return "Не нашёл такой личный проект."
	case errors.Is(err, bb.ErrProviderUnavailable):
		return "Провайдер для этого проекта недоступен."
	case errors.Is(err, bb.ErrModelNotFound):
		return "Эта модель недоступна."
	case errors.Is(err, bb.ErrEffortNotSupported):
		return "Этот уровень усилия недоступен."
	default:
		return bb.SpeechUnavailable
	}
}

func bbTitle(prefix, task string) string {
	summary := shorten(task, 60)
	if prefix == "" {
		return summary
	}
	return prefix + ": " + summary
}

func shorten(text string, limit int) string {
	runes := []rune(strings.TrimSpace(text))
	if len(runes) <= limit {
		return string(runes)
	}
	return strings.TrimSpace(string(runes[:limit-1])) + "…"
}

func newIdempotencyKey() string {
	buf := make([]byte, 16)
	if _, err := rand.Read(buf); err != nil {
		// crypto/rand failure is fatal for uniqueness; fall back to a value the
		// plugin will still accept rather than spawning without a key.
		return "bb-spawn-fallback"
	}
	return "bb-spawn-" + hex.EncodeToString(buf)
}

// --- bb_confirm -----------------------------------------------------------

type bbConfirmDetail struct {
	Started    bool   `json:"started"`
	ThreadID   string `json:"threadId,omitempty"`
	Project    string `json:"project,omitempty"`
	Idempotent bool   `json:"idempotent,omitempty"`
	Reason     string `json:"reason,omitempty"`
	Error      string `json:"error,omitempty"`
}

func (d *Deps) bbConfirm(ctx context.Context, _ *mcp.CallToolRequest, _ struct{}) (*mcp.CallToolResult, any, error) {
	if d.BB == nil || d.BBPending == nil {
		return Failure(bb.SpeechNotReady, nil)
	}
	action, ok := d.BBPending.Take(BBPendingKey)
	if !ok {
		return respond(Result{Speech: "Нет отложенного запуска.", Details: bbConfirmDetail{Reason: "no pending spawn"}})
	}
	result, err := d.BB.Spawn(ctx, action.Request())
	if err != nil {
		bridgeErr := bb.AsError(err)
		d.audit("bb_spawn", action.ProjectID, "failure", "error", bridgeErr.Code)
		return Failure(bridgeErr.Speech, bbConfirmDetail{Project: action.Project, Error: bridgeErr.Code})
	}
	if result.ThreadID == "" {
		d.audit("bb_spawn", action.ProjectID, "failure", "error", "empty thread id")
		return Failure(bb.SpeechUnavailable, bbConfirmDetail{Project: action.Project, Error: "empty thread id"})
	}
	if d.BBMode != nil {
		d.BBMode.Enter(result.ThreadID)
	}
	d.audit("bb_spawn", action.ProjectID, "success", "thread_id", result.ThreadID)
	return respond(Result{
		Speech:  "Запустил агента. Теперь это режим агента.",
		Details: bbConfirmDetail{Started: true, ThreadID: result.ThreadID, Project: action.Project, Idempotent: result.Idempotent},
	})
}

// --- bb_cancel ------------------------------------------------------------

type bbCancelDetail struct {
	Cancelled bool   `json:"cancelled"`
	Project   string `json:"project,omitempty"`
}

func (d *Deps) bbCancel(_ context.Context, _ *mcp.CallToolRequest, _ struct{}) (*mcp.CallToolResult, any, error) {
	if d.BBPending == nil {
		return Failure(bb.SpeechNotReady, nil)
	}
	action, ok := d.BBPending.Peek(BBPendingKey)
	d.BBPending.Clear(BBPendingKey)
	if !ok {
		return respond(Result{Speech: "Нечего отменять.", Details: bbCancelDetail{}})
	}
	d.audit("bb_spawn", action.ProjectID, "cancelled")
	return respond(Result{Speech: "Отменил запуск агента.", Details: bbCancelDetail{Cancelled: true, Project: action.Project}})
}
