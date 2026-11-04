package sanitize_test

import (
	"strings"
	"testing"

	"ricardo.local/monorepo/solutions/voice-proxy/internal/sanitize"
)

// TestSanitizeText covers every branch of the cleaning pipeline: citations,
// Markdown links, bare URLs, list prefixes, emphasis marks, empty parentheses
// and whitespace collapsing.
func TestSanitizeText(t *testing.T) {
	cases := map[string]string{
		// Untouched prose.
		"Простой ответ.":                       "Простой ответ.",
		"Это тест (не ссылка), всё нормально.": "Это тест (не ссылка), всё нормально.",
		"Привет!":                       "Привет!",
		"Вопрос?":                       "Вопрос?",
		"Полный вопрос？":                "Полный вопрос？",
		"Многоточие…":                   "Многоточие…",
		"Дефис - и тире — внутри.":      "Дефис - и тире — внутри.",
		"Кавычки «ёлочки» и \"лапки\".": "Кавычки «ёлочки» и \"лапки\".",
		"Точка с запятой; двоеточие: и запятая,": "Точка с запятой; двоеточие: и запятая,",
		"Эмодзи 😀 в тексте.":                     "Эмодзи 😀 в тексте.",
		"👨\u200d👩\u200d👧\u200d👦 семья":           "👨\u200d👩\u200d👧\u200d👦 семья",
		"É\u0301 combining":                      "É\u0301 combining",
		"Ｆｕｌｌｗｉｄｔｈ ｔｅｘｔ":                         "Ｆｕｌｌｗｉｄｔｈ ｔｅｘｔ",
		"𝔘𝔫𝔦𝔠𝔬𝔡𝔢":                                "𝔘𝔫𝔦𝔠𝔬𝔡𝔢",
		// Empty input.
		"":     "",
		"   ":  "",
		"\n\n": "",
		// Whole citation parentheses go before link unwrapping.
		"Новость. ([investing.com](https://example.com/a?b=1))":              "Новость.",
		"Новость. ([investing.com](https://example.com/a?b=1)) Ещё новость.": "Новость. Ещё новость.",
		"Цитата ([сайт](http://сайт.рф/путь)) конец.":                        "Цитата конец.",
		// Markdown links keep their text.
		"Источник: [Reuters](https://reuters.com/x).":                "Источник: Reuters.",
		"Ссылка [x](https://a.example) в середине.":                  "Ссылка x в середине.",
		"Две ссылки [a](https://a.example) и [b](http://b.example).": "Две ссылки a и b.",
		"**Готово**: отчёт [тут](https://example.com/x).":            "Готово: отчёт тут.",
		"Ответ [источник](https://example)":                          "Ответ источник",
		// Bare URLs are deleted, including in brackets, quotes and angle brackets.
		"Смотри https://example.com/news и всё.":           "Смотри и всё.",
		"URL в скобках (https://example.com/x) конец.":     "URL в скобках конец.",
		"URL с query https://example.com/a?b=1&c=2 конец.": "URL с query конец.",
		"URL в угловых <https://example.com/x> конец.":     "URL в угловых <> конец.",
		"URL в кавычках \"https://example.com/x\" конец.":  "URL в кавычках \"\" конец.",
		"Схема http://example.com":                         "Схема",
		"Хвост https://example.com/path?q=1#frag.":         "Хвост",
		// Incomplete schemes are not URLs and survive.
		"Не URL: https//example.com и http:/example.com.": "Не URL: https//example.com и http:/example.com.",
		"Хвост http":                             "Хвост http",
		"Хвост https:":                           "Хвост https:",
		"Хвост https://":                         "Хвост https://",
		"Ответ [источник]":                       "Ответ [источник]",
		"Ответ [источник](https://example.com/x": "Ответ [источник](",
		// Line prefixes: headings, bullets and numbered items.
		"## Заголовок\n- **важно**\n- `код`":                        "Заголовок важно код",
		"# h1\n## h2\n### h3\n#### h4\n##### h5\n###### h6":         "h1 h2 h3 h4 h5 h6",
		"* звёздочка\n+ плюс\n- минус\n1. один\n2) два\n10. десять": "звёздочка плюс минус один два десять",
		// Emphasis marks.
		"~~зачёркнуто~~ и _курсив_ и *жирный*": "зачёркнуто и курсив и жирный",
		"a*b*c_d_e`f`g~h~i":                    "abcdefghi",
		"***тройные*** звёздочки":              "тройные звёздочки",
		"__подчёркивания__":                    "подчёркивания",
		"`код с [ссылкой](https://x.example)`": "код с ссылкой",
		// Empty parentheses disappear.
		"Пустые скобки () и ( ) и (  ) остаются?": "Пустые скобки и и остаются?",
		"Пустые скобки () и ( ) и (  )":           "Пустые скобки и и",
		// Unicode whitespace, including the C0 information separators that
		// strings.TrimSpace leaves behind.
		"привет\u00a0мир":      "привет мир",
		"привет\u202fмир":      "привет мир",
		"привет\u2009мир":      "привет мир",
		"привет\u3000мир":      "привет мир",
		"привет\u200bмир":      "привет\u200bмир",
		"привет\tмир":          "привет мир",
		"привет\x0bмир":        "привет мир",
		"привет\x0cмир":        "привет мир",
		"привет\x1cмир":        "привет мир",
		"привет\x1dмир":        "привет мир",
		"привет\x1eмир":        "привет мир",
		"привет\x1fмир":        "привет мир",
		"привет\u0085мир":      "привет мир",
		"привет\u2028мир":      "привет мир",
		"привет\u2029мир":      "привет мир",
		"  привет \n\n  мир  ": "привет мир",
		"\u00a0привет\u00a0":   "привет",
		"- \u00a0пункт":        "пункт",
		"#\u00a0Заголовок":     "Заголовок",
		"1.\u00a0пункт":        "пункт",
	}
	for input, want := range cases {
		if got := sanitize.SanitizeText(input); got != want {
			t.Errorf("SanitizeText(%q) = %q, want %q", input, got, want)
		}
	}
}

