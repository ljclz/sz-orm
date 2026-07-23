//! 采样策略与 Baggage 传播
//!
//! 提供分布式追踪的采样决策与 Baggage 上下文传播能力。
//!
//! ## 采样策略
//!
//! - [`TraceIdRatioSampler`]：基于 trace_id 的确定性采样，保证同一 trace
//!   在所有节点上做出相同的采样决策。
//! - [`ParentBasedSampler`]：遵循父 span 的采样标志，适用于局部采样场景。
//! - [`AlwaysOnSampler`] / [`AlwaysOffSampler`]：全量采样/全量不采样。
//!
//! ## Baggage 传播
//!
//! Baggage 是 W3C 规范定义的跨进程键值对传播机制，用于在请求链路上传递
//! 业务上下文（如 user_id、request_id、locale）。
//!
//! 格式：`baggage: key1=value1,key2=value2`

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// 采样决策
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SamplingDecision {
    /// 采样：该 trace 会被记录与上报
    RecordAndSample,
    /// 不采样：该 trace 不会被记录
    NotRecord,
}

impl SamplingDecision {
    /// 是否采样
    pub fn is_sampled(&self) -> bool {
        matches!(self, SamplingDecision::RecordAndSample)
    }

    /// 转为 W3C traceparent 中的 trace_flags（`01` 表示采样，`00` 表示不采样）
    pub fn as_trace_flags(&self) -> &'static str {
        match self {
            SamplingDecision::RecordAndSample => "01",
            SamplingDecision::NotRecord => "00",
        }
    }
}

/// 采样器 trait：根据 trace_id 与父 span 状态做出采样决策
pub trait Sampler: Send + Sync {
    /// 做出采样决策
    ///
    /// # 参数
    /// - `trace_id`：trace 标识（32 字符 hex）
    /// - `parent_sampled`：父 span 是否已采样（`None` 表示根 span）
    fn should_sample(&self, trace_id: &str, parent_sampled: Option<bool>) -> SamplingDecision;

    /// 采样器名称（用于可观测性）
    fn name(&self) -> &'static str;
}

/// 全量采样器：始终返回 RecordAndSample
#[derive(Debug, Clone, Default)]
pub struct AlwaysOnSampler;

impl AlwaysOnSampler {
    pub fn new() -> Self {
        Self
    }
}

impl Sampler for AlwaysOnSampler {
    fn should_sample(&self, _trace_id: &str, _parent_sampled: Option<bool>) -> SamplingDecision {
        SamplingDecision::RecordAndSample
    }

    fn name(&self) -> &'static str {
        "always_on"
    }
}

/// 全量不采样器：始终返回 NotRecord
#[derive(Debug, Clone, Default)]
pub struct AlwaysOffSampler;

impl AlwaysOffSampler {
    pub fn new() -> Self {
        Self
    }
}

impl Sampler for AlwaysOffSampler {
    fn should_sample(&self, _trace_id: &str, _parent_sampled: Option<bool>) -> SamplingDecision {
        SamplingDecision::NotRecord
    }

    fn name(&self) -> &'static str {
        "always_off"
    }
}

/// 基于 trace_id 比例的确定性采样器。
///
/// 使用 trace_id 的哈希值与阈值比较，保证同一 trace_id 在所有节点上
/// 做出相同的采样决策（无需共享状态）。
///
/// 比例范围为 0.0..=1.0：
/// - 0.0：不采样任何 trace
/// - 1.0：采样所有 trace
/// - 0.5：采样约 50% 的 trace
pub struct TraceIdRatioSampler {
    /// 采样比例（0.0..=1.0）
    ratio: f64,
    /// 阈值 = ratio * u64::MAX
    threshold: u64,
    /// 总采样次数（用于统计）
    total: AtomicU64,
    /// 被采样的次数
    sampled: AtomicU64,
}

impl TraceIdRatioSampler {
    /// 创建比例采样器
    ///
    /// # Panics
    /// 当 ratio 不在 [0.0, 1.0] 范围内时 panic。
    pub fn new(ratio: f64) -> Self {
        assert!(
            (0.0..=1.0).contains(&ratio),
            "sampling ratio must be in [0.0, 1.0], got {ratio}"
        );
        Self {
            ratio,
            threshold: (ratio * u64::MAX as f64) as u64,
            total: AtomicU64::new(0),
            sampled: AtomicU64::new(0),
        }
    }

