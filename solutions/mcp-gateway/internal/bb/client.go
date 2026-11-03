// Package bb talks to the bb home-dispatcher plugin through the forced-command
// SSH bridge on the bb host.
//
// The gateway never speaks HTTP to bb directly and never has a shell: every
// call is one SSH session, one JSON object on stdin, one JSON answer on stdout.
// The bridge owns the plugin token and the HMAC secret; the gateway owns only
// an ed25519 key restricted to that bridge and a pinned known_hosts file.
package bb

import (
	"context"
	"encoding/json"
	"errors"
	"time"
)

// Methods exposed by the bridge. The list is closed: the gateway cannot ask
// for anything else.
const (
	MethodTargets       = "targets"
	MethodStatus        = "status"
	MethodValidateSpawn = "validate_spawn"
	MethodSpawn         = "spawn"
	MethodSend          = "send"
	MethodTurnResult    = "turn_result"
	MethodUpdates       = "updates"
)

// Short Russian phrases used when the bridge or the plugin fails. They are
// spoken as-is, so they must stay one short line.
const (
	SpeechUnavailable = "Сервер агентов недоступен."
	SpeechNotReady    = "Сервер агентов не настроен."
)

// Error is a refused or failed bridge call. Code is the plugin/bridge code,
// Speech is the short spoken line.
type Error struct {
	Code   string
	Speech string
}

func (e *Error) Error() string { return "bb: " + e.Code + ": " + e.Speech }

// AsError extracts a bridge error; other errors are wrapped as unavailable.
func AsError(err error) *Error {
	var target *Error
	if errors.As(err, &target) {
		return target
	}
	return &Error{Code: "bridge_error", Speech: SpeechUnavailable}
}

// Runner sends one request to the bridge and returns its stdout and exit code.
// A non-zero exit code still carries a JSON answer from the bridge.
type Runner interface {
	Run(ctx context.Context, payload []byte) (stdout []byte, exitCode int, err error)
}

// Dispatcher is the typed plugin API on top of one Runner.
type Dispatcher struct {
	runner  Runner
	timeout time.Duration
}

// NewDispatcher builds a dispatcher. A zero timeout defaults to 15 s.
func NewDispatcher(runner Runner, timeout time.Duration) *Dispatcher {
	if timeout <= 0 {
		timeout = 15 * time.Second
	}
	return &Dispatcher{runner: runner, timeout: timeout}
}

// API is the subset used by the MCP tools and the voice path, so both can be
// tested with a fake.
type API interface {
	Targets(ctx context.Context) (Targets, error)
	Status(ctx context.Context, limit int) (StatusResult, error)
	ValidateSpawn(ctx context.Context, req SpawnRequest) error
	Spawn(ctx context.Context, req SpawnRequest) (SpawnResult, error)
	Send(ctx context.Context, req SendRequest) (SendResult, error)
	TurnResult(ctx context.Context, req TurnResultRequest) (TurnResultResult, error)
	Updates(ctx context.Context, req UpdatesRequest) (UpdatesResult, error)
}

var _ API = (*Dispatcher)(nil)

// Call performs one bridge call and returns the raw result.
func (d *Dispatcher) Call(ctx context.Context, method string, params any) (json.RawMessage, error) {
	payload, err := json.Marshal(request{Method: method, Params: params})
	if err != nil {
		return nil, &Error{Code: "invalid_request", Speech: "Некорректный запрос."}
	}
	callCtx, cancel := context.WithTimeout(ctx, d.timeout)
	defer cancel()
	stdout, exitCode, err := d.runner.Run(callCtx, payload)
	if err != nil {
		if errors.Is(err, context.DeadlineExceeded) {
			return nil, &Error{Code: "timeout", Speech: SpeechUnavailable}
		}
		return nil, &Error{Code: "bridge_error", Speech: SpeechUnavailable}
	}
	answer, parseErr := parseAnswer(stdout)
	if parseErr != nil {
		return nil, parseErr
	}
	if !answer.OK || exitCode != 0 {
		code := answer.Code
		if code == "" {
			code = "bridge_error"
		}
		speech := answer.Speech
		if speech == "" {
			speech = SpeechUnavailable
		}
		return nil, &Error{Code: code, Speech: speech}
	}
	return answer.Result, nil
}

type request struct {
	Method string `json:"method"`
	Params any    `json:"params,omitempty"`
}

type answer struct {
	OK     bool            `json:"ok"`
	Result json.RawMessage `json:"result"`
	Code   string          `json:"code"`
	Speech string          `json:"speech"`
}

func parseAnswer(stdout []byte) (answer, error) {
	var parsed answer
	if err := json.Unmarshal(stdout, &parsed); err != nil {
		return answer{}, &Error{Code: "bridge_error", Speech: SpeechUnavailable}
	}
	return parsed, nil
}

