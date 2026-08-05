//! Mock 数据库连接 — 用于单元测试，无需真实数据库
//!
//! # 概述
//!
//! `MockConnection` 实现了 `Connection` trait，允许在测试中预设 SQL 查询的
//! 预期结果，从而在不连接真实数据库的情况下测试业务逻辑。
//!
//! # 使用示例
//!
//! ```ignore
//! use sz_orm_core::mock::{MockConnection, MockRow};
//! use sz_orm_core::value::Value;
//!
//! let mut mock = MockConnection::new();
//!
//! // 预设查询结果
//! mock.expect_query("SELECT * FROM users")
//!     .with_rows(vec![
//!         MockRow::from(vec![
//!             ("id", Value::from(1i64)),
//!             ("name", Value::from("Alice")),
//!         ]),
//!     ]);
//!
//! // 执行查询，返回预设结果
//! let rows = mock.query("SELECT * FROM users").await.unwrap();
//! assert_eq!(rows.len(), 1);
//! ```

use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;

use crate::pool::Connection;
use crate::pool::QueryRows;
use crate::value::Value;
use crate::DbError;

/// 一行 mock 数据
///
/// 由 `(列名, 值)` 对组成，与 `QueryRows` 的行格式一致。
pub type MockRow = Vec<(&'static str, Value)>;

/// 预设的查询期望
#[derive(Clone)]
struct QueryExpectation {
    /// 匹配的 SQL（None 表示匹配任意 SQL，作为 fallback）
    sql: Option<String>,
    /// 返回的行数据
    rows: Vec<MockRow>,
    /// 影响的行数（用于 execute）
    rows_affected: u64,
}

/// Mock 数据库连接
///
/// 用于单元测试，预设 SQL 查询的预期结果，无需真实数据库。
///
/// # 事务模拟
///
/// `MockConnection` 会跟踪 `begin`/`commit`/`rollback` 调用状态，
/// 可通过 `was_committed()` / `was_rolled_back()` 断言事务行为。
pub struct MockConnection {
    /// 预设的查询期望队列（FIFO）
    expectations: VecDeque<QueryExpectation>,
    /// 已执行的 SQL 记录（用于断言）
    executed_sql: Vec<String>,
    /// 事务状态
    in_transaction: bool,
    committed: bool,
    rolled_back: bool,
    /// 当无匹配期望时的行为
    fallback_behavior: FallbackBehavior,
}

/// 无匹配期望时的行为
#[derive(Clone, Copy, Debug, Default)]
pub enum FallbackBehavior {
    /// 返回空结果（默认）
    #[default]
    Empty,
    /// 返回错误
    Error,
}

impl MockConnection {
    /// 创建新的空 MockConnection
    pub fn new() -> Self {
        Self {
            expectations: VecDeque::new(),
            executed_sql: Vec::new(),
            in_transaction: false,
            committed: false,
            rolled_back: false,
            fallback_behavior: FallbackBehavior::Empty,
        }
    }

    /// 设置无匹配期望时的行为
    pub fn with_fallback(mut self, behavior: FallbackBehavior) -> Self {
        self.fallback_behavior = behavior;
        self
    }

    /// 预设一个查询期望
    ///
    /// 返回 `QueryExpectationBuilder` 用于链式配置结果。
    ///
    /// # 示例
    ///
    /// ```ignore
    /// mock.expect_query("SELECT * FROM users WHERE id = ?")
    ///     .with_rows(vec![vec![("id", Value::from(1i64))]]);
    /// ```
    pub fn expect_query(&mut self, sql: impl Into<String>) -> QueryExpectationBuilder<'_> {
        QueryExpectationBuilder {
            mock: self,
            sql: Some(sql.into()),
            rows: vec![],
            rows_affected: 0,
        }
    }

    /// 预设一个匹配任意 SQL 的 fallback 期望
    pub fn expect_any(&mut self) -> QueryExpectationBuilder<'_> {
        QueryExpectationBuilder {
            mock: self,
            sql: None,
            rows: vec![],
            rows_affected: 0,
        }
    }

    /// 预设 execute 的影响行数
    pub fn expect_execute(&mut self, sql: impl Into<String>, rows_affected: u64) {
        self.expectations.push_back(QueryExpectation {
            sql: Some(sql.into()),
            rows: vec![],
            rows_affected,
        });
    }

    /// 是否处于事务中
    pub fn in_transaction(&self) -> bool {
        self.in_transaction
    }

    /// 是否已提交
    pub fn was_committed(&self) -> bool {
        self.committed
    }

    /// 是否已回滚
    pub fn was_rolled_back(&self) -> bool {
        self.rolled_back
    }

    /// 获取已执行的 SQL 列表（用于断言）
    pub fn executed_sql(&self) -> &[String] {
        &self.executed_sql
    }

    /// 断言某 SQL 被执行了恰好 n 次
    pub fn assert_executed_count(&self, sql: &str, count: usize) {
        let actual = self.executed_sql.iter().filter(|s| *s == sql).count();
        assert!(
            actual == count,
            "SQL `{}` 预期执行 {} 次，实际 {} 次",
            sql,
            count,
            actual
        );
    }

    /// 查找匹配的期望（精确匹配优先，fallback 兜底）
    fn find_expectation(&mut self, sql: &str) -> Option<QueryExpectation> {
        // 精确匹配
        if let Some(pos) = self
            .expectations
            .iter()
            .position(|e| e.sql.as_deref() == Some(sql))
        {
            return Some(self.expectations.remove(pos).unwrap()); // SAFETY: pos 来自 position() 保证有效，remove 一定返回 Some
        }
        // Fallback 匹配
        if let Some(pos) = self.expectations.iter().position(|e| e.sql.is_none()) {
            return Some(self.expectations.remove(pos).unwrap()); // SAFETY: pos 来自 position() 保证有效，remove 一定返回 Some
        }
        None
    }

    fn handle_query(&mut self, sql: &str) -> Result<QueryRows, DbError> {
        self.executed_sql.push(sql.to_string());
        match self.find_expectation(sql) {
            Some(exp) => Ok(exp
                .rows
                .into_iter()
                .map(|row| row.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
                .collect()),
            None => match self.fallback_behavior {
                FallbackBehavior::Empty => Ok(Vec::new()),
                FallbackBehavior::Error => Err(DbError::Internal(format!(
                    "MockConnection: 未预设的查询 `{}`",
                    sql
                ))),
            },
        }
    }

    fn handle_execute(&mut self, sql: &str) -> Result<u64, DbError> {
        self.executed_sql.push(sql.to_string());
        match self.find_expectation(sql) {
            Some(exp) => Ok(exp.rows_affected),
            None => match self.fallback_behavior {
                FallbackBehavior::Empty => Ok(0),
                FallbackBehavior::Error => Err(DbError::Internal(format!(
                    "MockConnection: 未预设的 execute `{}`",
                    sql
                ))),
            },
        }
    }
}

