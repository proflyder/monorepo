package sanitize_test

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"ricardo.local/monorepo/solutions/voice-proxy/internal/sanitize"
)

// SanitizeTests mirrors SanitizeTests in test_voice_proxy.py.
func TestSanitizeRemovesMarkdownLinkAndURL(t *testing.T) {
	text := "Новость. ([investing.com](https://example.com/a?b=1))"
	if got, want := sanitize.SanitizeText(text), "Новость."; got != want {
		t.Fatalf("SanitizeText(%q) = %q, want %q", text, got, want)
	}
}

func TestSanitizeRemovesBareURL(t *testing.T) {
	text := "Смотри https://example.com/news и всё."
	if got, want := sanitize.SanitizeText(text), "Смотри и всё."; got != want {
		t.Fatalf("SanitizeText(%q) = %q, want %q", text, got, want)
	}
}

func TestSanitizeUnwrapsLinkTextWithoutURL(t *testing.T) {
	text := "Источник: [Reuters](https://reuters.com/x)."
	if got, want := sanitize.SanitizeText(text), "Источник: Reuters."; got != want {
		t.Fatalf("SanitizeText(%q) = %q, want %q", text, got, want)
	}
}

func TestSanitizeStripsMarkdownMarksAndListPrefix(t *testing.T) {
	text := "## Заголовок\n- **важно**\n- `код`"
	if got, want := sanitize.SanitizeText(text), "Заголовок важно код"; got != want {
		t.Fatalf("SanitizeText(%q) = %q, want %q", text, got, want)
	}
}

func TestSanitizeKeepsNormalPunctuationAndParentheses(t *testing.T) {
	text := "Это тест (не ссылка), всё нормально."
	if got := sanitize.SanitizeText(text); got != text {
		t.Fatalf("SanitizeText(%q) = %q, want it unchanged", text, got)
	}
}

func TestSanitizeCollapsesWhitespace(t *testing.T) {
	if got, want := sanitize.SanitizeText("  привет \n\n  мир  "), "привет мир"; got != want {
		t.Fatalf("got %q, want %q", got, want)
	}
}

func TestSanitizeEmptyValues(t *testing.T) {
	if got := sanitize.SanitizeText(""); got != "" {
		t.Fatalf("SanitizeText(\"\") = %q, want empty", got)
	}
}

func TestSanitizeAllChoices(t *testing.T) {
	payload := map[string]any{
		"choices": []any{
			map[string]any{"message": map[string]any{"content": "A https://a.example"}},
			map[string]any{"message": map[string]any{"content": "B [x](https://b.example)"}},
		},
	}
	out := sanitize.SanitizeChatPayload(payload)
	choices := out["choices"].([]any)
	first := choices[0].(map[string]any)["message"].(map[string]any)["content"]
	second := choices[1].(map[string]any)["message"].(map[string]any)["content"]
	if first != "A" || second != "B x" {
		t.Fatalf("contents = %q, %q; want %q, %q", first, second, "A", "B x")
	}
}

// StreamingTests mirrors the streaming half of StreamingTests in
// test_voice_proxy.py.
func TestStreamingFirstSentenceReleasedBeforeCompletion(t *testing.T) {
	s := &sanitize.StreamingSanitizer{}
	feed := func(text string, final bool) string {
		t.Helper()
		got, err := s.Feed(text, final)
		if err != nil {
			t.Fatalf("Feed(%q, %t): %v", text, final, err)
		}
		return got
	}
	if got := feed("Первое предло", false); got != "" {
		t.Fatalf("first chunk = %q, want empty", got)
	}
	if got, want := feed("жение. Второе", false), "Первое предложение."; got != want {
		t.Fatalf("second chunk = %q, want %q", got, want)
	}
	if got, want := feed(" предложение.", true), " Второе предложение."; got != want {
		t.Fatalf("final chunk = %q, want %q", got, want)
	}
}

