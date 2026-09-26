//! 双轨影子流量校验模块
//!
//! 在 ORM 查询执行时，同步通过原生路径执行相同 SQL 并比较结果集，
//! 用于在上线重大优化时验证 ORM 行为与原生驱动的一致性。
//!
//! # 设计
//!
//! [`ShadowConnection`] 是一个装饰器（decorator），包装一个 ORM [`Connection`]
//! 和一个用于比对的"原生" [`Connection`]（通常底层是相同的 sqlx pool）。
//! 每次 ORM 执行 `query` 时，原生连接同步执行相同 SQL，然后比较结果。
//!
//! 与 SKILL.md 中提到的 `tower` 中间件模式不同，sz-orm 是库而非 HTTP 服务，
//! 装饰器模式更贴合 ORM 的 `Connection` trait 抽象。
//!
//! # 使用示例
//!
//! ```ignore
//! use sz_orm_core::shadow::{ShadowConnection, ShadowConfig};
//!
//! let orm_conn = pool.acquire().await?;
//! let raw_conn = pool.acquire().await?;
//! let mut shadow = ShadowConnection::new(orm_conn, raw_conn, ShadowConfig::default());
//!
//! // 执行查询时自动双轨比对
//! let rows = shadow.query("SELECT * FROM users").await?;
//! ```

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crate::pool::{Connection, QueryRows};
use crate::value::Value;

/// 影子流量比较结果
#[derive(Debug, Clone)]
pub struct ShadowComparison {
    /// 执行的 SQL
    pub sql: String,
    /// ORM 路径执行耗时
    pub orm_duration: Duration,
    /// 原生路径执行耗时
    pub raw_duration: Duration,
    /// ORM 返回的行数
    pub orm_rows: usize,
    /// 原生返回的行数
    pub raw_rows: usize,
    /// 是否一致
    pub consistent: bool,
    /// 不一致时的差异描述（首条差异）
    pub mismatch: Option<String>,
}

impl ShadowComparison {
    /// ORM 延迟与原生延迟的比值（>1 表示 ORM 更慢）
    pub fn latency_ratio(&self) -> f64 {
        if self.raw_duration.is_zero() {
            1.0
        } else {
            self.orm_duration.as_secs_f64() / self.raw_duration.as_secs_f64()
        }
    }
}

/// 影子流量累计统计
#[derive(Debug, Default)]
pub struct ShadowStats {
    /// 累计比较次数
    pub comparisons: AtomicU64,
    /// 不一致次数
    pub mismatches: AtomicU64,
    /// ORM 累计耗时（微秒）
    pub orm_total_us: AtomicU64,
    /// 原生累计耗时（微秒）
    pub raw_total_us: AtomicU64,
}

impl ShadowStats {
    /// 平均 ORM 耗时（微秒）
    pub fn avg_orm_us(&self) -> u64 {
        let n = self.comparisons.load(Ordering::Relaxed);
        // 使用 checked_div 替代手动零检查，避免 clippy::manual_checked_ops 警告
        self.orm_total_us
            .load(Ordering::Relaxed)
            .checked_div(n)
            .unwrap_or(0)
    }

    /// 平均原生耗时（微秒）
    pub fn avg_raw_us(&self) -> u64 {
        let n = self.comparisons.load(Ordering::Relaxed);
        // 使用 checked_div 替代手动零检查，避免 clippy::manual_checked_ops 警告
        self.raw_total_us
            .load(Ordering::Relaxed)
            .checked_div(n)
            .unwrap_or(0)
    }

    /// 不一致率
    pub fn mismatch_rate(&self) -> f64 {
        let n = self.comparisons.load(Ordering::Relaxed);
        if n == 0 {
            0.0
        } else {
            self.mismatches.load(Ordering::Relaxed) as f64 / n as f64
        }
    }
}

/// 影子流量配置
#[derive(Debug, Clone)]
pub struct ShadowConfig {
    /// 单侧执行超时（秒）。超时记为不一致。
    pub timeout: Duration,
    /// 是否仅比较行数（跳过逐字段比对，用于快速验证）
    pub row_count_only: bool,
    /// 最大比对行数（避免超大结果集内存爆炸）
    pub max_compare_rows: usize,
}

