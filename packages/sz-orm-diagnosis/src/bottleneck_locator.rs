//! 性能瓶颈定位器
//!
//! 采样各阶段耗时，按占比与阈值定位性能瓶颈，
//! 提供瓶颈严重度分级与建议。
//! 本模块不依赖 `slow-query-diagnosis` feature，可独立使用。

use std::collections::HashMap;

/// 瓶颈严重度
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BottleneckSeverity {
    /// 正常
    Normal,
    /// 轻微（占比 10%~30%）
    Minor,
    /// 中等（占比 30%~50%）
    Moderate,
    /// 严重（占比 50%~70%）
    Severe,
    /// 致命（占比 ≥ 70%）
    Critical,
}

impl BottleneckSeverity {
    /// 人类可读名称
    pub fn as_str(&self) -> &'static str {
        match self {
            BottleneckSeverity::Normal => "normal",
            BottleneckSeverity::Minor => "minor",
            BottleneckSeverity::Moderate => "moderate",
            BottleneckSeverity::Severe => "severe",
            BottleneckSeverity::Critical => "critical",
        }
    }

    /// 从占比推断严重度
    pub fn from_percentage(percentage: f64) -> Self {
        if percentage >= 70.0 {
            BottleneckSeverity::Critical
        } else if percentage >= 50.0 {
            BottleneckSeverity::Severe
        } else if percentage >= 30.0 {
            BottleneckSeverity::Moderate
        } else if percentage >= 10.0 {
            BottleneckSeverity::Minor
        } else {
            BottleneckSeverity::Normal
        }
    }
}

/// 瓶颈采样条目
#[derive(Debug, Clone)]
pub struct BottleneckSample {
    /// 阶段名
    pub stage: String,
    /// 耗时（毫秒）
    pub duration_ms: u64,
    /// 采样时间戳（毫秒）
    pub timestamp_ms: u64,
}

impl BottleneckSample {
    /// 创建采样
    pub fn new(stage: impl Into<String>, duration_ms: u64, timestamp_ms: u64) -> Self {
        Self {
            stage: stage.into(),
            duration_ms,
            timestamp_ms,
        }
    }
}

/// 定位到的瓶颈
#[derive(Debug, Clone)]
pub struct Bottleneck {
    /// 阶段名
    pub stage: String,
    /// 累计耗时（毫秒）
    pub total_duration_ms: u64,
    /// 占总耗时百分比
    pub percentage: f64,
    /// 严重度
    pub severity: BottleneckSeverity,
    /// 采样次数
    pub sample_count: usize,
    /// 平均耗时（毫秒）
    pub avg_duration_ms: f64,
    /// 最大单次耗时（毫秒）
    pub max_duration_ms: u64,
}

/// 瓶颈定位器配置
#[derive(Debug, Clone)]
pub struct BottleneckLocatorConfig {
    /// 严重瓶颈占比阈值（默认 50.0%）
    pub severe_threshold_pct: f64,
    /// 致命瓶颈占比阈值（默认 70.0%）
    pub critical_threshold_pct: f64,
    /// Top-N 返回数（默认 5）
    pub top_n: usize,
}

impl Default for BottleneckLocatorConfig {
    fn default() -> Self {
        Self {
            severe_threshold_pct: 50.0,
            critical_threshold_pct: 70.0,
            top_n: 5,
        }
    }
}

/// 性能瓶颈定位器
///
/// 采样各阶段耗时，按占比定位瓶颈并分级。
pub struct BottleneckLocator {
    config: BottleneckLocatorConfig,
    samples: Vec<BottleneckSample>,
}

impl BottleneckLocator {
    /// 创建定位器
    pub fn new(config: BottleneckLocatorConfig) -> Self {
        Self {
            config,
            samples: Vec::new(),
        }
    }

    /// 使用默认配置创建
    pub fn with_defaults() -> Self {
        Self::new(BottleneckLocatorConfig::default())
    }

    /// 添加采样
    pub fn add_sample(&mut self, sample: BottleneckSample) {
        self.samples.push(sample);
    }

