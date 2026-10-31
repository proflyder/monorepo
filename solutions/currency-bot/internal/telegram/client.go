// Package telegram is a minimal Telegram Bot API client: sendMessage and the
// optional setWebhook self-registration.
package telegram

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"log/slog"
	"net/http"
	"time"
)

// DefaultBaseURL is the public Bot API endpoint.
const DefaultBaseURL = "https://api.telegram.org"

// Client calls the Telegram Bot API.
type Client struct {
	BaseURL       string
	Token         string
	HTTP          *http.Client
	Logger        *slog.Logger
	OnSendFailure func()
}

// New returns a client with a 20s timeout.
func New(baseURL, token string, logger *slog.Logger) *Client {
	if baseURL == "" {
		baseURL = DefaultBaseURL
	}
	if logger == nil {
		logger = slog.New(slog.NewTextHandler(io.Discard, nil))
	}
	return &Client{
		BaseURL: baseURL,
		Token:   token,
		HTTP:    &http.Client{Timeout: 20 * time.Second},
		Logger:  logger,
	}
}

type sendMessageRequest struct {
	ChatID    string `json:"chat_id"`
	Text      string `json:"text"`
	ParseMode string `json:"parse_mode"`
}

type telegramResponse struct {
	OK          bool            `json:"ok"`
	Description string          `json:"description"`
	Result      json.RawMessage `json:"result"`
}

// Send posts one HTML-formatted message to a chat. The chat id is always sent
// as a string, matching the Kotlin DTO.
func (c *Client) Send(ctx context.Context, chatID, text string) error {
	body, err := json.Marshal(sendMessageRequest{ChatID: chatID, Text: text, ParseMode: "HTML"})
	if err != nil {
		c.fail()
		return err
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, c.BaseURL+"/bot"+c.Token+"/sendMessage", bytes.NewReader(body))
	if err != nil {
		c.fail()
		return err
	}
	req.Header.Set("Content-Type", "application/json")
	resp, err := c.HTTP.Do(req)
	if err != nil {
		c.fail()
		return err
	}
	defer resp.Body.Close()
	raw, err := io.ReadAll(resp.Body)
	if err != nil {
		c.fail()
		return err
	}
	var result telegramResponse
	if err := json.Unmarshal(raw, &result); err != nil {
		c.fail()
		return fmt.Errorf("decode telegram response: %w", err)
	}
	if !result.OK {
		c.fail()
		return fmt.Errorf("telegram API error: %s", result.Description)
	}
	var sent struct {
		MessageID int64 `json:"message_id"`
	}
	_ = json.Unmarshal(result.Result, &sent)
	c.Logger.Info("sent message", "chat_id", chatID, "message_id", sent.MessageID)
	return nil
}

// SetWebhook registers the webhook URL with the secret token and restricts
// updates to messages. It is only called when WEBHOOK_URL is configured.
func (c *Client) SetWebhook(ctx context.Context, url, secret string) error {
	payload := struct {
		URL            string   `json:"url"`
		SecretToken    string   `json:"secret_token"`
		AllowedUpdates []string `json:"allowed_updates"`
	}{URL: url, SecretToken: secret, AllowedUpdates: []string{"message"}}
	body, err := json.Marshal(payload)
	if err != nil {
		return err
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, c.BaseURL+"/bot"+c.Token+"/setWebhook", bytes.NewReader(body))
	if err != nil {
		return err
	}
	req.Header.Set("Content-Type", "application/json")
	resp, err := c.HTTP.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	raw, err := io.ReadAll(resp.Body)
	if err != nil {
		return err
	}
	var result telegramResponse
	if err := json.Unmarshal(raw, &result); err != nil {
		return fmt.Errorf("decode setWebhook response: %w", err)
	}
	if !result.OK {
		return fmt.Errorf("setWebhook failed: %s", result.Description)
	}
	c.Logger.Info("telegram webhook registered", "url", url)
	return nil
}

func (c *Client) fail() {
	if c.OnSendFailure != nil {
		c.OnSendFailure()
	}
}