    /// 采样比例
    pub fn ratio(&self) -> f64 {
        self.ratio
    }

    /// 总采样决策次数
    pub fn total_decisions(&self) -> u64 {
        self.total.load(Ordering::Relaxed)
    }

    /// 被采样的次数
    pub fn sampled_count(&self) -> u64 {
        self.sampled.load(Ordering::Relaxed)
    }

    /// 实际采样率（sampled / total），总次数为 0 时返回 0.0
    pub fn actual_rate(&self) -> f64 {
        let total = self.total_decisions();
        if total == 0 {
            return 0.0;
        }
        self.sampled_count() as f64 / total as f64
    }

    /// 对 trace_id 做确定性哈希，返回 u64 值。
    ///
    /// 使用 FNV-1a 64-bit 哈希 + 末级 avalanche 混合（xorshift-mix），
    /// 保证同一输入始终产生相同输出，且对相近输入（如顺序递增的 hex 字符串）
    /// 也能均匀分散到 u64 值域，避免比例采样时出现严重偏差。
    fn hash_trace_id(trace_id: &str) -> u64 {
        // FNV-1a 64-bit
        const FNV_OFFSET: u64 = 14695981039346656037;
        const FNV_PRIME: u64 = 1099511628211;

        let mut hash = FNV_OFFSET;
        for byte in trace_id.as_bytes() {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(FNV_PRIME);
        }

        // Avalanche finalizer（基于 xxHash3 的尾混合思想）：
        // 将高位与低位充分混合，使输出在 u64 域内均匀分布。
        hash ^= hash >> 33;
        hash = hash.wrapping_mul(0xff51_afd7_ed55_8ccd);
        hash ^= hash >> 33;
        hash = hash.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
        hash ^= hash >> 33;
        hash
    }
}

impl Sampler for TraceIdRatioSampler {
    fn should_sample(&self, trace_id: &str, _parent_sampled: Option<bool>) -> SamplingDecision {
        self.total.fetch_add(1, Ordering::Relaxed);
        let hash = Self::hash_trace_id(trace_id);
        if hash <= self.threshold {
            self.sampled.fetch_add(1, Ordering::Relaxed);
            SamplingDecision::RecordAndSample
        } else {
            SamplingDecision::NotRecord
        }
    }

    fn name(&self) -> &'static str {
        "trace_id_ratio"
    }
}

/// 基于父 span 的采样器。
///
/// 采样策略：
/// - 根 span（parent_sampled = None）：使用内部 root sampler 决策
/// - 有父 span：遵循父 span 的采样标志
pub struct ParentBasedSampler {
    /// 根 span 采样器
    root: Box<dyn Sampler>,
}

impl ParentBasedSampler {
    /// 创建基于父 span 的采样器
    ///
    /// # 参数
    /// - `root`：根 span 使用的采样器（通常为 TraceIdRatioSampler）
    pub fn new(root: Box<dyn Sampler>) -> Self {
        Self { root }
    }

    /// 使用 AlwaysOn 作为根采样器的便捷构造
    pub fn always_on_root() -> Self {
        Self::new(Box::new(AlwaysOnSampler::new()))
    }

    /// 使用 TraceIdRatio 作为根采样器的便捷构造
    pub fn ratio_root(ratio: f64) -> Self {
        Self::new(Box::new(TraceIdRatioSampler::new(ratio)))
    }
}

impl Sampler for ParentBasedSampler {
    fn should_sample(&self, trace_id: &str, parent_sampled: Option<bool>) -> SamplingDecision {
        match parent_sampled {
            None => self.root.should_sample(trace_id, None),
            Some(sampled) => {
                if sampled {
                    SamplingDecision::RecordAndSample
                } else {
                    SamplingDecision::NotRecord
                }
            }
        }
    }

    fn name(&self) -> &'static str {
        "parent_based"
    }
}

