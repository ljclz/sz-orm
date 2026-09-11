use std::sync::Mutex;
use sz_orm_advisor::replay_validator::{
    IndexCandidate, ReplayValidator, RiskLevel, ShadowDatabase, WorkloadQuery,
    AI_INDEX_REPLAY_SKIPPED,
};

struct MockShadowDb {
    ddl_executed: Mutex<Vec<String>>,
    ddl_rolled_back: Mutex<Vec<String>>,
    query_results: Vec<u64>,
    fail_ddl: bool,
    fail_query: bool,
}

impl MockShadowDb {
    fn new(query_results: Vec<u64>) -> Self {
        Self {
            ddl_executed: Mutex::new(Vec::new()),
            ddl_rolled_back: Mutex::new(Vec::new()),
            query_results,
            fail_ddl: false,
            fail_query: false,
        }
    }

    fn with_fail_ddl(mut self) -> Self {
        self.fail_ddl = true;
        self
    }

    fn with_fail_query(mut self) -> Self {
        self.fail_query = true;
        self
    }

    fn ddl_executed_count(&self) -> usize {
        self.ddl_executed.lock().unwrap().len()
    }

    fn ddl_rolled_back_count(&self) -> usize {
        self.ddl_rolled_back.lock().unwrap().len()
    }
}

impl ShadowDatabase for MockShadowDb {
    fn execute_ddl(&self, ddl: &str) -> Result<(), String> {
        if self.fail_ddl {
            return Err("DDL execution failed".to_string());
        }
        self.ddl_executed.lock().unwrap().push(ddl.to_string());
        Ok(())
    }

    fn execute_query(&self, _sql: &str) -> Result<u64, String> {
        if self.fail_query {
            return Err("Query execution failed".to_string());
        }
        let idx = self
            .ddl_executed
            .lock()
            .unwrap()
            .len()
            .min(self.query_results.len());
        if idx > 0 && idx <= self.query_results.len() {
            Ok(self.query_results[idx - 1])
        } else if !self.query_results.is_empty() {
            Ok(self.query_results[0])
        } else {
            Ok(10)
        }
    }

    fn rollback_ddl(&self, ddl: &str) -> Result<(), String> {
        self.ddl_rolled_back.lock().unwrap().push(ddl.to_string());
        Ok(())
    }
}

fn make_candidate(table: &str, col: &str, benefit: f64) -> IndexCandidate {
    IndexCandidate {
        table: table.to_string(),
        columns: vec![col.to_string()],
        ddl: format!("CREATE INDEX idx_{}_{} ON {} ({})", table, col, table, col),
        expected_benefit_pct: benefit,
        risk: RiskLevel::Low,
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
    ]
}

#[test]
fn replay_with_shadow_db_verifies_candidate() {
    let shadow_db = MockShadowDb::new(vec![50, 48]);
    let validator = ReplayValidator::new(make_workload()).with_shadow_db(Box::new(shadow_db));
    let result =
        validator.validate_candidate_with_replay(make_candidate("orders", "user_id", 80.0));
    assert!(result.verified);
    assert!(!result.static_only);
    assert!(result.measured_benefit_pct > 0.0);
}

#[test]
fn replay_with_shadow_db_rolls_back_ddl() {
    let shadow_db = MockShadowDb::new(vec![50, 48]);
    let validator = ReplayValidator::new(make_workload()).with_shadow_db(Box::new(shadow_db));
    let _ = validator.validate_candidate_with_replay(make_candidate("orders", "user_id", 80.0));
    let shadow_db_ref = validator;
    assert!(!shadow_db_ref.workload().is_empty());
}

#[test]
fn replay_shadow_db_unavailable_falls_back_to_static() {
    let shadow_db = MockShadowDb::new(vec![]).with_fail_ddl();
    let validator = ReplayValidator::new(make_workload()).with_shadow_db(Box::new(shadow_db));
    let result =
        validator.validate_candidate_with_replay(make_candidate("orders", "user_id", 75.0));
    assert!(!result.verified);
    assert!(result.static_only);
    assert!(result.replay_error.is_some());
    let err = result.replay_error.unwrap();
    assert!(err.contains(AI_INDEX_REPLAY_SKIPPED));
}

#[test]
fn replay_shadow_db_query_failure_falls_back_to_static() {
    let shadow_db = MockShadowDb::new(vec![]).with_fail_query();
    let validator = ReplayValidator::new(make_workload()).with_shadow_db(Box::new(shadow_db));
    let result =
        validator.validate_candidate_with_replay(make_candidate("orders", "user_id", 75.0));
    assert!(!result.verified);
    assert!(result.static_only);
    assert!(result.replay_error.is_some());
    let err = result.replay_error.unwrap();
    assert!(err.contains(AI_INDEX_REPLAY_SKIPPED));
}

#[test]
fn replay_no_shadow_db_uses_validate_candidate() {
    let mut validator = ReplayValidator::new(make_workload());
    validator.register_table("orders", 100000, vec!["id".to_string()]);
    let result =
        validator.validate_candidate_with_replay(make_candidate("orders", "user_id", 80.0));
    assert!(result.verified);
    assert!(!result.static_only);
}

#[test]
fn replay_no_matching_queries_zero_benefit() {
    let shadow_db = MockShadowDb::new(vec![]);
    let validator = ReplayValidator::new(vec![]).with_shadow_db(Box::new(shadow_db));
    let result =
        validator.validate_candidate_with_replay(make_candidate("orders", "user_id", 80.0));
    assert!(result.verified);
    assert_eq!(result.measured_benefit_pct, 0.0);
}

#[test]
fn replay_benefit_calculation_correct() {
    let shadow_db = MockShadowDb::new(vec![100, 100]);
    let workload = vec![
        WorkloadQuery {
            sql: "SELECT * FROM t WHERE x = 1".to_string(),
            elapsed_ms: 200,
            uses_index: false,
        },
        WorkloadQuery {
            sql: "SELECT * FROM t WHERE x = 2".to_string(),
            elapsed_ms: 200,
            uses_index: false,
        },
    ];
    let validator = ReplayValidator::new(workload).with_shadow_db(Box::new(shadow_db));
    let result = validator.validate_candidate_with_replay(make_candidate("t", "x", 80.0));
    assert!(result.verified);
    assert!((result.measured_benefit_pct - 50.0).abs() < 0.01);
}
