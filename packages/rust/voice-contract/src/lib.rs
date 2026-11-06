//! Typed wire contract of the mcp-gateway voice API.
//!
//! The routes are served by `solutions/mcp-gateway/internal/voice` and called
//! by `solutions/voice-proxy/internal/gateway`:
//!
//! * `POST /v1/voice/utterance` — route one spoken phrase;
//! * `POST /v1/voice/result` — long-poll the answer of a routed turn;
//! * `POST /v1/voice/updates` — poll dispatcher turns;
//! * `GET  /v1/voice/state` — debug view of the agent-conversation switch.
//!
//! The types mirror the Go JSON exactly, including the protobuf-free field
//! names: `route`, `ok`, `threadId`, `afterSeq`, `waitSeconds`, `agentMode`,
//! `cursor`, `items`, `completedAt`. Request decoding rejects unknown fields,
//! like the server's `DisallowUnknownFields`. Response decoding tolerates them.
//!
//! There is no I/O and no runtime dependency here: this crate is safe to use
//! from a server, a client and a test.
//!
//! An unknown `route` value never fails the parse. It is preserved as
//! [`Route::Unknown`] so the caller can answer "Сервер агентов недоступен."
//! instead of inventing a destination.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A profile accepted by the gateway.
pub const PROFILE_PERSONAL: &str = "personal";
/// A profile accepted by the gateway.
pub const PROFILE_WORK: &str = "work";

/// Speech budget accepted from the proxy, in Unicode scalar values.
pub const MAX_TEXT_RUNES: usize = 2000;

/// Where the gateway routed one phrase.
///
/// Only [`Route::Hermes`], [`Route::Bb`] and [`Route::Local`] are known to the
/// proxy. Any other value is preserved as [`Route::Unknown`]; the proxy must
/// treat it as an unavailable agent server and must not forward it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    /// Forward the phrase to Hermes unchanged.
    Hermes,
    /// The phrase went to a bb thread; poll the result route.
    Bb,
    /// The gateway answered locally and the proxy must speak `speech`.
    Local,
    /// A route this version does not know. The value is the raw route name.
    Unknown(String),
}

impl Route {
    /// Parse a raw `route` value without failing on unknown routes.
    #[must_use]
    pub fn parse(raw: &str) -> Self {
        match raw {
            "hermes" => Self::Hermes,
            "bb" => Self::Bb,
            "local" => Self::Local,
            other => Self::Unknown(other.to_owned()),
        }
    }

    /// The raw wire value.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Hermes => "hermes",
            Self::Bb => "bb",
            Self::Local => "local",
            Self::Unknown(name) => name,
        }
    }

    /// Whether this is the catch-all route.
    #[must_use]
    pub fn is_unknown(&self) -> bool {
        matches!(self, Self::Unknown(_))
    }
}

impl fmt::Display for Route {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl Serialize for Route {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Route {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Ok(Self::parse(&raw))
    }
}

/// One recognised phrase plus the HA voice profile.
///
/// Unknown fields are rejected, matching the Go server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UtteranceRequest {
    pub text: String,
    pub profile: String,
}

/// Where one phrase went. `route` is always present; the other fields are
/// omitted when the gateway does not set them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UtteranceResponse {
    pub route: Route,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ok: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_seq: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speech: Option<String>,
}

impl UtteranceResponse {
    /// Forward the phrase to Hermes.
    #[must_use]
    pub fn hermes() -> Self {
        Self::route(Route::Hermes)
    }

    /// Speak a gateway-local answer.
    #[must_use]
    pub fn local(speech: impl Into<String>) -> Self {
        Self {
            speech: Some(speech.into()),
            ..Self::route(Route::Local)
        }
    }

    /// A phrase accepted by a bb thread.
    #[must_use]
    pub fn bb(
        thread_id: impl Into<String>,
        after_seq: i64,
        ok: bool,
        speech: impl Into<String>,
    ) -> Self {
        Self {
            ok: Some(ok),
            thread_id: Some(thread_id.into()),
            after_seq: Some(after_seq),
            speech: Some(speech.into()),
            ..Self::route(Route::Bb)
        }
    }

    /// A route this version does not understand.
    #[must_use]
    pub fn unknown(route: impl Into<String>) -> Self {
        Self::route(Route::Unknown(route.into()))
    }

    fn route(route: Route) -> Self {
        Self {
            route,
            ok: None,
            thread_id: None,
            after_seq: None,
            speech: None,
        }
    }
}

/// Long-poll one routed turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ResultRequest {
    pub thread_id: String,
    #[serde(default)]
    pub after_seq: i64,
    #[serde(default)]
    pub wait_seconds: i32,
}

/// The turn outcome. `status` is never omitted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultResponse {
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speech: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Sequence of the completed turn, used as the queue idempotency key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_seq: Option<i64>,
}

