// Package metrics exposes the Prometheus metrics required by the spec.
package metrics

import (
	"net/http"
	"strconv"

	"github.com/prometheus/client_golang/prometheus"
	"github.com/prometheus/client_golang/prometheus/collectors"
	"github.com/prometheus/client_golang/prometheus/promhttp"
)

// Metrics owns the process registry and all currency-bot instruments.
type Metrics struct {
	registry     *prometheus.Registry
	rates        *prometheus.GaugeVec
	jobRuns      *prometheus.CounterVec
	jobFailures  *prometheus.CounterVec
	parseSeconds *prometheus.HistogramVec
	tgFailures   prometheus.Counter
	httpRequests *prometheus.CounterVec
}

// New builds an isolated registry so tests can create independent instances.
func New() *Metrics {
	registry := prometheus.NewRegistry()
	m := &Metrics{
		registry: registry,
		rates: prometheus.NewGaugeVec(prometheus.GaugeOpts{
			Name: "currency_bot_rate",
			Help: "Latest aggregated rate by pair and side.",
		}, []string{"pair", "type"}),
		jobRuns: prometheus.NewCounterVec(prometheus.CounterOpts{
			Name: "currency_bot_job_runs_total",
			Help: "Scheduled or manual job runs.",
		}, []string{"job"}),
		jobFailures: prometheus.NewCounterVec(prometheus.CounterOpts{
			Name: "currency_bot_job_failures_total",
			Help: "Failed job runs.",
		}, []string{"job"}),
		parseSeconds: prometheus.NewHistogramVec(prometheus.HistogramOpts{
			Name:    "currency_bot_parse_duration_seconds",
			Help:    "Time spent fetching and parsing kurs.kz.",
			Buckets: prometheus.DefBuckets,
		}, []string{"job"}),
		tgFailures: prometheus.NewCounter(prometheus.CounterOpts{
			Name: "currency_bot_telegram_send_failures_total",
			Help: "Failed Telegram sendMessage calls.",
		}),
		httpRequests: prometheus.NewCounterVec(prometheus.CounterOpts{
			Name: "currency_bot_http_requests_total",
			Help: "HTTP requests by route and status.",
		}, []string{"route", "status"}),
	}
	registry.MustRegister(
		m.rates, m.jobRuns, m.jobFailures, m.parseSeconds, m.tgFailures, m.httpRequests,
		collectors.NewGoCollector(),
		collectors.NewProcessCollector(collectors.ProcessCollectorOpts{}),
	)
	return m
}

// Handler returns the exposition handler.
func (m *Metrics) Handler() http.Handler {
	return promhttp.HandlerFor(m.registry, promhttp.HandlerOpts{})
}

// SetRates updates the latest-rate gauges.
func (m *Metrics) SetRates(usdBuy, usdSell, rubBuy, rubSell float64) {
	m.rates.WithLabelValues("USD_TO_KZT", "BUY").Set(usdBuy)
	m.rates.WithLabelValues("USD_TO_KZT", "SELL").Set(usdSell)
	m.rates.WithLabelValues("RUB_TO_KZT", "BUY").Set(rubBuy)
	m.rates.WithLabelValues("RUB_TO_KZT", "SELL").Set(rubSell)
}

// IncJobRun counts a finished run.
func (m *Metrics) IncJobRun(job string) { m.jobRuns.WithLabelValues(job).Inc() }

// IncJobFailure counts a failed run.
func (m *Metrics) IncJobFailure(job string) { m.jobFailures.WithLabelValues(job).Inc() }

// ObserveParse records a parse duration.
func (m *Metrics) ObserveParse(job string, seconds float64) {
	m.parseSeconds.WithLabelValues(job).Observe(seconds)
}

// IncTelegramFailure counts a failed Telegram send.
func (m *Metrics) IncTelegramFailure() { m.tgFailures.Inc() }

// ObserveHTTP counts one request.
func (m *Metrics) ObserveHTTP(route string, status int) {
	m.httpRequests.WithLabelValues(route, strconv.Itoa(status)).Inc()
}
