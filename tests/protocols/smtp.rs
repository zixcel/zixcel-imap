use super::support::*;
use zixcel_mail::{
    message::Draft,
    smtp::{self, Outcome, Plan, Submission},
};
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
#[tokio::test]
async fn implicit_tls_authentication_and_noop_do_not_submit() {
    let Fixture {
        listener,
        config,
        trust,
        endpoint,
    } = Fixture::new().await;
    let server = tokio::spawn(async move {
        let mut stream = Fixture::accept(listener, config).await;
        smtp_auth(&mut stream).await;
        assert_eq!(line(&mut stream).await, "NOOP\r\n");
        reply(&mut stream, "250 OK\r\n").await;
        assert_eq!(line(&mut stream).await, "");
    });
    smtp::check_with_trust(&endpoint, &credential(), &trust)
        .await
        .unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn accepted_and_ambiguous_sends_are_permanently_non_replayable() {
    for acknowledge in [true, false] {
        let Fixture {
            listener,
            config,
            trust,
            endpoint,
        } = Fixture::new().await;
        let server = tokio::spawn(async move {
            let mut stream = Fixture::accept(listener, config).await;
            smtp_auth(&mut stream).await;
            assert!(line(&mut stream).await.starts_with("MAIL FROM:"));
            reply(&mut stream, "250 OK\r\n").await;
            assert!(line(&mut stream).await.starts_with("RCPT TO:"));
            reply(&mut stream, "250 OK\r\n").await;
            assert_eq!(line(&mut stream).await, "DATA\r\n");
            reply(&mut stream, "354 Continue\r\n").await;
            let mut count = 0;
            loop {
                let l = line(&mut stream).await;
                assert!(!l.is_empty());
                count += l.len();
                assert!(count < 8192);
                if l == ".\r\n" {
                    break;
                }
            }
            if acknowledge {
                reply(&mut stream, "250 Queued\r\n").await;
            }
        });
        let credentials = credential();
        let message = Draft {
            from: credentials.user().into(),
            to: vec!["recipient@example.org".into()],
            subject: "Synthetic".into(),
            text: "No real account".into(),
        }
        .prepare()
        .unwrap();
        let plan = Plan::new(
            endpoint.clone(),
            credentials.user().into(),
            "one-use".into(),
            &message,
            now(),
        )
        .unwrap();
        let digest = plan.digest().unwrap();
        let journal = tempfile::tempdir().unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(journal.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let request = || Submission {
            endpoint: &endpoint,
            credentials: &credentials,
            message: &message,
            plan: &plan,
            approved_digest: &digest,
            journal: journal.path(),
            trust: &trust,
        };
        let receipt = smtp::send_once(request()).await.unwrap();
        let outcome = receipt.outcome;
        assert_eq!(receipt.reason.is_none(), acknowledge);
        assert_eq!(
            outcome,
            if acknowledge {
                Outcome::AcceptedNotDeliveryConfirmed
            } else {
                Outcome::UnknownDoNotRetry
            }
        );
        server.await.unwrap();
        assert!(
            smtp::send_once(request()).await.is_err(),
            "restart/retry must not resend"
        );
        assert_eq!(std::fs::read_dir(journal.path()).unwrap().count(), 1);
    }
}
