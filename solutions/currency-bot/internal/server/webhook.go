package server

import (
	"context"
	"crypto/subtle"
	"encoding/json"
	"io"
	"net/http"
	"strings"
)

const (
	secretHeader = "X-Telegram-Bot-Api-Secret-Token"
	maxWebhook   = 1 << 20
)

const startMessage = `👋 Привет! Я бот для отслеживания курсов валют.

Доступные команды:
/trigger - Обновить курсы валют
/help - Показать справку`

const helpMessage = `📖 Справка по командам бота:

/trigger - Принудительно обновить и получить актуальные курсы USD→KZT и RUB→KZT
/start - Показать приветственное сообщение
/help - Показать эту справку

ℹ️ Бот автоматически отправляет курсы валют каждый час.`

type telegramMessage struct {
	Text *string `json:"text"`
	Chat struct {
		ID int64 `json:"id"`
	} `json:"chat"`
}

type webhookUpdate struct {
	UpdateID          int64            `json:"update_id"`
	Message           *telegramMessage `json:"message"`
	EditedMessage     *telegramMessage `json:"edited_message"`
	ChannelPost       *telegramMessage `json:"channel_post"`
	EditedChannelPost *telegramMessage `json:"edited_channel_post"`
}

func (u webhookUpdate) message() *telegramMessage {
	switch {
	case u.Message != nil:
		return u.Message
	case u.EditedMessage != nil:
		return u.EditedMessage
	case u.ChannelPost != nil:
		return u.ChannelPost
	default:
		return u.EditedChannelPost
	}
}

// handleWebhook validates the Telegram secret token, enforces the chat
// allowlist and dispatches /start, /help and /trigger by prefix match.
func (s *Server) handleWebhook(w http.ResponseWriter, r *http.Request) {
	got := r.Header.Get(secretHeader)
	if got == "" || subtle.ConstantTimeCompare([]byte(got), []byte(s.opts.WebhookSecret)) != 1 {
		s.opts.Logger.Warn("rejected webhook with invalid secret", "remote", r.RemoteAddr)
		http.Error(w, "unauthorized", http.StatusUnauthorized)
		return
	}

	// A valid secret is enough to answer 200. Telegram retries every non-2xx
	// response, which would re-run /trigger, so failures are logged only.
	body, err := io.ReadAll(http.MaxBytesReader(w, r.Body, maxWebhook))
	if err != nil {
		s.opts.Logger.Error("telegram webhook failed", "error", err, "step", "read webhook body")
		writeJSON(w, http.StatusOK, "OK")
		return
	}
	var update webhookUpdate
	if err := json.Unmarshal(body, &update); err != nil {
		s.opts.Logger.Error("telegram webhook failed", "error", err, "step", "decode webhook update")
		writeJSON(w, http.StatusOK, "OK")
		return
	}

	msg := update.message()
	if msg == nil {
		writeJSON(w, http.StatusOK, "OK")
		return
	}
	if !s.isAllowedChat(msg.Chat.ID) {
		s.opts.Logger.Info("ignoring message from unknown chat", "chat_id", msg.Chat.ID)
		writeJSON(w, http.StatusOK, "OK")
		return
	}

	text := ""
	if msg.Text != nil {
		text = strings.TrimSpace(*msg.Text)
	}
	if text == "" {
		writeJSON(w, http.StatusOK, "OK")
		return
	}

	chatID := itoa64(msg.Chat.ID)
	var handleErr error
	switch {
	case strings.HasPrefix(text, "/trigger"):
		handleErr = s.handleTriggerCommand(r.Context(), chatID)
	case strings.HasPrefix(text, "/start"):
		handleErr = s.opts.Sender.Send(r.Context(), chatID, startMessage)
	case strings.HasPrefix(text, "/help"):
		handleErr = s.opts.Sender.Send(r.Context(), chatID, helpMessage)
	default:
		s.opts.Logger.Debug("ignoring non-command message", "chat_id", chatID)
	}
	if handleErr != nil {
		s.opts.Logger.Error("telegram webhook failed", "error", handleErr, "step", "handle telegram command")
	}
	writeJSON(w, http.StatusOK, "OK")
}

func (s *Server) handleTriggerCommand(ctx context.Context, chatID string) error {
	if err := s.opts.Pipeline.Run(ctx, "trigger", chatID, true, false); err != nil {
		s.opts.Logger.Error("failed to trigger currency update", "chat_id", chatID, "error", err)
		// The original interpolates the unescaped error into HTML. Keep it.
		return s.opts.Sender.Send(ctx, chatID, "❌ Не удалось обновить курсы: "+err.Error())
	}
	return nil
}

