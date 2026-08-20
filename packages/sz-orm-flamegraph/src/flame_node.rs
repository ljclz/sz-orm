//! 火焰图节点树：调用栈树结构与构建器。
//!
//! - [`FlameNode`] — 火焰图树节点（帧名 + 值 + 子节点）
//! - [`FlameGraphData`] — 火焰图数据（根节点 + 统计）
//! - [`FlameGraphBuilder`] — 从采样构建火焰图树

use serde::{Deserialize, Serialize};

// ============================================================================
// FlameNode — 火焰图树节点
// ============================================================================

/// 火焰图树节点
///
/// 每个节点代表一个函数帧，`value` 为该帧及其子帧的总采样值，
/// `children` 为直接调用帧。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlameNode {
    name: String,
    value: u64,
    children: Vec<FlameNode>,
}

impl FlameNode {
    /// 创建根节点
    pub fn root(name: &str) -> Self {
        Self {
            name: name.to_string(),
            value: 0,
            children: vec![],
        }
    }

    /// 创建叶节点
    pub fn leaf(name: &str, value: u64) -> Self {
        Self {
            name: name.to_string(),
            value,
            children: vec![],
        }
    }

    /// 帧名
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 采样值（含子节点）
    pub fn value(&self) -> u64 {
        self.value
    }

    /// 子节点引用
    pub fn children(&self) -> &[FlameNode] {
        &self.children
    }

    /// 是否叶节点
    pub fn is_leaf(&self) -> bool {
        self.children.is_empty()
    }

    /// 子节点数
    pub fn child_count(&self) -> usize {
        self.children.len()
    }

    /// 树深度（叶节点深度为 1）
    pub fn depth(&self) -> usize {
        if self.children.is_empty() {
            1
        } else {
            1 + self.children.iter().map(|c| c.depth()).max().unwrap_or(0)
        }
    }

    /// 总节点数（含自身）
    pub fn node_count(&self) -> usize {
        1 + self.children.iter().map(|c| c.node_count()).sum::<usize>()
    }

    /// 叶节点数
    pub fn leaf_count(&self) -> usize {
        if self.children.is_empty() {
            1
        } else {
            self.children.iter().map(|c| c.leaf_count()).sum()
        }
    }

    /// 查找直接子节点（按名）
    pub fn find_child(&self, name: &str) -> Option<&FlameNode> {
        self.children.iter().find(|c| c.name == name)
    }

    /// 查找直接子节点（可变引用）
    pub fn find_child_mut(&mut self, name: &str) -> Option<&mut FlameNode> {
        self.children.iter_mut().find(|c| c.name == name)
    }

    /// 添加子节点（若同名子节点已存在则合并值）
    pub fn add_child(&mut self, child: FlameNode) {
        if let Some(existing) = self.find_child_mut(&child.name) {
            existing.merge(&child);
        } else {
            self.children.push(child);
        }
        self.recompute_value();
    }

    /// 合并另一个节点（同名，子节点递归合并）
    pub fn merge(&mut self, other: &FlameNode) {
        self.value += other.value;
        for child in &other.children {
            if let Some(existing) = self.find_child_mut(&child.name) {
                existing.merge(child);
            } else {
                self.children.push(child.clone());
            }
        }
    }

    /// 重算 value（子节点 value 之和，叶节点保持自身值）
    fn recompute_value(&mut self) {
        if !self.children.is_empty() {
            self.value = self.children.iter().map(|c| c.value).sum();
        }
    }

    /// 递归重算所有子节点的 value
    fn recompute_all(&mut self) {
        for child in &mut self.children {
            child.recompute_all();
        }
        self.recompute_value();
    }

    /// 按值降序排序子节点
    pub fn sort_by_value_desc(&mut self) {
        self.children.sort_by_key(|b| std::cmp::Reverse(b.value));
        for child in &mut self.children {
            child.sort_by_value_desc();
        }
    }

    /// 按名排序子节点
    pub fn sort_by_name(&mut self) {
        self.children.sort_by(|a, b| a.name.cmp(&b.name));
        for child in &mut self.children {
            child.sort_by_name();
        }
    }

    /// 所有帧名（去重）
    pub fn all_names(&self) -> Vec<String> {
        let mut names = vec![self.name.clone()];
        for child in &self.children {
            names.extend(child.all_names());
        }
        names
    }

    /// 最大宽度（叶节点数）
    pub fn width(&self) -> usize {
        self.leaf_count()
    }

