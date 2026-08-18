//! 火焰图差异分析：比较两个火焰图，找出性能回归/改善。
//!
//! - [`FlameDiff`] — 火焰图差异比较器
//! - [`DiffResult`] — 差异结果
//! - [`DiffEntry`] — 单帧差异
//! - [`DiffType`] — 差异类型

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::flame_node::{FlameGraphData, FlameNode};

// ============================================================================
// DiffType — 差异类型
// ============================================================================

/// 差异类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiffType {
    /// 新增帧（基线中不存在）
    Added,
    /// 删除帧（目标中不存在）
    Removed,
    /// 值增加（回归）
    Increased,
    /// 值减少（改善）
    Decreased,
    /// 值不变
    Unchanged,
}

impl DiffType {
    /// 是否为回归
    pub fn is_regression(&self) -> bool {
        matches!(self, DiffType::Added | DiffType::Increased)
    }

    /// 是否为改善
    pub fn is_improvement(&self) -> bool {
        matches!(self, DiffType::Removed | DiffType::Decreased)
    }

    /// 是否无变化
    pub fn is_unchanged(&self) -> bool {
        matches!(self, DiffType::Unchanged)
    }

    /// 字符串表示
    pub fn as_str(&self) -> &'static str {
        match self {
            DiffType::Added => "added",
            DiffType::Removed => "removed",
            DiffType::Increased => "increased",
            DiffType::Decreased => "decreased",
            DiffType::Unchanged => "unchanged",
        }
    }

    /// 符号表示
    pub fn symbol(&self) -> &'static str {
        match self {
            DiffType::Added => "+",
            DiffType::Removed => "-",
            DiffType::Increased => "↑",
            DiffType::Decreased => "↓",
            DiffType::Unchanged => "=",
        }
    }
}

// ============================================================================
// DiffEntry — 单帧差异
// ============================================================================

/// 单帧差异
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffEntry {
    name: String,
    baseline_value: u64,
    target_value: u64,
    delta: i64,
    delta_percentage: f64,
    diff_type: DiffType,
}

impl DiffEntry {
    /// 创建差异条目
    pub fn new(name: &str, baseline: u64, target: u64) -> Self {
        let delta = target as i64 - baseline as i64;
        let diff_type = if baseline == 0 && target > 0 {
            DiffType::Added
        } else if baseline > 0 && target == 0 {
            DiffType::Removed
        } else if delta > 0 {
            DiffType::Increased
        } else if delta < 0 {
            DiffType::Decreased
        } else {
            DiffType::Unchanged
        };

        let delta_percentage = if baseline == 0 {
            if target > 0 {
                100.0
            } else {
                0.0
            }
        } else {
            (delta as f64 / baseline as f64) * 100.0
        };

        Self {
            name: name.to_string(),
            baseline_value: baseline,
            target_value: target,
            delta,
            delta_percentage,
            diff_type,
        }
    }

    /// 帧名
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 基线值
    pub fn baseline_value(&self) -> u64 {
        self.baseline_value
    }

    /// 目标值
    pub fn target_value(&self) -> u64 {
        self.target_value
    }

    /// 差值（target - baseline）
    pub fn delta(&self) -> i64 {
        self.delta
    }

    /// 差百分比
    pub fn delta_percentage(&self) -> f64 {
        self.delta_percentage
    }

    /// 差异类型
    pub fn diff_type(&self) -> DiffType {
        self.diff_type
    }

    /// 是否为回归
    pub fn is_regression(&self) -> bool {
        self.diff_type.is_regression()
    }

    /// 是否为改善
    pub fn is_improvement(&self) -> bool {
        self.diff_type.is_improvement()
    }
}

// ============================================================================
// DiffResult — 差异结果
// ============================================================================

/// 差异结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffResult {
    entries: Vec<DiffEntry>,
    total_baseline: u64,
    total_target: u64,
    total_delta: i64,
    regressions: usize,
    improvements: usize,
    unchanged: usize,
}

impl DiffResult {
    /// 创建差异结果
    pub fn new(entries: Vec<DiffEntry>, total_baseline: u64, total_target: u64) -> Self {
        let total_delta = total_target as i64 - total_baseline as i64;
        let regressions = entries.iter().filter(|e| e.is_regression()).count();
        let improvements = entries.iter().filter(|e| e.is_improvement()).count();
        let unchanged = entries
            .iter()
            .filter(|e| e.diff_type().is_unchanged())
            .count();

        Self {
            entries,
            total_baseline,
            total_target,
            total_delta,
            regressions,
            improvements,
            unchanged,
        }
    }

