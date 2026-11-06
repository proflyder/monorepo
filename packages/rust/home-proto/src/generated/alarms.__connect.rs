///Shorthand for `OwnedView<CreateAlarmRequestView<'static>>`.
pub type OwnedCreateAlarmRequestView = ::buffa::view::OwnedView<
    __buffa::view::CreateAlarmRequestView<'static>,
>;
///Shorthand for `OwnedView<CreateAlarmResponseView<'static>>`.
pub type OwnedCreateAlarmResponseView = ::buffa::view::OwnedView<
    __buffa::view::CreateAlarmResponseView<'static>,
>;
///Shorthand for `OwnedView<GetAlarmRequestView<'static>>`.
pub type OwnedGetAlarmRequestView = ::buffa::view::OwnedView<
    __buffa::view::GetAlarmRequestView<'static>,
>;
///Shorthand for `OwnedView<GetAlarmResponseView<'static>>`.
pub type OwnedGetAlarmResponseView = ::buffa::view::OwnedView<
    __buffa::view::GetAlarmResponseView<'static>,
>;
///Shorthand for `OwnedView<ListAlarmsRequestView<'static>>`.
pub type OwnedListAlarmsRequestView = ::buffa::view::OwnedView<
    __buffa::view::ListAlarmsRequestView<'static>,
>;
///Shorthand for `OwnedView<ListAlarmsResponseView<'static>>`.
pub type OwnedListAlarmsResponseView = ::buffa::view::OwnedView<
    __buffa::view::ListAlarmsResponseView<'static>,
>;
///Shorthand for `OwnedView<UpdateAlarmRequestView<'static>>`.
pub type OwnedUpdateAlarmRequestView = ::buffa::view::OwnedView<
    __buffa::view::UpdateAlarmRequestView<'static>,
>;
///Shorthand for `OwnedView<UpdateAlarmResponseView<'static>>`.
pub type OwnedUpdateAlarmResponseView = ::buffa::view::OwnedView<
    __buffa::view::UpdateAlarmResponseView<'static>,
>;
///Shorthand for `OwnedView<DeleteAlarmRequestView<'static>>`.
pub type OwnedDeleteAlarmRequestView = ::buffa::view::OwnedView<
    __buffa::view::DeleteAlarmRequestView<'static>,
>;
///Shorthand for `OwnedView<DeleteAlarmResponseView<'static>>`.
pub type OwnedDeleteAlarmResponseView = ::buffa::view::OwnedView<
    __buffa::view::DeleteAlarmResponseView<'static>,
>;
///Shorthand for `OwnedView<SetAlarmEnabledRequestView<'static>>`.
pub type OwnedSetAlarmEnabledRequestView = ::buffa::view::OwnedView<
    __buffa::view::SetAlarmEnabledRequestView<'static>,
>;
///Shorthand for `OwnedView<SetAlarmEnabledResponseView<'static>>`.
pub type OwnedSetAlarmEnabledResponseView = ::buffa::view::OwnedView<
    __buffa::view::SetAlarmEnabledResponseView<'static>,
>;
///Shorthand for `OwnedView<SnoozeAlarmRequestView<'static>>`.
pub type OwnedSnoozeAlarmRequestView = ::buffa::view::OwnedView<
    __buffa::view::SnoozeAlarmRequestView<'static>,
>;
///Shorthand for `OwnedView<SnoozeAlarmResponseView<'static>>`.
pub type OwnedSnoozeAlarmResponseView = ::buffa::view::OwnedView<
    __buffa::view::SnoozeAlarmResponseView<'static>,
>;
///Shorthand for `OwnedView<StopAlarmRequestView<'static>>`.
pub type OwnedStopAlarmRequestView = ::buffa::view::OwnedView<
    __buffa::view::StopAlarmRequestView<'static>,
>;
///Shorthand for `OwnedView<StopAlarmResponseView<'static>>`.
pub type OwnedStopAlarmResponseView = ::buffa::view::OwnedView<
    __buffa::view::StopAlarmResponseView<'static>,
>;
impl ::connectrpc::Encodable<CreateAlarmResponse>
for __buffa::view::CreateAlarmResponseView<'_> {
    fn encode(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::buffa::bytes::Bytes, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body(self, codec)
    }
}
impl ::connectrpc::Encodable<CreateAlarmResponse>
for ::buffa::view::OwnedView<__buffa::view::CreateAlarmResponseView<'static>> {
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
impl ::connectrpc::Encodable<GetAlarmResponse>
for __buffa::view::GetAlarmResponseView<'_> {
    fn encode(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::buffa::bytes::Bytes, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body(self, codec)
    }
}
impl ::connectrpc::Encodable<GetAlarmResponse>
for ::buffa::view::OwnedView<__buffa::view::GetAlarmResponseView<'static>> {
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
impl ::connectrpc::Encodable<ListAlarmsResponse>
for __buffa::view::ListAlarmsResponseView<'_> {
    fn encode(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::buffa::bytes::Bytes, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body(self, codec)
    }
}
impl ::connectrpc::Encodable<ListAlarmsResponse>
for ::buffa::view::OwnedView<__buffa::view::ListAlarmsResponseView<'static>> {
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
impl ::connectrpc::Encodable<UpdateAlarmResponse>
for __buffa::view::UpdateAlarmResponseView<'_> {
    fn encode(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::buffa::bytes::Bytes, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body(self, codec)
    }
}
impl ::connectrpc::Encodable<UpdateAlarmResponse>
for ::buffa::view::OwnedView<__buffa::view::UpdateAlarmResponseView<'static>> {
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
impl ::connectrpc::Encodable<DeleteAlarmResponse>
for __buffa::view::DeleteAlarmResponseView<'_> {
    fn encode(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::buffa::bytes::Bytes, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body(self, codec)
    }
}
impl ::connectrpc::Encodable<DeleteAlarmResponse>
for ::buffa::view::OwnedView<__buffa::view::DeleteAlarmResponseView<'static>> {
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
impl ::connectrpc::Encodable<SetAlarmEnabledResponse>
for __buffa::view::SetAlarmEnabledResponseView<'_> {
    fn encode(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::buffa::bytes::Bytes, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body(self, codec)
    }
}
impl ::connectrpc::Encodable<SetAlarmEnabledResponse>
for ::buffa::view::OwnedView<__buffa::view::SetAlarmEnabledResponseView<'static>> {
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
impl ::connectrpc::Encodable<SnoozeAlarmResponse>
for __buffa::view::SnoozeAlarmResponseView<'_> {
    fn encode(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::buffa::bytes::Bytes, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body(self, codec)
    }
}
impl ::connectrpc::Encodable<SnoozeAlarmResponse>
for ::buffa::view::OwnedView<__buffa::view::SnoozeAlarmResponseView<'static>> {
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
impl ::connectrpc::Encodable<StopAlarmResponse>
for __buffa::view::StopAlarmResponseView<'_> {
    fn encode(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::buffa::bytes::Bytes, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body(self, codec)
    }
}
impl ::connectrpc::Encodable<StopAlarmResponse>
for ::buffa::view::OwnedView<__buffa::view::StopAlarmResponseView<'static>> {
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
pub const ALARM_SERVICE_SERVICE_NAME: &str = "ricardo.v1.AlarmService";
/// Static [`Spec`](::connectrpc::Spec) for the `CreateAlarm` RPC, as seen by the server; the generated client passes it with [`origin`](::connectrpc::Spec::origin) `Client` (compare across sides with [`Spec::same_method`](::connectrpc::Spec::same_method)).
pub const ALARM_SERVICE_CREATE_ALARM_SPEC: ::connectrpc::Spec = ::connectrpc::Spec::server(
        "/ricardo.v1.AlarmService/CreateAlarm",
        ::connectrpc::StreamType::Unary,
    )
    .with_idempotency_level(::connectrpc::IdempotencyLevel::Unknown);
