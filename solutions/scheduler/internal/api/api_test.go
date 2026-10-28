package api

import (
	"bytes"
	"context"
	"crypto/tls"
	"encoding/binary"
	"encoding/json"
	"io"
	"net"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"connectrpc.com/connect"
	"golang.org/x/net/http2"
	"google.golang.org/protobuf/proto"

	ricardov1 "ricardo.local/monorepo/packages/proto/ricardo/v1"
	ricardov1connect "ricardo.local/monorepo/packages/proto/ricardo/v1/ricardov1connect"
	"ricardo.local/monorepo/solutions/scheduler/internal/engine"
	"ricardo.local/monorepo/solutions/scheduler/internal/store"
)

const testToken = "test-token"

type fixture struct {
	st          *store.Store
	srv         *httptest.Server
	client      ricardov1connect.SchedulerServiceClient
	alarmClient ricardov1connect.AlarmServiceClient
	now         time.Time
}

func newFixture(t *testing.T, allowed ...string) *fixture {
	t.Helper()
	path := t.TempDir() + "/api.db"
	cfg := store.DefaultConfig()
	cfg.MaxAttempts = 2
	st, err := store.Open(context.Background(), path, cfg)
	if err != nil {
		t.Fatalf("open store: %v", err)
	}
	f := &fixture{st: st, now: time.Date(2026, 1, 1, 12, 0, 0, 0, time.UTC)}
	svc := NewService(Options{
		Store:            st,
		Now:              func() time.Time { return f.now },
		Version:          "test",
		Status:           func() engine.Status { return engine.Status{Ready: true} },
		AllowedConsumers: allowed,
	})
	f.srv = httptest.NewServer(NewHTTPHandler(svc, NewAlarmService(svc), testToken))
	httpClient := f.srv.Client()
	f.client = ricardov1connect.NewSchedulerServiceClient(httpClient, f.srv.URL)
	f.alarmClient = ricardov1connect.NewAlarmServiceClient(httpClient, f.srv.URL)
	t.Cleanup(func() {
		f.srv.Close()
		_ = st.Close()
	})
	return f
}

func authorized[T any](req *connect.Request[T]) *connect.Request[T] {
	req.Header().Set("Authorization", "Bearer "+testToken)
	return req
}

func TestAuthRejectsMissingAndWrongBearer(t *testing.T) {
	f := newFixture(t)
	ctx := context.Background()

	_, err := f.client.GetStatus(ctx, connect.NewRequest(&ricardov1.GetStatusRequest{}))
	if connect.CodeOf(err) != connect.CodeUnauthenticated {
		t.Fatalf("missing token error = %v, want UNAUTHENTICATED", err)
	}
	req := connect.NewRequest(&ricardov1.GetStatusRequest{})
	req.Header().Set("Authorization", "Bearer wrong")
	if _, err := f.client.GetStatus(ctx, req); connect.CodeOf(err) != connect.CodeUnauthenticated {
		t.Fatalf("wrong token error = %v, want UNAUTHENTICATED", err)
	}
	resp, err := f.client.GetStatus(ctx, authorized(connect.NewRequest(&ricardov1.GetStatusRequest{})))
	if err != nil {
		t.Fatalf("authorized status: %v", err)
	}
	if resp.Msg.GetNowUnix() != f.now.Unix() || !resp.Msg.GetReady() {
		t.Fatalf("status = %+v", resp.Msg)
	}
	if resp.Msg.GetScenarioExecutionSupported() {
		t.Fatalf("scenario execution advertised without an executor")
	}
}