// ============================================================================
// Baggage 传播
// ============================================================================

/// Baggage：跨进程键值对上下文。
///
/// 遵循 W3C Baggage 规范，通过 `baggage` HTTP header 传播。
/// 格式：`key1=value1,key2=value2`
#[derive(Debug, Clone, Default)]
pub struct Baggage {
    entries: HashMap<String, String>,
}

impl Baggage {
    /// 创建空 Baggage
    pub fn new() -> Self {
        Self::default()
    }

    /// 从键值对列表创建
    pub fn from_pairs(pairs: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>) -> Self {
        let mut baggage = Self::new();
        for (k, v) in pairs {
            baggage.set(k, v);
        }
        baggage
    }

    /// 设置键值对（覆盖已有值）
    pub fn set(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.entries.insert(key.into(), value.into());
    }

    /// 获取值
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries.get(key).map(|s| s.as_str())
    }

    /// 移除键
    pub fn remove(&mut self, key: &str) -> Option<String> {
        self.entries.remove(key)
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 条目数量
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 获取所有键
    pub fn keys(&self) -> Vec<&str> {
        self.entries.keys().map(|s| s.as_str()).collect()
    }

    /// 序列化为 W3C baggage header 值
    ///
    /// 格式：`key1=value1,key2=value2`
    /// 键值按字母序排列，保证输出确定性。
    pub fn to_header(&self) -> String {
        let mut pairs: Vec<(String, String)> =
            self.entries.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        pairs.sort_by(|a, b| a.0.cmp(&b.0));
        pairs
            .into_iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join(",")
    }

    /// 从 W3C baggage header 值解析
    ///
    /// 格式：`key1=value1,key2=value2`
    /// 解析时忽略空白条目和格式不正确的条目（无 `=`、空 key、空 value）。
    pub fn from_header(header: &str) -> Self {
        let mut baggage = Self::new();
        for entry in header.split(',') {
            let entry = entry.trim();
            if entry.is_empty() {
                continue;
            }
            if let Some(eq_pos) = entry.find('=') {
                let key = entry[..eq_pos].trim().to_string();
                let value = entry[eq_pos + 1..].trim().to_string();
                // 键和值都必须非空，否则视为格式错误并忽略。
                if !key.is_empty() && !value.is_empty() {
                    baggage.entries.insert(key, value);
                }
            }
        }
        baggage
    }

    /// 合并另一个 Baggage（后者覆盖前者）
    pub fn merge(&mut self, other: &Baggage) {
        for (k, v) in &other.entries {
            self.entries.insert(k.clone(), v.clone());
        }
    }

    /// 清空所有条目
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

/// Baggage 传播器：在 HTTP header 与 Baggage 对象之间转换
pub struct BaggagePropagator;

impl BaggagePropagator {
    /// 创建传播器
    pub fn new() -> Self {
        Self
    }

    /// 将 Baggage 注入到 header map
    pub fn inject(&self, baggage: &Baggage, headers: &mut HashMap<String, String>) {
        let header_value = baggage.to_header();
        if !header_value.is_empty() {
            headers.insert("baggage".to_string(), header_value);
        }
    }

    /// 从 header map 提取 Baggage
    pub fn extract(&self, headers: &HashMap<String, String>) -> Baggage {
        headers
            .get("baggage")
            .map(|h| Baggage::from_header(h))
            .unwrap_or_default()
    }
}

impl Default for BaggagePropagator {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// 批量导出
// ============================================================================

/// 批量 Span 导出器配置
#[derive(Debug, Clone)]
pub struct BatchConfig {
    /// 批次最大大小（达到后立即导出）
    pub max_batch_size: usize,
    /// 导出间隔（毫秒）
    pub export_interval_ms: u64,
    /// 最大队列长度（超出时丢弃最旧的 span）
    pub max_queue_size: usize,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            max_batch_size: 512,
            export_interval_ms: 5000,
            max_queue_size: 2048,
        }
    }
}

