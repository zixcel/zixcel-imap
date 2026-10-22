use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Error(pub &'static str);
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TlsMode {
    Implicit,
    StartTls,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
    pub tls: TlsMode,
}
impl Endpoint {
    pub fn validate(&self) -> Result<()> {
        if self.port == 0
            || self.host.is_empty()
            || self.host.len() > 253
            || !self.host.split('.').all(|l| {
                !l.is_empty()
                    && l.len() <= 63
                    && !l.starts_with('-')
                    && !l.ends_with('-')
                    && l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            })
        {
            return Err(Error("invalid_endpoint"));
        }
        Ok(())
    }
}
pub fn field(value: &str, maximum: usize) -> Result<()> {
    if value.is_empty() || value.len() > maximum || value.chars().any(char::is_control) {
        return Err(Error("invalid_field"));
    }
    Ok(())
}