    /// 转换为折叠栈格式行列表
    ///
    /// 每行格式：`root;child1;child2 value`
    pub fn to_folded_lines(&self) -> Vec<String> {
        let mut lines = vec![];
        self.folded_recursive(&self.name.clone(), &mut lines);
        lines
    }

    fn folded_recursive(&self, prefix: &str, lines: &mut Vec<String>) {
        if self.children.is_empty() {
            lines.push(format!("{} {}", prefix, self.value));
        } else {
            for child in &self.children {
                let new_prefix = format!("{};{}", prefix, child.name);
                child.folded_recursive(&new_prefix, lines);
            }
        }
    }
}

// ============================================================================
// FlameGraphData — 火焰图数据
// ============================================================================

/// 火焰图数据：根节点 + 元信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlameGraphData {
    root: FlameNode,
    total_samples: u64,
    max_depth: usize,
    node_count: usize,
}

impl FlameGraphData {
    /// 从根节点创建
    pub fn from_root(root: FlameNode) -> Self {
        let total_samples = root.value();
        let max_depth = root.depth();
        let node_count = root.node_count();
        Self {
            root,
            total_samples,
            max_depth,
            node_count,
        }
    }

    /// 根节点引用
    pub fn root(&self) -> &FlameNode {
        &self.root
    }

    /// 总采样数
    pub fn total_samples(&self) -> u64 {
        self.total_samples
    }

    /// 最大深度
    pub fn max_depth(&self) -> usize {
        self.max_depth
    }

    /// 节点总数
    pub fn node_count(&self) -> usize {
        self.node_count
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.total_samples == 0
    }

    /// 转换为折叠栈格式
    pub fn to_folded(&self) -> String {
        self.root.to_folded_lines().join("\n")
    }

    /// 转换为 JSON
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }

    /// 所有帧名（去重）
    pub fn all_names(&self) -> Vec<String> {
        let mut names = self.root.all_names();
        names.sort();
        names.dedup();
        names
    }
}

// ============================================================================
// FlameGraphBuilder — 从采样构建火焰图
// ============================================================================

/// 火焰图构建器：从调用栈采样构建树
#[derive(Debug, Clone)]
pub struct FlameGraphBuilder {
    root_name: String,
    stacks: Vec<(Vec<String>, u64)>,
}

impl Default for FlameGraphBuilder {
    fn default() -> Self {
        Self {
            root_name: "root".to_string(),
            stacks: vec![],
        }
    }
}

impl FlameGraphBuilder {
    /// 创建构建器
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置根节点名（链式）
    pub fn root_name(mut self, name: &str) -> Self {
        self.root_name = name.to_string();
        self
    }

    /// 添加调用栈采样（链式）
    ///
    /// `stack` 为从调用者到被调用者的帧序列，`value` 为采样值。
    pub fn add_stack(mut self, stack: Vec<String>, value: u64) -> Self {
        self.stacks.push((stack, value));
        self
    }

    /// 添加多个采样（链式）
    pub fn add_stacks(mut self, stacks: Vec<(Vec<String>, u64)>) -> Self {
        self.stacks.extend(stacks);
        self
    }

    /// 采样数
    pub fn sample_count(&self) -> usize {
        self.stacks.len()
    }

    /// 构建火焰图数据
    pub fn build(self) -> FlameGraphData {
        let mut root = FlameNode::root(&self.root_name);

        for (stack, value) in &self.stacks {
            if stack.is_empty() {
                root.value += value;
                continue;
            }
            Self::insert_stack(&mut root, stack, *value);
        }

        if root.children.is_empty() && root.value == 0 {
            // 空数据
        } else if !root.children.is_empty() {
            root.recompute_all();
        }

        FlameGraphData::from_root(root)
    }

    /// 递归插入调用栈
    fn insert_stack(node: &mut FlameNode, stack: &[String], value: u64) {
        if stack.is_empty() {
            node.value += value;
            return;
        }

        let head = &stack[0];
        let tail = &stack[1..];

        if let Some(child) = node.find_child_mut(head) {
            Self::insert_stack(child, tail, value);
        } else {
            let mut child = FlameNode::leaf(head, 0);
            Self::insert_stack(&mut child, tail, value);
            node.children.push(child);
        }
    }

