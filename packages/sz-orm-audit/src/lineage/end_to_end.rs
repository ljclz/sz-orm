//! v7.6.0 端到端血缘追踪 + 影响分析。
//!
//! 在既有字段级血缘（`LineageTracker` / `LineageGraph`）基础上，提供：
//! - [`EndToEndLineageTracker`]：端到端血缘追踪（从数据输入到最终消费的完整链路）
//! - [`ImpactAnalyzer`]：Schema 变更影响分析（评估 DDL 变更对下游的影响范围）
//!
//! 与既有 `downstream_impact` / `upstream_trace` 的区别：
//! - 既有功能基于 `LineageGraph` 的 BFS 边遍历（字段级）
//! - 本模块提供**端到端链路验证**（完整性检查）和 **Schema 变更影响评估**（含严重等级）

use std::collections::HashMap;
use std::sync::RwLock;

use super::graph::{LineageGraph, LineageNodeId};

/// 端到端血缘记录：从源到目标的完整链路。
#[derive(Debug, Clone)]
pub struct EndToEndLineage {
    /// 链路唯一 ID
    pub lineage_id: String,
    /// 源节点（table.column）
    pub source: String,
    /// 目标节点（table.column）
    pub target: String,
    /// 中间节点路径（按顺序）
    pub path: Vec<String>,
    /// 变换描述（如 "direct" / "hash" / "join+project"）
    pub transform: String,
    /// 时间戳
    pub timestamp: i64,
}

impl EndToEndLineage {
    pub fn new(source: &str, target: &str, transform: &str) -> Self {
        Self {
            lineage_id: format!("{}->{}", source, target),
            source: source.to_string(),
            target: target.to_string(),
            path: vec![source.to_string(), target.to_string()],
            transform: transform.to_string(),
            timestamp: current_timestamp(),
        }
    }

    pub fn with_path(mut self, path: Vec<String>) -> Self {
        self.path = path;
        self
    }

    /// 验证链路完整性：路径非空且首尾匹配 source/target。
    pub fn is_complete(&self) -> bool {
        if self.path.len() < 2 {
            return false;
        }
        self.path
            .first()
            .map(|s| s == &self.source)
            .unwrap_or(false)
            && self.path.last().map(|s| s == &self.target).unwrap_or(false)
    }
}

/// 端到端血缘追踪器。
pub struct EndToEndLineageTracker {
    lineages: RwLock<Vec<EndToEndLineage>>,
    /// 节点 → 链路索引映射（快速查询）
    node_index: RwLock<HashMap<String, Vec<usize>>>,
}

impl EndToEndLineageTracker {
    pub fn new() -> Self {
        Self {
            lineages: RwLock::new(Vec::new()),
            node_index: RwLock::new(HashMap::new()),
        }
    }

    /// 记录一条端到端血缘。
    pub fn track(&self, lineage: EndToEndLineage) {
        let mut lineages = self.lineages.write().unwrap();
        let idx = lineages.len();
        let source = lineage.source.clone();
        let target = lineage.target.clone();
        lineages.push(lineage);
        drop(lineages);

        let mut index = self.node_index.write().unwrap();
        index.entry(source).or_default().push(idx);
        index.entry(target).or_default().push(idx);
    }

    /// 查询涉及某节点的所有链路。
    pub fn query_by_node(&self, node: &str) -> Vec<EndToEndLineage> {
        let lineages = self.lineages.read().unwrap();
        let index = self.node_index.read().unwrap();
        match index.get(node) {
            Some(indices) => indices
                .iter()
                .filter_map(|&i| lineages.get(i).cloned())
                .collect(),
            None => Vec::new(),
        }
    }

    /// 验证所有链路的完整性。
    pub fn verify_completeness(&self) -> CompletenessResult {
        let lineages = self.lineages.read().unwrap();
        let total = lineages.len();
        let complete = lineages.iter().filter(|l| l.is_complete()).count();
        let incomplete: Vec<String> = lineages
            .iter()
            .filter(|l| !l.is_complete())
            .map(|l| l.lineage_id.clone())
            .collect();
        CompletenessResult {
            total_lineages: total,
            complete_lineages: complete,
            incomplete_lineage_ids: incomplete,
        }
    }

