//! # sz-orm-parallel — 并行查询执行器
//!
//! 基于 `parallel-query` feature，将多个独立查询并行执行降低复杂场景整体延迟。
//! v4.5.0 M1 实现 ParallelQueryScheduler + MergeStrategy + FailureStrategy。

pub mod config;
pub mod error;
pub mod merger;
pub mod outcome;

#[cfg(feature = "parallel-query")]
pub mod scheduler;

pub use config::{FailureStrategy, MergeStrategy, ParallelQueryConfig};
pub use error::ParallelQueryError;
pub use merger::ResultMerger;
pub use outcome::{ParallelQuery, ParallelQueryOutcome, QueryFailure, QueryOutcome};

#[cfg(feature = "parallel-query")]
pub use scheduler::{DefaultLike, ParallelQueryScheduler};
