//! AI-driven data governance: lineage, quality, compliance, masking.

#[cfg(feature = "governance-compliance")]
pub mod compliance;
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
