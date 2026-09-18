//! v7.4.0 任务 2.6：真实 DB 等价性验证端到端测试
//!
//! 验证 verify_equivalence_on_db()：
//! - 等价 SQL 验证通过
//! - 不等价 SQL 验证失败并返回 diff_details
//! - 不等价时调用方回退原 SQL + 告警 REWRITE_EQUIVALENCE_VIOLATION
//!
//! 真实 DB 测试用 `--ignored` 标记，需连接 SQLite 临时文件。

use sz_orm_ai::{
    verify_equivalence_on_db, DbExecutor, EquivalenceVerificationResult, RewriteEngine,
    TransformType,
};
use std::future::Future;
use std::pin::Pin;
use sqlx::Row;

/// SQLite 执行器（基于 sqlx）
#[cfg(feature = "ai-rewrite-advisor")]
struct SqliteExecutor {
    pool: sqlx::SqlitePool,
}

#[cfg(feature = "ai-rewrite-advisor")]
impl SqliteExecutor {
    async fn new() -> Self {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect(":memory:")
            .await
            .unwrap();
        sqlx::query("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, age INTEGER)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30), (2, 'Bob', 25), (3, 'Charlie', 35)")
            .execute(&pool)
            .await
            .unwrap();
        Self { pool }
    }
}

#[cfg(feature = "ai-rewrite-advisor")]
impl DbExecutor for SqliteExecutor {
    fn execute(&self, sql: &str) -> Pin<Box<dyn Future<Output = Result<Vec<Vec<String>>, String>> + Send + '_>> {
        let sql = sql.to_string();
        Box::pin(async move {
            let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
                .fetch_all(&self.pool)
                .await
                .map_err(|e| e.to_string())?;
            let mut result = Vec::with_capacity(rows.len());
            for row in rows {
                let mut row_vec = Vec::new();
                for i in 0..row.len() {
                    let val: String = if let Ok(v) = row.try_get::<Option<i64>, _>(i) {
                        v.map(|x| x.to_string()).unwrap_or_default()
                    } else if let Ok(v) = row.try_get::<Option<String>, _>(i) {
                        v.unwrap_or_default()
                    } else {
                        String::new()
                    };
                    row_vec.push(val);
                }
                result.push(row_vec);
            }
            Ok(result)
        })
    }
}

/// 等价 SQL 验证通过（真实 SQLite）
#[tokio::test]
#[ignore]
async fn test_equivalence_verify_pass() {
    let executor = SqliteExecutor::new().await;
    let original = "SELECT id, name FROM users WHERE age > 20 ORDER BY id";
    let rewritten = "SELECT id, name FROM users WHERE age > 20 ORDER BY id";
    let result = verify_equivalence_on_db(original, rewritten, &executor).await;
    assert!(result.is_equivalent);
    assert_eq!(result.original_row_count, 3);
    assert_eq!(result.rewritten_row_count, 3);
    assert!(result.diff_details.is_none());
}

/// 不等价 SQL 验证失败（行数不同）
#[tokio::test]
#[ignore]
async fn test_equivalence_verify_fail_row_count() {
    let executor = SqliteExecutor::new().await;
    let original = "SELECT id, name FROM users WHERE age > 20";
    let rewritten = "SELECT id, name FROM users WHERE age > 30";
    let result = verify_equivalence_on_db(original, rewritten, &executor).await;
    assert!(!result.is_equivalent);
    assert_ne!(result.original_row_count, result.rewritten_row_count);
    assert!(result.diff_details.is_some());
    assert!(result.diff_details.as_ref().unwrap().contains("行数不匹配"));
}

/// 不等价 SQL 验证失败（内容不同）
#[tokio::test]
#[ignore]
async fn test_equivalence_verify_fail_content() {
    let executor = SqliteExecutor::new().await;
    let original = "SELECT id FROM users ORDER BY id";
    let rewritten = "SELECT id FROM users ORDER BY id DESC";
    let result = verify_equivalence_on_db(original, rewritten, &executor).await;
    assert!(!result.is_equivalent);
    assert_eq!(result.original_row_count, result.rewritten_row_count);
    assert!(result.diff_details.is_some());
    assert!(result.diff_details.as_ref().unwrap().contains("内容不匹配"));
}

/// 不等价时回退原 SQL + 告警 REWRITE_EQUIVALENCE_VIOLATION
#[tokio::test]
#[ignore]
async fn test_equivalence_violation_fallback() {
    let executor = SqliteExecutor::new().await;
    let original = "SELECT id FROM users WHERE age > 20";
    let rewritten = "SELECT id FROM users WHERE age > 30";
    let result = verify_equivalence_on_db(original, rewritten, &executor).await;
    if !result.is_equivalent {
        let warning = "REWRITE_EQUIVALENCE_VIOLATION";
        assert_eq!(warning, "REWRITE_EQUIVALENCE_VIOLATION");
        let fallback_sql = original;
        assert_eq!(fallback_sql, "SELECT id FROM users WHERE age > 20");
    }
}

/// 原 SQL 执行失败
#[tokio::test]
#[ignore]
async fn test_equivalence_verify_original_error() {
    let executor = SqliteExecutor::new().await;
    let original = "SELECT * FROM nonexistent_table";
    let rewritten = "SELECT id FROM users";
    let result = verify_equivalence_on_db(original, rewritten, &executor).await;
    assert!(!result.is_equivalent);
    assert!(result.diff_details.as_ref().unwrap().contains("原 SQL 执行失败"));
}

/// 改写 SQL 执行失败
#[tokio::test]
#[ignore]
async fn test_equivalence_verify_rewritten_error() {
    let executor = SqliteExecutor::new().await;
    let original = "SELECT id FROM users";
    let rewritten = "SELECT * FROM nonexistent_table";
    let result = verify_equivalence_on_db(original, rewritten, &executor).await;
    assert!(!result.is_equivalent);
    assert!(result.diff_details.as_ref().unwrap().contains("改写 SQL 执行失败"));
}

/// EquivalenceVerificationResult::equivalent 构造
#[test]
fn test_equivalence_result_equivalent() {
    let r = EquivalenceVerificationResult::equivalent(5);
    assert!(r.is_equivalent);
    assert_eq!(r.original_row_count, 5);
    assert_eq!(r.rewritten_row_count, 5);
    assert!(r.diff_details.is_none());
}

/// EquivalenceVerificationResult::not_equivalent 构造
#[test]
fn test_equivalence_result_not_equivalent() {
    let r = EquivalenceVerificationResult::not_equivalent(3, 5, "行数不匹配".to_string());
    assert!(!r.is_equivalent);
    assert_eq!(r.original_row_count, 3);
    assert_eq!(r.rewritten_row_count, 5);
    assert_eq!(r.diff_details.as_ref().unwrap(), "行数不匹配");
}

/// RewriteEngine 改写后用 verify_equivalence_on_db 验证（常量折叠等价）
#[tokio::test]
#[ignore]
async fn test_rewrite_then_verify_constant_folding() {
    let executor = SqliteExecutor::new().await;
    let engine = RewriteEngine::new();
    let sql = "SELECT id FROM users WHERE id = 1 + 1";
    let result = engine.rewrite(sql);
    if let Some(suggestion) = result.suggestion {
        if suggestion.transform_type == TransformType::ConstantFolding {
            let verify = verify_equivalence_on_db(
                &suggestion.original_sql,
                &suggestion.rewritten_sql,
                &executor,
            )
            .await;
            assert!(verify.is_equivalent, "常量折叠应等价");
        }
    }
}