//! `home.v1.IntercomService` implementation.
//!
//! Handlers only translate wire messages and map domain errors; all state
//! lives in [`crate::domain::Intercom`]. SDP bodies are passed through and
//! never logged — only their direction and size are observable.

use std::sync::Arc;

use connectrpc::{
    ConnectError, RequestContext, Response, ServiceRequest, ServiceResult, ServiceStream,
};
use home_proto::v1 as proto;
use proto::IntercomService;
use tokio::sync::broadcast;

use crate::domain::{
    Call, CallState, EndReason, Endpoint, EndpointKind, ErrorKind, IceCandidate, Identity,
    SdpSignal, SdpType, Signal,
};

/// Shared intercom API state.
#[derive(Clone)]
pub struct Api {
    inner: Arc<crate::domain::Intercom>,
    version: String,
}

impl std::fmt::Debug for Api {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Api")
            .field("version", &self.version)
            .finish_non_exhaustive()
    }
}

impl Api {
    pub fn new(inner: Arc<crate::domain::Intercom>, version: impl Into<String>) -> Self {
        Self {
            inner,
            version: version.into(),
        }
    }

    pub fn intercom(&self) -> &Arc<crate::domain::Intercom> {
        &self.inner
    }

    fn identity(ctx: &RequestContext) -> Result<Identity, ConnectError> {
        ctx.extensions()
            .get::<Identity>()
            .cloned()
            .ok_or_else(|| ConnectError::unauthenticated("unauthenticated"))
    }

    fn map_error(&self, error: crate::domain::DomainError) -> ConnectError {
        self.inner.note_rejected();
        map_error(error)
    }
}

fn map_error(error: crate::domain::DomainError) -> ConnectError {
    let text = super::text::error_text(error.kind, error.message);
    let mapped = match error.kind {
        ErrorKind::InvalidArgument => ConnectError::invalid_argument(error.message),
        ErrorKind::NotFound => ConnectError::not_found(error.message),
        ErrorKind::AlreadyExists => ConnectError::already_exists(error.message),
        ErrorKind::FailedPrecondition => ConnectError::failed_precondition(error.message),
        ErrorKind::PermissionDenied => ConnectError::permission_denied(error.message),
        ErrorKind::Unavailable => ConnectError::unavailable(error.message),
    };
    mapped.with_detail(home_proto::error_detail(&text))
}

fn ok<T>(message: T) -> ServiceResult<T> {
    Response::ok(message)
}

impl IntercomService for Api {
    async fn register_endpoint<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, proto::RegisterEndpointRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<proto::RegisterEndpointResponse> + Send + use<'a>>
    {
        let identity = Self::identity(&ctx)?;
        let message = request.to_owned_message();
        let endpoint = message.endpoint.as_option();
        let display_name = endpoint.map_or("", |value| value.display_name.as_str());
        let capabilities: Vec<String> = endpoint
            .map(|value| value.capabilities.clone())
            .unwrap_or_default();
        let registered = self
            .inner
            .register(&identity, display_name, &capabilities)
            .map_err(|error| self.map_error(error))?;
        ok(proto::RegisterEndpointResponse {
            endpoint: Some(endpoint_to_proto(&registered)).into(),
            ..Default::default()
        })
    }

    async fn list_endpoints<'a>(
        &'a self,
        ctx: RequestContext,
        _request: ServiceRequest<'_, proto::ListEndpointsRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<proto::ListEndpointsResponse> + Send + use<'a>>
    {
        Self::identity(&ctx)?;
        let endpoints = self
            .inner
            .list_endpoints()
            .iter()
            .map(endpoint_to_proto)
            .collect();
        ok(proto::ListEndpointsResponse {
            endpoints,
            ..Default::default()
        })
    }