func (d *Dispatcher) decode(ctx context.Context, method string, params any, out any) error {
	raw, err := d.Call(ctx, method, params)
	if err != nil {
		return err
	}
	if err := json.Unmarshal(raw, out); err != nil {
		return &Error{Code: "bridge_error", Speech: SpeechUnavailable}
	}
	return nil
}

// Targets lists the projects the plugin can spawn into.
func (d *Dispatcher) Targets(ctx context.Context) (Targets, error) {
	var out Targets
	if err := d.decode(ctx, MethodTargets, struct{}{}, &out); err != nil {
		return Targets{}, err
	}
	return out, nil
}

// Status returns recent and running threads.
func (d *Dispatcher) Status(ctx context.Context, limit int) (StatusResult, error) {
	var out StatusResult
	if err := d.decode(ctx, MethodStatus, statusParams{Limit: limit}, &out); err != nil {
		return StatusResult{}, err
	}
	return out, nil
}

// ValidateSpawn asks the plugin to validate a spawn without creating a thread.
// Its params are the spawn *target* only: the plugin's `validate_spawn` schema
// deliberately rejects `title`, `prompt` and `idempotencyKey` (they belong to
// `spawn`), so the request cannot reuse SpawnRequest. See
// solutions/bb-home-dispatcher/contract/params.json.
func (d *Dispatcher) ValidateSpawn(ctx context.Context, req SpawnRequest) error {
	var out struct{}
	return d.decode(ctx, MethodValidateSpawn, req.target(), &out)
}

// Spawn creates a visible thread. It is idempotent on IdempotencyKey.
func (d *Dispatcher) Spawn(ctx context.Context, req SpawnRequest) (SpawnResult, error) {
	var out SpawnResult
	if err := d.decode(ctx, MethodSpawn, req, &out); err != nil {
		return SpawnResult{}, err
	}
	return out, nil
}

// Send appends a message to a thread created by the plugin or to the
// configured dispatcher thread.
func (d *Dispatcher) Send(ctx context.Context, req SendRequest) (SendResult, error) {
	var out SendResult
	if err := d.decode(ctx, MethodSend, req, &out); err != nil {
		return SendResult{}, err
	}
	return out, nil
}

// TurnResult reads the outcome of the turn that follows AfterSeq.
func (d *Dispatcher) TurnResult(ctx context.Context, req TurnResultRequest) (TurnResultResult, error) {
	var out TurnResultResult
	if err := d.decode(ctx, MethodTurnResult, req, &out); err != nil {
		return TurnResultResult{}, err
	}
	return out, nil
}

// Updates polls completed dispatcher turns the owner did not start.
func (d *Dispatcher) Updates(ctx context.Context, req UpdatesRequest) (UpdatesResult, error) {
	var out UpdatesResult
	if err := d.decode(ctx, MethodUpdates, req, &out); err != nil {
		return UpdatesResult{}, err
	}
	return out, nil
}

// Targets is the `targets` result.
type Targets struct {
	Projects []Target `json:"projects"`
}

// Target is one project the plugin can spawn into.
type Target struct {
	ProjectID string     `json:"projectId"`
	Name      string     `json:"name"`
	Profile   string     `json:"profile"`
	Policy    Policy     `json:"policy"`
	Machine   Machine    `json:"machine"`
	Providers []Provider `json:"providers"`
}

// Policy is the effective profile policy from the profiles plugin.
type Policy struct {
	Providers []string `json:"providers"`
	Machines  []string `json:"machines"`
}

// Machine is the project's source machine.
type Machine struct {
	HostID string `json:"hostId"`
	Name   string `json:"name"`
	Status string `json:"status"`
	Online bool   `json:"online"`
}

// Provider is one agent provider available on the project's machine.
type Provider struct {
	ProviderID  string  `json:"providerId"`
	DisplayName string  `json:"displayName"`
	Models      []Model `json:"models"`
}

// Model is one model plus its reasoning levels.
type Model struct {
	Model                 string   `json:"model"`
	DisplayName           string   `json:"displayName"`
	DefaultReasoningLevel string   `json:"defaultReasoningLevel"`
	ReasoningLevels       []string `json:"reasoningLevels"`
}

// SpawnRequest is the `spawn` params. ValidateSpawn narrows it to the target
// fields before sending.
type SpawnRequest struct {
	Profile        string `json:"profile"`
	ProjectID      string `json:"projectId"`
	ProviderID     string `json:"providerId"`
	Model          string `json:"model"`
	ReasoningLevel string `json:"reasoningLevel"`
	Title          string `json:"title,omitempty"`
	Prompt         string `json:"prompt,omitempty"`
	IdempotencyKey string `json:"idempotencyKey,omitempty"`
}