/// Static [`Spec`](::connectrpc::Spec) for the `GetAlarm` RPC, as seen by the server; the generated client passes it with [`origin`](::connectrpc::Spec::origin) `Client` (compare across sides with [`Spec::same_method`](::connectrpc::Spec::same_method)).
pub const ALARM_SERVICE_GET_ALARM_SPEC: ::connectrpc::Spec = ::connectrpc::Spec::server(
        "/ricardo.v1.AlarmService/GetAlarm",
        ::connectrpc::StreamType::Unary,
    )
    .with_idempotency_level(::connectrpc::IdempotencyLevel::Unknown);
/// Static [`Spec`](::connectrpc::Spec) for the `ListAlarms` RPC, as seen by the server; the generated client passes it with [`origin`](::connectrpc::Spec::origin) `Client` (compare across sides with [`Spec::same_method`](::connectrpc::Spec::same_method)).
pub const ALARM_SERVICE_LIST_ALARMS_SPEC: ::connectrpc::Spec = ::connectrpc::Spec::server(
        "/ricardo.v1.AlarmService/ListAlarms",
        ::connectrpc::StreamType::Unary,
    )
    .with_idempotency_level(::connectrpc::IdempotencyLevel::Unknown);
/// Static [`Spec`](::connectrpc::Spec) for the `UpdateAlarm` RPC, as seen by the server; the generated client passes it with [`origin`](::connectrpc::Spec::origin) `Client` (compare across sides with [`Spec::same_method`](::connectrpc::Spec::same_method)).
pub const ALARM_SERVICE_UPDATE_ALARM_SPEC: ::connectrpc::Spec = ::connectrpc::Spec::server(
        "/ricardo.v1.AlarmService/UpdateAlarm",
        ::connectrpc::StreamType::Unary,
    )
    .with_idempotency_level(::connectrpc::IdempotencyLevel::Unknown);
/// Static [`Spec`](::connectrpc::Spec) for the `DeleteAlarm` RPC, as seen by the server; the generated client passes it with [`origin`](::connectrpc::Spec::origin) `Client` (compare across sides with [`Spec::same_method`](::connectrpc::Spec::same_method)).
pub const ALARM_SERVICE_DELETE_ALARM_SPEC: ::connectrpc::Spec = ::connectrpc::Spec::server(
        "/ricardo.v1.AlarmService/DeleteAlarm",
        ::connectrpc::StreamType::Unary,
    )
    .with_idempotency_level(::connectrpc::IdempotencyLevel::Unknown);
/// Static [`Spec`](::connectrpc::Spec) for the `SetAlarmEnabled` RPC, as seen by the server; the generated client passes it with [`origin`](::connectrpc::Spec::origin) `Client` (compare across sides with [`Spec::same_method`](::connectrpc::Spec::same_method)).
pub const ALARM_SERVICE_SET_ALARM_ENABLED_SPEC: ::connectrpc::Spec = ::connectrpc::Spec::server(
        "/ricardo.v1.AlarmService/SetAlarmEnabled",
        ::connectrpc::StreamType::Unary,
    )
    .with_idempotency_level(::connectrpc::IdempotencyLevel::Unknown);
/// Static [`Spec`](::connectrpc::Spec) for the `SnoozeAlarm` RPC, as seen by the server; the generated client passes it with [`origin`](::connectrpc::Spec::origin) `Client` (compare across sides with [`Spec::same_method`](::connectrpc::Spec::same_method)).
pub const ALARM_SERVICE_SNOOZE_ALARM_SPEC: ::connectrpc::Spec = ::connectrpc::Spec::server(
        "/ricardo.v1.AlarmService/SnoozeAlarm",
        ::connectrpc::StreamType::Unary,
    )
    .with_idempotency_level(::connectrpc::IdempotencyLevel::Unknown);