func TestStreamingLinksCitationsAndURLsAtEverySplit(t *testing.T) {
	samples := []string{
		"Новость. ([investing.com](https://example.com/a?b=1)) Ещё новость.",
		"Источник: [Reuters](https://reuters.com/x). Следующий ответ.",
		"Смотри https://example.com/news и всё. Второе предложение.",
		"Ответ (не ссылка). **Второе** предложение.",
	}
	for _, text := range samples {
		runes := []rune(text)
		want := sanitize.SanitizeText(text)
		for split := 0; split <= len(runes); split++ {
			s := &sanitize.StreamingSanitizer{}
			var got strings.Builder
			for _, part := range []string{string(runes[:split]), string(runes[split:]), ""} {
				out, err := s.Feed(part, part == "")
				if err != nil {
					t.Fatalf("%q split %d: %v", text, split, err)
				}
				got.WriteString(out)
			}
			if got.String() != want {
				t.Fatalf("%q split %d = %q, want %q", text, split, got.String(), want)
			}
		}
		s := &sanitize.StreamingSanitizer{}
		var got strings.Builder
		for _, r := range text {
			out, err := s.Feed(string(r), false)
			if err != nil {
				t.Fatalf("%q char-by-char: %v", text, err)
			}
			got.WriteString(out)
		}
		out, err := s.Feed("", true)
		if err != nil {
			t.Fatalf("%q final: %v", text, err)
		}
		got.WriteString(out)
		if got.String() != want {
			t.Fatalf("%q char-by-char = %q, want %q", text, got.String(), want)
		}
	}
}

func TestStreamingUnfinishedLinkIsDiscarded(t *testing.T) {
	s := &sanitize.StreamingSanitizer{}
	got, err := s.Feed("Ответ [источник](https://example", true)
	if err != nil {
		t.Fatalf("Feed: %v", err)
	}
	if got != "Ответ" {
		t.Fatalf("got %q, want %q", got, "Ответ")
	}
}

// New Unicode parity cases: Go's RE2 \s is ASCII-only, so every one of these
// would leak a separator into speech if the package used \s.
func TestSanitizeUnicodeWhitespaceParity(t *testing.T) {
	cases := map[string]string{
		"привет\u00a0мир":                       "привет мир", // NBSP
		"привет\u202fмир":                       "привет мир", // narrow NBSP
		"привет\u2009мир":                       "привет мир", // thin space
		"привет\u3000мир":                       "привет мир", // ideographic space
		"привет\u0085мир":                       "привет мир", // NEL
		"привет\x1cмир":                         "привет мир", // file separator
		"  привет \n\n  мир  ":                  "привет мир",
		"\u00a0привет\u00a0":                    "привет",
		"- \u00a0пункт":                         "пункт",
		"#\u00a0Заголовок":                      "Заголовок",
		"1.\u00a0пункт":                         "пункт",
		"Пустые скобки ()\u00a0и ( ) остаются?": "Пустые скобки и остаются?",
	}
	for input, want := range cases {
		if got := sanitize.SanitizeText(input); got != want {
			t.Errorf("SanitizeText(%q) = %q, want %q", input, got, want)
		}
	}
}

func TestSanitizeKeepsEmojiAndCombiningMarks(t *testing.T) {
	for _, text := range []string{
		"Эмодзи 😀 в тексте.",
		"👨‍👩‍👧‍👦 семья",
		"É\u0301 combining",
		"Ｆｕｌｌｗｉｄｔｈ ｔｅｘｔ",
	} {
		if got := sanitize.SanitizeText(text); got != text {
			t.Errorf("SanitizeText(%q) = %q, want it unchanged", text, got)
		}
	}
}

func TestEnsureNotQuestion(t *testing.T) {
	cases := map[string]string{
		"Готово?":      "Готово.",
		"Готово？":      "Готово.",
		"  Готово ?  ": "Готово.",
		"Готово!":      "Готово!",
		"Готово…":      "Готово…",
		"Готово":       "Готово.",
		"？":            "Сервер агентов недоступен.",
		"":             "Сервер агентов недоступен.",
		"Что？Правда？":  "Что？Правда.",
	}
	for input, want := range cases {
		if got := sanitize.EnsureNotQuestion(input, "Сервер агентов недоступен."); got != want {
			t.Errorf("EnsureNotQuestion(%q) = %q, want %q", input, got, want)
		}
	}
}

