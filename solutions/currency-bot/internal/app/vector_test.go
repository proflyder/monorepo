package app

import (
	"flag"
	"testing"
	"time"

	"github.com/proflyder/monorepo/solutions/currency-bot/internal/golden"
)

const scheduleVectorPath = "../../../../solutions/currency-bot/testdata/vectors/schedule.json"

var updateGolden = flag.Bool("update", false, "rewrite golden vectors")

type hourlyCase struct {
	Name   string `json:"name"`
	Now    string `json:"now"`
	TZ     string `json:"tz"`
	Minute int    `json:"minute"`
	Next   string `json:"next"`
}

type weeklyCase struct {
	Name    string `json:"name"`
	Now     string `json:"now"`
	TZ      string `json:"tz"`
	Weekday string `json:"weekday"`
	Hour    int    `json:"hour"`
	Next    string `json:"next"`
}

type scheduleVectors struct {
	Hourly []hourlyCase `json:"hourly"`
	Weekly []weeklyCase `json:"weekly"`
}

var weekdays = map[string]time.Weekday{
	"Sunday": time.Sunday, "Monday": time.Monday, "Tuesday": time.Tuesday,
	"Wednesday": time.Wednesday, "Thursday": time.Thursday, "Friday": time.Friday,
	"Saturday": time.Saturday,
}

func TestScheduleVectors(t *testing.T) {
	hourly := []hourlyCase{
		{Name: "utc_midhour", Now: "2026-10-31T12:30:00Z", TZ: "UTC", Minute: 0},
		{Name: "utc_exact_hour", Now: "2026-10-31T12:00:00Z", TZ: "UTC", Minute: 0},
		{Name: "utc_end_of_day", Now: "2026-10-31T23:59:59Z", TZ: "UTC", Minute: 0},
		{Name: "almaty_local_hour", Now: "2026-10-31T10:30:00+05:00", TZ: "Asia/Almaty", Minute: 0},
		{Name: "minute_offset", Now: "2026-10-31T12:30:00Z", TZ: "UTC", Minute: 45},
		{Name: "dst_spring_forward", Now: "2026-03-29T00:30:00Z", TZ: "Europe/Berlin", Minute: 0},
		{Name: "dst_fall_back", Now: "2026-10-25T00:30:00Z", TZ: "Europe/Berlin", Minute: 30},
	}
	for index := range hourly {
		c := hourly[index]
		location, err := time.LoadLocation(c.TZ)
		if err != nil {
			t.Fatal(err)
		}
		now, err := time.Parse(time.RFC3339, c.Now)
		if err != nil {
			t.Fatal(err)
		}
		c.Next = nextHourly(now, location, c.Minute).Format(time.RFC3339)
		hourly[index] = c
	}

	weekly := []weeklyCase{
		{Name: "saturday_to_sunday", Now: "2026-10-31T12:00:00Z", TZ: "UTC", Weekday: "Sunday", Hour: 0},
		{Name: "monday_to_next_sunday", Now: "2026-11-02T00:00:00Z", TZ: "UTC", Weekday: "Sunday", Hour: 0},
		{Name: "same_day_earlier_hour", Now: "2026-11-01T05:00:00Z", TZ: "UTC", Weekday: "Sunday", Hour: 3},
		{Name: "almaty_weekday", Now: "2026-10-31T12:00:00Z", TZ: "Asia/Almaty", Weekday: "Wednesday", Hour: 4},
	}
	for index := range weekly {
		c := weekly[index]
		location, err := time.LoadLocation(c.TZ)
		if err != nil {
			t.Fatal(err)
		}
		now, err := time.Parse(time.RFC3339, c.Now)
		if err != nil {
			t.Fatal(err)
		}
		c.Next = nextWeekly(now, location, weekdays[c.Weekday], c.Hour).Format(time.RFC3339)
		weekly[index] = c
	}

	golden.Run(t, *updateGolden, scheduleVectorPath, scheduleVectors{Hourly: hourly, Weekly: weekly})
}
