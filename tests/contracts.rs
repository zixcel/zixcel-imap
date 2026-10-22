use zixcel_mail::{
    core::{self, Auth, Credentials, Endpoint, TlsMode},
    imap::Selection,
    message::{Draft, summarize},
    smtp::Plan,
};
fn endpoint() -> Endpoint {
    Endpoint {
        host: "smtp.example.org".into(),
        port: 587,
        tls: TlsMode::StartTls,
    }
}
fn draft() -> Draft {
    Draft {
        from: "sender@example.org".into(),
        to: vec!["receiver@example.org".into()],
        subject: "Synthetic message".into(),
        text: "Synthetic body".into(),
    }
}
#[test]
fn endpoints_and_credentials_reject_injected_controls_and_plaintext_modes() {
    for host in [
        "https://example.org",
        "x\r\nAUTH x",
        "a..b",
        "-a.example",
        "a:25",
    ] {
        let mut e = endpoint();
        e.host = host.into();
        assert!(e.validate().is_err());
    }
    assert!(Credentials::new("x\x01user=y".into(), "synthetic".into(), Auth::Xoauth2).is_err());
    assert!(Credentials::new("user".into(), "synthetic\r\nX".into(), Auth::Password).is_err());
    assert!(core::TlsTrust::private_ca(b"not a certificate").is_err());
}
#[test]
fn bounded_uid_identity_and_message_composition() {
    for uids in [vec![], vec![0], vec![1, 1], (1..=21).collect()] {
        assert!(
            Selection {
                uid_validity: 1,
                uids
            }
            .validate()
            .is_err()
        );
    }
    assert!(
        Selection {
            uid_validity: 0,
            uids: vec![1]
        }
        .validate()
        .is_err()
    );
    let message = draft().prepare().unwrap();
    let summary = summarize(message.bytes()).unwrap();
    assert_eq!(summary.subject.as_deref(), Some("Synthetic message"));
    let mut malicious = draft();
    malicious.subject = "x\r\nBcc: x@example.org".into();
    assert!(malicious.prepare().is_err());
    let mut large = draft();
    large.text = "x".repeat(core::MAX_MESSAGE);
    assert!(large.prepare().is_err());
    assert!(summarize(&vec![b'x'; core::MAX_MESSAGE + 1]).is_err());
}
#[test]
fn submission_digest_binds_account_endpoint_recipients_bytes_and_expiry() {
    let message = draft().prepare().unwrap();
    let plan = Plan::new(
        endpoint(),
        "sender@example.org".into(),
        "synthetic-request".into(),
        &message,
        100,
    )
    .unwrap();
    let hash = plan.digest().unwrap();
    assert!(
        plan.verify(&endpoint(), "sender@example.org", &message, &hash, 101)
            .is_ok()
    );
    assert!(
        plan.verify(&endpoint(), "other@example.org", &message, &hash, 101)
            .is_err()
    );
    assert!(
        plan.verify(&endpoint(), "sender@example.org", &message, &hash, 220)
            .is_err()
    );
    assert!(
        plan.verify(&endpoint(), "sender@example.org", &message, &hash, 99)
            .is_err()
    );
    let mut other = endpoint();
    other.port = 465;
    assert!(
        plan.verify(&other, "sender@example.org", &message, &hash, 101)
            .is_err()
    );
    let mut changed = draft();
    changed.text.push('x');
    let changed = changed.prepare().unwrap();
    assert!(
        plan.verify(&endpoint(), "sender@example.org", &changed, &hash, 101)
            .is_err()
    );
}
#[tokio::test]
async fn wire_limits_apply_before_unbounded_protocol_accumulation() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let (mut a, mut b) = tokio::io::duplex(32);
    b.write_all(b"123456789").await.unwrap();
    let mut limited = core::Bounded::new(&mut a, 8);
    let mut output = Vec::new();
    assert!(limited.read_to_end(&mut output).await.is_err());
    assert_eq!(output, b"12345678");
    let (a, _b) = tokio::io::duplex(32);
    let mut limited = core::Bounded::new(a, 8);
    assert!(limited.write_all(b"123456789").await.is_err());
}
#[tokio::test]
async fn imap_rejects_starttls_and_invalid_endpoint_before_authentication() {
    let credentials = Credentials::new(
        "user@example.org".into(),
        "synthetic-token".into(),
        Auth::Xoauth2,
    )
    .unwrap();
    assert!(
        zixcel_mail::imap::Client::connect(&endpoint(), &credentials)
            .await
            .is_err()
    );
}
