//! AI-INDEX-01 接线验证测试（v6.8.0）
//!
//! 验证索引推荐 → ReplayValidator → 回放结果 端到端管线。

use sz_orm_advisor::replay_validator::{IndexCandidate, ReplayValidator, RiskLevel, WorkloadQuery};

fn make_candidate(table: &str, col: &str, benefit: f64, risk: RiskLevel) -> IndexCandidate {
    IndexCandidate {
        table: table.to_string(),
        columns: vec![col.to_string()],
        ddl: format!("CREATE INDEX idx_{}_{} ON {} ({})", table, col, table, col),
        expected_benefit_pct: benefit,
        risk,
    }
}

fn make_workload() -> Vec<WorkloadQuery> {
    vec![
        WorkloadQuery {
            sql: "SELECT * FROM orders WHERE user_id = 1".to_string(),
            elapsed_ms: 500,
            uses_index: false,
        },
        WorkloadQuery {
            sql: "SELECT * FROM orders WHERE user_id = 2".to_string(),
            elapsed_ms: 480,
            uses_index: false,
        },
        WorkloadQuery {
            sql: "SELECT * FROM orders WHERE status = 'paid'".to_string(),
            elapsed_ms: 600,
            uses_index: false,
        },
    ]
}

#[test]
fn wiring_candidate_with_benefit_verified() {
    let mut validator = ReplayValidator::new(make_workload());
    validator.register_table("orders", 200000, vec!["id".to_string()]);
    let result =
        validator.validate_candidate(make_candidate("orders", "user_id", 80.0, RiskLevel::Low));
    assert!(result.verified);
    assert!(!result.static_only);
    assert!(result.measured_benefit_pct > 0.0);
    assert!(result.candidate.ddl.contains("CREATE INDEX"));
    assert!(result.candidate.ddl.contains("user_id"));
}

#[test]
fn wiring_candidate_without_stats_static_only() {
    let validator = ReplayValidator::new(make_workload());
    let result =
        validator.validate_candidate(make_candidate("orders", "user_id", 75.0, RiskLevel::Low));
    assert!(result.static_only);
    assert!(result.replay_error.is_some());
    assert_eq!(result.measured_benefit_pct, 75.0);
}

#[test]
fn wiring_batch_validation_all_candidates() {
    let mut validator = ReplayValidator::new(make_workload());
    validator.register_table("orders", 200000, vec!["id".to_string()]);
    let candidates = vec![
        make_candidate("orders", "user_id", 80.0, RiskLevel::Low),
        make_candidate("orders", "status", 60.0, RiskLevel::Low),
        make_candidate("orders", "created_at", 40.0, RiskLevel::Medium),
    ];
    let results = validator.validate_batch(candidates);
    assert_eq!(results.len(), 3);
    assert!(results.iter().all(|r| r.verified));
    assert!(results.iter().all(|r| r.measured_benefit_pct > 0.0));
}

#[test]
fn wiring_ddl_contains_table_and_column() {
    let mut validator = ReplayValidator::new(make_workload());
    validator.register_table("orders", 1000, vec![]);
    let result =
        validator.validate_candidate(make_candidate("orders", "user_id", 80.0, RiskLevel::Low));
    assert!(result.candidate.ddl.contains("orders"));
    assert!(result.candidate.ddl.contains("user_id"));
}

#[test]
fn wiring_risk_level_preserved() {
    let validator = ReplayValidator::new(make_workload());
    let result = validator.validate_candidate(make_candidate("orders", "x", 50.0, RiskLevel::High));
    assert_eq!(result.candidate.risk, RiskLevel::High);
}

#[test]
fn wiring_existing_index_detected() {
    let mut validator = ReplayValidator::new(make_workload());
    validator.register_table("orders", 1000, vec!["user_id".to_string()]);
    let result =
        validator.validate_candidate(make_candidate("orders", "user_id", 80.0, RiskLevel::Low));
    assert!(result.verified);
    assert_eq!(result.measured_benefit_pct, 0.0);
    assert!(result.replay_error.is_some());
}

#[test]
fn wiring_workload_summary_reports_full_scans() {
    let validator = ReplayValidator::new(make_workload());
    let summary = validator.workload_summary();
    assert_eq!(summary.total_queries, 3);
    assert_eq!(summary.full_scan_count, 3);
}

#[test]
fn wiring_expected_benefit_in_static_mode() {
    let validator = ReplayValidator::new(make_workload());
    let result =
        validator.validate_candidate(make_candidate("orders", "x", 65.5, RiskLevel::Medium));
    assert!(result.static_only);
    assert_eq!(result.measured_benefit_pct, 65.5);
}

#[test]
fn wiring_no_matching_workload_zero_benefit() {
    let mut validator = ReplayValidator::new(vec![]);
    validator.register_table("orders", 1000, vec![]);
    let result = validator.validate_candidate(make_candidate("orders", "x", 80.0, RiskLevel::Low));
    assert!(result.verified);
    assert_eq!(result.measured_benefit_pct, 0.0);
}

#[test]
fn wiring_multiple_risk_levels() {
    let validator = ReplayValidator::new(make_workload());
    for risk in [RiskLevel::Low, RiskLevel::Medium, RiskLevel::High] {
        let result =
            validator.validate_candidate(make_candidate("orders", "x", 50.0, risk.clone()));
        assert_eq!(result.candidate.risk, risk);
    }
}

#[test]
fn wiring_larger_table_higher_benefit() {
    let workload = make_workload();
    let mut validator_small = ReplayValidator::new(workload.clone());
    validator_small.register_table("orders", 1000, vec![]);
    let mut validator_large = ReplayValidator::new(workload);
    validator_large.register_table("orders", 1000000, vec![]);

    let candidate = make_candidate("orders", "user_id", 80.0, RiskLevel::Low);
    let small_result = validator_small.validate_candidate(candidate.clone());
    let large_result = validator_large.validate_candidate(candidate);
    assert!(large_result.measured_benefit_pct >= small_result.measured_benefit_pct);
}
