#![forbid(unsafe_code)]
mod connection;
mod journal;
mod planning;
pub use planning::Plan;
use serde::Serialize;
use std::path::Path;
use zixcel_mail_core::{Credentials, Endpoint, Error, OPERATION_TIMEOUT, Result};
use zixcel_mail_message::Prepared;

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    AcceptedNotDeliveryConfirmed,
    UnknownDoNotRetry,
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
/// Digest approval is an input from the host's human authorization path, not credential unlock.
/// A durable attempt marker is written before connection; no automatic retry or overwrite.
pub struct Submission<'a> {
    pub endpoint: &'a Endpoint,
    pub credentials: &'a Credentials,
    pub message: &'a Prepared,
    pub plan: &'a Plan,
    pub approved_digest: &'a str,
    pub journal: &'a Path,
    pub trust: &'a zixcel_mail_core::TlsTrust,
}
pub async fn send_once(request: Submission<'_>) -> Result<Outcome> {
    let Submission {
        endpoint,
        credentials,
        message,
        plan,
        approved_digest,
        journal,
        trust,
    } = request;
    plan.verify(
        endpoint,
        credentials.user(),
        message,
        approved_digest,
        now()?,
    )?;
    journal::admit(journal, approved_digest)?;
    let effect = tokio::time::timeout(OPERATION_TIMEOUT, async {
        let mut client = connection::connect_with_trust(endpoint, credentials, trust).await?;
        plan.verify(
            endpoint,
            credentials.user(),
            message,
            approved_digest,
            now()?,
        )?;
        let message = mail_send::smtp::message::Message::new(
            message.from(),
            message.to().iter().map(String::as_str),
            message.bytes(),
        );
        client
            .send(message)
            .await
            .map_err(|_| Error("smtp_submission_failed"))
    })
    .await;
    let outcome = if matches!(effect, Ok(Ok(()))) {
        Outcome::AcceptedNotDeliveryConfirmed
    } else {
        Outcome::UnknownDoNotRetry
    };
    journal::record(journal, approved_digest, &outcome)?;
    Ok(outcome)
}
fn now() -> Result<u64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map_err(|_| Error("clock_unavailable"))
}
