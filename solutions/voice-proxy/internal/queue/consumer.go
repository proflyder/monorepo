package queue

import (
	"context"
	"encoding/json"
	"fmt"
	"strconv"
	"strings"
	"time"

	"ricardo.local/monorepo/packages/go/scheduler"
	"ricardo.local/monorepo/solutions/voice-proxy/internal/profile"
)

// Consumer defaults from voice_proxy.py.
const (
	DefaultLeaseSeconds = 120
	DefaultTTLSeconds   = 10800
	StalePrefix         = "Пока вас не было: "
)

// Ack outcomes and the detail the consumer reports.
const (
	OutcomeDelivered = scheduler.AckDelivered
	OutcomeSkipped   = scheduler.AckSkipped

	DetailStale     = "stale"
	DetailEmptyText = "empty_text"
	DetailIdle      = "idle_timeout"
)

// ConsumerConfig wires the queue consumer. Every dependency is a function so
// the tests can drive the loop with fakes, like the Python fakes.
type ConsumerConfig struct {
	// Profile reads the current profile fresh from HA (no cache).
	Profile func(ctx context.Context) (string, bool)
	// State reads one entity state, fresh every call.
	State func(ctx context.Context, entity string) (string, bool)
	// List leases due events from one queue.
	List func(ctx context.Context, queue, consumer string, leaseSeconds, limit int) ([]scheduler.Event, error)
	// Ack reports a terminal outcome for one event.
	Ack func(ctx context.Context, eventID, consumer, outcome, detail string) error
	// Announce speaks an already-shaped text.
	Announce func(ctx context.Context, text string) (bool, bool)

	ConsumerID  string
	Satellite   string
	MediaPlayer string
	Prefix      string

	LeaseSeconds int
	TTL          int
	// StaleAfter is how long an answer may wait before it is prefixed.
	StaleAfter time.Duration
	// StalePrefix defaults to "Пока вас не было: ".
	StalePrefix string
	Poll        time.Duration
	RetryMin    time.Duration
	RetryMax    time.Duration
	BusyGrace   time.Duration
	IdleTimeout time.Duration

	Clock  func() time.Time
	Sleep  func(ctx context.Context, d time.Duration) bool
	Logger Logger
}

// Logger is the structural log surface; speech is never logged.
type Logger interface {
	Infof(format string, args ...any)
	Errorf(format string, args ...any)
}

// Consumer leases and speaks queued announcements.
type Consumer struct {
	cfg ConsumerConfig
}

