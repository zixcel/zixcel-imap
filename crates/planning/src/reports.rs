use serde::Serialize;

use crate::{CONNECTOR, ConnectorConfig, ConnectorError, PROVIDER};

const CAPABILITIES: &[&str] = &[
    "mailbox-observe",
    "message-header-observe",
    "message-metadata-observe",
];

/// Build-time evidence that this crate cannot perform an IMAP action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DoctorReport {
    pub schema: &'static str,
    pub connector: &'static str,
    pub status: &'static str,
    pub network_client_linked: bool,
    pub secret_resolution_enabled: bool,
    pub execution_enabled: bool,
}

/// Stable capability declaration for orchestrator discovery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CapabilityReport {
    pub schema: &'static str,
    pub connector: &'static str,
    pub provider: &'static str,
    pub planning_only: bool,
    pub capabilities: Vec<&'static str>,
}

/// Positive validation receipt containing no configuration secrets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ValidationReport {
    pub schema: &'static str,
    pub connector: &'static str,
    pub config_id: String,
    pub valid: bool,
}

/// Reports the deliberately absent execution dependencies.
#[must_use]
pub const fn doctor() -> DoctorReport {
    DoctorReport {
        schema: "zixcel://doctor/v1",
        connector: CONNECTOR,
        status: "healthy",
        network_client_linked: false,
        secret_resolution_enabled: false,
        execution_enabled: false,
    }
}

/// Returns capabilities that a separate approved executor may implement.
#[must_use]
pub fn capabilities() -> CapabilityReport {
    CapabilityReport {
        schema: "zixcel://capabilities/v1",
        connector: CONNECTOR,
        provider: PROVIDER,
        planning_only: true,
        capabilities: CAPABILITIES.to_vec(),
    }
}

/// Produces a serializable receipt after revalidating a typed config.
pub fn validation_report(config: &ConnectorConfig) -> Result<ValidationReport, ConnectorError> {
    config.validate()?;
    Ok(ValidationReport {
        schema: "zixcel://validation-result/v1",
        connector: CONNECTOR,
        config_id: config.config_id.clone(),
        valid: true,
    })
}