    /// 从 LineageGraph 构建端到端链路（遍历所有节点的下游影响）。
    pub fn build_from_graph(&self, graph: &LineageGraph) -> usize {
        let mut count = 0;
        let nodes: Vec<LineageNodeId> = graph.nodes.keys().cloned().collect();
        for source in &nodes {
            let reachable = graph.impact_analysis(source);
            for target_node in reachable {
                let target = &target_node.id;
                if target != source {
                    let source_str = format!("{}.{}", source.table, source.column);
                    let target_str = format!("{}.{}", target.table, target.column);
                    let lineage = EndToEndLineage::new(&source_str, &target_str, "graph-derived");
                    self.track(lineage);
                    count += 1;
                }
            }
        }
        count
    }

    /// 返回所有链路。
    pub fn all_lineages(&self) -> Vec<EndToEndLineage> {
        self.lineages.read().unwrap().clone()
    }

    /// 链路总数。
    pub fn len(&self) -> usize {
        self.lineages.read().unwrap().len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for EndToEndLineageTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// 完整性验证结果。
#[derive(Debug, Clone)]
pub struct CompletenessResult {
    pub total_lineages: usize,
    pub complete_lineages: usize,
    pub incomplete_lineage_ids: Vec<String>,
}

impl CompletenessResult {
    pub fn is_all_complete(&self) -> bool {
        self.complete_lineages == self.total_lineages
    }

    pub fn incomplete_count(&self) -> usize {
        self.incomplete_lineage_ids.len()
    }
}

/// Schema 变更类型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaChangeType {
    AddColumn,
    DropColumn,
    RenameColumn,
    AlterType,
    AddTable,
    DropTable,
}

impl SchemaChangeType {
    pub fn severity(&self) -> ChangeSeverity {
        match self {
            SchemaChangeType::DropColumn | SchemaChangeType::DropTable => ChangeSeverity::Critical,
            SchemaChangeType::AlterType | SchemaChangeType::RenameColumn => ChangeSeverity::High,
            SchemaChangeType::AddColumn | SchemaChangeType::AddTable => ChangeSeverity::Low,
        }
    }
}

/// 变更严重等级。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ChangeSeverity {
    Low,
    Medium,
    High,
    Critical,
}

impl ChangeSeverity {
    pub fn as_str(&self) -> &str {
        match self {
            ChangeSeverity::Low => "Low",
            ChangeSeverity::Medium => "Medium",
            ChangeSeverity::High => "High",
            ChangeSeverity::Critical => "Critical",
        }
    }
}

/// Schema 变更描述。
#[derive(Debug, Clone)]
pub struct SchemaChange {
    pub change_type: SchemaChangeType,
    pub table: String,
    pub column: Option<String>,
    pub description: String,
}

impl SchemaChange {
    pub fn new(change_type: SchemaChangeType, table: &str, description: &str) -> Self {
        Self {
            change_type,
            table: table.to_string(),
            column: None,
            description: description.to_string(),
        }
    }

    pub fn with_column(mut self, column: &str) -> Self {
        self.column = Some(column.to_string());
        self
    }

    pub fn severity(&self) -> ChangeSeverity {
        self.change_type.severity()
    }

    pub fn affected_node(&self) -> String {
        match &self.column {
            Some(col) => format!("{}.{}", self.table, col),
            None => self.table.clone(),
        }
    }
}

/// 影响分析结果。
#[derive(Debug, Clone)]
pub struct ImpactResult {
    pub change: SchemaChange,
    pub severity: ChangeSeverity,
    pub affected_nodes: Vec<String>,
    pub affected_lineages: Vec<String>,
    pub recommendation: String,
}

impl ImpactResult {
    pub fn affected_count(&self) -> usize {
        self.affected_nodes.len()
    }

    pub fn is_critical(&self) -> bool {
        self.severity == ChangeSeverity::Critical
    }
}

/// 影响分析器：评估 Schema 变更对下游的影响。
pub struct ImpactAnalyzer {
    tracker: std::sync::Arc<EndToEndLineageTracker>,
}

impl ImpactAnalyzer {
    pub fn new(tracker: std::sync::Arc<EndToEndLineageTracker>) -> Self {
        Self { tracker }
    }

