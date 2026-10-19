//! 闭环健康度监控（v10.2.0）
//!
//! 监控可观测性闭环自身的运转健康度：事件吞吐/处理延迟分布/积压深度/降级频次/丢弃数。

use std::sync::atomic::{AtomicU32, AtomicU64};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

pub use sz_orm_core::performance_linkage::LoopHealthConfig;

/// 闭环健康度指标（原子计数器无锁采集）
pub struct LoopHealthMetrics {
    pub enqueued_count: AtomicU64,
    pub dequeued_count: AtomicU64,
    pub processed_count: AtomicU64,
    pub degraded_count: AtomicU64,
    pub dropped_count: AtomicU64,
    pub current_backlog: AtomicU32,
    pub processing_latency_samples: Mutex<Vec<u64>>,
}

impl LoopHealthMetrics {
    pub fn new() -> Self {
        Self {
            enqueued_count: AtomicU64::new(0),
            dequeued_count: AtomicU64::new(0),
            processed_count: AtomicU64::new(0),
            degraded_count: AtomicU64::new(0),
            dropped_count: AtomicU64::new(0),
            current_backlog: AtomicU32::new(0),
            processing_latency_samples: Mutex::new(Vec::new()),
        }
    }

    pub fn record_enqueue(&self) {
        self.enqueued_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.current_backlog
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn record_process(&self, latency_ms: u64) {
        self.dequeued_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.processed_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.current_backlog
            .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        if let Ok(mut samples) = self.processing_latency_samples.lock() {
            if samples.len() < 1000 {
                samples.push(latency_ms);
            }
        }
    }

    pub fn record_degrade(&self) {
        self.degraded_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn record_drop(&self) {
        self.dropped_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

impl Default for LoopHealthMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// 闭环健康度快照（供查询返回）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopHealthMetricsSnapshot {
    pub enqueued_count: u64,
    pub dequeued_count: u64,
    pub processed_count: u64,
    pub degraded_count: u64,
    pub dropped_count: u64,
    pub current_backlog: u32,
    pub avg_latency_ms: f64,
    pub p99_latency_ms: f64,
    pub health_score: f64,
}

impl LoopHealthMetrics {
    pub fn snapshot(&self) -> LoopHealthMetricsSnapshot {
        let enqueued = self
            .enqueued_count
            .load(std::sync::atomic::Ordering::Relaxed);
        let dequeued = self
            .dequeued_count
            .load(std::sync::atomic::Ordering::Relaxed);
        let processed = self
            .processed_count
            .load(std::sync::atomic::Ordering::Relaxed);
        let degraded = self
            .degraded_count
            .load(std::sync::atomic::Ordering::Relaxed);
        let dropped = self
            .dropped_count
            .load(std::sync::atomic::Ordering::Relaxed);
        let backlog = self
            .current_backlog
            .load(std::sync::atomic::Ordering::Relaxed);

        let (avg_latency, p99_latency) = {
            let samples = self.processing_latency_samples.lock().unwrap();
            if samples.is_empty() {
                (0.0, 0.0)
            } else {
                let sum: u64 = samples.iter().sum();
                let avg = sum as f64 / samples.len() as f64;
                let mut sorted = samples.clone();
                sorted.sort_unstable();
                let p99_idx = ((sorted.len() as f64 - 1.0) * 0.99).round() as usize;
                (avg, sorted[p99_idx.min(sorted.len() - 1)] as f64)
            }
        };

        let health_score = if enqueued == 0 {
            100.0
        } else {
            let success_rate = processed as f64 / enqueued as f64;
            let degrade_penalty = degraded as f64 * 0.5;
            let drop_penalty = dropped as f64 * 2.0;
            let backlog_penalty = backlog as f64 * 0.1;
            (100.0 * success_rate - degrade_penalty - drop_penalty - backlog_penalty).max(0.0)
        };

        LoopHealthMetricsSnapshot {
            enqueued_count: enqueued,
            dequeued_count: dequeued,
            processed_count: processed,
            degraded_count: degraded,
            dropped_count: dropped,
            current_backlog: backlog,
            avg_latency_ms: avg_latency,
            p99_latency_ms: p99_latency,
            health_score,
        }
    }

    pub fn record_dequeue(&self) {
        self.dequeued_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.current_backlog
            .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn record_latency(&self, ms: u64) {
        if let Ok(mut samples) = self.processing_latency_samples.lock() {
            if samples.len() >= 1000 {
                samples.remove(0);
            }
            samples.push(ms);
        }
    }
}

/// 闭环健康度监控器
pub struct LoopHealthMonitor {
    config: LoopHealthConfig,
    metrics: LoopHealthMetrics,
}

impl LoopHealthMonitor {
    pub fn new(config: LoopHealthConfig) -> Self {
        Self {
            config,
            metrics: LoopHealthMetrics::new(),
        }
    }

    pub fn record_enqueue(&self) {
        self.metrics.record_enqueue();
    }

    pub fn record_dequeue(&self) {
        self.metrics.record_dequeue();
    }

    pub fn record_process(&self, latency_ms: u64) {
        self.metrics.record_process(latency_ms);
    }

    pub fn record_degrade(&self) {
        self.metrics.record_degrade();
    }

    pub fn record_drop(&self) {
        self.metrics.record_drop();
    }

    pub fn record_latency(&self, ms: u64) {
        self.metrics.record_latency(ms);
    }

    /// 计算健康度评分 ∈ [0, 100]
    pub fn compute_health_score(&self) -> f64 {
        let snap = self.metrics.snapshot();
        let mut score: f64 = 100.0;
        if snap.current_backlog > self.config.backlog_alert_limit {
            score -= 40.0;
        }
        let total = snap.degraded_count + snap.dropped_count;
        if snap.enqueued_count > 0 && total as f64 / snap.enqueued_count as f64 > 0.1 {
            score -= 30.0;
        }
        score.clamp(0.0, 100.0)
    }

    /// 指标快照
    pub fn metrics(&self) -> LoopHealthMetricsSnapshot {
        self.metrics.snapshot()
    }

    /// 获取配置引用
    pub fn config(&self) -> &LoopHealthConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_loop_health_config_default_disabled() {
        let config = LoopHealthConfig::default();
        assert!(!config.loop_health_enabled);
        assert_eq!(config.health_alert_threshold, 60, "default threshold 60");
        let enabled = LoopHealthConfig {
            loop_health_enabled: true,
            ..Default::default()
        };
        assert!(enabled.loop_health_enabled, "enabled config");
    }

    #[test]
    fn test_loop_health_metrics_record_and_snapshot() {
        let metrics = LoopHealthMetrics::new();
        metrics.record_enqueue();
        metrics.record_enqueue();
        metrics.record_process(10);
        metrics.record_process(20);
        metrics.record_degrade();
        let snap = metrics.snapshot();
        assert_eq!(snap.enqueued_count, 2, "2 enqueued");
        assert_eq!(snap.processed_count, 2, "2 processed");
        assert_eq!(snap.degraded_count, 1, "1 degraded");
        assert!(
            (snap.avg_latency_ms - 15.0).abs() < 1e-9,
            "avg latency 15ms"
        );
        assert!(snap.health_score < 100.0, "degrade penalty reduces score");
    }

    #[test]
    fn test_loop_health_metrics_atomic_count() {
        let monitor = LoopHealthMonitor::new(LoopHealthConfig::default());
        for _ in 0..10 {
            monitor.record_enqueue();
        }
        for _ in 0..5 {
            monitor.record_dequeue();
        }
        let snap = monitor.metrics();
        assert_eq!(snap.enqueued_count, 10, "10 enqueued");
        assert_eq!(snap.dequeued_count, 5, "5 dequeued");
        assert_eq!(snap.current_backlog, 5, "backlog 5");
    }

    #[test]
    fn test_loop_health_score_normal() {
        let monitor = LoopHealthMonitor::new(LoopHealthConfig::default());
        monitor.record_enqueue();
        monitor.record_process(5);
        let score = monitor.compute_health_score();
        assert!(score >= 80.0, "normal operation score >= 80: {score}");
    }

    #[test]
    fn test_loop_health_score_backlog_overlimit() {
        let config = LoopHealthConfig {
            loop_health_enabled: true,
            backlog_alert_limit: 10,
            ..Default::default()
        };
        let monitor = LoopHealthMonitor::new(config);
        for _ in 0..20 {
            monitor.record_enqueue();
        }
        let score = monitor.compute_health_score();
        assert!(score <= 60.0, "backlog overlimit score <= 60: {score}");
    }

    #[test]
    fn test_loop_health_score_degradation_high() {
        let monitor = LoopHealthMonitor::new(LoopHealthConfig::default());
        for _ in 0..100 {
            monitor.record_enqueue();
        }
        for _ in 0..20 {
            monitor.record_degrade();
        }
        let score = monitor.compute_health_score();
        assert!(score < 80.0, "high degradation score < 80: {score}");
    }
}
