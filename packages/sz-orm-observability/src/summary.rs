//! Summary 指标与 Pushgateway 导出
//!
//! 提供 Prometheus Summary 指标类型（基于分位数）与 Pushgateway 导出器。
//!
//! ## Summary vs Histogram
//!
//! - **Histogram**：预定义 bucket 边界，适合已知分布范围的指标（如延迟）。
//! - **Summary**：客户端计算分位数，适合需要精确 p50/p90/p99 的场景，
//!   但无法跨实例聚合。
//!
//! ## Pushgateway
//!
//! Pushgateway 用于短生命周期任务的指标推送：任务结束后将指标推送到
//! Pushgateway，Prometheus 再从 Pushgateway 拉取。本模块提供内存模拟
//! 实现，不进行真实网络发送，便于测试与本地开发。

use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

/// Summary 指标，基于排序数组计算分位数。
///
/// 与 Prometheus Summary 类似，记录所有观测值并按需计算分位数。
/// 适合样本量可控的场景；超大规模样本应考虑 T-Digest 近似算法。
pub struct Summary {
    name: String,
    help: String,
    /// 预配置的分位数列表（如 [0.5, 0.9, 0.99]）
    quantiles: Vec<f64>,
    /// 已排序的观测样本
    samples: Arc<RwLock<Vec<f64>>>,
    /// 样本总和
    sum: Arc<RwLock<f64>>,
    /// 样本计数
    count: Arc<RwLock<u64>>,
}

impl Summary {
    /// 创建新的 Summary 指标
    ///
    /// # 参数
    /// - `name`：指标名（如 `request_duration_seconds`）
    /// - `help`：帮助文本
    /// - `quantiles`：要计算的分位数列表（如 `[0.5, 0.9, 0.99]`）
    pub fn new(name: impl Into<String>, help: impl Into<String>, quantiles: Vec<f64>) -> Self {
        Self {
            name: name.into(),
            help: help.into(),
            quantiles,
            samples: Arc::new(RwLock::new(Vec::new())),
            sum: Arc::new(RwLock::new(0.0)),
            count: Arc::new(RwLock::new(0)),
        }
    }

    /// 观测一个值
    pub fn observe(&self, value: f64) {
        let mut samples = self.samples.write();
        let pos = samples.partition_point(|&v| v < value);
        samples.insert(pos, value);

        let mut sum = self.sum.write();
        *sum += value;

        let mut count = self.count.write();
        *count += 1;
    }

    /// 计算指定分位数的值（0.0..=1.0）
    ///
    /// 使用最近排名法（Nearest Rank）：`rank = ceil(p * n)`，至少为 1。
    /// 返回 `None` 表示无样本或分位数超出范围。
    pub fn quantile(&self, q: f64) -> Option<f64> {
        if !(0.0..=1.0).contains(&q) {
            return None;
        }
        let samples = self.samples.read();
        if samples.is_empty() {
            return None;
        }
        let n = samples.len();
        let rank = ((q * n as f64).ceil() as usize).max(1).min(n);
        Some(samples[rank - 1])
    }

    /// 计算所有预配置分位数，返回 (分位数, 值) 列表
    pub fn quantiles(&self) -> Vec<(f64, Option<f64>)> {
        self.quantiles.iter().map(|&q| (q, self.quantile(q))).collect()
    }

    /// 样本计数
    pub fn count(&self) -> u64 {
        *self.count.read()
    }

    /// 样本总和
    pub fn sum(&self) -> f64 {
        *self.sum.read()
    }

    /// 指标名
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 渲染为 Prometheus 文本格式
    pub fn render(&self) -> String {
        let samples = self.samples.read();
        let sum = *self.sum.read();
        let count = *self.count.read();

        let mut output = String::new();
        output.push_str(&format!("# HELP {} {}\n", self.name, self.help));
        output.push_str(&format!("# TYPE {} summary\n", self.name));

        for &q in &self.quantiles {
            let value = if samples.is_empty() {
                0.0
            } else {
                let n = samples.len();
                let rank = ((q * n as f64).ceil() as usize).max(1).min(n);
                samples[rank - 1]
            };
            output.push_str(&format!("{}{{quantile=\"{}\"}} {}\n", self.name, q, value));
        }

        output.push_str(&format!("{}_sum {}\n", self.name, sum));
        output.push_str(&format!("{}_count {}\n", self.name, count));
        output
    }

    /// 重置所有样本
    pub fn reset(&self) {
        let mut samples = self.samples.write();
        samples.clear();
        *self.sum.write() = 0.0;
        *self.count.write() = 0;
    }
}

