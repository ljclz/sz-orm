//! v10.12.0 T4.3: 跨方向集成测试
//!
//! 验证三方向（连接池自适应、计划缓存增强、故障恢复加速）同时运行无冲突 + 协同。

#![cfg(any(
    feature = "pool-elastic",
    feature = "plan-cache",
    feature = "auto-failover"
))]
#![allow(unused_imports)]

use std::time::Duration;

// ============================================================================
// 跨方向集成测试
// ============================================================================

#[cfg(all(feature = "pool-elastic", feature = "plan-cache"))]
mod cross_pool_elastic_plan_cache {
    use super::Duration;
    use sz_orm_core::perf_metrics::{AdaptiveFailoverMetrics, PerfMetrics};
    use sz_orm_core::plan_cache::{HitRateMonitor, HitRateTuningConfig, LfuTracker};
    use sz_orm_core::pool_elastic::{
        DynamicWaterline, LoadScorer, MultiSignalCollector, SignalConfig, SignalWeight,
        WaterlineConfig,
    };

    /// 池扩缩容不影响计划缓存
    #[tokio::test]
    async fn test_pool_elastic_does_not_affect_plan_cache() {
        let signal_config = SignalConfig::default();
        let waterline_config = WaterlineConfig::default();

        let collector = MultiSignalCollector::new(signal_config);
        let scorer = LoadScorer::new(SignalWeight::default());
        let waterline = DynamicWaterline::new(waterline_config);

        let lfu = LfuTracker::new();
        let monitor = HitRateMonitor::new(HitRateTuningConfig::default());

        // 记录计划缓存操作
        lfu.access("sql-1");
        lfu.access("sql-2");
        monitor.record_hit_rate(0.8);
        monitor.record_hit_rate(0.9);

        // 池扩缩容操作
        for _ in 0..100 {
            collector.record_op(Duration::from_millis(5), Duration::ZERO, false);
        }
        let signal = collector.collect();
        let score = scorer.compute_score(&signal);
        let _ = waterline.adjust(&score);

        // 验证计划缓存状态未被池操作破坏
        assert_eq!(lfu.frequency("sql-1"), 1);
        assert_eq!(lfu.frequency("sql-2"), 1);
        let rate = monitor.current_hit_rate();
        assert!((rate - 0.85).abs() < 1e-9);
    }

    /// 池扩缩容与计划缓存 metrics 统一记录
    #[tokio::test]
    async fn test_cross_metrics_pool_and_plan_cache() {
        let perf = PerfMetrics::global();
        let adaptive = AdaptiveFailoverMetrics::global();

        let before_perf = perf.snapshot();
        let before_adaptive = adaptive.adaptive_snapshot();

        // 记录池操作
        perf.record_pool_acquire();
        adaptive.record_signal_sample();
        adaptive.record_waterline_change();

        // 记录计划缓存操作
        perf.record_plan_cache_hit();
        perf.record_plan_cache_miss();

        let after_perf = perf.snapshot();
        let after_adaptive = adaptive.adaptive_snapshot();

        assert!(after_perf.pool_acquire_count > before_perf.pool_acquire_count);
        assert!(after_perf.plan_cache_hits > before_perf.plan_cache_hits);
        assert!(after_adaptive.signal_samples > before_adaptive.signal_samples);
        assert!(after_adaptive.waterline_changes > before_adaptive.waterline_changes);
    }
}

#[cfg(all(feature = "pool-elastic", feature = "auto-failover"))]
mod cross_pool_elastic_failover {
    use super::Duration;
    use sz_orm_core::failover_controller::{FailoverController, FailoverDetector};
    use sz_orm_core::perf_metrics::AdaptiveFailoverMetrics;
    use sz_orm_core::pool_elastic::{
        DynamicWaterline, LoadScorer, MultiSignalCollector, SignalConfig, SignalWeight,
        WaterlineConfig,
    };

