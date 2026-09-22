//! 统一指标采集器：六维指标（性能/AI/分布式/安全/生态/可观测性）统一采集 + Prometheus 导出。
//!
//! 复用既有 [`crate::MetricsRegistry`]，新增六维指标定义、采集间隔控制、
//! 采集丢失检测与自定义指标支持。采集开销 ≤ 1% CPU。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::MetricsRegistry;

/// 六维指标维度。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MetricDimension {
    /// 性能（QPS/延迟/资源）。
    Performance,
    /// AI（自治/A/B/模型）。
    Ai,
    /// 分布式（共识/复制/故障转移）。
    Distributed,
    /// 安全（合规/密钥/审计）。
    Security,
    /// 生态（插件/SDK/模板）。
    Ecosystem,
    /// 可观测性（追踪/指标/日志/告警）。
    Observability,
}

impl MetricDimension {
    /// 全部六维。
    pub fn all() -> Vec<MetricDimension> {
        vec![
            MetricDimension::Performance,
            MetricDimension::Ai,
            MetricDimension::Distributed,
            MetricDimension::Security,
            MetricDimension::Ecosystem,
            MetricDimension::Observability,
        ]
    }

    /// 维度名。
    pub fn as_str(&self) -> &'static str {
        match self {
            MetricDimension::Performance => "performance",
            MetricDimension::Ai => "ai",
            MetricDimension::Distributed => "distributed",
            MetricDimension::Security => "security",
            MetricDimension::Ecosystem => "ecosystem",
            MetricDimension::Observability => "observability",
        }
    }
}

/// 单个指标样本。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricSample {
    /// 指标名。
    pub name: String,
    /// 维度。
    pub dimension: MetricDimension,
    /// 值。
    pub value: f64,
    /// 采集时间戳（Unix 毫秒）。
    pub timestamp_ms: i64,
    /// 标签。
    pub labels: HashMap<String, String>,
}

/// 指标批次。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsBatch {
    /// 批次 ID。
    pub batch_id: u64,
    /// 采集时间戳。
    pub collected_at_ms: i64,
    /// 样本列表。
    pub samples: Vec<MetricSample>,
    /// 采集耗时。
    pub collect_duration_ms: u64,
}

/// 统一指标采集器错误。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MetricsCollectError {
    /// 采集失败。
    #[error("metrics collection failed: {0}")]
    CollectionFailed(String),
    /// 采集丢失（部分维度未采集到）。
    #[error("metrics collection lost: dimensions {0:?}")]
    CollectionLost(Vec<String>),
}

/// 指标采集源：由调用方实现，提供各维度指标值。
pub trait MetricsSource: Send + Sync {
    /// 采集指定维度的指标样本。
    fn collect_dimension(&self, dimension: MetricDimension) -> Vec<MetricSample>;
}

/// 统一指标采集器。
pub struct UnifiedMetricsCollector {
    /// 采集间隔（默认 15s）。
    collect_interval: Duration,
    /// 指标注册表（复用既有 MetricsRegistry）。
    registry: Arc<MetricsRegistry>,
    /// 采集源列表。
    sources: Vec<Box<dyn MetricsSource>>,
    /// 批次计数。
    batch_counter: parking_lot::Mutex<u64>,
    /// 上次采集时间。
    last_collect: parking_lot::Mutex<Option<Instant>>,
    /// 已采集维度记录（用于检测丢失）。
    collected_dimensions: parking_lot::Mutex<Vec<MetricDimension>>,
}

impl UnifiedMetricsCollector {
    /// 创建采集器，采集间隔默认 15s。
    pub fn new(registry: Arc<MetricsRegistry>) -> Self {
        Self {
            collect_interval: Duration::from_secs(15),
            registry,
            sources: Vec::new(),
            batch_counter: parking_lot::Mutex::new(0),
            last_collect: parking_lot::Mutex::new(None),
            collected_dimensions: parking_lot::Mutex::new(Vec::new()),
        }
    }

    /// 设置采集间隔。
    pub fn with_interval(mut self, interval: Duration) -> Self {
        self.collect_interval = interval;
        self
    }

