//! EXPLAIN 计划注解（`olap-vectorized` feature）
//!
//! 为 EXPLAIN 输出添加 OLAP 特定注解：
//! 向量化标记、聚合下推标记、物化视图匹配标记等。

use std::collections::HashMap;

/// 注解类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnnotationKind {
    /// 向量化执行
    Vectorized,
    /// 聚合下推
    AggregatePushdown,
    /// 物化视图匹配
    MaterializedViewMatch,
    /// Star Schema 优化
    StarSchemaOpt,
    /// 列存扫描
    ColumnarScan,
}

impl AnnotationKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            AnnotationKind::Vectorized => "vectorized",
            AnnotationKind::AggregatePushdown => "aggregate-pushdown",
            AnnotationKind::MaterializedViewMatch => "mv-match",
            AnnotationKind::StarSchemaOpt => "star-schema-opt",
            AnnotationKind::ColumnarScan => "columnar-scan",
        }
    }
}

/// 计划注解
#[derive(Debug, Clone)]
pub struct PlanAnnotation {
    /// 注解类型
    pub kind: AnnotationKind,
    /// 注解详情
    pub detail: String,
    /// 节点 ID（对应 EXPLAIN 中的节点）
    pub node_id: Option<usize>,
}

/// EXPLAIN 计划注解器
#[derive(Debug)]
pub struct OlapPlanAnnotator {
    annotations: Vec<PlanAnnotation>,
}

impl OlapPlanAnnotator {
    pub fn new() -> Self {
        Self {
            annotations: Vec::new(),
        }
    }

    /// 添加注解
    pub fn add(&mut self, kind: AnnotationKind, detail: impl Into<String>) {
        self.annotations.push(PlanAnnotation {
            kind,
            detail: detail.into(),
            node_id: None,
        });
    }

    /// 添加带节点 ID 的注解
    pub fn add_with_node(
        &mut self,
        kind: AnnotationKind,
        detail: impl Into<String>,
        node_id: usize,
    ) {
        self.annotations.push(PlanAnnotation {
            kind,
            detail: detail.into(),
            node_id: Some(node_id),
        });
    }

    /// 生成注解的 EXPLAIN 输出
    pub fn annotate(&self, explain_output: &str) -> String {
        if self.annotations.is_empty() {
            return explain_output.to_string();
        }
        let mut result = explain_output.to_string();
        result.push_str("\n-- OLAP Annotations:");
        for ann in &self.annotations {
            let node_str = ann
                .node_id
                .map(|id| format!("[node={}]", id))
                .unwrap_or_default();
            result.push_str(&format!(
                "\n--   {}{}: {}",
                ann.kind.as_str(),
                node_str,
                ann.detail
            ));
        }
        result
    }

    /// 获取所有注解
    pub fn annotations(&self) -> &[PlanAnnotation] {
        &self.annotations
    }

    /// 按类型分组统计
    pub fn summary(&self) -> HashMap<AnnotationKind, usize> {
        let mut counts = HashMap::new();
        for ann in &self.annotations {
            *counts.entry(ann.kind).or_default() += 1;
        }
        counts
    }
}

impl Default for OlapPlanAnnotator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_annotate() {
        let mut a = OlapPlanAnnotator::new();
        a.add(AnnotationKind::Vectorized, "batch size 1024");
        a.add(
            AnnotationKind::AggregatePushdown,
            "SUM(amount) pushed to storage",
        );
        let result = a.annotate("EXPLAIN SELECT SUM(amount) FROM sales");
        assert!(result.contains("vectorized"));
        assert!(result.contains("aggregate-pushdown"));
    }

    #[test]
    fn annotate_with_node_id() {
        let mut a = OlapPlanAnnotator::new();
        a.add_with_node(AnnotationKind::ColumnarScan, "columnar access", 3);
        let result = a.annotate("EXPLAIN");
        assert!(result.contains("[node=3]"));
    }

    #[test]
    fn no_annotations_returns_original() {
        let a = OlapPlanAnnotator::new();
        let result = a.annotate("EXPLAIN SELECT * FROM t");
        assert_eq!(result, "EXPLAIN SELECT * FROM t");
    }

    #[test]
    fn summary_counts() {
        let mut a = OlapPlanAnnotator::new();
        a.add(AnnotationKind::Vectorized, "a");
        a.add(AnnotationKind::Vectorized, "b");
        a.add(AnnotationKind::ColumnarScan, "c");
        let s = a.summary();
        assert_eq!(s.get(&AnnotationKind::Vectorized), Some(&2));
        assert_eq!(s.get(&AnnotationKind::ColumnarScan), Some(&1));
    }
}
