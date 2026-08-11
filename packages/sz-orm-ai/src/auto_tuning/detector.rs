//! SlowQueryDetector — Detect 阶段（慢查询检测 + EXPLAIN 解析）

use super::types::{DetectReport, SlowQueryInfo};
use super::TuningError;
use crate::explain_parser::ExplainPlanParser;
use std::time::Duration;

/// 数据库连接抽象（用于慢查询检测）
pub trait TuningConnection: Send + Sync {
    /// 执行 SQL 查询，返回结果文本
    fn query(&self, sql: &str) -> Result<String, String>;

    /// 执行 EXPLAIN，返回 EXPLAIN 输出
    fn explain(&self, sql: &str) -> Result<String, String>;
}

/// 慢查询检测器
pub struct SlowQueryDetector {
    /// 慢查询阈值
    threshold: Duration,
    /// EXPLAIN 解析器
    parser: Box<dyn ExplainPlanParser>,
}

impl SlowQueryDetector {
    /// 构造 SlowQueryDetector
    pub fn new(threshold: Duration, parser: Box<dyn ExplainPlanParser>) -> Self {
        Self { threshold, parser }
    }

    /// 返回慢查询阈值
    pub fn threshold(&self) -> Duration {
        self.threshold
    }

    /// 返回方言名称
    pub fn dialect(&self) -> &'static str {
        self.parser.dialect()
    }

    /// 采集慢查询日志并解析 EXPLAIN
    ///
    /// 从数据库慢查询日志中采集耗时超过阈值的查询，
    /// 对每条慢查询执行 EXPLAIN + `parser.parse()` 识别问题信号。
    pub fn detect(&self, conn: &dyn TuningConnection) -> Result<DetectReport, TuningError> {
        let slow_log_sql = self.slow_query_log_sql();
        let log_output = conn
            .query(&slow_log_sql)
            .map_err(|e| TuningError::Detect(format!("query slow log failed: {e}")))?;

        let slow_queries = self.parse_slow_log(&log_output);

        let mut detected = Vec::new();
        for query_info in slow_queries {
            if query_info.elapsed >= self.threshold {
                let info =
                    self.detect_from_sql_with_elapsed(&query_info.sql, query_info.elapsed, conn)?;
                detected.push(info);
            }
        }

        Ok(DetectReport {
            slow_queries: detected,
            threshold: self.threshold,
        })
    }

    /// 对单条 SQL 执行 EXPLAIN + 解析，返回慢查询信息
    pub fn detect_from_sql(
        &self,
        sql: &str,
        elapsed: Duration,
        conn: &dyn TuningConnection,
    ) -> Result<SlowQueryInfo, TuningError> {
        self.detect_from_sql_with_elapsed(sql, elapsed, conn)
    }

    fn detect_from_sql_with_elapsed(
        &self,
        sql: &str,
        elapsed: Duration,
        conn: &dyn TuningConnection,
    ) -> Result<SlowQueryInfo, TuningError> {
        let explain_output = conn
            .explain(sql)
            .map_err(|e| TuningError::Detect(format!("EXPLAIN failed: {e}")))?;

        let signals = self
            .parser
            .parse(&explain_output)
            .map_err(|e| TuningError::Detect(format!("parse EXPLAIN failed: {e}")))?;

        let signal_strings: Vec<String> = signals.iter().map(|s| s.as_str().to_string()).collect();

        Ok(SlowQueryInfo {
            sql: sql.to_string(),
            elapsed,
            signals: signal_strings,
        })
    }

    /// 慢查询日志采集 SQL（按方言适配）
    fn slow_query_log_sql(&self) -> String {
        match self.parser.dialect() {
            "mysql" => {
                "SELECT DIGEST_TEXT, AVG_TIMER_WAIT/1000000000 as avg_ms \
                 FROM performance_schema.events_statements_summary_by_digest \
                 WHERE AVG_TIMER_WAIT/1000000000 > 1000"
                    .to_string()
            }
            "postgresql" => {
                "SELECT query, mean_exec_time \
                 FROM pg_stat_statements \
                 WHERE mean_exec_time > 1000"
                    .to_string()
            }
            "sqlite" => "SELECT sql, 0 FROM sqlite_stat1".to_string(),
            "oracle" => "SELECT sql_text, elapsed_time/1000000 FROM v$sql WHERE elapsed_time/1000000 > 1000".to_string(),
            "mssql" => "SELECT query_hash, total_elapsed_time/execution_count/1000 FROM sys.dm_exec_query_stats WHERE total_elapsed_time/execution_count/1000 > 1000".to_string(),
            _ => "SELECT '' as sql, 0 as elapsed".to_string(),
        }
    }

    /// 解析慢查询日志输出（简单行解析）
    fn parse_slow_log(&self, log_output: &str) -> Vec<ParsedSlowQuery> {
        let mut queries = Vec::new();
        for line in log_output.lines() {
            if line.is_empty() || line.starts_with("--") {
                continue;
            }
            let parts: Vec<&str> = line.splitn(2, '|').collect();
            if parts.len() == 2 {
                let sql = parts[0].trim().to_string();
                let elapsed_ms: u64 = parts[1].trim().parse().unwrap_or(0);
                queries.push(ParsedSlowQuery {
                    sql,
                    elapsed: Duration::from_millis(elapsed_ms),
                });
            }
        }
        queries
    }
}

struct ParsedSlowQuery {
    sql: String,
    elapsed: Duration,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::explain_parser::MySqlExplainParser;

    struct MockConnection {
        explain_output: String,
    }

    impl TuningConnection for MockConnection {
        fn query(&self, _sql: &str) -> Result<String, String> {
            Ok(String::new())
        }

        fn explain(&self, _sql: &str) -> Result<String, String> {
            Ok(self.explain_output.clone())
        }
    }

    #[test]
    fn test_detector_no_slow_queries() {
        let detector = SlowQueryDetector::new(Duration::from_secs(1), Box::new(MySqlExplainParser));
        let conn = MockConnection {
            explain_output: String::new(),
        };
        let report = detector.detect(&conn).unwrap();
        assert!(report.slow_queries.is_empty());
        assert_eq!(report.threshold, Duration::from_secs(1));
    }

    #[test]
    fn test_detector_full_table_scan() {
        let detector = SlowQueryDetector::new(Duration::from_secs(1), Box::new(MySqlExplainParser));
        let conn = MockConnection {
            explain_output: "| id | select_type | table | type | possible_keys |\n| 1 | SIMPLE | users | ALL | NULL |".to_string(),
        };
        let info = detector
            .detect_from_sql(
                "SELECT * FROM users WHERE name LIKE '%foo%'",
                Duration::from_secs(2),
                &conn,
            )
            .unwrap();
        assert_eq!(info.sql, "SELECT * FROM users WHERE name LIKE '%foo%'");
        assert_eq!(info.elapsed, Duration::from_secs(2));
        assert!(info.signals.contains(&"FullTableScan".to_string()));
    }

    #[test]
    fn test_detector_dialect() {
        let mysql_detector =
            SlowQueryDetector::new(Duration::from_secs(1), Box::new(MySqlExplainParser));
        assert_eq!(mysql_detector.dialect(), "mysql");
    }

    #[test]
    fn test_slow_query_log_sql_dialects() {
        let mysql_detector =
            SlowQueryDetector::new(Duration::from_secs(1), Box::new(MySqlExplainParser));
        let sql = mysql_detector.slow_query_log_sql();
        assert!(sql.contains("performance_schema"));
    }
}