    /// 便捷方法：添加采样
    pub fn sample(&mut self, stage: impl Into<String>, duration_ms: u64, timestamp_ms: u64) {
        self.add_sample(BottleneckSample::new(stage, duration_ms, timestamp_ms));
    }

    /// 批量添加采样
    pub fn add_samples(&mut self, samples: Vec<BottleneckSample>) {
        self.samples.extend(samples);
    }

    /// 定位瓶颈
    ///
    /// 返回按占比降序排列的瓶颈列表（Top-N）。
    pub fn locate(&self) -> Vec<Bottleneck> {
        let stage_stats = self.aggregate_by_stage();
        let total_ms: u64 = stage_stats.values().map(|s| s.total).sum();

        if total_ms == 0 {
            return Vec::new();
        }

        let mut bottlenecks: Vec<Bottleneck> = stage_stats
            .into_iter()
            .map(|(stage, stats)| {
                let percentage = stats.total as f64 / total_ms as f64 * 100.0;
                let severity = BottleneckSeverity::from_percentage(percentage);
                let avg = if stats.count == 0 {
                    0.0
                } else {
                    stats.total as f64 / stats.count as f64
                };
                Bottleneck {
                    stage,
                    total_duration_ms: stats.total,
                    percentage,
                    severity,
                    sample_count: stats.count,
                    avg_duration_ms: avg,
                    max_duration_ms: stats.max,
                }
            })
            .collect();

        bottlenecks.sort_by(|a, b| {
            b.total_duration_ms
                .cmp(&a.total_duration_ms)
                .then_with(|| a.stage.cmp(&b.stage))
        });
        bottlenecks.truncate(self.config.top_n);
        bottlenecks
    }

    /// 按阶段聚合统计
    fn aggregate_by_stage(&self) -> HashMap<String, StageAgg> {
        let mut map: HashMap<String, StageAgg> = HashMap::new();
        for sample in &self.samples {
            let agg = map.entry(sample.stage.clone()).or_default();
            agg.total += sample.duration_ms;
            agg.count += 1;
            if sample.duration_ms > agg.max {
                agg.max = sample.duration_ms;
            }
        }
        map
    }

    /// 获取最严重瓶颈
    pub fn worst_bottleneck(&self) -> Option<Bottleneck> {
        self.locate().into_iter().next()
    }

    /// 是否存在严重瓶颈（占比超 severe_threshold）
    pub fn has_severe_bottleneck(&self) -> bool {
        self.locate()
            .iter()
            .any(|b| b.percentage >= self.config.severe_threshold_pct)
    }

    /// 是否存在致命瓶颈（占比超 critical_threshold）
    pub fn has_critical_bottleneck(&self) -> bool {
        self.locate()
            .iter()
            .any(|b| b.percentage >= self.config.critical_threshold_pct)
    }

    /// 采样总数
    pub fn sample_count(&self) -> usize {
        self.samples.len()
    }

    /// 清空采样
    pub fn clear(&mut self) {
        self.samples.clear();
    }

    /// 生成瓶颈报告字符串
    pub fn report(&self) -> String {
        let bottlenecks = self.locate();
        let mut out = String::new();
        out.push_str("=== 性能瓶颈定位报告 ===\n");
        if bottlenecks.is_empty() {
            out.push_str("(无采样数据)\n");
            return out;
        }
        out.push_str(&format!(
            "{:<25} {:>12} {:>10} {:>10} {:>10} {:>10}\n",
            "阶段", "总耗时(ms)", "占比(%)", "严重度", "采样数", "最大(ms)"
        ));
        for b in &bottlenecks {
            out.push_str(&format!(
                "{:<25} {:>12} {:>10.1} {:>10} {:>10} {:>10}\n",
                b.stage,
                b.total_duration_ms,
                b.percentage,
                b.severity.as_str(),
                b.sample_count,
                b.max_duration_ms
            ));
        }
        out
    }
}

/// 阶段聚合统计（内部使用）
#[derive(Debug, Clone, Default)]
struct StageAgg {
    total: u64,
    count: usize,
    max: u64,
}

