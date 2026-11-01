// Package telegram sends HTML reports through the Telegram Bot API.
//
// The recipient is fixed by deployment configuration; a tool call can never
// choose it.
package telegram

import (
	"context"
	"encoding/json"
	"fmt"
	"html"
	"io"
	"net/http"
	"net/url"
	"strings"
	"time"
	"unicode/utf8"
)

// MaxMessageRunes is the Bot API limit for one message.
const MaxMessageRunes = 4096

// Sender posts messages to one bot and one chat.
type Sender struct {
	APIURL string
	Token  string
	ChatID string
	HTTP   *http.Client
}

// New builds a sender.
func New(apiURL, token, chatID string, hc *http.Client) *Sender {
	if hc == nil {
		hc = &http.Client{Timeout: 10 * time.Second}
	}
	return &Sender{
		APIURL: strings.TrimSuffix(strings.TrimSpace(apiURL), "/"),
		Token:  strings.TrimSpace(token),
		ChatID: strings.TrimSpace(chatID),
		HTTP:   hc,
	}
}

// Send escapes the report and delivers it in as many chunks as needed.
func (s *Sender) Send(ctx context.Context, title, text string) error {
	if s.Token == "" || s.ChatID == "" {
		return fmt.Errorf("telegram token and chat id are required")
	}
	for _, chunk := range Chunk(title, text, MaxMessageRunes) {
		if err := s.sendOne(ctx, chunk); err != nil {
			return err
		}
	}
	return nil
}

func (s *Sender) sendOne(ctx context.Context, text string) error {
	form := url.Values{
		"chat_id":                  {s.ChatID},
		"text":                     {text},
		"parse_mode":               {"HTML"},
		"disable_web_page_preview": {"true"},
	}
	endpoint := fmt.Sprintf("%s/bot%s/sendMessage", s.APIURL, s.Token)
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, endpoint, strings.NewReader(form.Encode()))
	if err != nil {
		return err
	}
	req.Header.Set("Content-Type", "application/x-www-form-urlencoded")
	resp, err := s.HTTP.Do(req)
	if err != nil {
		return fmt.Errorf("telegram sendMessage: %w", err)
	}
	defer resp.Body.Close()
	body, err := io.ReadAll(io.LimitReader(resp.Body, 64<<10))
	if err != nil {
		return fmt.Errorf("telegram response: %w", err)
	}
	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("telegram sendMessage returned HTTP %d", resp.StatusCode)
	}
	var decoded struct {
		OK          bool   `json:"ok"`
		Description string `json:"description"`
	}
	if err := json.Unmarshal(body, &decoded); err != nil {
		return fmt.Errorf("telegram response is not JSON: %w", err)
	}
	if !decoded.OK {
		return fmt.Errorf("telegram sendMessage failed: %s", decoded.Description)
	}
	return nil
}

// Chunk escapes title and text as Telegram HTML and splits the result into
// messages no longer than limit runes. Splitting happens on line boundaries
// where possible; a single over-long line is hard-wrapped.
func Chunk(title, text string, limit int) []string {
	if limit <= 0 || limit > MaxMessageRunes {
		limit = MaxMessageRunes
	}
	title = strings.TrimSpace(title)
	var b strings.Builder
	if title != "" {
		b.WriteString("<b>")
		b.WriteString(html.EscapeString(title))
		b.WriteString("</b>")
	}
	body := html.EscapeString(strings.TrimRight(text, "\n"))
	if body != "" {
		if b.Len() > 0 {
			b.WriteString("\n\n")
		}
		b.WriteString(body)
	}
	full := b.String()
	if full == "" {
		return nil
	}
	return split(full, limit)
}

func split(full string, limit int) []string {
	var chunks []string
	remaining := full
	for utf8.RuneCountInString(remaining) > limit {
		cut := runeOffset(remaining, limit)
		// Prefer the last newline before the limit so HTML tags and words stay
		// intact; HTML entities introduce no raw '<' so this is safe.
		if idx := strings.LastIndex(remaining[:cut], "\n"); idx > 0 {
			cut = idx + 1
		}
		chunk := strings.TrimRight(remaining[:cut], "\n ")
		if chunk != "" {
			chunks = append(chunks, chunk)
		}
		remaining = strings.TrimLeft(remaining[cut:], "\n")
	}
	if strings.TrimSpace(remaining) != "" {
		chunks = append(chunks, remaining)
	}
	return chunks
}

func runeOffset(s string, runes int) int {
	count := 0
	for i := range s {
		if count == runes {
			return i
		}
		count++
	}
	return len(s)
}
