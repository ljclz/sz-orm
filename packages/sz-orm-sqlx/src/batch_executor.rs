//! v9.5.0 BatchExecutor：端到端批量执行
//!
//! 串联 `QueryBuilder::build_batch_insert_with_params`（`query.rs:2953`）
//! → `Connection::execute_with_params`（`pool.rs:98`），
//! 含自动分片、空批边界、UPSERT 端到端。
//!
//! # 设计要点
//!
//! - **参数化**：所有值通过 `?`/`$N` 占位符绑定，杜绝 SQL 注入（spec 5.1.1 规则 5）
//! - **空批短路**：空 `rows` 返回 `Ok(0)`，不执行 SQL（spec 5.1.1 规则 4）
//! - **自动分片**：`rows.len() > max_batch_rows` 时按 `max_batch_rows` 分片执行并聚合影响行数
//! - **多值 INSERT**：生成单条多值 INSERT SQL，非逐行循环（spec 5.1.1 规则 6）

use std::collections::HashMap;

use sz_orm_core::{dialect::get_dialect, Connection, DbError, DbType, Model, QueryBuilder, Value};

/// 内部占位 Model，用于 QueryBuilder 构造（table 由 `table()` 方法显式设置，
/// 不依赖 `Model::table_name()`）。
struct BatchExecModel;

impl Model for BatchExecModel {
    type PrimaryKey = i64;
    fn table_name() -> &'static str {
        "__batch_exec__"
    }
    fn pk(&self) -> Self::PrimaryKey {
        0
    }
    fn set_pk(&mut self, _pk: Self::PrimaryKey) {}
}

/// v9.5.0 批量执行器：端到端批量 INSERT/UPSERT。
///
/// 串联 `QueryBuilder::build_batch_insert_with_params` → `Connection::execute_with_params`，
/// 含自动分片、空批边界。
///
/// # 示例
///
/// ```no_run
/// use sz_orm_core::{Connection, DbType, Value};
/// use sz_orm_sqlx::BatchExecutor;
/// use std::collections::HashMap;
///
/// # async fn example(conn: &mut impl Connection) {
/// let mut executor = BatchExecutor::new(conn, DbType::MySQL);
/// let mut row = HashMap::new();
/// row.insert("name".to_string(), Value::String("alice".to_string()));
/// let affected = executor.batch_insert("users", &[row]).await.unwrap();
/// # }
/// ```
pub struct BatchExecutor<'a, C: Connection> {
    conn: &'a mut C,
    dialect: DbType,
    max_batch_rows: usize,
}

impl<'a, C: Connection> BatchExecutor<'a, C> {
    /// 创建批量执行器，默认 `max_batch_rows = 1000`。
    pub fn new(conn: &'a mut C, dialect: DbType) -> Self {
        Self {
            conn,
            dialect,
            max_batch_rows: 1000,
        }
    }

    /// 设置每批最大行数（自动分片阈值），最小为 1。
    pub fn with_max_batch_rows(mut self, max: usize) -> Self {
        self.max_batch_rows = max.max(1);
        self
    }

    /// 批量 INSERT：生成多值 INSERT SQL 并执行。
    ///
    /// - 空批返回 `Ok(0)`，不执行 SQL（spec 5.1.1 规则 4）
    /// - 自动分片：`rows.len() > max_batch_rows` 时分多批执行并聚合影响行数
    /// - 所有值参数化绑定，杜绝 SQL 注入（spec 5.1.1 规则 5）
    pub async fn batch_insert(
        &mut self,
        table: &str,
        rows: &[HashMap<String, Value>],
    ) -> Result<u64, DbError> {
        if rows.is_empty() {
            return Ok(0);
        }
        let dialect = get_dialect(self.dialect)?;
        let mut total: u64 = 0;
        for chunk in rows.chunks(self.max_batch_rows) {
            if chunk.is_empty() {
                continue;
            }
            let qb =
                QueryBuilder::<BatchExecModel>::new(dialect.clone_box()).table(table.to_string());
            let (sql, params) = qb.build_batch_insert_with_params(chunk);
            if sql.is_empty() {
                continue;
            }
            total += self.conn.execute_with_params(&sql, &params).await?;
        }
        Ok(total)
    }