func TestConnectJSONEndToEnd(t *testing.T) {
	f := newFixture(t)
	body := `{"label":"gym","kind":"SCHEDULE_KIND_ALARM","timezone":"UTC","at":{"hour":7,"minute":30},"repeat":"REPEAT_KIND_DAILY","enabled":true}`

	resp := f.postJSON(t, ricardov1connect.SchedulerServiceCreateScheduleProcedure, body)
	defer func() { _ = resp.Body.Close() }()
	if resp.StatusCode != http.StatusOK {
		raw, _ := io.ReadAll(resp.Body)
		t.Fatalf("create status = %d: %s", resp.StatusCode, raw)
	}
	var created map[string]any
	if err := json.NewDecoder(resp.Body).Decode(&created); err != nil {
		t.Fatalf("decode create: %v", err)
	}
	schedule, ok := created["schedule"].(map[string]any)
	if !ok {
		t.Fatalf("create response = %v", created)
	}
	if schedule["id"] == "" || schedule["label"] != "gym" {
		t.Fatalf("schedule = %v", schedule)
	}
	// int64 is a decimal string in protobuf JSON.
	nextDue, ok := schedule["nextDueUnix"].(string)
	if !ok || nextDue == "" || nextDue == "0" {
		t.Fatalf("nextDueUnix = %#v, want non-zero decimal string", schedule["nextDueUnix"])
	}

	if _, err := f.client.EnqueueTask(context.Background(), authorized(connect.NewRequest(&ricardov1.EnqueueTaskRequest{
		TaskType: "notify.push", PayloadJson: "{}",
	}))); err != nil {
		t.Fatalf("enqueue: %v", err)
	}

	statusResp := f.postJSON(t, ricardov1connect.SchedulerServiceGetStatusProcedure, `{}`)
	defer func() { _ = statusResp.Body.Close() }()
	var status map[string]any
	if err := json.NewDecoder(statusResp.Body).Decode(&status); err != nil {
		t.Fatalf("decode status: %v", err)
	}
	if _, ok := status["nowUnix"].(string); !ok {
		t.Fatalf("nowUnix = %#v, want decimal string", status["nowUnix"])
	}
	if enabled, ok := status["enabledSchedules"].(float64); !ok || enabled != 1 {
		t.Fatalf("enabledSchedules = %#v, want 1", status["enabledSchedules"])
	}
	// Protobuf JSON omits default values, so false/zero fields may be absent.
	if v, ok := status["scenarioExecutionSupported"]; ok && v != false {
		t.Fatalf("scenarioExecutionSupported = %#v", v)
	}
	queues, ok := status["queues"].([]any)
	if !ok || len(queues) == 0 {
		t.Fatalf("status missing queue discovery: %v", status)
	}

	list := f.postJSON(t, ricardov1connect.SchedulerServiceListSchedulesProcedure, `{"includeDisabled":true}`)
	defer func() { _ = list.Body.Close() }()
	var listed map[string]any
	if err := json.NewDecoder(list.Body).Decode(&listed); err != nil {
		t.Fatalf("decode list: %v", err)
	}
	items, _ := listed["schedules"].([]any)
	if len(items) != 1 {
		t.Fatalf("schedules = %v", listed)
	}
}

func (f *fixture) postJSON(t *testing.T, procedure, body string) *http.Response {
	t.Helper()
	req, err := http.NewRequest(http.MethodPost, f.srv.URL+procedure, strings.NewReader(body))
	if err != nil {
		t.Fatalf("new request: %v", err)
	}
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Authorization", "Bearer "+testToken)
	resp, err := f.srv.Client().Do(req)
	if err != nil {
		t.Fatalf("post %s: %v", procedure, err)
	}
	return resp
}

func TestGRPCOverH2C(t *testing.T) {
	f := newFixture(t)
	host := strings.TrimPrefix(f.srv.URL, "http://")
	transport := &http2.Transport{
		AllowHTTP: true,
		DialTLSContext: func(ctx context.Context, network, addr string, _ *tls.Config) (net.Conn, error) {
			var d net.Dialer
			return d.DialContext(ctx, network, host)
		},
	}
	client := &http.Client{Transport: transport}

	payload, err := proto.Marshal(&ricardov1.ListSchedulesRequest{IncludeDisabled: true})
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	frame := make([]byte, 5+len(payload))
	binary.BigEndian.PutUint32(frame[1:5], uint32(len(payload)))
	copy(frame[5:], payload)

	req, err := http.NewRequest(http.MethodPost, f.srv.URL+"/ricardo.v1.SchedulerService/ListSchedules", bytes.NewReader(frame))
	if err != nil {
		t.Fatalf("request: %v", err)
	}
	req.Header.Set("Content-Type", "application/grpc")
	req.Header.Set("TE", "trailers")
	req.Header.Set("Authorization", "Bearer "+testToken)
	resp, err := client.Do(req)
	if err != nil {
		t.Fatalf("grpc request: %v", err)
	}
	defer func() { _ = resp.Body.Close() }()
	raw, err := io.ReadAll(resp.Body)
	if err != nil {
		t.Fatalf("read grpc body: %v", err)
	}
	if resp.Trailer.Get("Grpc-Status") != "0" || len(raw) < 5 {
		t.Fatalf("grpc status = %q body=%d", resp.Trailer.Get("Grpc-Status"), len(raw))
	}
	var out ricardov1.ListSchedulesResponse
	if err := proto.Unmarshal(raw[5:], &out); err != nil {
		t.Fatalf("unmarshal grpc response: %v", err)
	}
}

