// Package queue is the durable announcement queue: a producer that stores a
// spoken answer as a "voice.announce" task and a consumer that speaks it only
// when the station is idle.
//
// The wire contract is shared with the Python proxy during the cutover, so the
// payload keys, task type, queue names, idempotency keys and ack outcomes are
// byte-for-byte compatible:
//
//	payloadJson: {"text","profile","thread_id","seq","created_unix"}
//	queue:       voice-<profile>
//	key:         voice:<thread_id>:<seq>
//
// Python and Go may drain each other's events after a rollback.
package queue

import (
	"bytes"
	"context"
	"encoding/json"
	"strings"
	"time"

	"ricardo.local/monorepo/packages/go/scheduler"
	"ricardo.local/monorepo/solutions/voice-proxy/internal/profile"
)

// Defaults from voice_proxy.py.
const (
	DefaultPrefix   = "voice-"
	DefaultTaskType = "voice.announce"
)

// AnnouncePayload is the voice.announce payload. Field order is the Python
// insertion order, so a Go-produced payload is textually familiar too.
type AnnouncePayload struct {
	Text        string `json:"text"`
	Profile     string `json:"profile"`
	ThreadID    string `json:"thread_id"`
	Seq         string `json:"seq"`
	CreatedUnix int64  `json:"created_unix"`
}

// Producer enqueues durable announcements.
type Producer struct {
	Client *scheduler.Client
	// Enabled reports whether the queue is configured and switched on.
	Enabled func() bool
	Prefix  string
	// TaskType defaults to "voice.announce".
	TaskType string
	Clock    func() time.Time
}

// NewProducer builds a Producer.
func NewProducer(client *scheduler.Client, enabled func() bool) *Producer {
	return &Producer{Client: client, Enabled: enabled, Prefix: DefaultPrefix, TaskType: DefaultTaskType, Clock: time.Now}
}

// QueueName is the scheduler queue for one profile.
func (p *Producer) QueueName(profileID string) string {
	prefix := p.Prefix
	if prefix == "" {
		prefix = DefaultPrefix
	}
	return prefix + profileID
}

// Enqueue stores one spoken answer and returns false when the queue is off, the
// text or profile is unusable, or the scheduler is unreachable. The caller then
// falls back to a direct announcement.
func (p *Producer) Enqueue(ctx context.Context, speech, profileID, threadID, seq string, createdUnix *int64) bool {
	if p == nil || p.Client == nil || p.Enabled == nil || !p.Enabled() {
		return false
	}
	if speech == "" || (profileID != profile.Work && profileID != profile.Personal) {
		return false
	}
	taskType := p.TaskType
	if taskType == "" {
		taskType = DefaultTaskType
	}
	clock := p.Clock
	if clock == nil {
		clock = time.Now
	}
	created := clock().Unix()
	if createdUnix != nil {
		created = *createdUnix
	}
	payload, err := encodePayload(AnnouncePayload{
		Text:        speech,
		Profile:     profileID,
		ThreadID:    threadID,
		Seq:         seq,
		CreatedUnix: created,
	})
	if err != nil {
		return false
	}
	_, err = p.Client.EnqueueTask(ctx, scheduler.EnqueueRequest{
		TaskType:       taskType,
		PayloadJSON:    payload,
		RunAtUnix:      clock().Unix(),
		Queue:          p.QueueName(profileID),
		IdempotencyKey: "voice:" + threadID + ":" + seq,
	})
	return err == nil
}

// encodePayload is json.dumps(..., ensure_ascii=False): UTF-8, no HTML escaping
// and no trailing newline.
func encodePayload(payload AnnouncePayload) (string, error) {
	var buf bytes.Buffer
	enc := json.NewEncoder(&buf)
	enc.SetEscapeHTML(false)
	if err := enc.Encode(payload); err != nil {
		return "", err
	}
	return strings.TrimSuffix(buf.String(), "\n"), nil
}