    /// 差异条目
    pub fn entries(&self) -> &[DiffEntry] {
        &self.entries
    }

    /// 差异条目数
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// 基线总值
    pub fn total_baseline(&self) -> u64 {
        self.total_baseline
    }

    /// 目标总值
    pub fn total_target(&self) -> u64 {
        self.total_target
    }

    /// 总差值
    pub fn total_delta(&self) -> i64 {
        self.total_delta
    }

    /// 总差百分比
    pub fn total_delta_percentage(&self) -> f64 {
        if self.total_baseline == 0 {
            if self.total_target > 0 {
                100.0
            } else {
                0.0
            }
        } else {
            (self.total_delta as f64 / self.total_baseline as f64) * 100.0
        }
    }

    /// 回归数
    pub fn regression_count(&self) -> usize {
        self.regressions
    }

    /// 改善数
    pub fn improvement_count(&self) -> usize {
        self.improvements
    }

    /// 不变数
    pub fn unchanged_count(&self) -> usize {
        self.unchanged
    }

    /// 是否有回归
    pub fn has_regressions(&self) -> bool {
        self.regressions > 0
    }

    /// 是否有改善
    pub fn has_improvements(&self) -> bool {
        self.improvements > 0
    }

    /// 过滤回归条目
    pub fn regressions(&self) -> Vec<&DiffEntry> {
        self.entries.iter().filter(|e| e.is_regression()).collect()
    }

    /// 过滤改善条目
    pub fn improvements(&self) -> Vec<&DiffEntry> {
        self.entries.iter().filter(|e| e.is_improvement()).collect()
    }

    /// 按差值绝对值降序排序
    pub fn sorted_by_abs_delta(&self) -> Vec<&DiffEntry> {
        let mut entries: Vec<&DiffEntry> = self.entries.iter().collect();
        entries.sort_by(|a, b| b.delta().abs().cmp(&a.delta().abs()));
        entries
    }

    /// 转换为 JSON
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }

    /// 生成差异报告
    pub fn report(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "Flame Graph Diff Report\n=======================\n"
        ));
        out.push_str(&format!("Baseline total: {} ms\n", self.total_baseline));
        out.push_str(&format!("Target total:   {} ms\n", self.total_target));
        out.push_str(&format!(
            "Delta:          {} ms ({:.2}%)\n\n",
            self.total_delta,
            self.total_delta_percentage()
        ));
        out.push_str(&format!(
            "Regressions: {}  Improvements: {}  Unchanged: {}\n\n",
            self.regressions, self.improvements, self.unchanged
        ));

        if !self.entries.is_empty() {
            out.push_str("Top changes (by |delta|):\n");
            for entry in self.sorted_by_abs_delta().iter().take(20) {
                out.push_str(&format!(
                    "  {} {:30} baseline={:6} target={:6} delta={:+6} ({:+.2}%)\n",
                    entry.diff_type().symbol(),
                    entry.name(),
                    entry.baseline_value(),
                    entry.target_value(),
                    entry.delta(),
                    entry.delta_percentage()
                ));
            }
        }

        out
    }
}

// ============================================================================
// FlameDiff — 火焰图差异比较器
// ============================================================================

/// 差异比较模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiffMode {
    /// 按总值比较
    TotalValue,
    /// 按自身值比较
    SelfValue,
}

impl Default for DiffMode {
    fn default() -> Self {
        Self::TotalValue
    }
}

/// 火焰图差异比较器
#[derive(Debug, Clone)]
pub struct FlameDiff {
    mode: DiffMode,
    include_unchanged: bool,
    min_delta: u64,
}

impl Default for FlameDiff {
    fn default() -> Self {
        Self {
            mode: DiffMode::default(),
            include_unchanged: false,
            min_delta: 0,
        }
    }
}

impl FlameDiff {
    /// 创建比较器
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置比较模式（链式）
    pub fn mode(mut self, mode: DiffMode) -> Self {
        self.mode = mode;
        self
    }

