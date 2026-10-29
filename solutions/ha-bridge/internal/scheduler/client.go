package scheduler

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"strings"

	"ricardo.local/monorepo/solutions/ha-bridge/internal/httpapi"
)

// maxResponseBytes bounds how much of a scheduler response is read.
const maxResponseBytes = 4 << 20

// maxAckDetail matches the proto limit for AckEventRequest.detail.
const maxAckDetail = 512

// Client speaks Connect unary JSON to SchedulerService.
type Client struct {
	baseURL string
	token   string
	http    *http.Client
}

// New returns a scheduler client. baseURL is the server base (an optional
// "/ricardo.v1.SchedulerService" suffix is tolerated). hc should carry the
// configured timeout.
func New(baseURL, token string, hc *http.Client) *Client {
	base := strings.TrimSuffix(strings.TrimSpace(baseURL), "/")
	base = strings.TrimSuffix(base, "/ricardo.v1.SchedulerService")
	return &Client{baseURL: base, token: token, http: httpapi.WithoutRedirects(hc)}
}

type listDueEventsRequest struct {
	ConsumerID   string `json:"consumerId"`
	LeaseSeconds int32  `json:"leaseSeconds"`
	Limit        int32  `json:"limit"`
	// Queue is field 4 of ListDueEventsRequest. An empty value would make the
	// server return every queue, so the client always sends it explicitly.
	Queue string `json:"queue"`
}

type eventWire struct {
	ID             string     `json:"id"`
	SourceID       string     `json:"sourceId"`
	SourceKind     protoEnum  `json:"sourceKind"`
	TaskType       string     `json:"taskType"`
	Queue          string     `json:"queue"`
	DueUnix        protoInt64 `json:"dueUnix"`
	OccurrenceUnix protoInt64 `json:"occurrenceUnix"`
	Label          string     `json:"label"`
	State          protoEnum  `json:"state"`
	Attempts       protoInt32 `json:"attempts"`
	SnoozeSeq      protoInt32 `json:"snoozeSeq"`
	LeaseUntilUnix protoInt64 `json:"leaseUntilUnix"`
	Detail         string     `json:"detail"`
	Action         protoEnum  `json:"action"`
}

type listDueEventsResponse struct {
	Events []eventWire `json:"events"`
}

// ListDueEvents leases due events for consumerID. It is never called in
// dry-run mode.
func (c *Client) ListDueEvents(ctx context.Context, req ListRequest) ([]Event, error) {
	body := listDueEventsRequest{
		ConsumerID:   req.ConsumerID,
		LeaseSeconds: int32(req.LeaseSeconds),
		Limit:        int32(req.Limit),
		Queue:        req.Queue,
	}
	var resp listDueEventsResponse
	if err := c.do(ctx, "ListDueEvents", body, &resp); err != nil {
		return nil, err
	}
	events := make([]Event, 0, len(resp.Events))
	for _, wire := range resp.Events {
		events = append(events, Event{
			ID:             wire.ID,
			SourceID:       wire.SourceID,
			SourceKind:     sourceKindOf(wire.SourceKind),
			TaskType:       wire.TaskType,
			Queue:          wire.Queue,
			DueUnix:        int64(wire.DueUnix),
			OccurrenceUnix: int64(wire.OccurrenceUnix),
			Label:          wire.Label,
			State:          enumValue(wire.State, "EVENT_STATE_", nil),
			Attempts:       int(wire.Attempts),
			SnoozeSeq:      int(wire.SnoozeSeq),
			LeaseUntilUnix: int64(wire.LeaseUntilUnix),
			Action:         actionOf(wire.Action),
			Detail:         wire.Detail,
		})
	}
	return events, nil
}

type ackEventRequest struct {
	EventID    string `json:"eventId"`
	ConsumerID string `json:"consumerId"`
	Outcome    string `json:"outcome"`
	Detail     string `json:"detail,omitempty"`
}

// AckEvent reports a terminal outcome for one event. Only DELIVERED means the
// executor accepted the event; FAILED is reserved for events that cannot be
// executed at all.
func (c *Client) AckEvent(ctx context.Context, eventID, consumerID, outcome, detail string) error {
	if eventID == "" {
		return &httpapi.Error{Op: "AckEvent", Err: errors.New("empty event id")}
	}
	if !validOutcome(outcome) {
		return &httpapi.Error{Op: "AckEvent", Err: fmt.Errorf("invalid outcome %q", outcome)}
	}
	body := ackEventRequest{
		EventID:    eventID,
		ConsumerID: consumerID,
		Outcome:    outcome,
		Detail:     truncate(detail, maxAckDetail),
	}
	return c.do(ctx, "AckEvent", body, nil)
}

func validOutcome(outcome string) bool {
	switch outcome {
	case AckDelivered, AckFailed, AckSkipped:
		return true
	default:
		return false
	}
}

type queueInfoWire struct {
	Name    string     `json:"name"`
	Pending protoInt32 `json:"pending"`
	Leased  protoInt32 `json:"leased"`
}

