// Package router decides whether the last user utterance reaches Hermes, is
// answered locally, or starts a bb turn whose answer is announced later.
//
// The fail-closed contract is the important part:
//
//   - an unknown profile answers "Не могу подтвердить профиль." and never
//     reaches Hermes, for every profile;
//   - an unknown route, or a gateway response with ok:false, answers
//     "Сервер агентов недоступен." and is never forwarded;
//   - a gateway failure in the work profile stays local, because forwarding it
//     would run a work phrase in the personal agent; in the personal profile the
//     request still reaches Hermes.
package router

import (
	"context"
	"encoding/json"
	"strconv"
	"strings"
	"sync"

	"ricardo.local/monorepo/solutions/voice-proxy/internal/profile"
	"ricardo.local/monorepo/solutions/voice-proxy/internal/proxy"
	"ricardo.local/monorepo/solutions/voice-proxy/internal/sanitize"
)

// Speech phrases.
const (
	SpeechProfileUnknown = "Не могу подтвердить профиль."
	SpeechAgentsDown     = "Сервер агентов недоступен."
)

// ProfileReader reads the current routing profile.
type ProfileReader interface {
	Profile(ctx context.Context) (string, bool)
}

// Gateway asks the mcp-gateway where an utterance belongs.
type Gateway interface {
	Utterance(ctx context.Context, text, profileID string) (map[string]any, error)
}

// Results starts the background announcer for a routed turn.
type Results interface {
	Start(threadID string, afterSeq int64, profileID string)
}

// Config configures a Router.
type Config struct {
	Profile ProfileReader
	Gateway Gateway
	// Results may be nil: a bb acknowledgement is still spoken, only the late
	// answer is not announced.
	Results Results
}

// Router implements proxy.Router.
type Router struct {
	cfg    Config
	mu     sync.Mutex
	counts map[string]int64
}

// New builds a Router.
func New(cfg Config) *Router {
	return &Router{cfg: cfg, counts: map[string]int64{}}
}

// Counts returns a copy of the per-outcome counter.
func (r *Router) Counts() map[string]int64 {
	r.mu.Lock()
	defer r.mu.Unlock()
	out := make(map[string]int64, len(r.counts))
	for key, value := range r.counts {
		out[key] = value
	}
	return out
}

// Count returns one outcome counter.
func (r *Router) Count(outcome string) int64 {
	r.mu.Lock()
	defer r.mu.Unlock()
	return r.counts[outcome]
}

// Route resolves profile and route for one utterance. It never panics or
// returns an error: every failure becomes an explicit decision so the caller
// keeps the fail-closed contract.
func (r *Router) Route(ctx context.Context, payload map[string]any, utterance string) proxy.RouteResult {
	profileID, ok := r.cfg.Profile.Profile(ctx)
	if !ok {
		return r.decide(proxy.RouteResult{
			Handled: true,
			Speech:  SpeechProfileUnknown,
			Outcome: "profile_unknown",
			Profile: "",
		})
	}
	response, err := r.cfg.Gateway.Utterance(ctx, utterance, profileID)
	if err != nil {
		if profileID == profile.Personal {
			// Personal speech already went to Hermes before this change.
			return r.decide(proxy.RouteResult{Handled: false, Profile: profileID, Outcome: "gateway_down_personal"})
		}
		return r.decide(proxy.RouteResult{
			Handled: true,
			Speech:  SpeechAgentsDown,
			Profile: profileID,
			Outcome: "gateway_down",
		})
	}
	route, _ := response["route"].(string)
	if route == "hermes" {
		return r.decide(proxy.RouteResult{Handled: false, Profile: profileID, Outcome: "hermes"})
	}
	speech, _ := response["speech"].(string)
	if strings.TrimSpace(speech) == "" {
		speech = SpeechAgentsDown
	}
	if route != "bb" && route != "local" {
		return r.decide(proxy.RouteResult{
			Handled: true,
			Speech:  SpeechAgentsDown,
			Profile: profileID,
			Outcome: "gateway_failed",
		})
	}
	if failed, isBool := response["ok"].(bool); isBool && !failed {
		return r.decide(proxy.RouteResult{
			Handled: true,
			Speech:  sanitize.EnsureNotQuestion(speech, SpeechAgentsDown),
			Profile: profileID,
			Outcome: "gateway_failed",
		})
	}
	threadID, _ := response["threadId"].(string)
	afterSeq, hasAfterSeq := intValue(response["afterSeq"])
	if route == "bb" && strings.TrimSpace(threadID) != "" && hasAfterSeq {
		decision := proxy.RouteResult{
			Handled: true,
			Speech:  sanitize.EnsureNotQuestion(speech, SpeechAgentsDown),
			Profile: profileID,
			Outcome: "bb",
		}
		if r.cfg.Results != nil {
			thread, seq := threadID, afterSeq
			decision.Announce = func() { r.cfg.Results.Start(thread, seq, profileID) }
		}
		return r.decide(decision)
	}
	outcome := "bb"
	if route == "local" {
		outcome = "local"
	}
	return r.decide(proxy.RouteResult{
		Handled: true,
		Speech:  sanitize.EnsureNotQuestion(speech, SpeechAgentsDown),
		Profile: profileID,
		Outcome: outcome,
	})
}

func (r *Router) decide(decision proxy.RouteResult) proxy.RouteResult {
	r.mu.Lock()
	r.counts[decision.Outcome]++
	r.mu.Unlock()
	return decision
}

// intValue accepts every numeric shape a JSON decoder can produce. bool is not
// a number, matching `isinstance(after_seq, int) and not isinstance(after_seq, bool)`.
func intValue(value any) (int64, bool) {
	switch number := value.(type) {
	case json.Number:
		if parsed, err := number.Int64(); err == nil {
			return parsed, true
		}
		if parsed, err := number.Float64(); err == nil {
			return int64(parsed), true
		}
	case float64:
		return int64(number), true
	case int64:
		return number, true
	case int:
		return int64(number), true
	case string:
		if parsed, err := strconv.ParseInt(number, 10, 64); err == nil {
			return parsed, true
		}
	}
	return 0, false
}
