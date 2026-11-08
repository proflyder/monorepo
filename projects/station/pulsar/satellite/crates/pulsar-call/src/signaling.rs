//! Клиент сигналинга intercom: Connect JSON поверх HTTP/1.1 (`home-proto`).
//!
//! `Watch` — server-streaming: одно долгоживущее соединение на endpoint, сервер присылает
//! снимок звонка на каждое изменение. Исходящие действия (Answer/Hangup/SendSignal) —
//! отдельные unary-вызовы. SDP и ICE-кандидаты передаются как есть и не логируются: в логах
//! только имена методов и коды ошибок.

use connectrpc::client::ClientConfig;
use connectrpc::{CodecFormat, ConnectError, Protocol};
use home_proto::ReqwestTransport;
use home_proto::v1::{
    AnswerRequest, CallState, GetIceConfigResponse, HangupRequest, IntercomServiceClient,
    RegisterEndpointRequest, SdpSignal, SdpType, SendSignalRequest, WatchEvent, WatchRequest,
    send_signal_request, watch_event,
};
use http::Uri;

use crate::CallConfig;
use crate::machine::{HangupReason, SignalEvent};

/// Ошибка сигналинга. Не содержит ни токена, ни SDP, ни кандидатов.
#[derive(Debug, thiserror::Error)]
pub enum SignalError {
    #[error("call: invalid intercom URL")]
    InvalidUrl,
    #[error("call: cannot build the HTTP client")]
    Transport,
    #[error("call: signaling failed with {0}")]
    Rpc(String),
    #[error("call: signaling stream ended")]
    Closed,
}

fn map_err(error: &ConnectError) -> SignalError {
    SignalError::Rpc(format!("{:?}", error.code))
}

/// Клиент одного endpoint. `Watch` открывается один раз и переоткрывается при обрыве.
pub struct Signaling {
    client: IntercomServiceClient<ReqwestTransport>,
}

impl std::fmt::Debug for Signaling {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Signaling")
    }
}

impl Signaling {
    pub fn new(config: &CallConfig) -> Result<Self, SignalError> {
        let uri: Uri = config
            .endpoint()
            .parse()
            .map_err(|_| SignalError::InvalidUrl)?;
        if uri.scheme_str().is_none() || uri.authority().is_none() {
            return Err(SignalError::InvalidUrl);
        }
        let transport = match config.ca_pem() {
            Some(pem) => ReqwestTransport::with_ca_pem(pem).map_err(|_| SignalError::Transport)?,
            None => ReqwestTransport::plaintext().map_err(|_| SignalError::Transport)?,
        };
        let client_config = ClientConfig::new(uri)
            .with_protocol(Protocol::Connect)
            .with_codec_format(CodecFormat::Json)
            .with_default_header(
                "authorization",
                format!("Bearer {}", config.token().expose()),
            );
        Ok(Self {
            client: IntercomServiceClient::new(transport, client_config),
        })
    }

    /// Зарегистрировать/обновить описание endpoint. Необязательно: `Watch` уже помечает
    /// endpoint online, но имя и capabilities видит телефон.
    pub async fn register(&self, display_name: &str) -> Result<(), SignalError> {
        let request = RegisterEndpointRequest {
            endpoint: Some(home_proto::v1::Endpoint {
                display_name: display_name.to_owned(),
                capabilities: vec!["audio".to_owned()],
                ..Default::default()
            })
            .into(),
            ..Default::default()
        };
        self.client
            .register_endpoint(request)
            .await
            .map_err(|e| map_err(&e))?;
        Ok(())
    }

    /// Открыть поток событий.
    pub async fn watch(&self) -> Result<WatchStream, SignalError> {
        let stream = self
            .client
            .watch(WatchRequest::default())
            .await
            .map_err(|e| map_err(&e))?;
        Ok(WatchStream { inner: stream })
    }

    pub async fn answer(&self, call_id: &str) -> Result<(), SignalError> {
        self.client
            .answer(AnswerRequest {
                call_id: call_id.to_owned(),
                ..Default::default()
            })
            .await
            .map_err(|e| map_err(&e))?;
        Ok(())
    }

    pub async fn hangup(&self, call_id: &str, reason: HangupReason) -> Result<(), SignalError> {
        self.client
            .hangup(HangupRequest {
                call_id: call_id.to_owned(),
                reason: reason_code(reason).into(),
                ..Default::default()
            })
            .await
            .map_err(|e| map_err(&e))?;
        Ok(())
    }