/// 带标签的 Histogram 扩展。
///
/// 标准 [`crate::Histogram`] 不支持在同一指标名下按标签区分。
/// `LabeledHistogram` 通过标签集合区分同一指标名的不同时间序列。
pub struct LabeledHistogram {
    /// 指标名
    name: String,
    /// 帮助文本
    help: String,
    /// bucket 边界
    buckets: Vec<f64>,
    /// 按标签键排序后的拼接字符串索引的子直方图
    series: RwLock<HashMap<String, LabeledSeries>>,
}

/// 单个标签组合对应的直方图数据
#[derive(Debug, Clone)]
struct LabeledSeries {
    /// 标签键值对（已排序）
    labels: Vec<(String, String)>,
    /// 各 bucket 的累计计数
    counts: Vec<u64>,
    /// 样本总和
    sum: f64,
    /// 样本计数
    count: u64,
}

impl LabeledSeries {
    fn new(labels: Vec<(String, String)>, bucket_count: usize) -> Self {
        Self {
            labels,
            counts: vec![0; bucket_count],
            sum: 0.0,
            count: 0,
        }
    }

    fn label_key(labels: &[(String, String)]) -> String {
        labels
            .iter()
            .map(|(k, v)| format!("{}=\"{}\"", k, v.replace('"', "\\\"")))
            .collect::<Vec<_>>()
            .join(",")
    }
}

impl LabeledHistogram {
    /// 创建带标签的直方图
    ///
    /// # 参数
    /// - `name`：指标名
    /// - `help`：帮助文本
    /// - `buckets`：bucket 边界（无需包含 +Inf，会自动追加）
    pub fn new(
        name: impl Into<String>,
        help: impl Into<String>,
        mut buckets: Vec<f64>,
    ) -> Self {
        buckets.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        if !buckets.contains(&f64::INFINITY) {
            buckets.push(f64::INFINITY);
        }
        Self {
            name: name.into(),
            help: help.into(),
            buckets,
            series: RwLock::new(HashMap::new()),
        }
    }

    /// 观测一个带标签的值
    ///
    /// # 参数
    /// - `labels`：标签键值对（顺序无关，内部会排序）
    /// - `value`：观测值
    pub fn observe(&self, labels: &HashMap<String, String>, value: f64) {
        let mut sorted_labels: Vec<(String, String)> =
            labels.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        sorted_labels.sort_by(|a, b| a.0.cmp(&b.0));

        let key = LabeledSeries::label_key(&sorted_labels);
        let mut series = self.series.write();
        let entry = series
            .entry(key)
            .or_insert_with(|| LabeledSeries::new(sorted_labels.clone(), self.buckets.len()));

        for (i, bucket) in self.buckets.iter().enumerate() {
            if value <= *bucket {
                entry.counts[i] += 1;
            }
        }
        entry.sum += value;
        entry.count += 1;
    }

    /// 获取指定标签组合的样本计数
    pub fn count(&self, labels: &HashMap<String, String>) -> u64 {
        let mut sorted_labels: Vec<(String, String)> =
            labels.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        sorted_labels.sort_by(|a, b| a.0.cmp(&b.0));
        let key = LabeledSeries::label_key(&sorted_labels);
        self.series
            .read()
            .get(&key)
            .map(|s| s.count)
            .unwrap_or(0)
    }

    /// 获取所有标签组合数量
    pub fn label_combination_count(&self) -> usize {
        self.series.read().len()
    }

    /// 渲染为 Prometheus 文本格式
    pub fn render(&self) -> String {
        let series = self.series.read();
        let mut output = String::new();
        output.push_str(&format!("# HELP {} {}\n", self.name, self.help));
        output.push_str(&format!("# TYPE {} histogram\n", self.name));

        for s in series.values() {
            let label_str = LabeledSeries::label_key(&s.labels);
            for (i, bucket) in self.buckets.iter().enumerate() {
                if *bucket == f64::INFINITY {
                    output.push_str(&format!(
                        "{}_bucket{{{},le=\"+Inf\"}} {}\n",
                        self.name, label_str, s.counts[i]
                    ));
                } else {
                    output.push_str(&format!(
                        "{}_bucket{{{},le=\"{}\"}} {}\n",
                        self.name, label_str, bucket, s.counts[i]
                    ));
                }
            }
            output.push_str(&format!("{}_sum{{{}}} {}\n", self.name, label_str, s.sum));
            output.push_str(&format!("{}_count{{{}}} {}\n", self.name, label_str, s.count));
        }

        output
    }
}

