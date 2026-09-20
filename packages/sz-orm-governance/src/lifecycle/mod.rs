//! 数据生命周期管理模块
//!
//! 提供规则引擎 + 冷热分离 + 归档 + TTL 清理全链路。
//! 规则引擎支持热更新（10s 内生效），冷热分类基于访问频率和时间维度，
//! 归档执行器复用 CDC 增量同步通道，TTL 清理先写日志后物理删除。

pub mod access_pattern_collector;
pub mod archive_executor;
pub mod archive_query_proxy;
pub mod cold_hot_classifier;
pub mod cold_hot_migration_scheduler;
pub mod rule_engine;
pub mod ttl_cleanup;
pub mod types;

pub use access_pattern_collector::AccessPatternCollector;
pub use archive_executor::ArchiveExecutor;
pub use archive_query_proxy::ArchiveQueryProxy;
pub use cold_hot_classifier::ColdHotClassifier;
pub use cold_hot_migration_scheduler::ColdHotMigrationScheduler;
pub use rule_engine::LifecycleRuleEngine;
pub use ttl_cleanup::TtlCleanupExecutor;
pub use types::*;
