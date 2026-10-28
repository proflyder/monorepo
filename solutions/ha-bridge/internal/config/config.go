// Package config loads the ha-bridge runtime configuration from the
// environment. Values are validated eagerly so a misconfigured bridge fails
// before it leases or acknowledges any scheduler event.
package config

import (
	"errors"
	"fmt"
	"net/url"
	"os"
	"regexp"
	"strconv"
	"strings"
	"time"
)

// Environment variable names. Every variable is read through the injected
// getenv function so tests never touch the real process environment.
const (
	EnvHAURL            = "HA_URL"
	EnvHAToken          = "HA_TOKEN"
	EnvHATokenFile      = "HA_TOKEN_FILE"
	EnvHACAFile         = "HA_CA_FILE"
	EnvSchedulerURL     = "SCHEDULER_ENDPOINT"
	EnvSchedulerToken   = "SCHEDULER_TOKEN"
	EnvSchedulerTokenF  = "SCHEDULER_TOKEN_FILE"
	EnvSchedulerCAFile  = "SCHEDULER_CA_FILE"
	EnvConsumerID       = "BRIDGE_CONSUMER_ID"
	EnvQueue            = "BRIDGE_QUEUE"
	EnvExecute          = "BRIDGE_EXECUTE"
	EnvBatchSize        = "BRIDGE_BATCH_SIZE"
	EnvLeaseSeconds     = "BRIDGE_LEASE_SECONDS"
	EnvPollInterval     = "BRIDGE_POLL_INTERVAL"
	EnvMaxBackoff       = "BRIDGE_MAX_BACKOFF"
	EnvHATimeout        = "BRIDGE_HA_TIMEOUT"
	EnvSchedulerTimeout = "BRIDGE_SCHEDULER_TIMEOUT"
	EnvMaxAttempts      = "BRIDGE_MAX_ATTEMPTS"
	EnvAlarmScript      = "BRIDGE_ALARM_SCRIPT"
	EnvAnnounceScript   = "BRIDGE_ANNOUNCE_SCRIPT"
	EnvStopScript       = "BRIDGE_STOP_SCRIPT"
	EnvLedgerPath       = "BRIDGE_LEDGER_PATH"
)

// Limits mirror packages/proto/ricardo/v1/scheduler.proto.
const (
	MinBatchSize    = 1
	MaxBatchSize    = 200
	MinLeaseSeconds = 1
	MaxLeaseSeconds = 3600

	// ledgerBudget is the assumed worst-case time spent writing the local
	// receipt ledger for one event.
	ledgerBudget = time.Second
	// leaseCushion keeps the lease valid a little beyond the modelled
	// worst-case processing time.
	leaseCushion = 15 * time.Second
)

// QueueHome is the queue that carries alarms, reminders and timers.
const QueueHome = "home"

// QueueTasks is the scheduler's default queue for generic tasks. The HA
// executor must never lease it: a task is not an HA sound action.
const QueueTasks = "tasks"

var (
	consumerIDPattern = regexp.MustCompile(`^[A-Za-z0-9._-]{1,64}$`)
	scriptPattern     = regexp.MustCompile(`^[A-Za-z0-9_.-]{1,64}$`)
	queuePattern      = regexp.MustCompile(`^[A-Za-z0-9._-]{1,64}$`)
)

// Config is the complete, validated bridge configuration.
type Config struct {
	HAURL            string
	SchedulerURL     string
	HAToken          string
	SchedulerToken   string
	ConsumerID       string
	Queue            string
	Execute          bool
	BatchSize        int
	LeaseSeconds     int
	PollInterval     time.Duration
	MaxBackoff       time.Duration
	HATimeout        time.Duration
	SchedulerTimeout time.Duration
	MaxAttempts      int
	AlarmScript      string
	AnnounceScript   string
	StopScript       string
	LedgerPath       string
	HACAFile         string
	SchedulerCAFile  string
}

// RequiredLeaseSeconds reports the smallest lease that still covers a full
// batch of sequential executions with the configured per-request timeouts.
func (c *Config) RequiredLeaseSeconds() int {
	perEvent := c.HATimeout + c.SchedulerTimeout + ledgerBudget
	total := time.Duration(c.BatchSize)*perEvent + leaseCushion
	secs := int(total / time.Second)
	if total%time.Second != 0 {
		secs++
	}
	return secs
}

