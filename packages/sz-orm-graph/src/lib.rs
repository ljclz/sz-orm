//! # SZ-ORM Graph — Neo4j 图数据库支持
//!
//! 提供图数据库的连接、参数化 Cypher 查询、结果类型化映射、声明式建模能力。
//! 不触碰 sz-orm-core/sz-orm-sqlx 既有 API。
//!
//! ## 主要模块
//!
//! - [`connection`] — Bolt 协议连接与连接池
//! - [`query`] — Cypher 查询构造与执行
//! - [`validator`] — 参数化校验 + SQL 透传拒绝
//! - [`model`] — 声明式建模
//! - [`mapping`] — 结果类型化映射
//! - [`error`] — GraphError 错误类型

pub mod algorithm;
pub mod community;
pub mod connection;
pub mod error;
pub mod graph_stats;
pub mod mapping;
pub mod model;
pub mod path_analysis;
pub mod query;
pub mod subgraph;
pub mod validator;

pub use algorithm::{DirectedGraph, NodeId, UndirectedGraph, Weight};
pub use community::{
    Community, CommunityDetectionResult, ConnectedComponentDetector, LabelPropagation,
};
pub use connection::{GraphConfig, GraphConnection, GraphPool, GraphPoolStatus};
pub use error::GraphError;
pub use graph_stats::{DegreeDistribution, GraphStats, GraphStatsCalculator};
pub use mapping::{NodeMapper, RelationMapper, ResultMapper};
pub use model::{
    GraphNodeModel, GraphPropertyDef, GraphRelationModel, GraphValueType, RelationDirection,
};
pub use path_analysis::{PathAnalyzer, ReachabilityMatrix};
pub use query::{
    CypherQuery, CypherQueryBuilder, GraphNode, GraphPath, GraphRelationship, GraphResult,
};
pub use subgraph::{CommonSubgraphFinder, IsomorphismChecker, SubgraphMatcher};
pub use validator::CypherValidator;
