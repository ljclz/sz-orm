//! v7.4.0 任务 3.5：性能指标暴露
//!
//! PerfMetrics 通过 OnceLock 全局单例 + 原子计数器无锁采集。
//! 通过 snapshot() 获取当前指标快照。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

/// 性能指标（全局单例，原子计数器无锁采集）
#[derive(Debug, Default)]
pub struct PerfMetrics {
    simd_hits: AtomicU64,
    simd_misses: AtomicU64,
    simd_speedup_micros: AtomicU64,
    zero_copy_hits: AtomicU64,
    zero_copy_misses: AtomicU64,
    zero_copy_alloc_reduction: AtomicU64,
    pool_acquire_count: AtomicU64,
    pool_acquire_failed_count: AtomicU64,
    pool_throughput_ops: AtomicU64,
    plan_cache_hits: AtomicU64,
    plan_cache_misses: AtomicU64,
}

impl PerfMetrics {
    /// 获取全局单例
    pub fn global() -> &'static PerfMetrics {
        static METRICS: OnceLock<PerfMetrics> = OnceLock::new();
        METRICS.get_or_init(PerfMetrics::default)
    }

    /// 记数：SIMD 命中
    pub fn record_simd_hit(&self) {
        self.simd_hits.fetch_add(1, Ordering::Relaxed);
    }

    /// 计数：SIMD 未命中（回退标量）
    pub fn record_simd_miss(&self) {
        self.simd_misses.fetch_add(1, Ordering::Relaxed);
    }

    /// 记数：零拷贝命中
    pub fn record_zero_copy_hit(&self) {
        self.zero_copy_hits.fetch_add(1, Ordering::Relaxed);
    }

    /// 计数：零拷贝未命中
    pub fn record_zero_copy_miss(&self) {
        self.zero_copy_misses.fetch_add(1, Ordering::Relaxed);
    }

    /// 记数：零拷贝分配减少（字节）
    pub fn record_alloc_reduction(&self, bytes: u64) {
        self.zero_copy_alloc_reduction
            .fetch_add(bytes, Ordering::Relaxed);
    }

    /// 计数：连接池 acquire
    pub fn record_pool_acquire(&self) {
        self.pool_acquire_count.fetch_add(1, Ordering::Relaxed);
    }

    /// 计数：连接池 acquire 失败
    pub fn record_pool_acquire_failed(&self) {
        self.pool_acquire_failed_count
            .fetch_add(1, Ordering::Relaxed);
    }

    /// 计数：连接池吞吐（ops/s）
    pub fn record_pool_throughput(&self, ops: u64) {
        self.pool_throughput_ops.store(ops, Ordering::Relaxed);
    }

    /// 计数：计划缓存命中
    pub fn record_plan_cache_hit(&self) {
        self.plan_cache_hits.fetch_add(1, Ordering::Relaxed);
    }

    /// 计数：计划缓存未命中
    pub fn record_plan_cache_miss(&self) {
        self.plan_cache_misses.fetch_add(1, Ordering::Relaxed);
    }

    /// 计数：SIMD 加速比（微秒级精度，存储为 micros = speedup * 1_000_000）
    pub fn record_simd_speedup(&self, speedup: f64) {
        let micros = (speedup * 1_000_000.0) as u64;
        self.simd_speedup_micros.store(micros, Ordering::Relaxed);
    }

    /// 获取指标快照
    pub fn snapshot(&self) -> PerfMetricsSnapshot {
        let simd_hits = self.simd_hits.load(Ordering::Relaxed);
        let simd_misses = self.simd_misses.load(Ordering::Relaxed);
        let plan_cache_hits = self.plan_cache_hits.load(Ordering::Relaxed);
        let plan_cache_misses = self.plan_cache_misses.load(Ordering::Relaxed);
        let pool_acquire_count = self.pool_acquire_count.load(Ordering::Relaxed);
        let zero_copy_hits = self.zero_copy_hits.load(Ordering::Relaxed);
        let zero_copy_misses = self.zero_copy_misses.load(Ordering::Relaxed);
        PerfMetricsSnapshot {
            simd_hits,
            simd_misses,
            simd_speedup_avg: self.simd_speedup_micros.load(Ordering::Relaxed) as f64 / 1_000_000.0,
            zero_copy_hits,
            zero_copy_misses,
            zero_copy_alloc_reduction: self.zero_copy_alloc_reduction.load(Ordering::Relaxed),
            pool_acquire_count,
            pool_acquire_failed_count: self.pool_acquire_failed_count.load(Ordering::Relaxed),
            pool_throughput_ops: self.pool_throughput_ops.load(Ordering::Relaxed),
            plan_cache_hits,
            plan_cache_misses,
            plan_cache_hit_rate: {
                let total = plan_cache_hits + plan_cache_misses;
                if total == 0 {
                    0.0
                } else {
                    plan_cache_hits as f64 / total as f64
                }
            },
            simd_hit_rate: {
                let total = simd_hits + simd_misses;
                if total == 0 {
                    0.0
                } else {
                    simd_hits as f64 / total as f64
                }
            },
            zero_copy_hit_rate: {
                let total = zero_copy_hits + zero_copy_misses;
                if total == 0 {
                    0.0
                } else {
                    zero_copy_hits as f64 / total as f64
                }
            },
            pool_acquire_success_rate: {
                let total =
                    pool_acquire_count + self.pool_acquire_failed_count.load(Ordering::Relaxed);
                if total == 0 {
                    0.0
                } else {
                    pool_acquire_count as f64 / total as f64
                }
            },
        }
    }

    /// Prometheus 格式导出
    pub fn to_prometheus(&self) -> String {
        let s = self.snapshot();
        format!(
            "# HELP sz_orm_simd_hits Total SIMD hits\n\
             # TYPE sz_orm_simd_hits counter\n\
             sz_orm_simd_hits {}\n\
             # HELP sz_orm_simd_misses Total SIMD misses (fallback to scalar)\n\
             # TYPE sz_orm_simd_misses counter\n\
             sz_orm_simd_misses {}\n\
             # HELP sz_orm_zero_copy_hits Total zero-copy hits\n\
             # TYPE sz_orm_zero_copy_hits counter\n\
             sz_orm_zero_copy_hits {}\n\
             # HELP sz_orm_zero_copy_misses Total zero-copy misses\n\
             # TYPE sz_orm_zero_copy_misses counter\n\
             sz_orm_zero_copy_misses {}\n\
             # HELP sz_orm_plan_cache_hits Total plan cache hits\n\
             # TYPE sz_orm_plan_cache_hits counter\n\
             sz_orm_plan_cache_hits {}\n\
             # HELP sz_orm_plan_cache_misses Total plan cache misses\n\
             # TYPE sz_orm_plan_cache_misses counter\n\
             sz_orm_plan_cache_misses {}\n\
             # HELP sz_orm_plan_cache_hit_rate Plan cache hit rate\n\
             # TYPE sz_orm_plan_cache_hit_rate gauge\n\
             sz_orm_plan_cache_hit_rate {}\n\
             # HELP sz_orm_pool_acquire_count Total pool acquire count\n\
             # TYPE sz_orm_pool_acquire_count counter\n\
             sz_orm_pool_acquire_count {}\n\
             # HELP sz_orm_pool_throughput_ops Pool throughput (ops/s)\n\
             # TYPE sz_orm_pool_throughput_ops gauge\n\
             sz_orm_pool_throughput_ops {}\n",
            s.simd_hits,
            s.simd_misses,
            s.zero_copy_hits,
            s.zero_copy_misses,
            s.plan_cache_hits,
            s.plan_cache_misses,
            s.plan_cache_hit_rate,
            s.pool_acquire_count,
            s.pool_throughput_ops,
        )
    }
}