// Load reads and validates the configuration. getenv is normally os.Getenv.
func Load(getenv func(string) string) (*Config, error) {
	if getenv == nil {
		return nil, errors.New("config: nil getenv")
	}

	cfg := &Config{
		BatchSize:    10,
		LeaseSeconds: 300,
	}

	var err error
	cfg.HAURL, err = baseURL(EnvHAURL, getenv(EnvHAURL), true)
	if err != nil {
		return nil, err
	}
	cfg.SchedulerURL, err = baseURL(EnvSchedulerURL, getenv(EnvSchedulerURL), true)
	if err != nil {
		return nil, err
	}
	// A full RPC URL is tolerated; the client appends the method name.
	cfg.SchedulerURL = strings.TrimSuffix(cfg.SchedulerURL, "/ricardo.v1.SchedulerService")

	cfg.SchedulerToken, err = token(getenv, EnvSchedulerToken)
	if err != nil {
		return nil, err
	}
	if cfg.SchedulerToken == "" {
		return nil, fmt.Errorf("config: %s (or %s) is required", EnvSchedulerToken, EnvSchedulerTokenF)
	}

	cfg.HAToken, err = token(getenv, EnvHAToken)
	if err != nil {
		return nil, err
	}

	if cfg.Execute, err = boolValue(getenv, EnvExecute, false); err != nil {
		return nil, err
	}
	if cfg.Execute && cfg.HAToken == "" {
		return nil, fmt.Errorf("config: %s (or %s) is required when %s=true", EnvHAToken, EnvHATokenFile, EnvExecute)
	}

	cfg.ConsumerID = strings.TrimSpace(getenv(EnvConsumerID))
	if cfg.ConsumerID == "" {
		return nil, fmt.Errorf("config: %s is required", EnvConsumerID)
	}
	if !consumerIDPattern.MatchString(cfg.ConsumerID) {
		return nil, fmt.Errorf("config: %s must match [A-Za-z0-9._-]{1,64}", EnvConsumerID)
	}

	cfg.Queue = strings.TrimSpace(getenv(EnvQueue))
	if cfg.Queue == "" {
		cfg.Queue = QueueHome
	}
	if !queuePattern.MatchString(cfg.Queue) {
		return nil, fmt.Errorf("config: %s must match [A-Za-z0-9._-]{1,64}", EnvQueue)
	}
	if cfg.Queue == QueueTasks {
		return nil, fmt.Errorf("config: %s=%q is reserved for generic tasks and must not be leased by the HA executor", EnvQueue, QueueTasks)
	}

	if cfg.BatchSize, err = intValue(getenv, EnvBatchSize, cfg.BatchSize, MinBatchSize, MaxBatchSize); err != nil {
		return nil, err
	}
	if cfg.LeaseSeconds, err = intValue(getenv, EnvLeaseSeconds, cfg.LeaseSeconds, MinLeaseSeconds, MaxLeaseSeconds); err != nil {
		return nil, err
	}
	if cfg.MaxAttempts, err = intValue(getenv, EnvMaxAttempts, 10, 1, 1000); err != nil {
		return nil, err
	}

	if cfg.HATimeout, err = durationValue(getenv, EnvHATimeout, 10*time.Second); err != nil {
		return nil, err
	}
	if cfg.SchedulerTimeout, err = durationValue(getenv, EnvSchedulerTimeout, 10*time.Second); err != nil {
		return nil, err
	}
	if cfg.PollInterval, err = durationValue(getenv, EnvPollInterval, time.Second); err != nil {
		return nil, err
	}
	if cfg.MaxBackoff, err = durationValue(getenv, EnvMaxBackoff, 5*time.Minute); err != nil {
		return nil, err
	}
	if cfg.MaxBackoff < cfg.PollInterval {
		return nil, fmt.Errorf("config: %s (%s) must be >= %s (%s)", EnvMaxBackoff, cfg.MaxBackoff, EnvPollInterval, cfg.PollInterval)
	}

	if required := cfg.RequiredLeaseSeconds(); cfg.LeaseSeconds < required {
		return nil, fmt.Errorf(
			"config: %s=%d is too short for %s=%d with %s=%s and %s=%s; need at least %d s (raise the lease, lower the batch or shorten the timeouts)",
			EnvLeaseSeconds, cfg.LeaseSeconds, EnvBatchSize, cfg.BatchSize,
			EnvHATimeout, cfg.HATimeout, EnvSchedulerTimeout, cfg.SchedulerTimeout, required)
	}

	if cfg.AlarmScript, err = scriptValue(getenv, EnvAlarmScript, "alarm_start"); err != nil {
		return nil, err
	}
	// schedule_announce is the existing HA voice script that takes message and
	// chime. Override it for a different deployment; the bridge does not
	// require any particular HA runtime.
	if cfg.AnnounceScript, err = scriptValue(getenv, EnvAnnounceScript, "schedule_announce"); err != nil {
		return nil, err
	}
	if cfg.StopScript, err = scriptValue(getenv, EnvStopScript, "alarm_stop"); err != nil {
		return nil, err
	}

	cfg.LedgerPath = strings.TrimSpace(getenv(EnvLedgerPath))
	cfg.HACAFile = strings.TrimSpace(getenv(EnvHACAFile))
	cfg.SchedulerCAFile = strings.TrimSpace(getenv(EnvSchedulerCAFile))

	return cfg, nil
}

