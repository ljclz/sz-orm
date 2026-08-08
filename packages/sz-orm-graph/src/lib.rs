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

pub mod connection;
pub mod error;
pub mod mapping;
pub mod model;
pub mod query;
pub mod validator;

pub use connection::{GraphConfig, GraphConnection, GraphPool, GraphPoolStatus};
pub use error::GraphError;
pub use mapping::{NodeMapper, RelationMapper, ResultMapper};
pub use model::{
    GraphNodeModel, GraphPropertyDef, GraphRelationModel, GraphValueType, RelationDirection,
};
pub use query::{
    CypherQuery, CypherQueryBuilder, GraphNode, GraphPath, GraphRelationship, GraphResult,
};
pub use validator::CypherValidator;