func TestAnnounceText(t *testing.T) {
	if got := sanitize.AnnounceText("Смотри https://example.com/x и **готово**.", 200); got != "Смотри и готово." {
		t.Fatalf("AnnounceText = %q", got)
	}
	if got := sanitize.AnnounceText("", 200); got != "" {
		t.Fatalf("AnnounceText(\"\") = %q", got)
	}
	if got := sanitize.AnnounceText(strings.Repeat("слово ", 80), 200); len([]rune(got)) > 201 {
		t.Fatalf("len = %d, want <= 201", len([]rune(got)))
	}
	// Prefer the final question sentence when it fits.
	if got := sanitize.AnnounceText("Первый факт. "+strings.Repeat("слово ", 80)+". Подтверждаете?", 200); got != "Подтверждаете?" {
		t.Fatalf("AnnounceText kept-question = %q", got)
	}
	// If even the question is too long, cut it but keep the trailing "?".
	got := sanitize.AnnounceText(strings.Repeat("слово ", 80)+"Подтверждаете?", 200)
	if !strings.HasSuffix(got, "?") || len([]rune(got)) > 201 {
		t.Fatalf("AnnounceText long-question = %q", got)
	}
	// A statement keeps the previous ellipsis truncation and no "?".
	got = sanitize.AnnounceText(strings.Repeat("слово ", 80), 200)
	if !strings.HasSuffix(got, "…") || strings.HasSuffix(got, "?") {
		t.Fatalf("AnnounceText long-statement = %q", got)
	}
}

// TestParityGoldenSanitize asserts byte-identical output for a corpus generated
// by calling the Python implementation directly. See testdata/parity/README.md.
func TestParityGoldenSanitize(t *testing.T) {
	dir := parityDir(t)
	var fixture struct {
		Cases []struct {
			Input string `json:"input"`
			Want  string `json:"want"`
		} `json:"cases"`
	}
	readJSON(t, filepath.Join(dir, "sanitize.json"), &fixture)
	if len(fixture.Cases) < 200 {
		t.Fatalf("parity corpus has %d cases, want at least 200", len(fixture.Cases))
	}
	for _, tc := range fixture.Cases {
		if got := sanitize.SanitizeText(tc.Input); got != tc.Want {
			t.Errorf("SanitizeText(%q) = %q, want %q", tc.Input, got, tc.Want)
		}
	}
}

// TestParityGoldenStreaming replays every corpus string char-by-char and, when
// the Python implementation is split-independent for that string, at every
// split index. The expectation always comes from Python, never from the Go
// implementation.
func TestParityGoldenStreaming(t *testing.T) {
	dir := parityDir(t)
	var fixture struct {
		Cases []struct {
			Input    string `json:"input"`
			Want     string `json:"want"`
			SplitsOK bool   `json:"splits_ok"`
		} `json:"cases"`
	}
	readJSON(t, filepath.Join(dir, "streaming.json"), &fixture)
	if len(fixture.Cases) < 200 {
		t.Fatalf("streaming corpus has %d cases, want at least 200", len(fixture.Cases))
	}
	for _, tc := range fixture.Cases {
		var got strings.Builder
		s := &sanitize.StreamingSanitizer{}
		for _, r := range tc.Input {
			out, err := s.Feed(string(r), false)
			if err != nil {
				t.Fatalf("%q char-by-char: %v", tc.Input, err)
			}
			got.WriteString(out)
		}
		out, err := s.Feed("", true)
		if err != nil {
			t.Fatalf("%q final: %v", tc.Input, err)
		}
		got.WriteString(out)
		if got.String() != tc.Want {
			t.Errorf("%q char-by-char = %q, want %q", tc.Input, got.String(), tc.Want)
			continue
		}
		if !tc.SplitsOK {
			continue
		}
		runes := []rune(tc.Input)
		for split := 0; split <= len(runes); split++ {
			s := &sanitize.StreamingSanitizer{}
			var mixed strings.Builder
			for _, part := range []string{string(runes[:split]), string(runes[split:]), ""} {
				out, err := s.Feed(part, part == "")
				if err != nil {
					t.Fatalf("%q split %d: %v", tc.Input, split, err)
				}
				mixed.WriteString(out)
			}
			if mixed.String() != tc.Want {
				t.Fatalf("%q split %d = %q, want %q", tc.Input, split, mixed.String(), tc.Want)
			}
		}
	}
}