impl Default for MockConnection {
    fn default() -> Self {
        Self::new()
    }
}

/// `expect_query` 的构建器
pub struct QueryExpectationBuilder<'a> {
    mock: &'a mut MockConnection,
    sql: Option<String>,
    rows: Vec<MockRow>,
    rows_affected: u64,
}

impl<'a> QueryExpectationBuilder<'a> {
    /// 设置返回的行数据
    pub fn with_rows(mut self, rows: Vec<MockRow>) -> &'a mut MockConnection {
        self.mock.expectations.push_back(QueryExpectation {
            sql: self.sql.take(),
            rows,
            rows_affected: self.rows_affected,
        });
        self.mock
    }

    /// 设置影响行数（用于 INSERT/UPDATE/DELETE）
    pub fn with_rows_affected(mut self, n: u64) -> &'a mut MockConnection {
        self.rows_affected = n;
        let rows = std::mem::take(&mut self.rows);
        self.with_rows(rows)
    }
}

// ---------------------------------------------------------------------------
// Connection trait 实现
// ---------------------------------------------------------------------------

impl Connection for MockConnection {
    fn execute<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<u64, DbError>> + Send + 'a>> {
        Box::pin(async move { self.handle_execute(sql) })
    }

    fn query<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<QueryRows, DbError>> + Send + 'a>> {
        Box::pin(async move { self.handle_query(sql) })
    }

    fn query_with_params<'a>(
        &'a mut self,
        sql: &'a str,
        _params: &'a [crate::value::Value],
    ) -> Pin<Box<dyn Future<Output = Result<QueryRows, DbError>> + Send + 'a>> {
        // MockConnection 将 query_with_params 委托给 query（mock 不实际绑定参数）
        Box::pin(async move { self.handle_query(sql) })
    }

    fn begin_transaction<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move {
            if self.in_transaction {
                return Err(DbError::Internal("MockConnection: 已在事务中".to_string()));
            }
            self.in_transaction = true;
            self.committed = false;
            self.rolled_back = false;
            Ok(())
        })
    }

    fn commit<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move {
            if !self.in_transaction {
                return Err(DbError::Internal("MockConnection: 未开启事务".to_string()));
            }
            self.in_transaction = false;
            self.committed = true;
            Ok(())
        })
    }

    fn rollback<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move {
            if !self.in_transaction {
                return Err(DbError::Internal("MockConnection: 未开启事务".to_string()));
            }
            self.in_transaction = false;
            self.rolled_back = true;
            Ok(())
        })
    }

    fn is_connected(&self) -> bool {
        true
    }

    fn ping<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move { true })
    }

    fn close<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(()) })
    }
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::Value;

    #[tokio::test]
    async fn test_mock_connection_basic_query() {
        let mut mock = MockConnection::new();
        mock.expect_query("SELECT * FROM users")
            .with_rows(vec![vec![
                ("id", Value::from(1i64)),
                ("name", Value::from("Alice")),
            ]]);

        let rows = mock.query("SELECT * FROM users").await.unwrap();
        assert_eq!(rows.len(), 1);
        let row = rows.first().unwrap();
        assert_eq!(row.get("id"), Some(&Value::from(1i64)));
        assert_eq!(row.get("name"), Some(&Value::from("Alice")));
    }

    #[tokio::test]
    async fn test_mock_connection_execute() {
        let mut mock = MockConnection::new();
        mock.expect_execute("INSERT INTO users (name) VALUES ('Bob')", 1);

        let affected = mock
            .execute("INSERT INTO users (name) VALUES ('Bob')")
            .await
            .unwrap();
        assert_eq!(affected, 1);
    }

    #[tokio::test]
    async fn test_mock_connection_fallback_empty() {
        let mut mock = MockConnection::new();
        // 未预设任何期望，默认返回空结果
        let rows = mock.query("SELECT * FROM anything").await.unwrap();
        assert_eq!(rows.len(), 0);
    }

    #[tokio::test]
    async fn test_mock_connection_fallback_error() {
        let mut mock = MockConnection::new().with_fallback(FallbackBehavior::Error);
        let result = mock.query("SELECT * FROM anything").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mock_connection_transaction_commit() {
        let mut mock = MockConnection::new();
        assert!(!mock.in_transaction());

        mock.begin_transaction().await.unwrap();
        assert!(mock.in_transaction());
        assert!(!mock.was_committed());

        mock.commit().await.unwrap();
        assert!(!mock.in_transaction());
        assert!(mock.was_committed());
        assert!(!mock.was_rolled_back());
    }

    #[tokio::test]
    async fn test_mock_connection_transaction_rollback() {
        let mut mock = MockConnection::new();
        mock.begin_transaction().await.unwrap();
        mock.rollback().await.unwrap();
        assert!(!mock.in_transaction());
        assert!(!mock.was_committed());
        assert!(mock.was_rolled_back());
    }

    #[tokio::test]
    async fn test_mock_connection_double_begin_errors() {
        let mut mock = MockConnection::new();
        mock.begin_transaction().await.unwrap();
        let result = mock.begin_transaction().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mock_connection_commit_without_transaction_errors() {
        let mut mock = MockConnection::new();
        let result = mock.commit().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mock_connection_executed_sql_tracking() {
        let mut mock = MockConnection::new();
        mock.expect_query("SELECT 1").with_rows(vec![]);
        mock.query("SELECT 1").await.unwrap();
        mock.query("SELECT 2").await.unwrap();

        assert_eq!(mock.executed_sql().len(), 2);
        assert_eq!(mock.executed_sql()[0], "SELECT 1");
        assert_eq!(mock.executed_sql()[1], "SELECT 2");
    }

    #[tokio::test]
    async fn test_mock_connection_assert_executed_count() {
        let mut mock = MockConnection::new();
        mock.expect_query("SELECT x").with_rows(vec![]);
        mock.query("SELECT x").await.unwrap();
        mock.query("SELECT x").await.unwrap();
        mock.query("SELECT y").await.unwrap();

        mock.assert_executed_count("SELECT x", 2);
        mock.assert_executed_count("SELECT y", 1);
    }

    #[tokio::test]
    async fn test_mock_connection_expect_any_fallback() {
        let mut mock = MockConnection::new();
        // 预设一个匹配任意 SQL 的 fallback
        mock.expectations.push_back(QueryExpectation {
            sql: None,
            rows: vec![vec![("a", Value::from(42i64))]],
            rows_affected: 0,
        });

        let rows = mock.query("ANY RANDOM SQL").await.unwrap();
        assert_eq!(rows.len(), 1);
    }

    #[tokio::test]
    async fn test_mock_connection_is_connected() {
        let mock = MockConnection::new();
        assert!(mock.is_connected());
    }

    #[tokio::test]
    async fn test_mock_connection_ping() {
        let mut mock = MockConnection::new();
        assert!(mock.ping().await);
    }

    #[tokio::test]
    async fn test_mock_connection_close() {
        let mut mock = MockConnection::new();
        assert!(mock.close().await.is_ok());
    }

    #[tokio::test]
    async fn test_mock_connection_multiple_rows() {
        let mut mock = MockConnection::new();
        mock.expect_query("SELECT * FROM products").with_rows(vec![
            vec![("id", Value::from(1i64)), ("name", Value::from("Widget"))],
            vec![("id", Value::from(2i64)), ("name", Value::from("Gadget"))],
            vec![
                ("id", Value::from(3i64)),
                ("name", Value::from("Doohickey")),
            ],
        ]);

        let rows = mock.query("SELECT * FROM products").await.unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(
            rows.first().unwrap().get("name"),
            Some(&Value::from("Widget"))
        );
        assert_eq!(
            rows.get(2).unwrap().get("name"),
            Some(&Value::from("Doohickey"))
        );
    }

    #[tokio::test]
    async fn test_mock_connection_exact_match_before_fallback() {
        let mut mock = MockConnection::new();
        // 先加 fallback
        mock.expectations.push_back(QueryExpectation {
            sql: None,
            rows: vec![vec![("source", Value::from("fallback"))]],
            rows_affected: 0,
        });
        // 再加精确匹配
        mock.expect_query("SELECT exact")
            .with_rows(vec![vec![("source", Value::from("exact"))]]);

        let rows = mock.query("SELECT exact").await.unwrap();
        assert_eq!(
            rows.first().unwrap().get("source"),
            Some(&Value::from("exact"))
        );
    }
}
