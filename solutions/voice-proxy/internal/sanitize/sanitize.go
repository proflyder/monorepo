// Package sanitize turns assistant replies into text that Piper TTS can speak.
//
// Go's RE2 \s and \b are ASCII-only, so every whitespace class below is written
// out explicitly instead: the ASCII whitespace characters, the C0 information
// separators U+001C..U+001F, NEL (U+0085) and the Unicode separator categories
// Zs, Zl and Zp.
package sanitize

import (
	"errors"
	"regexp"
	"strings"
	"unicode"
	"unicode/utf8"

	"golang.org/x/text/cases"
	"golang.org/x/text/unicode/norm"
)

// MaxStreamBytes bounds the buffered streaming tail like the 10 MiB request
// cap; a runaway stream fails instead of growing without bound.
const MaxStreamBytes = 10 << 20

// ErrStreamTooLarge reports a streaming tail that exceeded MaxStreamBytes.
var ErrStreamTooLarge = errors.New("stream text too large")

// spaceClass matches one character of the Unicode space set: ASCII whitespace,
// the C0 information separators, NEL and the Zs/Zl/Zp categories.
const spaceClass = `[ \t\n\v\f\r\x1c-\x1f\x85\p{Z}]`

// notSpaceClass is the negated form used inside larger character classes.
const notSpaceClass = `[^ \t\n\v\f\r\x1c-\x1f\x85\p{Z}`

// trailingPunct is the sentence-final punctuation STT may append.
const trailingPunct = ".!?…"

var (
	// Order matters: remove whole citation parentheses before unwrapping links.
	citation    = regexp.MustCompile(spaceClass + `*\(\[[^\]]+\]\(https?://[^)]+\)\)`)
	mdLink      = regexp.MustCompile(`\[([^\]]+)\]\(https?://[^)]+\)`)
	bareURL     = regexp.MustCompile(`https?://` + notSpaceClass + `)\]}>"']+`)
	mdLinePfx   = regexp.MustCompile(`(?m)^` + spaceClass + `*(?:#{1,6}` + spaceClass + `*|[-*+]` + spaceClass + `+|\d+[.)]` + spaceClass + `+)`)
	mdMarks     = regexp.MustCompile("[*_`~]+")
	emptyParens = regexp.MustCompile(`\(` + spaceClass + `*\)`)
	runOfSpace  = regexp.MustCompile(spaceClass + `+`)

	// truncatedLink and truncatedHTTP drop an unterminated link or scheme tail
	// at end of stream: a half URL must never be spoken.
	truncatedLink = regexp.MustCompile(`\[[^\]]*$|\[[^\]]+\]\([^)]*$`)
	truncatedHTTP = regexp.MustCompile(`https?(?::/{0,2})?$`)

	// questionTail finds the final question sentence of an over-long answer.
	questionTail = regexp.MustCompile(`[^.!?…？]*[?？]` + spaceClass + `*$`)
)

// IsSpace reports whether r belongs to the Unicode space set above.
func IsSpace(r rune) bool {
	switch r {
	case '\t', '\n', '\v', '\f', '\r', ' ', '\x1c', '\x1d', '\x1e', '\x1f', '\x85':
		return true
	}
	return unicode.Is(unicode.Zs, r) || unicode.Is(unicode.Zl, r) || unicode.Is(unicode.Zp, r)
}

// TrimSpace trims the Unicode space set, including the C0 information
// separators that strings.TrimSpace leaves behind.
func TrimSpace(s string) string { return strings.TrimFunc(s, IsSpace) }

// CollapseSpace is " ".join(s.split()): split on runs of whitespace and join
// with a single ASCII space.
func CollapseSpace(s string) string { return strings.Join(strings.FieldsFunc(s, IsSpace), " ") }

// SanitizeText returns a TTS-friendly version of an assistant reply.
func SanitizeText(text string) string {
	if text == "" {
		return ""
	}
	text = citation.ReplaceAllLiteralString(text, "")
	text = mdLink.ReplaceAllString(text, "$1")
	text = bareURL.ReplaceAllLiteralString(text, "")
	text = mdLinePfx.ReplaceAllLiteralString(text, "")
	text = mdMarks.ReplaceAllLiteralString(text, "")
	text = emptyParens.ReplaceAllLiteralString(text, "")
	text = runOfSpace.ReplaceAllString(text, " ")
	return TrimSpace(text)
}

// SanitizeChatPayload sanitizes assistant content in an OpenAI Chat
// Completions payload, in place, exactly like sanitize_chat_payload.
func SanitizeChatPayload(payload map[string]any) map[string]any {
	choices, _ := payload["choices"].([]any)
	for _, item := range choices {
		choice, ok := item.(map[string]any)
		if !ok {
			continue
		}
		message, ok := choice["message"].(map[string]any)
		if !ok {
			continue
		}
		if content, ok := message["content"].(string); ok {
			message["content"] = SanitizeText(content)
		}
	}
	return payload
}

// StreamingSanitizer releases complete sentences while keeping split links and
// URLs out of speech. A boundary must be outside links/parentheses and followed
// by whitespace; the tail is held until completion, so the regexes never see
// half a URL.
type StreamingSanitizer struct {
	pending string
	emitted bool
}