// TestParityGoldenSpeech pins ensure_not_question and announce_text.
func TestParityGoldenSpeech(t *testing.T) {
	dir := parityDir(t)
	var fixture struct {
		Question []struct {
			Input string `json:"input"`
			Want  string `json:"want"`
		} `json:"question"`
		Announce []struct {
			Input string `json:"input"`
			Want  string `json:"want"`
		} `json:"announce"`
	}
	readJSON(t, filepath.Join(dir, "speech.json"), &fixture)
	const down = "Сервер агентов недоступен."
	for _, tc := range fixture.Question {
		if got := sanitize.EnsureNotQuestion(tc.Input, down); got != tc.Want {
			t.Errorf("EnsureNotQuestion(%q) = %q, want %q", tc.Input, got, tc.Want)
		}
	}
	for _, tc := range fixture.Announce {
		if got := sanitize.AnnounceText(tc.Input, 200); got != tc.Want {
			t.Errorf("AnnounceText(%q) = %q, want %q", tc.Input, got, tc.Want)
		}
	}
}

func parityDir(t *testing.T) string {
	t.Helper()
	dir, err := os.Getwd()
	if err != nil {
		t.Fatalf("getwd: %v", err)
	}
	for range 12 {
		for _, candidate := range []string{
			filepath.Join(dir, "testdata", "parity"),
			filepath.Join(dir, "solutions", "voice-proxy", "testdata", "parity"),
		} {
			if _, err := os.Stat(filepath.Join(candidate, "sanitize.json")); err == nil {
				return candidate
			}
		}
		parent := filepath.Dir(dir)
		if parent == dir {
			break
		}
		dir = parent
	}
	t.Fatal("parity testdata not found")
	return ""
}

func readJSON(t *testing.T, path string, target any) {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read %s: %v", path, err)
	}
	if err := json.Unmarshal(data, target); err != nil {
		t.Fatalf("decode %s: %v", path, err)
	}
}

// TestAnnounceTextSanitizesAndShortens mirrors
// test_announce_text_sanitizes_and_shortens.
func TestAnnounceTextSanitizesAndShortens(t *testing.T) {
	if got := sanitize.AnnounceText(strings.Repeat("слово ", 80), 200); len([]rune(got)) > 201 {
		t.Fatalf("len = %d, want <= 201", len([]rune(got)))
	}
	if got := sanitize.AnnounceText("Смотри https://example.com/x и **готово**.", 200); got != "Смотри и готово." {
		t.Fatalf("AnnounceText = %q", got)
	}
	if got := sanitize.AnnounceText("", 200); got != "" {
		t.Fatalf("AnnounceText(\"\") = %q", got)
	}
}

// TestLongQuestionTruncationKeepsTheQuestionMark mirrors
// test_long_question_truncation_keeps_the_question_mark.
func TestLongQuestionTruncationKeepsTheQuestionMark(t *testing.T) {
	// Prefer the final question sentence when it fits.
	if got := sanitize.AnnounceText("Первый факт. "+strings.Repeat("слово ", 80)+". Подтверждаете?", 200); got != "Подтверждаете?" {
		t.Fatalf("kept question = %q", got)
	}
	// If even the question is too long, cut it but keep the trailing "?".
	got := sanitize.AnnounceText(strings.Repeat("слово ", 80)+"Подтверждаете?", 200)
	if !strings.HasSuffix(got, "?") || len([]rune(got)) > 201 {
		t.Fatalf("long question = %q", got)
	}
	// A statement keeps the previous ellipsis truncation and no "?".
	got = sanitize.AnnounceText(strings.Repeat("слово ", 80), 200)
	if !strings.HasSuffix(got, "…") || strings.HasSuffix(got, "?") {
		t.Fatalf("long statement = %q", got)
	}
}
