//! 死锁检测器与锁等待分析器
//!
//! 基于等待图（wait-for graph）的环检测实现死锁检测，
//! 锁等待分析器聚合等待事件提供统计与建议。
//! 本模块不依赖 `slow-query-diagnosis` feature，可独立使用。

use std::collections::{HashMap, HashSet, VecDeque};

/// 锁等待事件
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockWaitEvent {
    /// 等待的事务 ID
    pub transaction_id: u64,
    /// 持有锁的事务 ID
    pub blocking_transaction_id: u64,
    /// 争用的资源名
    pub resource: String,
    /// 等待时长（毫秒）
    pub wait_ms: u64,
}

/// 锁等待分析结果
#[derive(Debug, Clone)]
pub struct LockWaitAnalysis {
    /// 总等待事件数
    pub total_events: usize,
    /// 涉及的事务数
    pub involved_transactions: usize,
    /// 争用的资源数
    pub contended_resources: usize,
    /// 最大单次等待时长（毫秒）
    pub max_wait_ms: u64,
    /// 平均等待时长（毫秒）
    pub avg_wait_ms: f64,
    /// Top-N 争用资源（资源名, 等待次数）
    pub top_contended: Vec<(String, usize)>,
    /// 是否存在死锁风险
    pub deadlock_risk: bool,
}

/// 锁等待分析器
///
/// 聚合 [`LockWaitEvent`] 提供统计与争用分析。
pub struct LockWaitAnalyzer {
    events: Vec<LockWaitEvent>,
    /// 用于死锁检测的等待图
    wait_graph: HashMap<u64, HashSet<u64>>,
}

impl LockWaitAnalyzer {
    /// 创建空分析器
    pub fn new() -> Self {
        Self {
            events: Vec::new(),
            wait_graph: HashMap::new(),
        }
    }

    /// 添加锁等待事件
    pub fn add_event(&mut self, event: LockWaitEvent) {
        self.wait_graph
            .entry(event.transaction_id)
            .or_default()
            .insert(event.blocking_transaction_id);
        self.events.push(event);
    }

    /// 批量添加事件

    pub fn add_events(&mut self, events: Vec<LockWaitEvent>) {
        for event in events {
            self.add_event(event);
        }
    }

    /// 分析锁等待
    pub fn analyze(&self) -> LockWaitAnalysis {
        let total_events = self.events.len();
        let max_wait_ms = self.events.iter().map(|e| e.wait_ms).max().unwrap_or(0);
        let avg_wait_ms = if total_events == 0 {
            0.0
        } else {
            let sum: u64 = self.events.iter().map(|e| e.wait_ms).sum();
            sum as f64 / total_events as f64
        };

        let mut transactions: HashSet<u64> = HashSet::new();
        let mut resources: HashSet<&str> = HashSet::new();
        let mut resource_counts: HashMap<&str, usize> = HashMap::new();

        for event in &self.events {
            transactions.insert(event.transaction_id);
            transactions.insert(event.blocking_transaction_id);
            resources.insert(event.resource.as_str());
            *resource_counts.entry(event.resource.as_str()).or_insert(0) += 1;
        }

        let mut top_contended: Vec<(String, usize)> = resource_counts
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        top_contended.sort_by_key(|(_, v)| std::cmp::Reverse(*v));
        top_contended.truncate(5);

        let deadlock_risk = self.detect_cycle().is_some();

        LockWaitAnalysis {
            total_events,
            involved_transactions: transactions.len(),
            contended_resources: resources.len(),
            max_wait_ms,
            avg_wait_ms,
            top_contended,
            deadlock_risk,
        }
    }

    /// 检测等待图中的环（死锁）
    ///
    /// 返回环路径（事务 ID 序列），无环返回 `None`。
    pub fn detect_cycle(&self) -> Option<Vec<u64>> {
        let mut visited: HashSet<u64> = HashSet::new();
        let mut path: Vec<u64> = Vec::new();
        let mut path_set: HashSet<u64> = HashSet::new();

        let all_nodes: Vec<u64> = {
            let mut nodes: HashSet<u64> = HashSet::new();
            for (&k, vs) in &self.wait_graph {
                nodes.insert(k);
                for &v in vs {
                    nodes.insert(v);
                }
            }
            nodes.into_iter().collect()
        };

        for &node in &all_nodes {
            if !visited.contains(&node) {
                if self.dfs_find_cycle(node, &mut visited, &mut path, &mut path_set) {
                    return Some(path);
                }
            }
        }
        None
    }

