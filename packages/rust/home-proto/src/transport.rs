//! HTTP/1.1 transport for the connectrpc client over the workspace `reqwest`.
//!
//! connectrpc's own `HttpClient` lives behind its `client` feature, which
//! enables `hyper/http2` and `hyper-util/http2`. Cargo (and Bazel's single
//! crate_universe) unify features across the workspace, so that feature pulled
//! `h2` and friends into every binary that uses hyper — including the station
//! firmware through reqwest (+580 KB on armhf after #147). The scheduler speaks
//! Connect over HTTP/1.1, so this transport implements
//! [`connectrpc::client::ClientTransport`] with the reqwest client the
//! workspace already builds (`http1` only) and the `client` feature stays off.

use connectrpc::client::{BoxFuture, ClientBody, ClientTransport};
use http::{Request, Response};

/// [`ClientTransport`] over a shared `reqwest::Client`.
#[derive(Clone, Debug)]
pub struct ReqwestTransport {
    client: reqwest::Client,
}

impl ReqwestTransport {
    /// Wrap an existing client (timeouts and pooling are the client's).
    #[must_use]
    pub const fn new(client: reqwest::Client) -> Self {
        Self { client }
    }

    /// A plain client. The workspace builds reqwest with rustls but without a
    /// default crypto provider, and reqwest panics ("No provider set") when it
    /// builds any client without one, even for plain HTTP; the ring provider
    /// is installed first and an earlier one is kept.
    pub fn plaintext() -> Result<Self, reqwest::Error> {
        let _already_installed = rustls::crypto::ring::default_provider().install_default();
        Ok(Self::new(reqwest::Client::builder().build()?))
    }

    /// A client that trusts the given PEM bundle. The workspace `reqwest`
    /// uses manual roots, so this bundle is what makes a private scheduler
    /// certificate usable.
    pub fn with_ca_pem(ca_pem: &[u8]) -> Result<Self, CaError> {
        let _already_installed = rustls::crypto::ring::default_provider().install_default();
        let certificates = reqwest::Certificate::from_pem_bundle(ca_pem).map_err(|_| CaError)?;
        if certificates.is_empty() {
            return Err(CaError);
        }
        let mut builder = reqwest::Client::builder();
        for certificate in certificates {
            builder = builder.add_root_certificate(certificate);
        }
        builder.build().map(Self::new).map_err(|_| CaError)
    }
}

/// The PEM bundle contained no usable certificate.
#[derive(Debug, Clone, Copy, thiserror::Error)]
#[error("invalid CA certificate bundle")]
pub struct CaError;

impl ClientTransport for ReqwestTransport {
    type ResponseBody = reqwest::Body;
    type Error = reqwest::Error;

    fn send(
        &self,
        request: Request<ClientBody>,
    ) -> BoxFuture<'static, Result<Response<Self::ResponseBody>, Self::Error>> {
        let client = self.client.clone();
        Box::pin(async move {
            let (parts, body) = request.into_parts();
            let request =
                reqwest::Request::try_from(Request::from_parts(parts, reqwest::Body::wrap(body)))?;
            let response = client.execute(request).await?;
            Ok(Response::from(response))
        })
    }
}