    /// 池扩缩容与故障切换协同
    #[tokio::test]
    async fn test_pool_elastic_with_failover() {
        let detector = FailoverDetector::new(Duration::from_secs(5), 0.5);
        let controller = FailoverController::new_with_recovery_coordinator(detector, 3);
        controller.set_master("master-a");
        controller.add_slave("slave-b");
        controller.set_node_health("master-a", true);
        controller.set_node_health("slave-b", true);

        let collector = MultiSignalCollector::new(SignalConfig::default());
        let scorer = LoadScorer::new(SignalWeight::default());
        let waterline = DynamicWaterline::new(WaterlineConfig::default());

        // 池信号采集
        for _ in 0..100 {
            collector.record_op(Duration::from_millis(5), Duration::ZERO, false);
        }
        let signal = collector.collect();
        let score = scorer.compute_score(&signal);
        let _ = waterline.adjust(&score);

        // 故障转移
        controller.failover("slave-b").await.unwrap();
        controller.recover("master-a").await.unwrap();

        // 验证故障恢复协调器被触发
        let coord = controller.recovery_coordinator().unwrap();
        assert_eq!(coord.recovery_count(), 1);
    }

    /// 池扩缩容与故障切换 metrics 统一记录
    #[tokio::test]
    async fn test_cross_metrics_pool_and_failover() {
        let adaptive = AdaptiveFailoverMetrics::global();
        let before = adaptive.failover_snapshot();

        adaptive.record_failover_detection();
        adaptive.record_failover_switch();
        adaptive.record_recovery_coordination();
        adaptive.record_signal_sample();

        let after = adaptive.failover_snapshot();
        assert!(after.failover_detections > before.failover_detections);
        assert!(after.failover_switches > before.failover_switches);
        assert!(after.recovery_coordinations > before.recovery_coordinations);
    }
}

#[cfg(all(feature = "plan-cache", feature = "auto-failover"))]
mod cross_plan_cache_failover {
    use super::Duration;
    use sz_orm_core::failover_controller::{FailoverController, FailoverDetector};
    use sz_orm_core::plan_cache::{HitRateMonitor, HitRateTuningConfig, LfuTracker};

    /// 计划缓存与故障切换协同
    #[tokio::test]
    async fn test_plan_cache_with_failover() {
        let detector = FailoverDetector::new(Duration::from_secs(5), 0.5);
        let controller = FailoverController::new_with_recovery_coordinator(detector, 3);
        controller.set_master("master-a");
        controller.add_slave("slave-b");
        controller.set_node_health("master-a", true);
        controller.set_node_health("slave-b", true);

        let lfu = LfuTracker::new();
        let monitor = HitRateMonitor::new(HitRateTuningConfig::default());

        // 计划缓存操作
        lfu.access("sql-1");
        lfu.access("sql-2");
        monitor.record_hit_rate(0.7);
        monitor.record_hit_rate(0.5);

        // 故障转移
        controller.failover("slave-b").await.unwrap();
        controller.recover("master-a").await.unwrap();

        // 验证计划缓存状态未被故障转移破坏
        assert_eq!(lfu.frequency("sql-1"), 1);
        assert_eq!(lfu.frequency("sql-2"), 1);
        let rate = monitor.current_hit_rate();
        assert!((rate - 0.6).abs() < 1e-9);

        // 验证故障恢复协调器被触发
        let coord = controller.recovery_coordinator().unwrap();
        assert_eq!(coord.recovery_count(), 1);
    }
}

#[cfg(all(
    feature = "pool-elastic",
    feature = "plan-cache",
    feature = "auto-failover"
))]
mod cross_all_three {
    use super::Duration;
    use sz_orm_core::failover_controller::{FailoverController, FailoverDetector};
    use sz_orm_core::perf_metrics::AdaptiveFailoverMetrics;
    use sz_orm_core::plan_cache::{HitRateMonitor, HitRateTuningConfig, LfuTracker};
    use sz_orm_core::pool_elastic::{
        DynamicWaterline, LoadScorer, MultiSignalCollector, SignalConfig, SignalWeight,
        WaterlineConfig,
    };