    fn dfs_find_cycle(
        &self,
        node: u64,
        visited: &mut HashSet<u64>,
        path: &mut Vec<u64>,
        path_set: &mut HashSet<u64>,
    ) -> bool {
        visited.insert(node);
        path.push(node);
        path_set.insert(node);

        if let Some(neighbors) = self.wait_graph.get(&node) {
            for &next in neighbors {
                if !visited.contains(&next) {
                    if self.dfs_find_cycle(next, visited, path, path_set) {
                        return true;
                    }
                } else if path_set.contains(&next) {
                    // 找到环，截取环路径
                    let start = path.iter().position(|&n| n == next).unwrap_or(0);
                    let cycle: Vec<u64> = path[start..].to_vec();
                    *path = cycle;
                    return true;
                }
            }
        }

        path_set.remove(&node);
        path.pop();
        false
    }

    /// 所有事件引用
    pub fn events(&self) -> &[LockWaitEvent] {
        &self.events
    }

    /// 清空所有事件
    pub fn clear(&mut self) {
        self.events.clear();
        self.wait_graph.clear();
    }
}

impl Default for LockWaitAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

/// 死锁检测器
///
/// 基于等待图（wait-for graph）实现死锁检测。
/// 当事务 A 等待事务 B 持有的锁时，添加边 A→B；
/// 图中存在环即存在死锁。
pub struct DeadlockDetector {
    /// 等待图：事务 A → 它等待的事务集合
    wait_graph: HashMap<u64, HashSet<u64>>,
    /// 边的资源标注：(from, to) -> resource
    edge_resources: HashMap<(u64, u64), String>,
}

impl DeadlockDetector {
    /// 创建空检测器
    pub fn new() -> Self {
        Self {
            wait_graph: HashMap::new(),
            edge_resources: HashMap::new(),
        }
    }

    /// 添加等待边：`tx_id` 等待 `blocking_tx_id` 持有的 `resource`
    pub fn add_wait_edge(&mut self, tx_id: u64, blocking_tx_id: u64, resource: impl Into<String>) {
        self.wait_graph
            .entry(tx_id)
            .or_default()
            .insert(blocking_tx_id);
        self.edge_resources
            .insert((tx_id, blocking_tx_id), resource.into());
    }

    /// 移除等待边（事务获取到锁或超时）
    pub fn remove_wait_edge(&mut self, tx_id: u64, blocking_tx_id: u64) {
        if let Some(neighbors) = self.wait_graph.get_mut(&tx_id) {
            neighbors.remove(&blocking_tx_id);
            if neighbors.is_empty() {
                self.wait_graph.remove(&tx_id);
            }
        }
        self.edge_resources.remove(&(tx_id, blocking_tx_id));
    }

    /// 事务完成，移除所有相关边
    pub fn complete_transaction(&mut self, tx_id: u64) {
        self.wait_graph.remove(&tx_id);
        // 移除其他事务等待 tx_id 的边
        for (_, neighbors) in self.wait_graph.iter_mut() {
            neighbors.remove(&tx_id);
        }
        // 清理空邻居
        self.wait_graph.retain(|_, neighbors| !neighbors.is_empty());
        // 清理边资源标注
        self.edge_resources
            .retain(|(from, to), _| *from != tx_id && *to != tx_id);
    }

    /// 检测是否存在死锁
    pub fn has_deadlock(&self) -> bool {
        self.detect_deadlock().is_some()
    }

    /// 检测死锁，返回死锁环路径
    ///
    /// 返回 `(事务 ID 序列, 涉及的资源列表)`，无死锁返回 `None`。
    pub fn detect_deadlock(&self) -> Option<(Vec<u64>, Vec<String>)> {
        let cycle = self.find_cycle()?;
        let resources: Vec<String> = cycle
            .windows(2)
            .filter_map(|w| self.edge_resources.get(&(w[0], w[1])).cloned())
            .collect();
        // 处理环的闭合边（最后 → 第一个）
        let mut full_resources = resources;
        if cycle.len() > 1 {
            if let Some(r) = self.edge_resources.get(&(cycle[cycle.len() - 1], cycle[0])) {
                full_resources.push(r.clone());
            }
        }
        Some((cycle, full_resources))
    }

