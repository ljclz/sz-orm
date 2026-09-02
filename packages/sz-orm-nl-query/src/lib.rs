//! Natural language query pipeline: NL2SQL -> execute -> visualize -> insight.

#![cfg_attr(not(feature = "nl-query"), allow(unused_imports))]

#[cfg(feature = "nl-query")]
pub mod pipeline;
#[cfg(feature = "nl-query")]
pub mod types;
