use crate::{Bounded, Endpoint, Error, MAX_WIRE, Result};
use std::sync::Arc;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpStream,
};
use tokio_rustls::{
    TlsConnector,
    client::TlsStream,
    rustls::{ClientConfig, RootCertStore, pki_types::ServerName},
};
/// Explicit caller-owned CA policy, useful for enterprise relays. Never disables verification.
pub struct TlsTrust {
    config: Arc<ClientConfig>,
}
impl TlsTrust {
    pub fn webpki() -> Result<Self> {
        Self::build(RootCertStore::from_iter(
            webpki_roots::TLS_SERVER_ROOTS.iter().cloned(),
        ))
    }
    pub fn private_ca(der: &[u8]) -> Result<Self> {
        if der.is_empty() || der.len() > 16384 {
            return Err(Error("ca_size_limit"));
        }
        let mut roots = RootCertStore::empty();
        roots
            .add(tokio_rustls::rustls::pki_types::CertificateDer::from(
                der.to_vec(),
            ))
            .map_err(|_| Error("invalid_ca_certificate"))?;
        Self::build(roots)
    }
    fn build(roots: RootCertStore) -> Result<Self> {
        let provider = Arc::new(tokio_rustls::rustls::crypto::ring::default_provider());
        let config = ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|_| Error("tls_configuration_failed"))?
            .with_root_certificates(roots)
            .with_no_client_auth();
        Ok(Self {
            config: Arc::new(config),
        })
    }
}
pub async fn connect(endpoint: &Endpoint) -> Result<Bounded<TcpStream>> {
    endpoint.validate()?;
    let stream = tokio::time::timeout(
        crate::OPERATION_TIMEOUT,
        TcpStream::connect((endpoint.host.as_str(), endpoint.port)),
    )
    .await
    .map_err(|_| Error("tcp_connection_timeout"))?
    .map_err(|_| Error("tcp_connection_failed"))?;
    Ok(Bounded::new(stream, MAX_WIRE))
}
pub async fn tls<S: AsyncRead + AsyncWrite + Unpin>(
    stream: S,
    host: &str,
) -> Result<Bounded<TlsStream<S>>> {
    tls_with_trust(stream, host, &TlsTrust::webpki()?).await
}
pub async fn tls_with_trust<S: AsyncRead + AsyncWrite + Unpin>(
    stream: S,
    host: &str,
    trust: &TlsTrust,
) -> Result<Bounded<TlsStream<S>>> {
    let name = ServerName::try_from(host.to_owned()).map_err(|_| Error("invalid_tls_name"))?;
    let connector = TlsConnector::from(trust.config.clone());
    let stream = tokio::time::timeout(crate::OPERATION_TIMEOUT, connector.connect(name, stream))
        .await
        .map_err(|_| Error("tls_handshake_timeout"))?
        .map_err(|_| Error("tls_verification_failed"))?;
    Ok(Bounded::new(stream, MAX_WIRE))
}
