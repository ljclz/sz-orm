//! 火焰图统计分析：热点检测、统计聚合。
//!
//! - [`FlameStats`] — 火焰图统计信息
//! - [`HotspotDetector`] — 热点检测器
//! - [`Hotspot`] — 热点信息
//! - [`FrameStats`] — 单帧统计

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::flame_node::{FlameGraphData, FlameNode};

// ============================================================================
// FrameStats — 单帧统计
// ============================================================================

/// 单帧统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameStats {
    name: String,
    total_value: u64,
    self_value: u64,
    sample_count: usize,
    avg_value: f64,
    max_value: u64,
    min_value: u64,
}

impl FrameStats {
    /// 创建帧统计
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            total_value: 0,
            self_value: 0,
            sample_count: 0,
            avg_value: 0.0,
            max_value: 0,
            min_value: u64::MAX,
        }
    }

    /// 帧名
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 总值（含子帧）
    pub fn total_value(&self) -> u64 {
        self.total_value
    }

    /// 自身值（不含子帧）
    pub fn self_value(&self) -> u64 {
        self.self_value
    }

    /// 采样数
    pub fn sample_count(&self) -> usize {
        self.sample_count
    }

    /// 平均值
    pub fn avg_value(&self) -> f64 {
        self.avg_value
    }

    /// 最大值
    pub fn max_value(&self) -> u64 {
        self.max_value
    }

    /// 最小值
    pub fn min_value(&self) -> u64 {
        self.min_value
    }

    /// 添加采样值
    pub fn add_sample(&mut self, value: u64) {
        self.total_value += value;
        self.sample_count += 1;
        self.max_value = self.max_value.max(value);
        self.min_value = self.min_value.min(value);
        self.avg_value = self.total_value as f64 / self.sample_count as f64;
    }

    /// 设置自身值
    pub fn set_self_value(&mut self, value: u64) {
        self.self_value = value;
    }
}

// ============================================================================
// FlameStats — 火焰图统计
// ============================================================================

/// 火焰图统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlameStats {
    total_samples: u64,
    frame_count: usize,
    leaf_count: usize,
    max_depth: usize,
    avg_depth: f64,
    frame_stats: HashMap<String, FrameStats>,
    top_frames: Vec<String>,
}

impl FlameStats {
    /// 从火焰图数据计算统计
    pub fn from_data(data: &FlameGraphData) -> Self {
        let mut frame_stats: HashMap<String, FrameStats> = HashMap::new();
        let mut depth_sum = 0u64;
        let mut leaf_count = 0usize;

        Self::collect_stats(
            data.root(),
            1,
            &mut frame_stats,
            &mut depth_sum,
            &mut leaf_count,
        );

        let total_samples = data.total_samples();
        let frame_count = data.node_count();
        let max_depth = data.max_depth();
        let avg_depth = if leaf_count == 0 {
            0.0
        } else {
            depth_sum as f64 / leaf_count as f64
        };

        let mut top_frames: Vec<(u64, String)> = frame_stats
            .iter()
            .filter(|(name, _)| name.as_str() != data.root().name())
            .map(|(_, stats)| (stats.total_value, stats.name.clone()))
            .collect();
        top_frames.sort_by_key(|b| std::cmp::Reverse(b.0));
        let top_frames: Vec<String> = top_frames
            .into_iter()
            .take(10)
            .map(|(_, name)| name)
            .collect();

        Self {
            total_samples,
            frame_count,
            leaf_count,
            max_depth,
            avg_depth,
            frame_stats,
            top_frames,
        }
    }

    fn collect_stats(
        node: &FlameNode,
        depth: usize,
        stats: &mut HashMap<String, FrameStats>,
        depth_sum: &mut u64,
        leaf_count: &mut usize,
    ) {
        let entry = stats
            .entry(node.name().to_string())
            .or_insert_with(|| FrameStats::new(node.name()));
        entry.add_sample(node.value());

        let self_value: u64 = if node.is_leaf() {
            node.value()
        } else {
            node.children().iter().map(|c| c.value()).sum()
        };
        entry.set_self_value(entry.self_value() + self_value);

        if node.is_leaf() && node.value() > 0 {
            *depth_sum += depth as u64;
            *leaf_count += 1;
        } else if !node.is_leaf() {
            for child in node.children() {
                Self::collect_stats(child, depth + 1, stats, depth_sum, leaf_count);
            }
        }
    }

    /// 总采样数
    pub fn total_samples(&self) -> u64 {
        self.total_samples
    }

    /// 帧数
    pub fn frame_count(&self) -> usize {
        self.frame_count
    }

    /// 叶节点数
    pub fn leaf_count(&self) -> usize {
        self.leaf_count
    }

