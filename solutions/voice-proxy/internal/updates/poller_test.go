package updates

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"sync"
	"testing"
	"time"

	"ricardo.local/monorepo/solutions/voice-proxy/internal/announce"
	"ricardo.local/monorepo/solutions/voice-proxy/internal/profile"
)

type fetchCall struct {
	after *int64
	wait  int
}

// runPoller starts the poller with a sleeper that cancels the context after a
// fixed number of pauses.
func runPoller(t *testing.T, cfg Config, limit int, sleeps *[]time.Duration) {
	t.Helper()
	ctx, cancel := context.WithCancel(context.Background())
	count := 0
	base := cfg.Sleep
	cfg.Sleep = func(_ context.Context, d time.Duration) bool {
		count++
		if sleeps != nil {
			*sleeps = append(*sleeps, d)
		}
		if count >= limit {
			cancel()
			return false
		}
		if base != nil {
			return base(ctx, d)
		}
		return true
	}
	if cfg.Logger == nil {
		cfg.Logger = nopLogger{}
	}
	New(cfg).Run(ctx)
}

// TestBootstrapHasNoHistoryAndNextCallUsesTheCursor mirrors
// test_bootstrap_has_no_history_and_next_call_uses_the_cursor.
func TestBootstrapHasNoHistoryAndNextCallUsesTheCursor(t *testing.T) {
	var mu sync.Mutex
	var calls []fetchCall
	fetch := func(_ context.Context, after *int64, wait int) (map[string]any, error) {
		mu.Lock()
		calls = append(calls, fetchCall{after, wait})
		mu.Unlock()
		return map[string]any{"cursor": 5, "items": []any{}}, nil
	}
	runPoller(t, Config{Fetch: fetch, QueueEnabled: func() bool { return false }}, 2, nil)

	mu.Lock()
	defer mu.Unlock()
	if len(calls) < 2 {
		t.Fatalf("calls = %#v", calls)
	}
	if calls[0].after != nil {
		t.Errorf("bootstrap afterSeq = %v, want nil", *calls[0].after)
	}
	if calls[0].wait != 1 {
		t.Errorf("bootstrap wait = %d, want 1", calls[0].wait)
	}
	if calls[1].after == nil || *calls[1].after != 5 {
		t.Errorf("second afterSeq = %v, want 5", calls[1].after)
	}
}

// TestWorkProfileAnnouncesJoinedSpokenText mirrors
// test_work_profile_announces_joined_spoken_text.
func TestWorkProfileAnnouncesJoinedSpokenText(t *testing.T) {
	var mu sync.Mutex
	calls := 0
	fetch := func(_ context.Context, _ *int64, _ int) (map[string]any, error) {
		mu.Lock()
		calls++
		n := calls
		mu.Unlock()
		switch n {
		case 1:
			return map[string]any{"cursor": 5, "items": []any{}}, nil
		case 2:
			return map[string]any{"cursor": 9, "items": []any{
				map[string]any{"seq": 8, "spoken": "Первый."},
				map[string]any{"seq": 9, "spoken": "Второй."},
			}}, nil
		default:
			return map[string]any{"cursor": 9, "items": []any{}}, nil
		}
	}
	var announced []string
	runPoller(t, Config{
		Fetch:        fetch,
		Profile:      func(context.Context) (string, bool) { return profile.Work, true },
		QueueEnabled: func() bool { return false },
		Announce: func(_ context.Context, text string) (bool, bool) {
			mu.Lock()
			announced = append(announced, text)
			mu.Unlock()
			return true, false
		},
	}, 4, nil)

	mu.Lock()
	defer mu.Unlock()
	if len(announced) != 1 || announced[0] != "Первый. Второй." {
		t.Fatalf("announced = %#v", announced)
	}
}

