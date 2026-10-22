use crate::{Result, field};
use zeroize::Zeroizing;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Auth {
    Password,
    Xoauth2,
}
/// Neither Debug nor Serialize: credential acquisition/storage belongs to the caller.
pub struct Credentials {
    user: String,
    secret: Zeroizing<String>,
    auth: Auth,
}
impl Credentials {
    pub fn new(user: String, secret: String, auth: Auth) -> Result<Self> {
        let secret = Zeroizing::new(secret);
        field(&user, 320)?;
        field(&secret, 16384)?;
        Ok(Self { user, secret, auth })
    }
    pub fn user(&self) -> &str {
        &self.user
    }
    pub fn secret(&self) -> &str {
        &self.secret
    }
    pub fn auth(&self) -> Auth {
        self.auth
    }
}
