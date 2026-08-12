//! # sz-orm-parallel — 并行查询执行器
//!
//! 基于 `parallel-query` feature，将多个独立查询并行执行降低复杂场景整体延迟。
//! v4.5.0 M1 将实现 ParallelQueryScheduler + MergeStrategy + FailureStrategy。