// NewConsumer builds a Consumer with Python's defaults.
func NewConsumer(cfg ConsumerConfig) *Consumer {
	if cfg.Prefix == "" {
		cfg.Prefix = DefaultPrefix
	}
	if cfg.LeaseSeconds <= 0 {
		cfg.LeaseSeconds = DefaultLeaseSeconds
	}
	if cfg.TTL < 0 {
		cfg.TTL = DefaultTTLSeconds
	}
	if cfg.StaleAfter <= 0 {
		cfg.StaleAfter = 120 * time.Second
	}
	if cfg.StalePrefix == "" {
		cfg.StalePrefix = StalePrefix
	}
	if cfg.Poll <= 0 {
		cfg.Poll = time.Second
	}
	if cfg.RetryMin <= 0 {
		cfg.RetryMin = 2 * time.Second
	}
	if cfg.RetryMax <= 0 {
		cfg.RetryMax = 60 * time.Second
	}
	if cfg.BusyGrace <= 0 {
		cfg.BusyGrace = 10 * time.Second
	}
	if cfg.IdleTimeout <= 0 {
		cfg.IdleTimeout = 180 * time.Second
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
	return &Consumer{cfg: cfg}
}

// Run speaks queued answers one at a time until ctx is cancelled. It never
// returns an error: a transient failure is logged and retried with backoff, like
// the Python daemon thread.
func (c *Consumer) Run(ctx context.Context) {
	retry := c.cfg.RetryMin
	fail := func(err error) {
		c.cfg.Logger.Infof("queue error=%s backoff=%.1fs", fmt.Sprintf("%T", err), retry.Seconds())
		c.cfg.Sleep(ctx, retry)
		retry = c.nextRetry(retry)
	}
	for ctx.Err() == nil {
		profileID, ok := c.cfg.Profile(ctx)
		if !ok || (profileID != profile.Work && profileID != profile.Personal) {
			// Unknown profile: never guess which queue to lease.
			if !c.cfg.Sleep(ctx, c.cfg.Poll) {
				return
			}
			continue
		}
		satellite, _ := c.cfg.State(ctx, c.cfg.Satellite)
		media, _ := c.cfg.State(ctx, c.cfg.MediaPlayer)
		if !StationIdle(satellite, media) {
			if !c.cfg.Sleep(ctx, c.cfg.Poll) {
				return
			}
			continue
		}
		queueName := c.queueName(profileID)
		events, err := c.cfg.List(ctx, queueName, c.cfg.ConsumerID, c.cfg.LeaseSeconds, 1)
		if err != nil {
			fail(err)
			continue
		}
		if len(events) == 0 {
			if !c.cfg.Sleep(ctx, c.cfg.Poll) {
				return
			}
			continue
		}
		event := events[0]
		if event.ID == "" {
			if !c.cfg.Sleep(ctx, c.cfg.Poll) {
				return
			}
			continue
		}
		payload := eventPayload(event.Payload)
		speech, _ := payload["text"].(string)
		created, createdKnown := eventCreatedUnix(payload, event)
		age := 0
		if createdKnown {
			age = int(c.cfg.Clock().Unix() - created)
			if age < 0 {
				age = 0
			}
		}
		if strings.TrimSpace(speech) == "" {
			if err := c.ack(ctx, event.ID, OutcomeSkipped, DetailEmptyText); err != nil {
				fail(err)
				continue
			}
			c.cfg.Logger.Infof("queue skipped queue=%s reason=empty id=%s", queueName, event.ID)
			retry = c.cfg.RetryMin
			continue
		}
		if createdKnown && age > c.cfg.TTL {
			if err := c.ack(ctx, event.ID, OutcomeSkipped, DetailStale); err != nil {
				fail(err)
				continue
			}
			c.cfg.Logger.Infof("queue stale queue=%s age=%d id=%s", queueName, age, event.ID)
			retry = c.cfg.RetryMin
			continue
		}
		if age > int(c.cfg.StaleAfter.Seconds()) {
			speech = c.cfg.StalePrefix + speech
		}
		announced, listening := c.cfg.Announce(ctx, speech)
		if !announced {
			// Leave the event leased: expiry returns it to PENDING.
			c.cfg.Logger.Infof("queue announce failed queue=%s retry=%.1fs", queueName, retry.Seconds())
			if !c.cfg.Sleep(ctx, retry) {
				return
			}
			retry = c.nextRetry(retry)
			continue
		}
		idle := c.waitUntilStationIdle(ctx)
		detail := ""
		if !idle {
			detail = DetailIdle
		}
		if err := c.ack(ctx, event.ID, OutcomeDelivered, detail); err != nil {
			fail(err)
			continue
		}
		c.cfg.Logger.Infof("queue delivered queue=%s age=%d listen=%t idle=%t",
			queueName, age, listening, idle)
		retry = c.cfg.RetryMin
	}
}

func (c *Consumer) ack(ctx context.Context, eventID, outcome, detail string) error {
	return c.cfg.Ack(ctx, eventID, c.cfg.ConsumerID, outcome, detail)
}

func (c *Consumer) queueName(profileID string) string { return c.cfg.Prefix + profileID }

func (c *Consumer) nextRetry(current time.Duration) time.Duration {
	next := current * 2
	if next > c.cfg.RetryMax {
		return c.cfg.RetryMax
	}
	return next
}

// waitUntilStationIdle waits for an issued announcement to finish; true when the
// station is idle again. It first gives the station up to BusyGrace seconds to
// report busy (HA service calls are asynchronous), then waits for idle with the
// overall IdleTimeout budget.
func (c *Consumer) waitUntilStationIdle(ctx context.Context) bool {
	start := c.cfg.Clock()
	busySeen := false
	for ctx.Err() == nil {
		elapsed := c.cfg.Clock().Sub(start)
		if elapsed >= c.cfg.IdleTimeout {
			return false
		}
		satellite, _ := c.cfg.State(ctx, c.cfg.Satellite)
		media, _ := c.cfg.State(ctx, c.cfg.MediaPlayer)
		idle := StationIdle(satellite, media)
		if !idle {
			busySeen = true
		} else if busySeen || elapsed >= c.cfg.BusyGrace {
			return true
		}
		if !c.cfg.Sleep(ctx, c.cfg.Poll) {
			return false
		}
	}
	return false
}

// StationIdle reports whether the speaker is free for a queued announcement.
// Unknown or unavailable state is treated as busy: speaking into an unknown
// station would interrupt a dialog or another announcement. Music "playing" is
// fine; ANNOUNCING is the HA announcement channel.
func StationIdle(satellite, media string) bool {
	if satellite == "" || satellite == "unknown" || satellite == "unavailable" {
		return false
	}
	if satellite != "idle" {
		return false
	}
	if media == "" || media == "unknown" || media == "unavailable" {
		return false
	}
	return media != "announcing"
}

// eventPayload parses payloadJson; an invalid or non-object payload is empty,
// exactly like _event_payload.
func eventPayload(raw string) map[string]any {
	if raw == "" {
		return map[string]any{}
	}
	dec := json.NewDecoder(strings.NewReader(raw))
	dec.UseNumber()
	var value any
	if err := dec.Decode(&value); err != nil {
		return map[string]any{}
	}
	object, ok := value.(map[string]any)
	if !ok {
		return map[string]any{}
	}
	return object
}

// eventCreatedUnix prefers payload.created_unix over event.createdUnix, like
// _event_created_unix. A JSON bool is not a timestamp. A zero createdUnix is
// what the client decodes for an absent field: it is unknown (Python's None),
// never the epoch, so a missing timestamp cannot silently expire an answer.
func eventCreatedUnix(payload map[string]any, event scheduler.Event) (int64, bool) {
	if value, ok := intValue(payload["created_unix"]); ok {
		return value, true
	}
	if event.CreatedUnix > 0 {
		return event.CreatedUnix, true
	}
	return 0, false
}

// intValue accepts every numeric shape a JSON decoder can produce, plus a
// decimal string (the Connect int64 wire form).
func intValue(value any) (int64, bool) {
	switch number := value.(type) {
	case json.Number:
		if parsed, err := number.Int64(); err == nil {
			return parsed, true
		}
	case float64:
		return int64(number), true
	case int:
		return int64(number), true
	case int64:
		return number, true
	case string:
		if parsed, err := strconv.ParseInt(number, 10, 64); err == nil {
			return parsed, true
		}
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
