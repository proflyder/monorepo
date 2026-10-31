// Package model holds the currency-bot domain types shared by the parser,
// alerting, formatting, storage and HTTP layers.
package model

import "time"

// ExchangeRate is one currency pair as users see it on kurs.kz: Buy is the
// rate an exchange office pays when buying currency from you, Sell is the rate
// it charges when selling currency to you.
type ExchangeRate struct {
	Buy  float64
	Sell float64
}

// Rates is the aggregated USD/KZT and RUB/KZT rate snapshot.
type Rates struct {
	USDToKZT ExchangeRate
	RUBToKZT ExchangeRate
}

// Record is one persisted rate snapshot.
type Record struct {
	Timestamp time.Time
	Rates     Rates
}

// Pair identifies the currency pair an alert belongs to.
type Pair string

// Currency pairs evaluated by the bot.
const (
	USDToKZT Pair = "USD_TO_KZT"
	RUBToKZT Pair = "RUB_TO_KZT"
)

// DisplayName is the human-readable pair label used in messages.
func (p Pair) DisplayName() string {
	switch p {
	case RUBToKZT:
		return "RUB → KZT"
	default:
		return "USD → KZT"
	}
}

// Emoji is the flag shown next to the pair.
func (p Pair) Emoji() string {
	switch p {
	case RUBToKZT:
		return "🇷🇺"
	default:
		return "🇺🇸"
	}
}

// RateType distinguishes the buy and sell side of a pair.
type RateType string

// Rate side constants.
const (
	Buy  RateType = "BUY"
	Sell RateType = "SELL"
)

// DisplayName is the Russian label used in messages.
func (r RateType) DisplayName() string {
	if r == Buy {
		return "покупка"
	}
	return "продажа"
}

// Level is the alert severity.
type Level string

// Alert severities.
const (
	Warning  Level = "WARNING"
	Critical Level = "CRITICAL"
)

// Direction is the sign of the observed change.
type Direction string

// Change directions.
const (
	Up   Direction = "UP"
	Down Direction = "DOWN"
)

// Period describes one threshold lookback window. Duration is fixed (there is
// no calendar-month handling; MONTH is 720h = 30 days, exactly as in Kotlin).
type Period struct {
	Name     string
	Duration time.Duration
	Display  string
	Warning  float64
	Critical float64
	ReAlert  float64
}

// Periods is the evaluation order used by the alert checker:
// HOUR, DAY, WEEK, MONTH.
var Periods = []Period{
	{Name: "HOUR", Duration: time.Hour, Display: "час", Warning: 0.5, Critical: 1.0, ReAlert: 0.25},
	{Name: "DAY", Duration: 24 * time.Hour, Display: "сутки", Warning: 1.0, Critical: 2.0, ReAlert: 0.50},
	{Name: "WEEK", Duration: 168 * time.Hour, Display: "неделю", Warning: 2.0, Critical: 4.0, ReAlert: 1.00},
	{Name: "MONTH", Duration: 720 * time.Hour, Display: "месяц", Warning: 3.0, Critical: 5.0, ReAlert: 1.50},
}

// AlertKey uniquely identifies a deduplicated alert slot.
type AlertKey struct {
	Pair     Pair
	Period   string
	RateType RateType
}

// Alert is one threshold crossing that may be sent to Telegram.
type Alert struct {
	Level         Level
	Period        Period
	Pair          Pair
	RateType      RateType
	Direction     Direction
	ChangePercent float64
	OldRate       float64
	NewRate       float64
}

// Key returns the dedup key of the alert.
func (a Alert) Key() AlertKey {
	return AlertKey{Pair: a.Pair, Period: a.Period.Name, RateType: a.RateType}
}

// SentAlert is the last delivered alert for one AlertKey.
type SentAlert struct {
	Key           AlertKey
	Level         Level
	Direction     Direction
	RateAtAlert   float64
	ChangePercent float64
	SentAt        time.Time
}
