//! 告警输出模块：Alert 事件 + 告警去重（冷却期）+ 订阅 API
//!
//! 告警去重使用 `HashMap<AnomalyType, u64>` 记录每种异常最后告警时间，
//! 冷却期内（默认 5 分钟）不重复告警。订阅回调使用 `catch_unwind` 隔离 panic。

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::config::ConfigStore;

/// 异常类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AnomalyType {
    /// 慢查询突增
    SlowQuerySpike,
    /// 错误率突增
    ErrorRateSpike,
    /// 连接池耗尽
    PoolExhaustion,
    /// 偏离基线
    BaselineDrift,
}

impl AnomalyType {
    /// 转为字符串标识
    pub fn as_str(&self) -> &'static str {
        match self {
            AnomalyType::SlowQuerySpike => "slow_query_spike",
            AnomalyType::ErrorRateSpike => "error_rate_spike",
            AnomalyType::PoolExhaustion => "pool_exhaustion",
            AnomalyType::BaselineDrift => "baseline_drift",
        }
    }
}

/// 严重级别
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Severity {
    /// 信息
    Info,
    /// 警告
    Warn,
    /// 严重
    Critical,
}

impl Severity {
    /// 转为字符串标识
    pub fn as_str(&self) -> &'static str {
        match self {
            Severity::Info => "info",
            Severity::Warn => "warn",
            Severity::Critical => "critical",
        }
    }

    /// 数值化（用于排序/比较）
    pub fn level(&self) -> u8 {
        match self {
            Severity::Info => 0,
            Severity::Warn => 1,
            Severity::Critical => 2,
        }
    }
}

/// 基线信息
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Baseline {
    /// 均值
    pub mean: f64,
    /// 标准差
    pub stddev: f64,
    /// 样本数
    pub sample_count: usize,
}

/// 告警事件
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Alert {
    /// 异常类型
    pub anomaly_type: AnomalyType,
    /// 严重级别
    pub severity: Severity,
    /// 时间戳（毫秒）
    pub timestamp: u64,
    /// 指标值
    pub metric_value: f64,
    /// 阈值
    pub threshold: f64,
    /// 基线（可选）
    pub baseline: Option<Baseline>,
    /// 建议操作
    pub suggestion: String,
    /// SQL 摘要（可选，已脱敏）
    pub sql_summary: Option<String>,
}

/// 订阅 ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SubscriptionId(pub u64);

/// 告警回调类型
pub type AlertCallback = Arc<dyn Fn(&Alert) + Send + Sync>;

/// 告警去重器
///
/// 使用 `HashMap<AnomalyType, u64>` 记录每种异常最后告警时间，
/// 冷却期内不重复告警。
#[derive(Debug)]
pub struct AlertDedup {
    last_alert_time: RwLock<HashMap<AnomalyType, u64>>,
    cooldown_ms: u64,
    /// 冷却期内被抑制的计数
    suppressed_count: AtomicU64,
}

impl AlertDedup {
    /// 创建告警去重器
    pub fn new(cooldown_ms: u64) -> Self {
        Self {
            last_alert_time: RwLock::new(HashMap::new()),
            cooldown_ms,
            suppressed_count: AtomicU64::new(0),
        }
    }

    /// 检查是否可以告警（不在冷却期内）
    ///
    /// 返回 `true` 表示可以告警，`false` 表示在冷却期内被抑制。
    /// 如果可以告警，会更新最后告警时间。
    pub fn should_alert(&self, anomaly_type: AnomalyType, now_ms: u64) -> bool {
        let mut last_times = self.last_alert_time.write();
        if let Some(&last) = last_times.get(&anomaly_type) {
            if now_ms.saturating_sub(last) < self.cooldown_ms {
                self.suppressed_count.fetch_add(1, Ordering::Relaxed);
                return false;
            }
        }
        last_times.insert(anomaly_type, now_ms);
        true
    }

    /// 被抑制的告警数
    pub fn suppressed_count(&self) -> u64 {
        self.suppressed_count.load(Ordering::Relaxed)
    }

    /// 重置去重状态
    pub fn reset(&self) {
        self.last_alert_time.write().clear();
        self.suppressed_count.store(0, Ordering::Relaxed);
    }

    /// 获取某类型最后告警时间
    pub fn last_alert_time(&self, anomaly_type: AnomalyType) -> Option<u64> {
        self.last_alert_time.read().get(&anomaly_type).copied()
    }

    /// 冷却期（毫秒）
    pub fn cooldown_ms(&self) -> u64 {
        self.cooldown_ms
    }
}