/// 性能指标快照（一次性读取所有指标）
#[derive(Debug, Clone)]
pub struct PerfMetricsSnapshot {
    /// SIMD 命中次数
    pub simd_hits: u64,
    /// SIMD 未命中次数
    pub simd_misses: u64,
    /// SIMD 平均加速比
    pub simd_speedup_avg: f64,
    /// SIMD 命中率
    pub simd_hit_rate: f64,
    /// 零拷贝命中次数
    pub zero_copy_hits: u64,
    /// 零拷贝未命中次数
    pub zero_copy_misses: u64,
    /// 零拷贝命中率
    pub zero_copy_hit_rate: f64,
    /// 零拷贝分配减少（字节）
    pub zero_copy_alloc_reduction: u64,
    /// 连接池 acquire 次数
    pub pool_acquire_count: u64,
    /// 连接池 acquire 失败次数
    pub pool_acquire_failed_count: u64,
    /// 连接池 acquire 成功率
    pub pool_acquire_success_rate: f64,
    /// 连接池吞吐（ops/s）
    pub pool_throughput_ops: u64,
    /// 计划缓存命中次数
    pub plan_cache_hits: u64,
    /// 计划缓存未命中次数
    pub plan_cache_misses: u64,
    /// 计划缓存命中率
    pub plan_cache_hit_rate: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_global_singleton() {
        let m1 = PerfMetrics::global();
        let m2 = PerfMetrics::global();
        m1.record_simd_hit();
        assert!(m2.snapshot().simd_hits >= 1);
    }

