package app

import (
	"testing"
	"time"
)

func TestNextHourly(t *testing.T) {
	loc := time.UTC
	tests := []struct {
		now  time.Time
		want time.Time
	}{
		{time.Date(2026, 10, 31, 12, 30, 0, 0, loc), time.Date(2026, 10, 31, 13, 0, 0, 0, loc)},
		{time.Date(2026, 10, 31, 12, 0, 0, 0, loc), time.Date(2026, 10, 31, 13, 0, 0, 0, loc)},
		{time.Date(2026, 10, 31, 23, 59, 59, 0, loc), time.Date(2026, 11, 1, 0, 0, 0, 0, loc)},
	}
	for _, tc := range tests {
		if got := nextHourly(tc.now, loc, 0); !got.Equal(tc.want) {
			t.Fatalf("nextHourly(%v) = %v, want %v", tc.now, got, tc.want)
		}
	}
}

func TestNextWeekly(t *testing.T) {
	loc := time.UTC
	// Saturday 12:00 → Sunday 00:00.
	if got := nextWeekly(time.Date(2026, 10, 31, 12, 0, 0, 0, loc), loc, time.Sunday, 0); !got.Equal(time.Date(2026, 11, 1, 0, 0, 0, 0, loc)) {
		t.Fatalf("sunday = %v", got)
	}
	// Monday 00:00 → following Sunday.
	if got := nextWeekly(time.Date(2026, 11, 2, 0, 0, 0, 0, loc), loc, time.Sunday, 0); !got.Equal(time.Date(2026, 11, 8, 0, 0, 0, 0, loc)) {
		t.Fatalf("next sunday = %v", got)
	}
	// Different hour on the target day, when earlier than now.
	if got := nextWeekly(time.Date(2026, 11, 1, 5, 0, 0, 0, loc), loc, time.Sunday, 3); !got.Equal(time.Date(2026, 11, 8, 3, 0, 0, 0, loc)) {
		t.Fatalf("same-day earlier hour = %v", got)
	}
}

func TestNextHourlyHonoursTimezone(t *testing.T) {
	loc, err := time.LoadLocation("Asia/Almaty")
	if err != nil {
		t.Fatal(err)
	}
	now := time.Date(2026, 10, 31, 10, 30, 0, 0, loc)
	got := nextHourly(now, loc, 0)
	if got.Hour() != 11 || got.Location() != loc {
		t.Fatalf("got %v", got)
	}
}