// TestPersonalProfileDropsTheBatchWithoutSpeaking mirrors
// test_personal_profile_drops_the_batch_without_speaking.
func TestPersonalProfileDropsTheBatchWithoutSpeaking(t *testing.T) {
	var mu sync.Mutex
	calls := 0
	fetch := func(_ context.Context, _ *int64, _ int) (map[string]any, error) {
		mu.Lock()
		calls++
		n := calls
		mu.Unlock()
		switch n {
		case 1:
			return map[string]any{"cursor": 5, "items": []any{}}, nil
		case 2:
			return map[string]any{"cursor": 6, "items": []any{
				map[string]any{"seq": 6, "spoken": "Не озвучивать."},
			}}, nil
		default:
			return map[string]any{"cursor": 6, "items": []any{}}, nil
		}
	}
	reads := 0
	var announced []string
	runPoller(t, Config{
		Fetch: fetch,
		Profile: func(context.Context) (string, bool) {
			mu.Lock()
			reads++
			mu.Unlock()
			return profile.Personal, true
		},
		QueueEnabled: func() bool { return false },
		Announce: func(_ context.Context, text string) (bool, bool) {
			mu.Lock()
			announced = append(announced, text)
			mu.Unlock()
			return true, false
		},
	}, 5, nil)

	mu.Lock()
	defer mu.Unlock()
	if reads == 0 {
		t.Fatal("the profile was not read for the batch")
	}
	if len(announced) != 0 {
		t.Fatalf("announced = %#v", announced)
	}
}

