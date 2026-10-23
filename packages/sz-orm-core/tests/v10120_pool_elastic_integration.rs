//! v10.12.0 T1.1 + T1.2 + T1.3: 多维信号采集 + 负载评分 + 动态水位线集成测试

#![cfg(feature = "pool-elastic")]

use std::sync::Arc;
use std::time::Duration;

use sz_orm_core::pool_elastic::{
    AdjustDirection, DynamicWaterline, ElasticConfig, HysteresisWindow, LoadLevel, LoadScore,
    LoadScorer, LoadSignal, MultiSignalCollector, PoolElasticController, SignalConfig,
    SignalWeight, WaterlineConfig,
};

#[test]
fn test_multi_signal_collect_throughput() {
    let collector = MultiSignalCollector::new(SignalConfig::default());
    for _ in 0..100 {
        collector.record_op(Duration::from_millis(5), Duration::from_micros(100), false);
    }
    let signal = collector.collect();
    assert!(signal.qps > 0.0, "QPS should be positive after 100 ops");
}

#[test]
fn test_multi_signal_collect_acquire_latency() {
    let collector = MultiSignalCollector::new(SignalConfig::default());
    for i in 0..50 {
        collector.record_op(
            Duration::from_millis(i + 1),
            Duration::from_micros(200),
            false,
        );
    }
    let signal = collector.collect();
    assert!(
        signal.p99_latency >= Duration::from_millis(49),
        "P99 latency should be near max: {:?}",
        signal.p99_latency
    );
}

#[test]
fn test_multi_signal_collect_error_rate() {
    let collector = MultiSignalCollector::new(SignalConfig::default());
    for i in 0..100 {
        collector.record_op(
            Duration::from_millis(5),
            Duration::from_micros(100),
            i % 10 == 0,
        );
    }
    let signal = collector.collect();
    assert!(
        (signal.error_rate - 0.1).abs() < 0.01,
        "Error rate should be ~0.1: {}",
        signal.error_rate
    );
}

#[test]
fn test_multi_signal_collect_wait_duration() {
    let collector = MultiSignalCollector::new(SignalConfig::default());
    for _ in 0..10 {
        collector.record_op(Duration::from_millis(5), Duration::from_millis(50), false);
    }
    let signal = collector.collect();
    assert_eq!(
        signal.avg_wait_duration,
        Duration::from_millis(50),
        "Avg wait duration should be 50ms"
    );
}

#[test]
fn test_multi_signal_all_zero() {
    let collector = MultiSignalCollector::new(SignalConfig::default());
    let signal = collector.collect();
    assert_eq!(signal.qps, 0.0);
    assert_eq!(signal.p99_latency, Duration::ZERO);
    assert_eq!(signal.error_rate, 0.0);
    assert_eq!(signal.avg_wait_duration, Duration::ZERO);
}

#[test]
fn test_multi_signal_all_max() {
    let config = SignalConfig {
        collect_interval: Duration::from_millis(1),
        latency_window: 100,
        cpu_usage_enabled: false,
    };
    let collector = MultiSignalCollector::new(config);
    for _ in 0..1000 {
        collector.record_op(Duration::from_secs(10), Duration::from_secs(5), true);
    }
    let signal = collector.collect();
    assert!(signal.qps > 0.0, "QPS should be high");
    assert_eq!(signal.error_rate, 1.0, "Error rate should be 100%");
    assert_eq!(
        signal.avg_wait_duration,
        Duration::from_secs(5),
        "Wait should be 5s"
    );
}