// token resolves NAME_FILE first, then NAME. The value is never logged.
func token(getenv func(string) string, name string) (string, error) {
	path := strings.TrimSpace(getenv(name + "_FILE"))
	if path != "" {
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

func baseURL(name, raw string, required bool) (string, error) {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		if required {
			return "", fmt.Errorf("config: %s is required", name)
		}
		return "", nil
	}
	u, err := url.Parse(raw)
	if err != nil {
		return "", fmt.Errorf("config: %s is not a valid URL", name)
	}
	if u.Scheme != "http" && u.Scheme != "https" {
		return "", fmt.Errorf("config: %s must use http or https", name)
	}
	if u.Host == "" {
		return "", fmt.Errorf("config: %s has no host", name)
	}
	if u.User != nil {
		return "", fmt.Errorf("config: %s must not embed credentials; use a token file instead", name)
	}
	if u.RawQuery != "" || u.Fragment != "" {
		return "", fmt.Errorf("config: %s must not contain a query or fragment", name)
	}
	return strings.TrimSuffix(u.String(), "/"), nil
}

func scriptValue(getenv func(string) string, name, def string) (string, error) {
	raw := strings.TrimSpace(getenv(name))
	if raw == "" {
		raw = def
	}
	if !scriptPattern.MatchString(raw) {
		return "", fmt.Errorf("config: %s must match [A-Za-z0-9_.-]{1,64}", name)
	}
	return raw, nil
}

func boolValue(getenv func(string) string, name string, def bool) (bool, error) {
	raw := strings.TrimSpace(getenv(name))
	if raw == "" {
		return def, nil
	}
	v, err := strconv.ParseBool(raw)
	if err != nil {
		return false, fmt.Errorf("config: %s must be true/false", name)
	}
	return v, nil
}

func intValue(getenv func(string) string, name string, def, min, max int) (int, error) {
	raw := strings.TrimSpace(getenv(name))
	if raw == "" {
		return def, nil
	}
	v, err := strconv.Atoi(raw)
	if err != nil {
		return 0, fmt.Errorf("config: %s must be an integer", name)
	}
	if v < min || v > max {
		return 0, fmt.Errorf("config: %s must be within %d..%d", name, min, max)
	}
	return v, nil
}

func durationValue(getenv func(string) string, name string, def time.Duration) (time.Duration, error) {
	raw := strings.TrimSpace(getenv(name))
	if raw == "" {
		return def, nil
	}
	v, err := time.ParseDuration(raw)
	if err != nil {
		return 0, fmt.Errorf("config: %s must be a Go duration such as 10s or 2m", name)
	}
	if v <= 0 {
		return 0, fmt.Errorf("config: %s must be positive", name)
	}
	return v, nil
}