    pub async fn send_sdp(&self, call_id: &str, sdp: String) -> Result<(), SignalError> {
        self.send(SendSignalRequest {
            call_id: call_id.to_owned(),
            signal: Some(send_signal_request::Signal::Sdp(Box::new(SdpSignal {
                r#type: SdpType::Answer.into(),
                sdp,
                ..Default::default()
            }))),
            ..Default::default()
        })
        .await
    }

    pub async fn send_ice_end(&self, call_id: &str) -> Result<(), SignalError> {
        self.send(SendSignalRequest {
            call_id: call_id.to_owned(),
            signal: Some(send_signal_request::Signal::IceEnd(Box::default())),
            ..Default::default()
        })
        .await
    }

    async fn send(&self, request: SendSignalRequest) -> Result<(), SignalError> {
        // `delivered == false` не ошибка: событие могло не дойти до телефона, а ICE всё равно
        // живёт по peer-reflexive кандидату.
        self.client
            .send_signal(request)
            .await
            .map_err(|e| map_err(&e))?;
        Ok(())
    }

    /// STUN/TURN серверы из конфигурации сервиса; пустой список — только host-кандидаты.
    pub async fn ice_config(&self) -> Result<GetIceConfigResponse, SignalError> {
        let response = self
            .client
            .get_ice_config(Default::default())
            .await
            .map_err(|e| map_err(&e))?;
        Ok(response.into_owned())
    }
}

/// Поток `Watch`: снимки и сигналы.
pub struct WatchStream {
    inner: connectrpc::client::ServerStream<
        reqwest::Body,
        home_proto::v1::__buffa::view::WatchEventView<'static>,
    >,
}

impl WatchStream {
    /// Следующее событие; `None` — поток закрыт сервером.
    pub async fn next(&mut self) -> Result<Option<WatchMessage>, SignalError> {
        let item = self
            .inner
            .message::<WatchEvent>()
            .await
            .map_err(|e| signal_map_err(&e))?;
        Ok(item.map(|m| decode(&m.to_owned_message())))
    }
}

fn signal_map_err(error: &ConnectError) -> SignalError {
    SignalError::Rpc(format!("{:?}", error.code))
}

/// Декодированное событие потока. SDP и кандидат — непрозрачные строки.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WatchMessage {
    Call(SignalEvent),
    Offer {
        call_id: String,
        sdp: String,
    },
    Ice {
        call_id: String,
        candidate: String,
        sdp_mid: String,
        index: i32,
    },
    IceEnd {
        call_id: String,
    },
    Other,
}

/// Перевод proto-события в домен. Ничего не логирует: SDP и кандидаты — приватные данные.
#[must_use]
pub fn decode(event: &WatchEvent) -> WatchMessage {
    match &event.event {
        Some(watch_event::Event::Call(call)) => {
            let Some(snapshot) = call.call.as_option() else {
                return WatchMessage::Other;
            };
            let call_id = snapshot.id.clone();
            match snapshot.state.as_known() {
                Some(CallState::Ringing) => WatchMessage::Call(SignalEvent::Ringing {
                    call_id,
                    caller: snapshot
                        .caller
                        .as_option()
                        .map_or_else(String::new, |e| e.display_name.clone()),
                }),
                Some(CallState::Connecting) => {
                    WatchMessage::Call(SignalEvent::Connecting { call_id })
                }
                Some(CallState::Active) => WatchMessage::Call(SignalEvent::Active { call_id }),
                Some(CallState::Ended) => WatchMessage::Call(SignalEvent::Ended { call_id }),
                _ => WatchMessage::Other,
            }
        }
        Some(watch_event::Event::Sdp(sdp)) => {
            if sdp.signal.r#type != SdpType::Offer {
                // Ответ колонке не присылают: она callee.
                return WatchMessage::Other;
            }
            WatchMessage::Offer {
                call_id: sdp.call_id.clone(),
                sdp: sdp.signal.sdp.clone(),
            }
        }
        Some(watch_event::Event::IceCandidate(ice)) => WatchMessage::Ice {
            call_id: ice.call_id.clone(),
            candidate: ice.candidate.candidate.clone(),
            sdp_mid: ice.candidate.sdp_mid.clone(),
            index: ice.candidate.sdp_mline_index,
        },
        Some(watch_event::Event::IceEnd(end)) => WatchMessage::IceEnd {
            call_id: end.call_id.clone(),
        },
        None => WatchMessage::Other,
    }
}