/// Pushgateway 导出配置
#[derive(Debug, Clone)]
pub struct PushgatewayConfig {
    /// Pushgateway 地址（如 `http://localhost:9091`）
    pub endpoint: String,
    /// 作业名（job label）
    pub job: String,
    /// 实例标签（可选）
    pub instance: Option<String>,
}

impl Default for PushgatewayConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:9091".to_string(),
            job: "sz-orm".to_string(),
            instance: None,
        }
    }
}

/// Pushgateway 导出器（内存模拟）。
///
/// 模拟将指标推送到 Prometheus Pushgateway 的行为。
/// 实际网络发送被替换为内存记录，便于测试验证。
pub struct PushgatewayExporter {
    config: PushgatewayConfig,
    /// 已推送的指标文本快照
    pushed: RwLock<Vec<PushSnapshot>>,
}

/// 一次推送的快照
#[derive(Debug, Clone)]
pub struct PushSnapshot {
    /// 推送时间戳（Unix 毫秒）
    pub timestamp_ms: i64,
    /// 推送的指标文本
    pub metrics_text: String,
    /// 作业名
    pub job: String,
    /// 实例名
    pub instance: Option<String>,
}

impl PushgatewayExporter {
    /// 创建新的 Pushgateway 导出器
    pub fn new(config: PushgatewayConfig) -> Self {
        Self {
            config,
            pushed: RwLock::new(Vec::new()),
        }
    }

    /// 模拟推送指标到 Pushgateway
    ///
    /// 将渲染后的指标文本记录到内存，返回推送是否成功。
    /// 实际实现中此处会发起 HTTP PUT 请求。
    pub fn push(&self, metrics_text: impl Into<String>) -> Result<(), String> {
        let snapshot = PushSnapshot {
            timestamp_ms: current_timestamp_ms(),
            metrics_text: metrics_text.into(),
            job: self.config.job.clone(),
            instance: self.config.instance.clone(),
        };
        let mut pushed = self.pushed.write();
        pushed.push(snapshot);
        Ok(())
    }

    /// 从 MetricsRegistry 渲染并推送
    pub fn push_from_registry(&self, registry: &crate::MetricsRegistry) -> Result<(), String> {
        let text = registry.render();
        self.push(text)
    }

    /// 获取推送历史快照
    pub fn snapshots(&self) -> Vec<PushSnapshot> {
        self.pushed.read().clone()
    }

    /// 获取推送次数
    pub fn push_count(&self) -> usize {
        self.pushed.read().len()
    }

    /// 清空推送历史
    pub fn clear(&self) {
        self.pushed.write().clear();
    }

    /// 获取配置引用
    pub fn config(&self) -> &PushgatewayConfig {
        &self.config
    }
}

