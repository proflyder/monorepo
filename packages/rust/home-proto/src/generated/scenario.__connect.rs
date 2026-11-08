///Shorthand for `OwnedView<CreateScenarioRequestView<'static>>`.
pub type OwnedCreateScenarioRequestView = ::buffa::view::OwnedView<
    __buffa::view::CreateScenarioRequestView<'static>,
>;
///Shorthand for `OwnedView<CreateScenarioResponseView<'static>>`.
pub type OwnedCreateScenarioResponseView = ::buffa::view::OwnedView<
    __buffa::view::CreateScenarioResponseView<'static>,
>;
///Shorthand for `OwnedView<GetScenarioRequestView<'static>>`.
pub type OwnedGetScenarioRequestView = ::buffa::view::OwnedView<
    __buffa::view::GetScenarioRequestView<'static>,
>;
///Shorthand for `OwnedView<GetScenarioResponseView<'static>>`.
pub type OwnedGetScenarioResponseView = ::buffa::view::OwnedView<
    __buffa::view::GetScenarioResponseView<'static>,
>;
///Shorthand for `OwnedView<ListScenariosRequestView<'static>>`.
pub type OwnedListScenariosRequestView = ::buffa::view::OwnedView<
    __buffa::view::ListScenariosRequestView<'static>,
>;
///Shorthand for `OwnedView<ListScenariosResponseView<'static>>`.
pub type OwnedListScenariosResponseView = ::buffa::view::OwnedView<
    __buffa::view::ListScenariosResponseView<'static>,
>;
///Shorthand for `OwnedView<UpdateScenarioRequestView<'static>>`.
pub type OwnedUpdateScenarioRequestView = ::buffa::view::OwnedView<
    __buffa::view::UpdateScenarioRequestView<'static>,
>;
///Shorthand for `OwnedView<UpdateScenarioResponseView<'static>>`.
pub type OwnedUpdateScenarioResponseView = ::buffa::view::OwnedView<
    __buffa::view::UpdateScenarioResponseView<'static>,
>;
///Shorthand for `OwnedView<DeleteScenarioRequestView<'static>>`.
pub type OwnedDeleteScenarioRequestView = ::buffa::view::OwnedView<
    __buffa::view::DeleteScenarioRequestView<'static>,
>;
///Shorthand for `OwnedView<DeleteScenarioResponseView<'static>>`.
pub type OwnedDeleteScenarioResponseView = ::buffa::view::OwnedView<
    __buffa::view::DeleteScenarioResponseView<'static>,
>;
///Shorthand for `OwnedView<SetScenarioEnabledRequestView<'static>>`.
pub type OwnedSetScenarioEnabledRequestView = ::buffa::view::OwnedView<
    __buffa::view::SetScenarioEnabledRequestView<'static>,
>;
///Shorthand for `OwnedView<SetScenarioEnabledResponseView<'static>>`.
pub type OwnedSetScenarioEnabledResponseView = ::buffa::view::OwnedView<
    __buffa::view::SetScenarioEnabledResponseView<'static>,
>;
///Shorthand for `OwnedView<RunScenarioRequestView<'static>>`.
pub type OwnedRunScenarioRequestView = ::buffa::view::OwnedView<
    __buffa::view::RunScenarioRequestView<'static>,
>;
///Shorthand for `OwnedView<RunScenarioResponseView<'static>>`.
pub type OwnedRunScenarioResponseView = ::buffa::view::OwnedView<
    __buffa::view::RunScenarioResponseView<'static>,
>;
impl ::connectrpc::Encodable<CreateScenarioResponse>
for __buffa::view::CreateScenarioResponseView<'_> {
    fn encode(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::buffa::bytes::Bytes, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body(self, codec)
    }
}
impl ::connectrpc::Encodable<CreateScenarioResponse>
for ::buffa::view::OwnedView<__buffa::view::CreateScenarioResponseView<'static>> {
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
impl ::connectrpc::Encodable<GetScenarioResponse>
for __buffa::view::GetScenarioResponseView<'_> {
    fn encode(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::buffa::bytes::Bytes, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body(self, codec)
    }
}
impl ::connectrpc::Encodable<GetScenarioResponse>
for ::buffa::view::OwnedView<__buffa::view::GetScenarioResponseView<'static>> {
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
impl ::connectrpc::Encodable<ListScenariosResponse>
for __buffa::view::ListScenariosResponseView<'_> {
    fn encode(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::buffa::bytes::Bytes, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body(self, codec)
    }
}
impl ::connectrpc::Encodable<ListScenariosResponse>
for ::buffa::view::OwnedView<__buffa::view::ListScenariosResponseView<'static>> {
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
impl ::connectrpc::Encodable<UpdateScenarioResponse>
for __buffa::view::UpdateScenarioResponseView<'_> {
    fn encode(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::buffa::bytes::Bytes, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body(self, codec)
    }
}
impl ::connectrpc::Encodable<UpdateScenarioResponse>
for ::buffa::view::OwnedView<__buffa::view::UpdateScenarioResponseView<'static>> {
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
impl ::connectrpc::Encodable<DeleteScenarioResponse>
for __buffa::view::DeleteScenarioResponseView<'_> {
    fn encode(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::buffa::bytes::Bytes, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body(self, codec)
    }
}
impl ::connectrpc::Encodable<DeleteScenarioResponse>
for ::buffa::view::OwnedView<__buffa::view::DeleteScenarioResponseView<'static>> {
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
impl ::connectrpc::Encodable<SetScenarioEnabledResponse>
for __buffa::view::SetScenarioEnabledResponseView<'_> {
    fn encode(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::buffa::bytes::Bytes, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body(self, codec)
    }
}
impl ::connectrpc::Encodable<SetScenarioEnabledResponse>
for ::buffa::view::OwnedView<__buffa::view::SetScenarioEnabledResponseView<'static>> {
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
impl ::connectrpc::Encodable<RunScenarioResponse>
for __buffa::view::RunScenarioResponseView<'_> {
    fn encode(
        &self,
        codec: ::connectrpc::CodecFormat,
    ) -> ::std::result::Result<::buffa::bytes::Bytes, ::connectrpc::ConnectError> {
        ::connectrpc::__codegen::encode_view_body(self, codec)
    }
}
impl ::connectrpc::Encodable<RunScenarioResponse>
for ::buffa::view::OwnedView<__buffa::view::RunScenarioResponseView<'static>> {
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
pub const SCENARIO_SERVICE_SERVICE_NAME: &str = "home.v1.ScenarioService";
/// Static [`Spec`](::connectrpc::Spec) for the `CreateScenario` RPC, as seen by the server; the generated client passes it with [`origin`](::connectrpc::Spec::origin) `Client` (compare across sides with [`Spec::same_method`](::connectrpc::Spec::same_method)).
pub const SCENARIO_SERVICE_CREATE_SCENARIO_SPEC: ::connectrpc::Spec = ::connectrpc::Spec::server(
        "/home.v1.ScenarioService/CreateScenario",
        ::connectrpc::StreamType::Unary,
    )
    .with_idempotency_level(::connectrpc::IdempotencyLevel::Unknown);
