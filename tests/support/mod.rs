use std::sync::Arc;
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
};
use tokio_rustls::{
    TlsAcceptor,
    rustls::{ServerConfig, pki_types::PrivatePkcs8KeyDer},
    server::TlsStream,
};
use zixcel_mail::core::{Auth, Credentials, Endpoint, TlsMode, TlsTrust};
pub struct Fixture {
    pub listener: TcpListener,
    pub config: Arc<ServerConfig>,
    pub trust: TlsTrust,
    pub endpoint: Endpoint,
}
impl Fixture {
    pub async fn new() -> Self {
        let certified = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let trust = TlsTrust::private_ca(certified.cert.der()).unwrap();
        let provider = Arc::new(tokio_rustls::rustls::crypto::ring::default_provider());
        let config = ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(
                vec![certified.cert.der().clone()],
                PrivatePkcs8KeyDer::from(certified.signing_key.serialize_der()).into(),
            )
            .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = Endpoint {
            host: "localhost".into(),
            port: listener.local_addr().unwrap().port(),
            tls: TlsMode::Implicit,
        };
        Self {
            listener,
            config: Arc::new(config),
            trust,
            endpoint,
        }
    }
    pub async fn accept(
        listener: TcpListener,
        config: Arc<ServerConfig>,
    ) -> BufReader<TlsStream<TcpStream>> {
        let (stream, _) =
            tokio::time::timeout(std::time::Duration::from_secs(5), listener.accept())
                .await
                .unwrap()
                .unwrap();
        BufReader::new(TlsAcceptor::from(config).accept(stream).await.unwrap())
    }
}
pub fn credential() -> Credentials {
    Credentials::new(
        "synthetic@example.org".into(),
        "synthetic-never-issued-token".into(),
        Auth::Xoauth2,
    )
    .unwrap()
}
pub async fn line<S: AsyncRead + AsyncWrite + Unpin>(stream: &mut BufReader<S>) -> String {
    let mut line = String::new();
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        stream.read_line(&mut line),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(line.len() <= 8192);
    line
}
pub async fn reply<S: AsyncRead + AsyncWrite + Unpin>(stream: &mut BufReader<S>, text: &str) {
    stream.get_mut().write_all(text.as_bytes()).await.unwrap();
    stream.get_mut().flush().await.unwrap();
}
pub async fn smtp_auth<S: AsyncRead + AsyncWrite + Unpin>(stream: &mut BufReader<S>) {
    reply(stream, "220 localhost synthetic\r\n").await;
    smtp_ehlo_auth(stream).await;
}
pub async fn smtp_ehlo_auth<S: AsyncRead + AsyncWrite + Unpin>(stream: &mut BufReader<S>) {
    assert!(line(stream).await.starts_with("EHLO "));
    reply(stream, "250-localhost\r\n250 AUTH XOAUTH2\r\n").await;
    assert!(line(stream).await.starts_with("AUTH XOAUTH2 "));
    reply(stream, "235 Authentication successful\r\n").await;
}