    async fn start_call<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, proto::StartCallRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<proto::StartCallResponse> + Send + use<'a>> {
        let identity = Self::identity(&ctx)?;
        let message = request.to_owned_message();
        let call = self
            .inner
            .start_call(
                &identity,
                &message.callee_endpoint_id,
                &message.idempotency_key,
            )
            .map_err(|error| self.map_error(error))?;
        ok(proto::StartCallResponse {
            call: Some(call_to_proto(&call)).into(),
            ..Default::default()
        })
    }

    async fn answer<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, proto::AnswerRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<proto::AnswerResponse> + Send + use<'a>> {
        let identity = Self::identity(&ctx)?;
        let message = request.to_owned_message();
        let call = self
            .inner
            .answer(&identity, &message.call_id)
            .map_err(|error| self.map_error(error))?;
        ok(proto::AnswerResponse {
            call: Some(call_to_proto(&call)).into(),
            ..Default::default()
        })
    }

    async fn hangup<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, proto::HangupRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<proto::HangupResponse> + Send + use<'a>> {
        let identity = Self::identity(&ctx)?;
        let message = request.to_owned_message();
        let reason = message
            .reason
            .as_known()
            .map_or(EndReason::Unspecified, end_reason_from_proto);
        let call = self
            .inner
            .hangup(&identity, &message.call_id, reason)
            .map_err(|error| self.map_error(error))?;
        ok(proto::HangupResponse {
            call: Some(call_to_proto(&call)).into(),
            ..Default::default()
        })
    }

    async fn get_call<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, proto::GetCallRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<proto::GetCallResponse> + Send + use<'a>> {
        let identity = Self::identity(&ctx)?;
        let message = request.to_owned_message();
        let call = self
            .inner
            .get_call(&identity, &message.call_id)
            .map_err(|error| self.map_error(error))?;
        ok(proto::GetCallResponse {
            call: Some(call_to_proto(&call)).into(),
            ..Default::default()
        })
    }

    async fn send_signal<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, proto::SendSignalRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<proto::SendSignalResponse> + Send + use<'a>> {
        let identity = Self::identity(&ctx)?;
        let message = request.to_owned_message();
        let Some(signal) = message.signal.as_ref() else {
            return Err(
                ConnectError::invalid_argument("signal is required").with_detail(
                    home_proto::error_detail(&home_proto::localized(
                        "intercom.error.signal_required",
                    )),
                ),
            );
        };
        let signal = match signal {
            proto::send_signal_request::Signal::Sdp(sdp) => Signal::Sdp(SdpSignal {
                kind: sdp
                    .r#type
                    .as_known()
                    .map_or(SdpType::Offer, sdp_type_from_proto),
                sdp: sdp.sdp.clone(),
            }),
            proto::send_signal_request::Signal::IceCandidate(candidate) => {
                Signal::IceCandidate(IceCandidate {
                    candidate: candidate.candidate.clone(),
                    sdp_mid: candidate.sdp_mid.clone(),
                    sdp_mline_index: candidate.sdp_mline_index,
                })
            }
            proto::send_signal_request::Signal::IceEnd(_) => Signal::IceEnd,
        };
        let delivered = self
            .inner
            .send_signal(&identity, &message.call_id, &signal)
            .map_err(|error| self.map_error(error))?;
        ok(proto::SendSignalResponse {
            delivered,
            ..Default::default()
        })
    }

    async fn watch(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, proto::WatchRequest>,
    ) -> ServiceResult<ServiceStream<impl connectrpc::Encodable<proto::WatchEvent> + Send + use<>>>
    {
        let identity = Self::identity(&ctx)?;
        let message = request.to_owned_message();
        if !message.endpoint_id.is_empty() && message.endpoint_id != identity.endpoint_id {
            return Err(
                ConnectError::permission_denied("endpoint_id does not match the token")
                    .with_detail(home_proto::error_detail(&home_proto::localized(
                        "intercom.error.endpoint_token_mismatch",
                    ))),
            );
        }
        let (receiver, guard) = self
            .inner
            .watch(&identity)
            .map_err(|error| self.map_error(error))?;
        let stream =
            futures_util::stream::unfold((receiver, guard), |(mut receiver, guard)| async move {
                match receiver.recv().await {
                    Ok(event) => Some((Ok(event_to_proto(event)), (receiver, guard))),
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!(skipped, "watch subscriber lagged");
                        Some((
                            Err(ConnectError::unavailable(
                                "watch stream lagged; reconnect and resynchronize",
                            )),
                            (receiver, guard),
                        ))
                    }
                    Err(broadcast::error::RecvError::Closed) => None,
                }
            });
        Response::stream_ok(stream)
    }

    async fn get_ice_config<'a>(
        &'a self,
        ctx: RequestContext,
        _request: ServiceRequest<'_, proto::GetIceConfigRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<proto::GetIceConfigResponse> + Send + use<'a>>
    {
        Self::identity(&ctx)?;
        let ice_servers = self
            .inner
            .ice_servers()
            .iter()
            .map(|server| proto::IceServer {
                urls: server.urls.clone(),
                username: server.username.clone(),
                credential: server.credential.expose().to_owned(),
                ..Default::default()
            })
            .collect();
        ok(proto::GetIceConfigResponse {
            ice_servers,
            ..Default::default()
        })
    }

    async fn get_status<'a>(
        &'a self,
        ctx: RequestContext,
        _request: ServiceRequest<'_, proto::GetIntercomStatusRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<proto::GetIntercomStatusResponse> + Send + use<'a>>
    {
        Self::identity(&ctx)?;
        let status = self.inner.status();
        ok(proto::GetIntercomStatusResponse {
            version: self.version.clone(),
            now_unix: self.inner.now_unix(),
            endpoints_registered: i32::try_from(status.endpoints_registered).unwrap_or(i32::MAX),
            endpoints_online: i32::try_from(status.endpoints_online).unwrap_or(i32::MAX),
            active_calls: i32::try_from(status.active_calls).unwrap_or(i32::MAX),
            ready: true,
            ring_timeout_seconds: self.inner.ring_timeout_seconds(),
            connect_timeout_seconds: self.inner.connect_timeout_seconds(),
            ..Default::default()
        })
    }
}

