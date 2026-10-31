// Package message renders the Telegram HTML message. The buy/sell labels are
// intentionally inverted relative to the model, exactly as users see them today:
// "Покупка" prints Sell and "Продажа" prints Buy.
package message

import (
	"fmt"
	"math"
	"strings"

	"ricardo.local/monorepo/solutions/currency-bot/internal/model"
)

// Format builds the rates message, optionally with alert sections (warnings
// first, then criticals). The result always ends with a newline.
func Format(rates model.Rates, alerts []model.Alert) string {
	var b strings.Builder
	b.WriteString("💱 <b>Курсы валют на kurs.kz</b>\n")
	b.WriteString("\n")
	b.WriteString("🇺🇸 <b>USD → KZT</b>\n")
	fmt.Fprintf(&b, "💵 Покупка: <code>%.2f</code> ₸\n", rates.USDToKZT.Sell)
	fmt.Fprintf(&b, "💸 Продажа: <code>%.2f</code> ₸\n", rates.USDToKZT.Buy)
	b.WriteString("\n")
	b.WriteString("🇷🇺 <b>RUB → KZT</b>\n")
	fmt.Fprintf(&b, "💵 Покупка: <code>%.2f</code> ₸\n", rates.RUBToKZT.Sell)
	fmt.Fprintf(&b, "💸 Продажа: <code>%.2f</code> ₸\n", rates.RUBToKZT.Buy)

	if len(alerts) == 0 {
		return b.String()
	}

	warnings := make([]model.Alert, 0, len(alerts))
	critical := make([]model.Alert, 0, len(alerts))
	for _, alert := range alerts {
		if alert.Level == model.Warning {
			warnings = append(warnings, alert)
		} else {
			critical = append(critical, alert)
		}
	}

	if len(warnings) > 0 {
		b.WriteString("\n")
		b.WriteString("━━━━━━━━\n")
		b.WriteString("⚠️ <b>ПРЕДУПРЕЖДЕНИЯ</b>\n")
		b.WriteString("\n")
		for _, alert := range warnings {
			b.WriteString(formatAlert(alert))
		}
	}
	if len(critical) > 0 {
		b.WriteString("\n")
		b.WriteString("━━━━━━━━\n")
		b.WriteString("🚨 <b>КРИТИЧЕСКИЕ ИЗМЕНЕНИЯ</b>\n")
		b.WriteString("\n")
		for _, alert := range critical {
			b.WriteString(formatAlert(alert))
		}
	}
	return b.String()
}

func formatAlert(alert model.Alert) string {
	directionEmoji := "📉"
	verb := "упал"
	if alert.Direction == model.Up {
		directionEmoji = "📈"
		verb = "вырос"
	}
	var b strings.Builder
	fmt.Fprintf(&b, "%s %s <b>%s</b> (%s) %s на <code>%.2f%%</code> за %s\n",
		directionEmoji, alert.Pair.Emoji(), alert.Pair.DisplayName(), alert.RateType.DisplayName(),
		verb, math.Abs(alert.ChangePercent), alert.Period.Display)
	fmt.Fprintf(&b, "   <code>%.2f</code> ₸ → <code>%.2f</code> ₸\n", alert.OldRate, alert.NewRate)
	b.WriteString("\n")
	return b.String()
}