// TestSanitizeChatPayload touches every choice.
func TestSanitizeChatPayload(t *testing.T) {
	payload := map[string]any{
		"choices": []any{
			map[string]any{"message": map[string]any{"content": "A https://a.example"}},
			map[string]any{"message": map[string]any{"content": "B [x](https://b.example)"}},
			map[string]any{"message": map[string]any{"content": 5}},
			map[string]any{},
			"not an object",
		},
	}
	out := sanitize.SanitizeChatPayload(payload)
	choices := out["choices"].([]any)
	first := choices[0].(map[string]any)["message"].(map[string]any)["content"]
	second := choices[1].(map[string]any)["message"].(map[string]any)["content"]
	if first != "A" || second != "B x" {
		t.Fatalf("contents = %q, %q; want %q, %q", first, second, "A", "B x")
	}
	if choices[2].(map[string]any)["message"].(map[string]any)["content"] != 5 {
		t.Fatal("non-string content must be left alone")
	}
}

// TestStreamingFirstSentenceReleasedBeforeCompletion: only complete sentences
// are released early, and the tail is joined with a space.
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

// TestStreamingMatchesWholeTextAtEverySplit: feeding the text in every possible
// split and char-by-char produces the same speech as one buffered sanitize.
func TestStreamingMatchesWholeTextAtEverySplit(t *testing.T) {
	samples := []string{
		"Новость. ([investing.com](https://example.com/a?b=1)) Ещё новость.",
		"Источник: [Reuters](https://reuters.com/x). Следующий ответ.",
		"Смотри https://example.com/news и всё. Второе предложение.",
		"Ответ (не ссылка). **Второе** предложение.",
		"Вопрос? Да! Точно… Последнее.",
		"Полный вопрос？ И ответ.",
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

// TestStreamingUnfinishedLinkIsDiscarded: a half URL is never spoken.
func TestStreamingUnfinishedLinkIsDiscarded(t *testing.T) {
	for _, input := range []string{"Ответ [источник](https://example", "Ответ https://example"} {
		s := &sanitize.StreamingSanitizer{}
		got, err := s.Feed(input, true)
		if err != nil {
			t.Fatalf("Feed: %v", err)
		}
		if got != "Ответ" {
			t.Errorf("Feed(%q) = %q, want %q", input, got, "Ответ")
		}
	}
}

// TestStreamingRejectsOversizedTail bounds a runaway stream.
func TestStreamingRejectsOversizedTail(t *testing.T) {
	s := &sanitize.StreamingSanitizer{}
	if _, err := s.Feed(strings.Repeat("a", sanitize.MaxStreamBytes+1), false); err != sanitize.ErrStreamTooLarge {
		t.Fatalf("Feed = %v, want ErrStreamTooLarge", err)
	}
}

// TestEnsureNotQuestion covers each shape of the acknowledgement guard.
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

// TestAnnounceText covers sanitising and every truncation branch.
func TestAnnounceText(t *testing.T) {
	if got := sanitize.AnnounceText("Смотри https://example.com/x и **готово**.", 200); got != "Смотри и готово." {
		t.Fatalf("AnnounceText = %q", got)
	}
	if got := sanitize.AnnounceText("", 200); got != "" {
		t.Fatalf("AnnounceText(\"\") = %q", got)
	}
	// A long statement is cut at a word boundary and ends with an ellipsis.
	got := sanitize.AnnounceText(strings.Repeat("слово ", 80), 200)
	if !strings.HasSuffix(got, "…") || strings.HasSuffix(got, "?") || len([]rune(got)) > 201 {
		t.Fatalf("long statement = %q", got)
	}
	// The final question sentence is preferred when it fits.
	if got := sanitize.AnnounceText("Первый факт. "+strings.Repeat("слово ", 80)+". Подтверждаете?", 200); got != "Подтверждаете?" {
		t.Fatalf("kept question = %q", got)
	}
	// An over-long question is cut but keeps its trailing "?".
	got = sanitize.AnnounceText(strings.Repeat("слово ", 80)+"Подтверждаете?", 200)
	if !strings.HasSuffix(got, "?") || len([]rune(got)) > 201 {
		t.Fatalf("long question = %q", got)
	}
	// A short text under a small limit is returned unchanged.
	if got := sanitize.AnnounceText("Готово.", 200); got != "Готово." {
		t.Fatalf("short text = %q", got)
	}
}
