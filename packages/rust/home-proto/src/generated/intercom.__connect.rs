///Shorthand for `OwnedView<RegisterEndpointRequestView<'static>>`.
pub type OwnedRegisterEndpointRequestView = ::buffa::view::OwnedView<
    __buffa::view::RegisterEndpointRequestView<'static>,
>;
///Shorthand for `OwnedView<RegisterEndpointResponseView<'static>>`.
pub type OwnedRegisterEndpointResponseView = ::buffa::view::OwnedView<
    __buffa::view::RegisterEndpointResponseView<'static>,
>;
///Shorthand for `OwnedView<ListEndpointsRequestView<'static>>`.
pub type OwnedListEndpointsRequestView = ::buffa::view::OwnedView<
    __buffa::view::ListEndpointsRequestView<'static>,
>;
///Shorthand for `OwnedView<ListEndpointsResponseView<'static>>`.
pub type OwnedListEndpointsResponseView = ::buffa::view::OwnedView<
    __buffa::view::ListEndpointsResponseView<'static>,
>;
///Shorthand for `OwnedView<StartCallRequestView<'static>>`.
pub type OwnedStartCallRequestView = ::buffa::view::OwnedView<
    __buffa::view::StartCallRequestView<'static>,
>;
///Shorthand for `OwnedView<StartCallResponseView<'static>>`.
pub type OwnedStartCallResponseView = ::buffa::view::OwnedView<
    __buffa::view::StartCallResponseView<'static>,
>;
///Shorthand for `OwnedView<AnswerRequestView<'static>>`.
pub type OwnedAnswerRequestView = ::buffa::view::OwnedView<
    __buffa::view::AnswerRequestView<'static>,
>;
///Shorthand for `OwnedView<AnswerResponseView<'static>>`.
pub type OwnedAnswerResponseView = ::buffa::view::OwnedView<
    __buffa::view::AnswerResponseView<'static>,
>;
///Shorthand for `OwnedView<HangupRequestView<'static>>`.
pub type OwnedHangupRequestView = ::buffa::view::OwnedView<
    __buffa::view::HangupRequestView<'static>,
>;
///Shorthand for `OwnedView<HangupResponseView<'static>>`.
pub type OwnedHangupResponseView = ::buffa::view::OwnedView<
    __buffa::view::HangupResponseView<'static>,
>;
///Shorthand for `OwnedView<GetCallRequestView<'static>>`.
pub type OwnedGetCallRequestView = ::buffa::view::OwnedView<
    __buffa::view::GetCallRequestView<'static>,
>;
///Shorthand for `OwnedView<GetCallResponseView<'static>>`.
pub type OwnedGetCallResponseView = ::buffa::view::OwnedView<
    __buffa::view::GetCallResponseView<'static>,
>;
///Shorthand for `OwnedView<SendSignalRequestView<'static>>`.
pub type OwnedSendSignalRequestView = ::buffa::view::OwnedView<
    __buffa::view::SendSignalRequestView<'static>,
>;
///Shorthand for `OwnedView<SendSignalResponseView<'static>>`.
pub type OwnedSendSignalResponseView = ::buffa::view::OwnedView<
    __buffa::view::SendSignalResponseView<'static>,
>;
///Shorthand for `OwnedView<WatchRequestView<'static>>`.
pub type OwnedWatchRequestView = ::buffa::view::OwnedView<
    __buffa::view::WatchRequestView<'static>,
>;
///Shorthand for `OwnedView<WatchEventView<'static>>`.
pub type OwnedWatchEventView = ::buffa::view::OwnedView<
    __buffa::view::WatchEventView<'static>,
>;
///Shorthand for `OwnedView<GetIceConfigRequestView<'static>>`.
pub type OwnedGetIceConfigRequestView = ::buffa::view::OwnedView<
    __buffa::view::GetIceConfigRequestView<'static>,
>;
///Shorthand for `OwnedView<GetIceConfigResponseView<'static>>`.
pub type OwnedGetIceConfigResponseView = ::buffa::view::OwnedView<
    __buffa::view::GetIceConfigResponseView<'static>,
>;
///Shorthand for `OwnedView<GetIntercomStatusRequestView<'static>>`.
pub type OwnedGetIntercomStatusRequestView = ::buffa::view::OwnedView<
    __buffa::view::GetIntercomStatusRequestView<'static>,
>;
///Shorthand for `OwnedView<GetIntercomStatusResponseView<'static>>`.
pub type OwnedGetIntercomStatusResponseView = ::buffa::view::OwnedView<
    __buffa::view::GetIntercomStatusResponseView<'static>,
