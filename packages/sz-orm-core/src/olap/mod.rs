//! OLAP 分析查询模块（v7.1.0）
//!
//! 向量化分析查询：聚合下推 + Star Schema 优化 + 物化视图匹配 + 资源限制 + 工作负载路由。

pub mod aggregate_pushdown;
pub mod gateway;
pub mod mv_matcher;
pub mod plan_annotator;
pub mod resource_guard;
pub mod star_schema;
pub mod workload_router;

pub use aggregate_pushdown::{AggregateColumn, AggregateFunc, AggregatePushdown, PushdownResult};
pub use gateway::{OlapConfig, OlapError, OlapQueryGateway, OlapResult};
pub use mv_matcher::{MatchResult, MaterializedView, MaterializedViewMatcher};
pub use plan_annotator::{AnnotationKind, OlapPlanAnnotator, PlanAnnotation};
pub use resource_guard::{OlapResourceGuard, ResourceCheckResult, ResourceExceeded, ResourceLimit};
pub use star_schema::{DimensionTable, StarSchema, StarSchemaOptimizer};
pub use workload_router::{RouteTarget, WorkloadRouter, WorkloadType};