    /// 最大深度
    pub fn max_depth(&self) -> usize {
        self.max_depth
    }

    /// 平均深度
    pub fn avg_depth(&self) -> f64 {
        self.avg_depth
    }

    /// 帧统计
    pub fn frame_stats(&self, name: &str) -> Option<&FrameStats> {
        self.frame_stats.get(name)
    }

    /// 所有帧名
    pub fn frame_names(&self) -> Vec<String> {
        self.frame_stats.keys().cloned().collect()
    }

    /// Top N 帧名（按总值降序）
    pub fn top_frames(&self) -> &[String] {
        &self.top_frames
    }

    /// 帧统计数
    pub fn stats_count(&self) -> usize {
        self.frame_stats.len()
    }

    /// 转换为 JSON
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }
}

// ============================================================================
// Hotspot — 热点信息
// ============================================================================

/// 热点信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hotspot {
    name: String,
    self_value: u64,
    total_value: u64,
    self_percentage: f64,
    total_percentage: f64,
    rank: usize,
}

impl Hotspot {
    /// 帧名
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 自身值
    pub fn self_value(&self) -> u64 {
        self.self_value
    }

    /// 总值
    pub fn total_value(&self) -> u64 {
        self.total_value
    }

    /// 自身百分比
    pub fn self_percentage(&self) -> f64 {
        self.self_percentage
    }

    /// 总百分比
    pub fn total_percentage(&self) -> f64 {
        self.total_percentage
    }

    /// 排名
    pub fn rank(&self) -> usize {
        self.rank
    }
}

// ============================================================================
// HotspotDetector — 热点检测器
// ============================================================================

/// 热点检测策略
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[derive(Default)]
pub enum HotspotStrategy {
    /// 按自身耗时
    #[default]
    SelfTime,
    /// 按总耗时
    TotalTime,
    /// 按总耗时百分比阈值
    PercentageThreshold(u64),
}


/// 热点检测器
#[derive(Debug, Clone)]
pub struct HotspotDetector {
    strategy: HotspotStrategy,
    max_hotspots: usize,
    min_value: u64,
}

impl Default for HotspotDetector {
    fn default() -> Self {
        Self {
            strategy: HotspotStrategy::default(),
            max_hotspots: 10,
            min_value: 0,
        }
    }
}

impl HotspotDetector {
    /// 创建检测器
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置策略（链式）
    pub fn strategy(mut self, strategy: HotspotStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    /// 设置最大热点数（链式）
    pub fn max_hotspots(mut self, n: usize) -> Self {
        self.max_hotspots = n;
        self
    }

    /// 设置最小值阈值（链式）
    pub fn min_value(mut self, n: u64) -> Self {
        self.min_value = n;
        self
    }

    /// 策略
    pub fn strategy_value(&self) -> &HotspotStrategy {
        &self.strategy
    }

    /// 最大热点数
    pub fn max_hotspots_value(&self) -> usize {
        self.max_hotspots
    }

    /// 最小值
    pub fn min_value_value(&self) -> u64 {
        self.min_value
    }

    /// 检测热点
    pub fn detect(&self, data: &FlameGraphData) -> Vec<Hotspot> {
        let stats = FlameStats::from_data(data);
        let total = data.total_samples().max(1) as f64;
        let root_name = data.root().name();

        let mut candidates: Vec<(String, u64, u64)> = stats
            .frame_stats
            .iter()
            .filter(|(name, s)| name.as_str() != root_name && s.self_value() >= self.min_value)
            .map(|(name, s)| (name.clone(), s.self_value(), s.total_value()))
            .collect();

        match self.strategy {
            HotspotStrategy::SelfTime => {
                candidates.sort_by_key(|b| std::cmp::Reverse(b.1));
            }
            HotspotStrategy::TotalTime => {
                candidates.sort_by_key(|b| std::cmp::Reverse(b.2));
            }
            HotspotStrategy::PercentageThreshold(pct) => {
                let threshold = (pct as f64 / 100.0) * total;
                candidates.retain(|(_, _, total_val)| *total_val as f64 >= threshold);
                candidates.sort_by_key(|b| std::cmp::Reverse(b.2));
            }
        }

        candidates
            .into_iter()
            .take(self.max_hotspots)
            .enumerate()
            .map(|(i, (name, self_val, total_val))| Hotspot {
                name,
                self_value: self_val,
                total_value: total_val,
                self_percentage: (self_val as f64 / total) * 100.0,
                total_percentage: (total_val as f64 / total) * 100.0,
                rank: i + 1,
            })
            .collect()
    }

    /// 检测最热帧（单个）
    pub fn detect_top(&self, data: &FlameGraphData) -> Option<Hotspot> {
        self.detect(data).into_iter().next()
    }
}

// ============================================================================
// DepthDistribution — 深度分布
// ============================================================================

/// 深度分布统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepthDistribution {
    distribution: Vec<(usize, u64)>,
}

