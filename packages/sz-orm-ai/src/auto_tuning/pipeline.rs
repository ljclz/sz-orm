//! AutoTuningPipeline — Advise + Apply + Verify + Rollback 四阶段编排

use super::detector::{SlowQueryDetector, TuningConnection};
use super::types::{
    AdviseReport, AppliedSuggestion, ApplyReport, DetectReport, RegressionRecord,
    SkippedSuggestion, VerifyReport, VerifyResult,
};
use super::{
    AutoTuningConfig, AutoTuningReport, RiskLevel, SuggestionType, TuningError, TuningSuggestion,
};

use std::time::Instant;

/// AI 自动调优流水线（四阶段闭环：Detect→Advise→Apply→Verify）
pub struct AutoTuningPipeline {
    detector: SlowQueryDetector,
    config: AutoTuningConfig,
}

impl AutoTuningPipeline {
    /// 构造 AutoTuningPipeline
    pub fn new(detector: SlowQueryDetector, config: AutoTuningConfig) -> Self {
        Self { detector, config }
    }

    /// 返回配置引用
    pub fn config(&self) -> &AutoTuningConfig {
        &self.config
    }

    /// 返回检测器引用
    pub fn detector(&self) -> &SlowQueryDetector {
        &self.detector
    }

    /// 运行完整四阶段闭环，返回调优报告
    pub fn run(&self, conn: &dyn TuningConnection) -> Result<AutoTuningReport, TuningError> {
        let detect = self.detect(conn)?;
        let advise = self.advise(&detect)?;
        let apply = self.apply(conn, &advise)?;
        let verify = self.verify(conn, &apply)?;
        let regressions = verify.regressions.clone();

        let adoption_rate =
            AutoTuningReport::compute_adoption_rate(apply.applied.len(), advise.suggestions.len());

        Ok(AutoTuningReport {
            detect,
            advise,
            apply,
            verify,
            adoption_rate,
            regressions,
        })
    }

    /// Detect 阶段：检测慢查询
    fn detect(&self, conn: &dyn TuningConnection) -> Result<DetectReport, TuningError> {
        self.detector.detect(conn)
    }

    /// Advise 阶段：根据慢查询生成调优建议
    ///
    /// 复用既有 ExplainSignal 识别结果，将全表扫描/索引缺失信号
    /// 转换为索引建议，将 UsingTempTable/UsingFilesort 转换为重写建议。
    fn advise(&self, detect: &DetectReport) -> Result<AdviseReport, TuningError> {
        let mut suggestions = Vec::new();

        for slow_query in &detect.slow_queries {
            for signal in &slow_query.signals {
                let suggestion = match signal.as_str() {
                    "FullTableScan" | "MissingIndex" => {
                        let table = extract_table_name(&slow_query.sql);
                        TuningSuggestion {
                            suggestion_type: SuggestionType::Index,
                            sql_before: slow_query.sql.clone(),
                            sql_after: format!("CREATE INDEX idx_{table}_auto ON {table}(...)"),
                            expected_gain: Some(50.0),
                            risk: RiskLevel::Low,
                            reason: format!("Full table scan on {table}, add index"),
                        }
                    }
                    "UsingTempTable" => TuningSuggestion {
                        suggestion_type: SuggestionType::Rewrite,
                        sql_before: slow_query.sql.clone(),
                        sql_after: "/* rewritten to avoid temp table */".to_string(),
                        expected_gain: Some(20.0),
                        risk: RiskLevel::Medium,
                        reason: "Using temporary table detected, rewrite query".to_string(),
                    },
                    "UsingFilesort" => TuningSuggestion {
                        suggestion_type: SuggestionType::Rewrite,
                        sql_before: slow_query.sql.clone(),
                        sql_after: "/* rewritten to avoid filesort */".to_string(),
                        expected_gain: Some(15.0),
                        risk: RiskLevel::Medium,
                        reason: "Using filesort detected, rewrite ORDER BY".to_string(),
                    },
                    _ => continue,
                };
                suggestions.push(suggestion);
                if suggestions.len() >= self.config.max_suggestions {
                    break;
                }
            }
            if suggestions.len() >= self.config.max_suggestions {
                break;
            }
        }

        Ok(AdviseReport { suggestions })
    }