fn endpoint_to_proto(endpoint: &Endpoint) -> proto::Endpoint {
    proto::Endpoint {
        id: endpoint.id.clone(),
        kind: match endpoint.kind {
            EndpointKind::Phone => proto::EndpointKind::Phone,
            EndpointKind::Station => proto::EndpointKind::Station,
        }
        .into(),
        display_name: endpoint.display_name.clone(),
        capabilities: endpoint.capabilities.clone(),
        online: endpoint.online,
        last_seen_unix: endpoint.last_seen_unix,
        ..Default::default()
    }
}

fn call_to_proto(call: &Call) -> proto::Call {
    proto::Call {
        id: call.id.clone(),
        caller: Some(endpoint_to_proto(&call.caller)).into(),
        callee: Some(endpoint_to_proto(&call.callee)).into(),
        state: call_state_to_proto(call.state).into(),
        end_reason: end_reason_to_proto(call.end_reason).into(),
        created_unix: call.created_unix,
        updated_unix: call.updated_unix,
        answered_unix: call.answered_unix,
        ended_unix: call.ended_unix,
        ..Default::default()
    }
}

fn call_state_to_proto(state: CallState) -> proto::CallState {
    match state {
        CallState::Ringing => proto::CallState::Ringing,
        CallState::Connecting => proto::CallState::Connecting,
        CallState::Active => proto::CallState::Active,
        CallState::Ended => proto::CallState::Ended,
    }
}

fn end_reason_to_proto(reason: EndReason) -> proto::EndReason {
    match reason {
        EndReason::Unspecified => proto::EndReason::Unspecified,
        EndReason::Hangup => proto::EndReason::Hangup,
        EndReason::Declined => proto::EndReason::Declined,
        EndReason::Busy => proto::EndReason::Busy,
        EndReason::Canceled => proto::EndReason::Canceled,
        EndReason::Timeout => proto::EndReason::Timeout,
        EndReason::Unreachable => proto::EndReason::Unreachable,
        EndReason::Error => proto::EndReason::Error,
    }
}

fn end_reason_from_proto(reason: proto::EndReason) -> EndReason {
    match reason {
        proto::EndReason::Hangup => EndReason::Hangup,
        proto::EndReason::Declined => EndReason::Declined,
        proto::EndReason::Busy => EndReason::Busy,
        proto::EndReason::Canceled => EndReason::Canceled,
        proto::EndReason::Timeout => EndReason::Timeout,
        proto::EndReason::Unreachable => EndReason::Unreachable,
        proto::EndReason::Error => EndReason::Error,
        proto::EndReason::Unspecified => EndReason::Unspecified,
    }
}

fn sdp_type_from_proto(kind: proto::SdpType) -> SdpType {
    match kind {
        proto::SdpType::Answer => SdpType::Answer,
        proto::SdpType::Offer | proto::SdpType::Unspecified => SdpType::Offer,
    }
}

fn sdp_type_to_proto(kind: SdpType) -> proto::SdpType {
    match kind {
        SdpType::Offer => proto::SdpType::Offer,
        SdpType::Answer => proto::SdpType::Answer,
    }
}

fn event_to_proto(event: crate::domain::Event) -> proto::WatchEvent {
    use proto::watch_event::Event;
    let event = match event {
        crate::domain::Event::Call {
            call,
            previous_state,
        } => Event::Call(
            proto::CallEvent {
                call: Some(call_to_proto(&call)).into(),
                previous_state: previous_state
                    .map_or(proto::CallState::Unspecified, call_state_to_proto)
                    .into(),
                ..Default::default()
            }
            .into(),
        ),
        crate::domain::Event::Sdp {
            call_id,
            from_endpoint_id,
            signal,
        } => Event::Sdp(
            proto::SdpEvent {
                call_id,
                from_endpoint_id,
                signal: Some(proto::SdpSignal {
                    r#type: sdp_type_to_proto(signal.kind).into(),
                    sdp: signal.sdp,
                    ..Default::default()
                })
                .into(),
                ..Default::default()
            }
            .into(),
        ),
        crate::domain::Event::IceCandidate {
            call_id,
            from_endpoint_id,
            candidate,
        } => Event::IceCandidate(
            proto::IceCandidateEvent {
                call_id,
                from_endpoint_id,
                candidate: Some(proto::IceCandidateSignal {
                    candidate: candidate.candidate,
                    sdp_mid: candidate.sdp_mid,
                    sdp_mline_index: candidate.sdp_mline_index,
                    ..Default::default()
                })
                .into(),
                ..Default::default()
            }
            .into(),
        ),
        crate::domain::Event::IceEnd {
            call_id,
            from_endpoint_id,
        } => Event::IceEnd(
            proto::IceEndEvent {
                call_id,
                from_endpoint_id,
                ..Default::default()
            }
            .into(),
        ),
    };
    proto::WatchEvent {
        event: Some(event),
        ..Default::default()
    }
}
