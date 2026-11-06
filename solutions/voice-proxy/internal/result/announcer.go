// Package result announces the final answer of a routed bb turn.
//
// The immediate acknowledgement was already spoken by the router, so this
// background worker long-polls /v1/voice/result for up to 900 s and then either
// enqueues the answer (durable queue) or announces it directly.
//
// Two behaviours are deliberate:
//
//   - a timeout, a gateway error or a still-running turn speaks nothing at all,
//     because inventing speech would be worse than silence;
//   - an empty successful answer becomes "Готово.", an explicitly failed turn
//     becomes "Агент завершил работу с ошибкой.".
package result

import (
	"context"
	"encoding/json"
	"math"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"
	"time"

	"github.com/proflyder/monorepo/solutions/voice-proxy/internal/profile"
	"github.com/proflyder/monorepo/solutions/voice-proxy/internal/sanitize"
)

// Speech phrases. They never end with a question: HA's
// pipeline enables continued conversation when the assistant reply does.
const (
	SpeechAgentsDown = "Сервер агентов недоступен."
	SpeechTurnFailed = "Агент завершил работу с ошибкой."
	SpeechDone       = "Готово."
)

// Logger is the structural log surface: outcomes and timings only, never speech.
type Logger interface {
	Infof(format string, args ...any)
	Errorf(format string, args ...any)
}

// ResultClient is the /v1/voice/result half of the gateway client.
type ResultClient interface {
	Result(ctx context.Context, threadID string, afterSeq int64, waitSeconds int) (map[string]any, error)
}

// Speaker speaks an already-shaped text.
type Speaker interface {
	Announce(ctx context.Context, text string) (bool, bool)
}

// EnqueueFunc stores one durable announcement; false means the scheduler was
// unavailable and the caller must announce directly.
type EnqueueFunc func(ctx context.Context, speech, profileID, threadID, seq string, createdUnix *int64) bool

// Config configures an Announcer.
type Config struct {
	// Context is the process worker context. Announcers started by a request
	// must outlive it, so they use this context rather than the HTTP request.
	Context context.Context
	Client  ResultClient
	// Announce speaks directly when the queue is off or the enqueue failed.
	Announce Speaker
	Enqueue  EnqueueFunc
	// QueueEnabled reports whether the durable queue is usable right now.
	QueueEnabled func() bool
	// MaxChars is HERMES_VOICE_ANNOUNCE_MAX_CHARS.
	MaxChars int
	// Timeout is the overall budget (HERMES_VOICE_RESULT_TIMEOUT, 900 s).
	Timeout time.Duration
	// Wait is HERMES_VOICE_RESULT_WAIT (25 s).
	Wait int
	// Retry is HERMES_VOICE_RESULT_RETRY (2 s).
	Retry  time.Duration
	Logger Logger
	// Clock and Sleep are injectable for tests.
	Clock func() time.Time
	Sleep func(ctx context.Context, d time.Duration) bool
}

// Announcer tracks the background result announcers for graceful shutdown.
type Announcer struct {
	cfg     Config
	running atomic.Int64
	wg      sync.WaitGroup
}

// New builds an Announcer.
func New(cfg Config) *Announcer {
	if cfg.MaxChars <= 0 {
		cfg.MaxChars = 200
	}
	if cfg.Timeout <= 0 {
		cfg.Timeout = 900 * time.Second
	}
	if cfg.Wait <= 0 {
		cfg.Wait = 25
	}
	if cfg.Retry <= 0 {
		cfg.Retry = 2 * time.Second
	}
	if cfg.Clock == nil {
		cfg.Clock = time.Now
	}
	if cfg.Sleep == nil {
		cfg.Sleep = sleepContext
	}
	if cfg.Context == nil {
		cfg.Context = context.Background()
	}
	if cfg.Logger == nil {
		cfg.Logger = nopLogger{}
	}
	return &Announcer{cfg: cfg}
}

// Start spawns one background announcer, like start_result_announcer().
func (a *Announcer) Start(threadID string, afterSeq int64, profileID string) {
	a.running.Add(1)
	a.wg.Add(1)
	go func() {
		defer a.wg.Done()
		defer a.running.Add(-1)
		a.Poll(a.cfg.Context, threadID, afterSeq, profileID)
	}()
}

// Running is the number of live background announcers.
func (a *Announcer) Running() int64 { return a.running.Load() }