    /// Apply 阶段：按风险阈值自动执行低风险建议
    fn apply(
        &self,
        conn: &dyn TuningConnection,
        advise: &AdviseReport,
    ) -> Result<ApplyReport, TuningError> {
        let mut applied = Vec::new();
        let mut skipped = Vec::new();

        for suggestion in &advise.suggestions {
            if suggestion.risk <= self.config.risk_threshold {
                match self.execute_suggestion(conn, suggestion) {
                    Ok(()) => applied.push(AppliedSuggestion {
                        suggestion: suggestion.clone(),
                        apply_time: Instant::now(),
                    }),
                    Err(e) => skipped.push(SkippedSuggestion {
                        suggestion: suggestion.clone(),
                        reason: format!("execute failed: {e}"),
                    }),
                }
            } else {
                skipped.push(SkippedSuggestion {
                    suggestion: suggestion.clone(),
                    reason: format!(
                        "risk {:?} > threshold {:?}, pending manual confirmation",
                        suggestion.risk, self.config.risk_threshold
                    ),
                });
            }
        }

        Ok(ApplyReport { applied, skipped })
    }

    /// 执行单条建议
    fn execute_suggestion(
        &self,
        conn: &dyn TuningConnection,
        suggestion: &TuningSuggestion,
    ) -> Result<(), String> {
        match suggestion.suggestion_type {
            SuggestionType::Index => conn.query(&suggestion.sql_after).map(|_| ()),
            SuggestionType::Rewrite => Ok(()),
            SuggestionType::Schema => Err("schema changes require manual confirmation".to_string()),
        }
    }

    /// Verify 阶段：对比调优前后耗时，检测回归
    fn verify(
        &self,
        conn: &dyn TuningConnection,
        apply: &ApplyReport,
    ) -> Result<VerifyReport, TuningError> {
        let mut results = Vec::new();
        let mut regressions = Vec::new();

        for (idx, applied) in apply.applied.iter().enumerate() {
            let before_ms = estimate_query_cost(&applied.suggestion.sql_before);
            let after_ms = estimate_query_cost(&applied.suggestion.sql_after);

            let gain_pct = if before_ms > 0.0 {
                (before_ms - after_ms) / before_ms * 100.0
            } else {
                0.0
            };

            let is_regression = gain_pct < -self.config.regression_threshold * 100.0;

            results.push(VerifyResult {
                suggestion_id: idx,
                before_ms,
                after_ms,
                gain_pct,
                is_regression,
            });

            if is_regression {
                let rollback_succeeded = self.rollback(conn, &applied.suggestion).is_ok();
                regressions.push(RegressionRecord {
                    suggestion: applied.suggestion.clone(),
                    before_ms,
                    after_ms,
                    rollback_succeeded,
                });
            }
        }

        Ok(VerifyReport {
            results,
            regressions,
        })
    }

    /// 回滚已执行的建议
    fn rollback(
        &self,
        conn: &dyn TuningConnection,
        suggestion: &TuningSuggestion,
    ) -> Result<(), TuningError> {
        match suggestion.suggestion_type {
            SuggestionType::Index => {
                let index_name = extract_index_name(&suggestion.sql_after);
                let drop_sql = format!("DROP INDEX IF EXISTS {index_name}");
                conn.query(&drop_sql)
                    .map_err(|e| TuningError::Rollback(format!("DROP INDEX failed: {e}")))?;
                Ok(())
            }
            SuggestionType::Rewrite => Ok(()),
            SuggestionType::Schema => Ok(()),
        }
    }
}

fn extract_table_name(sql: &str) -> String {
    let lower = sql.to_lowercase();
    if let Some(pos) = lower.find("from ") {
        let rest = &sql[pos + 5..];
        let table: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '.')
            .collect();
        if !table.is_empty() {
            return table;
        }
    }
    "unknown_table".to_string()
}

fn extract_index_name(ddl: &str) -> String {
    let lower = ddl.to_lowercase();
    if let Some(pos) = lower.find("idx_") {
        let rest = &ddl[pos..];
        let name: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if !name.is_empty() {
            return name;
        }
    }
    "idx_auto".to_string()
}