// TestQuestionUsesStartConversation mirrors test_question_uses_start_conversation:
// the real announcer decides, so a question must open the microphone.
func TestQuestionUsesStartConversation(t *testing.T) {
	paths := make(chan string, 4)
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		paths <- r.URL.Path
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"changed_states":[]}`))
	}))
	defer server.Close()
	speaker := announce.New(announce.Config{
		URL:       server.URL,
		Token:     func() string { return "ha" },
		Script:    "script.schedule_announce",
		Satellite: "assist_satellite.station",
		Timeout:   time.Second,
	})

	var mu sync.Mutex
	calls := 0
	fetch := func(_ context.Context, _ *int64, _ int) (map[string]any, error) {
		mu.Lock()
		calls++
		n := calls
		mu.Unlock()
		switch n {
		case 1:
			return map[string]any{"cursor": 5, "items": []any{}}, nil
		case 2:
			return map[string]any{"cursor": 6, "items": []any{
				map[string]any{"seq": 6, "spoken": "Готово. Подтверждаете?"},
			}}, nil
		default:
			return map[string]any{"cursor": 6, "items": []any{}}, nil
		}
	}
	runPoller(t, Config{
		Fetch:        fetch,
		Profile:      func(context.Context) (string, bool) { return profile.Work, true },
		QueueEnabled: func() bool { return false },
		Announce:     speaker.Announce,
	}, 4, nil)

	select {
	case path := <-paths:
		if path != "/api/services/assist_satellite/start_conversation" {
			t.Fatalf("first call = %q", path)
		}
	default:
		t.Fatal("question did not open the microphone")
	}
	select {
	case path := <-paths:
		t.Fatalf("unexpected second call = %q", path)
	default:
	}
}

// TestErrorsBackOffExponentially mirrors test_errors_back_off_exponentially.
func TestErrorsBackOffExponentially(t *testing.T) {
	var delays []time.Duration
	fetch := func(context.Context, *int64, int) (map[string]any, error) {
		return nil, errors.New("gateway down")
	}
	runPoller(t, Config{Fetch: fetch, QueueEnabled: func() bool { return false }}, 4, &delays)

	want := []time.Duration{2 * time.Second, 4 * time.Second, 8 * time.Second, 16 * time.Second}
	if len(delays) != len(want) {
		t.Fatalf("delays = %#v", delays)
	}
	for i := range want {
		if delays[i] != want[i] {
			t.Fatalf("delays = %#v, want %#v", delays, want)
		}
	}
}

// TestBootstrapRescansARecentWindow mirrors test_bootstrap_rescans_a_recent_window.
func TestBootstrapRescansARecentWindow(t *testing.T) {
	var mu sync.Mutex
	var calls []*int64
	fetch := func(_ context.Context, after *int64, _ int) (map[string]any, error) {
		mu.Lock()
		calls = append(calls, after)
		mu.Unlock()
		return map[string]any{"cursor": 500, "items": []any{}}, nil
	}
	runPoller(t, Config{
		Fetch:        fetch,
		QueueEnabled: func() bool { return true },
		Enqueue:      func(context.Context, string, string, string, string, *int64) bool { return true },
		Rescan:       300,
	}, 3, nil)

	mu.Lock()
	defer mu.Unlock()
	if len(calls) < 2 {
		t.Fatalf("calls = %#v", calls)
	}
	if calls[0] != nil {
		t.Errorf("bootstrap afterSeq = %v", *calls[0])
	}
	if calls[1] == nil || *calls[1] != 200 {
		t.Errorf("rescan afterSeq = %v, want 200", calls[1])
	}
}

// TestBootstrapWithoutQueueStartsAtTheCursor mirrors
// test_bootstrap_without_queue_starts_at_the_cursor.
func TestBootstrapWithoutQueueStartsAtTheCursor(t *testing.T) {
	var mu sync.Mutex
	var calls []*int64
	fetch := func(_ context.Context, after *int64, _ int) (map[string]any, error) {
		mu.Lock()
		calls = append(calls, after)
		mu.Unlock()
		return map[string]any{"cursor": 500, "items": []any{}}, nil
	}
	runPoller(t, Config{Fetch: fetch, QueueEnabled: func() bool { return false }}, 3, nil)

	mu.Lock()
	defer mu.Unlock()
	if len(calls) < 2 || calls[1] == nil || *calls[1] != 500 {
		t.Fatalf("calls = %#v", calls)
	}
}

// TestPersonalProfileDoesNotDropTheWorkAnswer mirrors
// test_personal_profile_does_not_drop_the_work_answer.
func TestPersonalProfileDoesNotDropTheWorkAnswer(t *testing.T) {
	events := []any{map[string]any{
		"seq": 9, "spoken": "Готово.", "completedAt": 1_700_000_000_000,
	}}
	var mu sync.Mutex
	calls := 0
	fetch := func(_ context.Context, _ *int64, _ int) (map[string]any, error) {
		mu.Lock()
		calls++
		n := calls
		mu.Unlock()
		if n == 1 {
			return map[string]any{"cursor": 10, "items": []any{}}, nil
		}
		return map[string]any{"cursor": 10, "items": events}, nil
	}
	type enqueuedCall struct {
		speech  string
		profile string
		thread  string
		seq     string
		created *int64
	}
	var enqueued []enqueuedCall
	now := time.Unix(1_700_000_010, 0)
	runPoller(t, Config{
		Fetch:        fetch,
		Profile:      func(context.Context) (string, bool) { return profile.Personal, true },
		QueueEnabled: func() bool { return true },
		TTL:          10800,
		Clock:        func() time.Time { return now },
		Enqueue: func(_ context.Context, speech, profileID, threadID, seq string, created *int64) bool {
			mu.Lock()
			enqueued = append(enqueued, enqueuedCall{speech, profileID, threadID, seq, created})
			mu.Unlock()
			return true
		},
	}, 3, nil)

	mu.Lock()
	defer mu.Unlock()
	if len(enqueued) == 0 {
		t.Fatal("the work answer was dropped in the personal profile")
	}
	got := enqueued[0]
	if got.speech != "Готово." || got.profile != profile.Work || got.thread != "dispatcher" || got.seq != "9" {
		t.Fatalf("enqueued = %#v", got)
	}
	if got.created == nil || *got.created != 1_700_000_000 {
		t.Fatalf("created = %v, want 1700000000", got.created)
	}
}

// TestStaleUpdateIsNotEnqueued mirrors test_stale_update_is_not_enqueued.
func TestStaleUpdateIsNotEnqueued(t *testing.T) {
	var enqueued []string
	queued, stale := enqueueUpdates(
		context.Background(),
		[]any{map[string]any{"seq": 1, "spoken": "Старое.", "completedAt": 1_000_000}},
		func(_ context.Context, speech, _, _, _ string, _ *int64) bool {
			enqueued = append(enqueued, speech)
			return true
		},
		time.Unix(1_700_000_000, 0), 10800, 200)
	if queued != 0 || stale != 1 {
		t.Fatalf("queued=%d stale=%d, want 0/1", queued, stale)
	}
	if len(enqueued) != 0 {
		t.Fatalf("enqueued = %#v", enqueued)
	}
}

// TestUpdatesCursorRejectsBoolsAndNegatives pins _updates_cursor.
func TestUpdatesCursorRejectsBoolsAndNegatives(t *testing.T) {
	for _, response := range []map[string]any{
		{"cursor": true},
		{"cursor": -1},
		{"cursor": "5"},
		{},
	} {
		if _, ok := updatesCursor(response); ok {
			t.Errorf("updatesCursor(%#v) = ok", response)
		}
	}
	var decoded map[string]any
	_ = json.Unmarshal([]byte(`{"cursor":7}`), &decoded)
	if value, ok := updatesCursor(decoded); !ok || value != 7 {
		t.Fatalf("updatesCursor = %d (%t)", value, ok)
	}
}
