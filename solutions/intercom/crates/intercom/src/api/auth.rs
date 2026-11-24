use connectrpc::interceptor::StreamResponse;
use connectrpc::interceptor::{
    Interceptor, Next, NextStream, StreamRequest, UnaryRequest, UnaryResponse,
};
use connectrpc::{ConnectError, async_trait};
use home_app_auth::{AccessClient, Module};
use home_secret::Token;
use http::header::AUTHORIZATION;
use std::sync::Arc;

use crate::domain::{EndpointKind, Identity};

fn module_forbidden() -> ConnectError {
    ConnectError::permission_denied("module_forbidden").with_detail(
        connectrpc::error::ErrorDetail {
            type_url: "home.v1.ModuleForbidden".to_owned(),
            value: None,
            debug: Some(serde_json::json!({"module": Module::IntercomCall.id()})),
        },
    )
}

#[derive(Clone)]
pub struct TokenAuth {
    access: Arc<AccessClient>,
    phone_id: String,
    station_token: Token,
    station_id: String,
}

impl std::fmt::Debug for TokenAuth {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TokenAuth")
            .field("phone", &self.phone_id)
            .field("station", &self.station_id)
            .finish()
    }
}

impl TokenAuth {
    pub fn new(
        access: Arc<AccessClient>,
        phone_id: &str,
        station_token: Token,
        station_id: &str,
    ) -> Self {
        Self {
            access,
            phone_id: phone_id.to_owned(),
            station_token,
            station_id: station_id.to_owned(),
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
        let principal = self.access.authenticate(raw_header).await;
        if let Some(principal) = principal {
            if !principal.allows(Module::IntercomCall) {
                tracing::warn!(
                    token = principal.name.as_str(),
                    module = Module::IntercomCall.id(),
                    "intercom call is not allowed for this token"
                );
                return Err(module_forbidden());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_denied_call_token_carries_the_module_detail_for_the_app() {
        let error = module_forbidden();
        assert_eq!(error.code, connectrpc::ErrorCode::PermissionDenied);
        assert_eq!(error.message.as_deref(), Some("module_forbidden"));
        let detail = error.details.first().expect("module detail");
        assert_eq!(detail.type_url, "home.v1.ModuleForbidden");
        assert_eq!(
            detail
                .debug
                .as_ref()
                .and_then(|debug| debug.get("module"))
                .and_then(serde_json::Value::as_str),
            Some("intercom_call")
        );
    }
}