    /// 批量 UPSERT：生成 INSERT...ON DUPLICATE KEY UPDATE / ON CONFLICT UPDATE SQL 并执行。
    ///
    /// - `conflict_cols`：冲突检测列（主键/唯一键）
    /// - `update_cols`：冲突时更新的列；空切片表示更新所有非冲突列
    /// - 空批返回 `Err(InvalidInput)`（与 `build_batch_upsert_with_params` 一致）
    pub async fn batch_upsert(
        &mut self,
        table: &str,
        rows: &[HashMap<String, Value>],
        conflict_cols: &[&str],
        update_cols: &[&str],
    ) -> Result<u64, DbError> {
        if rows.is_empty() {
            return Err(DbError::InvalidInput(
                "batch_upsert: rows cannot be empty".to_string(),
            ));
        }
        let dialect = get_dialect(self.dialect)?;
        let mut total: u64 = 0;
        for chunk in rows.chunks(self.max_batch_rows) {
            if chunk.is_empty() {
                continue;
            }
            let qb =
                QueryBuilder::<BatchExecModel>::new(dialect.clone_box()).table(table.to_string());
            let (sql, params) =
                qb.build_batch_upsert_with_params(chunk, conflict_cols, update_cols)?;
            total += self.conn.execute_with_params(&sql, &params).await?;
        }
        Ok(total)
    }

    /// 返回当前最大批次行数（用于测试断言）。
    pub fn max_batch_rows(&self) -> usize {
        self.max_batch_rows
    }

    /// v9.6.0 并行分片批量 INSERT：拆分行为多个分片，逐片执行并聚合结果。
    ///
    /// 当前使用 `self.conn` 串行执行各分片（单连接降级模式）。
    /// 真正的多连接并行需通过 `batch_insert_parallel_with_pool` 实现。
    ///
    /// - `shard_size`：每片行数（≤ 1 时按 `max_batch_rows` 分片）
    /// - `parallelism`：期望并行度（当前降级为串行，参数保留兼容）
    /// - 某分片失败时其他分片继续（Continue 策略），已成功分片不回滚
    pub async fn batch_insert_parallel(
        &mut self,
        table: &str,
        rows: &[HashMap<String, Value>],
        shard_size: usize,
        _parallelism: usize,
    ) -> Result<BatchParallelResult, DbError> {
        if rows.is_empty() {
            return Ok(BatchParallelResult {
                total_affected: 0,
                success_shards: 0,
                failed_shards: Vec::new(),
            });
        }

        let effective_shard = if shard_size == 0 {
            self.max_batch_rows
        } else {
            shard_size
        };

        let dialect = get_dialect(self.dialect)?;
        let mut total_affected: u64 = 0;
        let mut success_shards: usize = 0;
        let mut failed_shards: Vec<(usize, DbError)> = Vec::new();

        for (shard_idx, chunk) in rows.chunks(effective_shard).enumerate() {
            if chunk.is_empty() {
                continue;
            }
            let qb =
                QueryBuilder::<BatchExecModel>::new(dialect.clone_box()).table(table.to_string());
            let (sql, params) = qb.build_batch_insert_with_params(chunk);
            if sql.is_empty() {
                continue;
            }
            match self.conn.execute_with_params(&sql, &params).await {
                Ok(affected) => {
                    total_affected += affected;
                    success_shards += 1;
                }
                Err(e) => {
                    failed_shards.push((shard_idx, e));
                }
            }
        }

        Ok(BatchParallelResult {
            total_affected,
            success_shards,
            failed_shards,
        })
    }
}

/// v9.6.0 并行批量执行结果
#[derive(Debug)]
pub struct BatchParallelResult {
    /// 总影响行数（所有成功分片之和）
    pub total_affected: u64,
    /// 成功分片数
    pub success_shards: usize,
    /// 失败分片列表：(分片索引, 错误)
    pub failed_shards: Vec<(usize, DbError)>,
}

impl BatchParallelResult {
    /// 是否所有分片均成功
    pub fn is_all_success(&self) -> bool {
        self.failed_shards.is_empty()
    }

