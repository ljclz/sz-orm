//! LineageTracker：编排 SQL 解析 + 增量更新图 + 影响分析 + 溯源分析。

use std::sync::{Arc, RwLock};

use super::graph::{LineageEdge, LineageError, LineageGraph, LineageNode, LineageNodeId};
use super::parser::{LineageDialect, LineageSqlParser};

/// lineage 更新结果
#[derive(Debug, Clone)]
pub struct LineageUpdate {
    pub edges_added: Vec<LineageEdge>,
    pub edges_skipped: usize,
}

/// lineage 追踪器
pub struct LineageTracker {
    graph: Arc<RwLock<LineageGraph>>,
    parser: LineageSqlParser,
    auditor: Option<Arc<crate::HashChainAuditor>>,
}

impl LineageTracker {
    pub fn new(dialect: LineageDialect, auditor: Option<Arc<crate::HashChainAuditor>>) -> Self {
        Self {
            graph: Arc::new(RwLock::new(LineageGraph::new())),
            parser: LineageSqlParser::new(dialect),
            auditor,
        }
    }

    /// 追踪 SQL 依赖，增量更新 lineage 图
    pub fn track_sql(&self, sql: &str) -> Result<LineageUpdate, LineageError> {
        let edges = self.parser.parse(sql)?;

        let mut graph = self
            .graph
            .write()
            .expect("LineageTracker graph lock poisoned (track_sql)");

        let existing_count = graph.edge_count();
        graph.incremental_update(edges.clone());
        let new_count = graph.edge_count();

        let edges_added: Vec<LineageEdge> = edges
            .iter()
            .filter(|e| graph.edges.contains(e))
            .cloned()
            .collect();
        let edges_skipped = edges.len().saturating_sub(new_count - existing_count);

        if let Some(auditor) = &self.auditor {
            if !edges_added.is_empty() {
                let ctx = crate::SqlAuditContext {
                    sql: format!(
                        "lineage_update: {} edges added ({} skipped)",
                        edges_added.len(),
                        edges_skipped
                    ),
                    user: "lineage_tracker".to_string(),
                    timestamp: chrono_timestamp(),
                };
                auditor.log(&ctx);
            }
        }

        Ok(LineageUpdate {
            edges_added,
            edges_skipped,
        })
    }

    /// 影响分析：变更某字段，输出下游受影响列表
    pub fn impact_analysis(&self, node: &LineageNodeId) -> Vec<LineageNode> {
        let graph = self
            .graph
            .read()
            .expect("LineageTracker graph lock poisoned (impact_analysis)");
        graph.impact_analysis(node)
    }

    /// 溯源分析：某字段来自哪些源头
    pub fn origin_analysis(&self, node: &LineageNodeId) -> Vec<LineageNode> {
        let graph = self
            .graph
            .read()
            .expect("LineageTracker graph lock poisoned (origin_analysis)");
        graph.origin_analysis(node)
    }

    /// 获取图的快照
    pub fn graph_snapshot(&self) -> LineageGraph {
        self.graph
            .read()
            .expect("LineageTracker graph lock poisoned (graph_snapshot)")
            .clone()
    }

    /// 节点数量
    pub fn node_count(&self) -> usize {
        self.graph
            .read()
            .expect("LineageTracker graph lock poisoned (node_count)")
            .node_count()
    }

    /// 边数量
    pub fn edge_count(&self) -> usize {
        self.graph
            .read()
            .expect("LineageTracker graph lock poisoned (edge_count)")
            .edge_count()
    }
}