    #[test]
    fn test_snapshot() {
        let m = PerfMetrics::default();
        m.record_simd_hit();
        m.record_simd_hit();
        m.record_simd_miss();
        m.record_plan_cache_hit();
        m.record_plan_cache_miss();
        let s = m.snapshot();
        assert_eq!(s.simd_hits, 2);
        assert_eq!(s.simd_misses, 1);
        assert_eq!(s.plan_cache_hits, 1);
        assert_eq!(s.plan_cache_misses, 1);
        assert!((s.simd_hit_rate - 2.0 / 3.0).abs() < 1e-9);
        assert!((s.plan_cache_hit_rate - 0.5).abs() < 1e-9);
    }

    #[test]
    fn test_prometheus_format() {
        let m = PerfMetrics::default();
        m.record_simd_hit();
        m.record_zero_copy_hit();
        m.record_plan_cache_hit();
        let prom = m.to_prometheus();
        assert!(prom.contains("sz_orm_simd_hits 1"));
        assert!(prom.contains("sz_orm_zero_copy_hits 1"));
        assert!(prom.contains("sz_orm_plan_cache_hits 1"));
    }

    #[test]
    fn test_simd_speedup() {
        let m = PerfMetrics::default();
        m.record_simd_speedup(1.5);
        let s = m.snapshot();
        assert!((s.simd_speedup_avg - 1.5).abs() < 1e-6);
    }

    #[test]
    fn test_pool_metrics() {
        let m = PerfMetrics::default();
        m.record_pool_acquire();
        m.record_pool_acquire();
        m.record_pool_acquire_failed();
        m.record_pool_throughput(10000);
        let s = m.snapshot();
        assert_eq!(s.pool_acquire_count, 2);
        assert_eq!(s.pool_acquire_failed_count, 1);
        assert_eq!(s.pool_throughput_ops, 10000);
        assert!((s.pool_acquire_success_rate - 2.0 / 3.0).abs() < 1e-9);
    }
}
// ============================================================================
// v10.12.0 T4.3：自适应调优 + 故障恢复指标扩展
// ============================================================================

/// 自适应调优指标快照
#[derive(Debug, Clone, Default)]
pub struct AdaptiveMetricsSnapshot {
    /// 水位线调整次数
    pub waterline_changes: u64,
    /// 信号采样次数
    pub signal_samples: u64,
    /// 负载评分次数
    pub load_scores: u64,
    /// 扩容次数
    pub scale_up_count: u64,
    /// 缩容次数
    pub scale_down_count: u64,
    /// 预热次数
    pub prewarm_count: u64,
    /// 读写分离调优次数
    pub rw_tune_count: u64,
}

/// 故障恢复指标快照
#[derive(Debug, Clone, Default)]
pub struct FailoverMetricsSnapshot {
    /// 故障检测次数
    pub failover_detections: u64,
    /// 故障确认次数
    pub failover_confirms: u64,
    /// 故障切换次数
    pub failover_switches: u64,
    /// 事务重试次数
    pub tx_retries: u64,
    /// 补偿操作次数
    pub compensate_count: u64,
    /// 半开探测次数
    pub half_open_probes: u64,
    /// RTO 优化次数
    pub rto_optimizations: u64,
    /// 恢复协调次数
    pub recovery_coordinations: u64,
}

/// 自适应调优 + 故障恢复指标采集器
#[derive(Debug, Default)]
pub struct AdaptiveFailoverMetrics {
    waterline_changes: AtomicU64,
    signal_samples: AtomicU64,
    load_scores: AtomicU64,
    scale_up_count: AtomicU64,
    scale_down_count: AtomicU64,
    prewarm_count: AtomicU64,
    rw_tune_count: AtomicU64,
    failover_detections: AtomicU64,
    failover_confirms: AtomicU64,
    failover_switches: AtomicU64,
    tx_retries: AtomicU64,
    compensate_count: AtomicU64,
    half_open_probes: AtomicU64,
    rto_optimizations: AtomicU64,
    recovery_coordinations: AtomicU64,
}