impl DepthDistribution {
    /// 从火焰图数据计算深度分布
    pub fn from_data(data: &FlameGraphData) -> Self {
        let mut dist: HashMap<usize, u64> = HashMap::new();
        Self::collect(data.root(), 1, &mut dist);

        let max_depth = data.max_depth();
        let distribution: Vec<(usize, u64)> = (1..=max_depth)
            .map(|d| (d, *dist.get(&d).unwrap_or(&0)))
            .collect();

        Self { distribution }
    }

    fn collect(node: &FlameNode, depth: usize, dist: &mut HashMap<usize, u64>) {
        if node.is_leaf() {
            *dist.entry(depth).or_insert(0) += node.value();
        } else {
            for child in node.children() {
                Self::collect(child, depth + 1, dist);
            }
        }
    }

    /// 分布数据
    pub fn distribution(&self) -> &[(usize, u64)] {
        &self.distribution
    }

    /// 最大深度
    pub fn max_depth(&self) -> usize {
        self.distribution.len()
    }

    /// 指定深度的采样值
    pub fn value_at_depth(&self, depth: usize) -> u64 {
        self.distribution
            .iter()
            .find(|(d, _)| *d == depth)
            .map(|(_, v)| *v)
            .unwrap_or(0)
    }

    /// 采样值最大的深度
    pub fn peak_depth(&self) -> Option<usize> {
        self.distribution
            .iter()
            .max_by_key(|(_, v)| *v)
            .filter(|(_, v)| *v > 0)
            .map(|(d, _)| *d)
    }

