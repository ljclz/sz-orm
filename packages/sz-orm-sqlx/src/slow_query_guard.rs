//! v9.5.0 SlowQueryGuard：慢查询自动捕获
//!
//! 包装 `Connection` 执行路径，自动测量耗时 + 超阈值调用 `QueryLogger.log`
//! （`query_logger.rs:79`），含采样率过滤、参数脱敏、SQL 指纹归一化。
//!
//! # 设计要点
//!
//! - **慢查询 100% 记录**：耗时 ≥ `slow_threshold` 的查询必定记录（spec 5.2.1 规则 1）
//! - **非慢查询采样记录**：按 `sample_rate` 概率采样（spec 5.2.1 规则 2）
//! - **参数脱敏**：由 `QueryLogger` 内部 `MaskingRule` 自动脱敏（spec 5.2.1 规则 4）
//! - **SQL 指纹归一化**：参数占位符归一化生成稳定指纹（spec 5.2.1 规则 3）
//! - **降级安全**：日志初始化/写入失败不影响业务执行（spec 5.2.3 异常 1/3）

#![cfg(feature = "slow-query-capture")]

use std::time::{Duration, Instant};

use sz_orm_core::{Connection, DbError, Utc, Value};
use sz_orm_observability::{
    query_logger::SerializablePhaseTiming, LogLevel, QueryLogEntry, QueryLogger,
};

/// v9.5.0 慢查询守卫：包装 Connection 自动捕获慢查询。
///
/// # 示例
///
/// ```no_run
/// use std::time::Duration;
/// use sz_orm_core::{Connection, DbType, Value};
/// use sz_orm_sqlx::SlowQueryGuard;
///
/// # async fn example(conn: impl Connection) {
/// let guard = SlowQueryGuard::new(conn)
///     .with_slow_threshold(Duration::from_millis(200));
/// // guard.execute_with_logging("SELECT * FROM users WHERE id = ?", &[Value::I64(1)]).await
/// # }
/// ```
pub struct SlowQueryGuard<C: Connection> {
    inner: C,
    logger: QueryLogger,
    slow_threshold: Duration,
    /// 已记录的日志 JSON 条目（QueryLogger.log 返回值）
    logged_jsons: Vec<String>,
    /// 已记录的日志条目元数据（用于测试断言 slow/fingerprint）
    logged_entries: Vec<QueryLogEntry>,
    /// 是否降级（logger 初始化失败时 true）
    degraded: bool,
}

impl<C: Connection> SlowQueryGuard<C> {
    /// 创建慢查询守卫，默认阈值 200ms，采样率 1%。
    pub fn new(conn: C) -> Self {
        Self {
            inner: conn,
            logger: QueryLogger::new(),
            slow_threshold: Duration::from_millis(200),
            logged_jsons: Vec::new(),
            logged_entries: Vec::new(),
            degraded: false,
        }
    }

    /// 设置慢查询阈值。
    pub fn with_slow_threshold(mut self, threshold: Duration) -> Self {
        self.slow_threshold = threshold;
        self
    }

    /// 设置采样率（0.0~1.0），非法值触发降级。
    pub fn with_sample_rate(mut self, rate: f64) -> Self {
        if rate < 0.0 || rate > 1.0 {
            self.degraded = true;
        } else {
            self.logger = self.logger.with_sample_rate(rate);
        }
        self
    }

    /// 设置日志级别。
    pub fn with_level(mut self, level: LogLevel) -> Self {
        self.logger = self.logger.with_level(level);
        self
    }

    /// 带日志的参数化执行：记录耗时 → 执行 → 判定慢查询 → 记录日志。
    ///
    /// 即使日志记录失败，业务执行结果仍正常返回（spec 5.2.3 异常 3）。
    pub async fn execute_with_logging(
        &mut self,
        sql: &str,
        params: &[Value],
    ) -> Result<u64, DbError> {
        let start = Instant::now();
        let result = self.inner.execute_with_params(sql, params).await;
        let elapsed = start.elapsed();

        let is_slow = elapsed >= self.slow_threshold;
        self.maybe_log(sql, params, elapsed, is_slow);

        result
    }

    /// 获取已记录的日志 JSON 条目（用于测试断言脱敏内容）。
    pub fn logged_jsons(&self) -> &[String] {
        &self.logged_jsons
    }

    /// 获取已记录的日志条目元数据（用于测试断言 slow/fingerprint）。
    pub fn logged_entries(&self) -> &[QueryLogEntry] {
        &self.logged_entries
    }

    /// 是否处于降级模式。
    pub fn is_degraded(&self) -> bool {
        self.degraded
    }

    /// 获取内部连接的可变引用。
    pub fn inner_mut(&mut self) -> &mut C {
        &mut self.inner
    }

    fn maybe_log(&mut self, sql: &str, params: &[Value], elapsed: Duration, is_slow: bool) {
        if self.degraded {
            return;
        }

        let fingerprint = normalize_fingerprint(sql);
        let param_strings: Vec<String> = params.iter().map(|v| v.to_string()).collect();

        let entry = QueryLogEntry {
            query_key: fingerprint,
            sql: sql.to_string(),
            params: param_strings,
            total_elapsed_ms: elapsed.as_millis() as u64,
            phase_breakdown: vec![SerializablePhaseTiming {
                phase: "execute".to_string(),
                start_ms: 0,
                duration_ms: elapsed.as_millis() as u64,
            }],
            slow: is_slow,
            from_cache: false,
            timestamp: Utc::now().to_rfc3339(),
            trace_id: None,
            span_id: None,
        };

        if let Some(json) = self.logger.log(entry.clone()) {
            self.logged_jsons.push(json);
            self.logged_entries.push(entry);
        }
    }
}

