//! # sz-orm-parallel — Parallel Query Executor
//!
//! Based on `parallel-query` feature, executes multiple independent queries in parallel to reduce overall latency in complex scenarios.
//! v4.5.0 M1 implements ParallelQueryScheduler + MergeStrategy + FailureStrategy.

pub mod config;
pub mod error;
pub mod executor;
pub mod merger;
pub mod outcome;
pub mod parallel_stats;
pub mod parallelism;
pub mod sharder;
pub mod task_scheduler;

#[cfg(feature = "parallel-query")]
pub mod scheduler;

pub use config::{FailureStrategy, MergeStrategy, ParallelQueryConfig};
pub use error::ParallelQueryError;
pub use executor::{
    BatchExecutor, ParallelExecutor, ParallelResult, ParallelTask, ParallelismMonitor,
};
pub use merger::ResultMerger;
pub use outcome::{ParallelQuery, ParallelQueryOutcome, QueryFailure, QueryOutcome};
pub use parallel_stats::{ParallelReport, ParallelStats, ParallelStatsCollector, QueryKeyStats};
pub use parallelism::{ParallelismControl, ParallelismStats, ParallelismStrategy};
pub use sharder::{DynamicSharder, Shard, ShardStrategy, TaskSharder};
pub use task_scheduler::{Priority, ScheduleState, ScheduledTask, TaskScheduler};

#[cfg(feature = "parallel-query")]
pub use scheduler::{DefaultLike, ParallelQueryScheduler};
