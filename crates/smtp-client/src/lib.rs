#![forbid(unsafe_code)]
mod connection;
mod journal;
mod planning;
mod submission;
pub use planning::Plan;
use serde::Serialize;
pub use submission::{Submission, send_once};
use zixcel_mail_core::{Credentials, Endpoint, Error, OPERATION_TIMEOUT, Result};

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    AcceptedNotDeliveryConfirmed,
    UnknownDoNotRetry,
}
#[derive(Debug, Serialize)]
pub struct Receipt {
    pub outcome: Outcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<&'static str>,
}
/// Authenticate and NOOP only; never submit a message or silently enable a server protocol.
pub async fn check(endpoint: &Endpoint, credentials: &Credentials) -> Result<()> {
    check_with_trust(
        endpoint,
        credentials,
        &zixcel_mail_core::TlsTrust::webpki()?,
    )
    .await
}
pub async fn check_with_trust(
    endpoint: &Endpoint,
    credentials: &Credentials,
    trust: &zixcel_mail_core::TlsTrust,
) -> Result<()> {
    tokio::time::timeout(OPERATION_TIMEOUT, async {
        let mut client = connection::connect_with_trust(endpoint, credentials, trust).await?;
        client.noop().await.map_err(|_| Error("smtp_noop_failed"))?;
        use tokio::io::AsyncWriteExt;
        client
            .stream
            .shutdown()
            .await
            .map_err(|_| Error("smtp_disconnect_failed"))?;
        Ok(())
    })
    .await
    .map_err(|_| Error("smtp_timeout"))?
}