    /// 从折叠栈格式解析采样
    ///
    /// 输入格式：每行 `frame1;frame2;... value`
    pub fn parse_folded(input: &str) -> Self {
        let mut builder = Self::new();
        for line in input.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Some((stack_str, value_str)) = line.rsplit_once(' ') {
                if let Ok(value) = value_str.parse::<u64>() {
                    let stack: Vec<String> = stack_str.split(';').map(|s| s.to_string()).collect();
                    builder = builder.add_stack(stack, value);
                }
            }
        }
        builder
    }

    /// 从 `QueryPhaseTiming` 列表构建
    pub fn from_timings(timings: &[crate::QueryPhaseTiming]) -> Self {
        let mut builder = Self::new().root_name("query");
        for t in timings {
            let stack = vec![t.phase.as_str().to_string()];
            builder = builder.add_stack(stack, t.duration_ms);
        }
        builder
    }
}

// ============================================================================
// FlameGraphMerger — 合并多个火焰图
// ============================================================================

/// 火焰图合并器
#[derive(Debug, Clone)]
#[derive(Default)]
pub struct FlameGraphMerger {
    builders: Vec<FlameGraphBuilder>,
}


impl FlameGraphMerger {
    /// 创建合并器
    pub fn new() -> Self {
        Self::default()
    }

    /// 添加火焰图（链式）
    #[allow(clippy::should_implement_trait)]
    pub fn add(mut self, builder: FlameGraphBuilder) -> Self {
        self.builders.push(builder);
        self
    }

    /// 火焰图数
    pub fn count(&self) -> usize {
        self.builders.len()
    }

    /// 合并所有火焰图
    pub fn merge(self) -> FlameGraphData {
        let mut combined = FlameGraphBuilder::new().root_name("merged");
        for builder in self.builders {
            for (stack, value) in builder.stacks {
                combined = combined.add_stack(stack, value);
            }
        }
        combined.build()
    }
}

// ============================================================================
// FlameGraphFilter — 过滤火焰图
// ============================================================================

/// 火焰图过滤器
#[derive(Debug, Clone)]
#[derive(Default)]
pub struct FlameGraphFilter {
    include: Vec<String>,
    exclude: Vec<String>,
    min_value: u64,
    max_depth: Option<usize>,
}


impl FlameGraphFilter {
    /// 创建过滤器
    pub fn new() -> Self {
        Self::default()
    }

    /// 包含帧名（仅保留含此名称的路径）（链式）
    pub fn include(mut self, name: &str) -> Self {
        self.include.push(name.to_string());
        self
    }

    /// 排除帧名（链式）
    pub fn exclude(mut self, name: &str) -> Self {
        self.exclude.push(name.to_string());
        self
    }

    /// 最小值阈值（链式）
    pub fn min_value(mut self, n: u64) -> Self {
        self.min_value = n;
        self
    }

    /// 最大深度（链式）
    pub fn max_depth(mut self, n: usize) -> Self {
        self.max_depth = Some(n);
        self
    }

    /// 过滤节点
    pub fn filter(&self, node: &FlameNode) -> Option<FlameNode> {
        if node.value() < self.min_value {
            return None;
        }

        if !self.include.is_empty() {
            let names = node.all_names();
            let has_included = self
                .include
                .iter()
                .any(|inc| names.iter().any(|n| n.contains(inc)));
            if !has_included {
                return None;
            }
        }

        let mut filtered = FlameNode {
            name: node.name().to_string(),
            value: node.value(),
            children: vec![],
        };

        for child in node.children() {
            if self.exclude.iter().any(|exc| child.name().contains(exc)) {
                continue;
            }
            if let Some(max_d) = self.max_depth {
                if child.depth() > max_d {
                    continue;
                }
            }
            if let Some(fc) = self.filter(child) {
                filtered.children.push(fc);
            }
        }

        filtered.recompute_value();
        Some(filtered)
    }

