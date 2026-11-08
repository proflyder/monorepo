//! End-to-end contract test over the real Connect HTTP/1.1 listener.
//!
//! This is the regression that pins the transport decision: unary calls and
//! the server-streaming `Watch` RPC work through the same client the Android
//! app uses, without HTTP/2 or bidi streams.

use std::sync::Arc;
use std::time::Duration;

use connectrpc::client::{CallOptions, ClientConfig};
use home_proto::v1 as proto;
use home_proto::{ReqwestTransport, v1::IntercomServiceClient};
use intercom::api::{Api, TokenAuth, router};
use intercom::config::EndpointConfig;
use intercom::domain::{Intercom, SystemClock, Timeouts};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

const PHONE: &str = "phone-token";
const STATION: &str = "station-token";

struct Server {
    base: String,
    _cancel: CancellationToken,
}

async fn spawn() -> Server {
    let phone = EndpointConfig {
        id: "phone".into(),
        display_name: "Phone".into(),
        capabilities: vec!["audio".into()],
    };
    let station = EndpointConfig {
        id: "station".into(),
        display_name: "Station".into(),
        capabilities: vec!["audio".into()],
    };
    let intercom = Arc::new(Intercom::new(
        &phone,
        &station,
        Timeouts {
            ring: Duration::from_secs(30),
            connect: Duration::from_secs(20),
            max_call: Duration::from_secs(3600),
            ended_retention: Duration::from_secs(60),
        },
        Vec::new(),
        Arc::new(SystemClock),
    ));
    let auth = TokenAuth::new(
        home_secret::Token::new(PHONE),
        &phone.id,
        home_secret::Token::new(STATION),
        &station.id,
    );
    let api = Api::new(intercom, "test");
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let address = listener.local_addr().expect("addr");
    let cancel = CancellationToken::new();
    let serve_cancel = cancel.clone();
    tokio::spawn(async move {
        home_http::serve(listener, router(api, auth), serve_cancel)
            .await
            .expect("serve");
    });
    Server {
        base: format!("http://{address}"),
        _cancel: cancel,
    }
}

fn client(server: &Server) -> IntercomServiceClient<ReqwestTransport> {
    let transport = ReqwestTransport::plaintext().expect("transport");
    let config = ClientConfig::new(server.base.parse().expect("uri"));
    IntercomServiceClient::new(transport, config)
}

fn options(token: &str) -> CallOptions {
    CallOptions::default().with_header(http::header::AUTHORIZATION, format!("Bearer {token}"))
}

#[tokio::test]
async fn unauthenticated_requests_are_rejected() {
    let server = spawn().await;
    let client = client(&server);
    let error = client
        .start_call(proto::StartCallRequest {
            callee_endpoint_id: "station".into(),
            ..Default::default()
        })
        .await
        .expect_err("no bearer");
    assert_eq!(error.code, connectrpc::ErrorCode::Unauthenticated);

    let error = client
        .start_call_with_options(
            proto::StartCallRequest {
                callee_endpoint_id: "station".into(),
                ..Default::default()
            },
            options("wrong"),
        )
        .await
        .expect_err("wrong bearer");
    assert_eq!(error.code, connectrpc::ErrorCode::Unauthenticated);
}

#[tokio::test]
async fn registration_listing_and_ice_config() {
    let server = spawn().await;
    let client = client(&server);

    let registered = client
        .register_endpoint_with_options(
            proto::RegisterEndpointRequest {
                endpoint: Some(proto::Endpoint {
                    // Server-substituted from the token.
                    id: "spoofed".into(),
                    display_name: "My phone".into(),
                    ..Default::default()
                })
                .into(),
                ..Default::default()
            },
            options(PHONE),
        )
        .await
        .expect("register")
        .into_owned();
    let endpoint = registered.endpoint.as_option().expect("endpoint");
    assert_eq!(endpoint.id, "phone");
    assert_eq!(endpoint.kind, proto::EndpointKind::Phone);
    assert_eq!(endpoint.display_name, "My phone");

    let listed = client
        .list_endpoints_with_options(proto::ListEndpointsRequest::default(), options(STATION))
        .await
        .expect("list")
        .into_owned();
    assert_eq!(listed.endpoints.len(), 2);

    let ice = client
        .get_ice_config_with_options(proto::GetIceConfigRequest::default(), options(PHONE))
        .await
        .expect("ice")
        .into_owned();
    assert!(ice.ice_servers.is_empty(), "host candidates only");
}