    /// 包含不变条目（链式）
    pub fn include_unchanged(mut self) -> Self {
        self.include_unchanged = true;
        self
    }

    /// 设置最小差值阈值（链式）
    pub fn min_delta(mut self, n: u64) -> Self {
        self.min_delta = n;
        self
    }

    /// 比较模式
    pub fn mode_value(&self) -> DiffMode {
        self.mode
    }

    /// 是否包含不变条目
    pub fn includes_unchanged(&self) -> bool {
        self.include_unchanged
    }

    /// 最小差值
    pub fn min_delta_value(&self) -> u64 {
        self.min_delta
    }

    /// 比较两个火焰图
    pub fn compare(&self, baseline: &FlameGraphData, target: &FlameGraphData) -> DiffResult {
        let baseline_map = self.collect_values(baseline);
        let target_map = self.collect_values(target);

        let mut all_names: std::collections::HashSet<String> =
            baseline_map.keys().cloned().collect();
        all_names.extend(target_map.keys().cloned());

        let root_name = baseline.root().name();
        let mut entries: Vec<DiffEntry> = all_names
            .iter()
            .filter(|name| name.as_str() != root_name)
            .map(|name| {
                let b = *baseline_map.get(name).unwrap_or(&0);
                let t = *target_map.get(name).unwrap_or(&0);
                DiffEntry::new(name, b, t)
            })
            .filter(|e| {
                if !self.include_unchanged && e.diff_type().is_unchanged() {
                    return false;
                }
                if e.delta().abs() < self.min_delta as i64 {
                    return false;
                }
                true
            })
            .collect();

        entries.sort_by(|a, b| b.delta().abs().cmp(&a.delta().abs()));

        DiffResult::new(entries, baseline.total_samples(), target.total_samples())
    }

    /// 收集帧值
    fn collect_values(&self, data: &FlameGraphData) -> HashMap<String, u64> {
        let mut map = HashMap::new();
        match self.mode {
            DiffMode::TotalValue => Self::collect_total(data.root(), &mut map),
            DiffMode::SelfValue => Self::collect_self(data.root(), &mut map),
        }
        map
    }

    fn collect_total(node: &FlameNode, map: &mut HashMap<String, u64>) {
        *map.entry(node.name().to_string()).or_insert(0) += node.value();
        for child in node.children() {
            Self::collect_total(child, map);
        }
    }

    fn collect_self(node: &FlameNode, map: &mut HashMap<String, u64>) {
        let children_sum: u64 = node.children().iter().map(|c| c.value()).sum();
        let self_val = node.value().saturating_sub(children_sum);
        *map.entry(node.name().to_string()).or_insert(0) += self_val;
        for child in node.children() {
            Self::collect_self(child, map);
        }
    }

    /// 快速比较（仅返回是否有回归）
    pub fn has_regression(&self, baseline: &FlameGraphData, target: &FlameGraphData) -> bool {
        self.compare(baseline, target).has_regressions()
    }