// spawnTarget is the whole of `validate_spawn`'s params and the common subset
// of `spawn`'s.
type spawnTarget struct {
	Profile        string `json:"profile"`
	ProjectID      string `json:"projectId"`
	ProviderID     string `json:"providerId"`
	Model          string `json:"model"`
	ReasoningLevel string `json:"reasoningLevel"`
}

func (r SpawnRequest) target() spawnTarget {
	return spawnTarget{
		Profile:        r.Profile,
		ProjectID:      r.ProjectID,
		ProviderID:     r.ProviderID,
		Model:          r.Model,
		ReasoningLevel: r.ReasoningLevel,
	}
}

// SpawnResult is the spawned thread identity.
type SpawnResult struct {
	ThreadID   string `json:"threadId"`
	Idempotent bool   `json:"idempotent"`
}

// SendRequest appends text to a thread.
type SendRequest struct {
	ThreadID       string `json:"threadId"`
	Text           string `json:"text"`
	IdempotencyKey string `json:"idempotencyKey,omitempty"`
}

// SendResult carries the cursor for TurnResult.
type SendResult struct {
	ThreadID   string `json:"threadId"`
	AfterSeq   int64  `json:"afterSeq"`
	RequestSeq *int64 `json:"requestSeq,omitempty"`
	RequestID  string `json:"requestId,omitempty"`
	Delivery   string `json:"delivery,omitempty"`
}

// TurnResultRequest long-polls one turn.
type TurnResultRequest struct {
	ThreadID string `json:"threadId"`
	AfterSeq int64  `json:"afterSeq"`
}

// UpdatesRequest polls completed dispatcher turns that the owner did not
// start. AfterSeq nil is the bootstrap call (cursor only, no history).
type UpdatesRequest struct {
	AfterSeq    *int64 `json:"afterSeq,omitempty"`
	WaitSeconds *int   `json:"waitSeconds,omitempty"`
}

// UpdateItem is one completed non-owner turn and its 🔊 line.
type UpdateItem struct {
	Seq    int64  `json:"seq"`
	Spoken string `json:"spoken"`
}

// UpdatesResult is the dispatcher-updates answer. Cursor is the latest event
// seq of the dispatcher thread.
type UpdatesResult struct {
	Cursor int64        `json:"cursor"`
	Items  []UpdateItem `json:"items"`
}

// TurnResultResult is the turn outcome. Spoken is nil when the agent did not
// emit the 🔊 line.
type TurnResultResult struct {
	Status       string  `json:"status"`
	Spoken       *string `json:"spoken"`
	Text         string  `json:"text"`
	ThreadID     string  `json:"threadId"`
	RequestSeq   *int64  `json:"requestSeq"`
	CompletedSeq *int64  `json:"completedSeq"`
	Ref          string  `json:"ref"`
}

// Thread is one entry of the status result.
type Thread struct {
	ID             string `json:"id"`
	Title          string `json:"title"`
	Project        string `json:"project"`
	ProjectID      string `json:"projectId"`
	Profile        string `json:"profile"`
	Status         string `json:"status"`
	RuntimeSeconds *int64 `json:"runtimeSeconds"`
}

// StatusResult is the status answer.
type StatusResult struct {
	Threads []Thread `json:"threads"`
}

type statusParams struct {
	Limit int `json:"limit"`
}

// SpawnAction is the pending value of bb.act: everything confirmed later.
type SpawnAction struct {
	Project        string
	ProjectID      string
	ProviderID     string
	Model          string
	ReasoningLevel string
	Title          string
	Prompt         string
	IdempotencyKey string
}

// Request converts the pending action into the plugin spawn request.
func (a SpawnAction) Request() SpawnRequest {
	return SpawnRequest{
		Profile:        ProfilePersonal,
		ProjectID:      a.ProjectID,
		ProviderID:     a.ProviderID,
		Model:          a.Model,
		ReasoningLevel: a.ReasoningLevel,
		Title:          a.Title,
		Prompt:         a.Prompt,
		IdempotencyKey: a.IdempotencyKey,
	}
}

// ProfilePersonal is the only profile Hermes and the MCP tools may use.
const ProfilePersonal = "personal"

// SpeechForErr returns the short spoken line of an error.
func SpeechForErr(err error) string {
	if err == nil {
		return ""
	}
	var target *Error
	if errors.As(err, &target) && target.Speech != "" {
		return target.Speech
	}
	return SpeechUnavailable
}

// FormatThread renders one thread for a details payload.
func FormatThread(t Thread) string {
	if t.Title != "" {
		return t.Title
	}
	return t.ID
}