    fn find_cycle(&self) -> Option<Vec<u64>> {
        let mut visited: HashSet<u64> = HashSet::new();
        let mut path: Vec<u64> = Vec::new();
        let mut path_set: HashSet<u64> = HashSet::new();

        let all_nodes: Vec<u64> = {
            let mut nodes: HashSet<u64> = HashSet::new();
            for (&k, vs) in &self.wait_graph {
                nodes.insert(k);
                for &v in vs {
                    nodes.insert(v);
                }
            }
            nodes.into_iter().collect()
        };

        for &node in &all_nodes {
            if !visited.contains(&node) {
                if self.dfs_cycle(node, &mut visited, &mut path, &mut path_set) {
                    return Some(path);
                }
            }
        }
        None
    }

    fn dfs_cycle(
        &self,
        node: u64,
        visited: &mut HashSet<u64>,
        path: &mut Vec<u64>,
        path_set: &mut HashSet<u64>,
    ) -> bool {
        visited.insert(node);
        path.push(node);
        path_set.insert(node);

        if let Some(neighbors) = self.wait_graph.get(&node) {
            for &next in neighbors {
                if !visited.contains(&next) {
                    if self.dfs_cycle(next, visited, path, path_set) {
                        return true;
                    }
                } else if path_set.contains(&next) {
                    let start = path.iter().position(|&n| n == next).unwrap_or(0);
                    let cycle: Vec<u64> = path[start..].to_vec();
                    *path = cycle;
                    return true;
                }
            }
        }

        path_set.remove(&node);
        path.pop();
        false
    }

    /// 当前等待边数
    pub fn edge_count(&self) -> usize {
        self.wait_graph
            .values()
            .map(|neighbors| neighbors.len())
            .sum()
    }

    /// 涉及的事务数
    pub fn transaction_count(&self) -> usize {
        let mut nodes: HashSet<u64> = HashSet::new();
        for (&k, vs) in &self.wait_graph {
            nodes.insert(k);
            for &v in vs {
                nodes.insert(v);
            }
        }
        nodes.len()
    }

    /// 获取事务正在等待的所有事务
    pub fn waiting_for(&self, tx_id: u64) -> Option<&HashSet<u64>> {
        self.wait_graph.get(&tx_id)
    }
}

impl Default for DeadlockDetector {
    fn default() -> Self {
        Self::new()
    }
}

/// 死锁预防策略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeadlockPreventionStrategy {
    /// 保守策略：事务开始时获取所有需要的锁
    Conservative,
    /// 顺序加锁：按资源 ID 升序加锁
    OrderedLocking,
    /// 超时回滚：等待超时自动回滚
    TimeoutAbort,
    /// 伤害等待（Wound-Wait）：老事务伤害新事务
    WoundWait,
    /// 等待伤害（Wait-Die）：新事务等待老事务
    WaitDie,
}

impl DeadlockPreventionStrategy {
    /// 策略描述
    pub fn description(&self) -> &'static str {
        match self {
            DeadlockPreventionStrategy::Conservative => {
                "conservative: acquire all locks at transaction start"
            }
            DeadlockPreventionStrategy::OrderedLocking => {
                "ordered-locking: acquire locks in resource-id ascending order"
            }
            DeadlockPreventionStrategy::TimeoutAbort => "timeout-abort: rollback on wait timeout",
            DeadlockPreventionStrategy::WoundWait => {
                "wound-wait: old transaction wounds (aborts) young transaction"
            }
            DeadlockPreventionStrategy::WaitDie => {
                "wait-die: young transaction waits for old transaction"
            }
        }
    }

    /// 是否需要事务时间戳
    pub fn requires_timestamp(&self) -> bool {
        matches!(
            self,
            DeadlockPreventionStrategy::WoundWait | DeadlockPreventionStrategy::WaitDie
        )
    }
}

/// 死锁解析建议
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeadlockResolution {
    /// 需要回滚的事务 ID
    pub victim_transaction: u64,
    /// 死锁涉及的资源
    pub resources: Vec<String>,
    /// 建议的预防策略
    pub prevention_strategy: DeadlockPreventionStrategy,
}