/// 告警输出器
///
/// 集成去重 + 订阅回调通知。回调 panic 自动隔离（catch_unwind）。
pub struct AlertEmitter {
    dedup: AlertDedup,
    subscribers: RwLock<Vec<(SubscriptionId, AlertCallback)>>,
    next_subscription_id: AtomicU64,
    /// 累计发出的告警数
    emitted_count: AtomicU64,
    /// 历史告警（用于报告导出）
    history: RwLock<Vec<Alert>>,
}

impl AlertEmitter {
    /// 创建告警输出器
    pub fn new(cooldown_ms: u64) -> Self {
        Self {
            dedup: AlertDedup::new(cooldown_ms),
            subscribers: RwLock::new(Vec::new()),
            next_subscription_id: AtomicU64::new(1),
            emitted_count: AtomicU64::new(0),
            history: RwLock::new(Vec::new()),
        }
    }

    /// 从配置存储创建
    pub fn from_config(config_store: &ConfigStore) -> Self {
        let config = config_store.get();
        Self::new(config.alert_cooldown.as_millis() as u64)
    }

    /// 订阅告警
    ///
    /// 返回订阅 ID，可用于取消订阅。异常发生时回调被调用。
    /// 回调 panic 自动隔离，不影响检测模块。
    pub fn subscribe(&self, callback: AlertCallback) -> SubscriptionId {
        let id = SubscriptionId(self.next_subscription_id.fetch_add(1, Ordering::Relaxed));
        self.subscribers.write().push((id, callback));
        id
    }

    /// 取消订阅
    pub fn unsubscribe(&self, id: SubscriptionId) -> bool {
        let mut subs = self.subscribers.write();
        let before = subs.len();
        subs.retain(|(sub_id, _)| *sub_id != id);
        subs.len() < before
    }

    /// 输出告警
    ///
    /// 流程：去重检查 → 通知订阅者 → 记录历史
    /// 返回 `Some(alert)` 表示已发出，`None` 表示被去重抑制。
    pub fn emit(&self, mut alert: Alert) -> Option<Alert> {
        let now = alert.timestamp;
        if !self.dedup.should_alert(alert.anomaly_type, now) {
            return None;
        }
        self.emitted_count.fetch_add(1, Ordering::Relaxed);
        self.notify_subscribers(&alert);
        alert.sql_summary = alert.sql_summary.clone();
        self.history.write().push(alert.clone());
        Some(alert)
    }