fn reason_code(reason: HangupReason) -> home_proto::v1::EndReason {
    use home_proto::v1::EndReason;
    match reason {
        HangupReason::Unspecified => EndReason::Unspecified,
        HangupReason::Hangup => EndReason::Hangup,
        HangupReason::Declined => EndReason::Declined,
        HangupReason::Busy => EndReason::Busy,
        HangupReason::Canceled => EndReason::Canceled,
        HangupReason::Timeout => EndReason::Timeout,
        HangupReason::Unreachable => EndReason::Unreachable,
        HangupReason::Error => EndReason::Error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use home_proto::v1::{
        Call, CallEvent as ProtoCallEvent, Endpoint, IceCandidateEvent, IceCandidateSignal,
        IceEndEvent, SdpEvent,
    };

    fn call_event(state: CallState) -> WatchEvent {
        WatchEvent {
            event: Some(watch_event::Event::Call(Box::new(ProtoCallEvent {
                call: Some(Call {
                    id: "c1".into(),
                    caller: Some(Endpoint {
                        id: "phone".into(),
                        display_name: "Телефон".into(),
                        ..Default::default()
                    })
                    .into(),
                    state: state.into(),
                    ..Default::default()
                })
                .into(),
                ..Default::default()
            }))),
            ..Default::default()
        }
    }

    #[test]
    fn ringing_carries_the_caller_and_the_call_id() {
        assert_eq!(
            decode(&call_event(CallState::Ringing)),
            WatchMessage::Call(SignalEvent::Ringing {
                call_id: "c1".into(),
                caller: "Телефон".into()
            })
        );
    }

    #[test]
    fn active_and_ended_map_to_domain_signal_events() {
        assert_eq!(
            decode(&call_event(CallState::Active)),
            WatchMessage::Call(SignalEvent::Active {
                call_id: "c1".into()
            })
        );
        assert_eq!(
            decode(&call_event(CallState::Ended)),
            WatchMessage::Call(SignalEvent::Ended {
                call_id: "c1".into()
            })
        );
    }

    #[test]
    fn an_offer_is_passed_through() {
        let event = WatchEvent {
            event: Some(watch_event::Event::Sdp(Box::new(SdpEvent {
                call_id: "c1".into(),
                from_endpoint_id: "phone".into(),
                signal: Some(SdpSignal {
                    r#type: SdpType::Offer.into(),
                    sdp: "v=0\r\n".into(),
                    ..Default::default()
                })
                .into(),
                ..Default::default()
            }))),
            ..Default::default()
        };
        assert_eq!(
            decode(&event),
            WatchMessage::Offer {
                call_id: "c1".into(),
                sdp: "v=0\r\n".into()
            }
        );
    }

    #[test]
    fn an_answer_from_the_peer_is_not_our_business() {
        let event = WatchEvent {
            event: Some(watch_event::Event::Sdp(Box::new(SdpEvent {
                call_id: "c1".into(),
                signal: Some(SdpSignal {
                    r#type: SdpType::Answer.into(),
                    sdp: "v=0\r\n".into(),
                    ..Default::default()
                })
                .into(),
                ..Default::default()
            }))),
            ..Default::default()
        };
        assert_eq!(decode(&event), WatchMessage::Other);
    }

    #[test]
    fn ice_candidates_and_end_keep_their_call_id() {
        let candidate = WatchEvent {
            event: Some(watch_event::Event::IceCandidate(Box::new(
                IceCandidateEvent {
                    call_id: "c1".into(),
                    candidate: Some(IceCandidateSignal {
                        candidate: "candidate:1 1 udp 1 10.0.0.1 5000 typ host".into(),
                        sdp_mid: "0".into(),
                        sdp_mline_index: 0,
                        ..Default::default()
                    })
                    .into(),
                    ..Default::default()
                },
            ))),
            ..Default::default()
        };
        assert!(matches!(decode(&candidate), WatchMessage::Ice { .. }));
        let end = WatchEvent {
            event: Some(watch_event::Event::IceEnd(Box::new(IceEndEvent {
                call_id: "c1".into(),
                ..Default::default()
            }))),
            ..Default::default()
        };
        assert_eq!(
            decode(&end),
            WatchMessage::IceEnd {
                call_id: "c1".into()
            }
        );
    }

    #[test]
    fn hangup_reason_codes_match_the_contract() {
        assert_eq!(reason_code(HangupReason::Busy) as i32, 3);
        assert_eq!(reason_code(HangupReason::Unreachable) as i32, 6);
    }
}
