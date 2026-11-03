package result

import (
	"context"
	"testing"
	"time"
)

type resultCall struct {
	threadID string
	afterSeq int64
	wait     int
}

type fakeClient struct {
	responses []map[string]any
	errors    []error
	calls     []resultCall
}

func (f *fakeClient) Result(_ context.Context, threadID string, afterSeq int64, wait int) (map[string]any, error) {
	f.calls = append(f.calls, resultCall{threadID, afterSeq, wait})
	if len(f.errors) > 0 {
		err := f.errors[0]
		f.errors = f.errors[1:]
		return nil, err
	}
	if len(f.responses) == 0 {
		return map[string]any{"status": "running"}, nil
	}
	response := f.responses[0]
	f.responses = f.responses[1:]
	return response, nil
}

type fakeSpeaker struct {
	spoken   []string
	announce bool
	listen   bool
}

func (f *fakeSpeaker) Announce(_ context.Context, text string) (bool, bool) {
	f.spoken = append(f.spoken, text)
	return f.announce, f.listen
}

type enqueueCall struct {
	speech  string
	profile string
	thread  string
	seq     string
	created *int64
}

func newTestAnnouncer(client *fakeClient, speaker *fakeSpeaker, queueOn bool, enqueue func(context.Context, string, string, string, string, *int64) bool) *Announcer {
	return New(Config{
		Context:      context.Background(),
		Client:       client,
		Announce:     speaker,
		Enqueue:      enqueue,
		QueueEnabled: func() bool { return queueOn },
		MaxChars:     200,
		Timeout:      5 * time.Second,
		Wait:         25,
		Retry:        20 * time.Millisecond,
	})
}

// TestResultAnnouncerEnqueuesInsteadOfSpeaking mirrors
// test_result_announcer_enqueues_instead_of_speaking.
func TestResultAnnouncerEnqueuesInsteadOfSpeaking(t *testing.T) {
	client := &fakeClient{responses: []map[string]any{{
		"status": "done", "speech": "**Готово**.", "completedSeq": 42,
	}}}
	speaker := &fakeSpeaker{announce: true}
	var enqueued []enqueueCall
	announcer := newTestAnnouncer(client, speaker, true,
		func(_ context.Context, speech, profileID, threadID, seq string, created *int64) bool {
			enqueued = append(enqueued, enqueueCall{speech, profileID, threadID, seq, created})
			return true
		})
	announcer.Poll(context.Background(), "thr_example", 7, "personal")

	if len(speaker.spoken) != 0 {
		t.Fatalf("spoke %#v, want nothing", speaker.spoken)
	}
	if len(enqueued) != 1 {
		t.Fatalf("enqueued = %#v", enqueued)
	}
	want := enqueueCall{speech: "Готово.", profile: "personal", thread: "thr_example", seq: "done:42"}
	got := enqueued[0]
	if got.speech != want.speech || got.profile != want.profile || got.thread != want.thread || got.seq != want.seq {
		t.Fatalf("enqueued = %#v, want %#v", got, want)
	}
	if got.created != nil {
		t.Fatalf("created = %v, want nil (the producer stamps it)", *got.created)
	}
}

// TestResultAnnouncerFallsBackWhenEnqueueFails mirrors
// test_result_announcer_falls_back_when_enqueue_fails.
func TestResultAnnouncerFallsBackWhenEnqueueFails(t *testing.T) {
	client := &fakeClient{responses: []map[string]any{{"status": "done", "speech": "Готово."}}}
	speaker := &fakeSpeaker{announce: true}
	announcer := newTestAnnouncer(client, speaker, true,
		func(context.Context, string, string, string, string, *int64) bool { return false })
	announcer.Poll(context.Background(), "thr_example", 7, "work")

	if len(speaker.spoken) != 1 || speaker.spoken[0] != "Готово." {
		t.Fatalf("spoken = %#v", speaker.spoken)
	}
}

// TestQueueDisabledKeepsTheDirectAnnounce mirrors
// test_queue_disabled_keeps_the_direct_announce.
func TestQueueDisabledKeepsTheDirectAnnounce(t *testing.T) {
	client := &fakeClient{responses: []map[string]any{{"status": "done", "speech": "Готово."}}}
	speaker := &fakeSpeaker{announce: true}
	announcer := newTestAnnouncer(client, speaker, false, nil)
	announcer.Poll(context.Background(), "thr_example", 7, "work")

	if len(speaker.spoken) != 1 || speaker.spoken[0] != "Готово." {
		t.Fatalf("spoken = %#v", speaker.spoken)
	}
}