>;
impl ::connectrpc::Encodable<RegisterEndpointResponse>
for __buffa::view::RegisterEndpointResponseView<'_> {
    fn encode(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::buffa::bytes::Bytes, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body(self, codec)
    }
}
impl ::connectrpc::Encodable<RegisterEndpointResponse>
for ::buffa::view::OwnedView<__buffa::view::RegisterEndpointResponseView<'static>> {
    fn encode(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::buffa::bytes::Bytes, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body(self.reborrow(), codec)
    }
    /// An `OwnedView` still holds the buffer it was decoded from, so
    /// its large fields can be handed to the response body by
    /// reference count instead of copied. The bare view impl above
    /// cannot do this: it has borrows but no buffer to name.
    fn encode_segments(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::connectrpc::EncodedBody, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body_segments(
            self.reborrow(),
            self.bytes(),
            codec,
        )
    }
}
impl ::connectrpc::Encodable<ListEndpointsResponse>
for __buffa::view::ListEndpointsResponseView<'_> {
    fn encode(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::buffa::bytes::Bytes, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body(self, codec)
    }
}
impl ::connectrpc::Encodable<ListEndpointsResponse>
for ::buffa::view::OwnedView<__buffa::view::ListEndpointsResponseView<'static>> {
    fn encode(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::buffa::bytes::Bytes, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body(self.reborrow(), codec)
    }
    /// An `OwnedView` still holds the buffer it was decoded from, so
    /// its large fields can be handed to the response body by
    /// reference count instead of copied. The bare view impl above
    /// cannot do this: it has borrows but no buffer to name.
    fn encode_segments(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::connectrpc::EncodedBody, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body_segments(
            self.reborrow(),
            self.bytes(),
            codec,
        )
    }
}
impl ::connectrpc::Encodable<StartCallResponse>
for __buffa::view::StartCallResponseView<'_> {
    fn encode(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::buffa::bytes::Bytes, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body(self, codec)
    }
}
impl ::connectrpc::Encodable<StartCallResponse>
for ::buffa::view::OwnedView<__buffa::view::StartCallResponseView<'static>> {
    fn encode(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::buffa::bytes::Bytes, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body(self.reborrow(), codec)
    }
    /// An `OwnedView` still holds the buffer it was decoded from, so
    /// its large fields can be handed to the response body by
    /// reference count instead of copied. The bare view impl above
    /// cannot do this: it has borrows but no buffer to name.
    fn encode_segments(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::connectrpc::EncodedBody, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body_segments(
            self.reborrow(),
            self.bytes(),
            codec,
        )
    }
}
impl ::connectrpc::Encodable<AnswerResponse> for __buffa::view::AnswerResponseView<'_> {
    fn encode(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::buffa::bytes::Bytes, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body(self, codec)
    }
}
impl ::connectrpc::Encodable<AnswerResponse>
for ::buffa::view::OwnedView<__buffa::view::AnswerResponseView<'static>> {
    fn encode(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::buffa::bytes::Bytes, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body(self.reborrow(), codec)
    }
    /// An `OwnedView` still holds the buffer it was decoded from, so
    /// its large fields can be handed to the response body by
    /// reference count instead of copied. The bare view impl above
    /// cannot do this: it has borrows but no buffer to name.
    fn encode_segments(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::connectrpc::EncodedBody, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body_segments(
            self.reborrow(),
            self.bytes(),
            codec,
        )
    }
}
impl ::connectrpc::Encodable<HangupResponse> for __buffa::view::HangupResponseView<'_> {
    fn encode(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::buffa::bytes::Bytes, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body(self, codec)
    }
}
impl ::connectrpc::Encodable<HangupResponse>
for ::buffa::view::OwnedView<__buffa::view::HangupResponseView<'static>> {
    fn encode(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::buffa::bytes::Bytes, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body(self.reborrow(), codec)
    }
    /// An `OwnedView` still holds the buffer it was decoded from, so
    /// its large fields can be handed to the response body by
    /// reference count instead of copied. The bare view impl above
    /// cannot do this: it has borrows but no buffer to name.
    fn encode_segments(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::connectrpc::EncodedBody, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body_segments(
            self.reborrow(),
            self.bytes(),
            codec,
        )
    }
}
impl ::connectrpc::Encodable<GetCallResponse>
for __buffa::view::GetCallResponseView<'_> {
    fn encode(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::buffa::bytes::Bytes, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body(self, codec)
    }
}
impl ::connectrpc::Encodable<GetCallResponse>
for ::buffa::view::OwnedView<__buffa::view::GetCallResponseView<'static>> {
    fn encode(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::buffa::bytes::Bytes, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body(self.reborrow(), codec)
    }
    /// An `OwnedView` still holds the buffer it was decoded from, so
    /// its large fields can be handed to the response body by
    /// reference count instead of copied. The bare view impl above
    /// cannot do this: it has borrows but no buffer to name.
    fn encode_segments(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::connectrpc::EncodedBody, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body_segments(
            self.reborrow(),
            self.bytes(),
            codec,
        )
    }
}
impl ::connectrpc::Encodable<SendSignalResponse>
for __buffa::view::SendSignalResponseView<'_> {
    fn encode(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::buffa::bytes::Bytes, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body(self, codec)
    }
}
impl ::connectrpc::Encodable<SendSignalResponse>
for ::buffa::view::OwnedView<__buffa::view::SendSignalResponseView<'static>> {
    fn encode(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::buffa::bytes::Bytes, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body(self.reborrow(), codec)
    }
    /// An `OwnedView` still holds the buffer it was decoded from, so
    /// its large fields can be handed to the response body by
    /// reference count instead of copied. The bare view impl above
    /// cannot do this: it has borrows but no buffer to name.
    fn encode_segments(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::connectrpc::EncodedBody, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body_segments(
            self.reborrow(),
            self.bytes(),
            codec,
        )
    }
}
impl ::connectrpc::Encodable<WatchEvent> for __buffa::view::WatchEventView<'_> {
    fn encode(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::buffa::bytes::Bytes, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body(self, codec)
    }
}
impl ::connectrpc::Encodable<WatchEvent>
for ::buffa::view::OwnedView<__buffa::view::WatchEventView<'static>> {
    fn encode(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::buffa::bytes::Bytes, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body(self.reborrow(), codec)
    }
    /// An `OwnedView` still holds the buffer it was decoded from, so
    /// its large fields can be handed to the response body by
    /// reference count instead of copied. The bare view impl above
    /// cannot do this: it has borrows but no buffer to name.
    fn encode_segments(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::connectrpc::EncodedBody, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body_segments(
            self.reborrow(),
            self.bytes(),
            codec,
        )
    }
}
impl ::connectrpc::Encodable<GetIceConfigResponse>
for __buffa::view::GetIceConfigResponseView<'_> {
    fn encode(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::buffa::bytes::Bytes, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body(self, codec)
    }
}
impl ::connectrpc::Encodable<GetIceConfigResponse>
for ::buffa::view::OwnedView<__buffa::view::GetIceConfigResponseView<'static>> {
    fn encode(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::buffa::bytes::Bytes, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body(self.reborrow(), codec)
    }
    /// An `OwnedView` still holds the buffer it was decoded from, so
    /// its large fields can be handed to the response body by
    /// reference count instead of copied. The bare view impl above
    /// cannot do this: it has borrows but no buffer to name.
    fn encode_segments(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::connectrpc::EncodedBody, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body_segments(
            self.reborrow(),
            self.bytes(),
            codec,
        )
    }
}
impl ::connectrpc::Encodable<GetIntercomStatusResponse>
for __buffa::view::GetIntercomStatusResponseView<'_> {
    fn encode(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::buffa::bytes::Bytes, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body(self, codec)
    }
}
impl ::connectrpc::Encodable<GetIntercomStatusResponse>
for ::buffa::view::OwnedView<__buffa::view::GetIntercomStatusResponseView<'static>> {
    fn encode(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::buffa::bytes::Bytes, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body(self.reborrow(), codec)
    }
    /// An `OwnedView` still holds the buffer it was decoded from, so
    /// its large fields can be handed to the response body by
    /// reference count instead of copied. The bare view impl above
    /// cannot do this: it has borrows but no buffer to name.
    fn encode_segments(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::connectrpc::EncodedBody, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body_segments(
            self.reborrow(),
            self.bytes(),
            codec,
        )
    }
}
/// Full service name for this service.
pub const INTERCOM_SERVICE_SERVICE_NAME: &str = "home.v1.IntercomService";
/// Static [`Spec`](::connectrpc::Spec) for the `RegisterEndpoint` RPC, as seen by the server; the generated client passes it with [`origin`](::connectrpc::Spec::origin) `Client` (compare across sides with [`Spec::same_method`](::connectrpc::Spec::same_method)).
pub const INTERCOM_SERVICE_REGISTER_ENDPOINT_SPEC: ::connectrpc::Spec = ::connectrpc::Spec::server(
        "/home.v1.IntercomService/RegisterEndpoint",
        ::connectrpc::StreamType::Unary,
    )
    .with_idempotency_level(::connectrpc::IdempotencyLevel::Unknown);
/// Static [`Spec`](::connectrpc::Spec) for the `ListEndpoints` RPC, as seen by the server; the generated client passes it with [`origin`](::connectrpc::Spec::origin) `Client` (compare across sides with [`Spec::same_method`](::connectrpc::Spec::same_method)).
pub const INTERCOM_SERVICE_LIST_ENDPOINTS_SPEC: ::connectrpc::Spec = ::connectrpc::Spec::server(
        "/home.v1.IntercomService/ListEndpoints",
        ::connectrpc::StreamType::Unary,
    )
    .with_idempotency_level(::connectrpc::IdempotencyLevel::Unknown);
/// Static [`Spec`](::connectrpc::Spec) for the `StartCall` RPC, as seen by the server; the generated client passes it with [`origin`](::connectrpc::Spec::origin) `Client` (compare across sides with [`Spec::same_method`](::connectrpc::Spec::same_method)).
pub const INTERCOM_SERVICE_START_CALL_SPEC: ::connectrpc::Spec = ::connectrpc::Spec::server(
        "/home.v1.IntercomService/StartCall",
        ::connectrpc::StreamType::Unary,
    )
    .with_idempotency_level(::connectrpc::IdempotencyLevel::Unknown);
/// Static [`Spec`](::connectrpc::Spec) for the `Answer` RPC, as seen by the server; the generated client passes it with [`origin`](::connectrpc::Spec::origin) `Client` (compare across sides with [`Spec::same_method`](::connectrpc::Spec::same_method)).
pub const INTERCOM_SERVICE_ANSWER_SPEC: ::connectrpc::Spec = ::connectrpc::Spec::server(
        "/home.v1.IntercomService/Answer",
        ::connectrpc::StreamType::Unary,
    )
    .with_idempotency_level(::connectrpc::IdempotencyLevel::Unknown);
/// Static [`Spec`](::connectrpc::Spec) for the `Hangup` RPC, as seen by the server; the generated client passes it with [`origin`](::connectrpc::Spec::origin) `Client` (compare across sides with [`Spec::same_method`](::connectrpc::Spec::same_method)).
pub const INTERCOM_SERVICE_HANGUP_SPEC: ::connectrpc::Spec = ::connectrpc::Spec::server(
        "/home.v1.IntercomService/Hangup",
        ::connectrpc::StreamType::Unary,
    )
    .with_idempotency_level(::connectrpc::IdempotencyLevel::Unknown);
/// Static [`Spec`](::connectrpc::Spec) for the `GetCall` RPC, as seen by the server; the generated client passes it with [`origin`](::connectrpc::Spec::origin) `Client` (compare across sides with [`Spec::same_method`](::connectrpc::Spec::same_method)).
pub const INTERCOM_SERVICE_GET_CALL_SPEC: ::connectrpc::Spec = ::connectrpc::Spec::server(
        "/home.v1.IntercomService/GetCall",
        ::connectrpc::StreamType::Unary,
    )
    .with_idempotency_level(::connectrpc::IdempotencyLevel::Unknown);
/// Static [`Spec`](::connectrpc::Spec) for the `SendSignal` RPC, as seen by the server; the generated client passes it with [`origin`](::connectrpc::Spec::origin) `Client` (compare across sides with [`Spec::same_method`](::connectrpc::Spec::same_method)).
pub const INTERCOM_SERVICE_SEND_SIGNAL_SPEC: ::connectrpc::Spec = ::connectrpc::Spec::server(
        "/home.v1.IntercomService/SendSignal",
        ::connectrpc::StreamType::Unary,
    )
    .with_idempotency_level(::connectrpc::IdempotencyLevel::Unknown);