    /// 分析 Schema 变更的影响。
    pub fn analyze(&self, change: SchemaChange) -> ImpactResult {
        let severity = change.severity();
        let affected_node = change.affected_node();

        let affected_lineages = self
            .tracker
            .query_by_node(&affected_node)
            .iter()
            .map(|l| l.lineage_id.clone())
            .collect();

        let affected_nodes = self.compute_affected_nodes(&change);

        let recommendation = self.recommend(&change, &affected_nodes);

        ImpactResult {
            change,
            severity,
            affected_nodes,
            affected_lineages,
            recommendation,
        }
    }

    fn compute_affected_nodes(&self, change: &SchemaChange) -> Vec<String> {
        let node = change.affected_node();
        let lineages = self.tracker.query_by_node(&node);
        let mut nodes: Vec<String> = Vec::new();
        for lineage in &lineages {
            if !nodes.contains(&lineage.source) {
                nodes.push(lineage.source.clone());
            }
            if !nodes.contains(&lineage.target) {
                nodes.push(lineage.target.clone());
            }
        }
        nodes.sort();
        nodes
    }

    fn recommend(&self, change: &SchemaChange, affected: &[String]) -> String {
        match change.change_type {
            SchemaChangeType::DropColumn | SchemaChangeType::DropTable => {
                if affected.is_empty() {
                    "无下游影响，可安全执行".to_string()
                } else {
                    format!(
                        "高危变更：影响 {} 个节点，建议先迁移下游依赖再执行",
                        affected.len()
                    )
                }
            }
            SchemaChangeType::AlterType => "类型变更可能导致数据丢失，建议先验证兼容性".to_string(),
            SchemaChangeType::RenameColumn => {
                "重命名需同步更新所有下游引用，建议使用渐进式重命名".to_string()
            }
            SchemaChangeType::AddColumn | SchemaChangeType::AddTable => {
                "新增操作通常无破坏性影响，可直接执行".to_string()
            }
        }
    }

    /// 批量分析多个变更。
    pub fn analyze_batch(&self, changes: Vec<SchemaChange>) -> Vec<ImpactResult> {
        changes.into_iter().map(|c| self.analyze(c)).collect()
    }

    /// 返回所有 Critical 级别的变更结果。
    pub fn critical_changes(results: &[ImpactResult]) -> Vec<&ImpactResult> {
        results.iter().filter(|r| r.is_critical()).collect()
    }
}

