use crate::boundary::{secret_ref, server};

#[test]
fn server_is_a_hostname_and_credentials_remain_references() {
    assert!(server("imap.example.invalid").is_ok());
    assert!(server("https://imap.example.invalid").is_err());
    assert!(server(".invalid").is_err());
    assert!(secret_ref("secret://imap/account/observer").is_ok());
    assert!(secret_ref("password").is_err());
}
