use mail_send::{Credentials as SmtpCredentials, SmtpClient};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use zixcel_mail_core::{
    Auth, Bounded, Credentials, Endpoint, Error, OPERATION_TIMEOUT, Result, TlsMode,
};
type Stream = Bounded<TlsStream<Bounded<TcpStream>>>;
pub(super) async fn connect_with_trust(
    endpoint: &Endpoint,
    credentials: &Credentials,
    trust: &zixcel_mail_core::TlsTrust,
) -> Result<SmtpClient<Stream>> {
    endpoint.validate()?;
    let tcp = zixcel_mail_core::connect(endpoint).await?;
    let (stream, needs_greeting) = match endpoint.tls {
        TlsMode::Implicit => (tcp, true),
        TlsMode::StartTls => {
            let mut plain = SmtpClient {
                stream: tcp,
                timeout: OPERATION_TIMEOUT,
            };
            if plain
                .read()
                .await
                .map_err(|_| Error("smtp_greeting_failed"))?
                .code
                != 220
            {
                return Err(Error("smtp_greeting_rejected"));
            }
            plain
                .ehlo("[127.0.0.1]")
                .await
                .map_err(|_| Error("smtp_ehlo_failed"))?;
            if plain
                .cmd(b"STARTTLS\r\n")
                .await
                .map_err(|_| Error("smtp_starttls_failed"))?
                .code
                != 220
            {
                return Err(Error("smtp_starttls_rejected"));
            }
            (plain.stream, false)
        }
    };
    let stream = zixcel_mail_core::tls_with_trust(stream, &endpoint.host, trust).await?;
    let mut client = SmtpClient {
        stream,
        timeout: OPERATION_TIMEOUT,
    };
    if needs_greeting
        && client
            .read()
            .await
            .map_err(|_| Error("smtp_greeting_failed"))?
            .code
            != 220
    {
        return Err(Error("smtp_greeting_rejected"));
    }
    // Discard all pre-TLS capabilities. Authenticate only after fresh EHLO over verified TLS.
    let capabilities = client
        .ehlo("[127.0.0.1]")
        .await
        .map_err(|_| Error("smtp_ehlo_failed"))?;
    let auth = match credentials.auth() {
        Auth::Password => SmtpCredentials::new(credentials.user(), credentials.secret()),
        Auth::Xoauth2 => SmtpCredentials::new_xoauth2(credentials.user(), credentials.secret()),
    };
    client
        .authenticate(auth, capabilities)
        .await
        .map_err(|_| Error("smtp_authentication_failed"))?;
    Ok(client)
}