fn chrono_timestamp() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lineage::EdgeType;
    use crate::HashChainAuditor;

    #[test]
    fn test_track_insert_select() {
        let tracker = LineageTracker::new(LineageDialect::PostgreSQL, None);
        let sql = "INSERT INTO report (name, amount) SELECT users.name, orders.amount FROM users JOIN orders ON users.id = orders.user_id";
        let update = tracker.track_sql(sql).unwrap();

        assert!(!update.edges_added.is_empty());

        let graph = tracker.graph_snapshot();
        assert!(graph.edges.contains(&LineageEdge::new(
            LineageNodeId::new("users", "name"),
            LineageNodeId::new("report", "name"),
            EdgeType::Derived,
        )));
        assert!(graph.edges.contains(&LineageEdge::new(
            LineageNodeId::new("orders", "amount"),
            LineageNodeId::new("report", "amount"),
            EdgeType::Derived,
        )));
    }

    #[test]
    fn test_impact_analysis() {
        let tracker = LineageTracker::new(LineageDialect::PostgreSQL, None);

        tracker
            .track_sql("CREATE VIEW report AS SELECT users.name FROM users")
            .unwrap();
        tracker
            .track_sql("CREATE VIEW dashboard AS SELECT report.name FROM report")
            .unwrap();

        let impacted = tracker.impact_analysis(&LineageNodeId::new("users", "name"));
        assert!(impacted
            .iter()
            .any(|n| n.id == LineageNodeId::new("report", "name")));
        assert!(impacted
            .iter()
            .any(|n| n.id == LineageNodeId::new("dashboard", "name")));
    }

    #[test]
    fn test_origin_analysis() {
        let tracker = LineageTracker::new(LineageDialect::PostgreSQL, None);

        tracker
            .track_sql("CREATE VIEW report AS SELECT users.name, orders.amount FROM users JOIN orders ON users.id = orders.user_id")
            .unwrap();

        let origins = tracker.origin_analysis(&LineageNodeId::new("report", "amount"));
        assert!(origins
            .iter()
            .any(|n| n.id == LineageNodeId::new("orders", "amount")));
    }

    #[test]
    fn test_audit_integration() {
        let auditor = Arc::new(HashChainAuditor::new());
        let tracker = LineageTracker::new(LineageDialect::PostgreSQL, Some(auditor.clone()));

        let sql = "CREATE VIEW v AS SELECT a, b FROM t";
        let update = tracker.track_sql(sql).unwrap();
        assert!(!update.edges_added.is_empty());

        assert!(!auditor.is_empty());
        assert!(auditor.verify().is_ok());
    }

    #[test]
    fn test_parse_error_skipped() {
        let tracker = LineageTracker::new(LineageDialect::PostgreSQL, None);

        let result = tracker.track_sql("THIS IS NOT VALID SQL !!!");
        assert!(result.is_err());

        tracker
            .track_sql("CREATE VIEW v AS SELECT a FROM t")
            .unwrap();

        assert_eq!(tracker.node_count(), 2);
        assert!(tracker.edge_count() > 0);
    }

    #[test]
    fn test_incremental_tracking() {
        let tracker = LineageTracker::new(LineageDialect::PostgreSQL, None);

        tracker
            .track_sql("CREATE VIEW v1 AS SELECT a FROM t1")
            .unwrap();
        assert_eq!(tracker.edge_count(), 1);

        tracker
            .track_sql("CREATE VIEW v2 AS SELECT a FROM t2")
            .unwrap();
        assert_eq!(tracker.edge_count(), 2);
    }

    #[test]
    fn test_track_create_materialized_view() {
        let tracker = LineageTracker::new(LineageDialect::PostgreSQL, None);

        tracker
            .track_sql("CREATE MATERIALIZED VIEW mv AS SELECT a, b FROM t")
            .unwrap();

        let graph = tracker.graph_snapshot();
        assert!(graph.edges.contains(&LineageEdge::new(
            LineageNodeId::new("t", "a"),
            LineageNodeId::new("mv", "a"),
            EdgeType::DirectDependency,
        )));
    }

    #[test]
    fn test_track_update() {
        let tracker = LineageTracker::new(LineageDialect::PostgreSQL, None);

        tracker
            .track_sql("UPDATE report SET name = users.name FROM users")
            .unwrap();

        let graph = tracker.graph_snapshot();
        assert!(graph.edges.contains(&LineageEdge::new(
            LineageNodeId::new("users", "name"),
            LineageNodeId::new("report", "name"),
            EdgeType::Derived,
        )));
    }

    #[test]
    fn test_no_auditor_no_panic() {
        let tracker = LineageTracker::new(LineageDialect::PostgreSQL, None);
        let update = tracker
            .track_sql("CREATE VIEW v AS SELECT a FROM t")
            .unwrap();
        assert!(!update.edges_added.is_empty());
    }

    #[test]
    fn test_multiple_sql_tracking() {
        let tracker = LineageTracker::new(LineageDialect::PostgreSQL, None);

        tracker
            .track_sql("CREATE VIEW v1 AS SELECT a FROM t1")
            .unwrap();
        tracker
            .track_sql("CREATE VIEW v2 AS SELECT a FROM t2")
            .unwrap();
        tracker
            .track_sql("CREATE VIEW v3 AS SELECT v1.a, v2.a AS b FROM v1 JOIN v2 ON v1.a = v2.a")
            .unwrap();

        let impacted = tracker.impact_analysis(&LineageNodeId::new("t1", "a"));
        assert!(impacted
            .iter()
            .any(|n| n.id == LineageNodeId::new("v1", "a")));
        assert!(impacted
            .iter()
            .any(|n| n.id == LineageNodeId::new("v3", "a")));
    }

    #[test]
    fn test_edge_count_after_cycle_skip() {
        let tracker = LineageTracker::new(LineageDialect::PostgreSQL, None);

        tracker
            .track_sql("CREATE VIEW a AS SELECT b.x FROM b")
            .unwrap();
        assert_eq!(tracker.edge_count(), 1);

        let result = tracker.track_sql("CREATE VIEW b AS SELECT a.x FROM a");
        assert!(result.is_ok());

        assert_eq!(tracker.edge_count(), 1);
    }
}