    /// 快速比较（仅返回回归数）
    pub fn regression_count(&self, baseline: &FlameGraphData, target: &FlameGraphData) -> usize {
        self.compare(baseline, target).regression_count()
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flame_node::FlameGraphBuilder;

    fn baseline() -> FlameGraphData {
        FlameGraphBuilder::new()
            .add_stack(vec!["a".to_string(), "b".to_string()], 10)
            .add_stack(vec!["a".to_string(), "c".to_string()], 20)
            .add_stack(vec!["d".to_string()], 30)
            .build()
    }

    fn target() -> FlameGraphData {
        FlameGraphBuilder::new()
            .add_stack(vec!["a".to_string(), "b".to_string()], 15)
            .add_stack(vec!["a".to_string(), "c".to_string()], 20)
            .add_stack(vec!["e".to_string()], 40)
            .build()
    }

    // ----- DiffType -----

    #[test]
    fn diff_type_is_regression() {
        assert!(DiffType::Added.is_regression());
        assert!(DiffType::Increased.is_regression());
        assert!(!DiffType::Decreased.is_regression());
        assert!(!DiffType::Removed.is_regression());
        assert!(!DiffType::Unchanged.is_regression());
    }

    #[test]
    fn diff_type_is_improvement() {
        assert!(DiffType::Removed.is_improvement());
        assert!(DiffType::Decreased.is_improvement());
        assert!(!DiffType::Added.is_improvement());
    }

    #[test]
    fn diff_type_is_unchanged() {
        assert!(DiffType::Unchanged.is_unchanged());
        assert!(!DiffType::Added.is_unchanged());
    }

    #[test]
    fn diff_type_as_str() {
        assert_eq!(DiffType::Added.as_str(), "added");
        assert_eq!(DiffType::Removed.as_str(), "removed");
        assert_eq!(DiffType::Increased.as_str(), "increased");
        assert_eq!(DiffType::Decreased.as_str(), "decreased");
        assert_eq!(DiffType::Unchanged.as_str(), "unchanged");
    }

    #[test]
    fn diff_type_symbol() {
        assert_eq!(DiffType::Added.symbol(), "+");
        assert_eq!(DiffType::Removed.symbol(), "-");
        assert_eq!(DiffType::Unchanged.symbol(), "=");
    }

    // ----- DiffEntry -----

    #[test]
    fn diff_entry_added() {
        let e = DiffEntry::new("func", 0, 10);
        assert_eq!(e.diff_type(), DiffType::Added);
        assert!(e.is_regression());
        assert_eq!(e.delta(), 10);
    }

    #[test]
    fn diff_entry_removed() {
        let e = DiffEntry::new("func", 10, 0);
        assert_eq!(e.diff_type(), DiffType::Removed);
        assert!(e.is_improvement());
        assert_eq!(e.delta(), -10);
    }

    #[test]
    fn diff_entry_increased() {
        let e = DiffEntry::new("func", 10, 20);
        assert_eq!(e.diff_type(), DiffType::Increased);
        assert!(e.is_regression());
        assert_eq!(e.delta(), 10);
        assert!((e.delta_percentage() - 100.0).abs() < 0.001);
    }

    #[test]
    fn diff_entry_decreased() {
        let e = DiffEntry::new("func", 20, 10);
        assert_eq!(e.diff_type(), DiffType::Decreased);
        assert!(e.is_improvement());
        assert_eq!(e.delta(), -10);
        assert!((e.delta_percentage() - (-50.0)).abs() < 0.001);
    }

    #[test]
    fn diff_entry_unchanged() {
        let e = DiffEntry::new("func", 10, 10);
        assert_eq!(e.diff_type(), DiffType::Unchanged);
        assert_eq!(e.delta(), 0);
    }

    #[test]
    fn diff_entry_fields() {
        let e = DiffEntry::new("func", 10, 20);
        assert_eq!(e.name(), "func");
        assert_eq!(e.baseline_value(), 10);
        assert_eq!(e.target_value(), 20);
    }

    // ----- DiffResult -----

    #[test]
    fn diff_result_new() {
        let entries = vec![DiffEntry::new("a", 10, 20), DiffEntry::new("b", 30, 15)];
        let result = DiffResult::new(entries, 40, 35);
        assert_eq!(result.entry_count(), 2);
        assert_eq!(result.total_baseline(), 40);
        assert_eq!(result.total_target(), 35);
        assert_eq!(result.total_delta(), -5);
    }

    #[test]
    fn diff_result_counts() {
        let entries = vec![
            DiffEntry::new("a", 10, 20), // increased
            DiffEntry::new("b", 30, 15), // decreased
            DiffEntry::new("c", 5, 0),   // removed
            DiffEntry::new("d", 0, 5),   // added
            DiffEntry::new("e", 10, 10), // unchanged
        ];
        let result = DiffResult::new(entries, 55, 50);
        assert_eq!(result.regression_count(), 2);
        assert_eq!(result.improvement_count(), 2);
        assert_eq!(result.unchanged_count(), 1);
    }

    #[test]
    fn diff_result_has_regressions() {
        let entries = vec![DiffEntry::new("a", 10, 20)];
        let result = DiffResult::new(entries, 10, 20);
        assert!(result.has_regressions());
        assert!(!result.has_improvements());
    }

    #[test]
    fn diff_result_has_improvements() {
        let entries = vec![DiffEntry::new("a", 20, 10)];
        let result = DiffResult::new(entries, 20, 10);
        assert!(result.has_improvements());
        assert!(!result.has_regressions());
    }

    #[test]
    fn diff_result_total_delta_percentage() {
        let entries = vec![];
        let result = DiffResult::new(entries, 100, 150);
        assert!((result.total_delta_percentage() - 50.0).abs() < 0.001);
    }

    #[test]
    fn diff_result_total_delta_percentage_zero_baseline() {
        let entries = vec![];
        let result = DiffResult::new(entries, 0, 100);
        assert!((result.total_delta_percentage() - 100.0).abs() < 0.001);
    }

    #[test]
    fn diff_result_regressions() {
        let entries = vec![DiffEntry::new("a", 10, 20), DiffEntry::new("b", 30, 15)];
        let result = DiffResult::new(entries, 40, 35);
        let regs = result.regressions();
        assert_eq!(regs.len(), 1);
        assert_eq!(regs[0].name(), "a");
    }

    #[test]
    fn diff_result_improvements() {
        let entries = vec![DiffEntry::new("a", 10, 20), DiffEntry::new("b", 30, 15)];
        let result = DiffResult::new(entries, 40, 35);
        let imps = result.improvements();
        assert_eq!(imps.len(), 1);
        assert_eq!(imps[0].name(), "b");
    }

    #[test]
    fn diff_result_sorted_by_abs_delta() {
        let entries = vec![
            DiffEntry::new("a", 10, 12), // delta=2
            DiffEntry::new("b", 30, 15), // delta=-15
            DiffEntry::new("c", 5, 20),  // delta=15
        ];
        let result = DiffResult::new(entries, 45, 47);
        let sorted = result.sorted_by_abs_delta();
        assert_eq!(sorted[0].delta().abs(), 15);
        assert_eq!(sorted[1].delta().abs(), 15);
        assert_eq!(sorted[2].delta().abs(), 2);
    }

    #[test]
    fn diff_result_to_json() {
        let entries = vec![DiffEntry::new("a", 10, 20)];
        let result = DiffResult::new(entries, 10, 20);
        let json = result.to_json();
        assert!(json.contains("entries"));
    }

    #[test]
    fn diff_result_report() {
        let entries = vec![DiffEntry::new("a", 10, 20)];
        let result = DiffResult::new(entries, 10, 20);
        let report = result.report();
        assert!(report.contains("Flame Graph Diff Report"));
        assert!(report.contains("Delta:"));
    }

    // ----- FlameDiff -----

    #[test]
    fn flame_diff_default() {
        let d = FlameDiff::new();
        assert_eq!(d.mode_value(), DiffMode::TotalValue);
        assert!(!d.includes_unchanged());
        assert_eq!(d.min_delta_value(), 0);
    }

    #[test]
    fn flame_diff_compare() {
        let b = baseline();
        let t = target();
        let result = FlameDiff::new().compare(&b, &t);
        assert!(result.entry_count() > 0);
    }

    #[test]
    fn flame_diff_compare_identical() {
        let b = baseline();
        let result = FlameDiff::new().compare(&b, &b);
        assert!(!result.has_regressions());
        assert!(!result.has_improvements());
    }

    #[test]
    fn flame_diff_include_unchanged() {
        let b = baseline();
        let result = FlameDiff::new().include_unchanged().compare(&b, &b);
        assert!(result.entry_count() > 0);
    }

    #[test]
    fn flame_diff_min_delta() {
        let b = baseline();
        let t = target();
        let result = FlameDiff::new().min_delta(1000).compare(&b, &t);
        assert_eq!(result.entry_count(), 0);
    }

    #[test]
    fn flame_diff_self_value_mode() {
        let b = baseline();
        let t = target();
        let result = FlameDiff::new().mode(DiffMode::SelfValue).compare(&b, &t);
        assert!(result.entry_count() > 0);
    }

    #[test]
    fn flame_diff_has_regression() {
        let b = baseline();
        let t = target();
        assert!(FlameDiff::new().has_regression(&b, &t));
    }

    #[test]
    fn flame_diff_no_regression() {
        let b = baseline();
        assert!(!FlameDiff::new().has_regression(&b, &b));
    }

    #[test]
    fn flame_diff_regression_count() {
        let b = baseline();
        let t = target();
        let count = FlameDiff::new().regression_count(&b, &t);
        assert!(count > 0);
    }

    #[test]
    fn flame_diff_regression_count_zero() {
        let b = baseline();
        let count = FlameDiff::new().regression_count(&b, &b);
        assert_eq!(count, 0);
    }
}
