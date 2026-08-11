//! # sz-orm-fusion — 多数据库融合查询（POC，可选实验）
//!
//! 透明多数据库操作：**主库 + 缓存 + 搜索** 的查询拆分、聚合与降级。
//! 本包为 v4.3.0 M5 的可选实验（POC），核心价值是验证"融合查询"语义
//! 是否值得转正；具体后端（Redis / 向量库）通过 trait 注入，不绑定实现。
//!
//! 核心概念：
//!
//! - [`FusionQuery`]：结构化查询描述（表 + 参数化等值条件 + 其他条件）
//! - [`FusionPlanner`]：静态分析 → [`FusionPlan`]（缓存下推 / 搜索下推 / 主库步骤）
//! - [`FusionExecutor`]：按计划执行，缓存命中跳过主库，主库失败降级回缓存
//!
//! ```rust,ignore
//! let config = FusionConfig {
//!     primary: "mysql".into(),
//!     cache: Some(CacheBackend::Memory),
//!     search: None,
//! };
//! let ex = FusionExecutor::new(config).with_cache(Arc::new(MemoryFusionCache::new()));
//! let q = FusionQuery::new("users").eq("id", "42");
//! let out = ex.execute(&q, |q| primary_query(q)).unwrap();
//! assert!(!out.degraded);
//! ```

#[cfg(feature = "db-fusion")]
pub mod executor;
#[cfg(feature = "db-fusion")]
pub mod plan;

#[cfg(feature = "db-fusion")]
pub use executor::{FusionCache, FusionExecutor, FusionOutcome, MemoryFusionCache};
#[cfg(feature = "db-fusion")]
pub use plan::{FusionPlan, FusionPlanner, FusionQuery, PlanStep};
