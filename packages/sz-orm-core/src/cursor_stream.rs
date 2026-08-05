//! 游标式流式查询（P1-2：Oracle/MSSQL 游标）
//!
//! 为无原生服务器端游标（或无法便捷暴露逐行拉取）的数据库提供
//! **分页游标**式的流式查询：将原始 SQL 包装为分批拉取的分页查询，
//! 按固定 batch 逐页获取并逐行 yield，避免大结果集全量载入内存。
//!
//! 支持的方言：
//! - Oracle：`ROWNUM` 子查询包装（`ROWNUM <= end AND rn > start`）
//! - SQL Server：`OFFSET ... ROWS FETCH NEXT ... ROWS ONLY`
//! - MySQL / PostgreSQL / SQLite：`LIMIT ... OFFSET ...`
//!
//! # 注意
//!
//! - 原始 SQL **不应以分号结尾**（会被包装为子查询）；
//! - 分页在无 `ORDER BY` 时结果顺序不确定（分页一致性由调用方保证）；
//! - 每页重复执行查询（每页一次），适合超大结果集的顺序消费。

use crate::db_type::DbType;
use crate::pool::{Connection, QueryStreamItem};
use crate::DbError;
use std::pin::Pin;

/// 将原始 SQL 包装为分页查询。
///
/// - `offset`：跳过行数（0-based）；
/// - `batch`：每页行数（>0）。
///
/// 返回包装后的分页 SQL；不支持分页的方言返回 `DbError::Unsupported`。
pub fn build_paged_query(
    db_type: DbType,
    sql: &str,
    offset: u64,
    batch: u64,
) -> Result<String, DbError> {
    let sql = sql.trim().trim_end_matches(';').trim();
    if sql.is_empty() {
        return Err(DbError::InvalidInput("build_paged_query: SQL 为空".into()));
    }
    if batch == 0 {
        return Err(DbError::InvalidInput(
            "build_paged_query: batch 必须大于 0".into(),
        ));
    }
    match db_type {
        // Oracle：ROWNUM 子查询包装，外层剔除游标列
        DbType::Oracle => {
            let end = offset + batch;
            Ok(format!(
                "SELECT * FROM (SELECT t.*, ROWNUM AS __sz_rn FROM ({}) t \
                 WHERE ROWNUM <= {}) WHERE __sz_rn > {}",
                sql, end, offset
            ))
        }
        // SQL Server 2012+：OFFSET/FETCH（ORDER BY (SELECT NULL) 兼容无 ORDER BY 的 SQL）
        DbType::SqlServer => Ok(format!(
            "SELECT * FROM ({}) AS __sz_t ORDER BY (SELECT NULL) \
             OFFSET {} ROWS FETCH NEXT {} ROWS ONLY",
            sql, offset, batch
        )),
        // MySQL / PostgreSQL / SQLite
        DbType::MySQL | DbType::PostgreSQL | DbType::Sqlite | DbType::OceanBase => {
            Ok(format!("{} LIMIT {} OFFSET {}", sql, batch, offset))
        }
        _ => Err(DbError::Unsupported(format!(
            "build_paged_query: {:?} 方言不支持分页游标",
            db_type
        ))),
    }
}

