//! v9.8.0 查询执行计划自适应优化基准测试。
//!
//! 运行：cargo test -p sz-orm-adaptive --test plan_regression_bench -- --ignored

use std::time::Duration;

use sz_orm_adaptive::executor::{ExecutionPath, IndexSelectionStrategy, JoinOrderStrategy};
use sz_orm_adaptive::planner::{
    ExecutionPlanCache, IndexAdvisor, PlanExecutionRegressionDetector, PlanSwitcher, QueryPlan,
    RegressionConfig, SwitchResult,
};

fn make_plan(key: &str, cost: u64) -> QueryPlan {
    QueryPlan {
        query_key: key.to_string(),
        execution_path: ExecutionPath::Normal,
        index_strategy: IndexSelectionStrategy::FullScan,
        join_order: JoinOrderStrategy::LeftDeep,
        batch_size: 100,
        estimated_cost_ms: cost,
        reason: "bench".to_string(),
    }
}

#[test]
#[ignore = "benchmark: plan regression detection"]
fn bench_plan_regression_detection() {
    let config = RegressionConfig {
        enabled: true,
        regression_multiplier: 3.0,
        detection_window: 10,
        detection_sensitivity: 3,
        ..RegressionConfig::default()
    };
    let detector = PlanExecutionRegressionDetector::new(config);
    for ms in [50, 48, 52] {
        detector.record("q1", "p1", ms);
    }
    let mut detected = None;
    for i in 0..5 {
        if let Some(ev) = detector.record("q1", "p1", 200) {
            detected = Some((i + 1, ev));
            break;
        }
    }
    assert!(detected.is_some());
    let (exec_count, ev) = detected.unwrap();
    assert!(exec_count <= 3);
    println!(
        "bench_plan_regression_detection: detected after {exec_count} execs, multiplier={:.2}",
        ev.multiplier
    );
}

#[test]
#[ignore = "benchmark: plan switch effect"]
fn bench_plan_switch_effect() {
    let mut cache = ExecutionPlanCache::new(10000);
    cache.set_time(0);
    cache.put(make_plan("q1", 200));
    let switcher = PlanSwitcher::new(3, 10.0, Duration::from_secs(30));
    let retest = vec![80, 85, 90];
    let result = switcher.switch("q1", "p1", "p2", &mut cache, &retest);
    assert!(matches!(result, SwitchResult::Switched { .. }));
    let cached = cache.get("q1").unwrap();
    assert!(cached.estimated_cost_ms < 200);
    println!(
        "bench_plan_switch_effect: switched, new cost={}ms",
        cached.estimated_cost_ms
    );
}

#[test]
#[ignore = "benchmark: index advisor generate"]
fn bench_index_advisor_generate() {
    let advisor = IndexAdvisor::new(100, 5);
    for _ in 0..101 {
        advisor.record_full_scan("t1");
    }
    let suggestions = advisor.advise("t1", &["col_a".to_string()]);
    assert_eq!(suggestions.len(), 1);
    assert!(suggestions[0].safe);
    assert!(suggestions[0].columns.contains(&"col_a".to_string()));

    let bad_table = "'; DROP TABLE--";
    for _ in 0..101 {
        advisor.record_full_scan(bad_table);
    }
    let bad = advisor.advise(bad_table, &["col_a".to_string()]);
    assert!(!bad.is_empty());
    assert!(!bad[0].safe);
    println!(
        "bench_index_advisor_generate: safe suggestion={:?}, injection rejected={}",
        suggestions[0], !bad[0].safe
    );
}
