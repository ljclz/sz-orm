//! AI-driven data governance: lineage, quality, compliance, masking.

#![cfg_attr(not(feature = "governance"), allow(unused_imports))]

#[cfg(feature = "governance-compliance")]
pub mod compliance;
#[cfg(feature = "governance-lineage")]
pub mod lineage;
#[cfg(feature = "governance")]
pub mod types;
