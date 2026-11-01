package tools

import (
	"context"
	"errors"
	"strings"
	"testing"
)

func TestSendReport(t *testing.T) {
	d := testDeps(t)
	report := d.Report.(*fakeReport)
	_, out, err := d.infraSendReport(context.Background(), nil, reportInput{Title: "Проверка", Text: "Всё работает."})
	if err != nil {
		t.Fatalf("infraSendReport: %v", err)
	}
	result := resultOf(t, out)
	if !strings.Contains(result.Speech, "Отправил") {
		t.Fatalf("speech = %q", result.Speech)
	}
	if report.title != "Проверка" || report.text != "Всё работает." {
		t.Fatalf("sent title=%q text=%q", report.title, report.text)
	}
	detail := result.Details.(reportDetail)
	if detail.Status != "sent" || detail.Bytes != len("Всё работает.") {
		t.Fatalf("detail = %+v", detail)
	}
}

func TestSendReportRedactsSecrets(t *testing.T) {
	d := testDeps(t)
	report := d.Report.(*fakeReport)
	if _, _, err := d.infraSendReport(context.Background(), nil, reportInput{
		Title: "Логи",
		Text:  "Authorization: Bearer abcdefghijklmnop\npassword=hunter2",
	}); err != nil {
		t.Fatalf("infraSendReport: %v", err)
	}
	if strings.Contains(report.text, "abcdefghijklmnop") || strings.Contains(report.text, "hunter2") {
		t.Fatalf("secret leaked: %q", report.text)
	}
	if !strings.Contains(report.text, "[REDACTED]") {
		t.Fatalf("no redaction: %q", report.text)
	}
}

func TestSendReportValidation(t *testing.T) {
	d := testDeps(t)
	cases := []reportInput{
		{Title: "", Text: "x"},
		{Title: "x", Text: ""},
		{Title: strings.Repeat("д", maxReportTitle+1), Text: "x"},
		{Title: "x", Text: strings.Repeat("я", maxReportBytes+1)},
	}
	for _, input := range cases {
		_, out, err := d.infraSendReport(context.Background(), nil, input)
		if err != nil {
			t.Fatalf("infraSendReport: %v", err)
		}
		result := resultOf(t, out)
		if result.Speech == "" {
			t.Fatal("empty speech")
		}
	}
}

func TestSendReportFailure(t *testing.T) {
	d := testDeps(t)
	d.Report.(*fakeReport).err = errors.New("telegram down")
	_, out, err := d.infraSendReport(context.Background(), nil, reportInput{Title: "x", Text: "y"})
	if err != nil {
		t.Fatalf("infraSendReport: %v", err)
	}
	result := resultOf(t, out)
	if !strings.Contains(result.Speech, "Не удалось") {
		t.Fatalf("speech = %q", result.Speech)
	}
	detail := result.Details.(map[string]string)
	if detail["error"] == "" {
		t.Fatal("error detail missing")
	}
}

func TestSendReportWithoutTelegram(t *testing.T) {
	d := testDeps(t)
	d.Report = nil
	_, out, _ := d.infraSendReport(context.Background(), nil, reportInput{Title: "x", Text: "y"})
	if !strings.Contains(resultOf(t, out).Speech, "не настроен") {
		t.Fatal("unconfigured telegram not reported")
	}
}
