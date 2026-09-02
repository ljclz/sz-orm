//! Natural language query pipeline: NL2SQL -> execute -> visualize -> insight.

#[cfg(feature = "nl-query")]
pub mod history_learner;
#[cfg(feature = "nl-query")]
pub mod insight;
#[cfg(feature = "nl-query")]
pub mod pipeline;
#[cfg(feature = "nl-query")]
pub mod sql_explainer;
#[cfg(feature = "nl-query")]
pub mod types;
#[cfg(feature = "nl-query-visualizer")]
pub mod visualizer;
