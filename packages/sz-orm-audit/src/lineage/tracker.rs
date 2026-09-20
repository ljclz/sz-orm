//! LineageTracker：编排 SQL 解析 + 增量更新图 + 影响分析 + 溯源分析。
//!
//! v7.5.0 扩展字段级血缘追踪：`DataLineage` 结构 + 敏感字段名脱敏。

use std::sync::{Arc, RwLock};

use super::graph::{LineageEdge, LineageError, LineageGraph, LineageNode, LineageNodeId};
use super::parser::{LineageDialect, LineageSqlParser};

/// lineage 更新结果
#[derive(Debug, Clone)]
pub struct LineageUpdate {
    pub edges_added: Vec<LineageEdge>,
    pub edges_skipped: usize,
}

// ============================================================================
// v7.5.0 字段级血缘追踪
// ============================================================================

/// 敏感字段名集合（用于字段名脱敏）。
const SENSITIVE_FIELD_NAMES: &[&str] = &[
    "phone",
    "email",
    "id_card",
    "idcard",
    "bank_card",
    "bankcard",
    "password",
    "pwd",
    "secret",
    "token",
    "ssn",
    "credit_card",
    "creditcard",
    "cvv",
];

/// 对敏感字段名进行脱敏：`phone` → `phone_masked`，非敏感字段原样返回。
pub fn mask_sensitive_field_name(field: &str) -> String {
    let lower = field.to_ascii_lowercase();
    if SENSITIVE_FIELD_NAMES.contains(&lower.as_str()) {
        format!("{}_masked", lower)
    } else {
        field.to_string()
    }
}

/// 字段级血缘记录：记录目标字段来自哪个表/列，经过什么变换。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataLineage {
    /// 目标字段（已脱敏，敏感字段名加 `_masked` 后缀）
    pub target_field: String,
    /// 源表名
    pub source_table: String,
    /// 源列名（已脱敏）
    pub source_column: String,
    /// 变换路径（如 "direct" / "hash" / "concat(a,b)"），描述从源到目标的变换
    pub transform_path: String,
    /// 是否为派生字段（表达式计算得到，非直接引用）
    pub is_derived: bool,
    /// 查询指纹（SQL 的归一化摘要，用于关联同一查询的多个血缘记录）
    pub query_fingerprint: String,
}

impl DataLineage {
    /// 创建直接引用的字段血缘（非派生）。
    pub fn direct(
        target_field: &str,
        source_table: &str,
        source_column: &str,
        query_fingerprint: &str,
    ) -> Self {
        Self {
            target_field: mask_sensitive_field_name(target_field),
            source_table: source_table.to_string(),
            source_column: mask_sensitive_field_name(source_column),
            transform_path: "direct".to_string(),
            is_derived: false,
            query_fingerprint: query_fingerprint.to_string(),
        }
    }

    /// 创建派生字段的血缘（表达式计算得到）。
    pub fn derived(
        target_field: &str,
        source_table: &str,
        source_column: &str,
        transform_expr: &str,
        query_fingerprint: &str,
    ) -> Self {
        Self {
            target_field: mask_sensitive_field_name(target_field),
            source_table: source_table.to_string(),
            source_column: mask_sensitive_field_name(source_column),
            transform_path: transform_expr.to_string(),
            is_derived: true,
            query_fingerprint: query_fingerprint.to_string(),
        }
    }

    /// 是否为派生字段
    pub fn is_derived(&self) -> bool {
        self.is_derived
    }

    /// 是否无法追溯源列（源表或源列为空）
    pub fn is_incomplete(&self) -> bool {
        self.source_table.is_empty() || self.source_column.is_empty()
    }
}

/// 字段级血缘完整性校验告警码
pub const LINEAGE_INCOMPLETE: &str = "LINEAGE_INCOMPLETE";

/// 字段级血缘追踪器：记录每个字段的来源（表/列/变换），支持完整性校验。
#[derive(Debug, Clone, Default)]
pub struct FieldLineageTracker {
    lineages: Vec<DataLineage>,
}

