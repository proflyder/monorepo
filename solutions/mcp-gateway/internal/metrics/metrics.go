// Package metrics owns the isolated Prometheus registry of the gateway.
package metrics

import (
	"net/http"
	"strconv"
	"time"

	"github.com/prometheus/client_golang/prometheus"
	"github.com/prometheus/client_golang/prometheus/collectors"
	"github.com/prometheus/client_golang/prometheus/promhttp"
)

// Metrics is the process registry plus the gateway instruments.
type Metrics struct {
	registry   *prometheus.Registry
	actions    *prometheus.CounterVec
	toolCalls  *prometheus.CounterVec
	toolTime   *prometheus.HistogramVec
	httpCount  *prometheus.CounterVec
	backendsUp *prometheus.GaugeVec
}

// New builds an isolated registry so tests never share state.
func New() *Metrics {
	registry := prometheus.NewRegistry()
	m := &Metrics{
		registry: registry,
		actions: prometheus.NewCounterVec(prometheus.CounterOpts{
			Name: "mcp_gateway_actions_total",
			Help: "Mutation attempts by action, target and outcome.",
		}, []string{"action", "target", "outcome"}),
		toolCalls: prometheus.NewCounterVec(prometheus.CounterOpts{
			Name: "mcp_gateway_tool_calls_total",
			Help: "MCP tool calls by tool and outcome.",
		}, []string{"tool", "outcome"}),
		toolTime: prometheus.NewHistogramVec(prometheus.HistogramOpts{
			Name:    "mcp_gateway_tool_duration_seconds",
			Help:    "MCP tool call duration.",
			Buckets: []float64{0.01, 0.05, 0.1, 0.25, 0.5, 1, 2, 4, 8},
		}, []string{"tool"}),
		httpCount: prometheus.NewCounterVec(prometheus.CounterOpts{
			Name: "mcp_gateway_http_requests_total",
			Help: "HTTP requests by route and status.",
		}, []string{"route", "status"}),
		backendsUp: prometheus.NewGaugeVec(prometheus.GaugeOpts{
			Name: "mcp_gateway_backend_up",
			Help: "Last observed backend reachability by backend name.",
		}, []string{"backend"}),
	}
	registry.MustRegister(
		m.actions, m.toolCalls, m.toolTime, m.httpCount, m.backendsUp,
		collectors.NewGoCollector(),
		collectors.NewProcessCollector(collectors.ProcessCollectorOpts{}),
	)
	return m
}

// Handler returns the exposition handler.
func (m *Metrics) Handler() http.Handler {
	return promhttp.HandlerFor(m.registry, promhttp.HandlerOpts{})
}

// Registry exposes the underlying registry for tests.
func (m *Metrics) Registry() *prometheus.Registry { return m.registry }

// ObserveAction counts one mutation attempt.
func (m *Metrics) ObserveAction(action, target, outcome string) {
	m.actions.WithLabelValues(action, target, outcome).Inc()
}

// ObserveTool counts one tool call.
func (m *Metrics) ObserveTool(tool, outcome string, duration time.Duration) {
	m.toolCalls.WithLabelValues(tool, outcome).Inc()
	m.toolTime.WithLabelValues(tool).Observe(duration.Seconds())
}

// ObserveHTTP counts one HTTP request.
func (m *Metrics) ObserveHTTP(route string, status int) {
	m.httpCount.WithLabelValues(route, strconv.Itoa(status)).Inc()
}

// SetBackendUp records backend reachability.
func (m *Metrics) SetBackendUp(backend string, up bool) {
	value := 0.0
	if up {
		value = 1
	}
	m.backendsUp.WithLabelValues(backend).Set(value)
}
