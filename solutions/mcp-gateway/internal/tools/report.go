package tools

import (
	"context"
	"fmt"
	"strings"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"ricardo.local/monorepo/solutions/mcp-gateway/internal/redact"
)

// ReportGroup is the infra.report tool group.
type ReportGroup struct{ Deps *Deps }

// Name implements group.Group.
func (g *ReportGroup) Name() string { return GroupReport }

// Register implements group.Group.
func (g *ReportGroup) Register(r *mcp.Server) {
	d := g.Deps
	mcp.AddTool(r, &mcp.Tool{
		Name:        "infra_send_report",
		Description: "Send a long HTML-escaped report to the preconfigured Telegram chat. The recipient cannot be chosen by the model. Suitable for detailed reports that are too long to speak.",
		Annotations: &mcp.ToolAnnotations{
			ReadOnlyHint:    false,
			DestructiveHint: boolPtr(false),
			OpenWorldHint:   boolPtr(true),
		},
	}, instrument(d, "infra_send_report", d.infraSendReport))
}

type reportInput struct {
	Title string `json:"title" jsonschema:"short report title"`
	Text  string `json:"text" jsonschema:"report body, plain text; HTML is escaped"`
}

type reportDetail struct {
	Title  string `json:"title"`
	Bytes  int    `json:"bytes"`
	Status string `json:"status"`
}

const (
	maxReportTitle = 200
	maxReportBytes = 64 << 10
)

func (d *Deps) infraSendReport(ctx context.Context, _ *mcp.CallToolRequest, in reportInput) (*mcp.CallToolResult, any, error) {
	title := strings.TrimSpace(in.Title)
	text := strings.TrimSpace(in.Text)
	if title == "" || text == "" {
		return Failure("Нужны заголовок и текст отчёта.", nil)
	}
	if len([]rune(title)) > maxReportTitle {
		return Failure("Заголовок отчёта слишком длинный.", nil)
	}
	if len(text) > maxReportBytes {
		return Failure("Отчёт слишком длинный.", map[string]int{"bytes": len(text)})
	}
	if d.Report == nil {
		return Failure("Telegram не настроен.", nil)
	}
	// Defence in depth: a report is model-written text, but it may quote logs.
	text = redact.Text(text)
	if err := d.Report.Send(ctx, title, text); err != nil {
		d.audit("report", "telegram", "failure", "error", err.Error())
		return Failure("Не удалось отправить отчёт.", map[string]string{"error": err.Error()})
	}
	d.audit("report", "telegram", "success")
	return respond(Result{
		Speech:  fmt.Sprintf("Отправил отчёт «%s».", title),
		Details: reportDetail{Title: title, Bytes: len(text), Status: "sent"},
	})
}

func boolPtr(v bool) *bool { return &v }