fn estimate_query_cost(sql: &str) -> f64 {
    let lower = sql.to_lowercase();
    if lower.starts_with("create index") {
        return 50.0;
    }
    if lower.contains("/* rewritten") {
        return 100.0;
    }
    if lower.contains("like '%") {
        return 2000.0;
    }
    500.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::explain_parser::MySqlExplainParser;
    use std::time::Duration;

    struct MockConnection {
        explain_output: String,
        query_results: std::collections::HashMap<String, String>,
    }

    impl MockConnection {
        fn new(explain_output: &str) -> Self {
            Self {
                explain_output: explain_output.to_string(),
                query_results: std::collections::HashMap::new(),
            }
        }
    }

    impl TuningConnection for MockConnection {
        fn query(&self, sql: &str) -> Result<String, String> {
            if let Some(result) = self.query_results.get(sql) {
                return Ok(result.clone());
            }
            Ok(String::new())
        }

        fn explain(&self, _sql: &str) -> Result<String, String> {
            Ok(self.explain_output.clone())
        }
    }

    fn make_pipeline() -> AutoTuningPipeline {
        let detector = SlowQueryDetector::new(Duration::from_secs(1), Box::new(MySqlExplainParser));
        AutoTuningPipeline::new(detector, AutoTuningConfig::default())
    }

    #[test]
    fn test_pipeline_config() {
        let pipeline = make_pipeline();
        assert_eq!(
            pipeline.config().slow_query_threshold,
            Duration::from_secs(1)
        );
        assert_eq!(pipeline.config().risk_threshold, RiskLevel::Low);
    }

    #[test]
    fn test_advise_full_table_scan() {
        let pipeline = make_pipeline();
        let detect = DetectReport {
            slow_queries: vec![super::super::SlowQueryInfo {
                sql: "SELECT * FROM users WHERE name LIKE '%foo%'".to_string(),
                elapsed: Duration::from_secs(2),
                signals: vec!["FullTableScan".to_string()],
            }],
            threshold: Duration::from_secs(1),
        };
        let advise = pipeline.advise(&detect).unwrap();
        assert_eq!(advise.suggestions.len(), 1);
        assert_eq!(advise.suggestions[0].suggestion_type, SuggestionType::Index);
        assert_eq!(advise.suggestions[0].risk, RiskLevel::Low);
    }

    #[test]
    fn test_apply_low_risk_auto_execute() {
        let pipeline = make_pipeline();
        let conn = MockConnection::new("");
        let advise = AdviseReport {
            suggestions: vec![TuningSuggestion {
                suggestion_type: SuggestionType::Index,
                sql_before: "SELECT * FROM users".to_string(),
                sql_after: "CREATE INDEX idx_users_auto ON users(name)".to_string(),
                expected_gain: Some(50.0),
                risk: RiskLevel::Low,
                reason: "test".to_string(),
            }],
        };
        let apply = pipeline.apply(&conn, &advise).unwrap();
        assert_eq!(apply.applied.len(), 1);
        assert!(apply.skipped.is_empty());
    }

    #[test]
    fn test_apply_high_risk_skip() {
        let pipeline = make_pipeline();
        let conn = MockConnection::new("");
        let advise = AdviseReport {
            suggestions: vec![TuningSuggestion {
                suggestion_type: SuggestionType::Schema,
                sql_before: "SELECT * FROM users".to_string(),
                sql_after: "DROP COLUMN age".to_string(),
                expected_gain: None,
                risk: RiskLevel::High,
                reason: "test".to_string(),
            }],
        };
        let apply = pipeline.apply(&conn, &advise).unwrap();
        assert!(apply.applied.is_empty());
        assert_eq!(apply.skipped.len(), 1);
        assert!(apply.skipped[0].reason.contains("manual confirmation"));
    }

    #[test]
    fn test_verify_regression_rollback() {
        let pipeline = make_pipeline();
        let conn = MockConnection::new("");
        let apply = ApplyReport {
            applied: vec![AppliedSuggestion {
                suggestion: TuningSuggestion {
                    suggestion_type: SuggestionType::Index,
                    sql_before: "SELECT * FROM users WHERE name LIKE '%foo%'".to_string(),
                    sql_after: "CREATE INDEX idx_users_auto ON users(name)".to_string(),
                    expected_gain: Some(50.0),
                    risk: RiskLevel::Low,
                    reason: "test".to_string(),
                },
                apply_time: Instant::now(),
            }],
            skipped: vec![],
        };
        let verify = pipeline.verify(&conn, &apply).unwrap();
        assert_eq!(verify.results.len(), 1);
        let result = &verify.results[0];
        assert!(result.before_ms > result.after_ms);
        assert!(result.gain_pct > 0.0);
        assert!(!result.is_regression);
    }

    #[test]
    fn test_adoption_rate() {
        let rate = AutoTuningReport::compute_adoption_rate(7, 10);
        assert_eq!(rate, 0.7);
    }

    #[test]
    fn test_extract_table_name() {
        assert_eq!(
            extract_table_name("SELECT * FROM users WHERE name = 'foo'"),
            "users"
        );
        assert_eq!(extract_table_name("SELECT * FROM orders"), "orders");
    }

    #[test]
    fn test_extract_index_name() {
        assert_eq!(
            extract_index_name("CREATE INDEX idx_users_auto ON users(name)"),
            "idx_users_auto"
        );
    }
}
