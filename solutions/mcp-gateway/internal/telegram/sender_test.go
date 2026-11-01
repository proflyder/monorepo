package telegram

import (
	"context"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync"
	"testing"
	"unicode/utf8"
)

func TestChunkEscapesHTML(t *testing.T) {
	chunks := Chunk("Отчёт <script>", "Строка <b>не тег</b> & далее", MaxMessageRunes)
	if len(chunks) != 1 {
		t.Fatalf("chunks = %d", len(chunks))
	}
	got := chunks[0]
	if !strings.Contains(got, "<b>Отчёт &lt;script&gt;</b>") {
		t.Fatalf("title not escaped: %q", got)
	}
	if !strings.Contains(got, "&lt;b&gt;не тег&lt;/b&gt; &amp; далее") {
		t.Fatalf("text not escaped: %q", got)
	}
	if strings.Contains(got, "<script>") {
		t.Fatalf("raw script tag survived: %q", got)
	}
}

func TestChunkSplitsAtLimit(t *testing.T) {
	var lines []string
	for i := 0; i < 40; i++ {
		lines = append(lines, "строка лога с текстом")
	}
	chunks := Chunk("Заголовок", strings.Join(lines, "\n"), 100)
	if len(chunks) < 2 {
		t.Fatalf("chunks = %d, want several", len(chunks))
	}
	for _, chunk := range chunks {
		if n := utf8.RuneCountInString(chunk); n > 100 {
			t.Fatalf("chunk has %d runes: %q", n, chunk)
		}
	}
	joined := strings.Join(chunks, "\n")
	if strings.Count(joined, "строка лога с текстом") != 40 {
		t.Fatalf("content lost: %d lines", strings.Count(joined, "строка лога с текстом"))
	}
}

func TestChunkEmpty(t *testing.T) {
	if chunks := Chunk("", "   ", 100); len(chunks) != 0 {
		t.Fatalf("chunks = %v", chunks)
	}
	if chunks := Chunk("Только заголовок", "", 100); len(chunks) != 1 || chunks[0] != "<b>Только заголовок</b>" {
		t.Fatalf("chunks = %v", chunks)
	}
}

func TestSendPostsEveryChunk(t *testing.T) {
	var mu sync.Mutex
	var bodies []map[string]string
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/botTOKEN/sendMessage" {
			t.Errorf("path = %q", r.URL.Path)
		}
		if err := r.ParseForm(); err != nil {
			t.Errorf("form: %v", err)
		}
		body := map[string]string{}
		for key := range r.PostForm {
			body[key] = r.PostForm.Get(key)
		}
		mu.Lock()
		bodies = append(bodies, body)
		mu.Unlock()
		_, _ = w.Write([]byte(`{"ok":true}`))
	}))
	defer srv.Close()

	sender := New(srv.URL, "TOKEN", "-100123", srv.Client())
	var lines []string
	for i := 0; i < 200; i++ {
		lines = append(lines, "достаточно длинная строка для разбиения отчёта")
	}
	if err := sender.Send(context.Background(), "Заголовок", strings.Join(lines, "\n")); err != nil {
		t.Fatalf("Send: %v", err)
	}
	if len(bodies) < 2 {
		t.Fatalf("requests = %d, want several", len(bodies))
	}
	for _, body := range bodies {
		if body["chat_id"] != "-100123" {
			t.Fatalf("chat_id = %q", body["chat_id"])
		}
		if body["parse_mode"] != "HTML" {
			t.Fatalf("parse_mode = %q", body["parse_mode"])
		}
	}
}

func TestSendErrors(t *testing.T) {
	if err := New("http://example", "", "1", nil).Send(context.Background(), "t", "x"); err == nil {
		t.Fatal("empty token accepted")
	}
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		_, _ = w.Write([]byte(`{"ok":false,"description":"chat not found"}`))
	}))
	defer srv.Close()
	if err := New(srv.URL, "TOKEN", "1", srv.Client()).Send(context.Background(), "t", "x"); err == nil {
		t.Fatal("ok=false accepted")
	}

	srv2 := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusUnauthorized)
	}))
	defer srv2.Close()
	if err := New(srv2.URL, "TOKEN", "1", srv2.Client()).Send(context.Background(), "t", "x"); err == nil {
		t.Fatal("HTTP 401 accepted")
	}
}
