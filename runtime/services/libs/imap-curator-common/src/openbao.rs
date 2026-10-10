//! OpenBao client helpers for storing secrets outside the database.

use std::env;
use std::time::Duration;

use anyhow::{Context, Result};
use reqwest::{Client, StatusCode};
use serde_json::Value;
use tokio::time::sleep;

const OPENBAO_RETRIES: u32 = 10;
const OPENBAO_RETRY_BASE_MS: u64 = 250;
const OPENBAO_READY_RETRIES: u32 = 30;
const OPENBAO_READY_DELAY_MS: u64 = 1000;

#[derive(Clone)]
pub struct OpenBaoClient {
    base_url: String,
    token: String,
    client: Client,
}

impl OpenBaoClient {
    pub fn from_env() -> Result<Option<Self>> {
        let addr = env::var("OPENBAO_ADDR")
            .ok()
            .filter(|v| !v.trim().is_empty());
        let token = env::var("OPENBAO_TOKEN")
            .ok()
            .filter(|v| !v.trim().is_empty());

        match (addr, token) {
            (None, None) => Ok(None),
            (Some(addr), Some(token)) => Ok(Some(Self::new(addr, token)?)),
            _ => anyhow::bail!("OPENBAO_ADDR and OPENBAO_TOKEN must be set together"),
        }
    }

    pub fn new(base_url: String, token: String) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .context("build OpenBao client")?;
        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            token,
            client,
        })
    }

    pub async fn wait_ready(&self) -> Result<()> {
        let url = self.health_url();
        let mut last_err = None;
        for _ in 1..=OPENBAO_READY_RETRIES {
            match self.client.get(&url).send().await {
                Ok(resp) => {
                    if resp.status().is_success() {
                        return Ok(());
                    }
                    last_err = Some(anyhow::anyhow!("openbao health returned {}", resp.status()));
                }
                Err(err) => {
                    last_err = Some(err.into());
                }
            }
            sleep(Duration::from_millis(OPENBAO_READY_DELAY_MS)).await;
        }
        let err = last_err.context("openbao health check failed")?;
        anyhow::bail!("openbao health check failed: {}", err);
    }

    pub async fn health_status(&self) -> Result<StatusCode> {
        let url = self.health_url();
        let resp = self
            .client
            .get(url)
            .send()
            .await
            .context("openbao health request failed")?;
        Ok(resp.status())
    }

    pub async fn write_value(&self, path: &str, value: &str) -> Result<()> {
        let url = self.build_kv2_url(path)?;
        let payload = serde_json::json!({ "data": { "value": value } });
        let mut last_err = None;
        for attempt in 1..=OPENBAO_RETRIES {
            let resp = self
                .client
                .post(&url)
                .header("X-Vault-Token", &self.token)
                .json(&payload)
                .send()
                .await;
            match resp {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        return Ok(());
                    }
                    let body = resp.text().await.unwrap_or_default();
                    anyhow::bail!("openbao write failed: {} {}", status, body);
                }
                Err(err) => {
                    if err.is_connect() || err.is_timeout() {
                        last_err = Some(err);
                        sleep(Duration::from_millis(
                            OPENBAO_RETRY_BASE_MS * attempt as u64,
                        ))
                        .await;
                        continue;
                    }
                    return Err(err).context("openbao write request failed");
                }
            }
        }
        let err = last_err.context("openbao write request failed")?;
        anyhow::bail!("openbao write request failed after retries: {}", err);
    }

    pub async fn read_value(&self, path: &str) -> Result<String> {
        let url = self.build_kv2_url(path)?;
        let mut last_err = None;
        for attempt in 1..=OPENBAO_RETRIES {
            let resp = self
                .client
                .get(&url)
                .header("X-Vault-Token", &self.token)
                .send()
                .await;
            match resp {
                Ok(resp) => {
                    let status = resp.status();
                    if !status.is_success() {
                        let body = resp.text().await.unwrap_or_default();
                        anyhow::bail!("openbao read failed: {} {}", status, body);
                    }
                    let payload: Value = resp.json().await.context("parse openbao response")?;
                    let value = payload
                        .get("data")
                        .and_then(|data| data.get("data"))
                        .and_then(|data| data.get("value"))
                        .and_then(|value| value.as_str())
                        .context("openbao value missing")?;
                    return Ok(value.to_string());
                }
                Err(err) => {
                    if err.is_connect() || err.is_timeout() {
                        last_err = Some(err);
                        sleep(Duration::from_millis(
                            OPENBAO_RETRY_BASE_MS * attempt as u64,
                        ))
                        .await;
                        continue;
                    }
                    return Err(err).context("openbao read request failed");
                }
            }
        }
        let err = last_err.context("openbao read request failed")?;
        anyhow::bail!("openbao read request failed after retries: {}", err);
    }

    fn build_kv2_url(&self, path: &str) -> Result<String> {
        let (mount, rest) = split_path(path)?;
        Ok(format!("{}/v1/{}/data/{}", self.base_url, mount, rest))
    }

    fn health_url(&self) -> String {
        format!("{}/v1/sys/health", self.base_url)
    }
}

fn split_path(path: &str) -> Result<(String, String)> {
    let trimmed = path.trim_start_matches('/');
    let mut parts = trimmed.splitn(2, '/');
    let mount = parts.next().unwrap_or("").trim();
    let rest = parts.next().unwrap_or("").trim();
    if mount.is_empty() || rest.is_empty() {
        anyhow::bail!("invalid OpenBao path: {}", path);
    }
    Ok((mount.to_string(), rest.to_string()))
}