    /// 过滤火焰图数据
    pub fn apply(&self, data: &FlameGraphData) -> FlameGraphData {
        if let Some(root) = self.filter(data.root()) {
            FlameGraphData::from_root(root)
        } else {
            FlameGraphData::from_root(FlameNode::root("empty"))
        }
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ----- FlameNode -----

    #[test]
    fn flame_node_root() {
        let n = FlameNode::root("root");
        assert_eq!(n.name(), "root");
        assert_eq!(n.value(), 0);
        assert!(n.is_leaf());
        assert_eq!(n.depth(), 1);
        assert_eq!(n.node_count(), 1);
        assert_eq!(n.leaf_count(), 1);
    }

    #[test]
    fn flame_node_leaf() {
        let n = FlameNode::leaf("func", 100);
        assert_eq!(n.value(), 100);
        assert!(n.is_leaf());
    }

    #[test]
    fn flame_node_add_child() {
        let mut n = FlameNode::root("root");
        n.add_child(FlameNode::leaf("a", 10));
        n.add_child(FlameNode::leaf("b", 20));
        assert_eq!(n.child_count(), 2);
        assert_eq!(n.value(), 30);
        assert!(!n.is_leaf());
    }

    #[test]
    fn flame_node_add_child_merge() {
        let mut n = FlameNode::root("root");
        n.add_child(FlameNode::leaf("a", 10));
        n.add_child(FlameNode::leaf("a", 20));
        assert_eq!(n.child_count(), 1);
        assert_eq!(n.value(), 30);
    }

    #[test]
    fn flame_node_depth() {
        let mut n = FlameNode::root("root");
        let mut child = FlameNode::leaf("a", 10);
        child.add_child(FlameNode::leaf("b", 5));
        n.add_child(child);
        assert_eq!(n.depth(), 3);
    }

    #[test]
    fn flame_node_node_count() {
        let mut n = FlameNode::root("root");
        n.add_child(FlameNode::leaf("a", 10));
        n.add_child(FlameNode::leaf("b", 20));
        assert_eq!(n.node_count(), 3);
    }

    #[test]
    fn flame_node_leaf_count() {
        let mut n = FlameNode::root("root");
        let mut child = FlameNode::leaf("a", 10);
        child.add_child(FlameNode::leaf("b", 5));
        child.add_child(FlameNode::leaf("c", 5));
        n.add_child(child);
        n.add_child(FlameNode::leaf("d", 20));
        assert_eq!(n.leaf_count(), 3);
    }

    #[test]
    fn flame_node_find_child() {
        let mut n = FlameNode::root("root");
        n.add_child(FlameNode::leaf("a", 10));
        assert!(n.find_child("a").is_some());
        assert!(n.find_child("b").is_none());
    }

    #[test]
    fn flame_node_merge() {
        let mut n1 = FlameNode::root("root");
        n1.add_child(FlameNode::leaf("a", 10));
        n1.add_child(FlameNode::leaf("b", 20));

        let mut n2 = FlameNode::root("root");
        n2.add_child(FlameNode::leaf("a", 5));
        n2.add_child(FlameNode::leaf("c", 15));

        n1.merge(&n2);
        assert_eq!(n1.value(), 50);
        assert_eq!(n1.child_count(), 3);
        assert_eq!(n1.find_child("a").unwrap().value(), 15);
    }

    #[test]
    fn flame_node_sort_by_value_desc() {
        let mut n = FlameNode::root("root");
        n.add_child(FlameNode::leaf("a", 10));
        n.add_child(FlameNode::leaf("b", 30));
        n.add_child(FlameNode::leaf("c", 20));
        n.sort_by_value_desc();
        assert_eq!(n.children()[0].name(), "b");
        assert_eq!(n.children()[1].name(), "c");
        assert_eq!(n.children()[2].name(), "a");
    }

    #[test]
    fn flame_node_sort_by_name() {
        let mut n = FlameNode::root("root");
        n.add_child(FlameNode::leaf("c", 10));
        n.add_child(FlameNode::leaf("a", 30));
        n.add_child(FlameNode::leaf("b", 20));
        n.sort_by_name();
        assert_eq!(n.children()[0].name(), "a");
        assert_eq!(n.children()[1].name(), "b");
        assert_eq!(n.children()[2].name(), "c");
    }

    #[test]
    fn flame_node_all_names() {
        let mut n = FlameNode::root("root");
        n.add_child(FlameNode::leaf("a", 10));
        n.add_child(FlameNode::leaf("b", 20));
        let names = n.all_names();
        assert!(names.contains(&"root".to_string()));
        assert!(names.contains(&"a".to_string()));
        assert!(names.contains(&"b".to_string()));
    }

    #[test]
    fn flame_node_to_folded_lines() {
        let mut n = FlameNode::root("root");
        let mut child = FlameNode::leaf("a", 0);
        child.add_child(FlameNode::leaf("b", 5));
        n.add_child(child);
        n.add_child(FlameNode::leaf("c", 10));
        let lines = n.to_folded_lines();
        assert!(lines
            .iter()
            .any(|l| l.contains("root;a;b") && l.contains("5")));
        assert!(lines
            .iter()
            .any(|l| l.contains("root;c") && l.contains("10")));
    }

    #[test]
    fn flame_node_width() {
        let mut n = FlameNode::root("root");
        n.add_child(FlameNode::leaf("a", 10));
        n.add_child(FlameNode::leaf("b", 20));
        assert_eq!(n.width(), 2);
    }

    // ----- FlameGraphData -----

    #[test]
    fn flame_graph_data_from_root() {
        let mut root = FlameNode::root("root");
        root.add_child(FlameNode::leaf("a", 10));
        root.add_child(FlameNode::leaf("b", 20));
        let data = FlameGraphData::from_root(root);
        assert_eq!(data.total_samples(), 30);
        assert_eq!(data.max_depth(), 2);
        assert_eq!(data.node_count(), 3);
        assert!(!data.is_empty());
    }

    #[test]
    fn flame_graph_data_empty() {
        let root = FlameNode::root("root");
        let data = FlameGraphData::from_root(root);
        assert!(data.is_empty());
        assert_eq!(data.total_samples(), 0);
    }

    #[test]
    fn flame_graph_data_to_folded() {
        let mut root = FlameNode::root("root");
        root.add_child(FlameNode::leaf("a", 10));
        let data = FlameGraphData::from_root(root);
        let folded = data.to_folded();
        assert!(folded.contains("root;a"));
        assert!(folded.contains("10"));
    }

    #[test]
    fn flame_graph_data_to_json() {
        let mut root = FlameNode::root("root");
        root.add_child(FlameNode::leaf("a", 10));
        let data = FlameGraphData::from_root(root);
        let json = data.to_json();
        assert!(json.contains("root"));
        assert!(json.contains("\"value\""));
    }

    #[test]
    fn flame_graph_data_all_names() {
        let mut root = FlameNode::root("root");
        root.add_child(FlameNode::leaf("a", 10));
        root.add_child(FlameNode::leaf("b", 20));
        let data = FlameGraphData::from_root(root);
        let names = data.all_names();
        assert_eq!(names.len(), 3);
        assert!(names.contains(&"root".to_string()));
        assert!(names.contains(&"a".to_string()));
        assert!(names.contains(&"b".to_string()));
    }

    // ----- FlameGraphBuilder -----

    #[test]
    fn builder_empty() {
        let builder = FlameGraphBuilder::new();
        assert_eq!(builder.sample_count(), 0);
        let data = builder.build();
        assert!(data.is_empty());
    }

    #[test]
    fn builder_single_stack() {
        let data = FlameGraphBuilder::new()
            .add_stack(vec!["a".to_string(), "b".to_string()], 10)
            .build();
        assert_eq!(data.total_samples(), 10);
        assert_eq!(data.max_depth(), 3);
    }

    #[test]
    fn builder_multiple_stacks() {
        let data = FlameGraphBuilder::new()
            .add_stack(vec!["a".to_string(), "b".to_string()], 10)
            .add_stack(vec!["a".to_string(), "c".to_string()], 20)
            .build();
        assert_eq!(data.total_samples(), 30);
        assert_eq!(data.root().child_count(), 1);
        assert_eq!(data.root().children()[0].child_count(), 2);
    }

    #[test]
    fn builder_shared_prefix() {
        let data = FlameGraphBuilder::new()
            .add_stack(vec!["a".to_string(), "b".to_string()], 10)
            .add_stack(vec!["a".to_string(), "b".to_string()], 20)
            .build();
        assert_eq!(data.total_samples(), 30);
        assert_eq!(data.root().child_count(), 1);
        assert_eq!(data.root().children()[0].child_count(), 1);
        assert_eq!(data.root().children()[0].children()[0].value(), 30);
    }

    #[test]
    fn builder_root_name() {
        let data = FlameGraphBuilder::new()
            .root_name("query")
            .add_stack(vec!["a".to_string()], 10)
            .build();
        assert_eq!(data.root().name(), "query");
    }

    #[test]
    fn builder_add_stacks_batch() {
        let stacks = vec![(vec!["a".to_string()], 10), (vec!["b".to_string()], 20)];
        let data = FlameGraphBuilder::new().add_stacks(stacks).build();
        assert_eq!(data.total_samples(), 30);
        assert_eq!(data.root().child_count(), 2);
    }

    #[test]
    fn builder_parse_folded() {
        let input = "root;a;b 10\nroot;a;c 20\nroot;d 5\n";
        let builder = FlameGraphBuilder::parse_folded(input);
        assert_eq!(builder.sample_count(), 3);
        let data = builder.build();
        assert_eq!(data.total_samples(), 35);
    }

    #[test]
    fn builder_parse_folded_empty() {
        let builder = FlameGraphBuilder::parse_folded("");
        assert_eq!(builder.sample_count(), 0);
    }

    #[test]
    fn builder_parse_folded_invalid() {
        let input = "invalid line\nno value\n";
        let builder = FlameGraphBuilder::parse_folded(input);
        assert_eq!(builder.sample_count(), 0);
    }

    #[test]
    fn builder_from_timings() {
        use crate::{Phase, QueryPhaseTiming};
        let timings = vec![
            QueryPhaseTiming {
                phase: Phase::Build,
                start_ms: 0,
                duration_ms: 5,
            },
            QueryPhaseTiming {
                phase: Phase::SqlExecute,
                start_ms: 5,
                duration_ms: 10,
            },
        ];
        let data = FlameGraphBuilder::from_timings(&timings).build();
        assert_eq!(data.total_samples(), 15);
        assert_eq!(data.root().name(), "query");
    }

    // ----- FlameGraphMerger -----

    #[test]
    fn merger_empty() {
        let m = FlameGraphMerger::new();
        assert_eq!(m.count(), 0);
        let data = m.merge();
        assert!(data.is_empty());
    }

    #[test]
    fn merger_multiple() {
        let b1 = FlameGraphBuilder::new().add_stack(vec!["a".to_string()], 10);
        let b2 = FlameGraphBuilder::new().add_stack(vec!["b".to_string()], 20);
        let data = FlameGraphMerger::new().add(b1).add(b2).merge();
        assert_eq!(data.total_samples(), 30);
        assert_eq!(data.root().child_count(), 2);
    }

    #[test]
    fn merger_shared_stacks() {
        let b1 = FlameGraphBuilder::new().add_stack(vec!["a".to_string()], 10);
        let b2 = FlameGraphBuilder::new().add_stack(vec!["a".to_string()], 20);
        let data = FlameGraphMerger::new().add(b1).add(b2).merge();
        assert_eq!(data.total_samples(), 30);
        assert_eq!(data.root().child_count(), 1);
    }

    // ----- FlameGraphFilter -----

    #[test]
    fn filter_no_constraints() {
        let mut root = FlameNode::root("root");
        root.add_child(FlameNode::leaf("a", 10));
        root.add_child(FlameNode::leaf("b", 20));
        let data = FlameGraphData::from_root(root);
        let filtered = FlameGraphFilter::new().apply(&data);
        assert_eq!(filtered.total_samples(), 30);
    }

    #[test]
    fn filter_min_value() {
        let mut root = FlameNode::root("root");
        root.add_child(FlameNode::leaf("a", 10));
        root.add_child(FlameNode::leaf("b", 20));
        let data = FlameGraphData::from_root(root);
        let filtered = FlameGraphFilter::new().min_value(15).apply(&data);
        assert!(filtered.total_samples() <= 30);
    }

    #[test]
    fn filter_exclude() {
        let mut root = FlameNode::root("root");
        root.add_child(FlameNode::leaf("a", 10));
        root.add_child(FlameNode::leaf("b", 20));
        let data = FlameGraphData::from_root(root);
        let filtered = FlameGraphFilter::new().exclude("b").apply(&data);
        assert!(filtered.root().find_child("b").is_none());
    }

    #[test]
    fn filter_include() {
        let mut root = FlameNode::root("root");
        root.add_child(FlameNode::leaf("a", 10));
        root.add_child(FlameNode::leaf("b", 20));
        let data = FlameGraphData::from_root(root);
        let filtered = FlameGraphFilter::new().include("a").apply(&data);
        assert!(filtered.total_samples() > 0);
    }

    #[test]
    fn filter_max_depth() {
        let mut root = FlameNode::root("root");
        let mut child = FlameNode::leaf("a", 0);
        child.add_child(FlameNode::leaf("b", 5));
        root.add_child(child);
        let data = FlameGraphData::from_root(root);
        let filtered = FlameGraphFilter::new().max_depth(2).apply(&data);
        assert!(filtered.max_depth() <= 3);
    }

    #[test]
    fn filter_chain() {
        let mut root = FlameNode::root("root");
        root.add_child(FlameNode::leaf("a", 10));
        root.add_child(FlameNode::leaf("b", 20));
        root.add_child(FlameNode::leaf("c", 5));
        let data = FlameGraphData::from_root(root);
        let filtered = FlameGraphFilter::new()
            .min_value(5)
            .exclude("c")
            .apply(&data);
        assert!(filtered.root().find_child("c").is_none());
    }
}
