use connectrpc::client::{BoxFuture, ClientBody, ClientTransport};
use http::{Request, Response};

#[derive(Clone, Debug)]
pub struct ReqwestTransport {
    client: reqwest::Client,
}

impl ReqwestTransport {
    #[must_use]
    pub const fn new(client: reqwest::Client) -> Self {
        Self { client }
    }

    pub fn plaintext() -> Result<Self, reqwest::Error> {
        let _already_installed = rustls::crypto::ring::default_provider().install_default();
        Ok(Self::new(reqwest::Client::builder().build()?))
    }

    pub fn with_ca_pem(ca_pem: &[u8]) -> Result<Self, CaError> {
        let _already_installed = rustls::crypto::ring::default_provider().install_default();
        let certificates = reqwest::Certificate::from_pem_bundle(ca_pem).map_err(|_| CaError)?;
        if certificates.is_empty() {
            return Err(CaError);
        }
        reqwest::Client::builder()
            .tls_certs_only(certificates)
            .build()
            .map(Self::new)
            .map_err(|_| CaError)
    }
}

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
