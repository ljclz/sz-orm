//! Natural language query pipeline: NL2SQL -> execute -> visualize -> insight.

#[cfg(feature = "nl2sql-deep")]
#[allow(missing_docs)]
pub mod cached_pipeline;
#[cfg(feature = "nl2sql-deep")]
#[allow(missing_docs)]
pub mod dialect_renderer;
#[cfg(feature = "nl-query")]
pub mod history_learner;
#[cfg(feature = "nl-query")]
pub mod insight;
#[cfg(feature = "nl-query")]
pub mod llm_generator;
#[cfg(feature = "nl-query")]
pub mod pipeline;
#[cfg(feature = "nl2sql-deep")]
#[allow(missing_docs)]
pub mod sec_scanner;
#[cfg(feature = "nl-query")]
pub mod sql_explainer;
#[cfg(feature = "nl-query")]
pub mod types;
#[cfg(feature = "nl-query-visualizer")]
pub mod visualizer;
