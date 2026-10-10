//! Thin IMAP client wrapper used by collector/operator.

use std::net::TcpStream;

use anyhow::{Context, Result};
use imap::types::Mailbox;
use mailparse::{MailHeaderMap, parse_headers};
use native_tls::TlsConnector;
use tracing::{debug, warn};

use crate::accounts::{AccountConn, AuthConfig};

pub struct ImapSession {
    session: imap::Session<native_tls::TlsStream<TcpStream>>,
}

pub struct HeaderSnapshot {
    pub uid: u32,
    pub from: Vec<String>,
    pub subject: Option<String>,
    pub date: Option<String>,
}

impl ImapSession {
    pub fn connect(conn: &AccountConn, secret: &str) -> Result<Self> {
        require_verified_tls(&conn.imap.tls)?;
        let tls = TlsConnector::builder()
            .build()
            .context("build tls connector")?;
        // Complete verified implicit TLS before handing the stream to the protocol parser.
        let tcp = TcpStream::connect((conn.imap.host.as_str(), conn.imap.port))
            .context("connect IMAP socket")?;
        let stream = tls
            .connect(conn.imap.host.as_str(), tcp)
            .context("verify IMAP TLS")?;
        let mut client = imap::Client::new(stream);
        client.read_greeting().context("read IMAP greeting")?;

        let session = match &conn.auth {
            AuthConfig::Password { username, .. } => client
                .login(username, secret)
                .map_err(|(err, _)| err)
                .context("imap login")?,
            AuthConfig::Xoauth2 { username, .. } => {
                let auth = Xoauth2Authenticator::new(username, secret);
                client
                    .authenticate("XOAUTH2", &auth)
                    .map_err(|(err, _)| err)
                    .context("imap xoauth2 authenticate")?
            }
        };

        Ok(Self { session })
    }

    pub fn select(&mut self, mailbox: &str) -> Result<Mailbox> {
        let mbox = self
            .session
            .select(mailbox)
            .with_context(|| format!("select mailbox {}", mailbox))?;
        Ok(mbox)
    }

    pub fn uid_search(&mut self, query: &str) -> Result<Vec<u32>> {
        let ids = self
            .session
            .uid_search(query)
            .with_context(|| format!("uid search {}", query))?;
        let mut list: Vec<u32> = ids.into_iter().collect();
        list.sort_unstable();
        Ok(list)
    }

    pub fn uid_fetch_headers(&mut self, uids: &[u32]) -> Result<Vec<HeaderSnapshot>> {
        if uids.is_empty() {
            return Ok(Vec::new());
        }
        let set = uids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let fetches = self
            .session
            .uid_fetch(set, "BODY.PEEK[HEADER]")
            .context("uid fetch headers")?;
        let mut headers = Vec::new();
        for fetch in fetches.iter() {
            let uid = match fetch.uid {
                Some(value) => value,
                None => {
                    debug!("fetch returned no uid");
                    continue;
                }
            };
            let header_bytes = match fetch.header() {
                Some(bytes) => bytes,
                None => {
                    debug!(uid = ?fetch.uid, "fetch returned no header data");
                    continue;
                }
            };
            match parse_headers(header_bytes) {
                Ok((parsed, _)) => {
                    let from_values = parsed.get_all_values("From");
                    let subject = parsed.get_first_value("Subject");
                    let date = parsed.get_first_value("Date");
                    if from_values.is_empty() {
                        debug!(
                            uid = ?fetch.uid,
                            "header parsed but missing From"
                        );
                    }
                    headers.push(HeaderSnapshot {
                        uid,
                        from: from_values,
                        subject,
                        date,
                    });
                }
                Err(err) => {
                    debug!(
                        uid = ?fetch.uid,
                        error = ?err,
                        "header parse failed"
                    );
                }
            }
        }
        Ok(headers)
    }

    pub fn ensure_mailbox(&mut self, mailbox: &str) -> Result<()> {
        if let Err(err) = self.session.create(mailbox) {
            warn!(error = ?err, mailbox, "mailbox create failed or already exists");
        }
        Ok(())
    }

    pub fn subscribe(&mut self, mailbox: &str) -> Result<()> {
        if let Err(err) = self.session.subscribe(mailbox) {
            warn!(error = ?err, mailbox, "mailbox subscribe failed");
        }
        Ok(())
    }

    pub fn uid_move(&mut self, uids: &[u32], mailbox: &str) -> Result<()> {
        if uids.is_empty() {
            return Ok(());
        }
        let set = uids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(",");
        // MOVE failure is returned; never fall back to mailbox-wide EXPUNGE.
        self.session.uid_mv(set, mailbox).context("uid move")?;
        Ok(())
    }
}

pub struct Xoauth2Authenticator {
    username: String,
    access_token: String,
}

impl Xoauth2Authenticator {
    pub fn new(username: &str, access_token: &str) -> Self {
        Self {
            username: username.to_string(),
            access_token: access_token.to_string(),
        }
    }
}

impl imap::Authenticator for Xoauth2Authenticator {
    type Response = Vec<u8>;

    fn process(&self, _data: &[u8]) -> Vec<u8> {
        format!(
            "user={}\u{1}auth=Bearer {}\u{1}\u{1}",
            self.username, self.access_token
        )
        .into_bytes()
    }
}

fn require_verified_tls(config: &crate::accounts::TlsConfig) -> Result<()> {
    anyhow::ensure!(
        config.verify_cert,
        "IMAP certificate verification is required"
    );
    anyhow::ensure!(config.mode == "implicit", "Only implicit TLS is supported");
    Ok(())
}

#[cfg(test)]
mod security_tests {
    use super::*;
    use crate::accounts::TlsConfig;

    #[test]
    fn insecure_or_unsupported_tls_is_rejected_before_network() {
        assert!(
            require_verified_tls(&TlsConfig {
                mode: "implicit".into(),
                verify_cert: false
            })
            .is_err()
        );
        assert!(
            require_verified_tls(&TlsConfig {
                mode: "starttls".into(),
                verify_cert: true
            })
            .is_err()
        );
        assert!(
            require_verified_tls(&TlsConfig {
                mode: "implicit".into(),
                verify_cert: true
            })
            .is_ok()
        );
    }
}
