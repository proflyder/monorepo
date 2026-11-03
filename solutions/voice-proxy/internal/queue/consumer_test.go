package queue_test

import (
	"context"
	"encoding/json"
	"testing"
	"time"

	"ricardo.local/monorepo/packages/go/scheduler"
	"ricardo.local/monorepo/solutions/voice-proxy/internal/queue"
)

const (
	satellite = "assist_satellite.station"
	media     = "media_player.station"
)

type leaseCall struct {
	queue    string
	consumer string
	lease    int
	limit    int
}

type ackCall struct {
	eventID string
	outcome string
	detail  string
}

type consumerCalls struct {
	leases    []leaseCall
	acks      []ackCall
	announced []string
	states    int
}

type driveOptions struct {
	profile     string
	stateReader func(entity string, call int) string
	events      []scheduler.Event
	// announceResult defaults to (true, false) when nil.
	announceResult *[2]bool
	limit          int
}

func (o *driveOptions) value(index int) bool {
	if o.announceResult == nil {
		if index == 0 {
			return true
		}
		return false
	}
	return o.announceResult[index]
}

func drive(t *testing.T, opts driveOptions) consumerCalls {
	t.Helper()
	profileID := opts.profile
	if profileID == "" {
		profileID = "work"
	}
	limit := opts.limit
	if limit == 0 {
		limit = 6
	}
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	clock := time.Unix(1_700_000_000, 0)
	sleeps := 0
	events := append([]scheduler.Event(nil), opts.events...)
	var calls consumerCalls

	sleep := func(_ context.Context, d time.Duration) bool {
		sleeps++
		clock = clock.Add(d)
		if sleeps >= limit {
			cancel()
			return false
		}
		return true
	}
	state := func(_ context.Context, entity string) (string, bool) {
		calls.states++
		if opts.stateReader != nil {
			return opts.stateReader(entity, calls.states), true
		}
		if entity == satellite {
			return "idle", true
		}
		return "playing", true
	}
	list := func(_ context.Context, queueName, consumer string, leaseSeconds, count int) ([]scheduler.Event, error) {
		calls.leases = append(calls.leases, leaseCall{queueName, consumer, leaseSeconds, count})
		if len(events) == 0 {
			return nil, nil
		}
		event := events[0]
		events = events[1:]
		return []scheduler.Event{event}, nil
	}
	ack := func(_ context.Context, eventID, consumer, outcome, detail string) error {
		calls.acks = append(calls.acks, ackCall{eventID, outcome, detail})
		return nil
	}
	announce := func(_ context.Context, text string) (bool, bool) {
		calls.announced = append(calls.announced, text)
		return opts.value(0), opts.value(1)
	}

	consumer := queue.NewConsumer(queue.ConsumerConfig{
		Profile:     func(context.Context) (string, bool) { return profileID, true },
		State:       state,
		List:        list,
		Ack:         ack,
		Announce:    announce,
		ConsumerID:  "voice-proxy-home",
		Satellite:   satellite,
		MediaPlayer: media,
		TTL:         10800,
		Clock:       func() time.Time { return clock },
		Sleep:       sleep,
	})
	consumer.Run(ctx)
	return calls
}

func event(created *int64, text string) scheduler.Event {
	payload := map[string]any{"text": text, "profile": "work", "thread_id": "thr_example", "seq": "1"}
	if created != nil {
		payload["created_unix"] = *created
	}
	raw, _ := json.Marshal(payload)
	return scheduler.Event{ID: "ev_1", Payload: string(raw)}
}

func int64ptr(value int64) *int64 { return &value }

// TestBusyOrUnknownStationNeverLeases mirrors
// test_busy_or_unknown_station_never_leases.
func TestBusyOrUnknownStationNeverLeases(t *testing.T) {
	cases := [][2]string{
		{"listening", "playing"},
		{"processing", "playing"},
		{"", "playing"},
		{"idle", ""},
		{"idle", "announcing"},
	}
	for _, states := range cases {
		t.Run(states[0]+"/"+states[1], func(t *testing.T) {
			calls := drive(t, driveOptions{
				stateReader: func(entity string, _ int) string {
					if entity == satellite {
						return states[0]
					}
					return states[1]
				},
				events: []scheduler.Event{event(int64ptr(1_700_000_000), "Готово.")},
				limit:  2,
			})
			if len(calls.leases) != 0 {
				t.Errorf("leases = %#v", calls.leases)
			}
			if len(calls.acks) != 0 {
				t.Errorf("acks = %#v", calls.acks)
			}
		})
	}
}