#[test]
fn test_signal_collector_concurrent() {
    let collector = Arc::new(MultiSignalCollector::new(SignalConfig::default()));
    let mut handles = Vec::new();
    for _ in 0..4 {
        let c = Arc::clone(&collector);
        handles.push(std::thread::spawn(move || {
            for _ in 0..100 {
                c.record_op(Duration::from_millis(5), Duration::from_micros(50), false);
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    let signal = collector.collect();
    assert!(signal.qps > 0.0, "Concurrent QPS should be positive");
}

#[test]
fn test_collector_failure_fallback() {
    let config = SignalConfig {
        cpu_usage_enabled: false,
        ..SignalConfig::default()
    };
    let collector = MultiSignalCollector::new(config);
    collector.record_op(Duration::from_millis(10), Duration::from_micros(100), false);
    let signal = collector.collect();
    assert_eq!(signal.cpu_usage, 0.0, "CPU disabled should fallback to 0.0");
    assert!(signal.qps > 0.0, "QPS should still work");
}

#[test]
fn test_last_signal_without_collect() {
    let collector = MultiSignalCollector::new(SignalConfig::default());
    let initial = collector.last_signal();
    assert_eq!(initial.qps, 0.0);
    collector.record_op(Duration::from_millis(5), Duration::from_micros(100), false);
    let _ = collector.collect();
    let after = collector.last_signal();
    assert!(after.qps > 0.0, "last_signal should reflect collected data");
}

#[test]
fn test_pool_elastic_controller_with_signal_collector() {
    let config = ElasticConfig {
        min_connections: 2,
        max_connections: 20,
        scale_up_threshold: 5,
        scale_down_threshold: 3,
        ..Default::default()
    };
    let collector = MultiSignalCollector::new(SignalConfig::default());
    let controller = PoolElasticController::new(config).with_signal_collector(collector);
    assert!(controller.signal_collector().is_some());
    let signal = controller.collect_signal();
    assert!(signal.is_some());
}

#[test]
fn test_pool_elastic_controller_without_signal_collector() {
    let config = ElasticConfig {
        min_connections: 2,
        max_connections: 20,
        scale_up_threshold: 5,
        scale_down_threshold: 3,
        ..Default::default()
    };
    let controller = PoolElasticController::new(config);
    assert!(controller.signal_collector().is_none());
    assert!(controller.collect_signal().is_none());
}
// ── T1.2: LoadScorer tests ─────────────────────────────────────

#[test]
fn test_load_score_low_level() {
    let scorer = LoadScorer::new(SignalWeight::default());
    let signal = LoadSignal::zero();
    let score = scorer.compute_score(&signal);
    assert_eq!(score.level, LoadLevel::Low);
    assert!(score.raw < 0.3);
}

#[test]
fn test_load_score_high_level() {
    let scorer = LoadScorer::new(SignalWeight::default());
    let signal = LoadSignal {
        qps: 0.8,
        p99_latency: Duration::from_millis(800),
        error_rate: 0.05,
        avg_wait_duration: Duration::from_millis(100),
        cpu_usage: 0.8,
    };
    let score = scorer.compute_score(&signal);
    assert!(score.raw >= 0.3, "raw should be >= 0.3: {}", score.raw);
}

#[test]
fn test_load_score_overload_level() {
    let scorer = LoadScorer::new(SignalWeight::default());
    let signal = LoadSignal {
        qps: 10.0,
        p99_latency: Duration::from_secs(10),
        error_rate: 1.0,
        avg_wait_duration: Duration::from_secs(5),
        cpu_usage: 1.0,
    };
    let score = scorer.compute_score(&signal);
    assert_eq!(score.level, LoadLevel::Overload);
    assert!(score.raw >= 0.9);
}

#[test]
fn test_load_score_weight_normalization() {
    let weight = SignalWeight {
        qps: 50.0,
        p99_latency: 50.0,
        error_rate: 40.0,
        wait_duration: 30.0,
        cpu_usage: 30.0,
    };
    let scorer = LoadScorer::new(weight);
    let signal = LoadSignal {
        qps: 1.0,
        p99_latency: Duration::ZERO,
        error_rate: 0.0,
        avg_wait_duration: Duration::ZERO,
        cpu_usage: 0.0,
    };
    let score = scorer.compute_score(&signal);
    assert!(
        score.raw > 0.0 && score.raw <= 1.0,
        "Normalized score: {}",
        score.raw
    );
}

#[test]
fn test_load_score_deterministic() {
    let scorer = LoadScorer::new(SignalWeight::default());
    let signal = LoadSignal {
        qps: 0.5,
        p99_latency: Duration::from_millis(500),
        error_rate: 0.1,
        avg_wait_duration: Duration::from_millis(50),
        cpu_usage: 0.5,
    };
    let s1 = scorer.compute_score(&signal);
    let s2 = scorer.compute_score(&signal);
    assert_eq!(s1.raw, s2.raw);
    assert_eq!(s1.level, s2.level);
}

#[test]
fn test_pool_controller_with_load_scorer() {
    let config = ElasticConfig::default();
    let collector = MultiSignalCollector::new(SignalConfig::default());
    let scorer = LoadScorer::new(SignalWeight::default());
    let controller = PoolElasticController::new(config)
        .with_signal_collector(collector)
        .with_load_scorer(scorer);
    assert!(controller.load_scorer().is_some());
    let score = controller.compute_load_score();
    assert!(score.is_some());
}

#[test]
fn test_pool_controller_load_scorer_none() {
    let config = ElasticConfig::default();
    let controller = PoolElasticController::new(config);
    assert!(controller.load_scorer().is_none());
    assert!(controller.compute_load_score().is_none());
}
// ============================================================================
// T1.3: 动态水位线测试
// ============================================================================

fn make_waterline_config() -> WaterlineConfig {
    WaterlineConfig {
        min_floor: 2,
        max_ceiling: 200,
        hysteresis_window: Duration::from_millis(1),
        scale_up_threshold: 0.7,
        scale_down_threshold: 0.3,
    }
}

#[test]
fn test_dynamic_waterline_adjust_up() {
    let wl = DynamicWaterline::new(make_waterline_config());
    let initial_max = wl.current_max();
    let score = LoadScore {
        raw: 0.9,
        level: LoadLevel::Overload,
    };
    let adj = wl.adjust(&score).unwrap();
    assert_eq!(adj.reason, AdjustDirection::Up);
    assert!(wl.current_max() > initial_max);
}

#[test]
fn test_dynamic_waterline_adjust_down() {
    let wl = DynamicWaterline::new(make_waterline_config());
    let score_up = LoadScore {
        raw: 0.9,
        level: LoadLevel::Overload,
    };
    wl.adjust(&score_up);
    let initial_max = wl.current_max();
    std::thread::sleep(Duration::from_millis(2));
    let score_down = LoadScore {
        raw: 0.1,
        level: LoadLevel::Low,
    };
    let adj = wl.adjust(&score_down).unwrap();
    assert_eq!(adj.reason, AdjustDirection::Down);
    assert!(wl.current_max() < initial_max);
}

#[test]
fn test_dynamic_waterline_no_adjust_in_mid_range() {
    let wl = DynamicWaterline::new(make_waterline_config());
    let score = LoadScore {
        raw: 0.5,
        level: LoadLevel::Mid,
    };
    let adj = wl.adjust(&score);
    assert!(adj.is_none());
}

#[test]
fn test_dynamic_waterline_bounds() {
    let config = WaterlineConfig {
        min_floor: 5,
        max_ceiling: 20,
        hysteresis_window: Duration::from_millis(1),
        scale_up_threshold: 0.7,
        scale_down_threshold: 0.3,
    };
    let wl = DynamicWaterline::new(config);
    for _ in 0..20 {
        let score = LoadScore {
            raw: 0.95,
            level: LoadLevel::Overload,
        };
        wl.adjust(&score);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(wl.current_min() <= 20);
    assert!(wl.current_max() <= 20);
    assert!(wl.current_min() >= 5);
}

#[test]
fn test_dynamic_waterline_smooth_factor() {
    let wl = DynamicWaterline::new(make_waterline_config());
    let score = LoadScore {
        raw: 0.9,
        level: LoadLevel::High,
    };
    let adj1 = wl.adjust(&score).unwrap();
    std::thread::sleep(Duration::from_millis(2));
    let adj2 = wl.adjust(&score).unwrap();
    let diff1 = adj1.new_max - adj1.new_min;
    let diff2 = adj2.new_max - adj2.new_min;
    assert!(diff2 >= diff1 / 2, "Smooth factor should limit change");
}

#[test]
fn test_waterline_no_oscillation() {
    let config = WaterlineConfig {
        min_floor: 2,
        max_ceiling: 100,
        hysteresis_window: Duration::from_secs(10),
        scale_up_threshold: 0.7,
        scale_down_threshold: 0.3,
    };
    let wl = DynamicWaterline::new(config);
    let up_score = LoadScore {
        raw: 0.9,
        level: LoadLevel::High,
    };
    let down_score = LoadScore {
        raw: 0.1,
        level: LoadLevel::Low,
    };
    let adj1 = wl.adjust(&up_score).unwrap();
    assert_eq!(adj1.reason, AdjustDirection::Up);
    let adj2 = wl.adjust(&down_score);
    assert!(adj2.is_none(), "Down should be blocked by hysteresis");
}

#[test]
fn test_hysteresis_window_allows_same_direction() {
    let mut hw = HysteresisWindow::new(Duration::from_secs(10));
    hw.record(AdjustDirection::Up);
    assert!(hw.allows(AdjustDirection::Up));
    assert!(!hw.allows(AdjustDirection::Down));
}

#[test]
fn test_hysteresis_window_allows_any_after_expiry() {
    let mut hw = HysteresisWindow::new(Duration::from_millis(1));
    hw.record(AdjustDirection::Up);
    std::thread::sleep(Duration::from_millis(2));
    assert!(hw.allows(AdjustDirection::Down));
    assert!(hw.allows(AdjustDirection::Up));
}

#[test]
fn test_hysteresis_window_blocks_none() {
    let hw = HysteresisWindow::new(Duration::from_secs(10));
    assert!(!hw.allows(AdjustDirection::None));
}

#[test]
fn test_pool_controller_with_waterline() {
    let config = ElasticConfig::default();
    let wl = DynamicWaterline::new(WaterlineConfig::default());
    let controller = PoolElasticController::new(config).with_waterline(wl);
    assert!(controller.waterline().is_some());
}

#[test]
fn test_pool_controller_without_waterline() {
    let config = ElasticConfig::default();
    let controller = PoolElasticController::new(config);
    assert!(controller.waterline().is_none());
}

#[test]
fn test_dynamic_waterline_concurrent_adjust() {
    use std::thread;
    let wl = Arc::new(DynamicWaterline::new(WaterlineConfig {
        min_floor: 2,
        max_ceiling: 100,
        hysteresis_window: Duration::from_millis(1),
        scale_up_threshold: 0.7,
        scale_down_threshold: 0.3,
    }));
    let mut handles = Vec::new();
    for _ in 0..4 {
        let w = Arc::clone(&wl);
        handles.push(thread::spawn(move || {
            let score = LoadScore {
                raw: 0.9,
                level: LoadLevel::High,
            };
            for _ in 0..5 {
                w.adjust(&score);
                std::thread::sleep(Duration::from_millis(2));
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    assert!(wl.current_max() <= 100);
    assert!(wl.current_min() >= 2);
}
// ============================================================================
// T1.4: 冷启动预热优化 HistoryPrewarmDriver 测试
// ============================================================================

#[cfg(feature = "auto-prewarm")]
mod history_prewarm {
    use super::Duration;
    use std::path::PathBuf;
    use sz_orm_core::prewarm::{
        ColdStartOptimizer, HistoryPrewarmDriver, OptimalConnRecord, WarmupHistory,
    };

    fn temp_history_path() -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "sz-orm-test-history-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        path
    }

    #[test]
    fn test_history_prewarm_load() {
        let path = temp_history_path();
        let driver = HistoryPrewarmDriver::new(path.clone());

        let history = WarmupHistory::new(10).with_hot_sql(vec![1, 2, 3]);
        driver.save_history_sync(&history).unwrap();

        let loaded = driver.load_history_sync().unwrap();
        assert_eq!(loaded.optimal_conn_count, 10);
        assert_eq!(loaded.hot_sql_fingerprints, vec![1, 2, 3]);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_history_prewarm_no_file() {
        let path = temp_history_path();
        let driver = HistoryPrewarmDriver::new(path.clone());

        let result = driver.prewarm_by_history_sync(5);
        assert_eq!(result, 5, "无历史文件时应返回默认值");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_history_prewarm_persist() {
        let path = temp_history_path();
        let driver = HistoryPrewarmDriver::new(path.clone());

        let history = WarmupHistory::new(15);
        driver.save_history_sync(&history).unwrap();

        let driver2 = HistoryPrewarmDriver::new(path.clone());
        let loaded = driver2.load_history_sync().unwrap();
        assert_eq!(loaded.optimal_conn_count, 15);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_history_prewarm_decay() {
        let history = WarmupHistory::new(20);
        let decayed = history.decay();
        assert_eq!(decayed.optimal_conn_count, 10, "衰减因子 0.5");
    }

    #[test]
    fn test_history_prewarm_merge() {
        let h1 = WarmupHistory {
            optimal_conn_count: 10,
            hot_sql_fingerprints: vec![1, 2],
            last_updated: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };
        let h2 = WarmupHistory {
            optimal_conn_count: 20,
            hot_sql_fingerprints: vec![2, 3],
            last_updated: 0,
        };

        let merged = h1.merge(&h2);
        assert!(merged.optimal_conn_count > 0);
        assert!(
            merged.hot_sql_fingerprints.len() >= 3,
            "合并后应包含所有指纹"
        );
    }

    #[test]
    fn test_history_prewarm_corrupt_recovery() {
        let path = temp_history_path();
        std::fs::write(&path, "not valid json").unwrap();

        let driver = HistoryPrewarmDriver::new(path.clone());
        let result = driver.load_history_sync();
        assert!(result.is_none(), "损坏文件应返回 None");

        let prewarm_count = driver.prewarm_by_history_sync(3);
        assert_eq!(prewarm_count, 3, "损坏文件应回退默认值");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_history_prewarm_path_create() {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "sz-orm-test-subdir-{}/history.json",
            std::process::id()
        ));
        let driver = HistoryPrewarmDriver::new(path.clone());

        let history = WarmupHistory::new(8);
        driver.save_history_sync(&history).unwrap();
        assert!(path.exists(), "文件应被创建");

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(path.parent().unwrap());
    }

    #[test]
    fn test_cold_start_with_history_driver() {
        let path = temp_history_path();
        let opt = ColdStartOptimizer::new_with_history_driver(
            Duration::from_millis(150),
            2,
            path.clone(),
        );

        let history = WarmupHistory::new(12);
        opt.history_driver()
            .unwrap()
            .save_history_sync(&history)
            .unwrap();

        let stats = opt.on_cold_start();
        assert_eq!(stats.warmed_connections, 12, "应按历史最优连接数预热");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_cold_start_without_history_fallback() {
        let path = temp_history_path();
        let opt = ColdStartOptimizer::new_with_history_driver(
            Duration::from_millis(150),
            5,
            path.clone(),
        );

        let stats = opt.on_cold_start();
        assert_eq!(stats.warmed_connections, 5, "无历史数据时应回退默认预热");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_cold_start_without_history_driver() {
        let opt = ColdStartOptimizer::new(Duration::from_millis(150), 3);
        assert!(opt.history_driver().is_none());

        let stats = opt.on_cold_start();
        assert_eq!(stats.warmed_connections, 3);
    }

    #[test]
    fn test_optimal_conn_record() {
        let record = OptimalConnRecord {
            conn_count: 20,
            collected_at: 1234567890,
            qps: 1000.0,
            p99_latency_ms: 50,
        };
        assert_eq!(record.conn_count, 20);
        assert_eq!(record.qps, 1000.0);
    }

    #[test]
    fn test_warmup_history_default() {
        let history = WarmupHistory::default();
        assert_eq!(history.optimal_conn_count, 2);
        assert!(history.hot_sql_fingerprints.is_empty());
    }

    #[test]
    fn test_history_driver_cached_history() {
        let path = temp_history_path();
        let driver = HistoryPrewarmDriver::new(path.clone());

        let history = WarmupHistory::new(7);
        driver.save_history_sync(&history).unwrap();

        let cached = driver.cached_history().unwrap();
        assert_eq!(cached.optimal_conn_count, 7);

        let _ = std::fs::remove_file(&path);
    }
}
// ============================================================================
// T1.5: 读写分离池独立调优测试
// ============================================================================

#[cfg(feature = "rw-split-enhanced")]
mod rw_split_adaptive {
    use super::Duration;
    use sz_orm_core::pool_elastic::{SignalConfig, WaterlineConfig};
    use sz_orm_core::rw_split_enhanced::{
        GlobalConnCoordinator, GlobalCoordResult, ReadPoolTuner, RwSplitAdaptiveTuner,
        RwSplitConfig, RwSplitRouter, WeightedSlave, WritePoolTuner,
    };

    fn make_signal_config() -> SignalConfig {
        SignalConfig::default()
    }

    fn make_waterline_config() -> WaterlineConfig {
        WaterlineConfig::default()
    }

    // --- ReadPoolTuner 测试 ---

    #[tokio::test]
    async fn test_read_pool_tuner_new() {
        let tuner = ReadPoolTuner::new(make_signal_config(), make_waterline_config());
        assert_eq!(tuner.current_size(), 0);
        assert!(tuner.current_min() <= tuner.current_max());
    }

    #[tokio::test]
    async fn test_read_pool_tuner_record_and_tune() {
        let tuner = ReadPoolTuner::new(make_signal_config(), make_waterline_config());

        for _ in 0..100 {
            tuner.record_op(Duration::from_millis(5), Duration::ZERO, false);
        }

        let result = tuner.tune().await;
        // 低负载时可能返回 None（评分在中间区间）或 Some(Down)
        // 关键是 tune 不 panic 且有返回值
        let _ = result;
    }

    #[tokio::test]
    async fn test_read_pool_tuner_set_current_size() {
        let tuner = ReadPoolTuner::new(make_signal_config(), make_waterline_config());
        tuner.set_current_size(10);
        assert_eq!(tuner.current_size(), 10);

        tuner.set_current_size(20);
        assert_eq!(tuner.current_size(), 20);
    }

    // --- WritePoolTuner 测试 ---

    #[tokio::test]
    async fn test_write_pool_tuner_new() {
        let tuner = WritePoolTuner::new(make_signal_config(), make_waterline_config());
        assert_eq!(tuner.current_size(), 0);
        assert!(tuner.current_min() <= tuner.current_max());
    }

    #[tokio::test]
    async fn test_write_pool_tuner_record_and_tune() {
        let tuner = WritePoolTuner::new(make_signal_config(), make_waterline_config());

        for _ in 0..100 {
            tuner.record_op(Duration::from_millis(10), Duration::ZERO, false);
        }

        let _ = tuner.tune().await;
    }

    // --- GlobalConnCoordinator 测试 ---

    #[test]
    fn test_global_coordinator_within_limit() {
        let coord = GlobalConnCoordinator::new(100);
        assert_eq!(coord.check_limit(30, 50), GlobalCoordResult::WithinLimit);
        assert_eq!(coord.check_limit(50, 50), GlobalCoordResult::WithinLimit);
        assert_eq!(coord.check_limit(0, 100), GlobalCoordResult::WithinLimit);
        assert_eq!(coord.total_reductions(), 0);
    }

    #[test]
    fn test_global_coordinator_over_limit() {
        let coord = GlobalConnCoordinator::new(100);
        let result = coord.check_limit(60, 50);
        match result {
            GlobalCoordResult::OverLimit {
                reduce_read,
                reduce_write,
            } => {
                assert!(reduce_read + reduce_write == 10);
            }
            _ => panic!("expected OverLimit"),
        }
        assert_eq!(coord.total_reductions(), 1);
    }

    #[test]
    fn test_global_coordinator_read_heavy_over_limit() {
        let coord = GlobalConnCoordinator::new(100);
        // 读池连接远多于写池
        let result = coord.check_limit(90, 30);
        match result {
            GlobalCoordResult::OverLimit {
                reduce_read,
                reduce_write,
            } => {
                // 读池负载更高，应缩减读池
                assert!(reduce_read >= reduce_write);
                assert_eq!(reduce_read + reduce_write, 20);
            }
            _ => panic!("expected OverLimit"),
        }
    }

    #[test]
    fn test_global_coordinator_db_max_getter() {
        let coord = GlobalConnCoordinator::new(200);
        assert_eq!(coord.db_max_connections(), 200);
    }

    #[test]
    fn test_global_coordinator_multiple_checks() {
        let coord = GlobalConnCoordinator::new(50);
        assert_eq!(coord.check_limit(20, 20), GlobalCoordResult::WithinLimit);
        assert_ne!(coord.check_limit(30, 30), GlobalCoordResult::WithinLimit);
        assert_ne!(coord.check_limit(40, 40), GlobalCoordResult::WithinLimit);
        assert_eq!(coord.total_reductions(), 2);
    }

    // --- RwSplitAdaptiveTuner 测试 ---

    #[tokio::test]
    async fn test_rw_split_adaptive_tuner_new() {
        let tuner = RwSplitAdaptiveTuner::new(
            make_signal_config(),
            make_waterline_config(),
            make_signal_config(),
            make_waterline_config(),
            100,
        );
        assert_eq!(tuner.tune_count(), 0);
        assert_eq!(tuner.global_coordinator().db_max_connections(), 100);
    }

    #[tokio::test]
    async fn test_rw_split_adaptive_tuner_tune_read() {
        let tuner = RwSplitAdaptiveTuner::new(
            make_signal_config(),
            make_waterline_config(),
            make_signal_config(),
            make_waterline_config(),
            100,
        );

        for _ in 0..50 {
            tuner
                .read_pool_tuner()
                .record_op(Duration::from_millis(5), Duration::ZERO, false);
        }

        let _ = tuner.tune_read_pool().await;
        assert_eq!(tuner.tune_count(), 1);
    }

    #[tokio::test]
    async fn test_rw_split_adaptive_tuner_tune_write() {
        let tuner = RwSplitAdaptiveTuner::new(
            make_signal_config(),
            make_waterline_config(),
            make_signal_config(),
            make_waterline_config(),
            100,
        );

        for _ in 0..50 {
            tuner
                .write_pool_tuner()
                .record_op(Duration::from_millis(10), Duration::ZERO, false);
        }

        let _ = tuner.tune_write_pool().await;
        assert_eq!(tuner.tune_count(), 1);
    }

    #[tokio::test]
    async fn test_rw_split_adaptive_tuner_independent_tuning() {
        let tuner = RwSplitAdaptiveTuner::new(
            make_signal_config(),
            make_waterline_config(),
            make_signal_config(),
            make_waterline_config(),
            100,
        );

        // 只记录读操作
        for _ in 0..100 {
            tuner
                .read_pool_tuner()
                .record_op(Duration::from_millis(5), Duration::ZERO, false);
        }
        // 写池无操作

        let read_result = tuner.tune_read_pool().await;
        let write_result = tuner.tune_write_pool().await;

        // 读池有操作，写池无操作，两者独立调优互不影响
        // 读池可能产生调整，写池不应有调整（无信号）
        let _ = (read_result, write_result);
        assert_eq!(tuner.tune_count(), 2);
    }

    #[test]
    fn test_rw_split_adaptive_tuner_check_global_limit() {
        let tuner = RwSplitAdaptiveTuner::new(
            make_signal_config(),
            make_waterline_config(),
            make_signal_config(),
            make_waterline_config(),
            100,
        );

        assert_eq!(
            tuner.check_global_limit(40, 50),
            GlobalCoordResult::WithinLimit
        );
        assert_ne!(
            tuner.check_global_limit(60, 50),
            GlobalCoordResult::WithinLimit
        );
    }

    // --- RwSplitRouter 集成测试 ---

    fn make_config() -> RwSplitConfig {
        RwSplitConfig {
            master: "master".into(),
            slaves: vec![WeightedSlave {
                name: "slave-1".into(),
                weight: 1,
                host: "127.0.0.1".into(),
                port: 3307,
            }],
            lag_threshold_secs: 5,
        }
    }

    #[tokio::test]
    async fn test_router_without_adaptive_tuner_backward_compat() {
        let router = RwSplitRouter::new(make_config());
        assert!(router.adaptive_tuner().is_none());

        let result = router.route_read();
        assert!(!result.is_master);

        let result = router.route_write();
        assert!(result.is_master);
    }

    #[tokio::test]
    async fn test_router_with_adaptive_tuner() {
        let tuner = RwSplitAdaptiveTuner::new(
            make_signal_config(),
            make_waterline_config(),
            make_signal_config(),
            make_waterline_config(),
            100,
        );
        let router = RwSplitRouter::new_with_adaptive_tuner(make_config(), tuner);
        assert!(router.adaptive_tuner().is_some());

        let result = router.route_read();
        assert!(!result.is_master);

        let result = router.route_write();
        assert!(result.is_master);
    }

    #[tokio::test]
    async fn test_router_record_read_op_and_tune() {
        let tuner = RwSplitAdaptiveTuner::new(
            make_signal_config(),
            make_waterline_config(),
            make_signal_config(),
            make_waterline_config(),
            100,
        );
        let router = RwSplitRouter::new_with_adaptive_tuner(make_config(), tuner);

        for _ in 0..100 {
            router.record_read_op(Duration::from_millis(5), Duration::ZERO, false);
        }

        let _ = router.tune_read_pool().await;
        assert!(router.adaptive_tuner().unwrap().tune_count() >= 1);
    }

    #[tokio::test]
    async fn test_router_record_write_op_and_tune() {
        let tuner = RwSplitAdaptiveTuner::new(
            make_signal_config(),
            make_waterline_config(),
            make_signal_config(),
            make_waterline_config(),
            100,
        );
        let router = RwSplitRouter::new_with_adaptive_tuner(make_config(), tuner);

        for _ in 0..100 {
            router.record_write_op(Duration::from_millis(10), Duration::ZERO, false);
        }

        let _ = router.tune_write_pool().await;
        assert!(router.adaptive_tuner().unwrap().tune_count() >= 1);
    }

    #[tokio::test]
    async fn test_router_check_global_limit() {
        let tuner = RwSplitAdaptiveTuner::new(
            make_signal_config(),
            make_waterline_config(),
            make_signal_config(),
            make_waterline_config(),
            50,
        );
        let router = RwSplitRouter::new_with_adaptive_tuner(make_config(), tuner);

        assert_eq!(
            router.check_global_limit(20, 20),
            GlobalCoordResult::WithinLimit
        );
        assert_ne!(
            router.check_global_limit(30, 30),
            GlobalCoordResult::WithinLimit
        );
    }

    #[tokio::test]
    async fn test_router_check_global_limit_without_tuner() {
        let router = RwSplitRouter::new(make_config());
        // 无调优器时返回 WithinLimit
        assert_eq!(
            router.check_global_limit(1000, 1000),
            GlobalCoordResult::WithinLimit
        );
    }

    #[tokio::test]
    async fn test_config_with_adaptive_tuner() {
        let (config, tuner) = RwSplitConfig::with_adaptive_tuner(
            "master",
            vec![WeightedSlave {
                name: "slave-1".into(),
                weight: 1,
                host: "127.0.0.1".into(),
                port: 3307,
            }],
            200,
        );
        assert_eq!(config.master, "master");
        assert_eq!(tuner.global_coordinator().db_max_connections(), 200);
    }

    #[tokio::test]
    async fn test_read_write_pool_sizes_independent() {
        let tuner = RwSplitAdaptiveTuner::new(
            make_signal_config(),
            WaterlineConfig {
                min_floor: 5,
                max_ceiling: 50,
                ..WaterlineConfig::default()
            },
            make_signal_config(),
            WaterlineConfig {
                min_floor: 2,
                max_ceiling: 20,
                ..WaterlineConfig::default()
            },
            100,
        );

        // 读池和写池有不同的水位线配置
        let read_min = tuner.read_pool_tuner().current_min();
        let write_min = tuner.write_pool_tuner().current_min();
        assert_eq!(read_min, 5);
        assert_eq!(write_min, 2);

        let read_max = tuner.read_pool_tuner().current_max();
        let write_max = tuner.write_pool_tuner().current_max();
        assert_eq!(read_max, (5 + 50) / 2);
        assert_eq!(write_max, (2 + 20) / 2);
    }
}
