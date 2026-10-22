use serde::{Deserialize, Serialize};
use zixcel_mail_core::{Error, MAX_MESSAGE, Result, field};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Draft {
    pub from: String,
    pub to: Vec<String>,
    pub subject: String,
    pub text: String,
}
/// Immutable prepared bytes and envelope; compose once before approval, never after it.
pub struct Prepared {
    draft: Draft,
    bytes: Vec<u8>,
}
pub fn address(value: &str) -> Result<()> {
    field(value, 254)?;
    let (local, domain) = value.rsplit_once('@').ok_or(Error("invalid_address"))?;
    if local.is_empty()
        || local.len() > 64
        || !local
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".!#$%&'*+-/=?^_`{|}~".contains(&b))
        || local.starts_with('.')
        || local.ends_with('.')
        || local.contains("..")
    {
        return Err(Error("unsupported_address"));
    }
    zixcel_mail_core::Endpoint {
        host: domain.into(),
        port: 1,
        tls: zixcel_mail_core::TlsMode::Implicit,
    }
    .validate()
}
impl Draft {
    pub fn validate(&self) -> Result<()> {
        address(&self.from)?;
        if self.to.is_empty() || self.to.len() > 20 {
            return Err(Error("recipient_limit"));
        }
        for a in &self.to {
            address(a)?;
        }
        if self
            .to
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != self.to.len()
        {
            return Err(Error("duplicate_recipient"));
        }
        field(&self.subject, 998)?;
        if self.text.len() > MAX_MESSAGE / 4 || self.text.contains('\0') {
            return Err(Error("message_input_limit"));
        }
        Ok(())
    }
    pub fn prepare(self) -> Result<Prepared> {
        self.validate()?;
        let bytes = mail_builder::MessageBuilder::new()
            .from(self.from.clone())
            .to(self.to.clone())
            .subject(self.subject.clone())
            .text_body(self.text.clone())
            .write_to_vec()
            .map_err(|_| Error("message_encoding_failed"))?;
        if bytes.len() > MAX_MESSAGE {
            return Err(Error("message_size_limit"));
        }
        Ok(Prepared { draft: self, bytes })
    }
}
impl Prepared {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn from(&self) -> &str {
        &self.draft.from
    }
    pub fn to(&self) -> &[String] {
        &self.draft.to
    }
}