fn current_timestamp_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===================== Summary 测试 =====================

    #[test]
    fn test_summary_new_empty() {
        let s = Summary::new("latency", "latency summary", vec![0.5, 0.9, 0.99]);
        assert_eq!(s.count(), 0);
        assert_eq!(s.sum(), 0.0);
        assert!(s.quantile(0.5).is_none());
    }

    #[test]
    fn test_summary_observe_single() {
        let s = Summary::new("latency", "help", vec![0.5]);
        s.observe(1.5);
        assert_eq!(s.count(), 1);
        assert!((s.sum() - 1.5).abs() < 1e-9);
        assert!((s.quantile(0.5).unwrap() - 1.5).abs() < 1e-9);
    }

    #[test]
    fn test_summary_observe_multiple_p50() {
        let s = Summary::new("latency", "help", vec![0.5]);
        for v in [1.0, 2.0, 3.0, 4.0, 5.0] {
            s.observe(v);
        }
        // p50 of [1,2,3,4,5] -> rank=ceil(0.5*5)=3 -> samples[2]=3.0
        assert!((s.quantile(0.5).unwrap() - 3.0).abs() < 1e-9);
    }

    #[test]
    fn test_summary_observe_multiple_p99() {
        let s = Summary::new("latency", "help", vec![0.99]);
        for v in [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 100.0] {
            s.observe(v);
        }
        // p99 of 10 samples -> rank=ceil(0.99*10)=10 -> samples[9]=100.0
        assert!((s.quantile(0.99).unwrap() - 100.0).abs() < 1e-9);
    }

    #[test]
    fn test_summary_quantile_out_of_range() {
        let s = Summary::new("latency", "help", vec![0.5]);
        s.observe(1.0);
        assert!(s.quantile(-0.1).is_none());
        assert!(s.quantile(1.1).is_none());
    }

    #[test]
    fn test_summary_quantile_empty() {
        let s = Summary::new("latency", "help", vec![0.5]);
        assert!(s.quantile(0.5).is_none());
    }

    #[test]
    fn test_summary_quantile_p0_and_p1() {
        let s = Summary::new("latency", "help", vec![]);
        for v in [10.0, 20.0, 30.0] {
            s.observe(v);
        }
        // p0 -> rank=ceil(0*3)=0 -> max(0,1)=1 -> samples[0]=10
        assert!((s.quantile(0.0).unwrap() - 10.0).abs() < 1e-9);
        // p1 -> rank=ceil(1*3)=3 -> samples[2]=30
        assert!((s.quantile(1.0).unwrap() - 30.0).abs() < 1e-9);
    }

    #[test]
    fn test_summary_quantiles_all() {
        let s = Summary::new("latency", "help", vec![0.5, 0.9, 0.99]);
        for v in [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0] {
            s.observe(v);
        }
        let qs = s.quantiles();
        assert_eq!(qs.len(), 3);
        assert!(qs.iter().all(|(_, v)| v.is_some()));
    }

    #[test]
    fn test_summary_unsorted_input_stays_sorted() {
        let s = Summary::new("latency", "help", vec![0.5]);
        s.observe(50.0);
        s.observe(10.0);
        s.observe(30.0);
        // p50 of sorted [10,30,50] -> rank=ceil(0.5*3)=2 -> samples[1]=30
        assert!((s.quantile(0.5).unwrap() - 30.0).abs() < 1e-9);
    }

    #[test]
    fn test_summary_render_contains_type() {
        let s = Summary::new("latency", "latency help", vec![0.5, 0.99]);
        s.observe(1.0);
        let output = s.render();
        assert!(output.contains("# HELP latency latency help"));
        assert!(output.contains("# TYPE latency summary"));
        assert!(output.contains("latency{quantile=\"0.5\"}"));
        assert!(output.contains("latency{quantile=\"0.99\"}"));
        assert!(output.contains("latency_sum"));
        assert!(output.contains("latency_count"));
    }

    #[test]
    fn test_summary_render_empty_shows_zero() {
        let s = Summary::new("latency", "help", vec![0.5]);
        let output = s.render();
        // 空样本时分位数值为 0
        assert!(output.contains("latency{quantile=\"0.5\"} 0"));
        assert!(output.contains("latency_count 0"));
    }

    #[test]
    fn test_summary_reset() {
        let s = Summary::new("latency", "help", vec![0.5]);
        s.observe(1.0);
        s.observe(2.0);
        assert_eq!(s.count(), 2);

        s.reset();
        assert_eq!(s.count(), 0);
        assert!((s.sum() - 0.0).abs() < 1e-9);
        assert!(s.quantile(0.5).is_none());
    }

    #[test]
    fn test_summary_name() {
        let s = Summary::new("my_metric", "help", vec![0.5]);
        assert_eq!(s.name(), "my_metric");
    }

    // ===================== LabeledHistogram 测试 =====================

    #[test]
    fn test_labeled_histogram_new() {
        let h = LabeledHistogram::new("requests", "help", vec![0.1, 0.5, 1.0]);
        assert_eq!(h.label_combination_count(), 0);
    }

    #[test]
    fn test_labeled_histogram_observe_single_label() {
        let h = LabeledHistogram::new("requests", "help", vec![0.1, 0.5, 1.0]);
        let mut labels = HashMap::new();
        labels.insert("method".to_string(), "GET".to_string());

        h.observe(&labels, 0.3);
        assert_eq!(h.count(&labels), 1);
        assert_eq!(h.label_combination_count(), 1);
    }

    #[test]
    fn test_labeled_histogram_observe_multiple_labels() {
        let h = LabeledHistogram::new("requests", "help", vec![0.1, 0.5, 1.0]);

        let mut get_labels = HashMap::new();
        get_labels.insert("method".to_string(), "GET".to_string());

        let mut post_labels = HashMap::new();
        post_labels.insert("method".to_string(), "POST".to_string());

        h.observe(&get_labels, 0.1);
        h.observe(&get_labels, 0.2);
        h.observe(&post_labels, 0.5);

        assert_eq!(h.count(&get_labels), 2);
        assert_eq!(h.count(&post_labels), 1);
        assert_eq!(h.label_combination_count(), 2);
    }

    #[test]
    fn test_labeled_histogram_label_order_independent() {
        let h = LabeledHistogram::new("requests", "help", vec![0.1, 1.0]);

        let mut labels1 = HashMap::new();
        labels1.insert("a".to_string(), "1".to_string());
        labels1.insert("b".to_string(), "2".to_string());

        let mut labels2 = HashMap::new();
        labels2.insert("b".to_string(), "2".to_string());
        labels2.insert("a".to_string(), "1".to_string());

        h.observe(&labels1, 0.5);
        // 标签顺序不同但键值对相同，应归入同一时间序列
        assert_eq!(h.count(&labels2), 1);
        assert_eq!(h.label_combination_count(), 1);
    }

    #[test]
    fn test_labeled_histogram_count_missing_labels() {
        let h = LabeledHistogram::new("requests", "help", vec![0.1, 1.0]);
        let labels = HashMap::new();
        assert_eq!(h.count(&labels), 0);
    }

    #[test]
    fn test_labeled_histogram_render_contains_labels() {
        let h = LabeledHistogram::new("requests", "request help", vec![0.1, 1.0]);
        let mut labels = HashMap::new();
        labels.insert("method".to_string(), "GET".to_string());
        h.observe(&labels, 0.05);

        let output = h.render();
        assert!(output.contains("# HELP requests request help"));
        assert!(output.contains("# TYPE requests histogram"));
        assert!(output.contains("method=\"GET\""));
        assert!(output.contains("requests_count"));
        assert!(output.contains("requests_sum"));
    }

    #[test]
    fn test_labeled_histogram_render_inf_bucket() {
        let h = LabeledHistogram::new("req", "help", vec![0.1]);
        let labels = HashMap::new();
        h.observe(&labels, 0.05);
        h.observe(&labels, 5.0);
        let output = h.render();
        assert!(output.contains("le=\"+Inf\""));
    }

    // ===================== PushgatewayExporter 测试 =====================

    #[test]
    fn test_pushgateway_config_default() {
        let config = PushgatewayConfig::default();
        assert_eq!(config.endpoint, "http://localhost:9091");
        assert_eq!(config.job, "sz-orm");
        assert!(config.instance.is_none());
    }

    #[test]
    fn test_pushgateway_exporter_new() {
        let exporter = PushgatewayExporter::new(PushgatewayConfig::default());
        assert_eq!(exporter.push_count(), 0);
        assert!(exporter.snapshots().is_empty());
    }

    #[test]
    fn test_pushgateway_push_text() {
        let exporter = PushgatewayExporter::new(PushgatewayConfig::default());
        exporter.push("metric1 1\n").unwrap();
        exporter.push("metric2 2\n").unwrap();

        assert_eq!(exporter.push_count(), 2);
        let snaps = exporter.snapshots();
        assert_eq!(snaps.len(), 2);
        assert_eq!(snaps[0].metrics_text, "metric1 1\n");
        assert_eq!(snaps[1].metrics_text, "metric2 2\n");
    }

    #[test]
    fn test_pushgateway_push_from_registry() {
        let registry = crate::MetricsRegistry::new();
        let counter = registry.register_counter("test_total", "test");
        counter.inc();

        let exporter = PushgatewayExporter::new(PushgatewayConfig::default());
        exporter.push_from_registry(&registry).unwrap();

        assert_eq!(exporter.push_count(), 1);
        let snap = &exporter.snapshots()[0];
        assert!(snap.metrics_text.contains("test_total"));
    }

    #[test]
    fn test_pushgateway_snapshot_has_metadata() {
        let config = PushgatewayConfig {
            endpoint: "http://push:9091".to_string(),
            job: "myjob".to_string(),
            instance: Some("inst1".to_string()),
        };
        let exporter = PushgatewayExporter::new(config);
        exporter.push("m 1\n").unwrap();

        let snap = &exporter.snapshots()[0];
        assert_eq!(snap.job, "myjob");
        assert_eq!(snap.instance, Some("inst1".to_string()));
        assert!(snap.timestamp_ms > 0);
    }

    #[test]
    fn test_pushgateway_clear() {
        let exporter = PushgatewayExporter::new(PushgatewayConfig::default());
        exporter.push("m 1\n").unwrap();
        assert_eq!(exporter.push_count(), 1);

        exporter.clear();
        assert_eq!(exporter.push_count(), 0);
    }

    #[test]
    fn test_pushgateway_config_access() {
        let config = PushgatewayConfig {
            job: "custom".to_string(),
            ..Default::default()
        };
        let exporter = PushgatewayExporter::new(config);
        assert_eq!(exporter.config().job, "custom");
    }
}