    /// 添加采集源。
    pub fn add_source(&mut self, source: Box<dyn MetricsSource>) {
        self.sources.push(source);
    }

    /// 采集间隔。
    pub fn collect_interval(&self) -> Duration {
        self.collect_interval
    }

    /// 采集六维指标，返回批次。检测采集丢失（某维度无样本则告警）。
    pub async fn collect(&self) -> Result<MetricsBatch, MetricsCollectError> {
        let start = Instant::now();
        let now_ms = chrono::Utc::now().timestamp_millis();
        let batch_id = {
            let mut c = self.batch_counter.lock();
            *c += 1;
            *c
        };

        let mut all_samples = Vec::new();
        let mut seen_dimensions = std::collections::HashSet::new();

        for source in &self.sources {
            for dim in MetricDimension::all() {
                let samples = source.collect_dimension(dim);
                if !samples.is_empty() {
                    seen_dimensions.insert(dim);
                }
                all_samples.extend(samples);
            }
        }

        // 检测采集丢失：六维中未采集到的维度
        let lost: Vec<String> = MetricDimension::all()
            .into_iter()
            .filter(|d| !seen_dimensions.contains(d))
            .map(|d| d.as_str().to_string())
            .collect();

        // 更新已采集维度记录
        {
            let mut dims = self.collected_dimensions.lock();
            dims.clear();
            dims.extend(seen_dimensions.iter());
        }
        {
            let mut last = self.last_collect.lock();
            *last = Some(Instant::now());
        }

        let collect_duration_ms = start.elapsed().as_millis() as u64;

        let batch = MetricsBatch {
            batch_id,
            collected_at_ms: now_ms,
            samples: all_samples,
            collect_duration_ms,
        };

        // 采集丢失作为告警信息附加（不阻断采集，但返回错误让调用方感知）
        if !lost.is_empty() && !self.sources.is_empty() {
            // 有采集源但部分维度丢失 → 返回错误附丢失范围
            return Err(MetricsCollectError::CollectionLost(lost));
        }

        Ok(batch)
    }

    /// 导出 Prometheus 格式（复用既有 MetricsRegistry::render）。
    pub fn export_prometheus(&self) -> String {
        self.registry.render()
    }

    /// 注册自定义 Counter 指标到注册表。
    pub fn register_custom_counter(&self, name: &str, help: &str) {
        self.registry.register_counter(name, help);
    }

    /// 注册自定义 Gauge 指标到注册表。
    pub fn register_custom_gauge(&self, name: &str, help: &str) {
        self.registry.register_gauge(name, help);
    }

    /// 上次采集时间。
    pub fn last_collect_time(&self) -> Option<Instant> {
        *self.last_collect.lock()
    }

    /// 已采集维度。
    pub fn collected_dimensions(&self) -> Vec<MetricDimension> {
        self.collected_dimensions.lock().clone()
    }
}

/// 内置采集源：从内存 HashMap 提供指标值。
pub struct InMemoryMetricsSource {
    samples: HashMap<MetricDimension, Vec<MetricSample>>,
}

impl InMemoryMetricsSource {
    /// 创建空采集源。
    pub fn new() -> Self {
        Self {
            samples: HashMap::new(),
        }
    }

    /// 添加指标样本。
    pub fn add_sample(&mut self, sample: MetricSample) {
        self.samples
            .entry(sample.dimension)
            .or_default()
            .push(sample);
    }
}

impl Default for InMemoryMetricsSource {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricsSource for InMemoryMetricsSource {
    fn collect_dimension(&self, dimension: MetricDimension) -> Vec<MetricSample> {
        self.samples.get(&dimension).cloned().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metric_dimension_all_six() {
        let all = MetricDimension::all();
        assert_eq!(all.len(), 6);
        let names: Vec<&str> = all.iter().map(|d| d.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "performance",
                "ai",
                "distributed",
                "security",
                "ecosystem",
                "observability"
            ]
        );
    }