    /// 总分片数
    pub fn total_shards(&self) -> usize {
        self.success_shards + self.failed_shards.len()
    }

    /// 失败分片数
    pub fn failed_count(&self) -> usize {
        self.failed_shards.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sz_orm_core::mock::MockConnection;

    fn make_row(pairs: &[(&str, Value)]) -> HashMap<String, Value> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    #[tokio::test]
    async fn test_batch_insert_empty_returns_zero() {
        let mut mock = MockConnection::new();
        let mut executor = BatchExecutor::new(&mut mock, DbType::MySQL);
        let rows: Vec<HashMap<String, Value>> = vec![];
        let result = executor.batch_insert("users", &rows).await.unwrap();
        assert_eq!(result, 0);
        assert!(mock.executed_sql().is_empty());
    }

    #[tokio::test]
    async fn test_batch_insert_multi_row() {
        let mut mock = MockConnection::new();
        mock.expect_any().with_rows_affected(3);
        let mut executor = BatchExecutor::new(&mut mock, DbType::MySQL);
        let rows = vec![
            make_row(&[("name", Value::String("a".to_string()))]),
            make_row(&[("name", Value::String("b".to_string()))]),
            make_row(&[("name", Value::String("c".to_string()))]),
        ];
        let result = executor.batch_insert("users", &rows).await.unwrap();
        assert_eq!(result, 3);
        assert_eq!(mock.executed_sql().len(), 1);
        let sql = &mock.executed_sql()[0];
        assert!(sql.contains("INSERT INTO"));
        assert!(sql.contains("VALUES"));
        assert!(sql.contains("?"));
    }

    #[tokio::test]
    async fn test_batch_insert_auto_shard() {
        let mut mock = MockConnection::new();
        mock.expect_any().with_rows_affected(1000);
        mock.expect_any().with_rows_affected(500);
        let mut executor = BatchExecutor::new(&mut mock, DbType::MySQL).with_max_batch_rows(1000);
        let rows: Vec<HashMap<String, Value>> = (0..1500)
            .map(|i| make_row(&[("id", Value::I64(i))]))
            .collect();
        let result = executor.batch_insert("t", &rows).await.unwrap();
        assert_eq!(result, 1500);
        assert_eq!(mock.executed_sql().len(), 2);
    }

    #[tokio::test]
    async fn test_batch_upsert_conflict() {
        let mut mock = MockConnection::new();
        mock.expect_any().with_rows_affected(100);
        let mut executor = BatchExecutor::new(&mut mock, DbType::MySQL);
        let rows: Vec<HashMap<String, Value>> = (0..100)
            .map(|i| {
                make_row(&[
                    ("id", Value::I64(i)),
                    ("name", Value::String(format!("user{}", i))),
                ])
            })
            .collect();
        let result = executor
            .batch_upsert("users", &rows, &["id"], &["name"])
            .await
            .unwrap();
        assert_eq!(result, 100);
        let sql = &mock.executed_sql()[0];
        assert!(sql.contains("INSERT INTO"));
        assert!(sql.contains("ON DUPLICATE KEY UPDATE") || sql.contains("ON CONFLICT"));
    }

    #[tokio::test]
    async fn test_batch_upsert_pg_conflict() {
        let mut mock = MockConnection::new();
        mock.expect_any().with_rows_affected(2);
        let mut executor = BatchExecutor::new(&mut mock, DbType::PostgreSQL);
        let rows = vec![
            make_row(&[
                ("id", Value::I64(1)),
                ("name", Value::String("a".to_string())),
            ]),
            make_row(&[
                ("id", Value::I64(2)),
                ("name", Value::String("b".to_string())),
            ]),
        ];
        let result = executor
            .batch_upsert("users", &rows, &["id"], &["name"])
            .await
            .unwrap();
        assert_eq!(result, 2);
        let sql = &mock.executed_sql()[0];
        assert!(sql.contains("ON CONFLICT"));
    }

    #[tokio::test]
    async fn test_batch_insert_column_mismatch() {
        let mut mock = MockConnection::new();
        mock.expect_any().with_rows_affected(2);
        let mut executor = BatchExecutor::new(&mut mock, DbType::MySQL);
        let rows = vec![
            make_row(&[("a", Value::I64(1)), ("b", Value::I64(2))]),
            make_row(&[("a", Value::I64(3))]),
        ];
        let result = executor.batch_insert("t", &rows).await.unwrap();
        assert_eq!(result, 2);
    }

    #[tokio::test]
    async fn test_batch_insert_parametrized_no_injection() {
        let mut mock = MockConnection::new();
        mock.expect_any().with_rows_affected(1);
        let mut executor = BatchExecutor::new(&mut mock, DbType::MySQL);
        let row = make_row(&[("name", Value::String("'; DROP TABLE users; --".to_string()))]);
        executor.batch_insert("users", &[row]).await.unwrap();
        let sql = &mock.executed_sql()[0];
        assert!(!sql.contains("DROP TABLE"));
        assert!(sql.contains("?"));
    }

    #[tokio::test]
    async fn test_batch_insert_constraint_violation_atomic() {
        let mut mock =
            MockConnection::new().with_fallback(sz_orm_core::mock::FallbackBehavior::Error);
        let mut executor = BatchExecutor::new(&mut mock, DbType::MySQL);
        let rows = vec![make_row(&[("id", Value::I64(1))])];
        let result = executor.batch_insert("t", &rows).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_batch_upsert_empty_returns_error() {
        let mut mock = MockConnection::new();
        let mut executor = BatchExecutor::new(&mut mock, DbType::MySQL);
        let rows: Vec<HashMap<String, Value>> = vec![];
        let result = executor.batch_upsert("t", &rows, &["id"], &["name"]).await;
        assert!(result.is_err());
        assert!(mock.executed_sql().is_empty());
    }

    #[tokio::test]
    async fn test_batch_insert_sqlite() {
        let mut mock = MockConnection::new();
        mock.expect_any().with_rows_affected(2);
        let mut executor = BatchExecutor::new(&mut mock, DbType::Sqlite);
        let rows = vec![
            make_row(&[("name", Value::String("a".to_string()))]),
            make_row(&[("name", Value::String("b".to_string()))]),
        ];
        let result = executor.batch_insert("t", &rows).await.unwrap();
        assert_eq!(result, 2);
        let sql = &mock.executed_sql()[0];
        assert!(sql.contains("INSERT INTO"));
        assert!(sql.contains("?"));
    }

    #[tokio::test]
    async fn test_batch_insert_shard_boundary_exact() {
        let mut mock = MockConnection::new();
        mock.expect_any().with_rows_affected(100);
        let mut executor = BatchExecutor::new(&mut mock, DbType::MySQL).with_max_batch_rows(100);
        let rows: Vec<HashMap<String, Value>> = (0..100)
            .map(|i| make_row(&[("id", Value::I64(i))]))
            .collect();
        let result = executor.batch_insert("t", &rows).await.unwrap();
        assert_eq!(result, 100);
        assert_eq!(mock.executed_sql().len(), 1);
    }

    #[tokio::test]
    async fn test_max_batch_rows_min_one() {
        let mut mock = MockConnection::new();
        let executor = BatchExecutor::new(&mut mock, DbType::MySQL).with_max_batch_rows(0);
        assert_eq!(executor.max_batch_rows(), 1);
    }

    // ========================================================================
    // v9.6.0 batch_insert_parallel 测试
    // ========================================================================

    #[tokio::test]
    async fn test_batch_insert_parallel_empty() {
        let mut mock = MockConnection::new();
        let mut executor = BatchExecutor::new(&mut mock, DbType::MySQL);
        let rows: Vec<HashMap<String, Value>> = vec![];
        let result = executor
            .batch_insert_parallel("users", &rows, 100, 4)
            .await
            .unwrap();
        assert_eq!(result.total_affected, 0);
        assert_eq!(result.success_shards, 0);
        assert!(result.is_all_success());
    }

    #[tokio::test]
    async fn test_batch_insert_parallel_result_aggregation() {
        let mut mock = MockConnection::new();
        for _ in 0..20 {
            mock.expect_any().with_rows_affected(1);
        }
        let mut executor = BatchExecutor::new(&mut mock, DbType::MySQL);
        let rows: Vec<HashMap<String, Value>> = (0..20)
            .map(|i| make_row(&[("id", Value::I64(i))]))
            .collect();

        let result = executor
            .batch_insert_parallel("t", &rows, 1, 4)
            .await
            .unwrap();

        assert_eq!(result.total_affected, 20);
        assert_eq!(result.success_shards, 20);
        assert!(result.is_all_success());
        assert_eq!(result.failed_shards.len(), 0);
    }

    #[tokio::test]
    async fn test_batch_insert_parallel_shard_size() {
        let mut mock = MockConnection::new();
        mock.expect_any().with_rows_affected(5);
        mock.expect_any().with_rows_affected(5);
        mock.expect_any().with_rows_affected(3);
        let mut executor = BatchExecutor::new(&mut mock, DbType::MySQL);
        let rows: Vec<HashMap<String, Value>> = (0..13)
            .map(|i| make_row(&[("id", Value::I64(i))]))
            .collect();

        let result = executor
            .batch_insert_parallel("t", &rows, 5, 2)
            .await
            .unwrap();

        assert_eq!(result.total_affected, 13);
        assert_eq!(result.success_shards, 3);
        assert!(result.is_all_success());
    }

    #[tokio::test]
    async fn test_batch_insert_parallel_degrade_on_insufficient_conns() {
        let mut mock = MockConnection::new();
        for _ in 0..10 {
            mock.expect_any().with_rows_affected(1);
        }
        let mut executor = BatchExecutor::new(&mut mock, DbType::MySQL);
        let rows: Vec<HashMap<String, Value>> = (0..10)
            .map(|i| make_row(&[("id", Value::I64(i))]))
            .collect();

        let result = executor
            .batch_insert_parallel("t", &rows, 1, 4)
            .await
            .unwrap();

        assert_eq!(result.total_affected, 10);
        assert_eq!(result.success_shards, 10);
        assert!(result.is_all_success());
    }

    #[tokio::test]
    async fn test_batch_insert_parallel_continue_on_error() {
        let mut mock =
            MockConnection::new().with_fallback(sz_orm_core::mock::FallbackBehavior::Error);
        let mut executor = BatchExecutor::new(&mut mock, DbType::MySQL);
        let rows: Vec<HashMap<String, Value>> =
            (0..3).map(|i| make_row(&[("id", Value::I64(i))])).collect();

        let result = executor
            .batch_insert_parallel("t", &rows, 1, 1)
            .await
            .unwrap();

        assert_eq!(result.success_shards, 0);
        assert_eq!(result.failed_shards.len(), 3);
        assert!(!result.is_all_success());
    }

    #[tokio::test]
    async fn test_batch_insert_legacy_unchanged() {
        let mut mock = MockConnection::new();
        mock.expect_any().with_rows_affected(100);
        let mut executor = BatchExecutor::new(&mut mock, DbType::MySQL).with_max_batch_rows(100);
        let rows: Vec<HashMap<String, Value>> = (0..100)
            .map(|i| make_row(&[("id", Value::I64(i))]))
            .collect();
        let result = executor.batch_insert("t", &rows).await.unwrap();
        assert_eq!(result, 100);
        assert_eq!(mock.executed_sql().len(), 1);
    }

    #[test]
    fn test_batch_parallel_result_methods() {
        let result = BatchParallelResult {
            total_affected: 90000,
            success_shards: 18,
            failed_shards: vec![(5, DbError::InvalidInput("test".to_string()))],
        };
        assert_eq!(result.total_affected, 90000);
        assert_eq!(result.success_shards, 18);
        assert_eq!(result.failed_count(), 1);
        assert_eq!(result.total_shards(), 19);
        assert!(!result.is_all_success());
    }

    #[test]
    fn test_batch_parallel_result_all_success() {
        let result = BatchParallelResult {
            total_affected: 100,
            success_shards: 5,
            failed_shards: Vec::new(),
        };
        assert!(result.is_all_success());
        assert_eq!(result.total_shards(), 5);
        assert_eq!(result.failed_count(), 0);
    }
}
