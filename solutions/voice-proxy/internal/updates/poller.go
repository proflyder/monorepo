// Package updates announces turns the owner did not start by voice: child
// completions and dispatcher messages.
//
// It is a port of _updates_poller(), _enqueue_updates(), _updates_cursor() and
// _updates_spoken() in voice_proxy.py. Two modes exist:
//
//   - with the durable queue every non-owner turn is enqueued for the work
//     profile, regardless of the current speaker profile; the queue consumer
//     waits for the work profile and an idle station. A restart rescans a recent
//     window and the idempotency key drops duplicates;
//   - without the queue the previous behaviour is unchanged: advance the cursor,
//     re-read the profile and speak only in the work profile, joining the batch
//     into one utterance.
//
// Errors back off 2 s -> 60 s and never speak or reach Hermes.
package updates

import (
	"context"
	"encoding/json"
	"fmt"
	"strconv"
	"strings"
	"time"

	"ricardo.local/monorepo/solutions/voice-proxy/internal/profile"
	"ricardo.local/monorepo/solutions/voice-proxy/internal/sanitize"
)

// Logger is the structural log surface; spoken text is never logged.
type Logger interface {
	Infof(format string, args ...any)
	Errorf(format string, args ...any)
}

// EnqueueFunc stores one durable announcement for one profile.
type EnqueueFunc func(ctx context.Context, speech, profileID, threadID, seq string, createdUnix *int64) bool

// Config configures a Poller.
type Config struct {
	// Fetch long-polls /v1/voice/updates. A nil afterSeq bootstraps without
	// history.
	Fetch func(ctx context.Context, afterSeq *int64, waitSeconds int) (map[string]any, error)
	// Profile reads the profile fresh from HA for the non-queue mode.
	Profile func(ctx context.Context) (string, bool)
	// Announce speaks an already-shaped text.
	Announce func(ctx context.Context, text string) (bool, bool)
	// Enqueue stores a durable announcement; used only when QueueEnabled.
	Enqueue EnqueueFunc
	// QueueEnabled decides which mode runs.
	QueueEnabled func() bool

	// Rescan is HERMES_VOICE_UPDATES_RESCAN (300).
	Rescan int
	// Wait is HERMES_VOICE_RESULT_WAIT (25).
	Wait int
	// MaxChars is HERMES_VOICE_ANNOUNCE_MAX_CHARS.
	MaxChars int
	// TTL is HERMES_VOICE_ANNOUNCE_TTL for stale updates.
	TTL int

	RetryMin  time.Duration
	RetryMax  time.Duration
	IdlePause time.Duration
	Clock     func() time.Time
	Sleep     func(ctx context.Context, d time.Duration) bool
	Logger    Logger
}

// Poller long-polls dispatcher updates and announces or enqueues them.
type Poller struct {
	cfg Config
}

// New builds a Poller with the Python defaults.
func New(cfg Config) *Poller {
	if cfg.Rescan <= 0 {
		cfg.Rescan = 300
	}
	if cfg.Wait <= 0 {
		cfg.Wait = 25
	}
	if cfg.MaxChars <= 0 {
		cfg.MaxChars = 200
	}
	if cfg.TTL < 0 {
		cfg.TTL = 10800
	}
	if cfg.RetryMin <= 0 {
		cfg.RetryMin = 2 * time.Second
	}
	if cfg.RetryMax <= 0 {
		cfg.RetryMax = 60 * time.Second
	}
	if cfg.IdlePause <= 0 {
		cfg.IdlePause = 100 * time.Millisecond
	}
	if cfg.Clock == nil {
		cfg.Clock = time.Now
	}
	if cfg.Sleep == nil {
		cfg.Sleep = sleepContext
	}
	if cfg.Logger == nil {
		cfg.Logger = nopLogger{}
	}
	return &Poller{cfg: cfg}
}