/// 批量 Span 导出器（内存模拟）。
///
/// 收集 span 到队列，达到批次大小或导出间隔时批量导出。
/// 实际实现中会通过 OTLP/HTTP 发送到 Collector。
pub struct BatchSpanExporter {
    config: BatchConfig,
    /// 待导出的 span 队列
    queue: Arc<std::sync::RwLock<Vec<crate::Span>>>,
    /// 已导出的 span 批次
    exported: Arc<std::sync::RwLock<Vec<Vec<crate::Span>>>>,
    /// 被丢弃的 span 数量
    dropped: AtomicU64,
}

impl BatchSpanExporter {
    /// 创建批量导出器
    pub fn new(config: BatchConfig) -> Self {
        Self {
            config,
            queue: Arc::new(std::sync::RwLock::new(Vec::new())),
            exported: Arc::new(std::sync::RwLock::new(Vec::new())),
            dropped: AtomicU64::new(0),
        }
    }

    /// 将 span 加入导出队列
    pub fn enqueue(&self, span: crate::Span) {
        let mut queue = self.queue.write().unwrap();
        if queue.len() >= self.config.max_queue_size {
            // 队列已满，丢弃最旧的 span
            queue.remove(0);
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
        queue.push(span);
    }

    /// 尝试导出一批 span。
    ///
    /// 当队列长度 >= max_batch_size 时导出整批，否则不导出。
    /// 返回导出的 span 数量。
    pub fn flush_batch(&self) -> usize {
        let mut queue = self.queue.write().unwrap();
        if queue.len() < self.config.max_batch_size {
            return 0;
        }
        let batch: Vec<crate::Span> = queue.drain(..self.config.max_batch_size).collect();
        let count = batch.len();
        let mut exported = self.exported.write().unwrap();
        exported.push(batch);
        count
    }

    /// 强制导出所有排队中的 span，不论批次大小。
    pub fn flush_all(&self) -> usize {
        let mut queue = self.queue.write().unwrap();
        if queue.is_empty() {
            return 0;
        }
        let batch: Vec<crate::Span> = queue.drain(..).collect();
        let count = batch.len();
        let mut exported = self.exported.write().unwrap();
        exported.push(batch);
        count
    }

    /// 获取已导出的批次数
    pub fn exported_batch_count(&self) -> usize {
        self.exported.read().unwrap().len()
    }

    /// 获取已导出的 span 总数
    pub fn exported_span_count(&self) -> usize {
        self.exported
            .read()
            .unwrap()
            .iter()
            .map(|b| b.len())
            .sum()
    }

    /// 获取当前队列长度
    pub fn queue_len(&self) -> usize {
        self.queue.read().unwrap().len()
    }

    /// 获取被丢弃的 span 数量
    pub fn dropped_count(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// 清空所有状态（队列与已导出记录）
    pub fn clear(&self) {
        self.queue.write().unwrap().clear();
        self.exported.write().unwrap().clear();
        self.dropped.store(0, Ordering::Relaxed);
    }

    /// 获取配置引用
    pub fn config(&self) -> &BatchConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===================== SamplingDecision 测试 =====================

    #[test]
    fn test_sampling_decision_is_sampled() {
        assert!(SamplingDecision::RecordAndSample.is_sampled());
        assert!(!SamplingDecision::NotRecord.is_sampled());
    }

    #[test]
    fn test_sampling_decision_as_trace_flags() {
        assert_eq!(SamplingDecision::RecordAndSample.as_trace_flags(), "01");
        assert_eq!(SamplingDecision::NotRecord.as_trace_flags(), "00");
    }

    #[test]
    fn test_sampling_decision_equality() {
        assert_eq!(SamplingDecision::RecordAndSample, SamplingDecision::RecordAndSample);
        assert_ne!(SamplingDecision::RecordAndSample, SamplingDecision::NotRecord);
    }

    // ===================== AlwaysOnSampler 测试 =====================

    #[test]
    fn test_always_on_sampler_returns_sampled() {
        let sampler = AlwaysOnSampler::new();
        let decision = sampler.should_sample("trace123", None);
        assert_eq!(decision, SamplingDecision::RecordAndSample);
    }

    #[test]
    fn test_always_on_sampler_ignores_parent() {
        let sampler = AlwaysOnSampler::new();
        assert!(sampler.should_sample("t", Some(false)).is_sampled());
        assert!(sampler.should_sample("t", Some(true)).is_sampled());
    }

    #[test]
    fn test_always_on_sampler_name() {
        let sampler = AlwaysOnSampler::new();
        assert_eq!(sampler.name(), "always_on");
    }

    // ===================== AlwaysOffSampler 测试 =====================

    #[test]
    fn test_always_off_sampler_returns_not_sampled() {
        let sampler = AlwaysOffSampler::new();
        let decision = sampler.should_sample("trace123", None);
        assert_eq!(decision, SamplingDecision::NotRecord);
    }

    #[test]
    fn test_always_off_sampler_name() {
        let sampler = AlwaysOffSampler::new();
        assert_eq!(sampler.name(), "always_off");
    }

    // ===================== TraceIdRatioSampler 测试 =====================

    #[test]
    fn test_trace_id_ratio_sampler_full_sampling() {
        let sampler = TraceIdRatioSampler::new(1.0);
        // ratio=1.0 时所有 trace 都应被采样
        for i in 0..100 {
            let trace_id = format!("{:032x}", i);
            assert!(sampler.should_sample(&trace_id, None).is_sampled());
        }
        assert_eq!(sampler.sampled_count(), 100);
        assert_eq!(sampler.total_decisions(), 100);
    }

    #[test]
    fn test_trace_id_ratio_sampler_zero_sampling() {
        let sampler = TraceIdRatioSampler::new(0.0);
        // ratio=0.0 时不应采样任何 trace
        for i in 0..100 {
            let trace_id = format!("{:032x}", i);
            assert!(!sampler.should_sample(&trace_id, None).is_sampled());
        }
        assert_eq!(sampler.sampled_count(), 0);
    }

    #[test]
    fn test_trace_id_ratio_sampler_deterministic() {
        let sampler = TraceIdRatioSampler::new(0.5);
        // 同一 trace_id 多次采样应得到相同结果
        let trace_id = "abcdef0123456789abcdef0123456789";
        let d1 = sampler.should_sample(trace_id, None);
        let d2 = sampler.should_sample(trace_id, None);
        let d3 = sampler.should_sample(trace_id, None);
        assert_eq!(d1, d2);
        assert_eq!(d2, d3);
    }

    #[test]
    fn test_trace_id_ratio_sampler_half_ratio_approximate() {
        let sampler = TraceIdRatioSampler::new(0.5);
        // 大量样本下采样率应接近 50%
        for i in 0..10000 {
            let trace_id = format!("{:032x}", i);
            sampler.should_sample(&trace_id, None);
        }
        let rate = sampler.actual_rate();
        assert!((rate - 0.5).abs() < 0.05, "expected ~0.5, got {rate}");
    }

    #[test]
    fn test_trace_id_ratio_sampler_stats() {
        let sampler = TraceIdRatioSampler::new(0.3);
        for i in 0..1000 {
            let trace_id = format!("{:032x}", i);
            sampler.should_sample(&trace_id, None);
        }
        assert_eq!(sampler.total_decisions(), 1000);
        assert!(sampler.sampled_count() > 0);
        let rate = sampler.actual_rate();
        assert!((rate - 0.3).abs() < 0.05, "expected ~0.3, got {rate}");
    }

    #[test]
    fn test_trace_id_ratio_sampler_name() {
        let sampler = TraceIdRatioSampler::new(1.0);
        assert_eq!(sampler.name(), "trace_id_ratio");
    }

    #[test]
    fn test_trace_id_ratio_sampler_ratio_accessor() {
        let sampler = TraceIdRatioSampler::new(0.75);
        assert!((sampler.ratio() - 0.75).abs() < 1e-9);
    }

    #[test]
    #[should_panic(expected = "sampling ratio must be in [0.0, 1.0]")]
    fn test_trace_id_ratio_sampler_invalid_ratio_high() {
        TraceIdRatioSampler::new(1.5);
    }

    #[test]
    #[should_panic(expected = "sampling ratio must be in [0.0, 1.0]")]
    fn test_trace_id_ratio_sampler_invalid_ratio_negative() {
        TraceIdRatioSampler::new(-0.1);
    }

    // ===================== ParentBasedSampler 测试 =====================

    #[test]
    fn test_parent_based_root_uses_inner_sampler() {
        let sampler = ParentBasedSampler::ratio_root(1.0);
        // 根 span -> 使用 TraceIdRatio(1.0) -> 采样
        assert!(sampler.should_sample("trace1", None).is_sampled());
    }

    #[test]
    fn test_parent_based_follows_sampled_parent() {
        let sampler = ParentBasedSampler::always_on_root();
        // 父 span 已采样 -> 子 span 也采样
        assert!(sampler.should_sample("t", Some(true)).is_sampled());
    }

    #[test]
    fn test_parent_based_follows_unsampled_parent() {
        let sampler = ParentBasedSampler::always_on_root();
        // 父 span 未采样 -> 子 span 也不采样（即使 root 是 AlwaysOn）
        assert!(!sampler.should_sample("t", Some(false)).is_sampled());
    }

    #[test]
    fn test_parent_based_name() {
        let sampler = ParentBasedSampler::always_on_root();
        assert_eq!(sampler.name(), "parent_based");
    }

    // ===================== Baggage 测试 =====================

    #[test]
    fn test_baggage_new_empty() {
        let b = Baggage::new();
        assert!(b.is_empty());
        assert_eq!(b.len(), 0);
    }

    #[test]
    fn test_baggage_set_and_get() {
        let mut b = Baggage::new();
        b.set("user_id", "12345");
        assert_eq!(b.get("user_id"), Some("12345"));
        assert_eq!(b.get("missing"), None);
    }

    #[test]
    fn test_baggage_set_overwrites() {
        let mut b = Baggage::new();
        b.set("key", "v1");
        b.set("key", "v2");
        assert_eq!(b.get("key"), Some("v2"));
        assert_eq!(b.len(), 1);
    }

    #[test]
    fn test_baggage_remove() {
        let mut b = Baggage::new();
        b.set("key", "value");
        assert_eq!(b.remove("key"), Some("value".to_string()));
        assert!(b.is_empty());
        assert_eq!(b.remove("key"), None);
    }

    #[test]
    fn test_baggage_from_pairs() {
        let b = Baggage::from_pairs([("a", "1"), ("b", "2")]);
        assert_eq!(b.len(), 2);
        assert_eq!(b.get("a"), Some("1"));
        assert_eq!(b.get("b"), Some("2"));
    }

    #[test]
    fn test_baggage_to_header_single() {
        let mut b = Baggage::new();
        b.set("key", "value");
        assert_eq!(b.to_header(), "key=value");
    }

    #[test]
    fn test_baggage_to_header_multiple_sorted() {
        let mut b = Baggage::new();
        b.set("zebra", "1");
        b.set("alpha", "2");
        // 输出按 key 字母序排列
        assert_eq!(b.to_header(), "alpha=2,zebra=1");
    }

    #[test]
    fn test_baggage_to_header_empty() {
        let b = Baggage::new();
        assert_eq!(b.to_header(), "");
    }

    #[test]
    fn test_baggage_from_header_single() {
        let b = Baggage::from_header("key=value");
        assert_eq!(b.get("key"), Some("value"));
    }

    #[test]
    fn test_baggage_from_header_multiple() {
        let b = Baggage::from_header("a=1,b=2,c=3");
        assert_eq!(b.len(), 3);
        assert_eq!(b.get("a"), Some("1"));
        assert_eq!(b.get("b"), Some("2"));
        assert_eq!(b.get("c"), Some("3"));
    }

    #[test]
    fn test_baggage_from_header_with_spaces() {
        let b = Baggage::from_header(" key = value1 , b = value2 ");
        assert_eq!(b.get("key"), Some("value1"));
        assert_eq!(b.get("b"), Some("value2"));
    }

    #[test]
    fn test_baggage_from_header_empty() {
        let b = Baggage::from_header("");
        assert!(b.is_empty());
    }

    #[test]
    fn test_baggage_from_header_ignores_malformed() {
        let b = Baggage::from_header("valid=1,invalid,=nokey,novalue=,good=2");
        assert_eq!(b.get("valid"), Some("1"));
        assert_eq!(b.get("good"), Some("2"));
        assert_eq!(b.len(), 2);
    }

    #[test]
    fn test_baggage_roundtrip() {
        let mut original = Baggage::new();
        original.set("user_id", "12345");
        original.set("request_id", "abc");
        original.set("locale", "zh-CN");

        let header = original.to_header();
        let parsed = Baggage::from_header(&header);

        assert_eq!(parsed.len(), original.len());
        for key in original.keys() {
            assert_eq!(parsed.get(key), original.get(key));
        }
    }

    #[test]
    fn test_baggage_merge() {
        let mut b1 = Baggage::new();
        b1.set("a", "1");
        b1.set("b", "2");

        let mut b2 = Baggage::new();
        b2.set("b", "override");
        b2.set("c", "3");

        b1.merge(&b2);
        assert_eq!(b1.get("a"), Some("1"));
        assert_eq!(b1.get("b"), Some("override"));
        assert_eq!(b1.get("c"), Some("3"));
    }

    #[test]
    fn test_baggage_clear() {
        let mut b = Baggage::new();
        b.set("a", "1");
        b.set("b", "2");
        b.clear();
        assert!(b.is_empty());
    }

    #[test]
    fn test_baggage_keys() {
        let mut b = Baggage::new();
        b.set("x", "1");
        b.set("y", "2");
        let mut keys = b.keys();
        keys.sort();
        assert_eq!(keys, vec!["x", "y"]);
    }

    // ===================== BaggagePropagator 测试 =====================

    #[test]
    fn test_propagator_inject_and_extract() {
        let propagator = BaggagePropagator::new();
        let mut baggage = Baggage::new();
        baggage.set("user_id", "123");
        baggage.set("locale", "en");

        let mut headers = HashMap::new();
        propagator.inject(&baggage, &mut headers);

        assert!(headers.contains_key("baggage"));

        let extracted = propagator.extract(&headers);
        assert_eq!(extracted.get("user_id"), Some("123"));
        assert_eq!(extracted.get("locale"), Some("en"));
    }

    #[test]
    fn test_propagator_extract_empty_headers() {
        let propagator = BaggagePropagator::new();
        let headers = HashMap::new();
        let baggage = propagator.extract(&headers);
        assert!(baggage.is_empty());
    }

    #[test]
    fn test_propagator_inject_empty_baggage() {
        let propagator = BaggagePropagator::new();
        let baggage = Baggage::new();
        let mut headers = HashMap::new();
        propagator.inject(&baggage, &mut headers);
        // 空 baggage 不应注入 header
        assert!(!headers.contains_key("baggage"));
    }

    #[test]
    fn test_propagator_roundtrip_multiple_entries() {
        let propagator = BaggagePropagator::new();
        let mut original = Baggage::new();
        original.set("a", "1");
        original.set("b", "2");
        original.set("c", "3");

        let mut headers = HashMap::new();
        propagator.inject(&original, &mut headers);
        let extracted = propagator.extract(&headers);

        assert_eq!(extracted.len(), 3);
        assert_eq!(extracted.get("a"), Some("1"));
        assert_eq!(extracted.get("b"), Some("2"));
        assert_eq!(extracted.get("c"), Some("3"));
    }

    // ===================== BatchSpanExporter 测试 =====================

    fn make_span(id: &str) -> crate::Span {
        crate::Span::new("trace", id, "operation")
    }

    #[test]
    fn test_batch_exporter_new_empty() {
        let exporter = BatchSpanExporter::new(BatchConfig::default());
        assert_eq!(exporter.queue_len(), 0);
        assert_eq!(exporter.exported_batch_count(), 0);
        assert_eq!(exporter.exported_span_count(), 0);
        assert_eq!(exporter.dropped_count(), 0);
    }

    #[test]
    fn test_batch_exporter_enqueue() {
        let exporter = BatchSpanExporter::new(BatchConfig::default());
        exporter.enqueue(make_span("span1"));
        exporter.enqueue(make_span("span2"));
        assert_eq!(exporter.queue_len(), 2);
    }

    #[test]
    fn test_batch_exporter_flush_batch_below_threshold() {
        let config = BatchConfig {
            max_batch_size: 10,
            ..Default::default()
        };
        let exporter = BatchSpanExporter::new(config);
        exporter.enqueue(make_span("s1"));
        exporter.enqueue(make_span("s2"));

        // 队列未达到批次大小，不应导出
        let exported = exporter.flush_batch();
        assert_eq!(exported, 0);
        assert_eq!(exporter.queue_len(), 2);
    }

    #[test]
    fn test_batch_exporter_flush_batch_at_threshold() {
        let config = BatchConfig {
            max_batch_size: 3,
            ..Default::default()
        };
        let exporter = BatchSpanExporter::new(config);
        exporter.enqueue(make_span("s1"));
        exporter.enqueue(make_span("s2"));
        exporter.enqueue(make_span("s3"));

        let exported = exporter.flush_batch();
        assert_eq!(exported, 3);
        assert_eq!(exporter.queue_len(), 0);
        assert_eq!(exporter.exported_batch_count(), 1);
        assert_eq!(exporter.exported_span_count(), 3);
    }

    #[test]
    fn test_batch_exporter_flush_all() {
        let exporter = BatchSpanExporter::new(BatchConfig::default());
        exporter.enqueue(make_span("s1"));
        exporter.enqueue(make_span("s2"));

        let exported = exporter.flush_all();
        assert_eq!(exported, 2);
        assert_eq!(exporter.queue_len(), 0);
        assert_eq!(exporter.exported_batch_count(), 1);
    }

    #[test]
    fn test_batch_exporter_flush_all_empty() {
        let exporter = BatchSpanExporter::new(BatchConfig::default());
        let exported = exporter.flush_all();
        assert_eq!(exported, 0);
    }

    #[test]
    fn test_batch_exporter_drops_when_queue_full() {
        let config = BatchConfig {
            max_batch_size: 100, // 大批次，不触发自动导出
            max_queue_size: 3,
            ..Default::default()
        };
        let exporter = BatchSpanExporter::new(config);
        exporter.enqueue(make_span("s1"));
        exporter.enqueue(make_span("s2"));
        exporter.enqueue(make_span("s3"));
        // 队列已满，第 4 个应导致丢弃最旧的
        exporter.enqueue(make_span("s4"));

        assert_eq!(exporter.queue_len(), 3);
        assert_eq!(exporter.dropped_count(), 1);
    }

    #[test]
    fn test_batch_exporter_multiple_batches() {
        let config = BatchConfig {
            max_batch_size: 2,
            ..Default::default()
        };
        let exporter = BatchSpanExporter::new(config);

        for i in 0..6 {
            exporter.enqueue(make_span(&format!("s{i}")));
        }

        // 6 个 span，batch_size=2，应导出 3 批
        let mut total_exported = 0;
        loop {
            let n = exporter.flush_batch();
            if n == 0 {
                break;
            }
            total_exported += n;
        }
        assert_eq!(total_exported, 6);
        assert_eq!(exporter.exported_batch_count(), 3);
    }

    #[test]
    fn test_batch_exporter_clear() {
        let exporter = BatchSpanExporter::new(BatchConfig::default());
        exporter.enqueue(make_span("s1"));
        exporter.flush_all();

        exporter.clear();
        assert_eq!(exporter.queue_len(), 0);
        assert_eq!(exporter.exported_batch_count(), 0);
        assert_eq!(exporter.dropped_count(), 0);
    }

    #[test]
    fn test_batch_config_default() {
        let config = BatchConfig::default();
        assert_eq!(config.max_batch_size, 512);
        assert_eq!(config.export_interval_ms, 5000);
        assert_eq!(config.max_queue_size, 2048);
    }

    #[test]
    fn test_batch_exporter_config_accessor() {
        let config = BatchConfig {
            max_batch_size: 42,
            ..Default::default()
        };
        let exporter = BatchSpanExporter::new(config);
        assert_eq!(exporter.config().max_batch_size, 42);
    }
}