func TestConsumerAllowlist(t *testing.T) {
	f := newFixture(t, "home-1")
	_, err := f.client.ListDueEvents(context.Background(), authorized(connect.NewRequest(&ricardov1.ListDueEventsRequest{ConsumerId: "other"})))
	if connect.CodeOf(err) != connect.CodePermissionDenied {
		t.Fatalf("allowlist error = %v, want PERMISSION_DENIED", err)
	}
	if _, err := f.client.ListDueEvents(context.Background(), authorized(connect.NewRequest(&ricardov1.ListDueEventsRequest{ConsumerId: "home-1"}))); err != nil {
		t.Fatalf("allowed consumer failed: %v", err)
	}
}

func TestBodyLimit(t *testing.T) {
	f := newFixture(t)
	big := `{"consumerId":"` + strings.Repeat("a", MaxBodyBytes+10) + `"}`
	resp := f.postJSON(t, ricardov1connect.SchedulerServiceListDueEventsProcedure, big)
	defer func() { _ = resp.Body.Close() }()
	if resp.StatusCode == http.StatusOK {
		t.Fatalf("oversized request accepted")
	}
}

func TestLegacyAlarmFacadeEndToEnd(t *testing.T) {
	f := newFixture(t)
	ctx := context.Background()

	created, err := f.alarmClient.CreateAlarm(ctx, authorized(connect.NewRequest(&ricardov1.CreateAlarmRequest{
		Label: "wake", Hour: 7, Minute: 30, Timezone: "UTC",
		Repeat: ricardov1.Repeat_DAILY, Enabled: true,
	})))
	if err != nil {
		t.Fatalf("CreateAlarm: %v", err)
	}
	alarm := created.Msg.GetAlarm()
	if alarm.GetId() == "" || alarm.GetNextRunUnix() == 0 || alarm.GetRevision() != 1 {
		t.Fatalf("created alarm = %+v", alarm)
	}
	listed, err := f.alarmClient.ListAlarms(ctx, authorized(connect.NewRequest(&ricardov1.ListAlarmsRequest{})))
	if err != nil || len(listed.Msg.GetAlarms()) != 1 {
		t.Fatalf("ListAlarms = %+v, %v", listed, err)
	}

	// Fire the occurrence through the shared engine, then lease and ack it.
	f.now = time.Unix(alarm.GetNextRunUnix(), 0).Add(time.Second)
	if _, err := f.st.Tick(ctx, f.now); err != nil {
		t.Fatalf("tick: %v", err)
	}
	leased, err := f.client.ListDueEvents(ctx, authorized(connect.NewRequest(&ricardov1.ListDueEventsRequest{
		ConsumerId: "home-1", Queue: "home",
	})))
	if err != nil || len(leased.Msg.GetEvents()) != 1 {
		t.Fatalf("lease = %+v, %v", leased, err)
	}
	if _, err := f.client.AckEvent(ctx, authorized(connect.NewRequest(&ricardov1.AckEventRequest{
		EventId: leased.Msg.GetEvents()[0].GetId(), ConsumerId: "home-1",
		Outcome: ricardov1.AckOutcome_ACK_OUTCOME_DELIVERED,
	}))); err != nil {
		t.Fatalf("AckEvent: %v", err)
	}

	snoozed, err := f.alarmClient.SnoozeAlarm(ctx, authorized(connect.NewRequest(&ricardov1.SnoozeAlarmRequest{Id: alarm.GetId(), Minutes: 9})))
	if err != nil {
		t.Fatalf("SnoozeAlarm after ack: %v", err)
	}
	if snoozed.Msg.GetAlarm().GetSnoozeUntilUnix() == 0 {
		t.Fatalf("snooze not recorded: %+v", snoozed.Msg.GetAlarm())
	}
	stopped, err := f.alarmClient.StopAlarm(ctx, authorized(connect.NewRequest(&ricardov1.StopAlarmRequest{Id: alarm.GetId()})))
	if err != nil || stopped.Msg.GetAlarm().GetSnoozeUntilUnix() != 0 {
		t.Fatalf("StopAlarm = %+v, %v", stopped, err)
	}
	if _, err := f.alarmClient.SnoozeAlarm(ctx, authorized(connect.NewRequest(&ricardov1.SnoozeAlarmRequest{Id: alarm.GetId()}))); connect.CodeOf(err) != connect.CodeFailedPrecondition {
		t.Fatalf("snooze after stop error = %v, want FAILED_PRECONDITION", err)
	}

	// UpdateAlarm honors Alarm.revision.
	alarm.Hour = 8
	alarm.Revision = stopped.Msg.GetAlarm().GetRevision()
	updated, err := f.alarmClient.UpdateAlarm(ctx, authorized(connect.NewRequest(&ricardov1.UpdateAlarmRequest{Alarm: alarm})))
	if err != nil || updated.Msg.GetAlarm().GetHour() != 8 {
		t.Fatalf("UpdateAlarm = %+v, %v", updated, err)
	}
	stale := proto.Clone(alarm).(*ricardov1.Alarm)
	stale.Revision = alarm.GetRevision() - 1
	if _, err := f.alarmClient.UpdateAlarm(ctx, authorized(connect.NewRequest(&ricardov1.UpdateAlarmRequest{Alarm: stale}))); connect.CodeOf(err) != connect.CodeAborted {
		t.Fatalf("stale alarm update error = %v, want ABORTED", err)
	}
}

