package sse_test

import (
	"bytes"
	"errors"
	"io"
	"strings"
	"testing"

	"ricardo.local/monorepo/solutions/voice-proxy/internal/sse"
)

// TestSSEMultilineCRLFCommentsAndUnicode mirrors
// test_sse_multiline_crlf_comments_and_unicode in test_voice_proxy.py.
func TestSSEMultilineCRLFCommentsAndUnicode(t *testing.T) {
	raw := ": ping\r\nevent: hermes.status\r\ndata: {\"text\":\r\ndata: \"Привет\"}\r\n\r\ndata: [DONE]\r\n\r\n"
	want := []sse.Event{
		{Event: sse.KeepaliveEvent, Data: ""},
		{Event: "hermes.status", Data: "{\"text\":\n\"Привет\"}"},
		{Event: "message", Data: "[DONE]"},
	}
	got := readAll(t, strings.NewReader(raw))
	if len(got) != len(want) {
		t.Fatalf("got %d events, want %d: %#v", len(got), len(want), got)
	}
	for i := range want {
		if got[i] != want[i] {
			t.Errorf("event %d = %#v, want %#v", i, got[i], want[i])
		}
	}
}

func TestSSEMultilineDataJoinedWithNewline(t *testing.T) {
	events := readAll(t, strings.NewReader("data: a\ndata: b\ndata:  c\n\n"))
	if len(events) != 1 || events[0].Data != "a\nb\n c" {
		t.Fatalf("events = %#v", events)
	}
}

func TestSSEDefaultEventNameIsMessage(t *testing.T) {
	events := readAll(t, strings.NewReader("data: x\n\n"))
	if len(events) != 1 || events[0].Event != "message" || events[0].Data != "x" {
		t.Fatalf("events = %#v", events)
	}
}

func TestSSEEventNameIsResetBetweenRecords(t *testing.T) {
	events := readAll(t, strings.NewReader("event: done\ndata: a\n\ndata: b\n\n"))
	if len(events) != 2 || events[0].Event != "done" || events[1].Event != "message" {
		t.Fatalf("events = %#v", events)
	}
}

func TestSSETrailingRecordWithoutBlankLine(t *testing.T) {
	events := readAll(t, strings.NewReader("data: tail"))
	if len(events) != 1 || events[0].Data != "tail" {
		t.Fatalf("events = %#v", events)
	}
}

func TestSSELastLineWithoutNewline(t *testing.T) {
	events := readAll(t, strings.NewReader("data: a\n\ndata: b"))
	if len(events) != 2 || events[1].Data != "b" {
		t.Fatalf("events = %#v", events)
	}
}

func TestSSEKeepsMultilineCRLFData(t *testing.T) {
	data := "data: 1\r\n\r\n: keepalive\r\n\r\ndata: 2\r\n\r\n"
	events := readAll(t, strings.NewReader(data))
	if len(events) != 3 {
		t.Fatalf("events = %#v", events)
	}
	if events[1].Event != sse.KeepaliveEvent || events[1].Data != "" {
		t.Fatalf("keepalive = %#v", events[1])
	}
}

func TestSSEInvalidUTF8IsAnError(t *testing.T) {
	reader := sse.NewReader(bytes.NewReader([]byte("data: \xff\xfe\n\n")))
	if _, err := reader.Next(); err == nil {
		t.Fatal("expected an error for invalid UTF-8")
	}
}

func TestWriterDataFrames(t *testing.T) {
	var buf bytes.Buffer
	writer := sse.NewWriter(&buf)
	if err := writer.WriteData(map[string]any{"a": "Привет"}); err != nil {
		t.Fatalf("WriteData: %v", err)
	}
	if err := writer.WriteData("[DONE]"); err != nil {
		t.Fatalf("WriteData: %v", err)
	}
	if err := writer.WriteKeepalive(); err != nil {
		t.Fatalf("WriteKeepalive: %v", err)
	}
	want := "data: {\"a\":\"Привет\"}\n\ndata: [DONE]\n\n: keepalive\n\n"
	if buf.String() != want {
		t.Fatalf("wrote %q, want %q", buf.String(), want)
	}
}

func readAll(t *testing.T, r io.Reader) []sse.Event {
	t.Helper()
	reader := sse.NewReader(r)
	var events []sse.Event
	for {
		event, err := reader.Next()
		if errors.Is(err, io.EOF) {
			return events
		}
		if err != nil {
			t.Fatalf("Next: %v", err)
		}
		events = append(events, event)
	}
}