/// 基于分页游标的流式查询执行器。
///
/// 按 `batch` 行一页循环调用 `conn.query()` 拉取，逐行 yield；
/// 某页返回空行即结束。任一页查询出错则 yield 单个 `Err` 后结束。
///
/// # 生命周期
///
/// 返回的 Stream 借用 `conn` 与 `sql`，需保持二者在消费期间存活。
pub fn stream_cursor_paged<'a>(
    conn: &'a mut dyn Connection,
    sql: &'a str,
    db_type: DbType,
    batch: u64,
) -> Pin<Box<dyn futures::Stream<Item = QueryStreamItem> + Send + 'a>> {
    // 状态：(连接, 当前 offset, 当前页剩余行[逆序存储], 是否已终止)
    // 页内行逆序 push，pop 时即为正序 yield。
    let stream = futures::stream::unfold(
        (conn, 0u64, Vec::<QueryStreamItem>::new(), false),
        move |(conn, mut offset, mut page, done)| async move {
            if done {
                return None;
            }
            loop {
                if let Some(item) = page.pop() {
                    return Some((item, (conn, offset, page, false)));
                }
                // 当前页耗尽：拉取下一页
                let paged = match build_paged_query(db_type, sql, offset, batch) {
                    Ok(s) => s,
                    Err(e) => return Some((Err(e), (conn, offset, Vec::new(), true))),
                };
                match conn.query(&paged).await {
                    Ok(rows) if rows.is_empty() => return None,
                    Ok(rows) => {
                        offset += rows.len() as u64;
                        page = rows.into_iter().map(Ok).rev().collect();
                        // 继续循环，弹出本页首行
                    }
                    Err(e) => return Some((Err(e), (conn, offset, Vec::new(), true))),
                }
            }
        },
    );
    Box::pin(stream)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pool::QueryRows;
    use crate::value::Value;
    use std::collections::HashMap;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    // ---- build_paged_query 单测 ----

    #[test]
    fn test_build_paged_query_oracle() {
        let sql = build_paged_query(
            DbType::Oracle,
            "SELECT id, name FROM users ORDER BY id",
            10,
            100,
        )
        .unwrap();
        assert!(sql.contains("SELECT * FROM (SELECT t.*, ROWNUM AS __sz_rn FROM (SELECT id, name FROM users ORDER BY id) t WHERE ROWNUM <= 110) WHERE __sz_rn > 10"), "Oracle 包装 SQL 不正确: {}", sql);
    }

    #[test]
    fn test_build_paged_query_oracle_strips_semicolon() {
        let sql = build_paged_query(DbType::Oracle, "SELECT * FROM t;", 0, 50).unwrap();
        assert!(sql.contains("FROM (SELECT * FROM t) t"));
    }

    #[test]
    fn test_build_paged_query_sqlserver() {
        let sql = build_paged_query(DbType::SqlServer, "SELECT id FROM orders", 20, 50).unwrap();
        assert!(
            sql.contains("OFFSET 20 ROWS FETCH NEXT 50 ROWS ONLY"),
            "MSSQL 包装 SQL 不正确: {}",
            sql
        );
        assert!(sql.contains("ORDER BY (SELECT NULL)"));
    }

    #[test]
    fn test_build_paged_query_mysql_postgres_sqlite() {
        for db in [DbType::MySQL, DbType::PostgreSQL, DbType::Sqlite] {
            let sql = build_paged_query(db, "SELECT id FROM t WHERE id > 0", 5, 10).unwrap();
            assert_eq!(sql, "SELECT id FROM t WHERE id > 0 LIMIT 10 OFFSET 5");
        }
    }

    #[test]
    fn test_build_paged_query_unsupported_dialect() {
        let r = build_paged_query(DbType::Redis, "SELECT 1", 0, 10);
        assert!(r.is_err(), "Redis 方言不应支持分页游标");
        let r2 = build_paged_query(DbType::ClickHouse, "SELECT 1", 0, 10);
        assert!(r2.is_err());
    }

    #[test]
    fn test_build_paged_query_invalid_args() {
        assert!(
            build_paged_query(DbType::MySQL, "", 0, 10).is_err(),
            "空 SQL 应报错"
        );
        assert!(
            build_paged_query(DbType::MySQL, "SELECT 1", 0, 0).is_err(),
            "batch=0 应报错"
        );
        assert!(
            build_paged_query(DbType::MySQL, ";", 0, 10).is_err(),
            "仅分号应报错"
        );
    }

    // ---- stream_cursor_paged 集成测试 ----

    /// 测试桩连接：query 解析 `LIMIT n OFFSET m` 返回预设行的对应分页；
    /// 记录 query 调用次数。
    struct PagedStubConn {
        rows: Vec<HashMap<String, Value>>,
        query_calls: Arc<AtomicUsize>,
    }

    impl PagedStubConn {
        fn new(rows: Vec<HashMap<String, Value>>) -> (Self, Arc<AtomicUsize>) {
            let calls = Arc::new(AtomicUsize::new(0));
            (
                Self {
                    rows,
                    query_calls: Arc::clone(&calls),
                },
                calls,
            )
        }

        fn make_row(id: i64) -> HashMap<String, Value> {
            let mut m = HashMap::new();
            m.insert("id".to_string(), Value::I64(id));
            m
        }
    }

    impl Connection for PagedStubConn {
        fn execute<'a>(
            &'a mut self,
            _sql: &'a str,
        ) -> Pin<Box<dyn Future<Output = Result<u64, crate::DbError>> + Send + 'a>> {
            Box::pin(async move { Ok(0) })
        }

        fn query<'a>(
            &'a mut self,
            sql: &'a str,
        ) -> Pin<Box<dyn Future<Output = Result<QueryRows, crate::DbError>> + Send + 'a>> {
            // 解析 "LIMIT {n} OFFSET {m}"；同步取快照（async 块外完成借用）
            let limit = sql
                .split("LIMIT ")
                .nth(1)
                .and_then(|s| s.split(' ').next())
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(0);
            let offset = sql
                .split("OFFSET ")
                .nth(1)
                .and_then(|s| s.trim().parse::<usize>().ok())
                .unwrap_or(0);
            let snapshot: Vec<_> = self.rows.iter().skip(offset).take(limit).cloned().collect();
            self.query_calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move { Ok(snapshot) })
        }

        fn begin_transaction<'a>(
            &'a mut self,
        ) -> Pin<Box<dyn Future<Output = Result<(), crate::DbError>> + Send + 'a>> {
            Box::pin(async move { Ok(()) })
        }

        fn commit<'a>(
            &'a mut self,
        ) -> Pin<Box<dyn Future<Output = Result<(), crate::DbError>> + Send + 'a>> {
            Box::pin(async move { Ok(()) })
        }

        fn rollback<'a>(
            &'a mut self,
        ) -> Pin<Box<dyn Future<Output = Result<(), crate::DbError>> + Send + 'a>> {
            Box::pin(async move { Ok(()) })
        }

        fn is_connected(&self) -> bool {
            true
        }

        fn ping<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
            Box::pin(async move { true })
        }

        fn close<'a>(
            &'a mut self,
        ) -> Pin<Box<dyn Future<Output = Result<(), crate::DbError>> + Send + 'a>> {
            Box::pin(async move { Ok(()) })
        }
    }

    #[tokio::test]
    async fn test_stream_cursor_paged_fetches_in_batches() {
        let rows: Vec<_> = (1..=5).map(PagedStubConn::make_row).collect();
        let (mut conn, calls) = PagedStubConn::new(rows);
        let mut stream = stream_cursor_paged(&mut conn, "SELECT id FROM t", DbType::MySQL, 2);
        use futures::StreamExt;
        let ids: Vec<i64> = {
            let mut out = Vec::new();
            while let Some(item) = stream.next().await {
                let row = item.unwrap();
                if let Value::I64(id) = row["id"] {
                    out.push(id);
                }
            }
            out
        };
        assert_eq!(ids, vec![1, 2, 3, 4, 5], "行序应保持");
        // 5 行 batch=2 → 4 次查询（2+2+1 数据页 + 1 次空页终止探测）
        assert_eq!(calls.load(Ordering::SeqCst), 4);
    }

    #[tokio::test]
    async fn test_stream_cursor_paged_empty() {
        let (mut conn, calls) = PagedStubConn::new(Vec::new());
        let mut stream = stream_cursor_paged(&mut conn, "SELECT id FROM t", DbType::MySQL, 2);
        use futures::StreamExt;
        let mut count = 0;
        while let Some(item) = stream.next().await {
            let _ = item.unwrap();
            count += 1;
        }
        assert_eq!(count, 0, "空结果不应 yield 任何行");
        assert_eq!(calls.load(Ordering::SeqCst), 1, "空结果应只查一次");
    }

    #[tokio::test]
    async fn test_stream_cursor_paged_batch_larger_than_rows() {
        let rows: Vec<_> = (1..=3).map(PagedStubConn::make_row).collect();
        let (mut conn, calls) = PagedStubConn::new(rows);
        let mut stream = stream_cursor_paged(&mut conn, "SELECT id FROM t", DbType::MySQL, 10);
        use futures::StreamExt;
        let mut count = 0;
        while let Some(item) = stream.next().await {
            let _ = item.unwrap();
            count += 1;
        }
        assert_eq!(count, 3);
        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "3 行 batch=10 → 首页 3 行 + 尾页空"
        );
    }
}
