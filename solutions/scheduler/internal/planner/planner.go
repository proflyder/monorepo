// Package planner computes the next occurrence of a local wall-clock rule in
// an IANA timezone. It is pure: no database, network or ambient clock.
//
// Deterministic DST policy:
//   - A non-existent wall time (spring-forward gap) is skipped: that civil
//     date produces no occurrence, and the following matching date is used.
//   - An ambiguous wall time (fall-back overlap) resolves to the earliest of
//     the two UTC instants.
//   - time.Date normalization is never trusted for decisions; candidate
//     instants are validated by reading their components back in the zone.
package planner

import (
	"errors"
	"fmt"
	"time"

	ricardov1 "ricardo.local/monorepo/packages/proto/ricardo/v1"
)

// MaxLookaheadDays bounds recurrence search. Every supported repeat kind has a
// matching date within 7 days, so this is a safety net, not a normal path.
const MaxLookaheadDays = 400

// ErrInvalidRule marks rule validation failures.
var ErrInvalidRule = errors.New("invalid schedule rule")

// Rule is a validated local wall-clock recurrence.
type Rule struct {
	Timezone string
	Hour     int
	Minute   int
	Second   int
	Repeat   ricardov1.RepeatKind
	Weekdays []int32 // ISO 1=Mon..7=Sun, REPEAT_KIND_CUSTOM only
	OnceDate string  // YYYY-MM-DD, REPEAT_KIND_ONCE only
}

// Validate reports whether the rule is well formed.
func (r Rule) Validate() error {
	if r.Timezone == "" {
		return fmt.Errorf("%w: timezone is required", ErrInvalidRule)
	}
	if _, err := time.LoadLocation(r.Timezone); err != nil {
		return fmt.Errorf("%w: unknown timezone %q", ErrInvalidRule, r.Timezone)
	}
	if r.Hour < 0 || r.Hour > 23 {
		return fmt.Errorf("%w: hour %d outside 0..23", ErrInvalidRule, r.Hour)
	}
	if r.Minute < 0 || r.Minute > 59 {
		return fmt.Errorf("%w: minute %d outside 0..59", ErrInvalidRule, r.Minute)
	}
	if r.Second < 0 || r.Second > 59 {
		return fmt.Errorf("%w: second %d outside 0..59", ErrInvalidRule, r.Second)
	}
	switch r.Repeat {
	case ricardov1.RepeatKind_REPEAT_KIND_ONCE:
		if r.OnceDate == "" {
			return fmt.Errorf("%w: once_date is required for REPEAT_KIND_ONCE", ErrInvalidRule)
		}
		if _, err := time.Parse("2006-01-02", r.OnceDate); err != nil {
			return fmt.Errorf("%w: once_date must be YYYY-MM-DD", ErrInvalidRule)
		}
		if len(r.Weekdays) != 0 {
			return fmt.Errorf("%w: weekdays are only valid for REPEAT_KIND_CUSTOM", ErrInvalidRule)
		}
	case ricardov1.RepeatKind_REPEAT_KIND_DAILY,
		ricardov1.RepeatKind_REPEAT_KIND_WEEKDAYS,
		ricardov1.RepeatKind_REPEAT_KIND_WEEKENDS:
		if r.OnceDate != "" {
			return fmt.Errorf("%w: once_date is only valid for REPEAT_KIND_ONCE", ErrInvalidRule)
		}
		if len(r.Weekdays) != 0 {
			return fmt.Errorf("%w: weekdays are only valid for REPEAT_KIND_CUSTOM", ErrInvalidRule)
		}
	case ricardov1.RepeatKind_REPEAT_KIND_CUSTOM:
		if r.OnceDate != "" {
			return fmt.Errorf("%w: once_date is only valid for REPEAT_KIND_ONCE", ErrInvalidRule)
		}
		if len(r.Weekdays) == 0 {
			return fmt.Errorf("%w: REPEAT_KIND_CUSTOM needs weekdays", ErrInvalidRule)
		}
		seen := make(map[int32]bool, len(r.Weekdays))
		for _, wd := range r.Weekdays {
			if wd < 1 || wd > 7 {
				return fmt.Errorf("%w: weekday %d outside 1..7", ErrInvalidRule, wd)
			}
			if seen[wd] {
				return fmt.Errorf("%w: duplicate weekday %d", ErrInvalidRule, wd)
			}
			seen[wd] = true
		}
	default:
		return fmt.Errorf("%w: unsupported repeat %v", ErrInvalidRule, r.Repeat)
	}
	return nil
}

