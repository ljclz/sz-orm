//! 性能 feature 命中率度量（v6.8.0 TASK W1-7）
//!
//! 追踪四大性能优化路径的命中/未命中次数，量化各 feature gate 的实际生效比例。
//! 四大路径：零拷贝、SIMD 向量化、io_uring 异步 IO、执行器优化。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// 性能优化路径标识
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PerfPath {
    ZeroCopy,
    Simd,
    IoUring,
    ExecutorOpt,
    Pool,
    PlanCache,
}

impl PerfPath {
    pub fn metric_name(&self) -> &'static str {
        match self {
            PerfPath::ZeroCopy => "sz_orm_perf_zero_copy",
            PerfPath::Simd => "sz_orm_perf_simd",
            PerfPath::IoUring => "sz_orm_perf_io_uring",
            PerfPath::ExecutorOpt => "sz_orm_perf_executor_opt",
            PerfPath::Pool => "sz_orm_perf_pool",
            PerfPath::PlanCache => "sz_orm_perf_plan_cache",
        }
    }
}

/// 单条性能路径的命中/未命中计数
#[derive(Debug, Default)]
struct PathCounters {
    hit: AtomicU64,
    miss: AtomicU64,
}

impl PathCounters {
    fn record_hit(&self) {
        self.hit.fetch_add(1, Ordering::Relaxed);
    }

    fn record_miss(&self) {
        self.miss.fetch_add(1, Ordering::Relaxed);
    }

    fn hit(&self) -> u64 {
        self.hit.load(Ordering::Relaxed)
    }

    fn miss(&self) -> u64 {
        self.miss.load(Ordering::Relaxed)
    }

    fn total(&self) -> u64 {
        self.hit() + self.miss()
    }

    fn hit_rate(&self) -> f64 {
        let total = self.total();
        if total == 0 {
            0.0
        } else {
            self.hit() as f64 / total as f64
        }
    }
}

/// 性能 feature 命中率度量集合
///
/// 追踪四大性能优化路径的命中/未命中次数。
/// 线程安全（`AtomicU64`），可在多线程环境下并发记录。
#[derive(Debug, Default)]
pub struct PerfHitMetrics {
    zero_copy: PathCounters,
    simd: PathCounters,
    io_uring: PathCounters,
    executor_opt: PathCounters,
    pool: PathCounters,
    plan_cache: PathCounters,
    /// v7.6.0 扩展指标
    v760: PerfHitMetricsV760,
    /// v7.7.0 稳定性加固指标
    v770: PerfHitMetricsV770,
}

/// v7.6.0 四项加速可观测指标扩展
#[derive(Debug, Default)]
pub struct PerfHitMetricsV760 {
    // SIMD 深化
    simd_aggregate_speedup: AtomicU64,
    simd_filter_bitmap_speedup: AtomicU64,
    simd_misses: AtomicU64,
    // 零拷贝扩展
    zero_copy_array_ref_hits: AtomicU64,
    zero_copy_object_ref_hits: AtomicU64,
    zero_copy_heap_reduction_rate: AtomicU64,
    // IO_uring
    io_uring_hits: AtomicU64,
    io_uring_fallbacks: AtomicU64,
    io_uring_throughput_ratio: AtomicU64,
    // 池调优
    pool_adaptive_tuning_count: AtomicU64,
    pool_shrink_count: AtomicU64,
    pool_expand_count: AtomicU64,
    pool_failover_error_rate: AtomicU64,
}

impl PerfHitMetricsV760 {
    /// 记录 SIMD 聚合加速比（存储为 f64::to_bits）
    pub fn set_simd_aggregate_speedup(&self, speedup: f64) {
        self.simd_aggregate_speedup
            .store(speedup.to_bits(), Ordering::Relaxed);
    }

    /// 记录 SIMD 位图过滤加速比
    pub fn set_simd_filter_bitmap_speedup(&self, speedup: f64) {
        self.simd_filter_bitmap_speedup
            .store(speedup.to_bits(), Ordering::Relaxed);
    }