impl Default for ShadowConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(3),
            row_count_only: false,
            max_compare_rows: 10_000,
        }
    }
}

/// 不一致处理策略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MismatchAction {
    /// 仅记录，继续返回 ORM 结果
    Record,
    /// 记录并返回错误
    Error,
    /// 记录并触发 panic（用于测试环境）
    Panic,
}

/// 双轨影子流量校验连接装饰器
///
/// 包装 ORM 连接和原生连接，每次查询时同步执行两路并比较结果。
///
/// # 线程安全
///
/// `ShadowConnection` 内部的 ORM 连接和原生连接都要求 `Connection: Send + Sync`，
/// 因此 `ShadowConnection` 自动满足 `Send + Sync`。
pub struct ShadowConnection<C: Connection> {
    /// ORM 路径连接
    orm_conn: C,
    /// 原生路径连接
    raw_conn: C,
    /// 配置
    config: ShadowConfig,
    /// 不匹配时的行为
    on_mismatch: MismatchAction,
    /// 累计统计
    stats: ShadowStats,
}

impl<C: Connection> ShadowConnection<C> {
    /// 创建影子流量连接
    pub fn new(orm_conn: C, raw_conn: C, config: ShadowConfig) -> Self {
        Self {
            orm_conn,
            raw_conn,
            config,
            on_mismatch: MismatchAction::Record,
            stats: ShadowStats::default(),
        }
    }

    /// 设置不匹配行为
    pub fn with_mismatch_action(mut self, action: MismatchAction) -> Self {
        self.on_mismatch = action;
        self
    }

    /// 获取统计快照
    pub fn stats(&self) -> &ShadowStats {
        &self.stats
    }

    /// 执行双轨查询并比较结果
    ///
    /// 返回 ORM 路径的结果。如果不匹配，根据 `on_mismatch` 策略处理。
    pub async fn query_shadow(&mut self, sql: &str) -> Result<QueryRows, crate::DbError> {
        // 并发执行两路查询
        let orm_fut = self.orm_conn.query(sql);
        let raw_fut = self.raw_conn.query(sql);

        let orm_start = Instant::now();
        let orm_result = tokio::time::timeout(self.config.timeout, orm_fut).await;
        let orm_duration = orm_start.elapsed();

        let raw_start = Instant::now();
        let raw_result = tokio::time::timeout(self.config.timeout, raw_fut).await;
        let raw_duration = raw_start.elapsed();

        // 统计累计耗时
        self.stats
            .orm_total_us
            .fetch_add(orm_duration.as_micros() as u64, Ordering::Relaxed);
        self.stats
            .raw_total_us
            .fetch_add(raw_duration.as_micros() as u64, Ordering::Relaxed);
        self.stats.comparisons.fetch_add(1, Ordering::Relaxed);

        // 处理超时与错误
        let orm_rows = match orm_result {
            Ok(Ok(rows)) => rows,
            Ok(Err(e)) => {
                self.stats.mismatches.fetch_add(1, Ordering::Relaxed);
                // H-5 修复：传播 handle_mismatch 的 Result（Panic 模式下返回 Internal 错误）
                self.handle_mismatch(ShadowComparison {
                    sql: sql.to_string(),
                    orm_duration,
                    raw_duration,
                    orm_rows: 0,
                    raw_rows: 0,
                    consistent: false,
                    mismatch: Some(format!("ORM error: {}", e)),
                })?;
                return Err(e);
            }
            Err(_) => {
                self.stats.mismatches.fetch_add(1, Ordering::Relaxed);
                return Err(crate::DbError::ConnectionTimeout(format!(
                    "ORM shadow path timeout after {:?}",
                    self.config.timeout
                )));
            }
        };

        let raw_rows = match raw_result {
            Ok(Ok(rows)) => rows,
            Ok(Err(e)) => {
                self.stats.mismatches.fetch_add(1, Ordering::Relaxed);
                // H-5 修复：传播 handle_mismatch 的 Result（Panic 模式下返回 Internal 错误）
                self.handle_mismatch(ShadowComparison {
                    sql: sql.to_string(),
                    orm_duration,
                    raw_duration,
                    orm_rows: orm_rows.len(),
                    raw_rows: 0,
                    consistent: false,
                    mismatch: Some(format!("Raw path error: {}", e)),
                })?;
                // 原生路径失败不影响 ORM 路径返回
                return Ok(orm_rows);
            }
            Err(_) => {
                self.stats.mismatches.fetch_add(1, Ordering::Relaxed);
                // H-5 修复：传播 handle_mismatch 的 Result（Panic 模式下返回 Internal 错误）
                self.handle_mismatch(ShadowComparison {
                    sql: sql.to_string(),
                    orm_duration,
                    raw_duration,
                    orm_rows: orm_rows.len(),
                    raw_rows: 0,
                    consistent: false,
                    mismatch: Some(format!("Raw path timeout after {:?}", self.config.timeout)),
                })?;
                return Ok(orm_rows);
            }
        };

        // 比较结果集
        let comparison = self.compare_rows(sql, orm_duration, raw_duration, &orm_rows, &raw_rows);
        if !comparison.consistent {
            self.stats.mismatches.fetch_add(1, Ordering::Relaxed);
            // H-5 修复：传播 handle_mismatch 的 Result（Panic 模式下返回 Internal 错误）
            self.handle_mismatch(comparison)?;
        }

        Ok(orm_rows)
    }