/// Static [`Spec`](::connectrpc::Spec) for the `StopAlarm` RPC, as seen by the server; the generated client passes it with [`origin`](::connectrpc::Spec::origin) `Client` (compare across sides with [`Spec::same_method`](::connectrpc::Spec::same_method)).
pub const ALARM_SERVICE_STOP_ALARM_SPEC: ::connectrpc::Spec = ::connectrpc::Spec::server(
        "/ricardo.v1.AlarmService/StopAlarm",
        ::connectrpc::StreamType::Unary,
    )
    .with_idempotency_level(::connectrpc::IdempotencyLevel::Unknown);
/// Server trait for AlarmService.
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
pub trait AlarmService: Send + Sync + 'static {
    /// Handle the CreateAlarm RPC.
    ///
    /// `'a` lets the response body borrow from `&self` (e.g. server-resident state).
    ///
    /// `request` is borrowed from the request body and is valid for the
    /// duration of the call; message fields are read directly on it
    /// (zero-copy). The response cannot borrow from `request` — use
    /// `.to_owned_message()` (or copy the specific fields) for anything
    /// returned, stored, or moved into `tokio::spawn`.
    fn create_alarm<'a>(
        &'a self,
        ctx: ::connectrpc::RequestContext,
        request: ::connectrpc::ServiceRequest<'_, CreateAlarmRequest>,
    ) -> impl ::std::future::Future<
        Output = ::connectrpc::ServiceResult<
            impl ::connectrpc::Encodable<CreateAlarmResponse> + Send + use<'a, Self>,
        >,
    > + Send;
    /// Handle the GetAlarm RPC.
    ///
    /// `'a` lets the response body borrow from `&self` (e.g. server-resident state).
    ///
    /// `request` is borrowed from the request body and is valid for the
    /// duration of the call; message fields are read directly on it
    /// (zero-copy). The response cannot borrow from `request` — use
    /// `.to_owned_message()` (or copy the specific fields) for anything
    /// returned, stored, or moved into `tokio::spawn`.
    fn get_alarm<'a>(
        &'a self,
        ctx: ::connectrpc::RequestContext,
        request: ::connectrpc::ServiceRequest<'_, GetAlarmRequest>,
    ) -> impl ::std::future::Future<
        Output = ::connectrpc::ServiceResult<
            impl ::connectrpc::Encodable<GetAlarmResponse> + Send + use<'a, Self>,
        >,
    > + Send;
    /// Handle the ListAlarms RPC.
    ///
    /// `'a` lets the response body borrow from `&self` (e.g. server-resident state).
    ///
    /// `request` is borrowed from the request body and is valid for the
    /// duration of the call; message fields are read directly on it
    /// (zero-copy). The response cannot borrow from `request` — use
    /// `.to_owned_message()` (or copy the specific fields) for anything
    /// returned, stored, or moved into `tokio::spawn`.
    fn list_alarms<'a>(
        &'a self,
        ctx: ::connectrpc::RequestContext,
        request: ::connectrpc::ServiceRequest<'_, ListAlarmsRequest>,
    ) -> impl ::std::future::Future<
        Output = ::connectrpc::ServiceResult<
            impl ::connectrpc::Encodable<ListAlarmsResponse> + Send + use<'a, Self>,
        >,
    > + Send;
    /// Handle the UpdateAlarm RPC.
    ///
    /// `'a` lets the response body borrow from `&self` (e.g. server-resident state).
    ///
    /// `request` is borrowed from the request body and is valid for the
    /// duration of the call; message fields are read directly on it
    /// (zero-copy). The response cannot borrow from `request` — use
    /// `.to_owned_message()` (or copy the specific fields) for anything
    /// returned, stored, or moved into `tokio::spawn`.
    fn update_alarm<'a>(
        &'a self,
        ctx: ::connectrpc::RequestContext,
        request: ::connectrpc::ServiceRequest<'_, UpdateAlarmRequest>,
    ) -> impl ::std::future::Future<
        Output = ::connectrpc::ServiceResult<
            impl ::connectrpc::Encodable<UpdateAlarmResponse> + Send + use<'a, Self>,
        >,
    > + Send;
    /// Handle the DeleteAlarm RPC.
    ///
    /// `'a` lets the response body borrow from `&self` (e.g. server-resident state).
    ///
    /// `request` is borrowed from the request body and is valid for the
    /// duration of the call; message fields are read directly on it
    /// (zero-copy). The response cannot borrow from `request` — use
    /// `.to_owned_message()` (or copy the specific fields) for anything
    /// returned, stored, or moved into `tokio::spawn`.
    fn delete_alarm<'a>(
        &'a self,
        ctx: ::connectrpc::RequestContext,
        request: ::connectrpc::ServiceRequest<'_, DeleteAlarmRequest>,
    ) -> impl ::std::future::Future<
        Output = ::connectrpc::ServiceResult<
            impl ::connectrpc::Encodable<DeleteAlarmResponse> + Send + use<'a, Self>,
        >,
    > + Send;
    /// Handle the SetAlarmEnabled RPC.
    ///
    /// `'a` lets the response body borrow from `&self` (e.g. server-resident state).
    ///
    /// `request` is borrowed from the request body and is valid for the
    /// duration of the call; message fields are read directly on it
    /// (zero-copy). The response cannot borrow from `request` — use
    /// `.to_owned_message()` (or copy the specific fields) for anything
    /// returned, stored, or moved into `tokio::spawn`.
    fn set_alarm_enabled<'a>(
        &'a self,
        ctx: ::connectrpc::RequestContext,
        request: ::connectrpc::ServiceRequest<'_, SetAlarmEnabledRequest>,
    ) -> impl ::std::future::Future<
        Output = ::connectrpc::ServiceResult<
            impl ::connectrpc::Encodable<SetAlarmEnabledResponse> + Send + use<'a, Self>,
        >,
    > + Send;
    /// Handle the SnoozeAlarm RPC.
    ///
    /// `'a` lets the response body borrow from `&self` (e.g. server-resident state).
    ///
    /// `request` is borrowed from the request body and is valid for the
    /// duration of the call; message fields are read directly on it
    /// (zero-copy). The response cannot borrow from `request` — use
    /// `.to_owned_message()` (or copy the specific fields) for anything
    /// returned, stored, or moved into `tokio::spawn`.
    fn snooze_alarm<'a>(
        &'a self,
        ctx: ::connectrpc::RequestContext,
        request: ::connectrpc::ServiceRequest<'_, SnoozeAlarmRequest>,
    ) -> impl ::std::future::Future<
        Output = ::connectrpc::ServiceResult<
            impl ::connectrpc::Encodable<SnoozeAlarmResponse> + Send + use<'a, Self>,
        >,
    > + Send;
    /// Handle the StopAlarm RPC.
    ///
    /// `'a` lets the response body borrow from `&self` (e.g. server-resident state).
    ///
    /// `request` is borrowed from the request body and is valid for the
    /// duration of the call; message fields are read directly on it
    /// (zero-copy). The response cannot borrow from `request` — use
    /// `.to_owned_message()` (or copy the specific fields) for anything
    /// returned, stored, or moved into `tokio::spawn`.
    fn stop_alarm<'a>(
        &'a self,
        ctx: ::connectrpc::RequestContext,
        request: ::connectrpc::ServiceRequest<'_, StopAlarmRequest>,
    ) -> impl ::std::future::Future<
        Output = ::connectrpc::ServiceResult<
            impl ::connectrpc::Encodable<StopAlarmResponse> + Send + use<'a, Self>,
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
pub trait AlarmServiceExt: AlarmService {
    /// Register this service implementation with a Router.
    ///
    /// Takes ownership of the `Arc<Self>` and returns a new Router with
    /// this service's methods registered.
    fn register(
        self: ::std::sync::Arc<Self>,
        router: ::connectrpc::Router,
    ) -> ::connectrpc::Router;
}
impl<S: AlarmService> AlarmServiceExt for S {
    fn register(
        self: ::std::sync::Arc<Self>,
        router: ::connectrpc::Router,
    ) -> ::connectrpc::Router {
        router
            .route_view(
                ALARM_SERVICE_SERVICE_NAME,
                "CreateAlarm",
                {
                    let svc = ::std::sync::Arc::clone(&self);
                    ::connectrpc::view_handler_fn(move |
                        ctx,
                        req: ::buffa::view::OwnedView<
                            __buffa::view::CreateAlarmRequestView<'static>,
                        >,
                        format|
                    {
                        let svc = ::std::sync::Arc::clone(&svc);
                        async move {
                            let sreq = ::connectrpc::ServiceRequest::<
                                CreateAlarmRequest,
                            >::from_parts(req.reborrow(), req.bytes());
                            svc.create_alarm(ctx, sreq)
                                .await?
                                .encode::<CreateAlarmResponse>(format)
                        }
                    })
                },
            )
            .with_spec(ALARM_SERVICE_CREATE_ALARM_SPEC)
            .route_view(
                ALARM_SERVICE_SERVICE_NAME,
                "GetAlarm",
                {
                    let svc = ::std::sync::Arc::clone(&self);
                    ::connectrpc::view_handler_fn(move |
                        ctx,
                        req: ::buffa::view::OwnedView<
                            __buffa::view::GetAlarmRequestView<'static>,
                        >,
                        format|
                    {
                        let svc = ::std::sync::Arc::clone(&svc);
                        async move {
                            let sreq = ::connectrpc::ServiceRequest::<
                                GetAlarmRequest,
                            >::from_parts(req.reborrow(), req.bytes());
                            svc.get_alarm(ctx, sreq)
                                .await?
                                .encode::<GetAlarmResponse>(format)
                        }
                    })
                },
            )
            .with_spec(ALARM_SERVICE_GET_ALARM_SPEC)
            .route_view(
                ALARM_SERVICE_SERVICE_NAME,
                "ListAlarms",
                {
                    let svc = ::std::sync::Arc::clone(&self);
                    ::connectrpc::view_handler_fn(move |
                        ctx,
                        req: ::buffa::view::OwnedView<
                            __buffa::view::ListAlarmsRequestView<'static>,
                        >,
                        format|
                    {
                        let svc = ::std::sync::Arc::clone(&svc);
                        async move {
                            let sreq = ::connectrpc::ServiceRequest::<
                                ListAlarmsRequest,
                            >::from_parts(req.reborrow(), req.bytes());
                            svc.list_alarms(ctx, sreq)
                                .await?
                                .encode::<ListAlarmsResponse>(format)
                        }
                    })
                },
            )
            .with_spec(ALARM_SERVICE_LIST_ALARMS_SPEC)
            .route_view(
                ALARM_SERVICE_SERVICE_NAME,
                "UpdateAlarm",
                {
                    let svc = ::std::sync::Arc::clone(&self);
                    ::connectrpc::view_handler_fn(move |
                        ctx,
                        req: ::buffa::view::OwnedView<
                            __buffa::view::UpdateAlarmRequestView<'static>,
                        >,
                        format|
                    {
                        let svc = ::std::sync::Arc::clone(&svc);
                        async move {
                            let sreq = ::connectrpc::ServiceRequest::<
                                UpdateAlarmRequest,
                            >::from_parts(req.reborrow(), req.bytes());
                            svc.update_alarm(ctx, sreq)
                                .await?
                                .encode::<UpdateAlarmResponse>(format)
                        }
                    })
                },
            )
            .with_spec(ALARM_SERVICE_UPDATE_ALARM_SPEC)
            .route_view(
                ALARM_SERVICE_SERVICE_NAME,
                "DeleteAlarm",
                {
                    let svc = ::std::sync::Arc::clone(&self);
                    ::connectrpc::view_handler_fn(move |
                        ctx,
                        req: ::buffa::view::OwnedView<
                            __buffa::view::DeleteAlarmRequestView<'static>,
                        >,
                        format|
                    {
                        let svc = ::std::sync::Arc::clone(&svc);
                        async move {
                            let sreq = ::connectrpc::ServiceRequest::<
                                DeleteAlarmRequest,
                            >::from_parts(req.reborrow(), req.bytes());
                            svc.delete_alarm(ctx, sreq)
                                .await?
                                .encode::<DeleteAlarmResponse>(format)
                        }
                    })
                },
            )
            .with_spec(ALARM_SERVICE_DELETE_ALARM_SPEC)
            .route_view(
                ALARM_SERVICE_SERVICE_NAME,
                "SetAlarmEnabled",
                {
                    let svc = ::std::sync::Arc::clone(&self);
                    ::connectrpc::view_handler_fn(move |
                        ctx,
                        req: ::buffa::view::OwnedView<
                            __buffa::view::SetAlarmEnabledRequestView<'static>,
                        >,
                        format|
                    {
                        let svc = ::std::sync::Arc::clone(&svc);
                        async move {
                            let sreq = ::connectrpc::ServiceRequest::<
                                SetAlarmEnabledRequest,
                            >::from_parts(req.reborrow(), req.bytes());
                            svc.set_alarm_enabled(ctx, sreq)
                                .await?
                                .encode::<SetAlarmEnabledResponse>(format)
                        }
                    })
                },
            )
            .with_spec(ALARM_SERVICE_SET_ALARM_ENABLED_SPEC)
            .route_view(
                ALARM_SERVICE_SERVICE_NAME,
                "SnoozeAlarm",
                {
                    let svc = ::std::sync::Arc::clone(&self);
                    ::connectrpc::view_handler_fn(move |
                        ctx,
                        req: ::buffa::view::OwnedView<
                            __buffa::view::SnoozeAlarmRequestView<'static>,
                        >,
                        format|
                    {
                        let svc = ::std::sync::Arc::clone(&svc);
                        async move {
                            let sreq = ::connectrpc::ServiceRequest::<
                                SnoozeAlarmRequest,
                            >::from_parts(req.reborrow(), req.bytes());
                            svc.snooze_alarm(ctx, sreq)
                                .await?
                                .encode::<SnoozeAlarmResponse>(format)
                        }
                    })
                },
            )
            .with_spec(ALARM_SERVICE_SNOOZE_ALARM_SPEC)
            .route_view(
                ALARM_SERVICE_SERVICE_NAME,
                "StopAlarm",
                {
                    let svc = ::std::sync::Arc::clone(&self);
                    ::connectrpc::view_handler_fn(move |
                        ctx,
                        req: ::buffa::view::OwnedView<
                            __buffa::view::StopAlarmRequestView<'static>,
                        >,
                        format|
                    {
                        let svc = ::std::sync::Arc::clone(&svc);
                        async move {
                            let sreq = ::connectrpc::ServiceRequest::<
                                StopAlarmRequest,
                            >::from_parts(req.reborrow(), req.bytes());
                            svc.stop_alarm(ctx, sreq)
                                .await?
                                .encode::<StopAlarmResponse>(format)
                        }
                    })
                },
            )
            .with_spec(ALARM_SERVICE_STOP_ALARM_SPEC)
    }
}
/// Type-inference marker used by [`Router::add_service`](::connectrpc::Router::add_service).
#[doc(hidden)]
pub struct AlarmServiceRegisterMarker;
impl<S: AlarmService> ::connectrpc::ServiceRegister<AlarmServiceRegisterMarker>
for ::std::sync::Arc<S> {
    fn register_service(self, router: ::connectrpc::Router) -> ::connectrpc::Router {
        <S as AlarmServiceExt>::register(self, router)
    }
}
/// Monomorphic dispatcher for `AlarmService`.
///
/// Unlike `.register(Router)` which type-erases each method into an `Arc<dyn ErasedHandler>` stored in a `HashMap`, this struct dispatches via a compile-time `match` on method name: no vtable, no hash lookup.
///
/// # Example
///
/// ```rust,ignore
/// use connectrpc::ConnectRpcService;
///
/// let server = AlarmServiceServer::new(MyImpl);
/// let service = ConnectRpcService::new(server);
/// // hand `service` to axum/hyper as a fallback_service
/// ```
pub struct AlarmServiceServer<T> {
    inner: ::std::sync::Arc<T>,
}
impl<T: AlarmService> AlarmServiceServer<T> {
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
impl<T> Clone for AlarmServiceServer<T> {
    fn clone(&self) -> Self {
        Self {
            inner: ::std::sync::Arc::clone(&self.inner),
        }
    }
}
impl<T: AlarmService> ::connectrpc::Dispatcher for AlarmServiceServer<T> {
    #[inline]
    fn lookup(
        &self,
        path: &str,
    ) -> Option<::connectrpc::dispatcher::codegen::MethodDescriptor> {
        let method = path.strip_prefix("ricardo.v1.AlarmService/")?;
        match method {
            "CreateAlarm" => {
                Some(
                    ::connectrpc::dispatcher::codegen::MethodDescriptor::unary(false)
                        .with_spec(ALARM_SERVICE_CREATE_ALARM_SPEC),
                )
            }
            "GetAlarm" => {
                Some(
                    ::connectrpc::dispatcher::codegen::MethodDescriptor::unary(false)
                        .with_spec(ALARM_SERVICE_GET_ALARM_SPEC),
                )
            }
            "ListAlarms" => {
                Some(
                    ::connectrpc::dispatcher::codegen::MethodDescriptor::unary(false)
                        .with_spec(ALARM_SERVICE_LIST_ALARMS_SPEC),
                )
            }
            "UpdateAlarm" => {
                Some(
                    ::connectrpc::dispatcher::codegen::MethodDescriptor::unary(false)
                        .with_spec(ALARM_SERVICE_UPDATE_ALARM_SPEC),
                )
            }
            "DeleteAlarm" => {
                Some(
                    ::connectrpc::dispatcher::codegen::MethodDescriptor::unary(false)
                        .with_spec(ALARM_SERVICE_DELETE_ALARM_SPEC),
                )
            }
            "SetAlarmEnabled" => {
                Some(
                    ::connectrpc::dispatcher::codegen::MethodDescriptor::unary(false)
                        .with_spec(ALARM_SERVICE_SET_ALARM_ENABLED_SPEC),
                )
            }
            "SnoozeAlarm" => {
                Some(
                    ::connectrpc::dispatcher::codegen::MethodDescriptor::unary(false)
                        .with_spec(ALARM_SERVICE_SNOOZE_ALARM_SPEC),
                )
            }
            "StopAlarm" => {
                Some(
                    ::connectrpc::dispatcher::codegen::MethodDescriptor::unary(false)
                        .with_spec(ALARM_SERVICE_STOP_ALARM_SPEC),
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
        let Some(method) = path.strip_prefix("ricardo.v1.AlarmService/") else {
            return ::connectrpc::dispatcher::codegen::unimplemented_unary(path);
        };
        let _ = (&ctx, &request, &format);
        match method {
            "CreateAlarm" => {
                let svc = ::std::sync::Arc::clone(&self.inner);
                Box::pin(async move {
                    let body = ::connectrpc::dispatcher::codegen::request_proto_bytes::<
                        CreateAlarmRequest,
                    >(request.encoded()?, format)?;
                    let req: __buffa::view::CreateAlarmRequestView<'_> = ::connectrpc::dispatcher::codegen::decode_borrowed_request_view(
                        &body,
                        ctx.decode_options(),
                    )?;
                    let req = ::connectrpc::ServiceRequest::<
                        CreateAlarmRequest,
                    >::from_parts(&req, &body);
                    svc.create_alarm(ctx, req)
                        .await?
                        .encode::<CreateAlarmResponse>(format)
                })
            }
            "GetAlarm" => {
                let svc = ::std::sync::Arc::clone(&self.inner);
                Box::pin(async move {
                    let body = ::connectrpc::dispatcher::codegen::request_proto_bytes::<
                        GetAlarmRequest,
                    >(request.encoded()?, format)?;
                    let req: __buffa::view::GetAlarmRequestView<'_> = ::connectrpc::dispatcher::codegen::decode_borrowed_request_view(
                        &body,
                        ctx.decode_options(),
                    )?;
                    let req = ::connectrpc::ServiceRequest::<
                        GetAlarmRequest,
                    >::from_parts(&req, &body);
                    svc.get_alarm(ctx, req).await?.encode::<GetAlarmResponse>(format)
                })
            }
            "ListAlarms" => {
                let svc = ::std::sync::Arc::clone(&self.inner);
                Box::pin(async move {
                    let body = ::connectrpc::dispatcher::codegen::request_proto_bytes::<
                        ListAlarmsRequest,
                    >(request.encoded()?, format)?;
                    let req: __buffa::view::ListAlarmsRequestView<'_> = ::connectrpc::dispatcher::codegen::decode_borrowed_request_view(
                        &body,
                        ctx.decode_options(),
                    )?;
                    let req = ::connectrpc::ServiceRequest::<
                        ListAlarmsRequest,
                    >::from_parts(&req, &body);
                    svc.list_alarms(ctx, req).await?.encode::<ListAlarmsResponse>(format)
                })
            }
            "UpdateAlarm" => {
                let svc = ::std::sync::Arc::clone(&self.inner);
                Box::pin(async move {
                    let body = ::connectrpc::dispatcher::codegen::request_proto_bytes::<
                        UpdateAlarmRequest,
                    >(request.encoded()?, format)?;
                    let req: __buffa::view::UpdateAlarmRequestView<'_> = ::connectrpc::dispatcher::codegen::decode_borrowed_request_view(
                        &body,
                        ctx.decode_options(),
                    )?;
                    let req = ::connectrpc::ServiceRequest::<
                        UpdateAlarmRequest,
                    >::from_parts(&req, &body);
                    svc.update_alarm(ctx, req)
                        .await?
                        .encode::<UpdateAlarmResponse>(format)
                })
            }
            "DeleteAlarm" => {
                let svc = ::std::sync::Arc::clone(&self.inner);
                Box::pin(async move {
                    let body = ::connectrpc::dispatcher::codegen::request_proto_bytes::<
                        DeleteAlarmRequest,
                    >(request.encoded()?, format)?;
                    let req: __buffa::view::DeleteAlarmRequestView<'_> = ::connectrpc::dispatcher::codegen::decode_borrowed_request_view(
                        &body,
                        ctx.decode_options(),
                    )?;
                    let req = ::connectrpc::ServiceRequest::<
                        DeleteAlarmRequest,
                    >::from_parts(&req, &body);
                    svc.delete_alarm(ctx, req)
                        .await?
                        .encode::<DeleteAlarmResponse>(format)
                })
            }
            "SetAlarmEnabled" => {
                let svc = ::std::sync::Arc::clone(&self.inner);
                Box::pin(async move {
                    let body = ::connectrpc::dispatcher::codegen::request_proto_bytes::<
                        SetAlarmEnabledRequest,
                    >(request.encoded()?, format)?;
                    let req: __buffa::view::SetAlarmEnabledRequestView<'_> = ::connectrpc::dispatcher::codegen::decode_borrowed_request_view(
                        &body,
                        ctx.decode_options(),
                    )?;
                    let req = ::connectrpc::ServiceRequest::<
                        SetAlarmEnabledRequest,
                    >::from_parts(&req, &body);
                    svc.set_alarm_enabled(ctx, req)
                        .await?
                        .encode::<SetAlarmEnabledResponse>(format)
                })
            }
            "SnoozeAlarm" => {
                let svc = ::std::sync::Arc::clone(&self.inner);
                Box::pin(async move {
                    let body = ::connectrpc::dispatcher::codegen::request_proto_bytes::<
                        SnoozeAlarmRequest,
                    >(request.encoded()?, format)?;
                    let req: __buffa::view::SnoozeAlarmRequestView<'_> = ::connectrpc::dispatcher::codegen::decode_borrowed_request_view(
                        &body,
                        ctx.decode_options(),
                    )?;
                    let req = ::connectrpc::ServiceRequest::<
                        SnoozeAlarmRequest,
                    >::from_parts(&req, &body);
                    svc.snooze_alarm(ctx, req)
                        .await?
                        .encode::<SnoozeAlarmResponse>(format)
                })
            }
            "StopAlarm" => {
                let svc = ::std::sync::Arc::clone(&self.inner);
                Box::pin(async move {
                    let body = ::connectrpc::dispatcher::codegen::request_proto_bytes::<
                        StopAlarmRequest,
                    >(request.encoded()?, format)?;
                    let req: __buffa::view::StopAlarmRequestView<'_> = ::connectrpc::dispatcher::codegen::decode_borrowed_request_view(
                        &body,
                        ctx.decode_options(),
                    )?;
                    let req = ::connectrpc::ServiceRequest::<
                        StopAlarmRequest,
                    >::from_parts(&req, &body);
                    svc.stop_alarm(ctx, req).await?.encode::<StopAlarmResponse>(format)
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
        let Some(method) = path.strip_prefix("ricardo.v1.AlarmService/") else {
            return ::connectrpc::dispatcher::codegen::unimplemented_streaming(path);
        };
        let _ = (&ctx, &request, &format);
        match method {
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
        let Some(method) = path.strip_prefix("ricardo.v1.AlarmService/") else {
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
        let Some(method) = path.strip_prefix("ricardo.v1.AlarmService/") else {
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
/// let client = AlarmServiceClient::new(conn, config);
/// let response = client.create_alarm(request).await?;
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
/// let client = AlarmServiceClient::new(http, config);
/// let response = client.create_alarm(request).await?;
/// ```
///
/// # Working with the response
///
/// Unary calls return [`UnaryResponse<OwnedView<FooView>>`](::connectrpc::client::UnaryResponse).
/// [`view()`](::connectrpc::client::UnaryResponse::view) borrows the response
/// message, so field access is zero-copy:
///
/// ```rust,ignore
/// let resp = client.create_alarm(request).await?;
/// let name: &str = resp.view().name;  // borrow into the response buffer
/// ```
///
/// If you need the owned struct (e.g. to store or pass by value), use
/// [`into_owned()`](::connectrpc::client::UnaryResponse::into_owned):
///
/// ```rust,ignore
/// let owned = client.create_alarm(request).await?.into_owned();
/// ```
///
/// [`into_view()`](::connectrpc::client::UnaryResponse::into_view) keeps the
/// zero-copy decoded body (an `OwnedView`) without copying; field access on it
/// goes through `.reborrow()`. Streaming responses yield one
/// [`StreamMessage`](::connectrpc::StreamMessage) per received message from
/// `.message().await` — read fields zero-copy through the generated accessor
/// methods (`msg.name()`) or `.view()`, or convert with `.to_owned_message()`.
#[derive(Clone)]
pub struct AlarmServiceClient<T> {
    transport: T,
    config: ::connectrpc::client::ClientConfig,
}
impl<T> AlarmServiceClient<T>
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
    /// Call the CreateAlarm RPC. Sends a request to /ricardo.v1.AlarmService/CreateAlarm.
    pub async fn create_alarm(
        &self,
        request: CreateAlarmRequest,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<__buffa::view::CreateAlarmResponseView<'static>>,
        >,
        ::connectrpc::ConnectError,
    > {
        self.create_alarm_with_options(
                request,
                ::connectrpc::client::CallOptions::default(),
            )
            .await
    }
    /// Call the CreateAlarm RPC with explicit per-call options. Options override [`ClientConfig`](::connectrpc::client::ClientConfig) defaults.
    pub async fn create_alarm_with_options(
        &self,
        request: CreateAlarmRequest,
        options: ::connectrpc::client::CallOptions,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<__buffa::view::CreateAlarmResponseView<'static>>,
        >,
        ::connectrpc::ConnectError,
    > {
        ::connectrpc::client::call_unary(
                &self.transport,
                &self.config,
                ALARM_SERVICE_CREATE_ALARM_SPEC
                    .with_origin(::connectrpc::SpecOrigin::Client),
                request,
                options,
            )
            .await
    }
    /// Call the GetAlarm RPC. Sends a request to /ricardo.v1.AlarmService/GetAlarm.
    pub async fn get_alarm(
        &self,
        request: GetAlarmRequest,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<__buffa::view::GetAlarmResponseView<'static>>,
        >,
        ::connectrpc::ConnectError,
    > {
        self.get_alarm_with_options(
                request,
                ::connectrpc::client::CallOptions::default(),
            )
            .await
    }
    /// Call the GetAlarm RPC with explicit per-call options. Options override [`ClientConfig`](::connectrpc::client::ClientConfig) defaults.
    pub async fn get_alarm_with_options(
        &self,
        request: GetAlarmRequest,
        options: ::connectrpc::client::CallOptions,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<__buffa::view::GetAlarmResponseView<'static>>,
        >,
        ::connectrpc::ConnectError,
    > {
        ::connectrpc::client::call_unary(
                &self.transport,
                &self.config,
                ALARM_SERVICE_GET_ALARM_SPEC
                    .with_origin(::connectrpc::SpecOrigin::Client),
                request,
                options,
            )
            .await
    }
    /// Call the ListAlarms RPC. Sends a request to /ricardo.v1.AlarmService/ListAlarms.
    pub async fn list_alarms(
        &self,
        request: ListAlarmsRequest,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<__buffa::view::ListAlarmsResponseView<'static>>,
        >,
        ::connectrpc::ConnectError,
    > {
        self.list_alarms_with_options(
                request,
                ::connectrpc::client::CallOptions::default(),
            )
            .await
    }
    /// Call the ListAlarms RPC with explicit per-call options. Options override [`ClientConfig`](::connectrpc::client::ClientConfig) defaults.
    pub async fn list_alarms_with_options(
        &self,
        request: ListAlarmsRequest,
        options: ::connectrpc::client::CallOptions,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<__buffa::view::ListAlarmsResponseView<'static>>,
        >,
        ::connectrpc::ConnectError,
    > {
        ::connectrpc::client::call_unary(
                &self.transport,
                &self.config,
                ALARM_SERVICE_LIST_ALARMS_SPEC
                    .with_origin(::connectrpc::SpecOrigin::Client),
                request,
                options,
            )
            .await
    }
    /// Call the UpdateAlarm RPC. Sends a request to /ricardo.v1.AlarmService/UpdateAlarm.
    pub async fn update_alarm(
        &self,
        request: UpdateAlarmRequest,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<__buffa::view::UpdateAlarmResponseView<'static>>,
        >,
        ::connectrpc::ConnectError,
    > {
        self.update_alarm_with_options(
                request,
                ::connectrpc::client::CallOptions::default(),
            )
            .await
    }
    /// Call the UpdateAlarm RPC with explicit per-call options. Options override [`ClientConfig`](::connectrpc::client::ClientConfig) defaults.
    pub async fn update_alarm_with_options(
        &self,
        request: UpdateAlarmRequest,
        options: ::connectrpc::client::CallOptions,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<__buffa::view::UpdateAlarmResponseView<'static>>,
        >,
        ::connectrpc::ConnectError,
    > {
        ::connectrpc::client::call_unary(
                &self.transport,
                &self.config,
                ALARM_SERVICE_UPDATE_ALARM_SPEC
                    .with_origin(::connectrpc::SpecOrigin::Client),
                request,
                options,
            )
            .await
    }
    /// Call the DeleteAlarm RPC. Sends a request to /ricardo.v1.AlarmService/DeleteAlarm.
    pub async fn delete_alarm(
        &self,
        request: DeleteAlarmRequest,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<__buffa::view::DeleteAlarmResponseView<'static>>,
        >,
        ::connectrpc::ConnectError,
    > {
        self.delete_alarm_with_options(
                request,
                ::connectrpc::client::CallOptions::default(),
            )
            .await
    }
    /// Call the DeleteAlarm RPC with explicit per-call options. Options override [`ClientConfig`](::connectrpc::client::ClientConfig) defaults.
    pub async fn delete_alarm_with_options(
        &self,
        request: DeleteAlarmRequest,
        options: ::connectrpc::client::CallOptions,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<__buffa::view::DeleteAlarmResponseView<'static>>,
        >,
        ::connectrpc::ConnectError,
    > {
        ::connectrpc::client::call_unary(
                &self.transport,
                &self.config,
                ALARM_SERVICE_DELETE_ALARM_SPEC
                    .with_origin(::connectrpc::SpecOrigin::Client),
                request,
                options,
            )
            .await
    }
    /// Call the SetAlarmEnabled RPC. Sends a request to /ricardo.v1.AlarmService/SetAlarmEnabled.
    pub async fn set_alarm_enabled(
        &self,
        request: SetAlarmEnabledRequest,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<__buffa::view::SetAlarmEnabledResponseView<'static>>,
        >,
        ::connectrpc::ConnectError,
    > {
        self.set_alarm_enabled_with_options(
                request,
                ::connectrpc::client::CallOptions::default(),
            )
            .await
    }
    /// Call the SetAlarmEnabled RPC with explicit per-call options. Options override [`ClientConfig`](::connectrpc::client::ClientConfig) defaults.
    pub async fn set_alarm_enabled_with_options(
        &self,
        request: SetAlarmEnabledRequest,
        options: ::connectrpc::client::CallOptions,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<__buffa::view::SetAlarmEnabledResponseView<'static>>,
        >,
        ::connectrpc::ConnectError,
    > {
        ::connectrpc::client::call_unary(
                &self.transport,
                &self.config,
                ALARM_SERVICE_SET_ALARM_ENABLED_SPEC
                    .with_origin(::connectrpc::SpecOrigin::Client),
                request,
                options,
            )
            .await
    }
    /// Call the SnoozeAlarm RPC. Sends a request to /ricardo.v1.AlarmService/SnoozeAlarm.
    pub async fn snooze_alarm(
        &self,
        request: SnoozeAlarmRequest,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<__buffa::view::SnoozeAlarmResponseView<'static>>,
        >,
        ::connectrpc::ConnectError,
    > {
        self.snooze_alarm_with_options(
                request,
                ::connectrpc::client::CallOptions::default(),
            )
            .await
    }
    /// Call the SnoozeAlarm RPC with explicit per-call options. Options override [`ClientConfig`](::connectrpc::client::ClientConfig) defaults.
    pub async fn snooze_alarm_with_options(
        &self,
        request: SnoozeAlarmRequest,
        options: ::connectrpc::client::CallOptions,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<__buffa::view::SnoozeAlarmResponseView<'static>>,
        >,
        ::connectrpc::ConnectError,
    > {
        ::connectrpc::client::call_unary(
                &self.transport,
                &self.config,
                ALARM_SERVICE_SNOOZE_ALARM_SPEC
                    .with_origin(::connectrpc::SpecOrigin::Client),
                request,
                options,
            )
            .await
    }
    /// Call the StopAlarm RPC. Sends a request to /ricardo.v1.AlarmService/StopAlarm.
    pub async fn stop_alarm(
        &self,
        request: StopAlarmRequest,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<__buffa::view::StopAlarmResponseView<'static>>,
        >,
        ::connectrpc::ConnectError,
    > {
        self.stop_alarm_with_options(
                request,
                ::connectrpc::client::CallOptions::default(),
            )
            .await
    }
    /// Call the StopAlarm RPC with explicit per-call options. Options override [`ClientConfig`](::connectrpc::client::ClientConfig) defaults.
    pub async fn stop_alarm_with_options(
        &self,
        request: StopAlarmRequest,
        options: ::connectrpc::client::CallOptions,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<__buffa::view::StopAlarmResponseView<'static>>,
        >,
        ::connectrpc::ConnectError,
    > {
        ::connectrpc::client::call_unary(
                &self.transport,
                &self.config,
                ALARM_SERVICE_STOP_ALARM_SPEC
                    .with_origin(::connectrpc::SpecOrigin::Client),
                request,
                options,
            )
            .await
    }
}