// Run polls until ctx is cancelled. It never returns: the poller must never die.
func (p *Poller) Run(ctx context.Context) {
	queued := p.cfg.QueueEnabled != nil && p.cfg.QueueEnabled()
	var cursor *int64
	retry := p.cfg.RetryMin
	for ctx.Err() == nil {
		if cursor == nil {
			response, err := p.fetch(ctx, nil, 1)
			if err != nil {
				retry = p.backoff(ctx, err, retry)
				continue
			}
			next, ok := updatesCursor(response)
			if !ok {
				retry = p.backoff(ctx, fmt.Errorf("updates bootstrap has no cursor"), retry)
				continue
			}
			if queued {
				rescan := int64(p.cfg.Rescan)
				from := next - rescan
				if from < 0 {
					from = 0
				}
				cursor = &from
				p.cfg.Logger.Infof("updates bootstrap cursor=%d rescan_from=%d", next, from)
			} else {
				value := next
				cursor = &value
				p.cfg.Logger.Infof("updates bootstrap cursor=%d", next)
			}
			retry = p.cfg.RetryMin
			if !p.cfg.Sleep(ctx, p.cfg.IdlePause) {
				return
			}
			continue
		}
		response, err := p.fetch(ctx, cursor, p.cfg.Wait)
		if err != nil {
			retry = p.backoff(ctx, err, retry)
			continue
		}
		next, ok := updatesCursor(response)
		if !ok {
			retry = p.backoff(ctx, fmt.Errorf("updates response has no cursor"), retry)
			continue
		}
		cursor = &next
		retry = p.cfg.RetryMin
		items, _ := response["items"].([]any)
		if len(items) > 0 {
			if queued {
				enqueued, stale := enqueueUpdates(ctx, items, p.cfg.Enqueue, p.cfg.Clock(), p.cfg.TTL, p.cfg.MaxChars)
				p.cfg.Logger.Infof("updates items=%d queued=%d stale=%d cursor=%d",
					len(items), enqueued, stale, next)
			} else {
				// Work profile only: read fresh from HA, no cache.
				profileID, ok := p.profile(ctx)
				spoken := updatesSpoken(items)
				announced, listening := false, false
				if ok && profileID == profile.Work && len(spoken) > 0 {
					text := sanitize.AnnounceText(strings.Join(spoken, " "), p.cfg.MaxChars)
					if text != "" && p.cfg.Announce != nil {
						announced, listening = p.cfg.Announce(ctx, text)
					}
				}
				display := "unknown"
				if ok {
					display = profileID
				}
				p.cfg.Logger.Infof("updates items=%d announced=%t listen=%t profile=%s cursor=%d",
					len(items), announced, listening, display, next)
			}
		}
		if !p.cfg.Sleep(ctx, p.cfg.IdlePause) {
			return
		}
	}
}

func (p *Poller) fetch(ctx context.Context, afterSeq *int64, wait int) (map[string]any, error) {
	if p.cfg.Fetch == nil {
		return nil, fmt.Errorf("updates fetch is not configured")
	}
	return p.cfg.Fetch(ctx, afterSeq, wait)
}

func (p *Poller) profile(ctx context.Context) (string, bool) {
	if p.cfg.Profile == nil {
		return "", false
	}
	return p.cfg.Profile(ctx)
}

func (p *Poller) backoff(ctx context.Context, err error, current time.Duration) time.Duration {
	p.cfg.Logger.Infof("updates error=%s backoff=%.1fs", fmt.Sprintf("%T", err), current.Seconds())
	p.cfg.Sleep(ctx, current)
	next := current * 2
	if next > p.cfg.RetryMax {
		return p.cfg.RetryMax
	}
	return next
}

// enqueueUpdates enqueues each non-owner update and returns
// (queued, skipped_stale).
func enqueueUpdates(ctx context.Context, items []any, enqueue EnqueueFunc, now time.Time, ttl, maxChars int) (int, int) {
	queued, stale := 0, 0
	for _, item := range items {
		entry, ok := item.(map[string]any)
		if !ok {
			continue
		}
		speech, ok := entry["spoken"].(string)
		if !ok || strings.TrimSpace(speech) == "" {
			continue
		}
		created, createdKnown := updateCreatedUnix(entry)
		if createdKnown && now.Unix()-created > int64(ttl) {
			stale++
			continue
		}
		text := sanitize.AnnounceText(strings.TrimSpace(speech), maxChars)
		if text == "" {
			continue
		}
		seqKey := "unknown"
		if seq, ok := intValue(entry["seq"]); ok {
			seqKey = strconv.FormatInt(seq, 10)
		}
		var createdPtr *int64
		if createdKnown {
			value := created
			createdPtr = &value
		}
		if enqueue != nil && enqueue(ctx, text, profile.Work, "dispatcher", seqKey, createdPtr) {
			queued++
		}
	}
	return queued, stale
}

// updatesCursor reads a non-negative integer cursor; bool, negative and missing
// values are invalid.
func updatesCursor(response map[string]any) (int64, bool) {
	value, ok := intValue(response["cursor"])
	if !ok || value < 0 {
		return 0, false
	}
	return value, true
}

// updatesSpoken returns the trimmed non-empty "spoken" texts of the batch.
func updatesSpoken(items []any) []string {
	var spoken []string
	for _, item := range items {
		entry, ok := item.(map[string]any)
		if !ok {
			continue
		}
		text, ok := entry["spoken"].(string)
		if ok && strings.TrimSpace(text) != "" {
			spoken = append(spoken, strings.TrimSpace(text))
		}
	}
	return spoken
}

// updateCreatedUnix is the completion time of one updates item in epoch seconds.
// Millisecond values are converted; <=0 and non-numeric values are unknown.
func updateCreatedUnix(item map[string]any) (int64, bool) {
	value := item["completedAt"]
	if _, isBool := value.(bool); isBool {
		return 0, false
	}
	var number int64
	switch typed := value.(type) {
	case json.Number:
		parsed, err := typed.Int64()
		if err != nil {
			asFloat, ferr := typed.Float64()
			if ferr != nil {
				return 0, false
			}
			number = int64(asFloat)
		} else {
			number = parsed
		}
	case float64:
		number = int64(typed)
	case int64:
		number = typed
	case int:
		number = int64(typed)
	default:
		return 0, false
	}
	if number <= 0 {
		return 0, false
	}
	if number > 1_000_000_000_000 {
		return number / 1000, true
	}
	return number, true
}

// intValue accepts every numeric shape a JSON decoder can produce.
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
