//! CDC Sink 实现（v6.8.0）
//!
//! 具体 sink：内存、日志、过滤组合、缓存失效、脱敏、搜索索引刷新。

pub mod memory;

/// 缓存失效 sink：依赖 `dist-cache-cluster` feature（仅 `cdc-mysql` 包含）
#[cfg(feature = "cdc-mysql")]
pub mod cache;

/// 脱敏 sink：依赖 `sz-orm-masking`（仅 `cdc-mysql` 包含）
#[cfg(feature = "cdc-mysql")]
pub mod masking;

/// 搜索索引刷新 sink：依赖 `sz-orm-search`（仅 `cdc-mysql` 包含）
#[cfg(feature = "cdc-mysql")]
pub mod search;

/// 数据库同步 sink：仅依赖 `sqlx`，所有 CDC 方言均可用
#[cfg(any(
    feature = "cdc-mysql",
    feature = "cdc-postgres",
    feature = "cdc-sqlite"
))]
pub mod db;
/// Kafka sink：v7.1.0 CDC 实时同步
#[cfg(feature = "cdc-realtime-sync")]
pub mod kafka;
