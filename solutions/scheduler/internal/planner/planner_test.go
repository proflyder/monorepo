package planner

import (
	"errors"
	"testing"
	"time"

	ricardov1 "ricardo.local/monorepo/packages/proto/ricardo/v1"
)

func daily(tz string, h, m int) Rule {
	return Rule{Timezone: tz, Hour: h, Minute: m, Repeat: ricardov1.RepeatKind_REPEAT_KIND_DAILY}
}

func mustLoad(t *testing.T, tz string) *time.Location {
	t.Helper()
	loc, err := time.LoadLocation(tz)
	if err != nil {
		t.Fatalf("LoadLocation(%q): %v", tz, err)
	}
	return loc
}

func mustNext(t *testing.T, r Rule, after time.Time) time.Time {
	t.Helper()
	next, ok, err := r.Next(after)
	if err != nil {
		t.Fatalf("Next: %v", err)
	}
	if !ok {
		t.Fatalf("Next returned no occurrence")
	}
	return next
}

func assertLocal(t *testing.T, got time.Time, tz string, y int, mo time.Month, d, h, mi, s int) {
	t.Helper()
	ly, lm, ld := got.In(mustLoad(t, tz)).Date()
	lh, lmi, ls := got.In(mustLoad(t, tz)).Clock()
	if ly != y || lm != mo || ld != d || lh != h || lmi != mi || ls != s {
		t.Fatalf("local time = %04d-%02d-%02d %02d:%02d:%02d, want %04d-%02d-%02d %02d:%02d:%02d",
			ly, lm, ld, lh, lmi, ls, y, mo, d, h, mi, s)
	}
}

func TestNextDailyNormal(t *testing.T) {
	loc := mustLoad(t, "Europe/Berlin")
	after := time.Date(2026, 3, 27, 12, 0, 0, 0, loc)
	next := mustNext(t, daily("Europe/Berlin", 7, 30), after)
	assertLocal(t, next, "Europe/Berlin", 2026, 3, 28, 7, 30, 0)
	if want := time.Date(2026, 3, 28, 6, 30, 0, 0, time.UTC); !next.Equal(want) {
		t.Fatalf("instant = %v, want %v", next.UTC(), want)
	}
}

func TestNextSkipsNonexistentWallTime(t *testing.T) {
	// Europe/Berlin 2026-03-29 02:00 -> 03:00. The 02:30 civil time does not
	// exist, so that date is skipped entirely.
	loc := mustLoad(t, "Europe/Berlin")
	after := time.Date(2026, 3, 28, 12, 0, 0, 0, loc)
	next := mustNext(t, daily("Europe/Berlin", 2, 30), after)
	assertLocal(t, next, "Europe/Berlin", 2026, 3, 30, 2, 30, 0)
	if want := time.Date(2026, 3, 30, 0, 30, 0, 0, time.UTC); !next.Equal(want) {
		t.Fatalf("instant = %v, want %v", next.UTC(), want)
	}
}

func TestNextAmbiguousUsesEarliestInstant(t *testing.T) {
	// Europe/Berlin 2026-10-25 falls back 03:00 -> 02:00, so 02:30 happens
	// twice: 00:30 UTC (CEST) and 01:30 UTC (CET). Earliest wins.
	loc := mustLoad(t, "Europe/Berlin")
	after := time.Date(2026, 10, 24, 12, 0, 0, 0, loc)
	next := mustNext(t, daily("Europe/Berlin", 2, 30), after)
	assertLocal(t, next, "Europe/Berlin", 2026, 10, 25, 2, 30, 0)
	if want := time.Date(2026, 10, 25, 0, 30, 0, 0, time.UTC); !next.Equal(want) {
		t.Fatalf("instant = %v, want earliest %v", next.UTC(), want)
	}
}

func TestNextNewYorkSpringGap(t *testing.T) {
	// America/New_York 2026-03-08 02:00 -> 03:00.
	loc := mustLoad(t, "America/New_York")
	after := time.Date(2026, 3, 7, 12, 0, 0, 0, loc)
	next := mustNext(t, daily("America/New_York", 2, 30), after)
	assertLocal(t, next, "America/New_York", 2026, 3, 9, 2, 30, 0)
	if want := time.Date(2026, 3, 9, 6, 30, 0, 0, time.UTC); !next.Equal(want) {
		t.Fatalf("instant = %v, want %v", next.UTC(), want)
	}
}

func TestNextWeekdaysWeekendsCustom(t *testing.T) {
	berlin := "Europe/Berlin"
	fridayNoon := time.Date(2026, 10, 30, 12, 0, 0, 0, mustLoad(t, berlin))

	weekday := Rule{Timezone: berlin, Hour: 8, Repeat: ricardov1.RepeatKind_REPEAT_KIND_WEEKDAYS}
	assertLocal(t, mustNext(t, weekday, fridayNoon), berlin, 2026, 11, 2, 8, 0, 0)

	weekend := Rule{Timezone: berlin, Hour: 8, Repeat: ricardov1.RepeatKind_REPEAT_KIND_WEEKENDS}
	assertLocal(t, mustNext(t, weekend, fridayNoon), berlin, 2026, 10, 31, 8, 0, 0)

	custom := Rule{
		Timezone: berlin, Hour: 8,
		Repeat:   ricardov1.RepeatKind_REPEAT_KIND_CUSTOM,
		Weekdays: []int32{1, 4},
	}
	mondayNoon := time.Date(2026, 11, 2, 12, 0, 0, 0, mustLoad(t, berlin))
	assertLocal(t, mustNext(t, custom, mondayNoon), berlin, 2026, 11, 5, 8, 0, 0)
}