impl DeadlockResolution {
    /// 创建解析建议
    pub fn new(
        victim_transaction: u64,
        resources: Vec<String>,
        prevention_strategy: DeadlockPreventionStrategy,
    ) -> Self {
        Self {
            victim_transaction,
            resources,
            prevention_strategy,
        }
    }

    /// 选择牺牲者（死锁环中事务 ID 最小的作为牺牲者）
    pub fn select_victim(cycle: &[u64]) -> u64 {
        *cycle.iter().min().unwrap_or(&0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(tx: u64, blocking: u64, resource: &str, ms: u64) -> LockWaitEvent {
        LockWaitEvent {
            transaction_id: tx,
            blocking_transaction_id: blocking,
            resource: resource.to_string(),
            wait_ms: ms,
        }
    }

    // --- LockWaitEvent tests ---

    #[test]
    fn event_equality() {
        let e1 = event(1, 2, "row_a", 100);
        let e2 = event(1, 2, "row_a", 100);
        assert_eq!(e1, e2);
    }

    #[test]
    fn event_inequality() {
        let e1 = event(1, 2, "row_a", 100);
        let e2 = event(1, 2, "row_b", 100);
        assert_ne!(e1, e2);
    }

    // --- LockWaitAnalyzer tests ---

    #[test]
    fn analyzer_empty() {
        let a = LockWaitAnalyzer::new();
        let analysis = a.analyze();
        assert_eq!(analysis.total_events, 0);
        assert_eq!(analysis.involved_transactions, 0);
        assert_eq!(analysis.contended_resources, 0);
        assert_eq!(analysis.max_wait_ms, 0);
        assert_eq!(analysis.avg_wait_ms, 0.0);
        assert!(!analysis.deadlock_risk);
    }

    #[test]
    fn analyzer_single_event() {
        let mut a = LockWaitAnalyzer::new();
        a.add_event(event(1, 2, "row_a", 100));
        let analysis = a.analyze();
        assert_eq!(analysis.total_events, 1);
        assert_eq!(analysis.involved_transactions, 2);
        assert_eq!(analysis.contended_resources, 1);
        assert_eq!(analysis.max_wait_ms, 100);
        assert!((analysis.avg_wait_ms - 100.0).abs() < 1e-9);
    }

    #[test]
    fn analyzer_multiple_events() {
        let mut a = LockWaitAnalyzer::new();
        a.add_event(event(1, 2, "row_a", 100));
        a.add_event(event(3, 2, "row_a", 200));
        a.add_event(event(4, 5, "row_b", 50));
        let analysis = a.analyze();
        assert_eq!(analysis.total_events, 3);
        assert_eq!(analysis.involved_transactions, 5);
        assert_eq!(analysis.contended_resources, 2);
        assert_eq!(analysis.max_wait_ms, 200);
    }

    #[test]
    fn analyzer_avg_wait() {
        let mut a = LockWaitAnalyzer::new();
        a.add_event(event(1, 2, "r", 100));
        a.add_event(event(3, 4, "r", 300));
        let analysis = a.analyze();
        assert!((analysis.avg_wait_ms - 200.0).abs() < 1e-9);
    }

    #[test]
    fn analyzer_top_contended() {
        let mut a = LockWaitAnalyzer::new();
        a.add_event(event(1, 2, "row_a", 10));
        a.add_event(event(3, 2, "row_a", 10));
        a.add_event(event(4, 2, "row_a", 10));
        a.add_event(event(5, 6, "row_b", 10));
        let analysis = a.analyze();
        assert_eq!(analysis.top_contended[0].0, "row_a");
        assert_eq!(analysis.top_contended[0].1, 3);
    }

    #[test]
    fn analyzer_detects_deadlock_risk() {
        let mut a = LockWaitAnalyzer::new();
        // 1→2, 2→1 形成环
        a.add_event(event(1, 2, "row_a", 100));
        a.add_event(event(2, 1, "row_b", 100));
        let analysis = a.analyze();
        assert!(analysis.deadlock_risk);
    }

    #[test]
    fn analyzer_no_deadlock_risk() {
        let mut a = LockWaitAnalyzer::new();
        // 1→2, 1→3, 2→3 无环
        a.add_event(event(1, 2, "row_a", 100));
        a.add_event(event(1, 3, "row_b", 100));
        a.add_event(event(2, 3, "row_c", 100));
        let analysis = a.analyze();
        assert!(!analysis.deadlock_risk);
    }

    #[test]
    fn analyzer_detect_cycle_returns_path() {
        let mut a = LockWaitAnalyzer::new();
        a.add_event(event(1, 2, "row_a", 100));
        a.add_event(event(2, 3, "row_b", 100));
        a.add_event(event(3, 1, "row_c", 100));
        let cycle = a.detect_cycle().expect("should detect cycle");
        assert!(cycle.len() >= 2);
    }

    #[test]
    fn analyzer_clear() {
        let mut a = LockWaitAnalyzer::new();
        a.add_event(event(1, 2, "r", 10));
        a.clear();
        assert_eq!(a.events().len(), 0);
        assert!(a.detect_cycle().is_none());
    }

    #[test]
    fn analyzer_add_events_batch() {
        let mut a = LockWaitAnalyzer::new();
        let events = vec![event(1, 2, "r", 10), event(3, 4, "r", 20)];
        a.add_events(events);
        assert_eq!(a.events().len(), 2);
    }

    #[test]
    fn analyzer_events_ref() {
        let mut a = LockWaitAnalyzer::new();
        a.add_event(event(1, 2, "r", 10));
        assert_eq!(a.events().len(), 1);
    }

    #[test]
    fn analyzer_default() {
        let a = LockWaitAnalyzer::default();
        assert_eq!(a.events().len(), 0);
    }

    // --- DeadlockDetector tests ---

    #[test]
    fn detector_no_deadlock_initially() {
        let d = DeadlockDetector::new();
        assert!(!d.has_deadlock());
        assert_eq!(d.edge_count(), 0);
        assert_eq!(d.transaction_count(), 0);
    }

    #[test]
    fn detector_single_edge_no_deadlock() {
        let mut d = DeadlockDetector::new();
        d.add_wait_edge(1, 2, "row_a");
        assert!(!d.has_deadlock());
        assert_eq!(d.edge_count(), 1);
        assert_eq!(d.transaction_count(), 2);
    }

    #[test]
    fn detector_two_node_cycle() {
        let mut d = DeadlockDetector::new();
        d.add_wait_edge(1, 2, "row_a");
        d.add_wait_edge(2, 1, "row_b");
        assert!(d.has_deadlock());
    }

    #[test]
    fn detector_three_node_cycle() {
        let mut d = DeadlockDetector::new();
        d.add_wait_edge(1, 2, "row_a");
        d.add_wait_edge(2, 3, "row_b");
        d.add_wait_edge(3, 1, "row_c");
        assert!(d.has_deadlock());
    }

    #[test]
    fn detector_no_cycle_dag() {
        let mut d = DeadlockDetector::new();
        d.add_wait_edge(1, 2, "row_a");
        d.add_wait_edge(1, 3, "row_b");
        d.add_wait_edge(2, 3, "row_c");
        assert!(!d.has_deadlock());
    }

    #[test]
    fn detector_detect_deadlock_returns_resources() {
        let mut d = DeadlockDetector::new();
        d.add_wait_edge(1, 2, "row_a");
        d.add_wait_edge(2, 1, "row_b");
        let (cycle, resources) = d.detect_deadlock().expect("should detect deadlock");
        assert!(cycle.len() >= 2);
        assert!(!resources.is_empty());
    }

    #[test]
    fn detector_remove_edge() {
        let mut d = DeadlockDetector::new();
        d.add_wait_edge(1, 2, "row_a");
        d.add_wait_edge(2, 1, "row_b");
        assert!(d.has_deadlock());
        d.remove_wait_edge(2, 1);
        assert!(!d.has_deadlock());
    }

    #[test]
    fn detector_complete_transaction() {
        let mut d = DeadlockDetector::new();
        d.add_wait_edge(1, 2, "row_a");
        d.add_wait_edge(2, 3, "row_b");
        d.add_wait_edge(3, 1, "row_c");
        assert!(d.has_deadlock());
        d.complete_transaction(2);
        assert!(!d.has_deadlock());
    }

    #[test]
    fn detector_waiting_for() {
        let mut d = DeadlockDetector::new();
        d.add_wait_edge(1, 2, "row_a");
        d.add_wait_edge(1, 3, "row_b");
        let waiting = d.waiting_for(1).expect("should have waiting set");
        assert_eq!(waiting.len(), 2);
        assert!(waiting.contains(&2));
        assert!(waiting.contains(&3));
    }

    #[test]
    fn detector_waiting_for_none() {
        let d = DeadlockDetector::new();
        assert!(d.waiting_for(1).is_none());
    }

    #[test]
    fn detector_edge_count() {
        let mut d = DeadlockDetector::new();
        d.add_wait_edge(1, 2, "row_a");
        d.add_wait_edge(1, 3, "row_b");
        d.add_wait_edge(2, 3, "row_c");
        assert_eq!(d.edge_count(), 3);
    }

    #[test]
    fn detector_transaction_count() {
        let mut d = DeadlockDetector::new();
        d.add_wait_edge(1, 2, "row_a");
        d.add_wait_edge(3, 4, "row_b");
        assert_eq!(d.transaction_count(), 4);
    }

    #[test]
    fn detector_default() {
        let d = DeadlockDetector::default();
        assert!(!d.has_deadlock());
    }

    #[test]
    fn detector_self_loop_is_deadlock() {
        let mut d = DeadlockDetector::new();
        d.add_wait_edge(1, 1, "row_a");
        assert!(d.has_deadlock());
    }

    #[test]
    fn detector_complex_graph_with_cycle() {
        let mut d = DeadlockDetector::new();
        // 1→2, 2→3, 3→4, 4→2 (环: 2→3→4→2)
        d.add_wait_edge(1, 2, "r1");
        d.add_wait_edge(2, 3, "r2");
        d.add_wait_edge(3, 4, "r3");
        d.add_wait_edge(4, 2, "r4");
        assert!(d.has_deadlock());
    }

    #[test]
    fn detector_disconnected_components() {
        let mut d = DeadlockDetector::new();
        // 组件 1: 1→2
        d.add_wait_edge(1, 2, "r1");
        // 组件 2: 3→4, 4→3 (环)
        d.add_wait_edge(3, 4, "r2");
        d.add_wait_edge(4, 3, "r3");
        assert!(d.has_deadlock());
    }

    // --- DeadlockPreventionStrategy tests ---

    #[test]
    fn strategy_descriptions_nonempty() {
        assert!(!DeadlockPreventionStrategy::Conservative
            .description()
            .is_empty());
        assert!(!DeadlockPreventionStrategy::OrderedLocking
            .description()
            .is_empty());
        assert!(!DeadlockPreventionStrategy::TimeoutAbort
            .description()
            .is_empty());
        assert!(!DeadlockPreventionStrategy::WoundWait
            .description()
            .is_empty());
        assert!(!DeadlockPreventionStrategy::WaitDie.description().is_empty());
    }

    #[test]
    fn strategy_requires_timestamp() {
        assert!(!DeadlockPreventionStrategy::Conservative.requires_timestamp());
        assert!(!DeadlockPreventionStrategy::OrderedLocking.requires_timestamp());
        assert!(!DeadlockPreventionStrategy::TimeoutAbort.requires_timestamp());
        assert!(DeadlockPreventionStrategy::WoundWait.requires_timestamp());
        assert!(DeadlockPreventionStrategy::WaitDie.requires_timestamp());
    }

    #[test]
    fn strategy_distinct() {
        assert_ne!(
            DeadlockPreventionStrategy::Conservative,
            DeadlockPreventionStrategy::OrderedLocking
        );
        assert_ne!(
            DeadlockPreventionStrategy::WoundWait,
            DeadlockPreventionStrategy::WaitDie
        );
    }

    // --- DeadlockResolution tests ---

    #[test]
    fn resolution_new() {
        let r = DeadlockResolution::new(
            1,
            vec!["row_a".to_string()],
            DeadlockPreventionStrategy::TimeoutAbort,
        );
        assert_eq!(r.victim_transaction, 1);
        assert_eq!(r.resources.len(), 1);
    }

    #[test]
    fn resolution_select_victim_minimum() {
        let cycle = vec![3, 1, 2];
        assert_eq!(DeadlockResolution::select_victim(&cycle), 1);
    }

    #[test]
    fn resolution_select_victim_single() {
        let cycle = vec![5];
        assert_eq!(DeadlockResolution::select_victim(&cycle), 5);
    }

    #[test]
    fn resolution_select_victim_empty() {
        let cycle: Vec<u64> = vec![];
        assert_eq!(DeadlockResolution::select_victim(&cycle), 0);
    }
}
