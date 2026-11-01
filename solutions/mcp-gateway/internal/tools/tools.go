// Package tools implements the MCP tool groups: infra.read, infra.fix and
// infra.report. It depends on narrow interfaces so every backend can be faked
// in tests without network access.
package tools

import (
	"context"
	"encoding/json"
	"fmt"
	"log/slog"
	"regexp"
	"strings"
	"time"
	"unicode"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"ricardo.local/monorepo/solutions/mcp-gateway/internal/alertmanager"
	"ricardo.local/monorepo/solutions/mcp-gateway/internal/catalog"
	"ricardo.local/monorepo/solutions/mcp-gateway/internal/checks"
	"ricardo.local/monorepo/solutions/mcp-gateway/internal/kubernetes"
	"ricardo.local/monorepo/solutions/mcp-gateway/internal/loki"
	"ricardo.local/monorepo/solutions/mcp-gateway/internal/metrics"
	"ricardo.local/monorepo/solutions/mcp-gateway/internal/pending"
	"ricardo.local/monorepo/solutions/mcp-gateway/internal/prometheus"
)

// Generic tool-group names enabled from configuration.
const (
	GroupRead   = "infra.read"
	GroupFix    = "infra.fix"
	GroupReport = "infra.report"
)

// MaxSpeechRunes is the voice budget for the speech field.
const MaxSpeechRunes = 160

// PromQuerier runs read-only instant PromQL.
type PromQuerier interface {
	Query(ctx context.Context, promql string, limit int) ([]prometheus.Sample, error)
}

// AlertLister lists active alerts.
type AlertLister interface {
	Active(ctx context.Context, limit int) ([]alertmanager.Alert, error)
}

// LogQuerier queries Loki.
type LogQuerier interface {
	Query(ctx context.Context, selector string, since time.Duration, limit int) ([]loki.Line, error)
}

// KubeAPI is the Kubernetes subset used by the tools.
type KubeAPI interface {
	GetWorkload(ctx context.Context, namespace, kind, name string) (kubernetes.Workload, error)
	ListPods(ctx context.Context, namespace string, labels map[string]string) ([]kubernetes.Pod, error)
	ListEvents(ctx context.Context, namespace, objectName string, limit int) ([]kubernetes.Event, error)
	PodLogs(ctx context.Context, namespace, pod, container string, tailLines int, since time.Duration) (string, error)
	PatchRestart(ctx context.Context, namespace, kind, name string, at time.Time) error
}

// ReportSender delivers a long report out of band.
type ReportSender interface {
	Send(ctx context.Context, title, text string) error
}

// CheckRunner executes the typed external checks of every catalog service.
type CheckRunner interface {
	RunAll(ctx context.Context, services []catalog.Service) []checks.Result
}

// Scheduler defers work without blocking a tool call.
type Scheduler interface {
	AfterFunc(d time.Duration, f func())
}

// TimerScheduler uses the process timers.
type TimerScheduler struct{}

// AfterFunc implements Scheduler with time.AfterFunc.
func (TimerScheduler) AfterFunc(d time.Duration, f func()) { time.AfterFunc(d, f) }

// Deps is everything the tool groups need. Optional backends may be nil; the
// tools then report that the backend is not configured instead of panicking.
type Deps struct {
	Catalog *catalog.Catalog
	Prom    PromQuerier
	Alerts  AlertLister
	Loki    LogQuerier
	Kube    KubeAPI
	Report  ReportSender
	Checks  CheckRunner
	Pending *pending.Store
	Timers  Scheduler
	Metrics *metrics.Metrics
	Logger  *slog.Logger
	Now     func() time.Time

	// Cooldown is the minimum interval between restarts of one workload.
	Cooldown time.Duration
	// SelfRestartDelay defers a self-disrupting restart so the reply is spoken.
	SelfRestartDelay time.Duration
	// QueryTimeout bounds one backend call inside a tool.
	QueryTimeout time.Duration
	// StatusTimeout bounds the whole infra_status aggregate.
	StatusTimeout time.Duration
	// ReportTimeout bounds the whole infra_report aggregate.
	ReportTimeout time.Duration
	// LogByteCap bounds the returned log payload.
	LogByteCap int
}

// Result is the response envelope of every tool: a short spoken line and
// structured detail for the model.
type Result struct {
	Speech  string `json:"speech"`
	Details any    `json:"details,omitempty"`
}

func (d *Deps) now() time.Time {
	if d.Now != nil {
		return d.Now()
	}
	return time.Now()
}

func (d *Deps) logger() *slog.Logger {
	if d.Logger != nil {
		return d.Logger
	}
	return slog.Default()
}