    /// 比较两路结果集
    fn compare_rows(
        &self,
        sql: &str,
        orm_duration: Duration,
        raw_duration: Duration,
        orm_rows: &[HashMap<String, Value>],
        raw_rows: &[HashMap<String, Value>],
    ) -> ShadowComparison {
        // 行数比较
        if orm_rows.len() != raw_rows.len() {
            return ShadowComparison {
                sql: sql.to_string(),
                orm_duration,
                raw_duration,
                orm_rows: orm_rows.len(),
                raw_rows: raw_rows.len(),
                consistent: false,
                mismatch: Some(format!(
                    "Row count mismatch: ORM={} vs Raw={}",
                    orm_rows.len(),
                    raw_rows.len()
                )),
            };
        }

        // 仅比较行数时直接返回一致
        if self.config.row_count_only {
            return ShadowComparison {
                sql: sql.to_string(),
                orm_duration,
                raw_duration,
                orm_rows: orm_rows.len(),
                raw_rows: raw_rows.len(),
                consistent: true,
                mismatch: None,
            };
        }

        // 逐行逐字段比较（限制最大行数）
        let limit = self.config.max_compare_rows.min(orm_rows.len());
        for (i, (orm_row, raw_row)) in orm_rows[..limit]
            .iter()
            .zip(raw_rows[..limit].iter())
            .enumerate()
        {
            if orm_row.len() != raw_row.len() {
                return ShadowComparison {
                    sql: sql.to_string(),
                    orm_duration,
                    raw_duration,
                    orm_rows: orm_rows.len(),
                    raw_rows: raw_rows.len(),
                    consistent: false,
                    mismatch: Some(format!(
                        "Row {} column count mismatch: ORM={} vs Raw={}",
                        i,
                        orm_row.len(),
                        raw_row.len()
                    )),
                };
            }
            for (key, orm_val) in orm_row {
                match raw_row.get(key) {
                    Some(raw_val) if !values_equal(orm_val, raw_val) => {
                        return ShadowComparison {
                            sql: sql.to_string(),
                            orm_duration,
                            raw_duration,
                            orm_rows: orm_rows.len(),
                            raw_rows: raw_rows.len(),
                            consistent: false,
                            mismatch: Some(format!(
                                "Row {} column '{}' value mismatch: ORM={:?} vs Raw={:?}",
                                i, key, orm_val, raw_val
                            )),
                        };
                    }
                    None => {
                        return ShadowComparison {
                            sql: sql.to_string(),
                            orm_duration,
                            raw_duration,
                            orm_rows: orm_rows.len(),
                            raw_rows: raw_rows.len(),
                            consistent: false,
                            mismatch: Some(format!(
                                "Row {} column '{}' missing in raw result",
                                i, key
                            )),
                        };
                    }
                    _ => {}
                }
            }
        }

        ShadowComparison {
            sql: sql.to_string(),
            orm_duration,
            raw_duration,
            orm_rows: orm_rows.len(),
            raw_rows: raw_rows.len(),
            consistent: true,
            mismatch: None,
        }
    }

