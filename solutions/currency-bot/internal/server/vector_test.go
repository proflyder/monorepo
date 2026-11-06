package server

import (
	"testing"

	"github.com/proflyder/monorepo/packages/go/contracttest"
	"github.com/proflyder/monorepo/solutions/currency-bot/internal/golden"
)

const messagesVectorPath = "../../../../solutions/currency-bot/testdata/vectors/messages.json"

type messagesVectors struct {
	Root           string `json:"root"`
	Start          string `json:"start"`
	Help           string `json:"help"`
	TriggerFailure string `json:"trigger_failure"`
}

// TestMessageVectors pins the exact Telegram texts and the root body. The
// /help text intentionally promises an hourly broadcast that the hourly job
// does not send; it is user-visible compatibility, not a bug to "fix" here.
func TestMessageVectors(t *testing.T) {
	golden.Run(t, *contracttest.Update, messagesVectorPath, messagesVectors{
		Root:           "Hello World!",
		Start:          startMessage,
		Help:           helpMessage,
		TriggerFailure: "❌ Не удалось обновить курсы: upstream down",
	})
}