/// Static [`Spec`](::connectrpc::Spec) for the `Watch` RPC, as seen by the server; the generated client passes it with [`origin`](::connectrpc::Spec::origin) `Client` (compare across sides with [`Spec::same_method`](::connectrpc::Spec::same_method)).
pub const INTERCOM_SERVICE_WATCH_SPEC: ::connectrpc::Spec = ::connectrpc::Spec::server(
        "/home.v1.IntercomService/Watch",
        ::connectrpc::StreamType::ServerStream,
    )
    .with_idempotency_level(::connectrpc::IdempotencyLevel::Unknown);
/// Static [`Spec`](::connectrpc::Spec) for the `GetIceConfig` RPC, as seen by the server; the generated client passes it with [`origin`](::connectrpc::Spec::origin) `Client` (compare across sides with [`Spec::same_method`](::connectrpc::Spec::same_method)).
pub const INTERCOM_SERVICE_GET_ICE_CONFIG_SPEC: ::connectrpc::Spec = ::connectrpc::Spec::server(
        "/home.v1.IntercomService/GetIceConfig",
        ::connectrpc::StreamType::Unary,
    )
    .with_idempotency_level(::connectrpc::IdempotencyLevel::Unknown);
/// Static [`Spec`](::connectrpc::Spec) for the `GetStatus` RPC, as seen by the server; the generated client passes it with [`origin`](::connectrpc::Spec::origin) `Client` (compare across sides with [`Spec::same_method`](::connectrpc::Spec::same_method)).
pub const INTERCOM_SERVICE_GET_STATUS_SPEC: ::connectrpc::Spec = ::connectrpc::Spec::server(
        "/home.v1.IntercomService/GetStatus",
        ::connectrpc::StreamType::Unary,
    )
    .with_idempotency_level(::connectrpc::IdempotencyLevel::Unknown);
