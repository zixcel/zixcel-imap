use crate::{Outcome, Plan, Receipt, connection, journal};
use std::path::Path;
use zixcel_mail_core::{Credentials, Endpoint, Error, OPERATION_TIMEOUT, Result};
use zixcel_mail_message::Prepared;
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
pub async fn send_once(request: Submission<'_>) -> Result<Receipt> {
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
    let receipt = match effect {
        Ok(Ok(())) => Receipt {
            outcome: Outcome::AcceptedNotDeliveryConfirmed,
            reason: None,
        },
        Ok(Err(e)) => Receipt {
            outcome: Outcome::UnknownDoNotRetry,
            reason: Some(e.0),
        },
        Err(_) => Receipt {
            outcome: Outcome::UnknownDoNotRetry,
            reason: Some("smtp_submission_timeout"),
        },
    };
    journal::record(journal, approved_digest, &receipt.outcome)?;
    Ok(receipt)
}
fn now() -> Result<u64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map_err(|_| Error("clock_unavailable"))
}