/// Static [`Spec`](::connectrpc::Spec) for the `GetScenario` RPC, as seen by the server; the generated client passes it with [`origin`](::connectrpc::Spec::origin) `Client` (compare across sides with [`Spec::same_method`](::connectrpc::Spec::same_method)).
pub const SCENARIO_SERVICE_GET_SCENARIO_SPEC: ::connectrpc::Spec = ::connectrpc::Spec::server(
        "/home.v1.ScenarioService/GetScenario",
        ::connectrpc::StreamType::Unary,
    )
    .with_idempotency_level(::connectrpc::IdempotencyLevel::Unknown);
/// Static [`Spec`](::connectrpc::Spec) for the `ListScenarios` RPC, as seen by the server; the generated client passes it with [`origin`](::connectrpc::Spec::origin) `Client` (compare across sides with [`Spec::same_method`](::connectrpc::Spec::same_method)).
pub const SCENARIO_SERVICE_LIST_SCENARIOS_SPEC: ::connectrpc::Spec = ::connectrpc::Spec::server(
        "/home.v1.ScenarioService/ListScenarios",
        ::connectrpc::StreamType::Unary,
    )
    .with_idempotency_level(::connectrpc::IdempotencyLevel::Unknown);
/// Static [`Spec`](::connectrpc::Spec) for the `UpdateScenario` RPC, as seen by the server; the generated client passes it with [`origin`](::connectrpc::Spec::origin) `Client` (compare across sides with [`Spec::same_method`](::connectrpc::Spec::same_method)).
pub const SCENARIO_SERVICE_UPDATE_SCENARIO_SPEC: ::connectrpc::Spec = ::connectrpc::Spec::server(
        "/home.v1.ScenarioService/UpdateScenario",
        ::connectrpc::StreamType::Unary,
    )
    .with_idempotency_level(::connectrpc::IdempotencyLevel::Unknown);
/// Static [`Spec`](::connectrpc::Spec) for the `DeleteScenario` RPC, as seen by the server; the generated client passes it with [`origin`](::connectrpc::Spec::origin) `Client` (compare across sides with [`Spec::same_method`](::connectrpc::Spec::same_method)).
pub const SCENARIO_SERVICE_DELETE_SCENARIO_SPEC: ::connectrpc::Spec = ::connectrpc::Spec::server(
        "/home.v1.ScenarioService/DeleteScenario",
        ::connectrpc::StreamType::Unary,
    )
    .with_idempotency_level(::connectrpc::IdempotencyLevel::Unknown);
/// Static [`Spec`](::connectrpc::Spec) for the `SetScenarioEnabled` RPC, as seen by the server; the generated client passes it with [`origin`](::connectrpc::Spec::origin) `Client` (compare across sides with [`Spec::same_method`](::connectrpc::Spec::same_method)).
pub const SCENARIO_SERVICE_SET_SCENARIO_ENABLED_SPEC: ::connectrpc::Spec = ::connectrpc::Spec::server(
        "/home.v1.ScenarioService/SetScenarioEnabled",
        ::connectrpc::StreamType::Unary,
    )
    .with_idempotency_level(::connectrpc::IdempotencyLevel::Unknown);
/// Static [`Spec`](::connectrpc::Spec) for the `RunScenario` RPC, as seen by the server; the generated client passes it with [`origin`](::connectrpc::Spec::origin) `Client` (compare across sides with [`Spec::same_method`](::connectrpc::Spec::same_method)).
pub const SCENARIO_SERVICE_RUN_SCENARIO_SPEC: ::connectrpc::Spec = ::connectrpc::Spec::server(
        "/home.v1.ScenarioService/RunScenario",
        ::connectrpc::StreamType::Unary,
    )
    .with_idempotency_level(::connectrpc::IdempotencyLevel::Unknown);