    /// 三方向同时运行无冲突
    #[tokio::test]
    async fn test_all_three_simultaneous() {
        // 方向1: 连接池自适应
        let collector = MultiSignalCollector::new(SignalConfig::default());
        let scorer = LoadScorer::new(SignalWeight::default());
        let waterline = DynamicWaterline::new(WaterlineConfig::default());

        // 方向2: 计划缓存
        let lfu = LfuTracker::new();
        let monitor = HitRateMonitor::new(HitRateTuningConfig::default());

        // 方向3: 故障恢复
        let detector = FailoverDetector::new(Duration::from_secs(5), 0.5);
        let controller = FailoverController::new_with_recovery_coordinator(detector, 3);
        controller.set_master("master-a");
        controller.add_slave("slave-b");
        controller.set_node_health("master-a", true);
        controller.set_node_health("slave-b", true);

        // 同时运行三方向
        for i in 0..100 {
            // 方向1: 池信号采集
            collector.record_op(Duration::from_millis(5), Duration::ZERO, false);

            // 方向2: 计划缓存操作
            lfu.access(&format!("sql-{i}"));
        }
        monitor.record_hit_rate(0.85);

        // 方向1: 水位线调整
        let signal = collector.collect();
        let score = scorer.compute_score(&signal);
        let _ = waterline.adjust(&score);

        // 方向3: 故障转移 + 恢复
        controller.failover("slave-b").await.unwrap();
        controller.recover("master-a").await.unwrap();

        // 验证三方向各自状态正确
        assert_eq!(lfu.frequency("sql-0"), 1);
        assert_eq!(lfu.frequency("sql-99"), 1);
        let rate = monitor.current_hit_rate();
        assert!((rate - 0.85).abs() < 1e-9);

        let coord = controller.recovery_coordinator().unwrap();
        assert_eq!(coord.recovery_count(), 1);
    }

    /// 三方向 metrics 统一记录
    #[tokio::test]
    async fn test_cross_metrics_all_three() {
        let metrics = AdaptiveFailoverMetrics::global();
        let before_adaptive = metrics.adaptive_snapshot();
        let before_failover = metrics.failover_snapshot();

        // 方向1: 池自适应
        metrics.record_signal_sample();
        metrics.record_waterline_change();
        metrics.record_scale_up();

        // 方向2: 计划缓存（通过 perf_metrics 记录）
        let perf = sz_orm_core::perf_metrics::PerfMetrics::global();
        let before_perf = perf.snapshot();
        perf.record_plan_cache_hit();
        perf.record_plan_cache_miss();
        let after_perf = perf.snapshot();
        assert!(after_perf.plan_cache_hits > before_perf.plan_cache_hits);

        // 方向3: 故障恢复
        metrics.record_failover_detection();
        metrics.record_failover_switch();
        metrics.record_recovery_coordination();

        let after_adaptive = metrics.adaptive_snapshot();
        let after_failover = metrics.failover_snapshot();

        assert!(after_adaptive.signal_samples > before_adaptive.signal_samples);
        assert!(after_adaptive.waterline_changes > before_adaptive.waterline_changes);
        assert!(after_failover.failover_detections > before_failover.failover_detections);
        assert!(after_failover.recovery_coordinations > before_failover.recovery_coordinations);
    }
}

#[cfg(all(feature = "pool-elastic", feature = "plan-cache"))]
mod cross_prewarm {
    use sz_orm_core::plan_cache::{HotSqlHistory, PlanCachePrewarm, PlanPrewarmConfig};
    use sz_orm_core::prewarm::WarmupHistory;

    /// 池预热 + 计划缓存预热协同
    #[tokio::test]
    async fn test_prewarm_both() {
        // 方向1: 池预热
        let pool_history = WarmupHistory::new(10);
        assert_eq!(pool_history.optimal_conn_count, 10);

        // 方向2: 计划缓存预热
        let _plan_prewarm = PlanCachePrewarm::new(PlanPrewarmConfig::default());
        let mut hot_history = HotSqlHistory::new();
        for i in 0..10 {
            hot_history.add(format!("sql-{i}"), 100 - i as u64);
        }
        let top_k = hot_history.topk(5);
        assert_eq!(top_k.len(), 5);
        assert_eq!(top_k[0].0, "sql-0");

        // 两者独立运行互不影响
        assert_eq!(pool_history.optimal_conn_count, 10);
        assert_eq!(top_k[0].0, "sql-0");
    }
}