// TestLeasesOnlyTheCurrentProfileQueueForThisConsumer mirrors
// test_leases_only_the_current_profile_queue_for_this_consumer.
func TestLeasesOnlyTheCurrentProfileQueueForThisConsumer(t *testing.T) {
	calls := drive(t, driveOptions{profile: "personal"})
	if len(calls.leases) == 0 {
		t.Fatal("no lease was attempted")
	}
	first := calls.leases[0]
	if first.queue != "voice-personal" || first.consumer != "voice-proxy-home" || first.lease != 120 {
		t.Fatalf("lease = %#v", first)
	}
}

// TestStaleAnswerIsAckedWithoutSpeaking mirrors
// test_stale_answer_is_acked_without_speaking.
func TestStaleAnswerIsAckedWithoutSpeaking(t *testing.T) {
	calls := drive(t, driveOptions{
		events: []scheduler.Event{event(int64ptr(1_699_000_000), "Готово.")},
		limit:  3,
	})
	if len(calls.announced) != 0 {
		t.Fatalf("announced = %#v", calls.announced)
	}
	want := ackCall{"ev_1", "ACK_OUTCOME_SKIPPED", "stale"}
	if len(calls.acks) != 1 || calls.acks[0] != want {
		t.Fatalf("acks = %#v, want %#v", calls.acks, want)
	}
}

// TestAnswerWaitedOverTwoMinutesIsPrefixedAndAcked mirrors
// test_answer_waited_over_two_minutes_is_prefixed_and_acked.
func TestAnswerWaitedOverTwoMinutesIsPrefixedAndAcked(t *testing.T) {
	calls := drive(t, driveOptions{
		events: []scheduler.Event{event(int64ptr(1_699_999_870), "Готово.")},
		limit:  3,
	})
	if len(calls.announced) != 1 || calls.announced[0] != "Пока вас не было: Готово." {
		t.Fatalf("announced = %#v", calls.announced)
	}
	if len(calls.acks) != 1 || calls.acks[0].eventID != "ev_1" || calls.acks[0].outcome != "ACK_OUTCOME_DELIVERED" {
		t.Fatalf("acks = %#v", calls.acks)
	}
}

// TestAckWaitsForTheStationToFinish mirrors
// test_ack_waits_for_the_station_to_finish. The ack is not issued while the
// station is busy; the wait loop reads states repeatedly and then gives up with
// an idle_timeout detail once the drive budget is exhausted.
func TestAckWaitsForTheStationToFinish(t *testing.T) {
	sequence := []string{"busy", "busy", "idle"}
	calls := drive(t, driveOptions{
		stateReader: func(entity string, _ int) string {
			value := "idle"
			if len(sequence) > 0 {
				value = sequence[0]
				sequence = sequence[1:]
			}
			if entity == satellite {
				if value == "busy" {
					return "listening"
				}
				return "idle"
			}
			return "playing"
		},
		events: []scheduler.Event{event(int64ptr(1_700_000_000), "Готово.")},
		limit:  8,
	})
	if len(calls.announced) != 1 || calls.announced[0] != "Готово." {
		t.Fatalf("announced = %#v", calls.announced)
	}
	if len(calls.acks) != 1 {
		t.Fatalf("acks = %#v", calls.acks)
	}
	if calls.states < 8 {
		t.Fatalf("state reads = %d, want >= 8 before the ack", calls.states)
	}
}

// TestAnnounceFailureIsNotAcked mirrors test_announce_failure_is_not_acked.
func TestAnnounceFailureIsNotAcked(t *testing.T) {
	calls := drive(t, driveOptions{
		announceResult: &[2]bool{false, false},
		events:         []scheduler.Event{event(int64ptr(1_700_000_000), "Готово.")},
		limit:          3,
	})
	if len(calls.announced) != 1 || calls.announced[0] != "Готово." {
		t.Fatalf("announced = %#v", calls.announced)
	}
	if len(calls.acks) != 0 {
		t.Fatalf("acks = %#v, want none", calls.acks)
	}
}

// TestStationIdleRules mirrors test_station_idle_rules.
func TestStationIdleRules(t *testing.T) {
	cases := []struct {
		satellite string
		media     string
		want      bool
	}{
		{"idle", "playing", true},
		{"idle", "idle", true},
		{"idle", "announcing", false},
		{"", "playing", false},
		{"idle", "", false},
		{"unknown", "playing", false},
	}
	for _, tc := range cases {
		if got := queue.StationIdle(tc.satellite, tc.media); got != tc.want {
			t.Errorf("StationIdle(%q, %q) = %t, want %t", tc.satellite, tc.media, got, tc.want)
		}
	}
}
