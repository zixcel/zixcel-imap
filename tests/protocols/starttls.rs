use super::support::*;
use tokio::{io::BufReader, net::TcpListener};
use zixcel_mail::{core::TlsMode, smtp};
#[tokio::test]
async fn required_starttls_re_ehlo_and_authenticate_only_after_verified_tls() {
    let Fixture {
        listener,
        config,
        trust,
        mut endpoint,
    } = Fixture::new().await;
    endpoint.tls = TlsMode::StartTls;
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut plain = BufReader::new(tcp);
        reply(&mut plain, "220 localhost synthetic\r\n").await;
        assert!(line(&mut plain).await.starts_with("EHLO "));
        reply(&mut plain, "250-localhost\r\n250 STARTTLS\r\n").await;
        assert_eq!(line(&mut plain).await, "STARTTLS\r\n");
        reply(&mut plain, "220 Begin TLS\r\n").await;
        let stream = tokio_rustls::TlsAcceptor::from(config)
            .accept(plain.into_inner())
            .await
            .unwrap();
        let mut stream = BufReader::new(stream);
        smtp_ehlo_auth(&mut stream).await;
        assert_eq!(line(&mut stream).await, "NOOP\r\n");
        reply(&mut stream, "250 OK\r\n").await;
    });
    smtp::check_with_trust(&endpoint, &credential(), &trust)
        .await
        .unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn refused_starttls_drops_connection_before_auth() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = zixcel_mail::core::Endpoint {
        host: "localhost".into(),
        port: listener.local_addr().unwrap().port(),
        tls: TlsMode::StartTls,
    };
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut stream = BufReader::new(stream);
        reply(&mut stream, "220 localhost\r\n").await;
        assert!(line(&mut stream).await.starts_with("EHLO "));
        reply(&mut stream, "250 localhost\r\n").await;
        assert_eq!(line(&mut stream).await, "STARTTLS\r\n");
        reply(&mut stream, "454 Not available\r\n").await;
        assert_eq!(
            line(&mut stream).await,
            "",
            "no credentials on a refused TLS upgrade"
        );
    });
    assert!(smtp::check(&endpoint, &credential()).await.is_err());
    server.await.unwrap();
}