    /// 根据策略处理不匹配
    ///
    /// 返回 `Err` 仅在 `MismatchAction::Panic` 模式下（H-5 修复：将 panic! 改为
    /// 返回 `Result`，由调用方决定如何处理）。其他模式返回 `Ok(())`。
    fn handle_mismatch(&self, comparison: ShadowComparison) -> Result<(), crate::DbError> {
        tracing::warn!(
            target: "sz_orm::shadow",
            sql = %comparison.sql,
            orm_rows = comparison.orm_rows,
            raw_rows = comparison.raw_rows,
            orm_us = ?comparison.orm_duration,
            raw_us = ?comparison.raw_duration,
            mismatch = ?comparison.mismatch,
            "Shadow traffic mismatch detected"
        );

        match self.on_mismatch {
            MismatchAction::Record => Ok(()),
            MismatchAction::Error => Ok(()),
            MismatchAction::Panic => Err(crate::DbError::Internal(format!(
                "Shadow traffic mismatch: SQL={:?} mismatch={:?}",
                comparison.sql, comparison.mismatch
            ))),
        }
    }
}

/// 值相等比较（容忍类型相近的等价，如 I64(42) vs I32(42)）
fn values_equal(a: &Value, b: &Value) -> bool {
    use Value::*;
    match (a, b) {
        (Null, Null) => true,
        (Bool(x), Bool(y)) => x == y,
        (I8(x), I8(y)) => x == y,
        (I8(x), I16(y)) => (*x as i16) == *y,
        (I16(x), I8(y)) => *x == (*y as i16),
        (I16(x), I16(y)) => x == y,
        (I16(x), I32(y)) => (*x as i32) == *y,
        (I32(x), I16(y)) => *x == (*y as i32),
        (I32(x), I32(y)) => x == y,
        (I32(x), I64(y)) => (*x as i64) == *y,
        (I64(x), I32(y)) => *x == (*y as i64),
        (I64(x), I64(y)) => x == y,
        (U8(x), U8(y)) => x == y,
        (U16(x), U16(y)) => x == y,
        (U32(x), U32(y)) => x == y,
        (U64(x), U64(y)) => x == y,
        (F32(x), F32(y)) => (x - y).abs() < 1e-6,
        (F32(x), F64(y)) => ((*x as f64) - y).abs() < 1e-6,
        (F64(x), F32(y)) => (x - (*y as f64)).abs() < 1e-6,
        (F64(x), F64(y)) => (x - y).abs() < 1e-9,
        (String(x), String(y)) => x == y,
        (Bytes(x), Bytes(y)) => x == y,
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockConnection;

    #[test]
    fn test_values_equal_numeric_cross_type() {
        assert!(values_equal(&Value::I32(42), &Value::I64(42)));
        assert!(values_equal(&Value::I8(1), &Value::I16(1)));
        assert!(!values_equal(&Value::I32(42), &Value::I64(43)));
    }

    #[test]
    fn test_values_equal_float_tolerance() {
        assert!(values_equal(&Value::F32(1.0), &Value::F64(1.0)));
        assert!(!values_equal(&Value::F32(1.0), &Value::F64(2.0)));
    }

    #[test]
    fn test_shadow_stats_mismatch_rate() {
        let stats = ShadowStats::default();
        assert_eq!(stats.mismatch_rate(), 0.0);
        stats.comparisons.store(10, Ordering::Relaxed);
        stats.mismatches.store(1, Ordering::Relaxed);
        assert!((stats.mismatch_rate() - 0.1).abs() < 1e-9);
    }

    #[test]
    fn test_shadow_comparison_latency_ratio() {
        let c = ShadowComparison {
            sql: "SELECT 1".into(),
            orm_duration: Duration::from_millis(10),
            raw_duration: Duration::from_millis(5),
            orm_rows: 1,
            raw_rows: 1,
            consistent: true,
            mismatch: None,
        };
        assert!((c.latency_ratio() - 2.0).abs() < 1e-9);

        let c_zero = ShadowComparison {
            sql: "SELECT 1".into(),
            orm_duration: Duration::from_millis(10),
            raw_duration: Duration::ZERO,
            orm_rows: 1,
            raw_rows: 1,
            consistent: true,
            mismatch: None,
        };
        assert_eq!(c_zero.latency_ratio(), 1.0);
    }

    #[test]
    fn test_shadow_stats_avg() {
        let stats = ShadowStats::default();
        assert_eq!(stats.avg_orm_us(), 0);
        assert_eq!(stats.avg_raw_us(), 0);

        stats.comparisons.store(4, Ordering::Relaxed);
        stats.orm_total_us.store(1000, Ordering::Relaxed);
        stats.raw_total_us.store(500, Ordering::Relaxed);
        assert_eq!(stats.avg_orm_us(), 250);
        assert_eq!(stats.avg_raw_us(), 125);
    }

    #[test]
    fn test_shadow_config_default() {
        let cfg = ShadowConfig::default();
        assert_eq!(cfg.timeout, Duration::from_secs(3));
        assert!(!cfg.row_count_only);
        assert_eq!(cfg.max_compare_rows, 10_000);
    }

    #[test]
    fn test_values_equal_null_bool_string_bytes() {
        assert!(values_equal(&Value::Null, &Value::Null));
        assert!(!values_equal(&Value::Null, &Value::Bool(true)));
        assert!(values_equal(&Value::Bool(true), &Value::Bool(true)));
        assert!(!values_equal(&Value::Bool(true), &Value::Bool(false)));
        assert!(values_equal(
            &Value::String("a".into()),
            &Value::String("a".into())
        ));
        assert!(!values_equal(
            &Value::String("a".into()),
            &Value::String("b".into())
        ));
        assert!(values_equal(
            &Value::Bytes(vec![1, 2, 3]),
            &Value::Bytes(vec![1, 2, 3])
        ));
        assert!(!values_equal(
            &Value::Bytes(vec![1, 2, 3]),
            &Value::Bytes(vec![1, 2, 4])
        ));
    }

    #[test]
    fn test_values_equal_unsigned_integers() {
        assert!(values_equal(&Value::U8(1), &Value::U8(1)));
        assert!(!values_equal(&Value::U8(1), &Value::U8(2)));
        assert!(values_equal(&Value::U16(100), &Value::U16(100)));
        assert!(!values_equal(&Value::U16(100), &Value::U16(200)));
        assert!(values_equal(&Value::U32(1000), &Value::U32(1000)));
        assert!(values_equal(&Value::U64(10000), &Value::U64(10000)));
    }

    #[test]
    fn test_values_equal_float_f32_f32_f64_f64() {
        assert!(values_equal(&Value::F32(3.14), &Value::F32(3.14)));
        assert!(!values_equal(&Value::F32(3.14), &Value::F32(2.71)));
        assert!(values_equal(&Value::F64(3.14159), &Value::F64(3.14159)));
        assert!(!values_equal(&Value::F64(3.14), &Value::F64(2.71)));
    }

    #[test]
    fn test_values_equal_i16_i32_cross_type() {
        assert!(values_equal(&Value::I16(100), &Value::I32(100)));
        assert!(values_equal(&Value::I32(100), &Value::I16(100)));
        assert!(!values_equal(&Value::I16(100), &Value::I32(200)));
        assert!(values_equal(&Value::I64(100), &Value::I32(100)));
        assert!(!values_equal(&Value::I64(100), &Value::I32(200)));
    }

    #[test]
    fn test_shadow_connection_new_and_stats() {
        let orm = MockConnection::new();
        let raw = MockConnection::new();
        let shadow = ShadowConnection::new(orm, raw, ShadowConfig::default());
        assert_eq!(shadow.stats().comparisons.load(Ordering::Relaxed), 0);
        assert_eq!(shadow.stats().mismatches.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_shadow_connection_with_mismatch_action() {
        let orm = MockConnection::new();
        let raw = MockConnection::new();
        let shadow = ShadowConnection::new(orm, raw, ShadowConfig::default())
            .with_mismatch_action(MismatchAction::Panic);
        assert_eq!(shadow.on_mismatch, MismatchAction::Panic);
    }

    #[tokio::test]
    async fn test_shadow_query_consistent() {
        let mut orm = MockConnection::new();
        let mut raw = MockConnection::new();
        orm.expect_any().with_rows(vec![]);
        raw.expect_any().with_rows(vec![]);
        let mut shadow = ShadowConnection::new(orm, raw, ShadowConfig::default());
        let rows = shadow.query_shadow("SELECT 1").await.unwrap();
        assert_eq!(rows.len(), 0);
        assert_eq!(shadow.stats().comparisons.load(Ordering::Relaxed), 1);
        assert_eq!(shadow.stats().mismatches.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn test_shadow_query_row_count_mismatch() {
        let mut orm = MockConnection::new();
        let mut raw = MockConnection::new();
        orm.expect_any()
            .with_rows(vec![vec![("id", Value::I32(1))]]);
        raw.expect_any().with_rows(vec![]);
        let mut shadow = ShadowConnection::new(orm, raw, ShadowConfig::default());
        let rows = shadow.query_shadow("SELECT * FROM t").await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(shadow.stats().mismatches.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn test_shadow_query_row_count_only_mode() {
        let mut orm = MockConnection::new();
        let mut raw = MockConnection::new();
        orm.expect_any()
            .with_rows(vec![vec![("id", Value::I32(1))]]);
        raw.expect_any()
            .with_rows(vec![vec![("id", Value::I32(2))]]);
        let cfg = ShadowConfig {
            row_count_only: true,
            ..ShadowConfig::default()
        };
        let mut shadow = ShadowConnection::new(orm, raw, cfg);
        shadow.query_shadow("SELECT * FROM t").await.unwrap();
        assert_eq!(shadow.stats().mismatches.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn test_shadow_query_value_mismatch_record() {
        let mut orm = MockConnection::new();
        let mut raw = MockConnection::new();
        orm.expect_any()
            .with_rows(vec![vec![("id", Value::I32(1))]]);
        raw.expect_any()
            .with_rows(vec![vec![("id", Value::I32(2))]]);
        let mut shadow = ShadowConnection::new(orm, raw, ShadowConfig::default())
            .with_mismatch_action(MismatchAction::Record);
        shadow.query_shadow("SELECT * FROM t").await.unwrap();
        assert_eq!(shadow.stats().mismatches.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn test_shadow_query_value_mismatch_panic() {
        let mut orm = MockConnection::new();
        let mut raw = MockConnection::new();
        orm.expect_any()
            .with_rows(vec![vec![("id", Value::I32(1))]]);
        raw.expect_any()
            .with_rows(vec![vec![("id", Value::I32(2))]]);
        let mut shadow = ShadowConnection::new(orm, raw, ShadowConfig::default())
            .with_mismatch_action(MismatchAction::Panic);
        let result = shadow.query_shadow("SELECT * FROM t").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_shadow_query_column_count_mismatch() {
        let mut orm = MockConnection::new();
        let mut raw = MockConnection::new();
        orm.expect_any().with_rows(vec![vec![
            ("id", Value::I32(1)),
            ("name", Value::String("a".into())),
        ]]);
        raw.expect_any()
            .with_rows(vec![vec![("id", Value::I32(1))]]);
        let mut shadow = ShadowConnection::new(orm, raw, ShadowConfig::default());
        shadow.query_shadow("SELECT * FROM t").await.unwrap();
        assert_eq!(shadow.stats().mismatches.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn test_shadow_query_column_missing_in_raw() {
        let mut orm = MockConnection::new();
        let mut raw = MockConnection::new();
        orm.expect_any()
            .with_rows(vec![vec![("a", Value::I32(1)), ("b", Value::I32(2))]]);
        raw.expect_any()
            .with_rows(vec![vec![("a", Value::I32(1)), ("c", Value::I32(2))]]);
        let mut shadow = ShadowConnection::new(orm, raw, ShadowConfig::default());
        shadow.query_shadow("SELECT * FROM t").await.unwrap();
        assert_eq!(shadow.stats().mismatches.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn test_shadow_query_consistent_with_data() {
        let mut orm = MockConnection::new();
        let mut raw = MockConnection::new();
        let row = vec![
            ("id", Value::I32(1)),
            ("name", Value::String("Alice".into())),
        ];
        orm.expect_any().with_rows(vec![row.clone()]);
        raw.expect_any().with_rows(vec![row]);
        let mut shadow = ShadowConnection::new(orm, raw, ShadowConfig::default());
        shadow.query_shadow("SELECT * FROM t").await.unwrap();
        assert_eq!(shadow.stats().mismatches.load(Ordering::Relaxed), 0);
        assert_eq!(shadow.stats().comparisons.load(Ordering::Relaxed), 1);
    }
}
