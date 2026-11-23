use connectrpc::interceptor::StreamResponse;
use connectrpc::interceptor::{
    Interceptor, Next, NextStream, StreamRequest, UnaryRequest, UnaryResponse,
};
use connectrpc::{ConnectError, async_trait};
use home_app_auth::{AccessCache, AccessSnapshot, Module, RefreshGate};
use home_secret::Token;
use http::header::AUTHORIZATION;

use crate::domain::{EndpointKind, Identity};

#[derive(Clone)]
pub struct TokenAuth {
    access: AccessCache,
    phone_id: String,
    station_token: Token,
    station_id: String,
    access_url: String,
    service_token: String,
    gate: std::sync::Arc<RefreshGate>,
}

impl std::fmt::Debug for TokenAuth {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TokenAuth")
            .field("phone", &self.phone_id)
            .field("access_revision", &self.access.revision())
            .field("station", &self.station_id)
            .finish()
    }
}

impl TokenAuth {
    pub fn new(
        access: AccessCache,
        phone_id: &str,
        station_token: Token,
        station_id: &str,
        access_url: &str,
        service_token: &str,
    ) -> Self {
        Self {
            access,
            phone_id: phone_id.to_owned(),
            station_token,
            station_id: station_id.to_owned(),
            access_url: access_url.to_owned(),
            service_token: service_token.to_owned(),
            gate: std::sync::Arc::new(RefreshGate::new(std::time::Duration::from_secs(5))),
        }
    }

    async fn identify(
        &self,
        raw_header: Option<&http::HeaderValue>,
    ) -> Result<Identity, ConnectError> {
        let Some(value) = raw_header.and_then(|value| value.to_str().ok()) else {
            return Err(ConnectError::unauthenticated("unauthenticated"));
        };
        let Some(candidate) = home_app_auth::bearer(value) else {
            return Err(ConnectError::unauthenticated("unauthenticated"));
        };
        let url = self.access_url.clone();
        let token = self.service_token.clone();
        let principal = home_app_auth::authenticate_with_refresh(
            &self.access,
            &self.gate,
            raw_header,
            || async move {
                if url.is_empty() {
                    return Err("access refresh is not configured".to_owned());
                }
                fetch_access(&url, &token).await
            },
        )
        .await;
        if let Some(principal) = principal {
            if !principal.allows(Module::IntercomCall) {
                tracing::warn!(
                    token = principal.name.as_str(),
                    module = Module::IntercomCall.id(),
                    "intercom call is not allowed for this token"
                );
                return Err(ConnectError::permission_denied(
                    home_app_auth::Forbidden::Module(Module::IntercomCall).body(),
                ));
            }
            tracing::debug!(
                token = principal.name.as_str(),
                modules = ?principal.module_ids(),
                endpoint = %self.phone_id,
                "intercom phone bearer accepted"
            );
            return Ok(Identity {
                endpoint_id: self.phone_id.clone(),
                kind: EndpointKind::Phone,
            });
        }
        if self.station_token.ct_eq(candidate) {
            return Ok(Identity {
                endpoint_id: self.station_id.clone(),
                kind: EndpointKind::Station,
            });
        }
        Err(ConnectError::unauthenticated("unauthenticated"))
    }
}

#[async_trait]
impl Interceptor for TokenAuth {
    async fn intercept_unary(
        &self,
        mut request: UnaryRequest,
        next: Next<'_>,
    ) -> Result<UnaryResponse, ConnectError> {
        let identity = self.identify(request.ctx.header(AUTHORIZATION)).await?;
        request.ctx.extensions_mut().insert(identity);
        next.run(request).await
    }

    async fn intercept_streaming(
        &self,
        mut request: StreamRequest,
        inbound: connectrpc::PayloadStream,
        next: NextStream<'_>,
    ) -> Result<StreamResponse, ConnectError> {
        let identity = self.identify(request.ctx.header(AUTHORIZATION)).await?;
        request.ctx.extensions_mut().insert(identity);
        next.run(request, inbound).await
    }
}

async fn fetch_access(url: &str, token: &str) -> Result<AccessSnapshot, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|error| format!("access client: {error}"))?;
    let response = client
        .get(url)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|_| "access request failed".to_owned())?;
    if !response.status().is_success() {
        return Err(format!(
            "access returned HTTP {}",
            response.status().as_u16()
        ));
    }
    let body = response
        .bytes()
        .await
        .map_err(|_| "access body".to_owned())?;
    AccessSnapshot::parse(&body).map_err(|error| error.to_string())
}
