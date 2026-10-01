#![forbid(unsafe_code)]
#![doc = "Planning-only IMAP observation boundary with credential-free input."]

mod boundary;
mod config;
mod plan;
mod reports;
#[cfg(test)]
mod unit_tests;

pub use boundary::ConnectorError;
pub use config::{ConnectorConfig, ContentPolicy, TlsMode, parse_config};
pub use plan::{ConnectorPlan, PlanStep, build_plan};
pub use reports::{
    CapabilityReport, DoctorReport, ValidationReport, capabilities, doctor, validation_report,
};

/// Stable connector identity used in reports and plans.
pub const CONNECTOR: &str = "zixcel-imap";
/// Provider family represented by this connector.
pub const PROVIDER: &str = "imap";
/// Accepted configuration schema.
pub const CONFIG_SCHEMA: &str = "zixcel://imap/config/v1";
/// Emitted planning contract schema.
pub const PLAN_SCHEMA: &str = "zixcel://contracts/connector-plan/v1";