impl FieldLineageTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// 记录一条字段血缘
    pub fn record(&mut self, lineage: DataLineage) -> Option<String> {
        if lineage.is_incomplete() {
            return Some(format!(
                "{}: field '{}' has incomplete lineage (source_table='{}', source_column='{}')",
                LINEAGE_INCOMPLETE,
                lineage.target_field,
                lineage.source_table,
                lineage.source_column,
            ));
        }
        self.lineages.push(lineage);
        None
    }

    /// 获取所有血缘记录
    pub fn lineages(&self) -> &[DataLineage] {
        &self.lineages
    }

    /// 记录数
    pub fn count(&self) -> usize {
        self.lineages.len()
    }

    /// 完整性校验：查询结果中每个字段的来源（表/列/变换）均有记录。
    ///
    /// 返回缺失来源记录的字段名列表（空表示全部完整）。
    pub fn check_completeness(&self, query_fields: &[&str]) -> Vec<String> {
        let recorded: std::collections::HashSet<&str> = self
            .lineages
            .iter()
            .map(|l| l.target_field.as_str())
            .collect();
        query_fields
            .iter()
            .filter(|f| !recorded.contains(**f))
            .map(|f| f.to_string())
            .collect()
    }

    /// 获取某字段的所有源（可能来自多个表/列）
    pub fn sources_of(&self, target_field: &str) -> Vec<&DataLineage> {
        self.lineages
            .iter()
            .filter(|l| l.target_field == target_field)
            .collect()
    }
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

    // ----- v7.5.0 字段级血缘测试 -----

    #[test]
    fn test_mask_sensitive_field_name() {
        assert_eq!(mask_sensitive_field_name("phone"), "phone_masked");
        assert_eq!(mask_sensitive_field_name("email"), "email_masked");
        assert_eq!(mask_sensitive_field_name("password"), "password_masked");
        assert_eq!(mask_sensitive_field_name("name"), "name");
        assert_eq!(mask_sensitive_field_name("age"), "age");
    }

    #[test]
    fn test_mask_sensitive_field_name_case_insensitive() {
        assert_eq!(mask_sensitive_field_name("Phone"), "phone_masked");
        assert_eq!(mask_sensitive_field_name("EMAIL"), "email_masked");
    }

    #[test]
    fn test_data_lineage_direct() {
        let l = DataLineage::direct("name", "users", "name", "q1");
        assert!(!l.is_derived());
        assert!(!l.is_incomplete());
        assert_eq!(l.transform_path, "direct");
    }

    #[test]
    fn test_data_lineage_derived() {
        let l = DataLineage::derived("full_name", "users", "name", "concat(first, last)", "q1");
        assert!(l.is_derived());
        assert_eq!(l.transform_path, "concat(first, last)");
    }

    #[test]
    fn test_data_lineage_sensitive_field_masked() {
        let l = DataLineage::direct("phone", "users", "phone", "q1");
        assert_eq!(l.target_field, "phone_masked");
        assert_eq!(l.source_column, "phone_masked");
    }

    #[test]
    fn test_data_lineage_incomplete() {
        let l = DataLineage::direct("name", "", "name", "q1");
        assert!(l.is_incomplete());
    }

    #[test]
    fn test_field_lineage_tracker_record() {
        let mut tracker = FieldLineageTracker::new();
        let warning = tracker.record(DataLineage::direct("name", "users", "name", "q1"));
        assert!(warning.is_none());
        assert_eq!(tracker.count(), 1);
    }

    #[test]
    fn test_field_lineage_tracker_incomplete_warning() {
        let mut tracker = FieldLineageTracker::new();
        let warning = tracker.record(DataLineage::direct("name", "", "name", "q1"));
        assert!(warning.is_some());
        assert!(warning.unwrap().contains("LINEAGE_INCOMPLETE"));
        assert_eq!(tracker.count(), 0);
    }

    #[test]
    fn test_field_lineage_completeness_all_present() {
        let mut tracker = FieldLineageTracker::new();
        tracker.record(DataLineage::direct("name", "users", "name", "q1"));
        tracker.record(DataLineage::direct("age", "users", "age", "q1"));
        let missing = tracker.check_completeness(&["name", "age"]);
        assert!(missing.is_empty());
    }

    #[test]
    fn test_field_lineage_completeness_missing_fields() {
        let mut tracker = FieldLineageTracker::new();
        tracker.record(DataLineage::direct("name", "users", "name", "q1"));
        let missing = tracker.check_completeness(&["name", "age", "email"]);
        assert_eq!(missing, vec!["age", "email"]);
    }

    #[test]
    fn test_field_lineage_sources_of() {
        let mut tracker = FieldLineageTracker::new();
        tracker.record(DataLineage::direct("name", "users", "name", "q1"));
        tracker.record(DataLineage::direct(
            "name",
            "profiles",
            "display_name",
            "q1",
        ));
        let sources = tracker.sources_of("name");
        assert_eq!(sources.len(), 2);
    }

    #[test]
    fn test_field_lineage_sensitive_not_recorded_raw() {
        let mut tracker = FieldLineageTracker::new();
        tracker.record(DataLineage::direct("phone", "users", "phone", "q1"));
        let lineages = tracker.lineages();
        assert_eq!(lineages[0].target_field, "phone_masked");
        assert_eq!(lineages[0].source_column, "phone_masked");
    }
}