/// Debug view of the agent-conversation switch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StateResponse {
    pub agent_mode: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
}

/// Long-poll dispatcher updates. `after_seq == None` is the bootstrap call:
/// the gateway returns the current cursor and no history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct UpdatesRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_seq: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait_seconds: Option<i32>,
}

/// One dispatcher turn the owner did not start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateItem {
    pub seq: i64,
    pub spoken: String,
    /// Turn-completed event time in epoch milliseconds; absent when the
    /// gateway did not report one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<i64>,
}

/// The dispatcher-updates answer. `status` is only set on a failure, mirroring
/// `{"status":"failed","speech":...}` of the result route.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdatesResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<i64>,
    #[serde(default)]
    pub items: Vec<UpdateItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speech: Option<String>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        MAX_TEXT_RUNES, ResultRequest, ResultResponse, Route, StateResponse, UpdateItem,
        UpdatesRequest, UpdatesResponse, UtteranceRequest, UtteranceResponse,
    };

    fn round_trip<T>(value: &T) -> serde_json::Value
    where
        T: serde::Serialize,
    {
        serde_json::to_value(value).expect("serialize")
    }

    // Samples below are taken from the Go tests on both sides:
    // solutions/mcp-gateway/internal/voice/voice_test.go and
    // solutions/voice-proxy/internal/gateway/client_test.go.

    #[test]
    fn utterance_request_round_trips_like_the_go_client() {
        let wire = json!({"text": "проверь", "profile": "work"});
        let request: UtteranceRequest = serde_json::from_value(wire.clone()).expect("parse");
        assert_eq!(request.text, "проверь");
        assert_eq!(request.profile, "work");
        assert_eq!(round_trip(&request), wire);
    }

    #[test]
    fn utterance_request_rejects_unknown_fields_like_the_go_server() {
        let wire = json!({"text": "проверь", "profile": "work", "extra": true});
        assert!(serde_json::from_value::<UtteranceRequest>(wire).is_err());
    }

    #[test]
    fn hermes_route_has_only_the_route_field() {
        let wire = json!({"route": "hermes"});
        let response: UtteranceResponse = serde_json::from_value(wire.clone()).expect("parse");
        assert_eq!(response.route, Route::Hermes);
        assert_eq!(response.ok, None);
        assert_eq!(round_trip(&response), wire);
    }

    #[test]
    fn bb_route_round_trips_with_the_go_speech() {
        let wire = json!({
            "route": "bb",
            "ok": true,
            "threadId": "thr_work",
            "afterSeq": 42,
            "speech": "Передал в рабочий тред."
        });
        let response: UtteranceResponse = serde_json::from_value(wire.clone()).expect("parse");
        assert_eq!(response.route, Route::Bb);
        assert_eq!(response.ok, Some(true));
        assert_eq!(response.thread_id.as_deref(), Some("thr_work"));
        assert_eq!(response.after_seq, Some(42));
        assert_eq!(round_trip(&response), wire);
    }

    #[test]
    fn failed_bb_route_keeps_ok_false() {
        let wire = json!({
            "route": "bb",
            "ok": false,
            "threadId": "thr_work",
            "speech": "Сервер агентов недоступен."
        });
        let response: UtteranceResponse = serde_json::from_value(wire.clone()).expect("parse");
        assert_eq!(response.ok, Some(false));
        assert_eq!(round_trip(&response), wire);
    }

    #[test]
    fn local_route_keeps_the_gateway_phrase() {
        let wire = json!({"route": "local", "speech": "Выключил режим агента."});
        let response: UtteranceResponse = serde_json::from_value(wire.clone()).expect("parse");
        assert_eq!(response.route, Route::Local);
        assert_eq!(response.speech.as_deref(), Some("Выключил режим агента."));
        assert_eq!(round_trip(&response), wire);
    }

    #[test]
    fn unknown_route_is_preserved_and_does_not_fail_the_parse() {
        let wire = json!({"route": "mcp", "speech": "нет"});
        let response: UtteranceResponse = serde_json::from_value(wire.clone()).expect("parse");
        assert_eq!(response.route, Route::Unknown("mcp".to_owned()));
        assert!(response.route.is_unknown());
        assert_eq!(round_trip(&response), wire);
    }

    #[test]
    fn response_decoding_ignores_unknown_fields() {
        let wire = json!({"route": "hermes", "future": 1});
        let response: UtteranceResponse = serde_json::from_value(wire).expect("parse");
        assert_eq!(response.route, Route::Hermes);
    }

    #[test]
    fn result_request_round_trips() {
        let wire = json!({"threadId": "thr_1", "afterSeq": 42, "waitSeconds": 1});
        let request: ResultRequest = serde_json::from_value(wire.clone()).expect("parse");
        assert_eq!(request.thread_id, "thr_1");
        assert_eq!(request.after_seq, 42);
        assert_eq!(request.wait_seconds, 1);
        assert_eq!(round_trip(&request), wire);
    }

    #[test]
    fn result_request_defaults_missing_numbers_to_zero() {
        let request: ResultRequest =
            serde_json::from_value(json!({"threadId": "thr_1"})).expect("parse");
        assert_eq!(request.after_seq, 0);
        assert_eq!(request.wait_seconds, 0);
    }

    #[test]
    fn result_response_matches_the_go_fixture() {
        let wire =
            json!({"status": "idle", "speech": "Готово, тесты починены.", "text": "details"});
        let response: ResultResponse = serde_json::from_value(wire.clone()).expect("parse");
        assert_eq!(response.status, "idle");
        assert_eq!(response.speech.as_deref(), Some("Готово, тесты починены."));
        assert_eq!(response.text.as_deref(), Some("details"));
        assert_eq!(response.completed_seq, None);
        assert_eq!(round_trip(&response), wire);
    }

    #[test]
    fn result_response_carries_the_completed_sequence() {
        let wire = json!({"status": "done", "speech": "Готово.", "completedSeq": 42});
        let response: ResultResponse = serde_json::from_value(wire.clone()).expect("parse");
        assert_eq!(response.completed_seq, Some(42));
        assert_eq!(round_trip(&response), wire);
    }

    #[test]
    fn state_response_round_trips() {
        let wire = json!({"agentMode": true, "threadId": "thr_agent"});
        let state: StateResponse = serde_json::from_value(wire.clone()).expect("parse");
        assert!(state.agent_mode);
        assert_eq!(state.thread_id.as_deref(), Some("thr_agent"));
        assert_eq!(round_trip(&state), wire);
    }

    #[test]
    fn updates_bootstrap_omits_after_seq() {
        let request: UpdatesRequest =
            serde_json::from_value(json!({"waitSeconds": 10})).expect("parse");
        assert_eq!(request.after_seq, None);
        assert_eq!(request.wait_seconds, Some(10));
        assert_eq!(round_trip(&request), json!({"waitSeconds": 10}));
    }

    #[test]
    fn updates_request_forwards_the_cursor() {
        let wire = json!({"afterSeq": 4, "waitSeconds": 5});
        let request: UpdatesRequest = serde_json::from_value(wire.clone()).expect("parse");
        assert_eq!(request.after_seq, Some(4));
        assert_eq!(round_trip(&request), wire);
    }

    #[test]
    fn updates_bootstrap_response_has_an_empty_item_array() {
        let wire = json!({"cursor": 12, "items": []});
        let response: UpdatesResponse = serde_json::from_value(wire.clone()).expect("parse");
        assert_eq!(response.cursor, Some(12));
        assert!(response.items.is_empty());
        assert_eq!(round_trip(&response), wire);
    }

    #[test]
    fn updates_response_carries_items_and_completed_at() {
        let wire = json!({
            "cursor": 14,
            "items": [{"seq": 13, "spoken": "Готово, ребёнок закончил.", "completedAt": 1730000000123i64}]
        });
        let response: UpdatesResponse = serde_json::from_value(wire.clone()).expect("parse");
        assert_eq!(response.cursor, Some(14));
        assert_eq!(
            response.items,
            vec![UpdateItem {
                seq: 13,
                spoken: "Готово, ребёнок закончил.".to_owned(),
                completed_at: Some(1_730_000_000_123),
            }]
        );
        assert_eq!(round_trip(&response), wire);
    }

    #[test]
    fn updates_failure_keeps_status_and_speech() {
        let wire = json!({
            "items": [],
            "status": "failed",
            "speech": "Сервер агентов недоступен."
        });
        let response: UpdatesResponse = serde_json::from_value(wire.clone()).expect("parse");
        assert_eq!(response.status.as_deref(), Some("failed"));
        assert_eq!(round_trip(&response), wire);
    }

    #[test]
    fn builders_match_the_wire_shape() {
        assert_eq!(
            serde_json::to_value(UtteranceResponse::bb("thr_work", 42, true, "Передал."))
                .expect("serialize"),
            json!({
                "route": "bb",
                "ok": true,
                "threadId": "thr_work",
                "afterSeq": 42,
                "speech": "Передал."
            })
        );
        assert_eq!(
            serde_json::to_value(UtteranceResponse::local("Готово.")).expect("serialize"),
            json!({"route": "local", "speech": "Готово."})
        );
        assert_eq!(
            serde_json::to_value(UtteranceResponse::hermes()).expect("serialize"),
            json!({"route": "hermes"})
        );
        assert_eq!(
            serde_json::to_value(UtteranceResponse::unknown("mcp")).expect("serialize"),
            json!({"route": "mcp"})
        );
    }

    #[test]
    fn speech_budget_matches_the_go_server() {
        assert_eq!(MAX_TEXT_RUNES, 2000);
    }
}
