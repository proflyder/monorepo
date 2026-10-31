package telegram

import (
	"context"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestSendMessageRequestShape(t *testing.T) {
	var body map[string]any
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/botTOKEN/sendMessage" {
			t.Errorf("path = %s", r.URL.Path)
		}
		raw, _ := io.ReadAll(r.Body)
		if err := json.Unmarshal(raw, &body); err != nil {
			t.Errorf("body: %v", err)
		}
		_, _ = io.WriteString(w, `{"ok":true,"result":{"message_id":42},"description":""}`)
	}))
	defer srv.Close()

	client := New(srv.URL, "TOKEN", nil)
	if err := client.Send(context.Background(), "-1001", "hello <b>world</b>"); err != nil {
		t.Fatal(err)
	}
	if body["chat_id"] != "-1001" {
		t.Fatalf("chat_id = %v (must be a string)", body["chat_id"])
	}
	if body["text"] != "hello <b>world</b>" {
		t.Fatalf("text = %v", body["text"])
	}
	if body["parse_mode"] != "HTML" {
		t.Fatalf("parse_mode = %v", body["parse_mode"])
	}
}

func TestSendMessageFailure(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		_, _ = io.WriteString(w, `{"ok":false,"description":"chat not found"}`)
	}))
	defer srv.Close()

	failures := 0
	client := New(srv.URL, "TOKEN", nil)
	client.OnSendFailure = func() { failures++ }
	err := client.Send(context.Background(), "1", "x")
	if err == nil || !strings.Contains(err.Error(), "chat not found") {
		t.Fatalf("err = %v", err)
	}
	if failures != 1 {
		t.Fatalf("failure callback = %d", failures)
	}
}

func TestSetWebhookPayload(t *testing.T) {
	var body map[string]any
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/botTOKEN/setWebhook" {
			t.Errorf("path = %s", r.URL.Path)
		}
		raw, _ := io.ReadAll(r.Body)
		_ = json.Unmarshal(raw, &body)
		_, _ = io.WriteString(w, `{"ok":true,"result":true}`)
	}))
	defer srv.Close()

	client := New(srv.URL, "TOKEN", nil)
	if err := client.SetWebhook(context.Background(), "https://example.test/telegram/webhook", "shh"); err != nil {
		t.Fatal(err)
	}
	if body["url"] != "https://example.test/telegram/webhook" || body["secret_token"] != "shh" {
		t.Fatalf("payload = %v", body)
	}
}
