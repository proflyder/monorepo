use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub const PROFILE_PERSONAL: &str = "personal";
pub const PROFILE_WORK: &str = "work";

pub const MAX_TEXT_RUNES: usize = 2000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    Hermes,
    Bb,
    Local,
    Unknown(String),
}

impl Route {
    #[must_use]
    pub fn parse(raw: &str) -> Self {
        match raw {
            "hermes" => Self::Hermes,
            "bb" => Self::Bb,
            "local" => Self::Local,
            other => Self::Unknown(other.to_owned()),
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Hermes => "hermes",
            Self::Bb => "bb",
            Self::Local => "local",
            Self::Unknown(name) => name,
        }
    }

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UtteranceRequest {
    pub text: String,
    pub profile: String,
}

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
    #[must_use]
    pub fn hermes() -> Self {
        Self::route(Route::Hermes)
    }

    #[must_use]
    pub fn local(speech: impl Into<String>) -> Self {
        Self {
            speech: Some(speech.into()),
            ..Self::route(Route::Local)
        }
    }

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ResultRequest {
    pub thread_id: String,
    #[serde(default)]
    pub after_seq: i64,
    #[serde(default)]
    pub wait_seconds: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultResponse {
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speech: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_seq: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StateResponse {
    pub agent_mode: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct UpdatesRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_seq: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait_seconds: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateItem {
    pub seq: i64,
    pub spoken: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<i64>,
}

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