// Next returns the first occurrence strictly after `after`. The second result
// is false when the rule is exhausted (a one-shot date in the past or a
// one-shot civil time that falls in a DST gap).
func (r Rule) Next(after time.Time) (time.Time, bool, error) {
	if err := r.Validate(); err != nil {
		return time.Time{}, false, err
	}
	loc, err := time.LoadLocation(r.Timezone)
	if err != nil {
		// Validate already checked this; keep the branch defensive.
		return time.Time{}, false, fmt.Errorf("%w: %v", ErrInvalidRule, err)
	}
	after = after.UTC()

	if r.Repeat == ricardov1.RepeatKind_REPEAT_KIND_ONCE {
		y, mo, d, err := parseDate(r.OnceDate)
		if err != nil {
			return time.Time{}, false, fmt.Errorf("%w: %v", ErrInvalidRule, err)
		}
		occ, ok := resolveCivil(loc, y, mo, d, r.Hour, r.Minute, r.Second)
		if !ok || !occ.After(after) {
			return time.Time{}, false, nil
		}
		return occ, true, nil
	}

	y, mo, d := after.In(loc).Date()
	// Iterate the civil calendar in UTC. A location-aware cursor can normalize
	// across a skipped civil date (for example Pacific/Apia 2011-12-30) and
	// stall on the same date; UTC date arithmetic is pure calendar math.
	cursor := time.Date(y, mo, d, 12, 0, 0, 0, time.UTC)
	for i := 0; i < MaxLookaheadDays; i++ {
		cy, cm, cd := cursor.Date()
		if r.matchesDate(cy, cm, cd) {
			if occ, ok := resolveCivil(loc, cy, cm, cd, r.Hour, r.Minute, r.Second); ok && occ.After(after) {
				return occ, true, nil
			}
		}
		cursor = cursor.AddDate(0, 0, 1)
	}
	return time.Time{}, false, fmt.Errorf("%w: no occurrence within %d days", ErrInvalidRule, MaxLookaheadDays)
}

func (r Rule) matchesDate(y int, mo time.Month, d int) bool {
	wd := isoWeekday(time.Date(y, mo, d, 12, 0, 0, 0, time.UTC).Weekday())
	switch r.Repeat {
	case ricardov1.RepeatKind_REPEAT_KIND_DAILY:
		return true
	case ricardov1.RepeatKind_REPEAT_KIND_WEEKDAYS:
		return wd >= 1 && wd <= 5
	case ricardov1.RepeatKind_REPEAT_KIND_WEEKENDS:
		return wd == 6 || wd == 7
	case ricardov1.RepeatKind_REPEAT_KIND_CUSTOM:
		for _, w := range r.Weekdays {
			if int(w) == wd {
				return true
			}
		}
		return false
	default:
		return false
	}
}

func isoWeekday(d time.Weekday) int {
	if d == time.Sunday {
		return 7
	}
	return int(d)
}

func parseDate(s string) (int, time.Month, int, error) {
	t, err := time.Parse("2006-01-02", s)
	if err != nil {
		return 0, 0, 0, fmt.Errorf("once_date must be YYYY-MM-DD: %w", err)
	}
	y, mo, d := t.Date()
	return y, mo, d, nil
}

// resolveCivil maps a civil date/time in loc to a UTC instant, or reports that
// the wall time does not exist (DST gap). For ambiguous times the earliest
// instant wins.
func resolveCivil(loc *time.Location, y int, mo time.Month, d, h, mi, s int) (time.Time, bool) {
	naive := time.Date(y, mo, d, h, mi, s, 0, time.UTC)
	base := naive.Unix()

	var best time.Time
	found := false
	for _, off := range zoneOffsetsNear(loc, base) {
		cand := time.Unix(base-int64(off), 0).UTC()
		ly, lm, ld := cand.In(loc).Date()
		lh, lmi, ls := cand.In(loc).Clock()
		if ly != y || lm != mo || ld != d || lh != h || lmi != mi || ls != s {
			continue
		}
		if !found || cand.Before(best) {
			best = cand
			found = true
		}
	}
	return best, found
}

// zoneOffsetsNear samples the zone offset around a civil timestamp. Sampling
// every six hours across +/-48h reliably captures both sides of any transition
// relevant to the requested wall time.
func zoneOffsetsNear(loc *time.Location, base int64) []int {
	seen := make(map[int]struct{}, 4)
	offsets := make([]int, 0, 4)
	add := func(unix int64) {
		_, off := time.Unix(unix, 0).In(loc).Zone()
		if _, ok := seen[off]; ok {
			return
		}
		seen[off] = struct{}{}
		offsets = append(offsets, off)
	}
	const step = int64(6 * 60 * 60)
	for delta := -int64(48 * 60 * 60); delta <= int64(48*60*60); delta += step {
		add(base + delta)
	}
	return offsets
}
