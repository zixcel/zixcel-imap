use crate::{Auth, Credentials, Endpoint, Error, Result, Stream};
pub(super) async fn login(
    endpoint: &Endpoint,
    credentials: &Credentials,
    trust: &zixcel_mail_core::TlsTrust,
) -> Result<async_imap::Session<Stream>> {
    let tcp = zixcel_mail_core::connect(endpoint).await?;
    let stream = zixcel_mail_core::tls_with_trust(tcp, &endpoint.host, trust).await?;
    let mut client = async_imap::Client::new(stream);
    client
        .read_response()
        .await
        .map_err(|_| Error("imap_greeting_failed"))?
        .ok_or(Error("imap_greeting_missing"))?;
    match credentials.auth() {
        Auth::Password => client.login(credentials.user(), credentials.secret()).await,
        Auth::Xoauth2 => client.authenticate("XOAUTH2", OAuth(credentials)).await,
    }
    .map_err(|_| Error("imap_authentication_failed"))
}
struct OAuth<'a>(&'a Credentials);
impl async_imap::Authenticator for OAuth<'_> {
    type Response = String;
    fn process(&mut self, _: &[u8]) -> String {
        format!(
            "user={}\x01auth=Bearer {}\x01\x01",
            self.0.user(),
            self.0.secret()
        )
    }
}