/// Server trait for ScenarioService.
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
pub trait ScenarioService: Send + Sync + 'static {
    /// Handle the CreateScenario RPC.
    ///
    /// `'a` lets the response body borrow from `&self` (e.g. server-resident state).
    ///
    /// `request` is borrowed from the request body and is valid for the
    /// duration of the call; message fields are read directly on it
    /// (zero-copy). The response cannot borrow from `request` — use
    /// `.to_owned_message()` (or copy the specific fields) for anything
    /// returned, stored, or moved into `tokio::spawn`.
    fn create_scenario<'a>(
        &'a self,
        ctx: ::connectrpc::RequestContext,
        request: ::connectrpc::ServiceRequest<'_, CreateScenarioRequest>,
    ) -> impl ::std::future::Future<
        Output = ::connectrpc::ServiceResult<
            impl ::connectrpc::Encodable<CreateScenarioResponse> + Send + use<'a, Self>,
        >,
    > + Send;
    /// Handle the GetScenario RPC.
    ///
    /// `'a` lets the response body borrow from `&self` (e.g. server-resident state).
    ///
    /// `request` is borrowed from the request body and is valid for the
    /// duration of the call; message fields are read directly on it
    /// (zero-copy). The response cannot borrow from `request` — use
    /// `.to_owned_message()` (or copy the specific fields) for anything
    /// returned, stored, or moved into `tokio::spawn`.
    fn get_scenario<'a>(
        &'a self,
        ctx: ::connectrpc::RequestContext,
        request: ::connectrpc::ServiceRequest<'_, GetScenarioRequest>,
    ) -> impl ::std::future::Future<
        Output = ::connectrpc::ServiceResult<
            impl ::connectrpc::Encodable<GetScenarioResponse> + Send + use<'a, Self>,
        >,
    > + Send;
    /// Handle the ListScenarios RPC.
    ///
    /// `'a` lets the response body borrow from `&self` (e.g. server-resident state).
    ///
    /// `request` is borrowed from the request body and is valid for the
    /// duration of the call; message fields are read directly on it
    /// (zero-copy). The response cannot borrow from `request` — use
    /// `.to_owned_message()` (or copy the specific fields) for anything
    /// returned, stored, or moved into `tokio::spawn`.
    fn list_scenarios<'a>(
        &'a self,
        ctx: ::connectrpc::RequestContext,
        request: ::connectrpc::ServiceRequest<'_, ListScenariosRequest>,
    ) -> impl ::std::future::Future<
        Output = ::connectrpc::ServiceResult<
            impl ::connectrpc::Encodable<ListScenariosResponse> + Send + use<'a, Self>,
        >,
    > + Send;
    /// Handle the UpdateScenario RPC.
    ///
    /// `'a` lets the response body borrow from `&self` (e.g. server-resident state).
    ///
    /// `request` is borrowed from the request body and is valid for the
    /// duration of the call; message fields are read directly on it
    /// (zero-copy). The response cannot borrow from `request` — use
    /// `.to_owned_message()` (or copy the specific fields) for anything
    /// returned, stored, or moved into `tokio::spawn`.
    fn update_scenario<'a>(
        &'a self,
        ctx: ::connectrpc::RequestContext,
        request: ::connectrpc::ServiceRequest<'_, UpdateScenarioRequest>,
    ) -> impl ::std::future::Future<
        Output = ::connectrpc::ServiceResult<
            impl ::connectrpc::Encodable<UpdateScenarioResponse> + Send + use<'a, Self>,
        >,
    > + Send;
    /// Handle the DeleteScenario RPC.
    ///
    /// `'a` lets the response body borrow from `&self` (e.g. server-resident state).
    ///
    /// `request` is borrowed from the request body and is valid for the
    /// duration of the call; message fields are read directly on it
    /// (zero-copy). The response cannot borrow from `request` — use
    /// `.to_owned_message()` (or copy the specific fields) for anything
    /// returned, stored, or moved into `tokio::spawn`.
    fn delete_scenario<'a>(
        &'a self,
        ctx: ::connectrpc::RequestContext,
        request: ::connectrpc::ServiceRequest<'_, DeleteScenarioRequest>,
    ) -> impl ::std::future::Future<
        Output = ::connectrpc::ServiceResult<
            impl ::connectrpc::Encodable<DeleteScenarioResponse> + Send + use<'a, Self>,
        >,
    > + Send;
    /// Handle the SetScenarioEnabled RPC.
    ///
    /// `'a` lets the response body borrow from `&self` (e.g. server-resident state).
    ///
    /// `request` is borrowed from the request body and is valid for the
    /// duration of the call; message fields are read directly on it
    /// (zero-copy). The response cannot borrow from `request` — use
    /// `.to_owned_message()` (or copy the specific fields) for anything
    /// returned, stored, or moved into `tokio::spawn`.
    fn set_scenario_enabled<'a>(
        &'a self,
        ctx: ::connectrpc::RequestContext,
        request: ::connectrpc::ServiceRequest<'_, SetScenarioEnabledRequest>,
    ) -> impl ::std::future::Future<
        Output = ::connectrpc::ServiceResult<
            impl ::connectrpc::Encodable<
                SetScenarioEnabledResponse,
            > + Send + use<'a, Self>,
        >,
    > + Send;
    /// Handle the RunScenario RPC.
    ///
    /// `'a` lets the response body borrow from `&self` (e.g. server-resident state).
    ///
    /// `request` is borrowed from the request body and is valid for the
    /// duration of the call; message fields are read directly on it
    /// (zero-copy). The response cannot borrow from `request` — use
    /// `.to_owned_message()` (or copy the specific fields) for anything
    /// returned, stored, or moved into `tokio::spawn`.
    fn run_scenario<'a>(
        &'a self,
        ctx: ::connectrpc::RequestContext,
        request: ::connectrpc::ServiceRequest<'_, RunScenarioRequest>,
    ) -> impl ::std::future::Future<
        Output = ::connectrpc::ServiceResult<
            impl ::connectrpc::Encodable<RunScenarioResponse> + Send + use<'a, Self>,
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
pub trait ScenarioServiceExt: ScenarioService {
    /// Register this service implementation with a Router.
    ///
    /// Takes ownership of the `Arc<Self>` and returns a new Router with
    /// this service's methods registered.
    fn register(
        self: ::std::sync::Arc<Self>,
        router: ::connectrpc::Router,
    ) -> ::connectrpc::Router;
}
impl<S: ScenarioService> ScenarioServiceExt for S {
    fn register(
        self: ::std::sync::Arc<Self>,
        router: ::connectrpc::Router,
    ) -> ::connectrpc::Router {
        router
            .route_view(
                SCENARIO_SERVICE_SERVICE_NAME,
                "CreateScenario",
                {
                    let svc = ::std::sync::Arc::clone(&self);
                    ::connectrpc::view_handler_fn(move |
                        ctx,
                        req: ::buffa::view::OwnedView<
                            __buffa::view::CreateScenarioRequestView<'static>,
                        >,
                        format|
                    {
                        let svc = ::std::sync::Arc::clone(&svc);
                        async move {
                            let sreq = ::connectrpc::ServiceRequest::<
                                CreateScenarioRequest,
                            >::from_parts(req.reborrow(), req.bytes());
                            svc.create_scenario(ctx, sreq)
                                .await?
                                .encode::<CreateScenarioResponse>(format)
                        }
                    })
                },
            )
            .with_spec(SCENARIO_SERVICE_CREATE_SCENARIO_SPEC)
            .route_view(
                SCENARIO_SERVICE_SERVICE_NAME,
                "GetScenario",
                {
                    let svc = ::std::sync::Arc::clone(&self);
                    ::connectrpc::view_handler_fn(move |
                        ctx,
                        req: ::buffa::view::OwnedView<
                            __buffa::view::GetScenarioRequestView<'static>,
                        >,
                        format|
                    {
                        let svc = ::std::sync::Arc::clone(&svc);
                        async move {
                            let sreq = ::connectrpc::ServiceRequest::<
                                GetScenarioRequest,
                            >::from_parts(req.reborrow(), req.bytes());
                            svc.get_scenario(ctx, sreq)
                                .await?
                                .encode::<GetScenarioResponse>(format)
                        }
                    })
                },
            )
            .with_spec(SCENARIO_SERVICE_GET_SCENARIO_SPEC)
            .route_view(
                SCENARIO_SERVICE_SERVICE_NAME,
                "ListScenarios",
                {
                    let svc = ::std::sync::Arc::clone(&self);
                    ::connectrpc::view_handler_fn(move |
                        ctx,
                        req: ::buffa::view::OwnedView<
                            __buffa::view::ListScenariosRequestView<'static>,
                        >,
                        format|
                    {
                        let svc = ::std::sync::Arc::clone(&svc);
                        async move {
                            let sreq = ::connectrpc::ServiceRequest::<
                                ListScenariosRequest,
                            >::from_parts(req.reborrow(), req.bytes());
                            svc.list_scenarios(ctx, sreq)
                                .await?
                                .encode::<ListScenariosResponse>(format)
                        }
                    })
                },
            )
            .with_spec(SCENARIO_SERVICE_LIST_SCENARIOS_SPEC)
            .route_view(
                SCENARIO_SERVICE_SERVICE_NAME,
                "UpdateScenario",
                {
                    let svc = ::std::sync::Arc::clone(&self);
                    ::connectrpc::view_handler_fn(move |
                        ctx,
                        req: ::buffa::view::OwnedView<
                            __buffa::view::UpdateScenarioRequestView<'static>,
                        >,
                        format|
                    {
                        let svc = ::std::sync::Arc::clone(&svc);
                        async move {
                            let sreq = ::connectrpc::ServiceRequest::<
                                UpdateScenarioRequest,
                            >::from_parts(req.reborrow(), req.bytes());
                            svc.update_scenario(ctx, sreq)
                                .await?
                                .encode::<UpdateScenarioResponse>(format)
                        }
                    })
                },
            )
            .with_spec(SCENARIO_SERVICE_UPDATE_SCENARIO_SPEC)
            .route_view(
                SCENARIO_SERVICE_SERVICE_NAME,
                "DeleteScenario",
                {
                    let svc = ::std::sync::Arc::clone(&self);
                    ::connectrpc::view_handler_fn(move |
                        ctx,
                        req: ::buffa::view::OwnedView<
                            __buffa::view::DeleteScenarioRequestView<'static>,
                        >,
                        format|
                    {
                        let svc = ::std::sync::Arc::clone(&svc);
                        async move {
                            let sreq = ::connectrpc::ServiceRequest::<
                                DeleteScenarioRequest,
                            >::from_parts(req.reborrow(), req.bytes());
                            svc.delete_scenario(ctx, sreq)
                                .await?
                                .encode::<DeleteScenarioResponse>(format)
                        }
                    })
                },
            )
            .with_spec(SCENARIO_SERVICE_DELETE_SCENARIO_SPEC)
            .route_view(
                SCENARIO_SERVICE_SERVICE_NAME,
                "SetScenarioEnabled",
                {
                    let svc = ::std::sync::Arc::clone(&self);
                    ::connectrpc::view_handler_fn(move |
                        ctx,
                        req: ::buffa::view::OwnedView<
                            __buffa::view::SetScenarioEnabledRequestView<'static>,
                        >,
                        format|
                    {
                        let svc = ::std::sync::Arc::clone(&svc);
                        async move {
                            let sreq = ::connectrpc::ServiceRequest::<
                                SetScenarioEnabledRequest,
                            >::from_parts(req.reborrow(), req.bytes());
                            svc.set_scenario_enabled(ctx, sreq)
                                .await?
                                .encode::<SetScenarioEnabledResponse>(format)
                        }
                    })
                },
            )
            .with_spec(SCENARIO_SERVICE_SET_SCENARIO_ENABLED_SPEC)
            .route_view(
                SCENARIO_SERVICE_SERVICE_NAME,
                "RunScenario",
                {
                    let svc = ::std::sync::Arc::clone(&self);
                    ::connectrpc::view_handler_fn(move |
                        ctx,
                        req: ::buffa::view::OwnedView<
                            __buffa::view::RunScenarioRequestView<'static>,
                        >,
                        format|
                    {
                        let svc = ::std::sync::Arc::clone(&svc);
                        async move {
                            let sreq = ::connectrpc::ServiceRequest::<
                                RunScenarioRequest,
                            >::from_parts(req.reborrow(), req.bytes());
                            svc.run_scenario(ctx, sreq)
                                .await?
                                .encode::<RunScenarioResponse>(format)
                        }
                    })
                },
            )
            .with_spec(SCENARIO_SERVICE_RUN_SCENARIO_SPEC)
    }
}
/// Type-inference marker used by [`Router::add_service`](::connectrpc::Router::add_service).
#[doc(hidden)]
pub struct ScenarioServiceRegisterMarker;
impl<S: ScenarioService> ::connectrpc::ServiceRegister<ScenarioServiceRegisterMarker>
for ::std::sync::Arc<S> {
    fn register_service(self, router: ::connectrpc::Router) -> ::connectrpc::Router {
        <S as ScenarioServiceExt>::register(self, router)
    }
}
/// Monomorphic dispatcher for `ScenarioService`.
///
/// Unlike `.register(Router)` which type-erases each method into an `Arc<dyn ErasedHandler>` stored in a `HashMap`, this struct dispatches via a compile-time `match` on method name: no vtable, no hash lookup.
///
/// # Example
///
/// ```rust,ignore
/// use connectrpc::ConnectRpcService;
///
/// let server = ScenarioServiceServer::new(MyImpl);
/// let service = ConnectRpcService::new(server);
/// // hand `service` to axum/hyper as a fallback_service
/// ```
pub struct ScenarioServiceServer<T> {
    inner: ::std::sync::Arc<T>,
}
impl<T: ScenarioService> ScenarioServiceServer<T> {
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
impl<T> Clone for ScenarioServiceServer<T> {
    fn clone(&self) -> Self {
        Self {
            inner: ::std::sync::Arc::clone(&self.inner),
        }
    }
}
impl<T: ScenarioService> ::connectrpc::Dispatcher for ScenarioServiceServer<T> {
    #[inline]
    fn lookup(
        &self,
        path: &str,
    ) -> Option<::connectrpc::dispatcher::codegen::MethodDescriptor> {
        let method = path.strip_prefix("home.v1.ScenarioService/")?;
        match method {
            "CreateScenario" => {
                Some(
                    ::connectrpc::dispatcher::codegen::MethodDescriptor::unary(false)
                        .with_spec(SCENARIO_SERVICE_CREATE_SCENARIO_SPEC),
                )
            }
            "GetScenario" => {
                Some(
                    ::connectrpc::dispatcher::codegen::MethodDescriptor::unary(false)
                        .with_spec(SCENARIO_SERVICE_GET_SCENARIO_SPEC),
                )
            }
            "ListScenarios" => {
                Some(
                    ::connectrpc::dispatcher::codegen::MethodDescriptor::unary(false)
                        .with_spec(SCENARIO_SERVICE_LIST_SCENARIOS_SPEC),
                )
            }
            "UpdateScenario" => {
                Some(
                    ::connectrpc::dispatcher::codegen::MethodDescriptor::unary(false)
                        .with_spec(SCENARIO_SERVICE_UPDATE_SCENARIO_SPEC),
                )
            }
            "DeleteScenario" => {
                Some(
                    ::connectrpc::dispatcher::codegen::MethodDescriptor::unary(false)
                        .with_spec(SCENARIO_SERVICE_DELETE_SCENARIO_SPEC),
                )
            }
            "SetScenarioEnabled" => {
                Some(
                    ::connectrpc::dispatcher::codegen::MethodDescriptor::unary(false)
                        .with_spec(SCENARIO_SERVICE_SET_SCENARIO_ENABLED_SPEC),
                )
            }
            "RunScenario" => {
                Some(
                    ::connectrpc::dispatcher::codegen::MethodDescriptor::unary(false)
                        .with_spec(SCENARIO_SERVICE_RUN_SCENARIO_SPEC),
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
        let Some(method) = path.strip_prefix("home.v1.ScenarioService/") else {
            return ::connectrpc::dispatcher::codegen::unimplemented_unary(path);
        };
        let _ = (&ctx, &request, &format);
        match method {
            "CreateScenario" => {
                let svc = ::std::sync::Arc::clone(&self.inner);
                Box::pin(async move {
                    let body = ::connectrpc::dispatcher::codegen::request_proto_bytes::<
                        CreateScenarioRequest,
                    >(request.encoded()?, format)?;
                    let req: __buffa::view::CreateScenarioRequestView<'_> = ::connectrpc::dispatcher::codegen::decode_borrowed_request_view(
                        &body,
                        ctx.decode_options(),
                    )?;
                    let req = ::connectrpc::ServiceRequest::<
                        CreateScenarioRequest,
                    >::from_parts(&req, &body);
                    svc.create_scenario(ctx, req)
                        .await?
                        .encode::<CreateScenarioResponse>(format)
                })
            }
            "GetScenario" => {
                let svc = ::std::sync::Arc::clone(&self.inner);
                Box::pin(async move {
                    let body = ::connectrpc::dispatcher::codegen::request_proto_bytes::<
                        GetScenarioRequest,
                    >(request.encoded()?, format)?;
                    let req: __buffa::view::GetScenarioRequestView<'_> = ::connectrpc::dispatcher::codegen::decode_borrowed_request_view(
                        &body,
                        ctx.decode_options(),
                    )?;
                    let req = ::connectrpc::ServiceRequest::<
                        GetScenarioRequest,
                    >::from_parts(&req, &body);
                    svc.get_scenario(ctx, req)
                        .await?
                        .encode::<GetScenarioResponse>(format)
                })
            }
            "ListScenarios" => {
                let svc = ::std::sync::Arc::clone(&self.inner);
                Box::pin(async move {
                    let body = ::connectrpc::dispatcher::codegen::request_proto_bytes::<
                        ListScenariosRequest,
                    >(request.encoded()?, format)?;
                    let req: __buffa::view::ListScenariosRequestView<'_> = ::connectrpc::dispatcher::codegen::decode_borrowed_request_view(
                        &body,
                        ctx.decode_options(),
                    )?;
                    let req = ::connectrpc::ServiceRequest::<
                        ListScenariosRequest,
                    >::from_parts(&req, &body);
                    svc.list_scenarios(ctx, req)
                        .await?
                        .encode::<ListScenariosResponse>(format)
                })
            }
            "UpdateScenario" => {
                let svc = ::std::sync::Arc::clone(&self.inner);
                Box::pin(async move {
                    let body = ::connectrpc::dispatcher::codegen::request_proto_bytes::<
                        UpdateScenarioRequest,
                    >(request.encoded()?, format)?;
                    let req: __buffa::view::UpdateScenarioRequestView<'_> = ::connectrpc::dispatcher::codegen::decode_borrowed_request_view(
                        &body,
                        ctx.decode_options(),
                    )?;
                    let req = ::connectrpc::ServiceRequest::<
                        UpdateScenarioRequest,
                    >::from_parts(&req, &body);
                    svc.update_scenario(ctx, req)
                        .await?
                        .encode::<UpdateScenarioResponse>(format)
                })
            }
            "DeleteScenario" => {
                let svc = ::std::sync::Arc::clone(&self.inner);
                Box::pin(async move {
                    let body = ::connectrpc::dispatcher::codegen::request_proto_bytes::<
                        DeleteScenarioRequest,
                    >(request.encoded()?, format)?;
                    let req: __buffa::view::DeleteScenarioRequestView<'_> = ::connectrpc::dispatcher::codegen::decode_borrowed_request_view(
                        &body,
                        ctx.decode_options(),
                    )?;
                    let req = ::connectrpc::ServiceRequest::<
                        DeleteScenarioRequest,
                    >::from_parts(&req, &body);
                    svc.delete_scenario(ctx, req)
                        .await?
                        .encode::<DeleteScenarioResponse>(format)
                })
            }
            "SetScenarioEnabled" => {
                let svc = ::std::sync::Arc::clone(&self.inner);
                Box::pin(async move {
                    let body = ::connectrpc::dispatcher::codegen::request_proto_bytes::<
                        SetScenarioEnabledRequest,
                    >(request.encoded()?, format)?;
                    let req: __buffa::view::SetScenarioEnabledRequestView<'_> = ::connectrpc::dispatcher::codegen::decode_borrowed_request_view(
                        &body,
                        ctx.decode_options(),
                    )?;
                    let req = ::connectrpc::ServiceRequest::<
                        SetScenarioEnabledRequest,
                    >::from_parts(&req, &body);
                    svc.set_scenario_enabled(ctx, req)
                        .await?
                        .encode::<SetScenarioEnabledResponse>(format)
                })
            }
            "RunScenario" => {
                let svc = ::std::sync::Arc::clone(&self.inner);
                Box::pin(async move {
                    let body = ::connectrpc::dispatcher::codegen::request_proto_bytes::<
                        RunScenarioRequest,
                    >(request.encoded()?, format)?;
                    let req: __buffa::view::RunScenarioRequestView<'_> = ::connectrpc::dispatcher::codegen::decode_borrowed_request_view(
                        &body,
                        ctx.decode_options(),
                    )?;
                    let req = ::connectrpc::ServiceRequest::<
                        RunScenarioRequest,
                    >::from_parts(&req, &body);
                    svc.run_scenario(ctx, req)
                        .await?
                        .encode::<RunScenarioResponse>(format)
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
        let Some(method) = path.strip_prefix("home.v1.ScenarioService/") else {
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
        let Some(method) = path.strip_prefix("home.v1.ScenarioService/") else {
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
        let Some(method) = path.strip_prefix("home.v1.ScenarioService/") else {
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
/// let client = ScenarioServiceClient::new(conn, config);
/// let response = client.create_scenario(request).await?;
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
/// let client = ScenarioServiceClient::new(http, config);
/// let response = client.create_scenario(request).await?;
/// ```
///
/// # Working with the response
///
/// Unary calls return [`UnaryResponse<OwnedView<FooView>>`](::connectrpc::client::UnaryResponse).
/// [`view()`](::connectrpc::client::UnaryResponse::view) borrows the response
/// message, so field access is zero-copy:
///
/// ```rust,ignore
/// let resp = client.create_scenario(request).await?;
/// let name: &str = resp.view().name;  // borrow into the response buffer
/// ```
///
/// If you need the owned struct (e.g. to store or pass by value), use
/// [`into_owned()`](::connectrpc::client::UnaryResponse::into_owned):
///
/// ```rust,ignore
/// let owned = client.create_scenario(request).await?.into_owned();
/// ```
///
/// [`into_view()`](::connectrpc::client::UnaryResponse::into_view) keeps the
/// zero-copy decoded body (an `OwnedView`) without copying; field access on it
/// goes through `.reborrow()`. Streaming responses yield one
/// [`StreamMessage`](::connectrpc::StreamMessage) per received message from
/// `.message().await` — read fields zero-copy through the generated accessor
/// methods (`msg.name()`) or `.view()`, or convert with `.to_owned_message()`.
#[derive(Clone)]
pub struct ScenarioServiceClient<T> {
    transport: T,
    config: ::connectrpc::client::ClientConfig,
}
impl<T> ScenarioServiceClient<T>
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
    /// Call the CreateScenario RPC. Sends a request to /home.v1.ScenarioService/CreateScenario.
    pub async fn create_scenario(
        &self,
        request: CreateScenarioRequest,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<__buffa::view::CreateScenarioResponseView<'static>>,
        >,
        ::connectrpc::ConnectError,
    > {
        self.create_scenario_with_options(
                request,
                ::connectrpc::client::CallOptions::default(),
            )
            .await
    }
    /// Call the CreateScenario RPC with explicit per-call options. Options override [`ClientConfig`](::connectrpc::client::ClientConfig) defaults.
    pub async fn create_scenario_with_options(
        &self,
        request: CreateScenarioRequest,
        options: ::connectrpc::client::CallOptions,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<__buffa::view::CreateScenarioResponseView<'static>>,
        >,
        ::connectrpc::ConnectError,
    > {
        ::connectrpc::client::call_unary(
                &self.transport,
                &self.config,
                SCENARIO_SERVICE_CREATE_SCENARIO_SPEC
                    .with_origin(::connectrpc::SpecOrigin::Client),
                request,
                options,
            )
            .await
    }
    /// Call the GetScenario RPC. Sends a request to /home.v1.ScenarioService/GetScenario.
    pub async fn get_scenario(
        &self,
        request: GetScenarioRequest,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<__buffa::view::GetScenarioResponseView<'static>>,
        >,
        ::connectrpc::ConnectError,
    > {
        self.get_scenario_with_options(
                request,
                ::connectrpc::client::CallOptions::default(),
            )
            .await
    }
    /// Call the GetScenario RPC with explicit per-call options. Options override [`ClientConfig`](::connectrpc::client::ClientConfig) defaults.
    pub async fn get_scenario_with_options(
        &self,
        request: GetScenarioRequest,
        options: ::connectrpc::client::CallOptions,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<__buffa::view::GetScenarioResponseView<'static>>,
        >,
        ::connectrpc::ConnectError,
    > {
        ::connectrpc::client::call_unary(
                &self.transport,
                &self.config,
                SCENARIO_SERVICE_GET_SCENARIO_SPEC
                    .with_origin(::connectrpc::SpecOrigin::Client),
                request,
                options,
            )
            .await
    }
    /// Call the ListScenarios RPC. Sends a request to /home.v1.ScenarioService/ListScenarios.
    pub async fn list_scenarios(
        &self,
        request: ListScenariosRequest,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<__buffa::view::ListScenariosResponseView<'static>>,
        >,
        ::connectrpc::ConnectError,
    > {
        self.list_scenarios_with_options(
                request,
                ::connectrpc::client::CallOptions::default(),
            )
            .await
    }
    /// Call the ListScenarios RPC with explicit per-call options. Options override [`ClientConfig`](::connectrpc::client::ClientConfig) defaults.
    pub async fn list_scenarios_with_options(
        &self,
        request: ListScenariosRequest,
        options: ::connectrpc::client::CallOptions,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<__buffa::view::ListScenariosResponseView<'static>>,
        >,
        ::connectrpc::ConnectError,
    > {
        ::connectrpc::client::call_unary(
                &self.transport,
                &self.config,
                SCENARIO_SERVICE_LIST_SCENARIOS_SPEC
                    .with_origin(::connectrpc::SpecOrigin::Client),
                request,
                options,
            )
            .await
    }
    /// Call the UpdateScenario RPC. Sends a request to /home.v1.ScenarioService/UpdateScenario.
    pub async fn update_scenario(
        &self,
        request: UpdateScenarioRequest,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<__buffa::view::UpdateScenarioResponseView<'static>>,
        >,
        ::connectrpc::ConnectError,
    > {
        self.update_scenario_with_options(
                request,
                ::connectrpc::client::CallOptions::default(),
            )
            .await
    }
    /// Call the UpdateScenario RPC with explicit per-call options. Options override [`ClientConfig`](::connectrpc::client::ClientConfig) defaults.
    pub async fn update_scenario_with_options(
        &self,
        request: UpdateScenarioRequest,
        options: ::connectrpc::client::CallOptions,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<__buffa::view::UpdateScenarioResponseView<'static>>,
        >,
        ::connectrpc::ConnectError,
    > {
        ::connectrpc::client::call_unary(
                &self.transport,
                &self.config,
                SCENARIO_SERVICE_UPDATE_SCENARIO_SPEC
                    .with_origin(::connectrpc::SpecOrigin::Client),
                request,
                options,
            )
            .await
    }
    /// Call the DeleteScenario RPC. Sends a request to /home.v1.ScenarioService/DeleteScenario.
    pub async fn delete_scenario(
        &self,
        request: DeleteScenarioRequest,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<__buffa::view::DeleteScenarioResponseView<'static>>,
        >,
        ::connectrpc::ConnectError,
    > {
        self.delete_scenario_with_options(
                request,
                ::connectrpc::client::CallOptions::default(),
            )
            .await
    }
    /// Call the DeleteScenario RPC with explicit per-call options. Options override [`ClientConfig`](::connectrpc::client::ClientConfig) defaults.
    pub async fn delete_scenario_with_options(
        &self,
        request: DeleteScenarioRequest,
        options: ::connectrpc::client::CallOptions,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<__buffa::view::DeleteScenarioResponseView<'static>>,
        >,
        ::connectrpc::ConnectError,
    > {
        ::connectrpc::client::call_unary(
                &self.transport,
                &self.config,
                SCENARIO_SERVICE_DELETE_SCENARIO_SPEC
                    .with_origin(::connectrpc::SpecOrigin::Client),
                request,
                options,
            )
            .await
    }
    /// Call the SetScenarioEnabled RPC. Sends a request to /home.v1.ScenarioService/SetScenarioEnabled.
    pub async fn set_scenario_enabled(
        &self,
        request: SetScenarioEnabledRequest,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<
                __buffa::view::SetScenarioEnabledResponseView<'static>,
            >,
        >,
        ::connectrpc::ConnectError,
    > {
        self.set_scenario_enabled_with_options(
                request,
                ::connectrpc::client::CallOptions::default(),
            )
            .await
    }
    /// Call the SetScenarioEnabled RPC with explicit per-call options. Options override [`ClientConfig`](::connectrpc::client::ClientConfig) defaults.
    pub async fn set_scenario_enabled_with_options(
        &self,
        request: SetScenarioEnabledRequest,
        options: ::connectrpc::client::CallOptions,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<
                __buffa::view::SetScenarioEnabledResponseView<'static>,
            >,
        >,
        ::connectrpc::ConnectError,
    > {
        ::connectrpc::client::call_unary(
                &self.transport,
                &self.config,
                SCENARIO_SERVICE_SET_SCENARIO_ENABLED_SPEC
                    .with_origin(::connectrpc::SpecOrigin::Client),
                request,
                options,
            )
            .await
    }
    /// Call the RunScenario RPC. Sends a request to /home.v1.ScenarioService/RunScenario.
    pub async fn run_scenario(
        &self,
        request: RunScenarioRequest,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<__buffa::view::RunScenarioResponseView<'static>>,
        >,
        ::connectrpc::ConnectError,
    > {
        self.run_scenario_with_options(
                request,
                ::connectrpc::client::CallOptions::default(),
            )
            .await
    }
    /// Call the RunScenario RPC with explicit per-call options. Options override [`ClientConfig`](::connectrpc::client::ClientConfig) defaults.
    pub async fn run_scenario_with_options(
        &self,
        request: RunScenarioRequest,
        options: ::connectrpc::client::CallOptions,
    ) -> Result<
        ::connectrpc::client::UnaryResponse<
            ::buffa::view::OwnedView<__buffa::view::RunScenarioResponseView<'static>>,
        >,
        ::connectrpc::ConnectError,
    > {
        ::connectrpc::client::call_unary(
                &self.transport,
                &self.config,
                SCENARIO_SERVICE_RUN_SCENARIO_SPEC
                    .with_origin(::connectrpc::SpecOrigin::Client),
                request,
                options,
            )
            .await
    }
}