/// Server trait for IntercomService.
///
/// # Implementing handlers
///
/// Implement methods with plain `async fn`; the returned future satisfies
/// the `Send` bound automatically.
///
/// **Unary and server-streaming requests** arrive as
/// [`ServiceRequest<'_, Req>`](::connectrpc::ServiceRequest): a zero-copy
/// view of the request plus its body, valid for the duration of the call.
/// Fields are read directly (`request.name` is a `&str` into the decoded
/// buffer) and the borrow may be held across `.await` points. Anything
/// that must outlive the call — `tokio::spawn`, channels, server state,
/// or data captured by a returned response stream — takes owned data:
/// call `request.to_owned_message()` (or copy the specific fields)
/// first.
///
/// **Client-streaming and bidi requests** arrive as
/// [`InboundStream<Req>`](::connectrpc::InboundStream) — a
/// `ServiceStream` of [`StreamMessage`](::connectrpc::StreamMessage)s.
/// Each item owns its decoded buffer and is `Send + 'static`, so items
/// can be buffered or moved into spawned tasks; read fields zero-copy
/// through the generated accessor methods (`item.name()`) or `.view()`,
/// convert with `.to_owned_message()`, or yield an item back unchanged —
/// `StreamMessage<M>` implements `Encodable<M>`.
///
/// Request types resolved through `extern_path` (e.g. well-known types
/// from another crate) use the same wrappers; the crate that owns the
/// type must be generated with buffa ≥ 0.9.0 and views enabled so the
/// backing `HasMessageView` impl exists.
///
/// The `impl Encodable<Out>` return bound accepts the owned `Out`, the
/// generated `OutView<'_>` / `OwnedOutView`,
/// [`MaybeBorrowed`](::connectrpc::MaybeBorrowed), or
/// [`PreEncoded`](::connectrpc::PreEncoded) for handlers that encode a
/// non-`'static` view internally and pass the bytes across the handler
/// boundary. View bodies are not emitted for output types mapped via
/// `extern_path` (the impl would be an orphan); return owned for
/// WKT/extern outputs.
///
/// Server-streaming and bidi-streaming methods return
/// `ServiceStream<impl Encodable<Out> + Send + use<Self>>`. The
/// `use<Self>` precise-capturing clause excludes `&self`'s lifetime and
/// the request's lifetime (unary methods use `use<'a, Self>` and may
/// borrow from `&self`), so stream items must be `'static` and cannot
/// borrow from the request. To stream view-encoded data, encode each
/// item inside the stream body and yield
/// [`PreEncoded`](::connectrpc::PreEncoded) — see its `# Streaming
/// example` doc.
#[allow(clippy::type_complexity)]
pub trait IntercomService: Send + Sync + 'static {
    /// Handle the RegisterEndpoint RPC.
    ///
    /// `'a` lets the response body borrow from `&self` (e.g. server-resident state).
    ///
    /// `request` is borrowed from the request body and is valid for the
    /// duration of the call; message fields are read directly on it
    /// (zero-copy). The response cannot borrow from `request` — use
    /// `.to_owned_message()` (or copy the specific fields) for anything
    /// returned, stored, or moved into `tokio::spawn`.
    fn register_endpoint<'a>(
        &'a self,
        ctx: ::connectrpc::RequestContext,
        request: ::connectrpc::ServiceRequest<'_, RegisterEndpointRequest>,
    ) -> impl ::std::future::Future<
        Output = ::connectrpc::ServiceResult<
            impl ::connectrpc::Encodable<RegisterEndpointResponse> + Send + use<'a, Self>,
        >,
    > + Send;
    /// Handle the ListEndpoints RPC.
    ///
    /// `'a` lets the response body borrow from `&self` (e.g. server-resident state).
    ///
    /// `request` is borrowed from the request body and is valid for the
    /// duration of the call; message fields are read directly on it
    /// (zero-copy). The response cannot borrow from `request` — use
    /// `.to_owned_message()` (or copy the specific fields) for anything
    /// returned, stored, or moved into `tokio::spawn`.
    fn list_endpoints<'a>(
        &'a self,
        ctx: ::connectrpc::RequestContext,
        request: ::connectrpc::ServiceRequest<'_, ListEndpointsRequest>,
    ) -> impl ::std::future::Future<
        Output = ::connectrpc::ServiceResult<
            impl ::connectrpc::Encodable<ListEndpointsResponse> + Send + use<'a, Self>,
        >,
    > + Send;
    /// Handle the StartCall RPC.
    ///
    /// `'a` lets the response body borrow from `&self` (e.g. server-resident state).
    ///
    /// `request` is borrowed from the request body and is valid for the
    /// duration of the call; message fields are read directly on it
    /// (zero-copy). The response cannot borrow from `request` — use
    /// `.to_owned_message()` (or copy the specific fields) for anything
    /// returned, stored, or moved into `tokio::spawn`.
    fn start_call<'a>(
        &'a self,
        ctx: ::connectrpc::RequestContext,
        request: ::connectrpc::ServiceRequest<'_, StartCallRequest>,
    ) -> impl ::std::future::Future<
        Output = ::connectrpc::ServiceResult<
            impl ::connectrpc::Encodable<StartCallResponse> + Send + use<'a, Self>,
        >,
    > + Send;
    /// Handle the Answer RPC.
    ///
    /// `'a` lets the response body borrow from `&self` (e.g. server-resident state).
    ///
    /// `request` is borrowed from the request body and is valid for the
    /// duration of the call; message fields are read directly on it
    /// (zero-copy). The response cannot borrow from `request` — use
    /// `.to_owned_message()` (or copy the specific fields) for anything
    /// returned, stored, or moved into `tokio::spawn`.
    fn answer<'a>(
        &'a self,
        ctx: ::connectrpc::RequestContext,
        request: ::connectrpc::ServiceRequest<'_, AnswerRequest>,
    ) -> impl ::std::future::Future<
        Output = ::connectrpc::ServiceResult<
            impl ::connectrpc::Encodable<AnswerResponse> + Send + use<'a, Self>,
        >,
    > + Send;
    /// Handle the Hangup RPC.
    ///
    /// `'a` lets the response body borrow from `&self` (e.g. server-resident state).
    ///
    /// `request` is borrowed from the request body and is valid for the
    /// duration of the call; message fields are read directly on it
    /// (zero-copy). The response cannot borrow from `request` — use
    /// `.to_owned_message()` (or copy the specific fields) for anything
    /// returned, stored, or moved into `tokio::spawn`.
    fn hangup<'a>(
        &'a self,
        ctx: ::connectrpc::RequestContext,
        request: ::connectrpc::ServiceRequest<'_, HangupRequest>,
    ) -> impl ::std::future::Future<
        Output = ::connectrpc::ServiceResult<
            impl ::connectrpc::Encodable<HangupResponse> + Send + use<'a, Self>,
        >,
    > + Send;
    /// Handle the GetCall RPC.
    ///
    /// `'a` lets the response body borrow from `&self` (e.g. server-resident state).
    ///
    /// `request` is borrowed from the request body and is valid for the
    /// duration of the call; message fields are read directly on it
    /// (zero-copy). The response cannot borrow from `request` — use
    /// `.to_owned_message()` (or copy the specific fields) for anything
    /// returned, stored, or moved into `tokio::spawn`.
    fn get_call<'a>(
        &'a self,
        ctx: ::connectrpc::RequestContext,
        request: ::connectrpc::ServiceRequest<'_, GetCallRequest>,
    ) -> impl ::std::future::Future<
        Output = ::connectrpc::ServiceResult<
            impl ::connectrpc::Encodable<GetCallResponse> + Send + use<'a, Self>,
        >,
    > + Send;
    /// Handle the SendSignal RPC.
    ///
    /// `'a` lets the response body borrow from `&self` (e.g. server-resident state).
    ///
    /// `request` is borrowed from the request body and is valid for the
    /// duration of the call; message fields are read directly on it
    /// (zero-copy). The response cannot borrow from `request` — use
    /// `.to_owned_message()` (or copy the specific fields) for anything
    /// returned, stored, or moved into `tokio::spawn`.
    fn send_signal<'a>(
        &'a self,
        ctx: ::connectrpc::RequestContext,
        request: ::connectrpc::ServiceRequest<'_, SendSignalRequest>,
    ) -> impl ::std::future::Future<
        Output = ::connectrpc::ServiceResult<
            impl ::connectrpc::Encodable<SendSignalResponse> + Send + use<'a, Self>,
        >,
    > + Send;
    /// Handle the Watch RPC.
    ///
    /// `request` is borrowed from the request body and is valid for the
    /// duration of the call (until the response stream is returned);
    /// message fields are read directly on it (zero-copy). Data the
    /// returned stream needs must be copied out or converted via
    /// `.to_owned_message()`.
    fn watch(
        &self,
        ctx: ::connectrpc::RequestContext,
        request: ::connectrpc::ServiceRequest<'_, WatchRequest>,
    ) -> impl ::std::future::Future<
        Output = ::connectrpc::ServiceResult<
            ::connectrpc::ServiceStream<
                impl ::connectrpc::Encodable<WatchEvent> + Send + use<Self>,
            >,
        >,
    > + Send;
    /// Handle the GetIceConfig RPC.
    ///
    /// `'a` lets the response body borrow from `&self` (e.g. server-resident state).
    ///
    /// `request` is borrowed from the request body and is valid for the
    /// duration of the call; message fields are read directly on it
    /// (zero-copy). The response cannot borrow from `request` — use
    /// `.to_owned_message()` (or copy the specific fields) for anything
    /// returned, stored, or moved into `tokio::spawn`.
    fn get_ice_config<'a>(
        &'a self,
        ctx: ::connectrpc::RequestContext,
        request: ::connectrpc::ServiceRequest<'_, GetIceConfigRequest>,
    ) -> impl ::std::future::Future<
        Output = ::connectrpc::ServiceResult<
            impl ::connectrpc::Encodable<GetIceConfigResponse> + Send + use<'a, Self>,
        >,
    > + Send;
    /// Handle the GetStatus RPC.
    ///
    /// `'a` lets the response body borrow from `&self` (e.g. server-resident state).
    ///
    /// `request` is borrowed from the request body and is valid for the
    /// duration of the call; message fields are read directly on it
    /// (zero-copy). The response cannot borrow from `request` — use
    /// `.to_owned_message()` (or copy the specific fields) for anything
    /// returned, stored, or moved into `tokio::spawn`.
    fn get_status<'a>(
        &'a self,
        ctx: ::connectrpc::RequestContext,
        request: ::connectrpc::ServiceRequest<'_, GetIntercomStatusRequest>,
    ) -> impl ::std::future::Future<
        Output = ::connectrpc::ServiceResult<
            impl ::connectrpc::Encodable<
                GetIntercomStatusResponse,
            > + Send + use<'a, Self>,
        >,
    > + Send;
}
/// Extension trait for registering a service implementation with a Router.
///
/// This trait is automatically implemented for all types that implement the service trait.
/// Prefer [`Router::add_service`](::connectrpc::Router::add_service) for
/// top-down registration; `register` remains available for compatibility
/// and cases where the service-first call shape is more convenient.
///
/// # Example
///
/// ```rust,ignore
/// use std::sync::Arc;
///
/// let service = Arc::new(MyServiceImpl);
/// let router = service.register(Router::new());
/// ```
pub trait IntercomServiceExt: IntercomService {
    /// Register this service implementation with a Router.
    ///
    /// Takes ownership of the `Arc<Self>` and returns a new Router with
    /// this service's methods registered.
    fn register(
        self: ::std::sync::Arc<Self>,
        router: ::connectrpc::Router,
    ) -> ::connectrpc::Router;
}
impl<S: IntercomService> IntercomServiceExt for S {
    fn register(
        self: ::std::sync::Arc<Self>,
        router: ::connectrpc::Router,
    ) -> ::connectrpc::Router {
        router
            .route_view(
                INTERCOM_SERVICE_SERVICE_NAME,
                "RegisterEndpoint",
                {
                    let svc = ::std::sync::Arc::clone(&self);
                    ::connectrpc::view_handler_fn(move |
                        ctx,
                        req: ::buffa::view::OwnedView<
                            __buffa::view::RegisterEndpointRequestView<'static>,
                        >,
                        format|
                    {
                        let svc = ::std::sync::Arc::clone(&svc);
                        async move {
                            let sreq = ::connectrpc::ServiceRequest::<
                                RegisterEndpointRequest,
                            >::from_parts(req.reborrow(), req.bytes());
                            svc.register_endpoint(ctx, sreq)
                                .await?
                                .encode::<RegisterEndpointResponse>(format)
                        }
                    })
                },
            )
            .with_spec(INTERCOM_SERVICE_REGISTER_ENDPOINT_SPEC)
            .route_view(
                INTERCOM_SERVICE_SERVICE_NAME,
                "ListEndpoints",
                {
                    let svc = ::std::sync::Arc::clone(&self);
                    ::connectrpc::view_handler_fn(move |
                        ctx,
                        req: ::buffa::view::OwnedView<
                            __buffa::view::ListEndpointsRequestView<'static>,
                        >,
                        format|
                    {
                        let svc = ::std::sync::Arc::clone(&svc);
                        async move {
                            let sreq = ::connectrpc::ServiceRequest::<
                                ListEndpointsRequest,
                            >::from_parts(req.reborrow(), req.bytes());
                            svc.list_endpoints(ctx, sreq)
                                .await?
                                .encode::<ListEndpointsResponse>(format)
                        }
                    })
                },
            )
            .with_spec(INTERCOM_SERVICE_LIST_ENDPOINTS_SPEC)
            .route_view(
                INTERCOM_SERVICE_SERVICE_NAME,
                "StartCall",
                {
                    let svc = ::std::sync::Arc::clone(&self);
                    ::connectrpc::view_handler_fn(move |
                        ctx,
                        req: ::buffa::view::OwnedView<
                            __buffa::view::StartCallRequestView<'static>,
                        >,
                        format|
                    {
                        let svc = ::std::sync::Arc::clone(&svc);
                        async move {
                            let sreq = ::connectrpc::ServiceRequest::<
                                StartCallRequest,
                            >::from_parts(req.reborrow(), req.bytes());
                            svc.start_call(ctx, sreq)
                                .await?
                                .encode::<StartCallResponse>(format)
                        }
                    })
                },
            )
            .with_spec(INTERCOM_SERVICE_START_CALL_SPEC)
            .route_view(
                INTERCOM_SERVICE_SERVICE_NAME,
                "Answer",
                {
                    let svc = ::std::sync::Arc::clone(&self);
                    ::connectrpc::view_handler_fn(move |
                        ctx,
                        req: ::buffa::view::OwnedView<
                            __buffa::view::AnswerRequestView<'static>,
                        >,
                        format|
                    {
                        let svc = ::std::sync::Arc::clone(&svc);
                        async move {
                            let sreq = ::connectrpc::ServiceRequest::<
                                AnswerRequest,
                            >::from_parts(req.reborrow(), req.bytes());
                            svc.answer(ctx, sreq).await?.encode::<AnswerResponse>(format)
                        }
                    })
                },
            )
            .with_spec(INTERCOM_SERVICE_ANSWER_SPEC)
            .route_view(
                INTERCOM_SERVICE_SERVICE_NAME,
                "Hangup",
                {
                    let svc = ::std::sync::Arc::clone(&self);
                    ::connectrpc::view_handler_fn(move |
                        ctx,
                        req: ::buffa::view::OwnedView<
                            __buffa::view::HangupRequestView<'static>,
                        >,
                        format|
                    {
                        let svc = ::std::sync::Arc::clone(&svc);
                        async move {
                            let sreq = ::connectrpc::ServiceRequest::<
                                HangupRequest,
                            >::from_parts(req.reborrow(), req.bytes());
                            svc.hangup(ctx, sreq).await?.encode::<HangupResponse>(format)
                        }
                    })
                },
            )
            .with_spec(INTERCOM_SERVICE_HANGUP_SPEC)
            .route_view(
                INTERCOM_SERVICE_SERVICE_NAME,
                "GetCall",
                {
                    let svc = ::std::sync::Arc::clone(&self);
                    ::connectrpc::view_handler_fn(move |
                        ctx,
                        req: ::buffa::view::OwnedView<
                            __buffa::view::GetCallRequestView<'static>,
                        >,
                        format|
                    {
                        let svc = ::std::sync::Arc::clone(&svc);
                        async move {
                            let sreq = ::connectrpc::ServiceRequest::<
                                GetCallRequest,
                            >::from_parts(req.reborrow(), req.bytes());
                            svc.get_call(ctx, sreq)
                                .await?
                                .encode::<GetCallResponse>(format)
                        }
                    })
                },
            )
            .with_spec(INTERCOM_SERVICE_GET_CALL_SPEC)
            .route_view(
                INTERCOM_SERVICE_SERVICE_NAME,
                "SendSignal",
                {
                    let svc = ::std::sync::Arc::clone(&self);
                    ::connectrpc::view_handler_fn(move |
                        ctx,
                        req: ::buffa::view::OwnedView<
                            __buffa::view::SendSignalRequestView<'static>,
                        >,
                        format|
                    {
                        let svc = ::std::sync::Arc::clone(&svc);
                        async move {
                            let sreq = ::connectrpc::ServiceRequest::<
                                SendSignalRequest,
                            >::from_parts(req.reborrow(), req.bytes());
                            svc.send_signal(ctx, sreq)
                                .await?
                                .encode::<SendSignalResponse>(format)
                        }
                    })
                },
            )
            .with_spec(INTERCOM_SERVICE_SEND_SIGNAL_SPEC)
            .route_view_server_stream::<
                _,
                _,
                WatchEvent,
            >(
                INTERCOM_SERVICE_SERVICE_NAME,
                "Watch",
                ::connectrpc::view_streaming_handler_fn({
                    let svc = ::std::sync::Arc::clone(&self);
                    move |
                        ctx,
                        req: ::buffa::view::OwnedView<
                            __buffa::view::WatchRequestView<'static>,
                        >|
                    {
                        let svc = ::std::sync::Arc::clone(&svc);
                        async move {
                            let sreq = ::connectrpc::ServiceRequest::<
                                WatchRequest,
                            >::from_parts(req.reborrow(), req.bytes());
                            svc.watch(ctx, sreq).await
                        }
                    }
                }),
            )
            .with_spec(INTERCOM_SERVICE_WATCH_SPEC)
            .route_view(
                INTERCOM_SERVICE_SERVICE_NAME,
                "GetIceConfig",
                {
                    let svc = ::std::sync::Arc::clone(&self);
                    ::connectrpc::view_handler_fn(move |
                        ctx,
                        req: ::buffa::view::OwnedView<
                            __buffa::view::GetIceConfigRequestView<'static>,
                        >,
                        format|
                    {
                        let svc = ::std::sync::Arc::clone(&svc);
                        async move {
                            let sreq = ::connectrpc::ServiceRequest::<
                                GetIceConfigRequest,
                            >::from_parts(req.reborrow(), req.bytes());
                            svc.get_ice_config(ctx, sreq)
                                .await?
                                .encode::<GetIceConfigResponse>(format)
                        }
                    })
                },
            )
            .with_spec(INTERCOM_SERVICE_GET_ICE_CONFIG_SPEC)
            .route_view(
                INTERCOM_SERVICE_SERVICE_NAME,
                "GetStatus",
                {
                    let svc = ::std::sync::Arc::clone(&self);
                    ::connectrpc::view_handler_fn(move |
                        ctx,
                        req: ::buffa::view::OwnedView<
                            __buffa::view::GetIntercomStatusRequestView<'static>,
                        >,
                        format|
                    {
                        let svc = ::std::sync::Arc::clone(&svc);
                        async move {
                            let sreq = ::connectrpc::ServiceRequest::<
                                GetIntercomStatusRequest,
                            >::from_parts(req.reborrow(), req.bytes());
                            svc.get_status(ctx, sreq)
                                .await?
                                .encode::<GetIntercomStatusResponse>(format)
                        }
                    })
                },
            )
            .with_spec(INTERCOM_SERVICE_GET_STATUS_SPEC)
    }
}
/// Type-inference marker used by [`Router::add_service`](::connectrpc::Router::add_service).
#[doc(hidden)]
pub struct IntercomServiceRegisterMarker;
impl<S: IntercomService> ::connectrpc::ServiceRegister<IntercomServiceRegisterMarker>
for ::std::sync::Arc<S> {
    fn register_service(self, router: ::connectrpc::Router) -> ::connectrpc::Router {
        <S as IntercomServiceExt>::register(self, router)
    }
}
/// Monomorphic dispatcher for `IntercomService`.
///
/// Unlike `.register(Router)` which type-erases each method into an `Arc<dyn ErasedHandler>` stored in a `HashMap`, this struct dispatches via a compile-time `match` on method name: no vtable, no hash lookup.
///
/// # Example
///
/// ```rust,ignore
/// use connectrpc::ConnectRpcService;
///
/// let server = IntercomServiceServer::new(MyImpl);
/// let service = ConnectRpcService::new(server);
/// // hand `service` to axum/hyper as a fallback_service
/// ```
pub struct IntercomServiceServer<T> {
    inner: ::std::sync::Arc<T>,
}
impl<T: IntercomService> IntercomServiceServer<T> {
    /// Wrap a service implementation in a monomorphic dispatcher.
    pub fn new(service: T) -> Self {
        Self {
            inner: ::std::sync::Arc::new(service),
        }
    }
    /// Wrap an already-`Arc`'d service implementation.
    pub fn from_arc(inner: ::std::sync::Arc<T>) -> Self {
        Self { inner }
    }
}
impl<T> Clone for IntercomServiceServer<T> {
    fn clone(&self) -> Self {
        Self {
            inner: ::std::sync::Arc::clone(&self.inner),
        }
    }
}
impl<T: IntercomService> ::connectrpc::Dispatcher for IntercomServiceServer<T> {
    #[inline]
    fn lookup(
        &self,
        path: &str,
    ) -> Option<::connectrpc::dispatcher::codegen::MethodDescriptor> {
        let method = path.strip_prefix("home.v1.IntercomService/")?;
        match method {
            "RegisterEndpoint" => {
                Some(
                    ::connectrpc::dispatcher::codegen::MethodDescriptor::unary(false)
                        .with_spec(INTERCOM_SERVICE_REGISTER_ENDPOINT_SPEC),
                )
            }
            "ListEndpoints" => {
                Some(
                    ::connectrpc::dispatcher::codegen::MethodDescriptor::unary(false)
                        .with_spec(INTERCOM_SERVICE_LIST_ENDPOINTS_SPEC),
                )
            }
            "StartCall" => {
                Some(
                    ::connectrpc::dispatcher::codegen::MethodDescriptor::unary(false)
                        .with_spec(INTERCOM_SERVICE_START_CALL_SPEC),
                )
            }
            "Answer" => {
                Some(
                    ::connectrpc::dispatcher::codegen::MethodDescriptor::unary(false)
                        .with_spec(INTERCOM_SERVICE_ANSWER_SPEC),
                )
            }
            "Hangup" => {
                Some(
                    ::connectrpc::dispatcher::codegen::MethodDescriptor::unary(false)
                        .with_spec(INTERCOM_SERVICE_HANGUP_SPEC),
                )
            }
            "GetCall" => {
                Some(
                    ::connectrpc::dispatcher::codegen::MethodDescriptor::unary(false)
                        .with_spec(INTERCOM_SERVICE_GET_CALL_SPEC),
                )
            }
            "SendSignal" => {
                Some(
                    ::connectrpc::dispatcher::codegen::MethodDescriptor::unary(false)
                        .with_spec(INTERCOM_SERVICE_SEND_SIGNAL_SPEC),
                )
            }
            "Watch" => {
                Some(
                    ::connectrpc::dispatcher::codegen::MethodDescriptor::server_streaming()
                        .with_spec(INTERCOM_SERVICE_WATCH_SPEC),
                )
            }
            "GetIceConfig" => {
                Some(
                    ::connectrpc::dispatcher::codegen::MethodDescriptor::unary(false)
                        .with_spec(INTERCOM_SERVICE_GET_ICE_CONFIG_SPEC),
                )
            }
            "GetStatus" => {
                Some(
                    ::connectrpc::dispatcher::codegen::MethodDescriptor::unary(false)
                        .with_spec(INTERCOM_SERVICE_GET_STATUS_SPEC),
                )
            }
            _ => None,
        }
    }
    fn call_unary(
        &self,
        path: &str,
        ctx: ::connectrpc::RequestContext,
        request: ::connectrpc::Payload,
        format: ::connectrpc::CodecFormat,
    ) -> ::connectrpc::dispatcher::codegen::UnaryResult {
        let Some(method) = path.strip_prefix("home.v1.IntercomService/") else {
            return ::connectrpc::dispatcher::codegen::unimplemented_unary(path);
        };
        let _ = (&ctx, &request, &format);
        match method {
            "RegisterEndpoint" => {
                let svc = ::std::sync::Arc::clone(&self.inner);
                Box::pin(async move {
                    let body = ::connectrpc::dispatcher::codegen::request_proto_bytes::<
                        RegisterEndpointRequest,
                    >(request.encoded()?, format)?;
                    let req: __buffa::view::RegisterEndpointRequestView<'_> = ::connectrpc::dispatcher::codegen::decode_borrowed_request_view(
                        &body,
                        ctx.decode_options(),
                    )?;
                    let req = ::connectrpc::ServiceRequest::<
                        RegisterEndpointRequest,
                    >::from_parts(&req, &body);
                    svc.register_endpoint(ctx, req)
                        .await?
                        .encode::<RegisterEndpointResponse>(format)
                })
            }
            "ListEndpoints" => {
                let svc = ::std::sync::Arc::clone(&self.inner);
                Box::pin(async move {
                    let body = ::connectrpc::dispatcher::codegen::request_proto_bytes::<
                        ListEndpointsRequest,
                    >(request.encoded()?, format)?;
                    let req: __buffa::view::ListEndpointsRequestView<'_> = ::connectrpc::dispatcher::codegen::decode_borrowed_request_view(
                        &body,
                        ctx.decode_options(),
                    )?;
                    let req = ::connectrpc::ServiceRequest::<
                        ListEndpointsRequest,
                    >::from_parts(&req, &body);
                    svc.list_endpoints(ctx, req)
                        .await?
                        .encode::<ListEndpointsResponse>(format)
                })
            }
            "StartCall" => {
                let svc = ::std::sync::Arc::clone(&self.inner);
                Box::pin(async move {
                    let body = ::connectrpc::dispatcher::codegen::request_proto_bytes::<
                        StartCallRequest,
                    >(request.encoded()?, format)?;
                    let req: __buffa::view::StartCallRequestView<'_> = ::connectrpc::dispatcher::codegen::decode_borrowed_request_view(
                        &body,
                        ctx.decode_options(),
                    )?;
                    let req = ::connectrpc::ServiceRequest::<
                        StartCallRequest,
                    >::from_parts(&req, &body);
                    svc.start_call(ctx, req).await?.encode::<StartCallResponse>(format)
                })
            }
            "Answer" => {
                let svc = ::std::sync::Arc::clone(&self.inner);
                Box::pin(async move {
                    let body = ::connectrpc::dispatcher::codegen::request_proto_bytes::<
                        AnswerRequest,
                    >(request.encoded()?, format)?;
                    let req: __buffa::view::AnswerRequestView<'_> = ::connectrpc::dispatcher::codegen::decode_borrowed_request_view(
                        &body,
                        ctx.decode_options(),
                    )?;
                    let req = ::connectrpc::ServiceRequest::<
                        AnswerRequest,
                    >::from_parts(&req, &body);
                    svc.answer(ctx, req).await?.encode::<AnswerResponse>(format)
                })
            }
            "Hangup" => {
                let svc = ::std::sync::Arc::clone(&self.inner);
                Box::pin(async move {
                    let body = ::connectrpc::dispatcher::codegen::request_proto_bytes::<
                        HangupRequest,
                    >(request.encoded()?, format)?;
                    let req: __buffa::view::HangupRequestView<'_> = ::connectrpc::dispatcher::codegen::decode_borrowed_request_view(
                        &body,
                        ctx.decode_options(),
                    )?;
                    let req = ::connectrpc::ServiceRequest::<
                        HangupRequest,
                    >::from_parts(&req, &body);
                    svc.hangup(ctx, req).await?.encode::<HangupResponse>(format)
                })
            }
            "GetCall" => {
                let svc = ::std::sync::Arc::clone(&self.inner);
                Box::pin(async move {
                    let body = ::connectrpc::dispatcher::codegen::request_proto_bytes::<
                        GetCallRequest,
                    >(request.encoded()?, format)?;
                    let req: __buffa::view::GetCallRequestView<'_> = ::connectrpc::dispatcher::codegen::decode_borrowed_request_view(
                        &body,
                        ctx.decode_options(),
                    )?;
                    let req = ::connectrpc::ServiceRequest::<
                        GetCallRequest,
                    >::from_parts(&req, &body);
                    svc.get_call(ctx, req).await?.encode::<GetCallResponse>(format)
                })
            }
            "SendSignal" => {
                let svc = ::std::sync::Arc::clone(&self.inner);
                Box::pin(async move {
                    let body = ::connectrpc::dispatcher::codegen::request_proto_bytes::<
                        SendSignalRequest,
                    >(request.encoded()?, format)?;
                    let req: __buffa::view::SendSignalRequestView<'_> = ::connectrpc::dispatcher::codegen::decode_borrowed_request_view(
                        &body,
                        ctx.decode_options(),
                    )?;
                    let req = ::connectrpc::ServiceRequest::<
                        SendSignalRequest,
                    >::from_parts(&req, &body);
                    svc.send_signal(ctx, req).await?.encode::<SendSignalResponse>(format)
                })
            }
            "GetIceConfig" => {
                let svc = ::std::sync::Arc::clone(&self.inner);
                Box::pin(async move {
                    let body = ::connectrpc::dispatcher::codegen::request_proto_bytes::<
                        GetIceConfigRequest,
                    >(request.encoded()?, format)?;
                    let req: __buffa::view::GetIceConfigRequestView<'_> = ::connectrpc::dispatcher::codegen::decode_borrowed_request_view(
                        &body,
                        ctx.decode_options(),
                    )?;
                    let req = ::connectrpc::ServiceRequest::<
                        GetIceConfigRequest,
                    >::from_parts(&req, &body);
                    svc.get_ice_config(ctx, req)
                        .await?
                        .encode::<GetIceConfigResponse>(format)
                })
            }
            "GetStatus" => {
                let svc = ::std::sync::Arc::clone(&self.inner);
                Box::pin(async move {
                    let body = ::connectrpc::dispatcher::codegen::request_proto_bytes::<
                        GetIntercomStatusRequest,
                    >(request.encoded()?, format)?;
                    let req: __buffa::view::GetIntercomStatusRequestView<'_> = ::connectrpc::dispatcher::codegen::decode_borrowed_request_view(
                        &body,
                        ctx.decode_options(),
                    )?;
                    let req = ::connectrpc::ServiceRequest::<
                        GetIntercomStatusRequest,
                    >::from_parts(&req, &body);
                    svc.get_status(ctx, req)
                        .await?
                        .encode::<GetIntercomStatusResponse>(format)
                })
            }
            _ => ::connectrpc::dispatcher::codegen::unimplemented_unary(path),
        }
    }
    fn call_server_streaming(
        &self,
        path: &str,
        ctx: ::connectrpc::RequestContext,
        request: ::buffa::bytes::Bytes,
        format: ::connectrpc::CodecFormat,
    ) -> ::connectrpc::dispatcher::codegen::StreamingResult {
        let Some(method) = path.strip_prefix("home.v1.IntercomService/") else {
            return ::connectrpc::dispatcher::codegen::unimplemented_streaming(path);
        };
        let _ = (&ctx, &request, &format);
        match method {
            "Watch" => {
                let svc = ::std::sync::Arc::clone(&self.inner);
                Box::pin(async move {
                    let body = ::connectrpc::dispatcher::codegen::request_proto_bytes::<
                        WatchRequest,
                    >(request, format)?;
                    let req: __buffa::view::WatchRequestView<'_> = ::connectrpc::dispatcher::codegen::decode_borrowed_request_view(
                        &body,
                        ctx.decode_options(),
                    )?;
                    let req = ::connectrpc::ServiceRequest::<
                        WatchRequest,
                    >::from_parts(&req, &body);
                    let resp = svc.watch(ctx, req).await?;
                    Ok(
                        resp
                            .map_body(|s| ::connectrpc::dispatcher::codegen::encode_response_stream::<
                                WatchEvent,
                                _,
                                _,
                            >(s, format)),
                    )
                })
            }
            _ => ::connectrpc::dispatcher::codegen::unimplemented_streaming(path),
        }
    }
    fn call_client_streaming(
        &self,
        path: &str,
        ctx: ::connectrpc::RequestContext,
        requests: ::connectrpc::dispatcher::codegen::RequestStream,
        format: ::connectrpc::CodecFormat,
    ) -> ::connectrpc::dispatcher::codegen::UnaryResult {
        let Some(method) = path.strip_prefix("home.v1.IntercomService/") else {
            return ::connectrpc::dispatcher::codegen::unimplemented_unary(path);
        };
        let _ = (&ctx, &requests, &format);
        match method {
            _ => ::connectrpc::dispatcher::codegen::unimplemented_unary(path),
        }
    }
    fn call_bidi_streaming(
        &self,
        path: &str,
        ctx: ::connectrpc::RequestContext,
        requests: ::connectrpc::dispatcher::codegen::RequestStream,
        format: ::connectrpc::CodecFormat,
    ) -> ::connectrpc::dispatcher::codegen::StreamingResult {
        let Some(method) = path.strip_prefix("home.v1.IntercomService/") else {
            return ::connectrpc::dispatcher::codegen::unimplemented_streaming(path);
        };
        let _ = (&ctx, &requests, &format);
        match method {
            _ => ::connectrpc::dispatcher::codegen::unimplemented_streaming(path),
        }
    }
}
/// Client for this service.
///
/// Generic over `T: ClientTransport`. For **gRPC** (HTTP/2), use
/// `Http2Connection` — it has honest `poll_ready` and composes with
/// `tower::balance` for multi-connection load balancing. For **Connect
/// over HTTP/1.1** (or unknown protocol), use `HttpClient`.
///
/// # Example (gRPC / HTTP/2)
///
/// ```rust,ignore
/// use connectrpc::client::{Http2Connection, ClientConfig};
/// use connectrpc::Protocol;
///
/// let uri: http::Uri = "http://localhost:8080".parse()?;
/// let conn = Http2Connection::connect_plaintext(uri.clone()).await?.shared(1024);
/// let config = ClientConfig::new(uri).with_protocol(Protocol::Grpc);
///
/// let client = IntercomServiceClient::new(conn, config);
/// let response = client.register_endpoint(request).await?;
/// ```
///
/// # Example (Connect / HTTP/1.1 or ALPN)
///
/// ```rust,ignore
/// use connectrpc::client::{HttpClient, ClientConfig};
///
/// let http = HttpClient::plaintext();  // cleartext http:// only
/// let config = ClientConfig::new("http://localhost:8080".parse()?);
///
/// let client = IntercomServiceClient::new(http, config);
/// let response = client.register_endpoint(request).await?;
/// ```
///
/// # Working with the response
///
/// Unary calls return [`UnaryResponse<OwnedView<FooView>>`](::connectrpc::client::UnaryResponse).
/// [`view()`](::connectrpc::client::UnaryResponse::view) borrows the response
/// message, so field access is zero-copy:
///
/// ```rust,ignore
/// let resp = client.register_endpoint(request).await?;
/// let name: &str = resp.view().name;  // borrow into the response buffer
/// ```
///
/// If you need the owned struct (e.g. to store or pass by value), use
/// [`into_owned()`](::connectrpc::client::UnaryResponse::into_owned):
///
/// ```rust,ignore
/// let owned = client.register_endpoint(request).await?.into_owned();
/// ```
///
/// [`into_view()`](::connectrpc::client::UnaryResponse::into_view) keeps the
/// zero-copy decoded body (an `OwnedView`) without copying; field access on it
/// goes through `.reborrow()`. Streaming responses yield one
/// [`StreamMessage`](::connectrpc::StreamMessage) per received message from
/// `.message().await` — read fields zero-copy through the generated accessor
/// methods (`msg.name()`) or `.view()`, or convert with `.to_owned_message()`.
#[derive(Clone)]
pub struct IntercomServiceClient<T> {
    transport: T,
    config: ::connectrpc::client::ClientConfig,
}
impl<T> IntercomServiceClient<T>
where
    T: ::connectrpc::client::ClientTransport,
    <T::ResponseBody as ::connectrpc::http_body::Body>::Error: ::std::fmt::Display,
{
    /// Create a new client with the given transport and configuration.
    pub fn new(transport: T, config: ::connectrpc::client::ClientConfig) -> Self {
        Self { transport, config }
    }
    /// Get the client configuration.
    pub fn config(&self) -> &::connectrpc::client::ClientConfig {
        &self.config
    }
    /// Get a mutable reference to the client configuration.
    pub fn config_mut(&mut self) -> &mut ::connectrpc::client::ClientConfig {
        &mut self.config
    }
    /// Call the RegisterEndpoint RPC. Sends a request to /home.v1.IntercomService/RegisterEndpoint.
    pub async fn register_endpoint(
        &self,
        request: RegisterEndpointRequest,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<
                __buffa::view::RegisterEndpointResponseView<'static>,
            >,
        >,
        ::connectrpc::ConnectError,
    > {
        self.register_endpoint_with_options(
                request,
                ::connectrpc::client::CallOptions::default(),
            )
            .await
    }
    /// Call the RegisterEndpoint RPC with explicit per-call options. Options override [`ClientConfig`](::connectrpc::client::ClientConfig) defaults.
    pub async fn register_endpoint_with_options(
        &self,
        request: RegisterEndpointRequest,
        options: ::connectrpc::client::CallOptions,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<
                __buffa::view::RegisterEndpointResponseView<'static>,
            >,
        >,
        ::connectrpc::ConnectError,
    > {
        ::connectrpc::client::call_unary(
                &self.transport,
                &self.config,
                INTERCOM_SERVICE_REGISTER_ENDPOINT_SPEC
                    .with_origin(::connectrpc::SpecOrigin::Client),
                request,
                options,
            )
            .await
    }
    /// Call the ListEndpoints RPC. Sends a request to /home.v1.IntercomService/ListEndpoints.
    pub async fn list_endpoints(
        &self,
        request: ListEndpointsRequest,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<__buffa::view::ListEndpointsResponseView<'static>>,
        >,
        ::connectrpc::ConnectError,
    > {
        self.list_endpoints_with_options(
                request,
                ::connectrpc::client::CallOptions::default(),
            )
            .await
    }
    /// Call the ListEndpoints RPC with explicit per-call options. Options override [`ClientConfig`](::connectrpc::client::ClientConfig) defaults.
    pub async fn list_endpoints_with_options(
        &self,
        request: ListEndpointsRequest,
        options: ::connectrpc::client::CallOptions,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<__buffa::view::ListEndpointsResponseView<'static>>,
        >,
        ::connectrpc::ConnectError,
    > {
        ::connectrpc::client::call_unary(
                &self.transport,
                &self.config,
                INTERCOM_SERVICE_LIST_ENDPOINTS_SPEC
                    .with_origin(::connectrpc::SpecOrigin::Client),
                request,
                options,
            )
            .await
    }
    /// Call the StartCall RPC. Sends a request to /home.v1.IntercomService/StartCall.
    pub async fn start_call(
        &self,
        request: StartCallRequest,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<__buffa::view::StartCallResponseView<'static>>,
        >,
        ::connectrpc::ConnectError,
    > {
        self.start_call_with_options(
                request,
                ::connectrpc::client::CallOptions::default(),
            )
            .await
    }
    /// Call the StartCall RPC with explicit per-call options. Options override [`ClientConfig`](::connectrpc::client::ClientConfig) defaults.
    pub async fn start_call_with_options(
        &self,
        request: StartCallRequest,
        options: ::connectrpc::client::CallOptions,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<__buffa::view::StartCallResponseView<'static>>,
        >,
        ::connectrpc::ConnectError,
    > {
        ::connectrpc::client::call_unary(
                &self.transport,
                &self.config,
                INTERCOM_SERVICE_START_CALL_SPEC
                    .with_origin(::connectrpc::SpecOrigin::Client),
                request,
                options,
            )
            .await
    }
    /// Call the Answer RPC. Sends a request to /home.v1.IntercomService/Answer.
    pub async fn answer(
        &self,
        request: AnswerRequest,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<__buffa::view::AnswerResponseView<'static>>,
        >,
        ::connectrpc::ConnectError,
    > {
        self.answer_with_options(request, ::connectrpc::client::CallOptions::default())
            .await
    }
    /// Call the Answer RPC with explicit per-call options. Options override [`ClientConfig`](::connectrpc::client::ClientConfig) defaults.
    pub async fn answer_with_options(
        &self,
        request: AnswerRequest,
        options: ::connectrpc::client::CallOptions,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<__buffa::view::AnswerResponseView<'static>>,
        >,
        ::connectrpc::ConnectError,
    > {
        ::connectrpc::client::call_unary(
                &self.transport,
                &self.config,
                INTERCOM_SERVICE_ANSWER_SPEC
                    .with_origin(::connectrpc::SpecOrigin::Client),
                request,
                options,
            )
            .await
    }
    /// Call the Hangup RPC. Sends a request to /home.v1.IntercomService/Hangup.
    pub async fn hangup(
        &self,
        request: HangupRequest,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<__buffa::view::HangupResponseView<'static>>,
        >,
        ::connectrpc::ConnectError,
    > {
        self.hangup_with_options(request, ::connectrpc::client::CallOptions::default())
            .await
    }
    /// Call the Hangup RPC with explicit per-call options. Options override [`ClientConfig`](::connectrpc::client::ClientConfig) defaults.
    pub async fn hangup_with_options(
        &self,
        request: HangupRequest,
        options: ::connectrpc::client::CallOptions,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<__buffa::view::HangupResponseView<'static>>,
        >,
        ::connectrpc::ConnectError,
    > {
        ::connectrpc::client::call_unary(
                &self.transport,
                &self.config,
                INTERCOM_SERVICE_HANGUP_SPEC
                    .with_origin(::connectrpc::SpecOrigin::Client),
                request,
                options,
            )
            .await
    }
    /// Call the GetCall RPC. Sends a request to /home.v1.IntercomService/GetCall.
    pub async fn get_call(
        &self,
        request: GetCallRequest,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<__buffa::view::GetCallResponseView<'static>>,
        >,
        ::connectrpc::ConnectError,
    > {
        self.get_call_with_options(request, ::connectrpc::client::CallOptions::default())
            .await
    }
    /// Call the GetCall RPC with explicit per-call options. Options override [`ClientConfig`](::connectrpc::client::ClientConfig) defaults.
    pub async fn get_call_with_options(
        &self,
        request: GetCallRequest,
        options: ::connectrpc::client::CallOptions,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<__buffa::view::GetCallResponseView<'static>>,
        >,
        ::connectrpc::ConnectError,
    > {
        ::connectrpc::client::call_unary(
                &self.transport,
                &self.config,
                INTERCOM_SERVICE_GET_CALL_SPEC
                    .with_origin(::connectrpc::SpecOrigin::Client),
                request,
                options,
            )
            .await
    }
    /// Call the SendSignal RPC. Sends a request to /home.v1.IntercomService/SendSignal.
    pub async fn send_signal(
        &self,
        request: SendSignalRequest,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<__buffa::view::SendSignalResponseView<'static>>,
        >,
        ::connectrpc::ConnectError,
    > {
        self.send_signal_with_options(
                request,
                ::connectrpc::client::CallOptions::default(),
            )
            .await
    }
    /// Call the SendSignal RPC with explicit per-call options. Options override [`ClientConfig`](::connectrpc::client::ClientConfig) defaults.
    pub async fn send_signal_with_options(
        &self,
        request: SendSignalRequest,
        options: ::connectrpc::client::CallOptions,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<__buffa::view::SendSignalResponseView<'static>>,
        >,
        ::connectrpc::ConnectError,
    > {
        ::connectrpc::client::call_unary(
                &self.transport,
                &self.config,
                INTERCOM_SERVICE_SEND_SIGNAL_SPEC
                    .with_origin(::connectrpc::SpecOrigin::Client),
                request,
                options,
            )
            .await
    }
    /// Call the Watch RPC. Sends a request to /home.v1.IntercomService/Watch.
    pub async fn watch(
        &self,
        request: WatchRequest,
    ) -> Result<
        ::connectrpc::client::ServerStream<
            T::ResponseBody,
            __buffa::view::WatchEventView<'static>,
        >,
        ::connectrpc::ConnectError,
    > {
        self.watch_with_options(request, ::connectrpc::client::CallOptions::default())
            .await
    }
    /// Call the Watch RPC with explicit per-call options. Options override [`ClientConfig`](::connectrpc::client::ClientConfig) defaults.
    pub async fn watch_with_options(
        &self,
        request: WatchRequest,
        options: ::connectrpc::client::CallOptions,
    ) -> Result<
        ::connectrpc::client::ServerStream<
            T::ResponseBody,
            __buffa::view::WatchEventView<'static>,
        >,
        ::connectrpc::ConnectError,
    > {
        ::connectrpc::client::call_server_stream(
                &self.transport,
                &self.config,
                INTERCOM_SERVICE_WATCH_SPEC
                    .with_origin(::connectrpc::SpecOrigin::Client),
                request,
                options,
            )
            .await
    }
    /// Call the GetIceConfig RPC. Sends a request to /home.v1.IntercomService/GetIceConfig.
    pub async fn get_ice_config(
        &self,
        request: GetIceConfigRequest,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<__buffa::view::GetIceConfigResponseView<'static>>,
        >,
        ::connectrpc::ConnectError,
    > {
        self.get_ice_config_with_options(
                request,
                ::connectrpc::client::CallOptions::default(),
            )
            .await
    }
    /// Call the GetIceConfig RPC with explicit per-call options. Options override [`ClientConfig`](::connectrpc::client::ClientConfig) defaults.
    pub async fn get_ice_config_with_options(
        &self,
        request: GetIceConfigRequest,
        options: ::connectrpc::client::CallOptions,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<__buffa::view::GetIceConfigResponseView<'static>>,
        >,
        ::connectrpc::ConnectError,
    > {
        ::connectrpc::client::call_unary(
                &self.transport,
                &self.config,
                INTERCOM_SERVICE_GET_ICE_CONFIG_SPEC
                    .with_origin(::connectrpc::SpecOrigin::Client),
                request,
                options,
            )
            .await
    }
    /// Call the GetStatus RPC. Sends a request to /home.v1.IntercomService/GetStatus.
    pub async fn get_status(
        &self,
        request: GetIntercomStatusRequest,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<
                __buffa::view::GetIntercomStatusResponseView<'static>,
            >,
        >,
        ::connectrpc::ConnectError,
    > {
        self.get_status_with_options(
                request,
                ::connectrpc::client::CallOptions::default(),
            )
            .await
    }
    /// Call the GetStatus RPC with explicit per-call options. Options override [`ClientConfig`](::connectrpc::client::ClientConfig) defaults.
    pub async fn get_status_with_options(
        &self,
        request: GetIntercomStatusRequest,
        options: ::connectrpc::client::CallOptions,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<
                __buffa::view::GetIntercomStatusResponseView<'static>,
            >,
        >,
        ::connectrpc::ConnectError,
    > {
        ::connectrpc::client::call_unary(
                &self.transport,
                &self.config,
                INTERCOM_SERVICE_GET_STATUS_SPEC
                    .with_origin(::connectrpc::SpecOrigin::Client),
                request,
                options,
            )
            .await
    }
}
