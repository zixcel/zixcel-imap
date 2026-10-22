use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zixcel_mail_core::{Endpoint, Error, Result, field};
use zixcel_mail_message::Prepared;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    schema: String,
    endpoint: Endpoint,
    account: String,
    request_id: String,
    from: String,
    to: Vec<String>,
    message_sha256: String,
    issued_at: u64,
    expires_at: u64,
}
fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
impl Plan {
    pub fn new(
        endpoint: Endpoint,
        account: String,
        request_id: String,
        message: &Prepared,
        now: u64,
    ) -> Result<Self> {
        endpoint.validate()?;
        field(&account, 320)?;
        field(&request_id, 128)?;
        Ok(Self {
            schema: "zixcel-mail/smtp-submission/v1".into(),
            endpoint,
            account,
            request_id,
            from: message.from().into(),
            to: message.to().to_vec(),
            message_sha256: hash(message.bytes()),
            issued_at: now,
            expires_at: now.checked_add(120).ok_or(Error("invalid_time"))?,
        })
    }
    pub fn digest(&self) -> Result<String> {
        Ok(hash(
            &serde_json::to_vec(self).map_err(|_| Error("plan_encoding_failed"))?,
        ))
    }
    pub fn verify(
        &self,
        endpoint: &Endpoint,
        account: &str,
        message: &Prepared,
        approved: &str,
        now: u64,
    ) -> Result<()> {
        let expected = Self::new(
            endpoint.clone(),
            account.into(),
            self.request_id.clone(),
            message,
            self.issued_at,
        )?;
        if &expected != self
            || now < self.issued_at
            || now >= self.expires_at
            || self.digest()? != approved
        {
            return Err(Error("submission_approval_mismatch_or_expired"));
        }
        Ok(())
    }
}