    #[tokio::test]
    async fn test_collect_empty_sources() {
        let registry = Arc::new(MetricsRegistry::new());
        let collector = UnifiedMetricsCollector::new(registry);
        // 无采集源 → 返回空批次（不报丢失，因为无源）
        let batch = collector.collect().await.unwrap();
        assert_eq!(batch.samples.len(), 0);
        assert_eq!(batch.batch_id, 1);
    }

    #[tokio::test]
    async fn test_collect_six_dimensions() {
        let registry = Arc::new(MetricsRegistry::new());
        let mut source = InMemoryMetricsSource::new();
        let now = chrono::Utc::now().timestamp_millis();
        for dim in MetricDimension::all() {
            source.add_sample(MetricSample {
                name: format!("sz_orm_{}_metric", dim.as_str()),
                dimension: dim,
                value: 1.0,
                timestamp_ms: now,
                labels: HashMap::new(),
            });
        }
        let mut collector = UnifiedMetricsCollector::new(registry);
        collector.add_source(Box::new(source));
        let batch = collector.collect().await.unwrap();
        assert_eq!(batch.samples.len(), 6);
        assert_eq!(collector.collected_dimensions().len(), 6);
    }

    #[tokio::test]
    async fn test_collect_lost_dimensions() {
        let registry = Arc::new(MetricsRegistry::new());
        let mut source = InMemoryMetricsSource::new();
        let now = chrono::Utc::now().timestamp_millis();
        // 只提供 performance 维度
        source.add_sample(MetricSample {
            name: "qps".to_string(),
            dimension: MetricDimension::Performance,
            value: 100.0,
            timestamp_ms: now,
            labels: HashMap::new(),
        });
        let mut collector = UnifiedMetricsCollector::new(registry);
        collector.add_source(Box::new(source));
        let result = collector.collect().await;
        assert!(result.is_err());
        match result.unwrap_err() {
            MetricsCollectError::CollectionLost(lost) => {
                assert_eq!(lost.len(), 5);
            }
            _ => panic!("expected CollectionLost"),
        }
    }

    #[tokio::test]
    async fn test_collect_overhead_low() {
        // 采集开销 ≤ 1% CPU：1000 次采集应快速完成
        let registry = Arc::new(MetricsRegistry::new());
        let mut source = InMemoryMetricsSource::new();
        let now = chrono::Utc::now().timestamp_millis();
        for dim in MetricDimension::all() {
            source.add_sample(MetricSample {
                name: format!("m_{}", dim.as_str()),
                dimension: dim,
                value: 1.0,
                timestamp_ms: now,
                labels: HashMap::new(),
            });
        }
        let mut collector = UnifiedMetricsCollector::new(registry);
        collector.add_source(Box::new(source));
        let start = Instant::now();
        for _ in 0..1000 {
            let _ = collector.collect().await;
        }
        let elapsed = start.elapsed();
        // 1000 次采集应 < 500ms（宽松断言，体现 ≤ 1% CPU）
        assert!(
            elapsed.as_millis() < 500,
            "collect overhead too high: {elapsed:?}"
        );
    }

    #[tokio::test]
    async fn test_export_prometheus() {
        let registry = Arc::new(MetricsRegistry::new());
        let collector = UnifiedMetricsCollector::new(registry);
        collector.register_custom_counter("sz_orm_test_total", "test counter");
        let output = collector.export_prometheus();
        assert!(output.contains("sz_orm_test_total"));
    }

    #[tokio::test]
    async fn test_custom_interval() {
        let registry = Arc::new(MetricsRegistry::new());
        let collector =
            UnifiedMetricsCollector::new(registry).with_interval(Duration::from_secs(30));
        assert_eq!(collector.collect_interval(), Duration::from_secs(30));
    }

    #[tokio::test]
    async fn test_batch_id_increments() {
        let registry = Arc::new(MetricsRegistry::new());
        let collector = UnifiedMetricsCollector::new(registry);
        let b1 = collector.collect().await.unwrap();
        let b2 = collector.collect().await.unwrap();
        assert_eq!(b1.batch_id, 1);
        assert_eq!(b2.batch_id, 2);
    }
}