// Feed adds text and returns the speech that became safe to speak.
func (s *StreamingSanitizer) Feed(text string, final bool) (string, error) {
	s.pending += text
	if len(s.pending) > MaxStreamBytes {
		return "", ErrStreamTooLarge
	}
	end := 0
	if final {
		end = len(s.pending)
	} else {
		end = boundaryEnd(s.pending)
	}
	if end == 0 {
		return "", nil
	}
	raw, rest := s.pending[:end], s.pending[end:]
	s.pending = rest
	if final {
		// A truncated/incomplete link is never safe to speak at end of stream.
		raw = truncatedLink.ReplaceAllLiteralString(raw, "")
		raw = truncatedHTTP.ReplaceAllLiteralString(raw, "")
	}
	cleaned := SanitizeText(raw)
	if cleaned == "" {
		return "", nil
	}
	result := cleaned
	if s.emitted {
		result = " " + cleaned
	}
	s.emitted = true
	return result, nil
}

// boundaryEnd is the equivalent of the scanner
//
//	r"https?://[^\s)\]]*|[\[\]()]|[.!?…]+(?=\s)"
//
// and returns the byte index after the last sentence boundary that is outside a
// link/parenthesis. RE2 has no lookahead, so the scanner is written by hand; it
// keeps the leftmost-first order of the alternatives.
func boundaryEnd(text string) int {
	brackets, parens, end := 0, 0, 0
	for i := 0; i < len(text); {
		if schemeLen := urlSchemeLen(text[i:]); schemeLen > 0 {
			i += schemeLen
			for i < len(text) {
				r, size := utf8.DecodeRuneInString(text[i:])
				if IsSpace(r) || r == ')' || r == ']' {
					break
				}
				i += size
			}
			continue
		}
		r, size := utf8.DecodeRuneInString(text[i:])
		switch r {
		case '[', ']', '(', ')':
			switch r {
			case '[':
				brackets++
			case ']':
				if brackets > 0 {
					brackets--
				}
			case '(':
				parens++
			case ')':
				if parens > 0 {
					parens--
				}
			}
			i += size
		case '.', '!', '?', '…':
			for i < len(text) {
				next, nextSize := utf8.DecodeRuneInString(text[i:])
				if next != '.' && next != '!' && next != '?' && next != '…' {
					break
				}
				i += nextSize
			}
			// The lookahead requires a whitespace character after the run, so a
			// sentence-final mark at the very end of the buffer is not a
			// boundary yet.
			if i < len(text) {
				next, _ := utf8.DecodeRuneInString(text[i:])
				if IsSpace(next) && brackets == 0 && parens == 0 {
					end = i
				}
			}
		default:
			i += size
		}
	}
	return end
}

// urlSchemeLen reports the length of a "http://" or "https://" prefix at the
// start of text, or 0.
func urlSchemeLen(text string) int {
	for _, scheme := range []string{"https://", "http://"} {
		if strings.HasPrefix(text, scheme) {
			return len(scheme)
		}
	}
	return 0
}

// NormalizeUtterance returns the compact lowercase form used only for exact
// cancel matching: NFKC, case folding, whitespace collapsing and sentence-final
// STT punctuation removal. Quotes, hyphens and internal commas survive so a
// quoted or reworded phrase is not treated as an exact cancel command.
func NormalizeUtterance(text string) string {
	text = norm.NFKC.String(text)
	text = cases.Fold().String(text)
	text = TrimSpace(text)
	text = strings.TrimRightFunc(text, func(r rune) bool {
		return strings.ContainsRune(trailingPunct, r)
	})
	return CollapseSpace(text)
}

// EnsureNotQuestion returns a short spoken phrase that cannot trigger HA's
// continued-conversation mode.
func EnsureNotQuestion(text, agentsDown string) string {
	text = CollapseSpace(text)
	for strings.HasSuffix(text, "?") || strings.HasSuffix(text, "？") {
		text = TrimSpace(strings.TrimSuffix(text, "？"))
		text = TrimSpace(strings.TrimSuffix(text, "?"))
	}
	if text == "" {
		return agentsDown
	}
	runes := []rune(text)
	if !strings.ContainsRune(".!…", runes[len(runes)-1]) {
		text += "."
	}
	return text
}

// EndsWithQuestion reports whether text ends, after trailing whitespace, with a
// question mark. HA continues the dialog only in that case.
func EndsWithQuestion(text string) bool {
	text = strings.TrimRightFunc(text, IsSpace)
	return strings.HasSuffix(text, "?") || strings.HasSuffix(text, "？")
}

// truncateSpeech cuts text to at most maxChars runes, preferring the last word
// boundary, and strips the punctuation that looks wrong before an ellipsis.
func truncateSpeech(text string, maxChars int) string {
	runes := []rune(text)
	if len(runes) > maxChars {
		runes = runes[:maxChars]
	}
	cut := string(runes)
	if idx := strings.LastIndex(cut, " "); idx >= 0 {
		cut = cut[:idx]
	}
	return strings.TrimRight(cut, " ,;:—-")
}

// AnnounceText sanitizes and shortens a late answer for a station
// announcement. A question must keep its trailing "?": HA only continues the
// dialog when the spoken text ends with one. If the full answer is too long,
// prefer the final question sentence; only cut inside it as a last resort.
func AnnounceText(speech string, maxChars int) string {
	text := SanitizeText(speech)
	if len([]rune(text)) <= maxChars {
		return text
	}
	if EndsWithQuestion(text) {
		lastQuestion := text
		if loc := questionTail.FindStringIndex(text); loc != nil {
			lastQuestion = TrimSpace(text[loc[0]:loc[1]])
		}
		if len([]rune(lastQuestion)) <= maxChars {
			return lastQuestion
		}
		cut := truncateSpeech(lastQuestion, maxChars)
		if EndsWithQuestion(cut) {
			return cut
		}
		trimmed := strings.TrimRightFunc(text, IsSpace)
		runes := []rune(trimmed)
		return cut + string(runes[len(runes)-1])
	}
	return truncateSpeech(text, maxChars) + "…"
}