    /// 总采样值
    pub fn total(&self) -> u64 {
        self.distribution.iter().map(|(_, v)| v).sum()
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flame_node::{FlameGraphBuilder, FlameNode};

    fn sample_data() -> FlameGraphData {
        FlameGraphBuilder::new()
            .root_name("root")
            .add_stack(vec!["a".to_string(), "b".to_string()], 30)
            .add_stack(vec!["a".to_string(), "c".to_string()], 20)
            .add_stack(vec!["d".to_string()], 50)
            .build()
    }

    // ----- FrameStats -----

    #[test]
    fn frame_stats_new() {
        let s = FrameStats::new("func");
        assert_eq!(s.name(), "func");
        assert_eq!(s.total_value(), 0);
        assert_eq!(s.sample_count(), 0);
    }

    #[test]
    fn frame_stats_add_sample() {
        let mut s = FrameStats::new("func");
        s.add_sample(10);
        s.add_sample(20);
        s.add_sample(30);
        assert_eq!(s.total_value(), 60);
        assert_eq!(s.sample_count(), 3);
        assert_eq!(s.max_value(), 30);
        assert_eq!(s.min_value(), 10);
        assert!((s.avg_value() - 20.0).abs() < 0.001);
    }

    #[test]
    fn frame_stats_set_self_value() {
        let mut s = FrameStats::new("func");
        s.set_self_value(42);
        assert_eq!(s.self_value(), 42);
    }

    // ----- FlameStats -----

    #[test]
    fn flame_stats_from_data() {
        let data = sample_data();
        let stats = FlameStats::from_data(&data);
        assert_eq!(stats.total_samples(), 100);
        assert!(stats.frame_count() > 0);
        assert!(stats.leaf_count() > 0);
    }

    #[test]
    fn flame_stats_empty() {
        let data = FlameGraphData::from_root(FlameNode::root("root"));
        let stats = FlameStats::from_data(&data);
        assert_eq!(stats.total_samples(), 0);
        assert_eq!(stats.leaf_count(), 0);
    }

    #[test]
    fn flame_stats_frame_stats() {
        let data = sample_data();
        let stats = FlameStats::from_data(&data);
        assert!(stats.frame_stats("a").is_some());
        assert!(stats.frame_stats("b").is_some());
        assert!(stats.frame_stats("nonexistent").is_none());
    }

    #[test]
    fn flame_stats_top_frames() {
        let data = sample_data();
        let stats = FlameStats::from_data(&data);
        let top = stats.top_frames();
        assert!(!top.is_empty());
    }

    #[test]
    fn flame_stats_frame_names() {
        let data = sample_data();
        let stats = FlameStats::from_data(&data);
        let names = stats.frame_names();
        assert!(names.contains(&"a".to_string()));
        assert!(names.contains(&"b".to_string()));
    }

    #[test]
    fn flame_stats_max_depth() {
        let data = sample_data();
        let stats = FlameStats::from_data(&data);
        assert!(stats.max_depth() >= 2);
    }

    #[test]
    fn flame_stats_avg_depth() {
        let data = sample_data();
        let stats = FlameStats::from_data(&data);
        assert!(stats.avg_depth() > 0.0);
    }

    #[test]
    fn flame_stats_to_json() {
        let data = sample_data();
        let stats = FlameStats::from_data(&data);
        let json = stats.to_json();
        assert!(json.contains("total_samples"));
    }

    // ----- Hotspot -----

    #[test]
    fn hotspot_fields() {
        let data = sample_data();
        let hotspots = HotspotDetector::new().detect(&data);
        assert!(!hotspots.is_empty());
        let h = &hotspots[0];
        assert_eq!(h.rank(), 1);
        assert!(h.self_percentage() >= 0.0);
        assert!(h.total_percentage() >= 0.0);
    }

    // ----- HotspotDetector -----

    #[test]
    fn detector_default() {
        let d = HotspotDetector::new();
        assert_eq!(d.strategy_value(), &HotspotStrategy::SelfTime);
        assert_eq!(d.max_hotspots_value(), 10);
        assert_eq!(d.min_value_value(), 0);
    }

    #[test]
    fn detector_self_time() {
        let data = sample_data();
        let hotspots = HotspotDetector::new()
            .strategy(HotspotStrategy::SelfTime)
            .detect(&data);
        assert!(!hotspots.is_empty());
        for i in 1..hotspots.len() {
            assert!(hotspots[i - 1].self_value() >= hotspots[i].self_value());
        }
    }

    #[test]
    fn detector_total_time() {
        let data = sample_data();
        let hotspots = HotspotDetector::new()
            .strategy(HotspotStrategy::TotalTime)
            .detect(&data);
        assert!(!hotspots.is_empty());
        for i in 1..hotspots.len() {
            assert!(hotspots[i - 1].total_value() >= hotspots[i].total_value());
        }
    }

    #[test]
    fn detector_percentage_threshold() {
        let data = sample_data();
        let hotspots = HotspotDetector::new()
            .strategy(HotspotStrategy::PercentageThreshold(20))
            .detect(&data);
        for h in &hotspots {
            assert!(h.total_percentage() >= 20.0 - 0.001);
        }
    }

    #[test]
    fn detector_max_hotspots() {
        let data = sample_data();
        let hotspots = HotspotDetector::new().max_hotspots(2).detect(&data);
        assert!(hotspots.len() <= 2);
    }

    #[test]
    fn detector_min_value() {
        let data = sample_data();
        let hotspots = HotspotDetector::new().min_value(1000).detect(&data);
        assert!(hotspots.is_empty());
    }

    #[test]
    fn detector_detect_top() {
        let data = sample_data();
        let top = HotspotDetector::new().detect_top(&data);
        assert!(top.is_some());
        assert_eq!(top.unwrap().rank(), 1);
    }

    #[test]
    fn detector_detect_top_empty() {
        let data = FlameGraphData::from_root(FlameNode::root("root"));
        let top = HotspotDetector::new().detect_top(&data);
        assert!(top.is_none());
    }

    #[test]
    fn detector_ranks_sequential() {
        let data = sample_data();
        let hotspots = HotspotDetector::new().detect(&data);
        for (i, h) in hotspots.iter().enumerate() {
            assert_eq!(h.rank(), i + 1);
        }
    }

    // ----- DepthDistribution -----

    #[test]
    fn depth_distribution_from_data() {
        let data = sample_data();
        let dist = DepthDistribution::from_data(&data);
        assert!(dist.max_depth() > 0);
    }

    #[test]
    fn depth_distribution_empty() {
        let data = FlameGraphData::from_root(FlameNode::root("root"));
        let dist = DepthDistribution::from_data(&data);
        assert_eq!(dist.total(), 0);
    }

    #[test]
    fn depth_distribution_value_at_depth() {
        let data = sample_data();
        let dist = DepthDistribution::from_data(&data);
        let _val = dist.value_at_depth(1);
        assert!(dist.value_at_depth(999) == 0);
    }

    #[test]
    fn depth_distribution_peak_depth() {
        let data = sample_data();
        let dist = DepthDistribution::from_data(&data);
        let peak = dist.peak_depth();
        assert!(peak.is_some());
    }

    #[test]
    fn depth_distribution_total() {
        let data = sample_data();
        let dist = DepthDistribution::from_data(&data);
        assert_eq!(dist.total(), data.total_samples());
    }

    #[test]
    fn depth_distribution_distribution() {
        let data = sample_data();
        let dist = DepthDistribution::from_data(&data);
        let d = dist.distribution();
        assert!(!d.is_empty());
    }
}
