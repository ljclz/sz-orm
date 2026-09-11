//! AI-driven data governance: lineage, quality, compliance, masking.

#[cfg(feature = "governance-compliance")]
pub mod compliance;
#[cfg(feature = "compliance-report")]
#[allow(missing_docs)]
pub mod compliance_report;
#[cfg(feature = "governance")]
pub mod data_catalog;
#[cfg(feature = "governance-lineage")]
pub mod lineage;
#[cfg(feature = "governance")]
pub mod masking_recommend;
#[cfg(feature = "governance")]
pub mod quality_rule;
#[cfg(feature = "governance")]
pub mod types;

#[cfg(feature = "sla-monitor")]
#[allow(missing_docs)]
pub mod sla_violation_tracker;

#[cfg(feature = "cost-governance")]
#[allow(missing_docs)]
pub mod cost_accountant;

#[cfg(feature = "sensitive-discover")]
#[allow(missing_docs)]
pub mod sensitive_discoverer;