func (d *Deps) queryTimeout() time.Duration {
	if d.QueryTimeout > 0 {
		return d.QueryTimeout
	}
	return 4 * time.Second
}

func (d *Deps) statusTimeout() time.Duration {
	if d.StatusTimeout > 0 {
		return d.StatusTimeout
	}
	return 8 * time.Second
}

func (d *Deps) reportTimeout() time.Duration {
	if d.ReportTimeout > 0 {
		return d.ReportTimeout
	}
	return 9 * time.Second
}

func (d *Deps) cooldown() time.Duration {
	if d.Cooldown > 0 {
		return d.Cooldown
	}
	return 5 * time.Minute
}

func (d *Deps) selfRestartDelay() time.Duration {
	if d.SelfRestartDelay > 0 {
		return d.SelfRestartDelay
	}
	return 20 * time.Second
}

func (d *Deps) logByteCap() int {
	if d.LogByteCap > 0 {
		return d.LogByteCap
	}
	return 64 << 10
}

// instrument counts and times one tool call.
func instrument[In any](d *Deps, name string, h mcp.ToolHandlerFor[In, any]) mcp.ToolHandlerFor[In, any] {
	return func(ctx context.Context, req *mcp.CallToolRequest, in In) (*mcp.CallToolResult, any, error) {
		start := d.now()
		res, out, err := h(ctx, req, in)
		outcome := "ok"
		if err != nil {
			outcome = "error"
		}
		if d.Metrics != nil {
			d.Metrics.ObserveTool(name, outcome, d.now().Sub(start))
		}
		return res, out, err
	}
}

// respond packs a Result into one JSON text block, which is what the voice
// path passes to the model verbatim.
func respond(r Result) (*mcp.CallToolResult, any, error) {
	r.Speech = Speech(r.Speech)
	payload, err := json.MarshalIndent(r, "", "  ")
	if err != nil {
		return nil, nil, err
	}
	return &mcp.CallToolResult{
		Content: []mcp.Content{&mcp.TextContent{Text: string(payload)}},
	}, r, nil
}

// Failure returns a tool result that explains a backend problem in speech
// instead of surfacing a protocol error the voice path cannot read.
func Failure(speech string, details any) (*mcp.CallToolResult, any, error) {
	return respond(Result{Speech: speech, Details: details})
}

var urlPattern = regexp.MustCompile(`\S+://\S+`)

// Speech normalizes a spoken line: single line, no markdown, no URLs, at most
// MaxSpeechRunes runes.
func Speech(s string) string {
	s = urlPattern.ReplaceAllString(s, "ссылка")
	var b strings.Builder
	space := false
	for _, r := range s {
		if r == '\n' || r == '\r' || r == '\t' || unicode.IsSpace(r) {
			space = true
			continue
		}
		if strings.ContainsRune("*_`#>|[]", r) {
			continue
		}
		if space && b.Len() > 0 {
			b.WriteByte(' ')
		}
		space = false
		b.WriteRune(r)
	}
	out := strings.TrimSpace(b.String())
	runes := []rune(out)
	if len(runes) > MaxSpeechRunes {
		out = strings.TrimSpace(string(runes[:MaxSpeechRunes-1])) + "…"
	}
	return out
}

// DisplayName returns the best spoken name for a service.
func DisplayName(s catalog.Service) string {
	if len(s.Aliases.RU) > 0 {
		return capitalize(s.Aliases.RU[0])
	}
	if len(s.Aliases.EN) > 0 {
		return capitalize(s.Aliases.EN[0])
	}
	return capitalize(s.ID)
}

func capitalize(s string) string {
	runes := []rune(strings.TrimSpace(s))
	if len(runes) == 0 {
		return s
	}
	runes[0] = unicode.ToUpper(runes[0])
	return string(runes)
}

// audit writes the structured action log and bumps the action metric.
func (d *Deps) audit(action, target, outcome string, extra ...any) {
	args := []any{"event", "action", "action", action, "target", target, "outcome", outcome}
	args = append(args, extra...)
	level := slog.LevelInfo
	if outcome != "success" && outcome != "scheduled" {
		level = slog.LevelWarn
	}
	d.logger().Log(context.Background(), level, "mcp-gateway action", args...)
	if d.Metrics != nil {
		d.Metrics.ObserveAction(action, target, outcome)
	}
}

// describeWorkload renders the compact workload status used in speech and
// details.
func describeWorkload(w kubernetes.Workload) string {
	return fmt.Sprintf("%s %s/%s: готово %d из %d", w.Kind, w.Namespace, w.Name, w.Ready, w.Desired)
}