#[tokio::test]
async fn watch_events_reach_both_endpoints_and_streaming_works() {
    let server = spawn().await;
    let client = client(&server);

    let mut station_watch = client
        .watch_with_options(
            proto::WatchRequest {
                endpoint_id: "station".into(),
                ..Default::default()
            },
            options(STATION),
        )
        .await
        .expect("station watch");
    let mut phone_watch = client
        .watch_with_options(
            proto::WatchRequest {
                endpoint_id: "phone".into(),
                ..Default::default()
            },
            options(PHONE),
        )
        .await
        .expect("phone watch");

    let started = client
        .start_call_with_options(
            proto::StartCallRequest {
                callee_endpoint_id: "station".into(),
                idempotency_key: "call-1".into(),
                ..Default::default()
            },
            options(PHONE),
        )
        .await
        .expect("start")
        .into_owned();
    let call = started.call.as_option().expect("call").clone();
    assert_eq!(call.state, proto::CallState::Ringing);

    // Both Watch streams receive the incoming call over HTTP/1.1.
    let station_event = station_watch
        .message::<proto::WatchEvent>()
        .await
        .expect("station stream")
        .expect("station event")
        .to_owned_message();
    let phone_event = phone_watch
        .message::<proto::WatchEvent>()
        .await
        .expect("phone stream")
        .expect("phone event")
        .to_owned_message();
    for event in [station_event, phone_event] {
        match event.event.expect("event") {
            proto::watch_event::Event::Call(call_event) => {
                let event_call = call_event.call.as_option().expect("call");
                assert_eq!(event_call.id, call.id);
                assert_eq!(event_call.state, proto::CallState::Ringing);
            }
            other => panic!("expected a call event, got {other:?}"),
        }
    }

    // Answer from the station and exchange SDP: offer from caller, answer from
    // callee; the server promotes the call to ACTIVE once both halves arrived.
    let answered = client
        .answer_with_options(
            proto::AnswerRequest {
                call_id: call.id.clone(),
                ..Default::default()
            },
            options(STATION),
        )
        .await
        .expect("answer")
        .into_owned();
    assert_eq!(
        answered.call.as_option().expect("call").state,
        proto::CallState::Connecting
    );
    // Both Watch streams see the state change before the SDP exchange.
    for watch in [&mut station_watch, &mut phone_watch] {
        let event = watch
            .message::<proto::WatchEvent>()
            .await
            .expect("stream")
            .expect("connecting event")
            .to_owned_message();
        match event.event.expect("event") {
            proto::watch_event::Event::Call(call_event) => assert_eq!(
                call_event.call.as_option().expect("call").state,
                proto::CallState::Connecting
            ),
            other => panic!("expected a call event, got {other:?}"),
        }
    }

    client
        .send_signal_with_options(
            proto::SendSignalRequest {
                call_id: call.id.clone(),
                signal: Some(
                    proto::SdpSignal {
                        r#type: proto::SdpType::Offer.into(),
                        sdp: "v=0\r\n".into(),
                        ..Default::default()
                    }
                    .into(),
                ),
                ..Default::default()
            },
            options(PHONE),
        )
        .await
        .expect("offer");

    let offer = station_watch
        .message::<proto::WatchEvent>()
        .await
        .expect("stream")
        .expect("offer event")
        .to_owned_message();
    match offer.event.expect("event") {
        proto::watch_event::Event::Sdp(sdp) => {
            assert_eq!(sdp.from_endpoint_id, "phone");
            assert_eq!(sdp.signal.as_option().expect("signal").sdp, "v=0\r\n");
        }
        other => panic!("expected SDP, got {other:?}"),
    }

    client
        .send_signal_with_options(
            proto::SendSignalRequest {
                call_id: call.id.clone(),
                signal: Some(
                    proto::SdpSignal {
                        r#type: proto::SdpType::Answer.into(),
                        sdp: "v=0\r\nanswer\r\n".into(),
                        ..Default::default()
                    }
                    .into(),
                ),
                ..Default::default()
            },
            options(STATION),
        )
        .await
        .expect("answer sdp");

    // The phone receives the answer and then the ACTIVE state change.
    let answer = phone_watch
        .message::<proto::WatchEvent>()
        .await
        .expect("stream")
        .expect("answer event")
        .to_owned_message();
    assert!(matches!(
        answer.event.expect("event"),
        proto::watch_event::Event::Sdp(_)
    ));
    let active = phone_watch
        .message::<proto::WatchEvent>()
        .await
        .expect("stream")
        .expect("active event")
        .to_owned_message();
    match active.event.expect("event") {
        proto::watch_event::Event::Call(call_event) => {
            assert_eq!(
                call_event.call.as_option().expect("call").state,
                proto::CallState::Active
            );
        }
        other => panic!("expected a call event, got {other:?}"),
    }
}

#[tokio::test]
async fn watch_rejects_a_foreign_endpoint_id() {
    let server = spawn().await;
    let client = client(&server);
    let mut stream = client
        .watch_with_options(
            proto::WatchRequest {
                endpoint_id: "phone".into(),
                ..Default::default()
            },
            options(STATION),
        )
        .await
        .expect("stream headers");
    // A server-streaming handler error arrives in the stream envelope, not at
    // header time.
    let error = stream
        .message::<proto::WatchEvent>()
        .await
        .expect_err("foreign endpoint");
    assert_eq!(error.code, connectrpc::ErrorCode::PermissionDenied);
}