type consumerStatusWire struct {
	ConsumerID   string     `json:"consumerId"`
	LastSeenUnix protoInt64 `json:"lastSeenUnix"`
	LastAckUnix  protoInt64 `json:"lastAckUnix"`
	Alive        bool       `json:"alive"`
}

type statusWire struct {
	Version                    string               `json:"version"`
	SchemaVersion              protoInt32           `json:"schemaVersion"`
	NowUnix                    protoInt64           `json:"nowUnix"`
	LastTickUnix               protoInt64           `json:"lastTickUnix"`
	PendingEvents              protoInt32           `json:"pendingEvents"`
	LeasedEvents               protoInt32           `json:"leasedEvents"`
	EnabledSchedules           protoInt32           `json:"enabledSchedules"`
	RunningTimers              protoInt32           `json:"runningTimers"`
	ScenarioExecutionSupported bool                 `json:"scenarioExecutionSupported"`
	Ready                      bool                 `json:"ready"`
	Queues                     []queueInfoWire      `json:"queues"`
	Consumers                  []consumerStatusWire `json:"consumers"`
}

// GetStatus reads scheduler state without leasing anything.
func (c *Client) GetStatus(ctx context.Context) (Status, error) {
	var resp statusWire
	if err := c.do(ctx, "GetStatus", struct{}{}, &resp); err != nil {
		return Status{}, err
	}
	status := Status{
		Version:                    resp.Version,
		SchemaVersion:              int32(resp.SchemaVersion),
		NowUnix:                    int64(resp.NowUnix),
		LastTickUnix:               int64(resp.LastTickUnix),
		PendingEvents:              int32(resp.PendingEvents),
		LeasedEvents:               int32(resp.LeasedEvents),
		EnabledSchedules:           int32(resp.EnabledSchedules),
		RunningTimers:              int32(resp.RunningTimers),
		ScenarioExecutionSupported: resp.ScenarioExecutionSupported,
		Ready:                      resp.Ready,
	}
	for _, queue := range resp.Queues {
		status.Queues = append(status.Queues, QueueInfo{
			Name:    queue.Name,
			Pending: int32(queue.Pending),
			Leased:  int32(queue.Leased),
		})
	}
	for _, consumer := range resp.Consumers {
		status.Consumers = append(status.Consumers, ConsumerStatus{
			ConsumerID:   consumer.ConsumerID,
			LastSeenUnix: int64(consumer.LastSeenUnix),
			LastAckUnix:  int64(consumer.LastAckUnix),
			Alive:        consumer.Alive,
		})
	}
	return status, nil
}

func (c *Client) do(ctx context.Context, method string, requestBody, responseBody any) error {
	payload, err := json.Marshal(requestBody)
	if err != nil {
		return &httpapi.Error{Op: method, Err: fmt.Errorf("encode request: %w", err)}
	}

	req, err := http.NewRequestWithContext(ctx, http.MethodPost, c.baseURL+ServicePath+method, bytes.NewReader(payload))
	if err != nil {
		return &httpapi.Error{Op: method, Err: errors.New("build request")}
	}
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Accept", "application/json")
	req.Header.Set("Connect-Protocol-Version", "1")
	req.Header.Set("Authorization", "Bearer "+c.token)

	resp, err := c.http.Do(req)
	if err != nil {
		return &httpapi.Error{Op: method, Err: networkError(err, c.token)}
	}
	defer func() { _ = resp.Body.Close() }()

	body := io.LimitReader(resp.Body, maxResponseBytes)
	if resp.StatusCode != http.StatusOK {
		return connectError(method, resp.StatusCode, body, c.token)
	}
	if responseBody == nil {
		_, _ = io.Copy(io.Discard, body)
		return nil
	}
	if err := json.NewDecoder(body).Decode(responseBody); err != nil {
		return &httpapi.Error{
			Op:         method,
			StatusCode: resp.StatusCode,
			Err:        fmt.Errorf("decode response: %w", err),
		}
	}
	return nil
}

// networkError drops the *url.Error wrapper, whose text contains the request
// URL, and sanitizes anything the transport echoed.
func networkError(err error, token string) error {
	var urlErr *url.Error
	if errors.As(err, &urlErr) && urlErr.Err != nil {
		return errors.New(httpapi.Sanitize(urlErr.Err.Error(), token))
	}
	return errors.New(httpapi.Sanitize(err.Error(), token))
}

func connectError(op string, status int, body io.Reader, token string) error {
	var payload struct {
		Code    string `json:"code"`
		Message string `json:"message"`
	}
	_ = json.NewDecoder(body).Decode(&payload)
	return &httpapi.Error{
		Op:         op,
		StatusCode: status,
		Code:       httpapi.Sanitize(payload.Code, token),
		Message:    httpapi.Sanitize(payload.Message, token),
	}
}

func truncate(text string, limit int) string {
	text = strings.TrimSpace(text)
	if len(text) <= limit {
		return text
	}
	return text[:limit]
}
