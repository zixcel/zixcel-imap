//! Domain discovery and routing configuration shared by collector/operator.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use mailparse::addrparse;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DomainsConfig {
    pub version: u32,
    #[serde(default)]
    pub defaults: Defaults,
    #[serde(default)]
    pub accounts: HashMap<String, AccountDomains>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Defaults {
    #[serde(default = "default_true")]
    pub auto_subscribe: bool,
    #[serde(default = "default_move_limit")]
    pub move_limit_per_run: u32,
    #[serde(default = "default_collect_batch_size")]
    pub collect_batch_size: u32,
    #[serde(default)]
    pub exclude_domains: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AccountDomains {
    pub mailbox: String,
    #[serde(default)]
    pub collector_state: CollectorState,
    #[serde(default)]
    pub domains: HashMap<String, DomainEntry>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DomainEntry {
    pub enabled: bool,
    pub folder: String,
    pub discovered: Discovered,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Discovered {
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub count_estimate: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct CollectorState {
    pub last_uid_processed: u32,
}

impl Default for Defaults {
    fn default() -> Self {
        Self {
            auto_subscribe: true,
            move_limit_per_run: default_move_limit(),
            collect_batch_size: default_collect_batch_size(),
            exclude_domains: Vec::new(),
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_move_limit() -> u32 {
    2000
}

fn default_collect_batch_size() -> u32 {
    5000
}

pub fn ensure_account_entry<'a>(
    config: &'a mut DomainsConfig,
    account_id: &str,
    mailbox: &str,
) -> &'a mut AccountDomains {
    config
        .accounts
        .entry(account_id.to_string())
        .or_insert_with(|| AccountDomains {
            mailbox: mailbox.to_string(),
            collector_state: CollectorState::default(),
            domains: HashMap::new(),
        })
}

pub fn apply_domain_observation(
    account: &mut AccountDomains,
    domain: &str,
    seen_at: DateTime<Utc>,
) {
    let domain_key = normalize_domain(domain);
    let entry = account
        .domains
        .entry(domain_key.clone())
        .or_insert_with(|| DomainEntry {
            enabled: false,
            folder: format!("INBOX/{}", domain_key),
            discovered: Discovered {
                first_seen: seen_at,
                last_seen: seen_at,
                count_estimate: 0,
            },
        });

    entry.discovered.first_seen = entry.discovered.first_seen.min(seen_at);
    entry.discovered.last_seen = entry.discovered.last_seen.max(seen_at);
    entry.discovered.count_estimate = entry.discovered.count_estimate.saturating_add(1);
}

pub fn normalize_domain(domain: &str) -> String {
    domain.trim().trim_matches('.').to_ascii_lowercase()
}

pub fn extract_domains_from_from_header(raw: &str) -> Vec<String> {
    let mut domains = Vec::new();
    if let Ok(addrs) = addrparse(raw) {
        for addr in addrs.iter() {
            match addr {
                mailparse::MailAddr::Single(info) => {
                    if let Some(domain) = extract_domain_from_addr(&info.addr) {
                        domains.push(domain);
                    }
                }
                mailparse::MailAddr::Group(group) => {
                    for info in &group.addrs {
                        if let Some(domain) = extract_domain_from_addr(&info.addr) {
                            domains.push(domain);
                        }
                    }
                }
            }
        }
    }
    domains
}

fn extract_domain_from_addr(addr: &str) -> Option<String> {
    let parts: Vec<&str> = addr.split('@').collect();
    if parts.len() != 2 {
        return None;
    }
    let domain = normalize_domain(parts[1]);
    if domain.is_empty() {
        None
    } else {
        Some(domain)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_domains_simple() {
        let domains = extract_domains_from_from_header("Example <user@example.com>");
        assert_eq!(domains, vec!["example.com".to_string()]);
    }

    #[test]
    fn test_extract_domains_multiple() {
        let domains = extract_domains_from_from_header("a@one.test, b@two.test");
        assert_eq!(
            domains,
            vec!["one.test".to_string(), "two.test".to_string()]
        );
    }

    #[test]
    fn test_extract_domains_encoded() {
        let raw = "=?UTF-8?B?Sm9obiBEb2U=?= <john.doe@encoded.example>";
        let domains = extract_domains_from_from_header(raw);
        assert_eq!(domains, vec!["encoded.example".to_string()]);
    }

    #[test]
    fn test_apply_domain_stat_preserves_user_fields() {
        let mut account = AccountDomains {
            mailbox: "INBOX".to_string(),
            collector_state: CollectorState::default(),
            domains: HashMap::new(),
        };
        let now = Utc::now();
        apply_domain_observation(&mut account, "example.com", now);
        let entry = account.domains.get("example.com").unwrap();
        assert!(!entry.enabled);
        assert_eq!(entry.folder, "INBOX/example.com");

        apply_domain_observation(&mut account, "example.com", now);
        let entry2 = account.domains.get("example.com").unwrap();
        assert_eq!(entry2.folder, "INBOX/example.com");
        assert!(!entry2.enabled);
        assert!(entry2.discovered.count_estimate >= 2);

        let custom = account.domains.get_mut("example.com").unwrap();
        custom.enabled = true;
        custom.folder = "INBOX/Custom".to_string();

        apply_domain_observation(&mut account, "example.com", Utc::now());
        let entry3 = account.domains.get("example.com").unwrap();
        assert!(entry3.enabled);
        assert_eq!(entry3.folder, "INBOX/Custom");
    }
}