/// SQL 指纹归一化：将参数占位符和字面量归一化为 `?`。
///
/// - `WHERE id = 1` → `WHERE id = ?`
/// - `WHERE id = 2` → `WHERE id = ?`
/// - `WHERE name = 'alice'` → `WHERE name = ?`
/// - `$1`/`$2` → `?`
pub fn normalize_fingerprint(sql: &str) -> String {
    let mut result = String::with_capacity(sql.len());
    let chars: Vec<char> = sql.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\'' {
            result.push('?');
            i += 1;
            while i < chars.len() && chars[i] != '\'' {
                i += 1;
            }
            if i < chars.len() {
                i += 1;
            }
        } else if c == '$' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit() {
            result.push('?');
            i += 2;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
        } else if c.is_ascii_digit() {
            let mut j = i;
            while j < chars.len() && chars[j].is_ascii_digit() {
                j += 1;
            }
            if j == chars.len() || !chars[j].is_ascii_alphabetic() {
                result.push('?');
            } else {
                result.extend(&chars[i..j]);
            }
            i = j;
        } else {
            result.push(c);
            i += 1;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use sz_orm_core::mock::MockConnection;

    #[tokio::test]
    async fn test_slow_query_above_threshold_logged() {
        let mut mock = MockConnection::new();
        mock.expect_any().with_rows_affected(1);
        let mut guard = SlowQueryGuard::new(mock).with_slow_threshold(Duration::from_millis(0));
        guard
            .execute_with_logging("SELECT * FROM t WHERE id = ?", &[Value::I64(1)])
            .await
            .unwrap();
        let entries = guard.logged_entries();
        assert!(!entries.is_empty());
        assert!(entries[0].slow);
    }

    #[tokio::test]
    async fn test_non_slow_not_logged_without_sampling() {
        let mut mock = MockConnection::new();
        mock.expect_any().with_rows_affected(1);
        let mut guard = SlowQueryGuard::new(mock)
            .with_slow_threshold(Duration::from_secs(10))
            .with_sample_rate(0.0);
        guard.execute_with_logging("SELECT 1", &[]).await.unwrap();
        assert!(guard.logged_entries().is_empty());
    }

    #[tokio::test]
    async fn test_query_logger_init_fail_degrade() {
        let mut mock = MockConnection::new();
        mock.expect_any().with_rows_affected(1);
        let mut guard = SlowQueryGuard::new(mock).with_sample_rate(2.0);
        assert!(guard.is_degraded());
        let result = guard.execute_with_logging("SELECT 1", &[]).await.unwrap();
        assert_eq!(result, 1);
        assert!(guard.logged_entries().is_empty());
    }

    #[tokio::test]
    async fn test_param_masking() {
        let mut mock = MockConnection::new();
        mock.expect_any().with_rows_affected(1);
        let mut guard = SlowQueryGuard::new(mock)
            .with_slow_threshold(Duration::from_millis(0))
            .with_level(LogLevel::Debug);
        guard
            .execute_with_logging(
                "SELECT * FROM users WHERE phone = ?",
                &[Value::String("13800138000".to_string())],
            )
            .await
            .unwrap();
        let jsons = guard.logged_jsons();
        assert!(!jsons.is_empty());
        assert!(!jsons[0].contains("13800138000"));
    }

    #[test]
    fn test_fingerprint_normalization() {
        let fp1 = normalize_fingerprint("SELECT * FROM users WHERE id = 1");
        let fp2 = normalize_fingerprint("SELECT * FROM users WHERE id = 2");
        assert_eq!(fp1, fp2);
        assert!(fp1.contains('?'));
    }

    #[test]
    fn test_fingerprint_pg_placeholders() {
        let fp1 = normalize_fingerprint("SELECT * FROM users WHERE id = $1");
        let fp2 = normalize_fingerprint("SELECT * FROM users WHERE id = $2");
        assert_eq!(fp1, fp2);
    }

    #[test]
    fn test_fingerprint_string_literals() {
        let fp1 = normalize_fingerprint("SELECT * FROM users WHERE name = 'alice'");
        let fp2 = normalize_fingerprint("SELECT * FROM users WHERE name = 'bob'");
        assert_eq!(fp1, fp2);
    }

    #[tokio::test]
    async fn test_no_full_logging() {
        let mut mock = MockConnection::new();
        mock.expect_any().with_rows_affected(1);
        let mut guard = SlowQueryGuard::new(mock)
            .with_slow_threshold(Duration::from_secs(10))
            .with_sample_rate(0.0);
        for _ in 0..100 {
            guard.execute_with_logging("SELECT 1", &[]).await.unwrap();
        }
        assert!(guard.logged_entries().is_empty());
    }

    #[tokio::test]
    async fn test_execute_result_returned_correctly() {
        let mut mock = MockConnection::new();
        mock.expect_any().with_rows_affected(42);
        let mut guard = SlowQueryGuard::new(mock).with_slow_threshold(Duration::from_millis(0));
        let result = guard
            .execute_with_logging("INSERT INTO t VALUES (?)", &[Value::I64(1)])
            .await
            .unwrap();
        assert_eq!(result, 42);
    }

    #[tokio::test]
    async fn test_slow_query_error_still_logged() {
        let mock = MockConnection::new().with_fallback(sz_orm_core::mock::FallbackBehavior::Error);
        let mut guard = SlowQueryGuard::new(mock).with_slow_threshold(Duration::from_millis(0));
        let _ = guard.execute_with_logging("SELECT bad_sql", &[]).await;
        assert!(!guard.logged_entries().is_empty());
        assert!(guard.logged_entries()[0].slow);
    }
}