func TestTaskAPI(t *testing.T) {
	f := newFixture(t)
	ctx := context.Background()
	enqueued, err := f.client.EnqueueTask(ctx, authorized(connect.NewRequest(&ricardov1.EnqueueTaskRequest{
		TaskType: "notify.push", PayloadJson: `{"text":"hi"}`, Queue: "tasks", IdempotencyKey: "k1",
	})))
	if err != nil {
		t.Fatalf("EnqueueTask: %v", err)
	}
	ev := enqueued.Msg.GetEvent()
	if ev.GetTaskType() != "notify.push" || ev.GetQueue() != "tasks" || ev.GetSourceKind() != ricardov1.SourceKind_SOURCE_KIND_TASK {
		t.Fatalf("event = %+v", ev)
	}
	home, err := f.client.ListDueEvents(ctx, authorized(connect.NewRequest(&ricardov1.ListDueEventsRequest{ConsumerId: "home-1", Queue: "home"})))
	if err != nil || len(home.Msg.GetEvents()) != 0 {
		t.Fatalf("home leased generic task: %+v, %v", home, err)
	}
	cancelled, err := f.client.CancelTask(ctx, authorized(connect.NewRequest(&ricardov1.CancelTaskRequest{EventId: ev.GetId()})))
	if err != nil || cancelled.Msg.GetEvent().GetState() != ricardov1.EventState_EVENT_STATE_SKIPPED {
		t.Fatalf("CancelTask = %+v, %v", cancelled, err)
	}
}

func TestAckViaAPIRequiresLease(t *testing.T) {
	f := newFixture(t)
	ctx := context.Background()
	enqueued, err := f.client.EnqueueTask(ctx, authorized(connect.NewRequest(&ricardov1.EnqueueTaskRequest{
		TaskType: "notify.push", PayloadJson: "{}",
	})))
	if err != nil {
		t.Fatalf("enqueue: %v", err)
	}
	_, err = f.client.AckEvent(ctx, authorized(connect.NewRequest(&ricardov1.AckEventRequest{
		EventId: enqueued.Msg.GetEvent().GetId(), ConsumerId: "home-1",
		Outcome: ricardov1.AckOutcome_ACK_OUTCOME_DELIVERED,
	})))
	if connect.CodeOf(err) != connect.CodeFailedPrecondition {
		t.Fatalf("unleased ack error = %v, want FAILED_PRECONDITION", err)
	}
}
