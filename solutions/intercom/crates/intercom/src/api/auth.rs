//! Bearer authentication shared by unary and streaming RPCs.
//!
//! Two tokens are accepted: the phone bearer and the station device token.
//! The endpoint identity is taken from the matching token and inserted into
//! the request extensions; a handler never trusts an endpoint id from the
//! request body. Both comparisons run on every request, so a mismatch does not
//! reveal which token kind was closer.

use connectrpc::interceptor::StreamResponse;
use connectrpc::interceptor::{
    Interceptor, Next, NextStream, StreamRequest, UnaryRequest, UnaryResponse,
};
use connectrpc::{ConnectError, async_trait};
use home_secret::Token;
use http::header::AUTHORIZATION;

use crate::domain::{EndpointKind, Identity};

const BEARER_PREFIX: &str = "bearer ";

/// One token-to-identity pair.
#[derive(Clone)]
struct Device {
    token: Token,
    identity: Identity,
}

/// Resolves the bearer token to the phone or station identity.
#[derive(Clone)]
pub struct TokenAuth {
    phone: Device,
    station: Device,
}

impl std::fmt::Debug for TokenAuth {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TokenAuth")
            .field("phone", &self.phone.identity.endpoint_id)
            .field("station", &self.station.identity.endpoint_id)
            .finish()
    }
}

impl TokenAuth {
    pub fn new(phone_token: Token, phone_id: &str, station_token: Token, station_id: &str) -> Self {
        Self {
            phone: Device {
                token: phone_token,
                identity: Identity {
                    endpoint_id: phone_id.to_owned(),
                    kind: EndpointKind::Phone,
                },
            },
            station: Device {
                token: station_token,
                identity: Identity {
                    endpoint_id: station_id.to_owned(),
                    kind: EndpointKind::Station,
                },
            },
        }
    }

    fn identify(&self, header: Option<&http::HeaderValue>) -> Result<Identity, ConnectError> {
        let Some(header) = header.and_then(|value| value.to_str().ok()) else {
            return Err(ConnectError::unauthenticated("unauthenticated"));
        };
        let header = header.trim();
        let Some(rest) = header.get(..BEARER_PREFIX.len()) else {
            return Err(ConnectError::unauthenticated("unauthenticated"));
        };
        if !rest.eq_ignore_ascii_case(BEARER_PREFIX) {
            return Err(ConnectError::unauthenticated("unauthenticated"));
        }
        let candidate = header[BEARER_PREFIX.len()..].trim();
        if candidate.is_empty() {
            return Err(ConnectError::unauthenticated("unauthenticated"));
        }
        let is_phone = self.phone.token.ct_eq(candidate);
        let is_station = self.station.token.ct_eq(candidate);
        if is_phone {
            return Ok(self.phone.identity.clone());
        }
        if is_station {
            return Ok(self.station.identity.clone());
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
        let identity = self.identify(request.ctx.header(AUTHORIZATION))?;
        request.ctx.extensions_mut().insert(identity);
        next.run(request).await
    }

    async fn intercept_streaming(
        &self,
        mut request: StreamRequest,
        inbound: connectrpc::PayloadStream,
        next: NextStream<'_>,
    ) -> Result<StreamResponse, ConnectError> {
        let identity = self.identify(request.ctx.header(AUTHORIZATION))?;
        request.ctx.extensions_mut().insert(identity);
        next.run(request, inbound).await
    }
}
