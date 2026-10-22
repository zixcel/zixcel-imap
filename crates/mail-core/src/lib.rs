#![forbid(unsafe_code)]
mod auth;
mod io;
mod network;
mod types;
pub use auth::{Auth, Credentials};
pub use io::Bounded;
pub use network::{TlsTrust, connect, tls, tls_with_trust};
pub use types::{Endpoint, Error, Result, TlsMode, field};
pub const MAX_MESSAGE: usize = 1_048_576;
pub const MAX_WIRE: usize = 2_097_152;
pub const OPERATION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