    /// 记录 SIMD 未命中
    pub fn record_simd_miss(&self) {
        self.simd_misses.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录零拷贝 ArrayRef 命中
    pub fn record_zero_copy_array_ref_hit(&self) {
        self.zero_copy_array_ref_hits
            .fetch_add(1, Ordering::Relaxed);
    }

    /// 记录零拷贝 ObjectRef 命中
    pub fn record_zero_copy_object_ref_hit(&self) {
        self.zero_copy_object_ref_hits
            .fetch_add(1, Ordering::Relaxed);
    }

    /// 记录零拷贝堆分配降低率
    pub fn set_zero_copy_heap_reduction_rate(&self, rate: f64) {
        self.zero_copy_heap_reduction_rate
            .store(rate.to_bits(), Ordering::Relaxed);
    }

    /// 记录 IO_uring 命中
    pub fn record_io_uring_hit(&self) {
        self.io_uring_hits.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录 IO_uring 回退
    pub fn record_io_uring_fallback(&self) {
        self.io_uring_fallbacks.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录 IO_uring 吞吐量比率
    pub fn set_io_uring_throughput_ratio(&self, ratio: f64) {
        self.io_uring_throughput_ratio
            .store(ratio.to_bits(), Ordering::Relaxed);
    }

    /// 记录池自适应调优
    pub fn record_pool_adaptive_tuning(&self) {
        self.pool_adaptive_tuning_count
            .fetch_add(1, Ordering::Relaxed);
    }

    /// 记录池缩容
    pub fn record_pool_shrink(&self) {
        self.pool_shrink_count.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录池扩容
    pub fn record_pool_expand(&self) {
        self.pool_expand_count.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录池故障转移错误率
    pub fn set_pool_failover_error_rate(&self, rate: f64) {
        self.pool_failover_error_rate
            .store(rate.to_bits(), Ordering::Relaxed);
    }

    /// SIMD 聚合加速比
    pub fn simd_aggregate_speedup(&self) -> f64 {
        f64::from_bits(self.simd_aggregate_speedup.load(Ordering::Relaxed))
    }

    /// SIMD 位图过滤加速比
    pub fn simd_filter_bitmap_speedup(&self) -> f64 {
        f64::from_bits(self.simd_filter_bitmap_speedup.load(Ordering::Relaxed))
    }

    /// SIMD 未命中次数
    pub fn simd_misses(&self) -> u64 {
        self.simd_misses.load(Ordering::Relaxed)
    }

    /// 零拷贝 ArrayRef 命中次数
    pub fn zero_copy_array_ref_hits(&self) -> u64 {
        self.zero_copy_array_ref_hits.load(Ordering::Relaxed)
    }

    /// 零拷贝 ObjectRef 命中次数
    pub fn zero_copy_object_ref_hits(&self) -> u64 {
        self.zero_copy_object_ref_hits.load(Ordering::Relaxed)
    }

    /// 零拷贝堆分配降低率
    pub fn zero_copy_heap_reduction_rate(&self) -> f64 {
        f64::from_bits(self.zero_copy_heap_reduction_rate.load(Ordering::Relaxed))
    }

    /// IO_uring 命中次数
    pub fn io_uring_hits(&self) -> u64 {
        self.io_uring_hits.load(Ordering::Relaxed)
    }

    /// IO_uring 回退次数
    pub fn io_uring_fallbacks(&self) -> u64 {
        self.io_uring_fallbacks.load(Ordering::Relaxed)
    }

    /// IO_uring 吞量比率
    pub fn io_uring_throughput_ratio(&self) -> f64 {
        f64::from_bits(self.io_uring_throughput_ratio.load(Ordering::Relaxed))
    }

    /// 池自适应调优次数
    pub fn pool_adaptive_tuning_count(&self) -> u64 {
        self.pool_adaptive_tuning_count.load(Ordering::Relaxed)
    }

    /// 池缩容次数
    pub fn pool_shrink_count(&self) -> u64 {
        self.pool_shrink_count.load(Ordering::Relaxed)
    }

    /// 池扩容次数
    pub fn pool_expand_count(&self) -> u64 {
        self.pool_expand_count.load(Ordering::Relaxed)
    }

    /// 池故障转移错误率
    pub fn pool_failover_error_rate(&self) -> f64 {
        f64::from_bits(self.pool_failover_error_rate.load(Ordering::Relaxed))
    }

    /// 渲染 v7.6.0 扩展指标为 Prometheus text format
    pub fn render(&self) -> String {
        let mut output = String::new();
        output.push_str("# HELP sz_orm_perf_simd_aggregate_speedup SIMD aggregate speedup ratio\n");
        output.push_str("# TYPE sz_orm_perf_simd_aggregate_speedup gauge\n");
        output.push_str(&format!(
            "sz_orm_perf_simd_aggregate_speedup {}\n",
            self.simd_aggregate_speedup()
        ));
        output.push_str(
            "# HELP sz_orm_perf_simd_filter_bitmap_speedup SIMD bitmap filter speedup ratio\n",
        );
        output.push_str("# TYPE sz_orm_perf_simd_filter_bitmap_speedup gauge\n");
        output.push_str(&format!(
            "sz_orm_perf_simd_filter_bitmap_speedup {}\n",
            self.simd_filter_bitmap_speedup()
        ));
        output.push_str("# HELP sz_orm_perf_simd_misses_total SIMD miss count\n");
        output.push_str("# TYPE sz_orm_perf_simd_misses_total counter\n");
        output.push_str(&format!(
            "sz_orm_perf_simd_misses_total {}\n",
            self.simd_misses()
        ));
        output.push_str(
            "# HELP sz_orm_perf_zero_copy_array_ref_hits_total Zero-copy ArrayRef hit count\n",
        );
        output.push_str("# TYPE sz_orm_perf_zero_copy_array_ref_hits_total counter\n");
        output.push_str(&format!(
            "sz_orm_perf_zero_copy_array_ref_hits_total {}\n",
            self.zero_copy_array_ref_hits()
        ));
        output.push_str(
            "# HELP sz_orm_perf_zero_copy_object_ref_hits_total Zero-copy ObjectRef hit count\n",
        );
        output.push_str("# TYPE sz_orm_perf_zero_copy_object_ref_hits_total counter\n");
        output.push_str(&format!(
            "sz_orm_perf_zero_copy_object_ref_hits_total {}\n",
            self.zero_copy_object_ref_hits()
        ));
        output.push_str(
            "# HELP sz_orm_perf_zero_copy_heap_reduction_rate Zero-copy heap reduction rate\n",
        );
        output.push_str("# TYPE sz_orm_perf_zero_copy_heap_reduction_rate gauge\n");
        output.push_str(&format!(
            "sz_orm_perf_zero_copy_heap_reduction_rate {}\n",
            self.zero_copy_heap_reduction_rate()
        ));
        output.push_str("# HELP sz_orm_perf_io_uring_hits_total IO_uring hit count\n");
        output.push_str("# TYPE sz_orm_perf_io_uring_hits_total counter\n");
        output.push_str(&format!(
            "sz_orm_perf_io_uring_hits_total {}\n",
            self.io_uring_hits()
        ));
        output.push_str("# HELP sz_orm_perf_io_uring_fallbacks_total IO_uring fallback count\n");
        output.push_str("# TYPE sz_orm_perf_io_uring_fallbacks_total counter\n");
        output.push_str(&format!(
            "sz_orm_perf_io_uring_fallbacks_total {}\n",
            self.io_uring_fallbacks()
        ));
        output.push_str("# HELP sz_orm_perf_io_uring_throughput_ratio IO_uring throughput ratio\n");
        output.push_str("# TYPE sz_orm_perf_io_uring_throughput_ratio gauge\n");
        output.push_str(&format!(
            "sz_orm_perf_io_uring_throughput_ratio {}\n",
            self.io_uring_throughput_ratio()
        ));
        output
            .push_str("# HELP sz_orm_perf_pool_adaptive_tuning_total Pool adaptive tuning count\n");
        output.push_str("# TYPE sz_orm_perf_pool_adaptive_tuning_total counter\n");
        output.push_str(&format!(
            "sz_orm_perf_pool_adaptive_tuning_total {}\n",
            self.pool_adaptive_tuning_count()
        ));
        output.push_str("# HELP sz_orm_perf_pool_shrink_count_total Pool shrink count\n");
        output.push_str("# TYPE sz_orm_perf_pool_shrink_count_total counter\n");
        output.push_str(&format!(
            "sz_orm_perf_pool_shrink_count_total {}\n",
            self.pool_shrink_count()
        ));
        output.push_str("# HELP sz_orm_perf_pool_expand_count_total Pool expand count\n");
        output.push_str("# TYPE sz_orm_perf_pool_expand_count_total counter\n");
        output.push_str(&format!(
            "sz_orm_perf_pool_expand_count_total {}\n",
            self.pool_expand_count()
        ));
        output.push_str("# HELP sz_orm_perf_pool_failover_error_rate Pool failover error rate\n");
        output.push_str("# TYPE sz_orm_perf_pool_failover_error_rate gauge\n");
        output.push_str(&format!(
            "sz_orm_perf_pool_failover_error_rate {}\n",
            self.pool_failover_error_rate()
        ));
        output
    }
}

/// v7.7.0 稳定性加固六项可观测指标扩展
#[derive(Debug, Default)]
pub struct PerfHitMetricsV770 {
    // 故障自愈
    self_heal_trigger_count: AtomicU64,
    self_heal_success_count: AtomicU64,
    self_heal_fail_count: AtomicU64,
    self_heal_recovery_time_ms: AtomicU64,
    // 熔断降级
    circuit_breaker_trigger_count: AtomicU64,
    degrade_non_core_count: AtomicU64,
    core_link_protected_count: AtomicU64,
    // 慢查询治理
    slow_query_governed_count: AtomicU64,
    slow_query_rollback_count: AtomicU64,
    slow_query_governance_latency_ms: AtomicU64,
    // 池深化
    dynamic_pool_tuning_count: AtomicU64,
    multi_pool_isolated_count: AtomicU64,
    pool_tuning_response_time_ms: AtomicU64,
    // 缓存优化
    cache_hit_rate_before: AtomicU64,
    cache_hit_rate_after: AtomicU64,
    cache_optimize_improvement_pp: AtomicU64,
    // 计划优化
    plan_auto_optimize_count: AtomicU64,
    plan_optimize_p95_improvement: AtomicU64,
    plan_optimize_decision_latency_ms: AtomicU64,
}

impl PerfHitMetricsV770 {
    // --- 故障自愈 ---
    pub fn record_self_heal_trigger(&self) {
        self.self_heal_trigger_count.fetch_add(1, Ordering::Relaxed);
    }
    pub fn record_self_heal_success(&self) {
        self.self_heal_success_count.fetch_add(1, Ordering::Relaxed);
    }
    pub fn record_self_heal_fail(&self) {
        self.self_heal_fail_count.fetch_add(1, Ordering::Relaxed);
    }
    pub fn set_self_heal_recovery_time_ms(&self, ms: f64) {
        self.self_heal_recovery_time_ms
            .store(ms.to_bits(), Ordering::Relaxed);
    }
    pub fn self_heal_trigger_count(&self) -> u64 {
        self.self_heal_trigger_count.load(Ordering::Relaxed)
    }
    pub fn self_heal_success_count(&self) -> u64 {
        self.self_heal_success_count.load(Ordering::Relaxed)
    }
    pub fn self_heal_fail_count(&self) -> u64 {
        self.self_heal_fail_count.load(Ordering::Relaxed)
    }
    pub fn self_heal_recovery_time_ms(&self) -> f64 {
        f64::from_bits(self.self_heal_recovery_time_ms.load(Ordering::Relaxed))
    }

    // --- 熔断降级 ---
    pub fn record_circuit_breaker_trigger(&self) {
        self.circuit_breaker_trigger_count
            .fetch_add(1, Ordering::Relaxed);
    }
    pub fn record_degrade_non_core(&self) {
        self.degrade_non_core_count.fetch_add(1, Ordering::Relaxed);
    }
    pub fn record_core_link_protected(&self) {
        self.core_link_protected_count
            .fetch_add(1, Ordering::Relaxed);
    }
    pub fn circuit_breaker_trigger_count(&self) -> u64 {
        self.circuit_breaker_trigger_count.load(Ordering::Relaxed)
    }
    pub fn degrade_non_core_count(&self) -> u64 {
        self.degrade_non_core_count.load(Ordering::Relaxed)
    }
    pub fn core_link_protected_count(&self) -> u64 {
        self.core_link_protected_count.load(Ordering::Relaxed)
    }

    // --- 慢查询治理 ---
    pub fn record_slow_query_governed(&self) {
        self.slow_query_governed_count
            .fetch_add(1, Ordering::Relaxed);
    }
    pub fn record_slow_query_rollback(&self) {
        self.slow_query_rollback_count
            .fetch_add(1, Ordering::Relaxed);
    }
    pub fn set_slow_query_governance_latency_ms(&self, ms: f64) {
        self.slow_query_governance_latency_ms
            .store(ms.to_bits(), Ordering::Relaxed);
    }
    pub fn slow_query_governed_count(&self) -> u64 {
        self.slow_query_governed_count.load(Ordering::Relaxed)
    }
    pub fn slow_query_rollback_count(&self) -> u64 {
        self.slow_query_rollback_count.load(Ordering::Relaxed)
    }
    pub fn slow_query_governance_latency_ms(&self) -> f64 {
        f64::from_bits(
            self.slow_query_governance_latency_ms
                .load(Ordering::Relaxed),
        )
    }

    // --- 池深化 ---
    pub fn record_dynamic_pool_tuning(&self) {
        self.dynamic_pool_tuning_count
            .fetch_add(1, Ordering::Relaxed);
    }
    pub fn record_multi_pool_isolated(&self) {
        self.multi_pool_isolated_count
            .fetch_add(1, Ordering::Relaxed);
    }
    pub fn set_pool_tuning_response_time_ms(&self, ms: f64) {
        self.pool_tuning_response_time_ms
            .store(ms.to_bits(), Ordering::Relaxed);
    }
    pub fn dynamic_pool_tuning_count(&self) -> u64 {
        self.dynamic_pool_tuning_count.load(Ordering::Relaxed)
    }
    pub fn multi_pool_isolated_count(&self) -> u64 {
        self.multi_pool_isolated_count.load(Ordering::Relaxed)
    }
    pub fn pool_tuning_response_time_ms(&self) -> f64 {
        f64::from_bits(self.pool_tuning_response_time_ms.load(Ordering::Relaxed))
    }

    // --- 缓存优化 ---
    pub fn set_cache_hit_rate_before(&self, rate: f64) {
        self.cache_hit_rate_before
            .store(rate.to_bits(), Ordering::Relaxed);
    }
    pub fn set_cache_hit_rate_after(&self, rate: f64) {
        self.cache_hit_rate_after
            .store(rate.to_bits(), Ordering::Relaxed);
    }
    pub fn set_cache_optimize_improvement_pp(&self, pp: f64) {
        self.cache_optimize_improvement_pp
            .store(pp.to_bits(), Ordering::Relaxed);
    }
    pub fn cache_hit_rate_before(&self) -> f64 {
        f64::from_bits(self.cache_hit_rate_before.load(Ordering::Relaxed))
    }
    pub fn cache_hit_rate_after(&self) -> f64 {
        f64::from_bits(self.cache_hit_rate_after.load(Ordering::Relaxed))
    }
    pub fn cache_optimize_improvement_pp(&self) -> f64 {
        f64::from_bits(self.cache_optimize_improvement_pp.load(Ordering::Relaxed))
    }

    // --- 计划优化 ---
    pub fn record_plan_auto_optimize(&self) {
        self.plan_auto_optimize_count
            .fetch_add(1, Ordering::Relaxed);
    }
    pub fn set_plan_optimize_p95_improvement(&self, imp: f64) {
        self.plan_optimize_p95_improvement
            .store(imp.to_bits(), Ordering::Relaxed);
    }
    pub fn set_plan_optimize_decision_latency_ms(&self, ms: f64) {
        self.plan_optimize_decision_latency_ms
            .store(ms.to_bits(), Ordering::Relaxed);
    }
    pub fn plan_auto_optimize_count(&self) -> u64 {
        self.plan_auto_optimize_count.load(Ordering::Relaxed)
    }
    pub fn plan_optimize_p95_improvement(&self) -> f64 {
        f64::from_bits(self.plan_optimize_p95_improvement.load(Ordering::Relaxed))
    }
    pub fn plan_optimize_decision_latency_ms(&self) -> f64 {
        f64::from_bits(
            self.plan_optimize_decision_latency_ms
                .load(Ordering::Relaxed),
        )
    }

    /// 渲染 v7.7.0 稳定性加固指标为 Prometheus text format
    pub fn render(&self) -> String {
        let mut o = String::new();
        o.push_str(&format!("# HELP sz_orm_self_heal_trigger_total Self-heal trigger count\n# TYPE sz_orm_self_heal_trigger_total counter\nsz_orm_self_heal_trigger_total {}\n", self.self_heal_trigger_count()));
        o.push_str(&format!("# HELP sz_orm_self_heal_success_total Self-heal success count\n# TYPE sz_orm_self_heal_success_total counter\nsz_orm_self_heal_success_total {}\n", self.self_heal_success_count()));
        o.push_str(&format!("# HELP sz_orm_self_heal_fail_total Self-heal fail count\n# TYPE sz_orm_self_heal_fail_total counter\nsz_orm_self_heal_fail_total {}\n", self.self_heal_fail_count()));
        o.push_str(&format!("# HELP sz_orm_self_heal_recovery_time_ms Self-heal recovery time\n# TYPE sz_orm_self_heal_recovery_time_ms gauge\nsz_orm_self_heal_recovery_time_ms {}\n", self.self_heal_recovery_time_ms()));
        o.push_str(&format!("# HELP sz_orm_circuit_breaker_trigger_total Circuit breaker trigger count\n# TYPE sz_orm_circuit_breaker_trigger_total counter\nsz_orm_circuit_breaker_trigger_total {}\n", self.circuit_breaker_trigger_count()));
        o.push_str(&format!("# HELP sz_orm_degrade_non_core_total Degrade non-core count\n# TYPE sz_orm_degrade_non_core_total counter\nsz_orm_degrade_non_core_total {}\n", self.degrade_non_core_count()));
        o.push_str(&format!("# HELP sz_orm_core_link_protected_total Core link protected count\n# TYPE sz_orm_core_link_protected_total counter\nsz_orm_core_link_protected_total {}\n", self.core_link_protected_count()));
        o.push_str(&format!("# HELP sz_orm_slow_query_governed_total Slow query governed count\n# TYPE sz_orm_slow_query_governed_total counter\nsz_orm_slow_query_governed_total {}\n", self.slow_query_governed_count()));
        o.push_str(&format!("# HELP sz_orm_slow_query_rollback_total Slow query rollback count\n# TYPE sz_orm_slow_query_rollback_total counter\nsz_orm_slow_query_rollback_total {}\n", self.slow_query_rollback_count()));
        o.push_str(&format!("# HELP sz_orm_slow_query_governance_latency_ms Slow query governance latency\n# TYPE sz_orm_slow_query_governance_latency_ms gauge\nsz_orm_slow_query_governance_latency_ms {}\n", self.slow_query_governance_latency_ms()));
        o.push_str(&format!("# HELP sz_orm_dynamic_pool_tuning_total Dynamic pool tuning count\n# TYPE sz_orm_dynamic_pool_tuning_total counter\nsz_orm_dynamic_pool_tuning_total {}\n", self.dynamic_pool_tuning_count()));
        o.push_str(&format!("# HELP sz_orm_multi_pool_isolated_total Multi-pool isolated count\n# TYPE sz_orm_multi_pool_isolated_total counter\nsz_orm_multi_pool_isolated_total {}\n", self.multi_pool_isolated_count()));
        o.push_str(&format!("# HELP sz_orm_pool_tuning_response_time_ms Pool tuning response time\n# TYPE sz_orm_pool_tuning_response_time_ms gauge\nsz_orm_pool_tuning_response_time_ms {}\n", self.pool_tuning_response_time_ms()));
        o.push_str(&format!("# HELP sz_orm_cache_hit_rate_before Cache hit rate before\n# TYPE sz_orm_cache_hit_rate_before gauge\nsz_orm_cache_hit_rate_before {}\n", self.cache_hit_rate_before()));
        o.push_str(&format!("# HELP sz_orm_cache_hit_rate_after Cache hit rate after\n# TYPE sz_orm_cache_hit_rate_after gauge\nsz_orm_cache_hit_rate_after {}\n", self.cache_hit_rate_after()));
        o.push_str(&format!("# HELP sz_orm_cache_optimize_improvement_pp Cache optimize improvement\n# TYPE sz_orm_cache_optimize_improvement_pp gauge\nsz_orm_cache_optimize_improvement_pp {}\n", self.cache_optimize_improvement_pp()));
        o.push_str(&format!("# HELP sz_orm_plan_auto_optimize_total Plan auto-optimize count\n# TYPE sz_orm_plan_auto_optimize_total counter\nsz_orm_plan_auto_optimize_total {}\n", self.plan_auto_optimize_count()));
        o.push_str(&format!("# HELP sz_orm_plan_optimize_p95_improvement Plan optimize P95 improvement\n# TYPE sz_orm_plan_optimize_p95_improvement gauge\nsz_orm_plan_optimize_p95_improvement {}\n", self.plan_optimize_p95_improvement()));
        o.push_str(&format!("# HELP sz_orm_plan_optimize_decision_latency_ms Plan optimize decision latency\n# TYPE sz_orm_plan_optimize_decision_latency_ms gauge\nsz_orm_plan_optimize_decision_latency_ms {}\n", self.plan_optimize_decision_latency_ms()));
        o
    }
}

impl PerfHitMetrics {
    /// 创建空的度量集合
    pub fn new() -> Self {
        Self::default()
    }

    /// 创建共享（`Arc`）实例
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::new())
    }

    /// 记录指定路径命中
    pub fn record_hit(&self, path: PerfPath) {
        self.counters(path).record_hit();
    }

    /// 计算指定路径未命中
    pub fn record_miss(&self, path: PerfPath) {
        self.counters(path).record_miss();
    }

    /// 批量记录命中
    pub fn record_hits(&self, path: PerfPath, n: u64) {
        let c = self.counters(path);
        c.hit.fetch_add(n, Ordering::Relaxed);
    }

    /// 批量记录未命中
    pub fn record_misses(&self, path: PerfPath, n: u64) {
        let c = self.counters(path);
        c.miss.fetch_add(n, Ordering::Relaxed);
    }

    /// 获取指定路径的命中次数
    pub fn hit(&self, path: PerfPath) -> u64 {
        self.counters(path).hit()
    }

    /// 获取指定路径的未命中次数
    pub fn miss(&self, path: PerfPath) -> u64 {
        self.counters(path).miss()
    }

    /// 获取指定路径的总调用次数
    pub fn total(&self, path: PerfPath) -> u64 {
        self.counters(path).total()
    }

    /// 计算指定路径的命中率（0.0~1.0）
    pub fn hit_rate(&self, path: PerfPath) -> f64 {
        self.counters(path).hit_rate()
    }

    /// 渲染为 Prometheus text format
    pub fn render(&self) -> String {
        let paths = [
            PerfPath::ZeroCopy,
            PerfPath::Simd,
            PerfPath::IoUring,
            PerfPath::ExecutorOpt,
            PerfPath::Pool,
            PerfPath::PlanCache,
        ];
        let mut output = String::new();
        for path in &paths {
            let c = self.counters(*path);
            let name = path.metric_name();
            output.push_str(&format!("# HELP {}_hit Total hit count\n", name));
            output.push_str(&format!("# TYPE {}_hit counter\n", name));
            output.push_str(&format!("{}_hit {}\n", name, c.hit()));
            output.push_str(&format!("# HELP {}_miss Total miss count\n", name));
            output.push_str(&format!("# TYPE {}_miss counter\n", name));
            output.push_str(&format!("{}_miss {}\n", name, c.miss()));
            output.push_str(&format!("# HELP {}_hit_ratio Hit ratio (0~1)\n", name));
            output.push_str(&format!("# TYPE {}_hit_ratio gauge\n", name));
            output.push_str(&format!("{}_hit_ratio {}\n", name, c.hit_rate()));
        }
        output.push_str(&self.v760.render());
        output.push_str(&self.v770.render());
        output
    }

    /// v7.6.0 扩展指标引用
    pub fn v760(&self) -> &PerfHitMetricsV760 {
        &self.v760
    }

    /// v7.7.0 稳定性加固指标引用
    pub fn v770(&self) -> &PerfHitMetricsV770 {
        &self.v770
    }

    fn counters(&self, path: PerfPath) -> &PathCounters {
        match path {
            PerfPath::ZeroCopy => &self.zero_copy,
            PerfPath::Simd => &self.simd,
            PerfPath::IoUring => &self.io_uring,
            PerfPath::ExecutorOpt => &self.executor_opt,
            PerfPath::Pool => &self.pool,
            PerfPath::PlanCache => &self.plan_cache,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v770_smoke_test() {
        let m = PerfHitMetrics::new();
        let _ = m.v770();
    }

    #[test]
    fn record_hit_miss_basic() {
        let m = PerfHitMetrics::new();
        m.record_hit(PerfPath::ZeroCopy);
        m.record_hit(PerfPath::ZeroCopy);
        m.record_miss(PerfPath::ZeroCopy);
        assert_eq!(m.hit(PerfPath::ZeroCopy), 2);
        assert_eq!(m.miss(PerfPath::ZeroCopy), 1);
        assert_eq!(m.total(PerfPath::ZeroCopy), 3);
    }

    #[test]
    fn hit_rate_calculation() {
        let m = PerfHitMetrics::new();
        m.record_hits(PerfPath::Simd, 7);
        m.record_misses(PerfPath::Simd, 3);
        assert!((m.hit_rate(PerfPath::Simd) - 0.7).abs() < 0.001);
    }

    #[test]
    fn empty_metrics_zero_rate() {
        let m = PerfHitMetrics::new();
        assert_eq!(m.hit_rate(PerfPath::IoUring), 0.0);
        assert_eq!(m.total(PerfPath::IoUring), 0);
    }

    #[test]
    fn all_four_paths_independent() {
        let m = PerfHitMetrics::new();
        m.record_hit(PerfPath::ZeroCopy);
        m.record_hit(PerfPath::Simd);
        m.record_hit(PerfPath::IoUring);
        m.record_hit(PerfPath::ExecutorOpt);
        assert_eq!(m.hit(PerfPath::ZeroCopy), 1);
        assert_eq!(m.hit(PerfPath::Simd), 1);
        assert_eq!(m.hit(PerfPath::IoUring), 1);
        assert_eq!(m.hit(PerfPath::ExecutorOpt), 1);
    }

    #[test]
    fn render_prometheus_format() {
        let m = PerfHitMetrics::new();
        m.record_hit(PerfPath::ZeroCopy);
        m.record_miss(PerfPath::ZeroCopy);
        let output = m.render();
        assert!(output.contains("sz_orm_perf_zero_copy_hit 1"));
        assert!(output.contains("sz_orm_perf_zero_copy_miss 1"));
        assert!(output.contains("sz_orm_perf_zero_copy_hit_ratio 0.5"));
    }

    #[test]
    fn shared_arc_thread_safe() {
        let m = PerfHitMetrics::shared();
        let m2 = m.clone();
        std::thread::spawn(move || {
            for _ in 0..100 {
                m2.record_hit(PerfPath::ExecutorOpt);
            }
        })
        .join()
        .unwrap();
        assert_eq!(m.hit(PerfPath::ExecutorOpt), 100);
    }

    // ========================================================================
    // v7.6.0 任务 1.7：四项加速可观测指标测试
    // ========================================================================

    #[test]
    fn test_v760_simd_aggregate_speedup() {
        let m = PerfHitMetrics::new();
        m.v760().set_simd_aggregate_speedup(2.5);
        assert!((m.v760().simd_aggregate_speedup() - 2.5).abs() < 1e-9);
    }

    #[test]
    fn test_v760_simd_filter_bitmap_speedup() {
        let m = PerfHitMetrics::new();
        m.v760().set_simd_filter_bitmap_speedup(2.2);
        assert!((m.v760().simd_filter_bitmap_speedup() - 2.2).abs() < 1e-9);
    }

    #[test]
    fn test_v760_simd_misses() {
        let m = PerfHitMetrics::new();
        m.v760().record_simd_miss();
        m.v760().record_simd_miss();
        assert_eq!(m.v760().simd_misses(), 2);
    }

    #[test]
    fn test_v760_zero_copy_array_ref_hits() {
        let m = PerfHitMetrics::new();
        m.v760().record_zero_copy_array_ref_hit();
        m.v760().record_zero_copy_array_ref_hit();
        m.v760().record_zero_copy_array_ref_hit();
        assert_eq!(m.v760().zero_copy_array_ref_hits(), 3);
    }

    #[test]
    fn test_v760_zero_copy_object_ref_hits() {
        let m = PerfHitMetrics::new();
        m.v760().record_zero_copy_object_ref_hit();
        assert_eq!(m.v760().zero_copy_object_ref_hits(), 1);
    }

    #[test]
    fn test_v760_heap_reduction_rate() {
        let m = PerfHitMetrics::new();
        m.v760().set_zero_copy_heap_reduction_rate(0.85);
        assert!((m.v760().zero_copy_heap_reduction_rate() - 0.85).abs() < 1e-9);
    }

    #[test]
    fn test_v760_io_uring_hits_fallbacks() {
        let m = PerfHitMetrics::new();
        m.v760().record_io_uring_hit();
        m.v760().record_io_uring_hit();
        m.v760().record_io_uring_fallback();
        assert_eq!(m.v760().io_uring_hits(), 2);
        assert_eq!(m.v760().io_uring_fallbacks(), 1);
    }

    #[test]
    fn test_v760_io_uring_throughput_ratio() {
        let m = PerfHitMetrics::new();
        m.v760().set_io_uring_throughput_ratio(1.15);
        assert!((m.v760().io_uring_throughput_ratio() - 1.15).abs() < 1e-9);
    }

    #[test]
    fn test_v760_pool_adaptive_tuning() {
        let m = PerfHitMetrics::new();
        m.v760().record_pool_adaptive_tuning();
        m.v760().record_pool_adaptive_tuning();
        assert_eq!(m.v760().pool_adaptive_tuning_count(), 2);
    }

    #[test]
    fn test_v760_pool_shrink_expand() {
        let m = PerfHitMetrics::new();
        m.v760().record_pool_shrink();
        m.v760().record_pool_expand();
        assert_eq!(m.v760().pool_shrink_count(), 1);
        assert_eq!(m.v760().pool_expand_count(), 1);
    }

    #[test]
    fn test_v760_pool_failover_error_rate() {
        let m = PerfHitMetrics::new();
        m.v760().set_pool_failover_error_rate(0.02);
        assert!((m.v760().pool_failover_error_rate() - 0.02).abs() < 1e-9);
    }

    #[test]
    fn test_v760_render_prometheus() {
        let m = PerfHitMetrics::new();
        m.v760().set_simd_aggregate_speedup(2.5);
        m.v760().record_zero_copy_array_ref_hit();
        m.v760().record_io_uring_hit();
        m.v760().record_pool_adaptive_tuning();
        let output = m.render();
        assert!(output.contains("sz_orm_perf_simd_aggregate_speedup 2.5"));
        assert!(output.contains("sz_orm_perf_zero_copy_array_ref_hits_total 1"));
        assert!(output.contains("sz_orm_perf_io_uring_hits_total 1"));
        assert!(output.contains("sz_orm_perf_pool_adaptive_tuning_total 1"));
    }

    #[test]
    fn test_v760_all_metrics_non_empty_after_use() {
        let m = PerfHitMetrics::new();
        m.v760().set_simd_aggregate_speedup(2.5);
        m.v760().set_simd_filter_bitmap_speedup(2.2);
        m.v760().record_simd_miss();
        m.v760().record_zero_copy_array_ref_hit();
        m.v760().record_zero_copy_object_ref_hit();
        m.v760().set_zero_copy_heap_reduction_rate(0.8);
        m.v760().record_io_uring_hit();
        m.v760().record_io_uring_fallback();
        m.v760().set_io_uring_throughput_ratio(1.15);
        m.v760().record_pool_adaptive_tuning();
        m.v760().record_pool_shrink();
        m.v760().record_pool_expand();
        m.v760().set_pool_failover_error_rate(0.05);
        let output = m.render();
        assert!(!output.is_empty());
        assert!(output.contains("sz_orm_perf_simd_aggregate_speedup"));
        assert!(output.contains("sz_orm_perf_simd_filter_bitmap_speedup"));
        assert!(output.contains("sz_orm_perf_simd_misses_total"));
        assert!(output.contains("sz_orm_perf_zero_copy_array_ref_hits_total"));
        assert!(output.contains("sz_orm_perf_zero_copy_object_ref_hits_total"));
        assert!(output.contains("sz_orm_perf_zero_copy_heap_reduction_rate"));
        assert!(output.contains("sz_orm_perf_io_uring_hits_total"));
        assert!(output.contains("sz_orm_perf_io_uring_fallbacks_total"));
        assert!(output.contains("sz_orm_perf_io_uring_throughput_ratio"));
        assert!(output.contains("sz_orm_perf_pool_adaptive_tuning_total"));
        assert!(output.contains("sz_orm_perf_pool_shrink_count_total"));
        assert!(output.contains("sz_orm_perf_pool_expand_count_total"));
        assert!(output.contains("sz_orm_perf_pool_failover_error_rate"));
    }

    #[test]
    fn test_v770_self_heal_metrics() {
        let m = PerfHitMetrics::new();
        m.v770().record_self_heal_trigger();
        m.v770().record_self_heal_success();
        m.v770().record_self_heal_fail();
        m.v770().set_self_heal_recovery_time_ms(1500.0);
        assert_eq!(m.v770().self_heal_trigger_count(), 1);
        assert_eq!(m.v770().self_heal_success_count(), 1);
        assert_eq!(m.v770().self_heal_fail_count(), 1);
        assert!((m.v770().self_heal_recovery_time_ms() - 1500.0).abs() < 1e-9);
    }

    #[test]
    fn test_v770_circuit_breaker_metrics() {
        let m = PerfHitMetrics::new();
        m.v770().record_circuit_breaker_trigger();
        m.v770().record_degrade_non_core();
        m.v770().record_core_link_protected();
        assert_eq!(m.v770().circuit_breaker_trigger_count(), 1);
        assert_eq!(m.v770().degrade_non_core_count(), 1);
        assert_eq!(m.v770().core_link_protected_count(), 1);
    }

    #[test]
    fn test_v770_slow_query_metrics() {
        let m = PerfHitMetrics::new();
        m.v770().record_slow_query_governed();
        m.v770().record_slow_query_rollback();
        m.v770().set_slow_query_governance_latency_ms(50.0);
        assert_eq!(m.v770().slow_query_governed_count(), 1);
        assert_eq!(m.v770().slow_query_rollback_count(), 1);
        assert!((m.v770().slow_query_governance_latency_ms() - 50.0).abs() < 1e-9);
    }

    #[test]
    fn test_v770_pool_tuning_metrics() {
        let m = PerfHitMetrics::new();
        m.v770().record_dynamic_pool_tuning();
        m.v770().record_multi_pool_isolated();
        m.v770().set_pool_tuning_response_time_ms(800.0);
        assert_eq!(m.v770().dynamic_pool_tuning_count(), 1);
        assert_eq!(m.v770().multi_pool_isolated_count(), 1);
        assert!((m.v770().pool_tuning_response_time_ms() - 800.0).abs() < 1e-9);
    }

    #[test]
    fn test_v770_cache_optimize_metrics() {
        let m = PerfHitMetrics::new();
        m.v770().set_cache_hit_rate_before(0.70);
        m.v770().set_cache_hit_rate_after(0.76);
        m.v770().set_cache_optimize_improvement_pp(6.0);
        assert!((m.v770().cache_hit_rate_before() - 0.70).abs() < 1e-9);
        assert!((m.v770().cache_hit_rate_after() - 0.76).abs() < 1e-9);
        assert!((m.v770().cache_optimize_improvement_pp() - 6.0).abs() < 1e-9);
    }

    #[test]
    fn test_v770_plan_optimize_metrics() {
        let m = PerfHitMetrics::new();
        m.v770().record_plan_auto_optimize();
        m.v770().set_plan_optimize_p95_improvement(12.0);
        m.v770().set_plan_optimize_decision_latency_ms(45.0);
        assert_eq!(m.v770().plan_auto_optimize_count(), 1);
        assert!((m.v770().plan_optimize_p95_improvement() - 12.0).abs() < 1e-9);
        assert!((m.v770().plan_optimize_decision_latency_ms() - 45.0).abs() < 1e-9);
    }

    #[test]
    fn test_v770_render_prometheus() {
        let m = PerfHitMetrics::new();
        m.v770().record_self_heal_trigger();
        m.v770().record_circuit_breaker_trigger();
        m.v770().record_slow_query_governed();
        m.v770().record_dynamic_pool_tuning();
        m.v770().set_cache_hit_rate_before(0.60);
        m.v770().set_cache_hit_rate_after(0.66);
        m.v770().record_plan_auto_optimize();
        let output = m.render();
        assert!(output.contains("sz_orm_self_heal_trigger_total 1"));
        assert!(output.contains("sz_orm_circuit_breaker_trigger_total 1"));
        assert!(output.contains("sz_orm_slow_query_governed_total 1"));
        assert!(output.contains("sz_orm_dynamic_pool_tuning_total 1"));
        assert!(output.contains("sz_orm_cache_hit_rate_before 0.6"));
        assert!(output.contains("sz_orm_cache_hit_rate_after 0.66"));
        assert!(output.contains("sz_orm_plan_auto_optimize_total 1"));
    }

    #[test]
    fn test_v770_all_metrics_non_empty_after_use() {
        let m = PerfHitMetrics::new();
        m.v770().record_self_heal_trigger();
        m.v770().record_self_heal_success();
        m.v770().record_self_heal_fail();
        m.v770().set_self_heal_recovery_time_ms(1000.0);
        m.v770().record_circuit_breaker_trigger();
        m.v770().record_degrade_non_core();
        m.v770().record_core_link_protected();
        m.v770().record_slow_query_governed();
        m.v770().record_slow_query_rollback();
        m.v770().set_slow_query_governance_latency_ms(30.0);
        m.v770().record_dynamic_pool_tuning();
        m.v770().record_multi_pool_isolated();
        m.v770().set_pool_tuning_response_time_ms(500.0);
        m.v770().set_cache_hit_rate_before(0.65);
        m.v770().set_cache_hit_rate_after(0.72);
        m.v770().set_cache_optimize_improvement_pp(7.0);
        m.v770().record_plan_auto_optimize();
        m.v770().set_plan_optimize_p95_improvement(15.0);
        m.v770().set_plan_optimize_decision_latency_ms(80.0);
        let output = m.render();
        assert!(output.contains("sz_orm_self_heal_trigger_total"));
        assert!(output.contains("sz_orm_self_heal_success_total"));
        assert!(output.contains("sz_orm_self_heal_fail_total"));
        assert!(output.contains("sz_orm_self_heal_recovery_time_ms"));
        assert!(output.contains("sz_orm_circuit_breaker_trigger_total"));
        assert!(output.contains("sz_orm_degrade_non_core_total"));
        assert!(output.contains("sz_orm_core_link_protected_total"));
        assert!(output.contains("sz_orm_slow_query_governed_total"));
        assert!(output.contains("sz_orm_slow_query_rollback_total"));
        assert!(output.contains("sz_orm_slow_query_governance_latency_ms"));
        assert!(output.contains("sz_orm_dynamic_pool_tuning_total"));
        assert!(output.contains("sz_orm_multi_pool_isolated_total"));
        assert!(output.contains("sz_orm_pool_tuning_response_time_ms"));
        assert!(output.contains("sz_orm_cache_hit_rate_before"));
        assert!(output.contains("sz_orm_cache_hit_rate_after"));
        assert!(output.contains("sz_orm_cache_optimize_improvement_pp"));
        assert!(output.contains("sz_orm_plan_auto_optimize_total"));
        assert!(output.contains("sz_orm_plan_optimize_p95_improvement"));
        assert!(output.contains("sz_orm_plan_optimize_decision_latency_ms"));
    }
}
