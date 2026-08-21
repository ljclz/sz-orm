//! # SZ-ORM Graph — Neo4j Graph Database Support
//!
//! Provides graph database connection, parameterized Cypher queries, typed result mapping, and declarative modeling capabilities.
//! Does not modify existing sz-orm-core/sz-orm-sqlx APIs.
//!
//! ## Main Modules
//!
//! - [`connection`] — Bolt protocol connection and connection pool
//! - [`query`] — Cypher query construction and execution
//! - [`validator`] — Parameterized validation + SQL passthrough rejection
//! - [`model`] — Declarative modeling
//! - [`mapping`] — Typed result mapping
//! - [`error`] — GraphError error type

pub mod algorithm;
pub mod community;
pub mod connection;
pub mod cypher_parser;
pub mod engine;
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
pub use cypher_parser::{CypherSubsetParser, NodePattern, ParsedQuery, RelPattern, ReturnItem};
pub use engine::InMemoryGraphEngine;
pub use error::GraphError;
pub use graph_stats::{DegreeDistribution, GraphStats, GraphStatsCalculator};
pub use mapping::{NodeMapper, RelationMapper, ResultMapper};
pub use model::{
    GraphNodeModel, GraphPropertyDef, GraphRelationModel, GraphValueType, RelationDirection,
};
pub use path_analysis::{PathAnalyzer, ReachabilityMatrix};
pub use query::{
    execute_query, CypherQuery, CypherQueryBuilder, GraphNode, GraphPath, GraphRelationship,
    GraphResult,
};
pub use subgraph::{CommonSubgraphFinder, IsomorphismChecker, SubgraphMatcher};
pub use validator::CypherValidator;
