use serde::{Deserialize, Serialize};
use zixcel_mail_core::{Error, Result};
#[derive(Debug, Serialize)]
pub struct Mailbox {
    pub messages: u32,
    pub uid_validity: u32,
}
#[derive(Debug, Serialize)]
pub struct Headers {
    pub uid: u32,
    pub raw: Vec<u8>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub uid_validity: u32,
    pub uids: Vec<u32>,
}
impl Selection {
    pub fn validate(&self) -> Result<()> {
        if self.uid_validity == 0
            || self.uids.is_empty()
            || self.uids.len() > 20
            || self.uids.contains(&0)
            || self
                .uids
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != self.uids.len()
        {
            return Err(Error("invalid_uid_selection"));
        }
        Ok(())
    }
}