/// 瓶颈趋势分析器
///
/// 按时间窗口分析瓶颈变化趋势。
#[derive(Debug, Clone)]
pub struct BottleneckTrendAnalyzer {
    /// 窗口大小（毫秒）
    window_ms: u64,
    /// 历史定位结果：`(窗口起始时间, 瓶颈列表)`
    history: Vec<(u64, Vec<Bottleneck>)>,
}

impl BottleneckTrendAnalyzer {
    /// 创建趋势分析器
    pub fn new(window_ms: u64) -> Self {
        Self {
            window_ms,
            history: Vec::new(),
        }
    }

    /// 记录一个窗口的瓶颈
    pub fn record(&mut self, window_start: u64, bottlenecks: Vec<Bottleneck>) {
        self.history.push((window_start, bottlenecks));
    }

    /// 历史窗口数
    pub fn window_count(&self) -> usize {
        self.history.len()
    }

    /// 某阶段是否持续为瓶颈（在所有窗口中都出现且占比超阈值）
    pub fn is_persistent_bottleneck(&self, stage: &str, threshold_pct: f64) -> bool {
        if self.history.is_empty() {
            return false;
        }
        self.history.iter().all(|(_, bottlenecks)| {
            bottlenecks
                .iter()
                .any(|b| b.stage == stage && b.percentage >= threshold_pct)
        })
    }

    /// 某阶段占比变化趋势（最后两个窗口的差值，正=恶化）
    pub fn trend(&self, stage: &str) -> f64 {
        let n = self.history.len();
        if n < 2 {
            return 0.0;
        }
        let recent = self.find_stage_percentage(n - 1, stage);
        let prev = self.find_stage_percentage(n - 2, stage);
        recent - prev
    }

    fn find_stage_percentage(&self, window_idx: usize, stage: &str) -> f64 {
        if window_idx >= self.history.len() {
            return 0.0;
        }
        let (_, bottlenecks) = &self.history[window_idx];
        bottlenecks
            .iter()
            .find(|b| b.stage == stage)
            .map(|b| b.percentage)
            .unwrap_or(0.0)
    }

