package voice

import (
	"io"
	"log/slog"
	"net/http"
	"testing"
	"time"

	"golang.org/x/time/rate"

	"github.com/proflyder/monorepo/packages/go/contracttest"
	"github.com/proflyder/monorepo/solutions/mcp-gateway/internal/agentmode"
	"github.com/proflyder/monorepo/solutions/mcp-gateway/internal/bb"
	"github.com/proflyder/monorepo/solutions/mcp-gateway/internal/server"
)

// voiceClock advances one second per read so the result long-poll reaches its
// deadline deterministically.
type voiceClock struct{ calls int }

func (c *voiceClock) Now() time.Time {
	c.calls++
	return time.Date(2026, 11, 1, 12, 0, 0, 0, time.UTC).Add(time.Duration(c.calls) * time.Second)
}

func ptr(s string) *string { return &s }

// voiceScenario is the live state behind one fixture file.
type voiceScenario struct {
	dispatcher *fakeDispatcher
	mode       *agentmode.State
}

func (s *voiceScenario) beforeStep(scenario string, step int) {
	switch scenario {
	case "voice_utterance_routes":
		switch step {
		case 7:
			s.mode.Enter("thr_agent")
		case 10:
			s.dispatcher.sendErr = &bb.Error{Code: "bridge_error", Speech: bb.SpeechUnavailable}
		}
	case "voice_updates":
		switch step {
		case 3:
			s.dispatcher.updates = append(s.dispatcher.updates,
				bb.UpdatesResult{Cursor: 42, Items: []bb.UpdateItem{
					{Seq: 40, Spoken: "Первый ответ.", CompletedAt: 1700000000000},
					{Seq: 41, Spoken: "Второй ответ."},
				}})
		}
	case "voice_updates_errors":
		// The fake dispatcher starts with the error already set.
	case "voice_state":
		switch step {
		case 3:
			s.mode.Enter("thr_agent")
		case 5:
			s.mode.Exit()
		}
	}
}

func newVoiceContractHandler(t *testing.T, scenario string) http.Handler {
	t.Helper()
	dispatcher := &fakeDispatcher{}
	mode := agentmode.New()
	workThread := "thr_work"
	clock := &voiceClock{}
	limiter := rate.NewLimiter(rate.Inf, 1)
	if scenario == "voice_rate_limit" {
		limiter = rate.NewLimiter(rate.Every(time.Hour), 1)
	}
	if scenario == "voice_work_missing" {
		workThread = ""
	}
	completed := int64(77)
	switch scenario {
	case "voice_result":
		dispatcher.turns = []bb.TurnResultResult{
			{Status: "idle", Spoken: ptr("Готово, тесты починены."), Text: "details", CompletedSeq: &completed},
			{Status: "waiting_input"},
			{Status: "failed"},
		}
	case "voice_result_errors":
		dispatcher.err = &bb.Error{Code: "bridge_error", Speech: bb.SpeechUnavailable}
	case "voice_updates":
		dispatcher.updates = []bb.UpdatesResult{{Cursor: 12}, {Cursor: 12}}
	case "voice_updates_errors":
		dispatcher.updatesErr = &bb.Error{Code: "bridge_error", Speech: bb.SpeechUnavailable}
	}

	handler := New(Options{
		Dispatcher:   dispatcher,
		WorkThreadID: workThread,
		Mode:         mode,
		Logger:       slog.New(slog.NewTextHandler(io.Discard, nil)),
		Limiter:      limiter,
		Now:          clock.Now,
		PollInterval: time.Millisecond,
		MaxWait:      time.Second,
		UpdatesWait:  time.Second,
	}).Handler()

	scenarioState := &voiceScenario{dispatcher: dispatcher, mode: mode}
	voiceContractState[scenario] = scenarioState
	return server.New(server.Options{
		AuthToken:  "mcp-contract-token",
		VoiceToken: "voice-contract-token",
		Voice:      handler,
		Logger:     slog.New(slog.NewTextHandler(io.Discard, nil)),
	}).Handler()
}

var voiceContractState = map[string]*voiceScenario{}

// TestVoiceContractFixtures replays the voice routes behind the dedicated
// bearer token, with a fake dispatcher and a deterministic clock.
func TestVoiceContractFixtures(t *testing.T) {
	contracttest.Run(t,
		"solutions/mcp-gateway/internal/voice/testdata/contract",
		newVoiceContractHandler,
		contracttest.WithBeforeStep(func(_ *testing.T, scenario string, step int) {
			if state := voiceContractState[scenario]; state != nil {
				state.beforeStep(scenario, step)
			}
		}),
	)
}
