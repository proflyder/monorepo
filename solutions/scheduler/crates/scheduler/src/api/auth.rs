//! Bearer authentication as a Connect interceptor.
//!
//! The token is compared through fixed-size SHA-256 digests, so the comparison
//! is constant-time and the secret never enters a log line.

use connectrpc::interceptor::{UnaryRequest, UnaryResponse};
use connectrpc::{ConnectError, Interceptor, Next, async_trait};
use http::header::AUTHORIZATION;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

const BEARER_PREFIX: &str = "bearer ";

/// Rejects every RPC without a valid `Authorization: Bearer <token>` header.
pub struct BearerAuth {
    expected: [u8; 32],
}

impl std::fmt::Debug for BearerAuth {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The digest is derived from the secret and never printed.
        formatter.debug_struct("BearerAuth").finish_non_exhaustive()
    }
}

impl BearerAuth {
    pub fn new(token: &str) -> Self {
        let digest = Sha256::digest(token.as_bytes());
        let mut expected = [0_u8; 32];
        expected.copy_from_slice(&digest);
        Self { expected }
    }

    /// Whether this request carries the expected bearer token. Shared with the
    /// plain JSON push routes, which are outside the Connect interceptor.
    pub fn authorized(&self, header: Option<&http::HeaderValue>) -> bool {
        self.matches(header)
    }

    fn matches(&self, header: Option<&http::HeaderValue>) -> bool {
        let Some(header) = header.and_then(|value| value.to_str().ok()) else {
            return false;
        };
        let header = header.trim();
        if header.len() <= BEARER_PREFIX.len() {
            return false;
        }
        let (prefix, rest) = header.split_at(BEARER_PREFIX.len());
        if !prefix.eq_ignore_ascii_case(BEARER_PREFIX) {
            return false;
        }
        let provided = rest.trim();
        if provided.is_empty() {
            return false;
        }
        let digest = Sha256::digest(provided.as_bytes());
        digest.as_slice().ct_eq(&self.expected).into()
    }
}

#[async_trait]
impl Interceptor for BearerAuth {
    async fn intercept_unary(
        &self,
        request: UnaryRequest,
        next: Next<'_>,
    ) -> Result<UnaryResponse, ConnectError> {
        if !self.matches(request.ctx.header(AUTHORIZATION)) {
            return Err(ConnectError::unauthenticated("unauthenticated"));
        }
        next.run(request).await
    }
}
