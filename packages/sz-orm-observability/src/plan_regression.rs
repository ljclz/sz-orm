//! v6.7.0 执行计划回归检测：按 SQL 指纹缓存历史执行计划，对比变化时告警。

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScanType {
    SeqScan,
    IndexScan,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPlanSummary {
    pub scan_type: ScanType,
    pub index_name: Option<String>,
    pub estimated_rows: u64,
}

#[derive(Debug, Clone)]
pub struct PlanRegressionAlert {
    pub sql_fingerprint: String,
    pub old_plan: ExecutionPlanSummary,
    pub new_plan: ExecutionPlanSummary,
    pub timestamp: Instant,
}

pub struct PlanRegressionDetector {
    history: Mutex<HashMap<String, ExecutionPlanSummary>>,
}

impl PlanRegressionDetector {
    pub fn new() -> Self {
        Self {
            history: Mutex::new(HashMap::new()),
        }
    }

    pub fn detect_regression(
        &self,
        fingerprint: &str,
        current_plan: ExecutionPlanSummary,
    ) -> Option<PlanRegressionAlert> {
        let mut history = self.history.lock().unwrap();
        if let Some(old_plan) = history.get(fingerprint) {
            if plan_changed(old_plan, &current_plan) {
                let alert = PlanRegressionAlert {
                    sql_fingerprint: fingerprint.to_string(),
                    old_plan: old_plan.clone(),
                    new_plan: current_plan.clone(),
                    timestamp: Instant::now(),
                };
                history.insert(fingerprint.to_string(), current_plan);
                return Some(alert);
            }
        }
        history.insert(fingerprint.to_string(), current_plan);
        None
    }

    pub fn history_count(&self) -> usize {
        self.history.lock().unwrap().len()
    }
}

impl Default for PlanRegressionDetector {
    fn default() -> Self {
        Self::new()
    }
}

pub fn plan_changed(old: &ExecutionPlanSummary, new: &ExecutionPlanSummary) -> bool {
    if old.scan_type != new.scan_type {
        return true;
    }
    if old.index_name != new.index_name {
        return true;
    }
    false
}

pub fn fingerprint_sql(sql: &str) -> String {
    let mut result = String::with_capacity(sql.len());
    let mut in_quote = false;
    let mut quote_char = ' ';
    for ch in sql.chars() {
        if !in_quote && (ch == '\'' || ch == '"') {
            in_quote = true;
            quote_char = ch;
            result.push('?');
            continue;
        }
        if in_quote && ch == quote_char {
            in_quote = false;
            continue;
        }
        if !in_quote {
            result.push(ch);
        }
    }
    result.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_to_seq_scan_regression() {
        let detector = PlanRegressionDetector::new();
        let old_plan = ExecutionPlanSummary {
            scan_type: ScanType::IndexScan,
            index_name: Some("idx_user_id".to_string()),
            estimated_rows: 100,
        };
        detector.detect_regression("SELECT * FROM users WHERE id = ?", old_plan);

        let new_plan = ExecutionPlanSummary {
            scan_type: ScanType::SeqScan,
            index_name: None,
            estimated_rows: 10000,
        };
        let alert = detector.detect_regression("SELECT * FROM users WHERE id = ?", new_plan);
        assert!(alert.is_some());
        let alert = alert.unwrap();
        assert_eq!(alert.old_plan.scan_type, ScanType::IndexScan);
        assert_eq!(alert.new_plan.scan_type, ScanType::SeqScan);
    }

    #[test]
    fn no_regression_same_plan() {
        let detector = PlanRegressionDetector::new();
        let plan = ExecutionPlanSummary {
            scan_type: ScanType::IndexScan,
            index_name: Some("idx_id".to_string()),
            estimated_rows: 100,
        };
        detector.detect_regression("SELECT * FROM t WHERE id = ?", plan.clone());
        let alert = detector.detect_regression("SELECT * FROM t WHERE id = ?", plan);
        assert!(alert.is_none());
    }

    #[test]
    fn fingerprint_replaces_literals() {
        let fp = fingerprint_sql("SELECT * FROM users WHERE name = 'Alice' AND age = 25");
        assert!(fp.contains('?'));
        assert!(!fp.contains("Alice"));
    }

    #[test]
    fn index_name_change_detected() {
        let detector = PlanRegressionDetector::new();
        let old = ExecutionPlanSummary {
            scan_type: ScanType::IndexScan,
            index_name: Some("idx_a".to_string()),
            estimated_rows: 10,
        };
        detector.detect_regression("fp1", old);
        let new = ExecutionPlanSummary {
            scan_type: ScanType::IndexScan,
            index_name: Some("idx_b".to_string()),
            estimated_rows: 10,
        };
        assert!(detector.detect_regression("fp1", new).is_some());
    }

    #[test]
    fn first_plan_no_alert() {
        let detector = PlanRegressionDetector::new();
        let plan = ExecutionPlanSummary {
            scan_type: ScanType::SeqScan,
            index_name: None,
            estimated_rows: 100,
        };
        let alert = detector.detect_regression("new_fp", plan);
        assert!(alert.is_none());
        assert_eq!(detector.history_count(), 1);
    }
}