// TestResultLongPollWaitsForTheFinalAnswer mirrors
// test_result_long_poll_waits_for_the_final_answer: a running answer is polled
// again and only the final speech is announced.
func TestResultLongPollWaitsForTheFinalAnswer(t *testing.T) {
	client := &fakeClient{responses: []map[string]any{
		{"status": "running"},
		{"status": "waiting_input", "speech": "Нужно уточнение."},
	}}
	speaker := &fakeSpeaker{announce: true}
	announcer := newTestAnnouncer(client, speaker, false, nil)
	announcer.Poll(context.Background(), "thr_example", 4, "work")

	if len(client.calls) != 2 {
		t.Fatalf("result calls = %d, want 2", len(client.calls))
	}
	if client.calls[0].threadID != "thr_example" || client.calls[0].afterSeq != 4 {
		t.Fatalf("call = %#v", client.calls[0])
	}
	if len(speaker.spoken) != 1 || speaker.spoken[0] != "Нужно уточнение." {
		t.Fatalf("spoken = %#v", speaker.spoken)
	}
}

// TestResultTimeoutDoesNotSpeakAnything mirrors
// test_result_timeout_does_not_speak_anything.
func TestResultTimeoutDoesNotSpeakAnything(t *testing.T) {
	client := &fakeClient{}
	speaker := &fakeSpeaker{announce: true}
	announcer := New(Config{
		Context:      context.Background(),
		Client:       client,
		Announce:     speaker,
		QueueEnabled: func() bool { return false },
		MaxChars:     200,
		Timeout:      100 * time.Millisecond,
		Wait:         25,
		Retry:        10 * time.Millisecond,
	})
	announcer.Poll(context.Background(), "thr_example", 4, "work")

	if len(speaker.spoken) != 0 {
		t.Fatalf("spoken = %#v, want nothing on timeout", speaker.spoken)
	}
}

// TestFailedTurnSpeaksTheExplicitPhrase pins the failed -> phrase mapping.
func TestFailedTurnSpeaksTheExplicitPhrase(t *testing.T) {
	client := &fakeClient{responses: []map[string]any{{"status": "failed"}}}
	speaker := &fakeSpeaker{announce: true}
	announcer := newTestAnnouncer(client, speaker, false, nil)
	announcer.Poll(context.Background(), "thr_example", 1, "work")

	if len(speaker.spoken) != 1 || speaker.spoken[0] != SpeechTurnFailed {
		t.Fatalf("spoken = %#v", speaker.spoken)
	}
}

// TestEmptySuccessfulAnswerBecomesDone pins the empty -> "Готово." mapping.
func TestEmptySuccessfulAnswerBecomesDone(t *testing.T) {
	client := &fakeClient{responses: []map[string]any{{"status": "done"}}}
	speaker := &fakeSpeaker{announce: true}
	announcer := newTestAnnouncer(client, speaker, false, nil)
	announcer.Poll(context.Background(), "thr_example", 1, "work")

	if len(speaker.spoken) != 1 || speaker.spoken[0] != SpeechDone {
		t.Fatalf("spoken = %#v", speaker.spoken)
	}
}

// TestGatewayErrorSpeaksNothing pins the gateway_error branch.
func TestGatewayErrorSpeaksNothing(t *testing.T) {
	client := &fakeClient{errors: []error{context.DeadlineExceeded}}
	speaker := &fakeSpeaker{announce: true}
	announcer := New(Config{
		Context:      context.Background(),
		Client:       client,
		Announce:     speaker,
		QueueEnabled: func() bool { return false },
		Timeout:      200 * time.Millisecond,
		Wait:         25,
		Retry:        20 * time.Millisecond,
	})
	announcer.Poll(context.Background(), "thr_example", 1, "work")

	if len(speaker.spoken) != 0 {
		t.Fatalf("spoken = %#v, want nothing", speaker.spoken)
	}
}

// TestBackgroundAnnouncerIsTracked pins Start/Running/Wait for graceful
// shutdown.
func TestBackgroundAnnouncerIsTracked(t *testing.T) {
	client := &fakeClient{responses: []map[string]any{{"status": "done", "speech": "Готово."}}}
	speaker := &fakeSpeaker{announce: true}
	announcer := newTestAnnouncer(client, speaker, false, nil)
	announcer.Start("thr_example", 1, "work")
	announcer.Wait(2 * time.Second)
	if running := announcer.Running(); running != 0 {
		t.Fatalf("running = %d", running)
	}
	if len(speaker.spoken) != 1 {
		t.Fatalf("spoken = %#v", speaker.spoken)
	}
}
