package main

import (
	"errors"
	"fmt"
	"net"
	"net/url"
	"os"
	"regexp"
	"strconv"
	"strings"
	"time"
)

var consumerIDPattern = regexp.MustCompile(`^[A-Za-z0-9._-]{1,64}$`)
var queuePattern = regexp.MustCompile(`^[a-z][a-z0-9._-]{0,31}$`)

// settings is the validated ha-control configuration (environment only).
type settings struct {
	HAURL, HAToken, HACAFile              string
	SchedulerURL, SchedulerToken, SchedCA string
	CatalogPath                           string
	Listen                                string
	ControlToken                          string
	ConsumerID, Queue                     string
	Execute                               bool
	BatchSize, LeaseSeconds, MaxAttempts  int
	PollInterval, MaxBackoff, HATimeout   time.Duration
	SchedulerTimeout                      time.Duration
}

func loadSettings(getenv func(string) string) (*settings, error) {
	s := &settings{BatchSize: 10, LeaseSeconds: 300, MaxAttempts: 10}
	var err error
	if s.HAURL, err = baseURL("HA_URL", getenv("HA_URL")); err != nil {
		return nil, err
	}
	if s.SchedulerURL, err = baseURL("SCHEDULER_ENDPOINT", getenv("SCHEDULER_ENDPOINT")); err != nil {
		return nil, err
	}
	s.SchedulerURL = strings.TrimSuffix(s.SchedulerURL, "/ricardo.v1.SchedulerService")
	for _, t := range []struct {
		dst  *string
		name string
	}{{&s.HAToken, "HA_TOKEN"}, {&s.SchedulerToken, "SCHEDULER_TOKEN"}, {&s.ControlToken, "CONTROL_TOKEN"}} {
		if *t.dst, err = token(getenv, t.name); err != nil {
			return nil, err
		}
		if *t.dst == "" {
			return nil, fmt.Errorf("config: %s (or %s_FILE) is required", t.name, t.name)
		}
	}
	s.HACAFile = strings.TrimSpace(getenv("HA_CA_FILE"))
	s.SchedCA = strings.TrimSpace(getenv("SCHEDULER_CA_FILE"))

	if s.CatalogPath = strings.TrimSpace(getenv("CONTROL_CATALOG")); s.CatalogPath == "" {
		return nil, errors.New("config: CONTROL_CATALOG is required")
	}
	if s.Listen = strings.TrimSpace(getenv("CONTROL_LISTEN")); s.Listen == "" {
		s.Listen = "127.0.0.1:8099"
	}
	if _, _, err := net.SplitHostPort(s.Listen); err != nil {
		return nil, errors.New("config: CONTROL_LISTEN must be host:port")
	}

	s.Queue = strings.TrimSpace(getenv("CONTROL_QUEUE"))
	if s.Queue == "" {
		s.Queue = "control"
	}
	if !queuePattern.MatchString(s.Queue) || s.Queue == "home" || s.Queue == "tasks" {
		return nil, errors.New("config: CONTROL_QUEUE must be a dedicated queue (not home or tasks)")
	}
	s.ConsumerID = strings.TrimSpace(getenv("CONTROL_CONSUMER_ID"))
	if !consumerIDPattern.MatchString(s.ConsumerID) {
		return nil, errors.New("config: CONTROL_CONSUMER_ID must match [A-Za-z0-9._-]{1,64}")
	}
	if raw := strings.TrimSpace(getenv("CONTROL_EXECUTE")); raw != "" {
		if s.Execute, err = strconv.ParseBool(raw); err != nil {
			return nil, errors.New("config: CONTROL_EXECUTE must be true/false")
		}
	}
	s.PollInterval = 500 * time.Millisecond
	s.MaxBackoff = 2 * time.Minute
	s.HATimeout = 10 * time.Second
	s.SchedulerTimeout = 10 * time.Second
	for _, d := range []struct {
		dst  *time.Duration
		name string
	}{{&s.PollInterval, "CONTROL_POLL_INTERVAL"}, {&s.MaxBackoff, "CONTROL_MAX_BACKOFF"},
		{&s.HATimeout, "CONTROL_HA_TIMEOUT"}, {&s.SchedulerTimeout, "CONTROL_SCHEDULER_TIMEOUT"}} {
		if raw := strings.TrimSpace(getenv(d.name)); raw != "" {
			v, err := time.ParseDuration(raw)
			if err != nil || v <= 0 {
				return nil, fmt.Errorf("config: %s must be a positive Go duration", d.name)
			}
			*d.dst = v
		}
	}
	return s, nil
}

func token(getenv func(string) string, name string) (string, error) {
	if path := strings.TrimSpace(getenv(name + "_FILE")); path != "" {
		raw, err := os.ReadFile(path)
		if err != nil {
			return "", fmt.Errorf("config: read %s_FILE: %w", name, err)
		}
		value := strings.TrimSpace(string(raw))
		if value == "" {
			return "", fmt.Errorf("config: %s_FILE is empty", name)
		}
		return value, nil
	}
	return strings.TrimSpace(getenv(name)), nil
}

func baseURL(name, raw string) (string, error) {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return "", fmt.Errorf("config: %s is required", name)
	}
	u, err := url.Parse(raw)
	if err != nil || (u.Scheme != "http" && u.Scheme != "https") || u.Host == "" {
		return "", fmt.Errorf("config: %s must be an http(s) URL", name)
	}
	if u.User != nil || u.RawQuery != "" || u.Fragment != "" {
		return "", fmt.Errorf("config: %s must not contain credentials, query or fragment", name)
	}
	return strings.TrimSuffix(u.String(), "/"), nil
}