fn current_timestamp() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn end_to_end_lineage_complete() {
        let lineage = EndToEndLineage::new("users.id", "report.uid", "direct");
        assert!(lineage.is_complete());
    }

    #[test]
    fn end_to_end_lineage_with_path() {
        let lineage = EndToEndLineage::new("users.id", "report.uid", "join").with_path(vec![
            "users.id".to_string(),
            "orders.user_id".to_string(),
            "report.uid".to_string(),
        ]);
        assert!(lineage.is_complete());
        assert_eq!(lineage.path.len(), 3);
    }

    #[test]
    fn end_to_end_lineage_incomplete_empty_path() {
        let mut lineage = EndToEndLineage::new("a", "b", "direct");
        lineage.path = vec![];
        assert!(!lineage.is_complete());
    }

    #[test]
    fn tracker_track_and_query() {
        let tracker = EndToEndLineageTracker::new();
        tracker.track(EndToEndLineage::new("users.id", "orders.uid", "join"));
        tracker.track(EndToEndLineage::new("orders.uid", "report.oid", "project"));
        assert_eq!(tracker.len(), 2);

        let users_lineages = tracker.query_by_node("users.id");
        assert_eq!(users_lineages.len(), 1);

        let orders_lineages = tracker.query_by_node("orders.uid");
        assert_eq!(orders_lineages.len(), 2);
    }

    #[test]
    fn tracker_verify_completeness_all_complete() {
        let tracker = EndToEndLineageTracker::new();
        tracker.track(EndToEndLineage::new("a.x", "b.y", "direct"));
        tracker.track(EndToEndLineage::new("b.y", "c.z", "direct"));
        let result = tracker.verify_completeness();
        assert!(result.is_all_complete());
        assert_eq!(result.total_lineages, 2);
        assert_eq!(result.complete_lineages, 2);
    }

    #[test]
    fn tracker_verify_completeness_with_incomplete() {
        let tracker = EndToEndLineageTracker::new();
        tracker.track(EndToEndLineage::new("a.x", "b.y", "direct"));
        let mut incomplete = EndToEndLineage::new("c.z", "d.w", "direct");
        incomplete.path = vec!["c.z".to_string()];
        tracker.track(incomplete);
        let result = tracker.verify_completeness();
        assert!(!result.is_all_complete());
        assert_eq!(result.incomplete_count(), 1);
    }

    #[test]
    fn schema_change_severity() {
        let drop_col =
            SchemaChange::new(SchemaChangeType::DropColumn, "users", "drop id").with_column("id");
        assert_eq!(drop_col.severity(), ChangeSeverity::Critical);

        let add_col = SchemaChange::new(SchemaChangeType::AddColumn, "users", "add col")
            .with_column("new_col");
        assert_eq!(add_col.severity(), ChangeSeverity::Low);

        let alter =
            SchemaChange::new(SchemaChangeType::AlterType, "users", "alter").with_column("id");
        assert_eq!(alter.severity(), ChangeSeverity::High);
    }

    #[test]
    fn impact_analyzer_drop_column() {
        let tracker = std::sync::Arc::new(EndToEndLineageTracker::new());
        tracker.track(EndToEndLineage::new("users.id", "orders.uid", "join"));
        tracker.track(EndToEndLineage::new("orders.uid", "report.oid", "project"));

        let analyzer = ImpactAnalyzer::new(tracker);
        let change =
            SchemaChange::new(SchemaChangeType::DropColumn, "users", "drop id").with_column("id");
        let result = analyzer.analyze(change);

        assert!(result.is_critical());
        assert!(result.affected_count() > 0);
        assert!(result.recommendation.contains("高危"));
    }

    #[test]
    fn impact_analyzer_add_column_low_severity() {
        let tracker = std::sync::Arc::new(EndToEndLineageTracker::new());
        let analyzer = ImpactAnalyzer::new(tracker);
        let change = SchemaChange::new(SchemaChangeType::AddColumn, "users", "add col")
            .with_column("new_col");
        let result = analyzer.analyze(change);
        assert_eq!(result.severity, ChangeSeverity::Low);
        assert!(result.recommendation.contains("无破坏性"));
    }

    #[test]
    fn impact_analyzer_batch() {
        let tracker = std::sync::Arc::new(EndToEndLineageTracker::new());
        tracker.track(EndToEndLineage::new("users.id", "orders.uid", "join"));

        let analyzer = ImpactAnalyzer::new(tracker);
        let changes = vec![
            SchemaChange::new(SchemaChangeType::DropColumn, "users", "drop").with_column("id"),
            SchemaChange::new(SchemaChangeType::AddColumn, "users", "add").with_column("new"),
        ];
        let results = analyzer.analyze_batch(changes);
        assert_eq!(results.len(), 2);

        let critical = ImpactAnalyzer::critical_changes(&results);
        assert_eq!(critical.len(), 1);
    }

    #[test]
    fn impact_result_affected_count() {
        let tracker = std::sync::Arc::new(EndToEndLineageTracker::new());
        tracker.track(EndToEndLineage::new("a.x", "b.y", "direct"));
        tracker.track(EndToEndLineage::new("a.x", "c.z", "direct"));

        let analyzer = ImpactAnalyzer::new(tracker);
        let change = SchemaChange::new(SchemaChangeType::DropColumn, "a", "drop").with_column("x");
        let result = analyzer.analyze(change);
        assert!(result.affected_count() >= 2);
    }

    #[test]
    fn tracker_build_from_graph() {
        use super::super::graph::{EdgeType, LineageEdge, LineageNode, NodeType};
        let mut graph = LineageGraph::new();
        let n1 = LineageNodeId::new("users", "id");
        let n2 = LineageNodeId::new("orders", "uid");
        graph.add_node(LineageNode::new(n1.clone(), NodeType::Column));
        graph.add_node(LineageNode::new(n2.clone(), NodeType::Column));
        graph
            .add_edge(LineageEdge::new(n1, n2, EdgeType::Join))
            .unwrap();

        let tracker = EndToEndLineageTracker::new();
        let count = tracker.build_from_graph(&graph);
        assert!(count > 0);
        assert_eq!(tracker.len(), count);
    }

    #[test]
    fn change_severity_ordering() {
        assert!(ChangeSeverity::Critical > ChangeSeverity::High);
        assert!(ChangeSeverity::High > ChangeSeverity::Medium);
        assert!(ChangeSeverity::Medium > ChangeSeverity::Low);
    }
}