func TestNextSkipsSkippedCivilDate(t *testing.T) {
	// Samoa skipped 2011-12-30 entirely (UTC-10 -> UTC+14). The daily 08:00
	// occurrence on that date must be skipped, not stalled.
	loc := mustLoad(t, "Pacific/Apia")
	after := time.Date(2011, 12, 29, 12, 0, 0, 0, loc)
	next := mustNext(t, daily("Pacific/Apia", 8, 0), after)
	assertLocal(t, next, "Pacific/Apia", 2011, 12, 31, 8, 0, 0)
	if want := time.Date(2011, 12, 30, 18, 0, 0, 0, time.UTC); !next.Equal(want) {
		t.Fatalf("instant = %v, want %v", next.UTC(), want)
	}
}

func TestNextStrictlyAfter(t *testing.T) {
	loc := mustLoad(t, "Europe/Berlin")
	occurrence := time.Date(2026, 5, 1, 7, 30, 0, 0, loc)
	next := mustNext(t, daily("Europe/Berlin", 7, 30), occurrence)
	assertLocal(t, next, "Europe/Berlin", 2026, 5, 2, 7, 30, 0)
}

func TestNextSecondGranularity(t *testing.T) {
	loc := mustLoad(t, "Europe/Berlin")
	r := daily("Europe/Berlin", 7, 30)
	r.Second = 45
	next := mustNext(t, r, time.Date(2026, 5, 1, 12, 0, 0, 0, loc))
	assertLocal(t, next, "Europe/Berlin", 2026, 5, 2, 7, 30, 45)
}

func TestOnce(t *testing.T) {
	berlin := "Europe/Berlin"
	r := Rule{
		Timezone: berlin, Hour: 23, Minute: 59,
		Repeat:   ricardov1.RepeatKind_REPEAT_KIND_ONCE,
		OnceDate: "2026-12-31",
	}
	next := mustNext(t, r, time.Date(2026, 12, 1, 0, 0, 0, 0, mustLoad(t, berlin)))
	assertLocal(t, next, berlin, 2026, 12, 31, 23, 59, 0)

	if _, ok, err := r.Next(time.Date(2027, 1, 1, 0, 0, 0, 0, mustLoad(t, berlin))); err != nil || ok {
		t.Fatalf("past once = (%v, %v), want (false, nil)", ok, err)
	}

	leap := Rule{
		Timezone: berlin, Hour: 8,
		Repeat:   ricardov1.RepeatKind_REPEAT_KIND_ONCE,
		OnceDate: "2028-02-29",
	}
	next = mustNext(t, leap, time.Date(2028, 1, 1, 0, 0, 0, 0, mustLoad(t, berlin)))
	assertLocal(t, next, berlin, 2028, 2, 29, 8, 0, 0)

	gap := Rule{
		Timezone: berlin, Hour: 2, Minute: 30,
		Repeat:   ricardov1.RepeatKind_REPEAT_KIND_ONCE,
		OnceDate: "2026-03-29",
	}
	if _, ok, err := gap.Next(time.Date(2026, 3, 1, 0, 0, 0, 0, mustLoad(t, berlin))); err != nil || ok {
		t.Fatalf("gap once = (%v, %v), want (false, nil)", ok, err)
	}
}

func TestValidate(t *testing.T) {
	base := daily("Europe/Berlin", 7, 30)
	tests := []struct {
		name string
		rule Rule
	}{
		{"empty timezone", Rule{Hour: 7, Repeat: ricardov1.RepeatKind_REPEAT_KIND_DAILY}},
		{"unknown timezone", Rule{Timezone: "Mars/Olympus", Repeat: ricardov1.RepeatKind_REPEAT_KIND_DAILY}},
		{"hour 24", Rule{Timezone: "UTC", Hour: 24, Repeat: ricardov1.RepeatKind_REPEAT_KIND_DAILY}},
		{"minute 60", Rule{Timezone: "UTC", Minute: 60, Repeat: ricardov1.RepeatKind_REPEAT_KIND_DAILY}},
		{"second 60", Rule{Timezone: "UTC", Second: 60, Repeat: ricardov1.RepeatKind_REPEAT_KIND_DAILY}},
		{"unspecified repeat", Rule{Timezone: "UTC"}},
		{"custom without weekdays", Rule{Timezone: "UTC", Repeat: ricardov1.RepeatKind_REPEAT_KIND_CUSTOM}},
		{"custom bad weekday", Rule{Timezone: "UTC", Repeat: ricardov1.RepeatKind_REPEAT_KIND_CUSTOM, Weekdays: []int32{0}}},
		{"custom duplicate weekday", Rule{Timezone: "UTC", Repeat: ricardov1.RepeatKind_REPEAT_KIND_CUSTOM, Weekdays: []int32{1, 1}}},
		{"once without date", Rule{Timezone: "UTC", Repeat: ricardov1.RepeatKind_REPEAT_KIND_ONCE}},
		{"once bad date", Rule{Timezone: "UTC", Repeat: ricardov1.RepeatKind_REPEAT_KIND_ONCE, OnceDate: "31-12-2026"}},
		{"daily with date", Rule{Timezone: "UTC", Repeat: ricardov1.RepeatKind_REPEAT_KIND_DAILY, OnceDate: "2026-12-31"}},
		{"daily with weekdays", Rule{Timezone: "UTC", Repeat: ricardov1.RepeatKind_REPEAT_KIND_DAILY, Weekdays: []int32{1}}},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			if err := tc.rule.Validate(); !errors.Is(err, ErrInvalidRule) {
				t.Fatalf("Validate = %v, want ErrInvalidRule", err)
			}
			if _, _, err := tc.rule.Next(time.Now()); err == nil {
				t.Fatalf("Next accepted invalid rule")
			}
		})
	}
	if err := base.Validate(); err != nil {
		t.Fatalf("valid rule rejected: %v", err)
	}
}
