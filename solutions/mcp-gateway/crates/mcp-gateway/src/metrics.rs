//! `mcp_gateway_*` Prometheus series.
//!
//! Ports `solutions/mcp-gateway/internal/metrics/metrics.go` without the dead
//! `mcp_gateway_backend_up` series and without Go/process collectors: the Rust
//! service exposes exactly the gateway metrics that are actually filled.

use std::sync::Arc;
use std::time::Duration;

use prometheus_client::encoding::text::encode;
use prometheus_client::encoding::{EncodeLabelSet, EncodeLabelValue};
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::family::Family;
use prometheus_client::metrics::histogram::Histogram;
use prometheus_client::registry::Registry;

/// Tool duration buckets, identical to the Go buckets.
pub const TOOL_DURATION_BUCKETS: [f64; 11] =
    [0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.0, 4.0, 6.0, 8.0];

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct ActionLabels {
    action: String,
    target: String,
    outcome: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct ToolLabels {
    tool: String,
    outcome: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct ToolNameLabels {
    tool: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct HttpLabels {
    route: String,
    status: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct VoiceLabels {
    route: String,
    outcome: String,
}

/// The metric registry that is rendered at `/metrics`.
#[derive(Debug)]
pub struct Metrics {
    registry: Registry,
    actions: Family<ActionLabels, Counter>,
    tool_calls: Family<ToolLabels, Counter>,
    tool_duration: Family<ToolNameLabels, Histogram>,
    http_requests: Family<HttpLabels, Counter>,
    voice_utterances: Family<VoiceLabels, Counter>,
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

impl Metrics {
    /// Build and register every series.
    #[must_use]
    pub fn new() -> Self {
        let mut registry = Registry::default();
        let actions = Family::<ActionLabels, Counter>::default();
        let tool_calls = Family::<ToolLabels, Counter>::default();
        let tool_duration = Family::<ToolNameLabels, Histogram>::new_with_constructor(|| {
            Histogram::new(TOOL_DURATION_BUCKETS)
        });
        let http_requests = Family::<HttpLabels, Counter>::default();
        let voice_utterances = Family::<VoiceLabels, Counter>::default();
        registry.register(
            "mcp_gateway_actions",
            "Executed mutating gateway actions.",
            actions.clone(),
        );
        registry.register(
            "mcp_gateway_tool_calls",
            "MCP tool calls by tool and outcome.",
            tool_calls.clone(),
        );
        registry.register(
            "mcp_gateway_tool_duration_seconds",
            "MCP tool call duration.",
            tool_duration.clone(),
        );
        registry.register(
            "mcp_gateway_http_requests",
            "HTTP requests by normalized route and status.",
            http_requests.clone(),
        );
        registry.register(
            "mcp_gateway_voice_utterances",
            "Voice utterances by route and outcome.",
            voice_utterances.clone(),
        );
        Self {
            registry,
            actions,
            tool_calls,
            tool_duration,
            http_requests,
            voice_utterances,
        }
    }

    /// Counts one mutating action.
    pub fn observe_action(&self, action: &str, target: &str, outcome: &str) {
        self.actions
            .get_or_create(&ActionLabels {
                action: action.to_owned(),
                target: target.to_owned(),
                outcome: outcome.to_owned(),
            })
            .inc();
    }

    /// Counts and times one tool call.
    pub fn observe_tool(&self, tool: &str, outcome: &str, duration: Duration) {
        self.tool_calls
            .get_or_create(&ToolLabels {
                tool: tool.to_owned(),
                outcome: outcome.to_owned(),
            })
            .inc();
        self.tool_duration
            .get_or_create(&ToolNameLabels {
                tool: tool.to_owned(),
            })
            .observe(duration.as_secs_f64());
    }

    /// Counts one HTTP request.
    pub fn observe_http(&self, route: &str, status: u16) {
        self.http_requests
            .get_or_create(&HttpLabels {
                route: route.to_owned(),
                status: status.to_string(),
            })
            .inc();
    }

    /// Counts one voice utterance outcome.
    pub fn observe_voice(&self, route: &str, outcome: &str) {
        self.voice_utterances
            .get_or_create(&VoiceLabels {
                route: route.to_owned(),
                outcome: outcome.to_owned(),
            })
            .inc();
    }

    /// Prometheus text exposition.
    #[must_use]
    pub fn render(&self) -> String {
        let mut body = String::new();
        encode(&mut body, &self.registry).expect("writing to a String cannot fail");
        body
    }
}

/// A cloneable handle used by every handler.
pub type SharedMetrics = Arc<Metrics>;

#[allow(dead_code)]
fn _assert_label_value(_: impl EncodeLabelValue) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_only_gateway_series() {
        let metrics = Metrics::new();
        metrics.observe_tool("infra_status", "ok", Duration::from_millis(12));
        metrics.observe_http("/mcp", 200);
        metrics.observe_action("rollout-restart", "scheduler", "success");
        metrics.observe_voice("hermes", "ok");
        let body = metrics.render();
        // `prometheus-client` appends `_total` to counters; the Go-era names
        // already end in `_total`, so the registered names must not.
        assert!(body.contains("mcp_gateway_tool_calls_total{"));
        assert!(body.contains("mcp_gateway_tool_duration_seconds_bucket"));
        assert!(body.contains("mcp_gateway_http_requests_total{"));
        assert!(body.contains("mcp_gateway_actions_total{"));
        assert!(body.contains("mcp_gateway_voice_utterances_total{"));
        assert!(!body.contains("_total_total"));
        assert!(!body.contains("backend_up"));
        assert!(!body.contains("go_"));
        assert!(!body.contains("process_"));
    }
}