    /// 通知订阅者（panic 隔离）
    fn notify_subscribers(&self, alert: &Alert) {
        let subs = self.subscribers.read();
        for (_, callback) in subs.iter() {
            // catch_unwind 隔离回调 panic
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| callback(alert)));
        }
    }

    /// 累计发出的告警数
    pub fn emitted_count(&self) -> u64 {
        self.emitted_count.load(Ordering::Relaxed)
    }

    /// 被抑制的告警数
    pub fn suppressed_count(&self) -> u64 {
        self.dedup.suppressed_count()
    }

    /// 当前订阅者数
    pub fn subscriber_count(&self) -> usize {
        self.subscribers.read().len()
    }

    /// 获取历史告警快照
    pub fn history(&self) -> Vec<Alert> {
        self.history.read().clone()
    }

    /// 获取指定时间范围内的历史告警
    pub fn history_in_range(&self, start_ms: u64, end_ms: u64) -> Vec<Alert> {
        self.history
            .read()
            .iter()
            .filter(|a| a.timestamp >= start_ms && a.timestamp <= end_ms)
            .cloned()
            .collect()
    }

    /// 清空历史告警
    pub fn clear_history(&self) {
        self.history.write().clear();
    }

    /// 重置去重状态
    pub fn reset_dedup(&self) {
        self.dedup.reset();
    }

    /// 冷却期（毫秒）
    pub fn cooldown_ms(&self) -> u64 {
        self.dedup.cooldown_ms()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_alert(anomaly_type: AnomalyType, timestamp: u64) -> Alert {
        Alert {
            anomaly_type,
            severity: Severity::Warn,
            timestamp,
            metric_value: 100.0,
            threshold: 50.0,
            baseline: None,
            suggestion: "test suggestion".to_string(),
            sql_summary: None,
        }
    }

    #[test]
    fn test_anomaly_type_as_str() {
        assert_eq!(AnomalyType::SlowQuerySpike.as_str(), "slow_query_spike");
        assert_eq!(AnomalyType::ErrorRateSpike.as_str(), "error_rate_spike");
        assert_eq!(AnomalyType::PoolExhaustion.as_str(), "pool_exhaustion");
        assert_eq!(AnomalyType::BaselineDrift.as_str(), "baseline_drift");
    }

    #[test]
    fn test_severity_level() {
        assert_eq!(Severity::Info.level(), 0);
        assert_eq!(Severity::Warn.level(), 1);
        assert_eq!(Severity::Critical.level(), 2);
    }

    #[test]
    fn test_alert_dedup_within_cooldown() {
        let dedup = AlertDedup::new(60_000); // 1 分钟
        assert!(dedup.should_alert(AnomalyType::SlowQuerySpike, 1000));
        // 冷却期内不重复
        assert!(!dedup.should_alert(AnomalyType::SlowQuerySpike, 30_000));
        assert_eq!(dedup.suppressed_count(), 1);
    }

    #[test]
    fn test_alert_dedup_after_cooldown() {
        let dedup = AlertDedup::new(60_000);
        assert!(dedup.should_alert(AnomalyType::SlowQuerySpike, 1000));
        // 冷却期后可以告警
        assert!(dedup.should_alert(AnomalyType::SlowQuerySpike, 70_000));
        assert_eq!(dedup.suppressed_count(), 0);
    }

    #[test]
    fn test_alert_dedup_different_types() {
        let dedup = AlertDedup::new(60_000);
        assert!(dedup.should_alert(AnomalyType::SlowQuerySpike, 1000));
        // 不同类型不受影响
        assert!(dedup.should_alert(AnomalyType::ErrorRateSpike, 2000));
        assert!(dedup.should_alert(AnomalyType::PoolExhaustion, 3000));
    }

    #[test]
    fn test_alert_dedup_reset() {
        let dedup = AlertDedup::new(60_000);
        dedup.should_alert(AnomalyType::SlowQuerySpike, 1000);
        dedup.reset();
        assert_eq!(dedup.suppressed_count(), 0);
        assert!(dedup.last_alert_time(AnomalyType::SlowQuerySpike).is_none());
    }

    #[test]
    fn test_alert_emitter_emit() {
        let emitter = AlertEmitter::new(60_000);
        let alert = sample_alert(AnomalyType::SlowQuerySpike, 1000);
        let emitted = emitter.emit(alert);
        assert!(emitted.is_some());
        assert_eq!(emitter.emitted_count(), 1);
    }

    #[test]
    fn test_alert_emitter_dedup() {
        let emitter = AlertEmitter::new(60_000);
        let alert1 = sample_alert(AnomalyType::SlowQuerySpike, 1000);
        let alert2 = sample_alert(AnomalyType::SlowQuerySpike, 30_000);
        assert!(emitter.emit(alert1).is_some());
        assert!(emitter.emit(alert2).is_none()); // 被去重
        assert_eq!(emitter.emitted_count(), 1);
        assert_eq!(emitter.suppressed_count(), 1);
    }

    #[test]
    fn test_alert_emitter_subscribe() {
        let emitter = AlertEmitter::new(60_000);
        let counter = Arc::new(AtomicU64::new(0));
        let counter_clone = Arc::clone(&counter);
        let _id = emitter.subscribe(Arc::new(move |_alert| {
            counter_clone.fetch_add(1, Ordering::Relaxed);
        }));
        let alert = sample_alert(AnomalyType::SlowQuerySpike, 1000);
        emitter.emit(alert);
        assert_eq!(counter.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_alert_emitter_subscribe_panic_isolation() {
        let emitter = AlertEmitter::new(60_000);
        let _id = emitter.subscribe(Arc::new(|_alert| {
            panic!("callback panic");
        }));
        let alert = sample_alert(AnomalyType::SlowQuerySpike, 1000);
        // 回调 panic 不影响 emit
        let emitted = emitter.emit(alert);
        assert!(emitted.is_some());
    }

    #[test]
    fn test_alert_emitter_unsubscribe() {
        let emitter = AlertEmitter::new(60_000);
        let id = emitter.subscribe(Arc::new(|_alert| {}));
        assert_eq!(emitter.subscriber_count(), 1);
        assert!(emitter.unsubscribe(id));
        assert_eq!(emitter.subscriber_count(), 0);
    }

    #[test]
    fn test_alert_emitter_history() {
        let emitter = AlertEmitter::new(0); // 无冷却期
        emitter.emit(sample_alert(AnomalyType::SlowQuerySpike, 1000));
        emitter.emit(sample_alert(AnomalyType::ErrorRateSpike, 2000));
        emitter.emit(sample_alert(AnomalyType::PoolExhaustion, 3000));
        let history = emitter.history();
        assert_eq!(history.len(), 3);
        let range = emitter.history_in_range(1500, 2500);
        assert_eq!(range.len(), 1);
        assert_eq!(range[0].anomaly_type, AnomalyType::ErrorRateSpike);
    }

    #[test]
    fn test_alert_emitter_clear_history() {
        let emitter = AlertEmitter::new(0);
        emitter.emit(sample_alert(AnomalyType::SlowQuerySpike, 1000));
        assert_eq!(emitter.history().len(), 1);
        emitter.clear_history();
        assert_eq!(emitter.history().len(), 0);
    }
}