impl AdaptiveFailoverMetrics {
    /// 全局单例
    pub fn global() -> &'static AdaptiveFailoverMetrics {
        static METRICS: OnceLock<AdaptiveFailoverMetrics> = OnceLock::new();
        METRICS.get_or_init(AdaptiveFailoverMetrics::default)
    }

    /// 记录水位线调整
    pub fn record_waterline_change(&self) {
        self.waterline_changes.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录信号采样
    pub fn record_signal_sample(&self) {
        self.signal_samples.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录负载评分
    pub fn record_load_score(&self) {
        self.load_scores.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录扩容
    pub fn record_scale_up(&self) {
        self.scale_up_count.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录缩容
    pub fn record_scale_down(&self) {
        self.scale_down_count.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录预热
    pub fn record_prewarm(&self) {
        self.prewarm_count.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录读写分离调优
    pub fn record_rw_tune(&self) {
        self.rw_tune_count.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录故障检测
    pub fn record_failover_detection(&self) {
        self.failover_detections.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录故障确认
    pub fn record_failover_confirm(&self) {
        self.failover_confirms.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录故障切换
    pub fn record_failover_switch(&self) {
        self.failover_switches.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录事务重试
    pub fn record_tx_retry(&self) {
        self.tx_retries.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录补偿操作
    pub fn record_compensate(&self) {
        self.compensate_count.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录半开探测
    pub fn record_half_open_probe(&self) {
        self.half_open_probes.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录 RTO 优化
    pub fn record_rto_optimization(&self) {
        self.rto_optimizations.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录恢复协调
    pub fn record_recovery_coordination(&self) {
        self.recovery_coordinations.fetch_add(1, Ordering::Relaxed);
    }

    /// 自适应调优指标快照
    pub fn adaptive_snapshot(&self) -> AdaptiveMetricsSnapshot {
        AdaptiveMetricsSnapshot {
            waterline_changes: self.waterline_changes.load(Ordering::Relaxed),
            signal_samples: self.signal_samples.load(Ordering::Relaxed),
            load_scores: self.load_scores.load(Ordering::Relaxed),
            scale_up_count: self.scale_up_count.load(Ordering::Relaxed),
            scale_down_count: self.scale_down_count.load(Ordering::Relaxed),
            prewarm_count: self.prewarm_count.load(Ordering::Relaxed),
            rw_tune_count: self.rw_tune_count.load(Ordering::Relaxed),
        }
    }

    /// 故障恢复指标快照
    pub fn failover_snapshot(&self) -> FailoverMetricsSnapshot {
        FailoverMetricsSnapshot {
            failover_detections: self.failover_detections.load(Ordering::Relaxed),
            failover_confirms: self.failover_confirms.load(Ordering::Relaxed),
            failover_switches: self.failover_switches.load(Ordering::Relaxed),
            tx_retries: self.tx_retries.load(Ordering::Relaxed),
            compensate_count: self.compensate_count.load(Ordering::Relaxed),
            half_open_probes: self.half_open_probes.load(Ordering::Relaxed),
            rto_optimizations: self.rto_optimizations.load(Ordering::Relaxed),
            recovery_coordinations: self.recovery_coordinations.load(Ordering::Relaxed),
        }
    }
}

#[cfg(test)]
mod adaptive_failover_tests {
    use super::*;

    #[test]
    fn test_adaptive_metrics_record() {
        let m = AdaptiveFailoverMetrics::default();
        m.record_waterline_change();
        m.record_waterline_change();
        m.record_signal_sample();
        m.record_load_score();
        m.record_scale_up();
        m.record_scale_down();
        m.record_prewarm();
        m.record_rw_tune();

        let s = m.adaptive_snapshot();
        assert_eq!(s.waterline_changes, 2);
        assert_eq!(s.signal_samples, 1);
        assert_eq!(s.load_scores, 1);
        assert_eq!(s.scale_up_count, 1);
        assert_eq!(s.scale_down_count, 1);
        assert_eq!(s.prewarm_count, 1);
        assert_eq!(s.rw_tune_count, 1);
    }

    #[test]
    fn test_failover_metrics_record() {
        let m = AdaptiveFailoverMetrics::default();
        m.record_failover_detection();
        m.record_failover_confirm();
        m.record_failover_switch();
        m.record_tx_retry();
        m.record_compensate();
        m.record_half_open_probe();
        m.record_rto_optimization();
        m.record_recovery_coordination();

        let s = m.failover_snapshot();
        assert_eq!(s.failover_detections, 1);
        assert_eq!(s.failover_confirms, 1);
        assert_eq!(s.failover_switches, 1);
        assert_eq!(s.tx_retries, 1);
        assert_eq!(s.compensate_count, 1);
        assert_eq!(s.half_open_probes, 1);
        assert_eq!(s.rto_optimizations, 1);
        assert_eq!(s.recovery_coordinations, 1);
    }

    #[test]
    fn test_global_singleton() {
        let m1 = AdaptiveFailoverMetrics::global();
        let m2 = AdaptiveFailoverMetrics::global();
        m1.record_waterline_change();
        assert!(m2.adaptive_snapshot().waterline_changes >= 1);
    }
}
