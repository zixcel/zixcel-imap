#![forbid(unsafe_code)]
mod connection;
mod fetch;
mod model;
pub use model::{Headers, Mailbox, Selection};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use zixcel_mail_core::{
    Auth, Bounded, Credentials, Endpoint, Error, OPERATION_TIMEOUT, Result, TlsMode,
};
type Stream = Bounded<TlsStream<Bounded<TcpStream>>>;
/// Read-only session. Any failed/timeout operation drops the connection to avoid desynchronization.
pub struct Client {
    session: Option<async_imap::Session<Stream>>,
    deadline: tokio::time::Instant,
}
impl Client {
    pub async fn connect(endpoint: &Endpoint, credentials: &Credentials) -> Result<Self> {
        Self::connect_with_trust(
            endpoint,
            credentials,
            &zixcel_mail_core::TlsTrust::webpki()?,
        )
        .await
    }
    pub async fn connect_with_trust(
        endpoint: &Endpoint,
        credentials: &Credentials,
        trust: &zixcel_mail_core::TlsTrust,
    ) -> Result<Self> {
        endpoint.validate()?;
        if endpoint.tls != TlsMode::Implicit {
            return Err(Error("imap_requires_implicit_tls"));
        }
        let session = tokio::time::timeout(
            OPERATION_TIMEOUT,
            connection::login(endpoint, credentials, trust),
        )
        .await
        .map_err(|_| Error("imap_connection_timeout"))??;
        Ok(Self {
            session: Some(session),
            deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(120),
        })
    }
    pub async fn examine(&mut self, folder: &str) -> Result<Mailbox> {
        zixcel_mail_core::field(folder, 160)?;
        let mut session = self.session.take().ok_or(Error("imap_session_closed"))?;
        let result = tokio::time::timeout_at(self.operation_deadline(), async {
            let m = session
                .examine(folder)
                .await
                .map_err(|_| Error("imap_examine_failed"))?;
            Ok(Mailbox {
                messages: m.exists,
                uid_validity: m.uid_validity.ok_or(Error("uid_validity_missing"))?,
            })
        })
        .await
        .map_err(|_| Error("imap_read_timeout"))?;
        if result.is_ok() {
            self.session = Some(session);
        }
        result
    }
    pub async fn headers(&mut self, folder: &str, selection: &Selection) -> Result<Vec<Headers>> {
        zixcel_mail_core::field(folder, 160)?;
        selection.validate()?;
        let mut session = self.session.take().ok_or(Error("imap_session_closed"))?;
        let result = tokio::time::timeout_at(
            self.operation_deadline(),
            fetch::headers(&mut session, folder, selection),
        )
        .await
        .map_err(|_| Error("imap_read_timeout"))?;
        if result.is_ok() {
            self.session = Some(session);
        }
        result
    }
    pub async fn logout(mut self) -> Result<()> {
        let mut session = self.session.take().ok_or(Error("imap_session_closed"))?;
        tokio::time::timeout_at(self.operation_deadline(), session.logout())
            .await
            .map_err(|_| Error("imap_logout_timeout"))?
            .map_err(|_| Error("imap_logout_failed"))
    }
    fn operation_deadline(&self) -> tokio::time::Instant {
        self.deadline
            .min(tokio::time::Instant::now() + OPERATION_TIMEOUT)
    }
}
