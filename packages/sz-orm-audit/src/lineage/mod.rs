//! # 数据 lineage — 字段级血缘追踪
//!
//! 提供 `LineageGraph`（DAG）数据结构，节点为表.字段，边为依赖关系。
//! 支持环路检测（A→B→A 返回 `CycleDetected`）和增量更新。
//!
//! ## 主要类型
//!
//! - [`LineageGraph`] — 有向无环图（DAG）
//! - [`LineageNode`] / [`LineageNodeId`] — 节点（table.column）
//! - [`LineageEdge`] / [`EdgeType`] — 边（依赖关系）

pub mod export;
pub mod graph;
#[cfg(feature = "lineage-viz")]
pub mod impact;
pub mod parser;
pub mod tracker;

pub use export::LineageExportFormat;
pub use graph::{
    EdgeType, LineageEdge, LineageError, LineageGraph, LineageNode, LineageNodeId, NodeType,
};
#[cfg(feature = "lineage-viz")]
pub use impact::{downstream_impact, upstream_trace, ImpactEdge};
pub use parser::{LineageDialect, LineageSqlParser};
pub use tracker::{LineageTracker, LineageUpdate};