    /// 持续瓶颈阶段列表
    pub fn persistent_bottlenecks(&self, threshold_pct: f64) -> Vec<String> {
        if self.history.is_empty() {
            return Vec::new();
        }
        let first_window = &self.history[0].1;
        first_window
            .iter()
            .filter(|b| b.percentage >= threshold_pct)
            .filter(|b| self.is_persistent_bottleneck(&b.stage, threshold_pct))
            .map(|b| b.stage.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- BottleneckSeverity tests ---

    #[test]
    fn severity_as_str() {
        assert_eq!(BottleneckSeverity::Normal.as_str(), "normal");
        assert_eq!(BottleneckSeverity::Minor.as_str(), "minor");
        assert_eq!(BottleneckSeverity::Moderate.as_str(), "moderate");
        assert_eq!(BottleneckSeverity::Severe.as_str(), "severe");
        assert_eq!(BottleneckSeverity::Critical.as_str(), "critical");
    }

    #[test]
    fn severity_from_percentage_normal() {
        assert_eq!(
            BottleneckSeverity::from_percentage(5.0),
            BottleneckSeverity::Normal
        );
    }

    #[test]
    fn severity_from_percentage_minor() {
        assert_eq!(
            BottleneckSeverity::from_percentage(15.0),
            BottleneckSeverity::Minor
        );
    }

    #[test]
    fn severity_from_percentage_moderate() {
        assert_eq!(
            BottleneckSeverity::from_percentage(35.0),
            BottleneckSeverity::Moderate
        );
    }

    #[test]
    fn severity_from_percentage_severe() {
        assert_eq!(
            BottleneckSeverity::from_percentage(55.0),
            BottleneckSeverity::Severe
        );
    }

    #[test]
    fn severity_from_percentage_critical() {
        assert_eq!(
            BottleneckSeverity::from_percentage(75.0),
            BottleneckSeverity::Critical
        );
    }

    #[test]
    fn severity_ordering() {
        assert!(BottleneckSeverity::Normal < BottleneckSeverity::Minor);
        assert!(BottleneckSeverity::Minor < BottleneckSeverity::Moderate);
        assert!(BottleneckSeverity::Moderate < BottleneckSeverity::Severe);
        assert!(BottleneckSeverity::Severe < BottleneckSeverity::Critical);
    }

    #[test]
    fn severity_boundary_values() {
        assert_eq!(
            BottleneckSeverity::from_percentage(10.0),
            BottleneckSeverity::Minor
        );
        assert_eq!(
            BottleneckSeverity::from_percentage(30.0),
            BottleneckSeverity::Moderate
        );
        assert_eq!(
            BottleneckSeverity::from_percentage(50.0),
            BottleneckSeverity::Severe
        );
        assert_eq!(
            BottleneckSeverity::from_percentage(70.0),
            BottleneckSeverity::Critical
        );
    }

    // --- BottleneckSample tests ---

    #[test]
    fn sample_new() {
        let s = BottleneckSample::new("db.execute", 100, 1000);
        assert_eq!(s.stage, "db.execute");
        assert_eq!(s.duration_ms, 100);
        assert_eq!(s.timestamp_ms, 1000);
    }

    // --- BottleneckLocator tests ---

    #[test]
    fn locator_empty() {
        let l = BottleneckLocator::with_defaults();
        assert!(l.locate().is_empty());
        assert!(l.worst_bottleneck().is_none());
        assert!(!l.has_severe_bottleneck());
        assert_eq!(l.sample_count(), 0);
    }

    #[test]
    fn locator_single_sample() {
        let mut l = BottleneckLocator::with_defaults();
        l.sample("db.execute", 100, 0);
        let bottlenecks = l.locate();
        assert_eq!(bottlenecks.len(), 1);
        assert_eq!(bottlenecks[0].stage, "db.execute");
        assert!((bottlenecks[0].percentage - 100.0).abs() < 1e-9);
    }

    #[test]
    fn locator_multiple_stages() {
        let mut l = BottleneckLocator::with_defaults();
        l.sample("db.execute", 80, 0);
        l.sample("pool.acquire", 15, 0);
        l.sample("result.map", 5, 0);
        let bottlenecks = l.locate();
        assert_eq!(bottlenecks.len(), 3);
        assert_eq!(bottlenecks[0].stage, "db.execute");
        assert!((bottlenecks[0].percentage - 80.0).abs() < 1e-9);
    }

    #[test]
    fn locator_aggregates_same_stage() {
        let mut l = BottleneckLocator::with_defaults();
        l.sample("db.execute", 50, 0);
        l.sample("db.execute", 30, 0);
        l.sample("pool.acquire", 20, 0);
        let bottlenecks = l.locate();
        assert_eq!(bottlenecks.len(), 2);
        let db = bottlenecks
            .iter()
            .find(|b| b.stage == "db.execute")
            .unwrap();
        assert_eq!(db.total_duration_ms, 80);
        assert_eq!(db.sample_count, 2);
        assert!((db.avg_duration_ms - 40.0).abs() < 1e-9);
        assert_eq!(db.max_duration_ms, 50);
    }

    #[test]
    fn locator_worst_bottleneck() {
        let mut l = BottleneckLocator::with_defaults();
        l.sample("db.execute", 80, 0);
        l.sample("pool.acquire", 20, 0);
        let worst = l.worst_bottleneck().unwrap();
        assert_eq!(worst.stage, "db.execute");
    }

    #[test]
    fn locator_has_severe_bottleneck() {
        let mut l = BottleneckLocator::with_defaults();
        l.sample("db.execute", 60, 0);
        l.sample("pool.acquire", 40, 0);
        assert!(l.has_severe_bottleneck());
    }

    #[test]
    fn locator_no_severe_bottleneck() {
        let mut l = BottleneckLocator::with_defaults();
        l.sample("db.execute", 30, 0);
        l.sample("pool.acquire", 30, 0);
        l.sample("result.map", 40, 0);
        assert!(!l.has_severe_bottleneck());
    }

    #[test]
    fn locator_has_critical_bottleneck() {
        let mut l = BottleneckLocator::with_defaults();
        l.sample("db.execute", 75, 0);
        l.sample("pool.acquire", 25, 0);
        assert!(l.has_critical_bottleneck());
    }

    #[test]
    fn locator_top_n_limit() {
        let config = BottleneckLocatorConfig {
            top_n: 2,
            ..BottleneckLocatorConfig::default()
        };
        let mut l = BottleneckLocator::new(config);
        l.sample("a", 10, 0);
        l.sample("b", 20, 0);
        l.sample("c", 30, 0);
        l.sample("d", 40, 0);
        let bottlenecks = l.locate();
        assert_eq!(bottlenecks.len(), 2);
        assert_eq!(bottlenecks[0].stage, "d");
        assert_eq!(bottlenecks[1].stage, "c");
    }

    #[test]
    fn locator_clear() {
        let mut l = BottleneckLocator::with_defaults();
        l.sample("a", 10, 0);
        l.clear();
        assert_eq!(l.sample_count(), 0);
        assert!(l.locate().is_empty());
    }

    #[test]
    fn locator_add_samples_batch() {
        let mut l = BottleneckLocator::with_defaults();
        let samples = vec![
            BottleneckSample::new("a", 10, 0),
            BottleneckSample::new("b", 20, 0),
        ];
        l.add_samples(samples);
        assert_eq!(l.sample_count(), 2);
    }

    #[test]
    fn locator_report_empty() {
        let l = BottleneckLocator::with_defaults();
        let report = l.report();
        assert!(report.contains("无采样数据"));
    }

    #[test]
    fn locator_report_with_data() {
        let mut l = BottleneckLocator::with_defaults();
        l.sample("db.execute", 80, 0);
        l.sample("pool.acquire", 20, 0);
        let report = l.report();
        assert!(report.contains("db.execute"));
        assert!(report.contains("pool.acquire"));
    }

    #[test]
    fn locator_zero_duration_no_panic() {
        let mut l = BottleneckLocator::with_defaults();
        l.sample("a", 0, 0);
        l.sample("b", 0, 0);
        let bottlenecks = l.locate();
        assert!(bottlenecks.is_empty());
    }

    #[test]
    fn locator_severity_assigned() {
        let mut l = BottleneckLocator::with_defaults();
        l.sample("db.execute", 75, 0);
        l.sample("pool.acquire", 25, 0);
        let bottlenecks = l.locate();
        let db = bottlenecks
            .iter()
            .find(|b| b.stage == "db.execute")
            .unwrap();
        assert_eq!(db.severity, BottleneckSeverity::Critical);
    }

    // --- BottleneckTrendAnalyzer tests ---

    #[test]
    fn trend_analyzer_empty() {
        let a = BottleneckTrendAnalyzer::new(1000);
        assert_eq!(a.window_count(), 0);
        assert!(!a.is_persistent_bottleneck("db.execute", 50.0));
    }

    #[test]
    fn trend_analyzer_record() {
        let mut a = BottleneckTrendAnalyzer::new(1000);
        let bottlenecks = vec![Bottleneck {
            stage: "db.execute".to_string(),
            total_duration_ms: 80,
            percentage: 80.0,
            severity: BottleneckSeverity::Critical,
            sample_count: 1,
            avg_duration_ms: 80.0,
            max_duration_ms: 80,
        }];
        a.record(0, bottlenecks);
        assert_eq!(a.window_count(), 1);
    }

    #[test]
    fn trend_analyzer_persistent_bottleneck() {
        let mut a = BottleneckTrendAnalyzer::new(1000);
        let make_bottleneck = || {
            vec![Bottleneck {
                stage: "db.execute".to_string(),
                total_duration_ms: 80,
                percentage: 80.0,
                severity: BottleneckSeverity::Critical,
                sample_count: 1,
                avg_duration_ms: 80.0,
                max_duration_ms: 80,
            }]
        };
        a.record(0, make_bottleneck());
        a.record(1000, make_bottleneck());
        a.record(2000, make_bottleneck());
        assert!(a.is_persistent_bottleneck("db.execute", 50.0));
    }

    #[test]
    fn trend_analyzer_not_persistent() {
        let mut a = BottleneckTrendAnalyzer::new(1000);
        let high = vec![Bottleneck {
            stage: "db.execute".to_string(),
            total_duration_ms: 80,
            percentage: 80.0,
            severity: BottleneckSeverity::Critical,
            sample_count: 1,
            avg_duration_ms: 80.0,
            max_duration_ms: 80,
        }];
        let low = vec![Bottleneck {
            stage: "db.execute".to_string(),
            total_duration_ms: 10,
            percentage: 10.0,
            severity: BottleneckSeverity::Minor,
            sample_count: 1,
            avg_duration_ms: 10.0,
            max_duration_ms: 10,
        }];
        a.record(0, high);
        a.record(1000, low);
        assert!(!a.is_persistent_bottleneck("db.execute", 50.0));
    }

    #[test]
    fn trend_analyzer_trend_improving() {
        let mut a = BottleneckTrendAnalyzer::new(1000);
        let high = vec![Bottleneck {
            stage: "db.execute".to_string(),
            total_duration_ms: 80,
            percentage: 80.0,
            severity: BottleneckSeverity::Critical,
            sample_count: 1,
            avg_duration_ms: 80.0,
            max_duration_ms: 80,
        }];
        let low = vec![Bottleneck {
            stage: "db.execute".to_string(),
            total_duration_ms: 30,
            percentage: 30.0,
            severity: BottleneckSeverity::Moderate,
            sample_count: 1,
            avg_duration_ms: 30.0,
            max_duration_ms: 30,
        }];
        a.record(0, high);
        a.record(1000, low);
        let trend = a.trend("db.execute");
        assert!(trend < 0.0, "trend should be negative (improving)");
    }

    #[test]
    fn trend_analyzer_trend_worsening() {
        let mut a = BottleneckTrendAnalyzer::new(1000);
        let low = vec![Bottleneck {
            stage: "db.execute".to_string(),
            total_duration_ms: 30,
            percentage: 30.0,
            severity: BottleneckSeverity::Moderate,
            sample_count: 1,
            avg_duration_ms: 30.0,
            max_duration_ms: 30,
        }];
        let high = vec![Bottleneck {
            stage: "db.execute".to_string(),
            total_duration_ms: 80,
            percentage: 80.0,
            severity: BottleneckSeverity::Critical,
            sample_count: 1,
            avg_duration_ms: 80.0,
            max_duration_ms: 80,
        }];
        a.record(0, low);
        a.record(1000, high);
        let trend = a.trend("db.execute");
        assert!(trend > 0.0, "trend should be positive (worsening)");
    }

    #[test]
    fn trend_analyzer_trend_single_window() {
        let mut a = BottleneckTrendAnalyzer::new(1000);
        a.record(
            0,
            vec![Bottleneck {
                stage: "db.execute".to_string(),
                total_duration_ms: 80,
                percentage: 80.0,
                severity: BottleneckSeverity::Critical,
                sample_count: 1,
                avg_duration_ms: 80.0,
                max_duration_ms: 80,
            }],
        );
        assert_eq!(a.trend("db.execute"), 0.0);
    }

    #[test]
    fn trend_analyzer_persistent_bottlenecks_list() {
        let mut a = BottleneckTrendAnalyzer::new(1000);
        let make = || {
            vec![Bottleneck {
                stage: "db.execute".to_string(),
                total_duration_ms: 80,
                percentage: 80.0,
                severity: BottleneckSeverity::Critical,
                sample_count: 1,
                avg_duration_ms: 80.0,
                max_duration_ms: 80,
            }]
        };
        a.record(0, make());
        a.record(1000, make());
        let persistent = a.persistent_bottlenecks(50.0);
        assert!(persistent.contains(&"db.execute".to_string()));
    }

    // --- BottleneckLocatorConfig tests ---

    #[test]
    fn config_default_values() {
        let c = BottleneckLocatorConfig::default();
        assert!((c.severe_threshold_pct - 50.0).abs() < 1e-9);
        assert!((c.critical_threshold_pct - 70.0).abs() < 1e-9);
        assert_eq!(c.top_n, 5);
    }
}
