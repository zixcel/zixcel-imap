use super::support::*;
use zixcel_mail::imap::{Client, Selection};
fn tag(line: &str) -> &str {
    line.split_whitespace().next().unwrap()
}
#[tokio::test]
async fn imap_reads_are_read_only_uid_bound_and_do_not_mark_seen() {
    let Fixture {
        listener,
        config,
        trust,
        endpoint,
    } = Fixture::new().await;
    let server = tokio::spawn(async move {
        let mut stream = Fixture::accept(listener, config).await;
        reply(&mut stream, "* OK Synthetic IMAP\r\n").await;
        let auth = line(&mut stream).await;
        assert!(auth.contains(" AUTHENTICATE XOAUTH2"));
        reply(&mut stream, "+ \r\n").await;
        assert!(!line(&mut stream).await.is_empty());
        reply(&mut stream, &format!("{} OK authenticated\r\n", tag(&auth))).await;
        for _ in 0..2 {
            let cmd = line(&mut stream).await;
            assert!(cmd.contains(" EXAMINE "));
            reply(
                &mut stream,
                &format!(
                    "* 1 EXISTS\r\n* OK [UIDVALIDITY 7] valid\r\n{} OK [READ-ONLY] examined\r\n",
                    tag(&cmd)
                ),
            )
            .await;
        }
        let cmd = line(&mut stream).await;
        assert!(cmd.contains("UID FETCH 9 (UID BODY.PEEK[HEADER]<0.65536>)"));
        let header = "From: sender@example.org\r\nSubject: Synthetic\r\n\r\n";
        reply(
            &mut stream,
            &format!(
                "* 1 FETCH (UID 9 BODY[HEADER]<0> {{{}}}\r\n{})\r\n{} OK fetched\r\n",
                header.len(),
                header,
                tag(&cmd)
            ),
        )
        .await;
        let cmd = line(&mut stream).await;
        assert!(cmd.contains(" LOGOUT"));
        reply(
            &mut stream,
            &format!("* BYE goodbye\r\n{} OK logout\r\n", tag(&cmd)),
        )
        .await;
    });
    let mut client = Client::connect_with_trust(&endpoint, &credential(), &trust)
        .await
        .unwrap();
    let mailbox = client.examine("INBOX").await.unwrap();
    assert_eq!(mailbox.uid_validity, 7);
    let headers = client
        .headers(
            "INBOX",
            &Selection {
                uid_validity: 7,
                uids: vec![9],
            },
        )
        .await
        .unwrap();
    assert_eq!(headers.len(), 1);
    assert_eq!(headers[0].uid, 9);
    client.logout().await.unwrap();
    server.await.unwrap();
}
#[tokio::test]
async fn untrusted_tls_is_rejected_before_any_authentication() {
    let Fixture {
        listener,
        config,
        endpoint,
        ..
    } = Fixture::new().await;
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        assert!(
            tokio_rustls::TlsAcceptor::from(config)
                .accept(stream)
                .await
                .is_err()
        );
    });
    assert!(Client::connect(&endpoint, &credential()).await.is_err());
    server.await.unwrap();
}