// Wait joins the live background announcers, up to timeout (tests and graceful
// shutdown).
func (a *Announcer) Wait(timeout time.Duration) {
	done := make(chan struct{})
	go func() {
		a.wg.Wait()
		close(done)
	}()
	timer := time.NewTimer(timeout)
	defer timer.Stop()
	select {
	case <-done:
	case <-timer.C:
	}
}

// Poll long-polls one turn and speaks or enqueues its answer. It is synchronous
// so a test can drive it directly.
func (a *Announcer) Poll(ctx context.Context, threadID string, afterSeq int64, profileID string) {
	deadline := a.cfg.Clock().Add(a.cfg.Timeout)
	status := "timeout"
	speech := ""
	var completedSeq int64
	hasCompletedSeq := false
	finished := false

	for a.cfg.Clock().Before(deadline) {
		remaining := int(math.Min(float64(a.cfg.Wait), deadline.Sub(a.cfg.Clock()).Seconds()))
		if remaining < 1 {
			remaining = 1
		}
		response, err := a.cfg.Client.Result(ctx, threadID, afterSeq, remaining)
		if err != nil {
			status = "gateway_error"
			if !a.cfg.Clock().Add(a.cfg.Retry).Before(deadline) {
				finished = true
				break
			}
			if !a.cfg.Sleep(ctx, a.cfg.Retry) {
				return
			}
			continue
		}
		status = stringValue(response["status"])
		if status == "running" || status == "starting" {
			status = "running"
			continue
		}
		speech = stringValue(response["speech"])
		if seq, ok := intValue(response["completedSeq"]); ok {
			completedSeq = seq
			hasCompletedSeq = true
		}
		finished = true
		break
	}
	// Reaching the deadline without an answer is a timeout, even when the last
	// poll said "running".
	if !finished {
		status = "timeout"
	}

	if status == "running" || status == "gateway_error" || status == "" {
		// The overall budget ran out or the turn is still running; do not
		// invent speech.
		speech = ""
	}
	if strings.TrimSpace(speech) == "" &&
		status != "running" && status != "gateway_error" && status != "" && status != "timeout" {
		if status == "failed" {
			speech = SpeechTurnFailed
		} else {
			speech = SpeechDone
		}
	}
	text := sanitize.AnnounceText(speech, a.cfg.MaxChars)
	if text == "" {
		a.cfg.Logger.Infof("announce thread=%s outcome=%s announced=false listen=false",
			threadID, statusOrUnknown(status))
		return
	}

	if a.queueEnabled() && (profileID == profile.Work || profileID == profile.Personal) {
		seqKey := "after:" + strconv.FormatInt(afterSeq, 10)
		if hasCompletedSeq {
			seqKey = "done:" + strconv.FormatInt(completedSeq, 10)
		}
		if a.cfg.Enqueue != nil && a.cfg.Enqueue(ctx, text, profileID, threadID, seqKey, nil) {
			a.cfg.Logger.Infof("queue enqueued thread=%s profile=%s outcome=%s",
				threadID, profileID, statusOrUnknown(status))
			return
		}
		// The scheduler is down or misconfigured: never lose the answer.
		a.cfg.Logger.Infof("queue enqueue failed thread=%s fallback=direct", threadID)
	}

	announced, listening := a.cfg.Announce.Announce(ctx, text)
	a.cfg.Logger.Infof("announce thread=%s outcome=%s announced=%t listen=%t",
		threadID, statusOrUnknown(status), announced, listening)
}

func (a *Announcer) queueEnabled() bool {
	return a.cfg.QueueEnabled != nil && a.cfg.QueueEnabled()
}

func statusOrUnknown(status string) string {
	if status == "" {
		return "unknown"
	}
	return status
}

func stringValue(value any) string {
	text, _ := value.(string)
	return text
}

// intValue accepts the numeric forms a JSON decoder can produce. bool is not a
// number, matching `isinstance(seq, int) and not isinstance(seq, bool)`.
func intValue(value any) (int64, bool) {
	switch number := value.(type) {
	case json.Number:
		if parsed, err := number.Int64(); err == nil {
			return parsed, true
		}
	case float64:
		return int64(number), true
	case int64:
		return number, true
	case int:
		return int64(number), true
	}
	return 0, false
}

func sleepContext(ctx context.Context, d time.Duration) bool {
	timer := time.NewTimer(d)
	defer timer.Stop()
	select {
	case <-ctx.Done():
		return false
	case <-timer.C:
		return true
	}
}

type nopLogger struct{}

func (nopLogger) Infof(string, ...any)  {}
func (nopLogger) Errorf(string, ...any) {}
