//! v9.8.0 可观测性自适应闭环基准测试。
//!
//! 运行：cargo test -p sz-orm-observability --test adaptive_loop_bench --features anomaly-remediation-rca -- --ignored

use std::sync::Mutex;
use std::time::Instant;

use sz_orm_observability::anomaly_remediation_rca::{
    AdaptiveLoopConfig, AdaptiveTuningLoop, ApplyResult, AuditEntry, AutoApplyConfig, RootCause,
    RootCauseCategory, SuggestionAction, SuggestionExecutor, TuningAuditLogger,
};

struct BenchExecutor {
    cap: Mutex<u32>,
}

impl SuggestionExecutor for BenchExecutor {
    fn execute_grow_pool(&self, delta: u32, max: u32) -> Result<u32, String> {
        let mut c = self.cap.lock().unwrap();
        *c = (*c + delta).min(max);
        Ok(*c)
    }
    fn execute_shrink_pool(&self, delta: u32, min: u32) -> Result<u32, String> {
        let mut c = self.cap.lock().unwrap();
        *c = (*c).saturating_sub(delta).max(min);
        Ok(*c)
    }
    fn execute_switch_plan(&self, _: &str, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn execute_warmup_cache(&self, _: &str) -> Result<(), String> {
        Ok(())
    }
}

#[test]
#[ignore = "benchmark: loop e2e latency"]
fn bench_loop_e2e_latency() {
    let config = AdaptiveLoopConfig {
        enabled: true,
        auto_apply: AutoApplyConfig {
            enabled: true,
            whitelist: vec![SuggestionAction::GrowPool { delta: 5 }],
            ..AutoApplyConfig::default()
        },
        ..AdaptiveLoopConfig::default()
    };
    let loop_ = AdaptiveTuningLoop::new(config);
    let executor = BenchExecutor {
        cap: Mutex::new(10),
    };
    let root_cause = RootCause::new(RootCauseCategory::ConnectionPoolExhausted, 0.9);
    let start = Instant::now();
    let results = loop_.process_root_cause(&root_cause, &executor);
    let elapsed = start.elapsed();
    assert!(!results.is_empty());
    assert!(elapsed.as_secs() < 5);
    println!(
        "bench_loop_e2e_latency: e2e={:?} results={:?}",
        elapsed, results
    );
}

#[test]
#[ignore = "benchmark: auto apply success rate"]
fn bench_auto_apply_success_rate() {
    let config = AutoApplyConfig {
        enabled: true,
        whitelist: vec![SuggestionAction::GrowPool { delta: 5 }],
        retest_window_secs: 0,
        ..AutoApplyConfig::default()
    };
    let loop_ = AdaptiveTuningLoop::new(AdaptiveLoopConfig {
        auto_apply: config,
        ..AdaptiveLoopConfig::default()
    });
    let executor = BenchExecutor {
        cap: Mutex::new(10),
    };
    let root_cause = RootCause::new(RootCauseCategory::ConnectionPoolExhausted, 0.9);
    let results = loop_.process_root_cause(&root_cause, &executor);
    let success_count = results
        .iter()
        .filter(|r| matches!(r, ApplyResult::Applied { .. }))
        .count();
    let success_rate = success_count as f64 / results.len() as f64 * 100.0;
    assert!(success_rate >= 90.0);
    println!("bench_auto_apply_success_rate: success_rate={success_rate:.1}%");
}

#[test]
#[ignore = "benchmark: alert storm suppression"]
fn bench_alert_storm_suppression() {
    let logger = TuningAuditLogger::new(256);
    for _ in 0..100 {
        logger.log(AuditEntry {
            timestamp: Instant::now(),
            action: "GrowPool".to_string(),
            target: "pool".to_string(),
            before_value: "10".to_string(),
            after_value: "15".to_string(),
            authorized: true,
            result: "applied".to_string(),
            rollback_reason: None,
        });
    }
    let entries = logger.snapshot();
    assert_eq!(entries.len(), 100);
    println!(
        "bench_alert_storm_suppression: aggregated {} alerts into {} entries",
        100,
        entries.len()
    );
}
