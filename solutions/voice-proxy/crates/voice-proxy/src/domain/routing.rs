use voice_contract::{Route, UtteranceResponse};

use super::profile::Profile;
use super::sanitize::{ensure_not_question, trim_space};

pub const SPEECH_PROFILE_UNKNOWN: &str = "Не могу подтвердить профиль.";
pub const SPEECH_AGENTS_DOWN: &str = "Сервер агентов недоступен.";

#[derive(Debug, Clone)]
pub enum GatewayAnswer {
    Ok(UtteranceResponse),
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Announcement {
    pub thread_id: String,
    pub after_seq: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    pub handled: bool,
    pub speech: Option<String>,
    pub announcement: Option<Announcement>,
    pub outcome: &'static str,
    pub profile: Option<Profile>,
}

impl Decision {
    fn local(profile: Profile, speech: String, outcome: &'static str) -> Self {
        Self {
            handled: true,
            speech: Some(speech),
            announcement: None,
            outcome,
            profile: Some(profile),
        }
    }

    fn forward(profile: Option<Profile>, outcome: &'static str) -> Self {
        Self {
            handled: false,
            speech: None,
            announcement: None,
            outcome,
            profile,
        }
    }
}

#[must_use]
pub fn decide(profile: Option<Profile>, gateway: GatewayAnswer) -> Decision {
    let Some(profile) = profile else {
        return Decision {
            handled: true,
            speech: Some(SPEECH_PROFILE_UNKNOWN.to_owned()),
            announcement: None,
            outcome: "profile_unknown",
            profile: None,
        };
    };
    let response = match gateway {
        GatewayAnswer::Failed => {
            return match profile {
                Profile::Personal => Decision::forward(Some(profile), "gateway_down_personal"),
                Profile::Work => {
                    Decision::local(profile, SPEECH_AGENTS_DOWN.to_owned(), "gateway_down")
                }
            };
        }
        GatewayAnswer::Ok(response) => response,
    };

    if response.route == Route::Hermes {
        return Decision::forward(Some(profile), "hermes");
    }
    let raw_speech = response.speech.unwrap_or_default();
    let speech = if trim_space(&raw_speech).is_empty() {
        SPEECH_AGENTS_DOWN.to_owned()
    } else {
        raw_speech
    };

    if matches!(response.route, Route::Bb | Route::Local) {
        if response.ok == Some(false) {
            return Decision::local(
                profile,
                ensure_not_question(&speech, SPEECH_AGENTS_DOWN),
                "gateway_failed",
            );
        }
        if response.route == Route::Bb {
            if let (Some(thread_id), Some(after_seq)) = (response.thread_id, response.after_seq)
                && !trim_space(&thread_id).is_empty()
            {
                let mut decision = Decision::local(
                    profile,
                    ensure_not_question(&speech, SPEECH_AGENTS_DOWN),
                    "bb",
                );
                decision.announcement = Some(Announcement {
                    thread_id,
                    after_seq,
                });
                return decision;
            }
        } else {
            return Decision::local(
                profile,
                ensure_not_question(&speech, SPEECH_AGENTS_DOWN),
                "local",
            );
        }
        return Decision::local(
            profile,
            ensure_not_question(&speech, SPEECH_AGENTS_DOWN),
            "bb",
        );
    }

    Decision::local(profile, SPEECH_AGENTS_DOWN.to_owned(), "gateway_failed")
}

#[cfg(test)]
mod tests {
    use voice_contract::UtteranceResponse;

    use super::{Decision, GatewayAnswer, SPEECH_AGENTS_DOWN, SPEECH_PROFILE_UNKNOWN, decide};
    use crate::domain::profile::Profile;

    fn ok(response: UtteranceResponse) -> GatewayAnswer {
        GatewayAnswer::Ok(response)
    }

    #[test]
    fn unknown_profile_is_fail_closed() {
        let decision = decide(None, ok(UtteranceResponse::hermes()));
        assert!(decision.handled);
        assert_eq!(decision.speech.as_deref(), Some(SPEECH_PROFILE_UNKNOWN));
        assert_eq!(decision.outcome, "profile_unknown");
        assert_eq!(decision.profile, None);
    }

    #[test]
    fn gateway_failure_keeps_work_local_but_forwards_personal() {
        let work = decide(Some(Profile::Work), GatewayAnswer::Failed);
        assert!(work.handled);
        assert_eq!(work.speech.as_deref(), Some(SPEECH_AGENTS_DOWN));
        assert_eq!(work.outcome, "gateway_down");

        let personal = decide(Some(Profile::Personal), GatewayAnswer::Failed);
        assert!(!personal.handled);
        assert_eq!(personal.outcome, "gateway_down_personal");
        assert_eq!(personal.profile, Some(Profile::Personal));
    }

    #[test]
    fn hermes_route_is_forwarded_unchanged() {
        let decision = decide(Some(Profile::Work), ok(UtteranceResponse::hermes()));
        assert_eq!(decision, Decision::forward(Some(Profile::Work), "hermes"));
    }

    #[test]
    fn bb_route_is_spoken_and_announced_later() {
        let decision = decide(
            Some(Profile::Work),
            ok(UtteranceResponse::bb("thr_work", 42, true, "Передал.")),
        );
        assert!(decision.handled);
        assert_eq!(decision.speech.as_deref(), Some("Передал."));
        assert_eq!(decision.outcome, "bb");
        let announcement = decision.announcement.expect("announcement");
        assert_eq!(announcement.thread_id, "thr_work");
        assert_eq!(announcement.after_seq, 42);
    }

    #[test]
    fn bb_route_without_thread_or_seq_is_still_spoken_locally() {
        let mut response = UtteranceResponse::bb("", 0, true, "Передал.");
        response.thread_id = None;
        let decision = decide(Some(Profile::Work), ok(response));
        assert!(decision.handled);
        assert!(decision.announcement.is_none());
        assert_eq!(decision.outcome, "bb");
    }

    #[test]
    fn local_route_speaks_the_gateway_phrase_without_a_question() {
        let decision = decide(
            Some(Profile::Work),
            ok(UtteranceResponse::local("Выключил режим агента.")),
        );
        assert!(decision.handled);
        assert_eq!(decision.speech.as_deref(), Some("Выключил режим агента."));
        assert_eq!(decision.outcome, "local");

        let question = decide(Some(Profile::Work), ok(UtteranceResponse::local("Готово?")));
        assert_eq!(question.speech.as_deref(), Some("Готово."));
    }

    #[test]
    fn ok_false_is_never_forwarded() {
        let decision = decide(
            Some(Profile::Personal),
            ok(UtteranceResponse::bb(
                "thr_work",
                1,
                false,
                "Сервер агентов недоступен.",
            )),
        );
        assert!(decision.handled);
        assert_eq!(decision.outcome, "gateway_failed");
        assert!(decision.announcement.is_none());
    }

    #[test]
    fn unknown_route_answers_agents_down() {
        let decision = decide(Some(Profile::Work), ok(UtteranceResponse::unknown("mcp")));
        assert!(decision.handled);
        assert_eq!(decision.speech.as_deref(), Some(SPEECH_AGENTS_DOWN));
        assert_eq!(decision.outcome, "gateway_failed");
    }
}
