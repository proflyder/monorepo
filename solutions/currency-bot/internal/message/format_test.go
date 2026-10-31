package message

import (
	"strings"
	"testing"

	"ricardo.local/monorepo/solutions/currency-bot/internal/model"
)

var sampleRates = model.Rates{
	USDToKZT: model.ExchangeRate{Buy: 485.50, Sell: 487.20},
	RUBToKZT: model.ExchangeRate{Buy: 4.85, Sell: 4.92},
}

func period(name string) model.Period {
	for _, p := range model.Periods {
		if p.Name == name {
			return p
		}
	}
	panic("unknown period " + name)
}

func alert(level model.Level, pair model.Pair, rateType model.RateType, dir model.Direction, change float64, periodName string, oldRate, newRate float64) model.Alert {
	return model.Alert{
		Level: level, Period: period(periodName), Pair: pair, RateType: rateType,
		Direction: dir, ChangePercent: change, OldRate: oldRate, NewRate: newRate,
	}
}

func TestFormatNoAlerts(t *testing.T) {
	got := Format(sampleRates, nil)
	for _, want := range []string{
		"💱 <b>Курсы валют на kurs.kz</b>",
		"🇺🇸 <b>USD → KZT</b>",
		"🇷🇺 <b>RUB → KZT</b>",
		"Покупка:",
		"Продажа:",
		"487.20",
		"485.50",
		"4.92",
		"4.85",
		"₸",
	} {
		if !strings.Contains(got, want) {
			t.Fatalf("message missing %q:\n%s", want, got)
		}
	}
	for _, forbidden := range []string{"ПРЕДУПРЕЖДЕНИЯ", "КРИТИЧЕСКИЕ ИЗМЕНЕНИЯ"} {
		if strings.Contains(got, forbidden) {
			t.Fatalf("unexpected alert section %q", forbidden)
		}
	}
}

func TestFormatWarningUpSell(t *testing.T) {
	got := Format(sampleRates, []model.Alert{
		alert(model.Warning, model.USDToKZT, model.Sell, model.Up, 0.80, "HOUR", 480, 483.84),
	})
	for _, want := range []string{
		"⚠️ <b>ПРЕДУПРЕЖДЕНИЯ</b>",
		"📈", "🇺🇸", "USD → KZT", "(продажа)", "вырос", "0.80", "час", "480.00", "483.84",
	} {
		if !strings.Contains(got, want) {
			t.Fatalf("message missing %q:\n%s", want, got)
		}
	}
}

func TestFormatBuyLabel(t *testing.T) {
	got := Format(sampleRates, []model.Alert{
		alert(model.Warning, model.USDToKZT, model.Buy, model.Up, 1.2, "DAY", 480, 485.76),
	})
	if !strings.Contains(got, "(покупка)") {
		t.Fatalf("missing buy label:\n%s", got)
	}
}

func TestFormatDownWeekRub(t *testing.T) {
	got := Format(sampleRates, []model.Alert{
		alert(model.Warning, model.RUBToKZT, model.Sell, model.Down, -2.5, "WEEK", 5.0, 4.875),
	})
	for _, want := range []string{"📉", "🇷🇺", "RUB → KZT", "(продажа)", "упал", "2.50", "неделю"} {
		if !strings.Contains(got, want) {
			t.Fatalf("message missing %q:\n%s", want, got)
		}
	}
}

func TestFormatTwoWarnings(t *testing.T) {
	got := Format(sampleRates, []model.Alert{
		alert(model.Warning, model.USDToKZT, model.Sell, model.Up, 0.8, "HOUR", 480, 483.84),
		alert(model.Warning, model.RUBToKZT, model.Buy, model.Down, 1.1, "DAY", 5.0, 4.945),
	})
	section := between(got, "⚠️ <b>ПРЕДУПРЕЖДЕНИЯ</b>", "🚨")
	if strings.Count(section, "📈")+strings.Count(section, "📉") != 2 {
		t.Fatalf("expected exactly two alert markers in the warning block:\n%s", got)
	}
}

func TestFormatCritical(t *testing.T) {
	got := Format(sampleRates, []model.Alert{
		alert(model.Critical, model.USDToKZT, model.Sell, model.Down, -2.5, "DAY", 500, 487.5),
	})
	for _, want := range []string{"🚨 <b>КРИТИЧЕСКИЕ ИЗМЕНЕНИЯ</b>", "(продажа)", "упал", "2.50", "сутки"} {
		if !strings.Contains(got, want) {
			t.Fatalf("message missing %q:\n%s", want, got)
		}
	}
	if strings.Contains(got, "⚠️ <b>ПРЕДУПРЕЖДЕНИЯ</b>") {
		t.Fatal("empty warning section should not be rendered")
	}
}

func TestFormatWarningsBeforeCritical(t *testing.T) {
	got := Format(sampleRates, []model.Alert{
		alert(model.Critical, model.USDToKZT, model.Sell, model.Down, 2.5, "DAY", 500, 487.5),
		alert(model.Warning, model.USDToKZT, model.Sell, model.Up, 0.8, "HOUR", 480, 483.84),
	})
	w := strings.Index(got, "ПРЕДУПРЕЖДЕНИЯ")
	c := strings.Index(got, "КРИТИЧЕСКИЕ ИЗМЕНЕНИЯ")
	if w < 0 || c < 0 || w > c {
		t.Fatalf("expected warning section before critical:\n%s", got)
	}
}

func TestFormatAllPeriodWords(t *testing.T) {
	got := Format(sampleRates, []model.Alert{
		alert(model.Warning, model.USDToKZT, model.Sell, model.Up, 0.8, "HOUR", 480, 483.84),
		alert(model.Warning, model.USDToKZT, model.Sell, model.Up, 1.2, "DAY", 480, 485.76),
		alert(model.Warning, model.USDToKZT, model.Sell, model.Up, 2.1, "WEEK", 480, 490.08),
		alert(model.Warning, model.USDToKZT, model.Sell, model.Up, 3.1, "MONTH", 480, 494.88),
	})
	for _, want := range []string{"час", "сутки", "неделю", "месяц"} {
		if !strings.Contains(got, want) {
			t.Fatalf("missing period word %q", want)
		}
	}
}

func between(s, start, end string) string {
	i := strings.Index(s, start)
	if i < 0 {
		return ""
	}
	rest := s[i+len(start):]
	j := strings.Index(rest, end)
	if j < 0 {
		return rest
	}
	return rest[:j]
}
