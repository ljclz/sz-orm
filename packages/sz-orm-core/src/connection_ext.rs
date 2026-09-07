//! v6.5.0 ConnectionExt trait — 接入层扩展
//!
//! 为 `Connection` trait 提供三个扩展方法：
//! - `prepare_cached`：PreparedStatement 句柄复用（委托 `PreparedStatementCache`）
//! - `query_stream_unified`：统一流式结果集（返回 `Box<dyn AsyncRowStream>`）
//! - `execute_batch_parallel`：批量 DML 并行化（默认串行，适配器可覆盖）
//!
//! 所有方法提供默认实现（向后兼容），适配器层可覆盖以接入真实功能。

use std::future::Future;
use std::pin::Pin;

use crate::error::DbError;
use crate::pool::{Connection, QueryRows};
use crate::value::Value;

#[cfg(feature = "prepared-stmt-cache")]
use crate::prepared_cache::{ConnId, PreparedLookup, PreparedStatementCache};

#[cfg(feature = "async-row-stream")]
use crate::row_stream::{AsyncRowStream, BoxedCursorRowStream};

/// Connection 扩展 trait
///
/// 为 `Connection` 提供三个扩展方法，所有方法有默认实现（向后兼容）。
/// 适配器层可覆盖以接入真实功能。
pub trait ConnectionExt: Connection {
    /// 连接唯一标识（用于 PreparedStatementCache 分桶）
    ///
    /// 默认返回 `0`，适配器应覆盖此方法返回真实连接 ID。
    #[cfg(feature = "prepared-stmt-cache")]
    fn conn_id(&self) -> ConnId {
        0
    }

    /// PreparedStatement 句柄复用查询
    ///
    /// 默认实现：委托 `query_with_params`（无缓存收益，向后兼容）。
    /// 适配器覆盖此方法以接入 `PreparedStatementCache`。
    #[cfg(feature = "prepared-stmt-cache")]
    fn prepare_cached<'a>(
        &'a mut self,
        cache: &'a PreparedStatementCache,
        sql: &'a str,
        params: &'a [Value],
    ) -> Pin<Box<dyn Future<Output = Result<QueryRows, DbError>> + Send + 'a>> {
        Box::pin(async move {
            match cache.get_or_prepare(self.conn_id(), sql, params).await? {
                PreparedLookup::Hit(rows) => Ok(rows),
                PreparedLookup::Miss => {
                    let rows = self.query_with_params(sql, params).await?;
                    Ok(rows)
                }
            }
        })
    }

    /// 统一流式结果集
    ///
    /// 默认实现：委托 `query_stream_cursor` 包装为 `BoxedCursorRowStream`（降级）。
    /// 适配器覆盖此方法以提供真游标实现。
    #[cfg(feature = "async-row-stream")]
    fn query_stream_unified<'a>(
        &'a mut self,
        sql: &'a str,
        batch_size: usize,
    ) -> Result<Box<dyn AsyncRowStream + 'a>, DbError> {
        if batch_size == 0 {
            return Err(DbError::InvalidInput("batch_size 必须大于 0".to_string()));
        }
        let stream = self.query_stream_cursor(sql, batch_size);
        Ok(Box::new(BoxedCursorRowStream::new(stream)))
    }

    /// 批量 DML 并行化
    ///
    /// 默认实现：委托 `execute_batch`（串行，向后兼容）。
    /// `concurrency == 0` 退化为串行。
    /// 适配器覆盖此方法以利用多连接实现真正并行。
    fn execute_batch_parallel<'a>(
        &'a mut self,
        sqls: &'a [String],
        concurrency: usize,
    ) -> Pin<Box<dyn Future<Output = Result<u64, DbError>> + Send + 'a>> {
        let _ = concurrency;
        self.execute_batch(sqls)
    }

    /// 查询结果缓存查询
    ///
    /// 查询流程：QueryResultCache → DB → 写入缓存。
    /// 与 `prepare_cached` 协同：适配器可覆盖此方法，在未命中时委托 `prepare_cached`。
    ///
    /// `depends_on`：该查询依赖的表名（用于主动失效）。
    #[cfg(feature = "query-result-cache")]
    fn query_with_result_cache<'a>(
        &'a mut self,
        cache: &'a crate::query_result_cache::QueryResultCache,
        sql: &'a str,
        params: &'a [Value],
        depends_on: std::collections::HashSet<String>,
    ) -> Pin<Box<dyn Future<Output = Result<QueryRows, DbError>> + Send + 'a>> {
        Box::pin(async move {
            let key = crate::query_result_cache::CacheKey::new(sql, params, None);
            if let Some(rows) = cache.get(&key) {
                return Ok(rows);
            }
            let rows = self.query_with_params(sql, params).await?;
            cache.put(key, rows.clone(), depends_on);
            Ok(rows)
        })
    }
}

// 为所有 `Connection` 自动实现 `ConnectionExt`（blanket impl）
impl<C: Connection + ?Sized> ConnectionExt for C {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct MockConnection {
        connected: bool,
    }

    impl MockConnection {
        fn new() -> Self {
            Self { connected: true }
        }
    }

    impl Connection for MockConnection {
        fn execute<'a>(
            &'a mut self,
            sql: &'a str,
        ) -> Pin<Box<dyn Future<Output = Result<u64, DbError>> + Send + 'a>> {
            let _ = sql;
            Box::pin(async { Ok(1) })
        }

        fn query<'a>(
            &'a mut self,
            sql: &'a str,
        ) -> Pin<Box<dyn Future<Output = Result<QueryRows, DbError>> + Send + 'a>> {
            let _ = sql;
            Box::pin(async { Ok(vec![HashMap::new()]) })
        }

        fn begin_transaction<'a>(
            &'a mut self,
        ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
            Box::pin(async { Ok(()) })
        }

        fn commit<'a>(
            &'a mut self,
        ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
            Box::pin(async { Ok(()) })
        }

        fn rollback<'a>(
            &'a mut self,
        ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
            Box::pin(async { Ok(()) })
        }

        fn is_connected(&self) -> bool {
            self.connected
        }

        fn ping<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
            Box::pin(async { true })
        }

        fn close<'a>(
            &'a mut self,
        ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
            self.connected = false;
            Box::pin(async { Ok(()) })
        }

        fn query_with_params<'a>(
            &'a mut self,
            sql: &'a str,
            params: &'a [Value],
        ) -> Pin<Box<dyn Future<Output = Result<QueryRows, DbError>> + Send + 'a>> {
            let _ = (sql, params);
            Box::pin(async { Ok(vec![HashMap::new()]) })
        }
    }

    #[tokio::test]
    async fn test_prepare_cached_default() {
        #[cfg(feature = "prepared-stmt-cache")]
        {
            let mut conn = MockConnection::new();
            let cache = PreparedStatementCache::new(256);
            let rows = conn
                .prepare_cached(&cache, "SELECT 1", &[Value::I64(1)])
                .await
                .unwrap();
            assert_eq!(rows.len(), 1);
        }
    }

    #[tokio::test]
    async fn test_execute_batch_parallel_default() {
        let mut conn = MockConnection::new();
        let sqls = vec!["INSERT INTO t VALUES (1)".to_string()];
        let count = conn.execute_batch_parallel(&sqls, 0).await.unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn test_execute_batch_parallel_concurrency() {
        let mut conn = MockConnection::new();
        let sqls = vec![
            "INSERT INTO t VALUES (1)".to_string(),
            "INSERT INTO t VALUES (2)".to_string(),
        ];
        let count = conn.execute_batch_parallel(&sqls, 2).await.unwrap();
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn test_execute_batch_parallel_empty() {
        let mut conn = MockConnection::new();
        let sqls: Vec<String> = vec![];
        let count = conn.execute_batch_parallel(&sqls, 4).await.unwrap();
        assert_eq!(count, 0);
    }

    #[cfg(feature = "async-row-stream")]
    #[tokio::test]
    async fn test_query_stream_unified_default() {
        use crate::row_stream::AsyncRowStream;
        let mut conn = MockConnection::new();
        let mut stream = conn.query_stream_unified("SELECT 1", 100).unwrap();
        let row = stream.next_row().await;
        assert!(row.is_some());
    }

    #[cfg(feature = "async-row-stream")]
    #[test]
    fn test_query_stream_unified_zero_batch() {
        let mut conn = MockConnection::new();
        let result = conn.query_stream_unified("SELECT 1", 0);
        assert!(result.is_err());
    }

    #[cfg(feature = "query-result-cache")]
    #[tokio::test]
    async fn test_query_with_result_cache_miss_then_hit() {
        use crate::query_result_cache::QueryResultCache;
        use std::collections::HashSet;

        let mut conn = MockConnection::new();
        let cache = QueryResultCache::with_default();
        let deps = HashSet::new();

        let rows1 = conn
            .query_with_result_cache(&cache, "SELECT 1", &[], deps.clone())
            .await
            .unwrap();
        assert_eq!(rows1.len(), 1);

        let rows2 = conn
            .query_with_result_cache(&cache, "SELECT 1", &[], deps)
            .await
            .unwrap();
        assert_eq!(rows2.len(), 1);

        let stats = cache.stats();
        assert!(stats.hits >= 1);
        assert!(stats.misses >= 1);
    }

    #[cfg(feature = "query-result-cache")]
    #[tokio::test]
    async fn test_query_with_result_cache_invalidate() {
        use crate::query_result_cache::QueryResultCache;
        use std::collections::HashSet;

        let mut conn = MockConnection::new();
        let cache = QueryResultCache::with_default();
        let mut deps = HashSet::new();
        deps.insert("users".to_string());

        let _ = conn
            .query_with_result_cache(&cache, "SELECT * FROM users", &[], deps)
            .await
            .unwrap();

        let key = crate::query_result_cache::CacheKey::new("SELECT * FROM users", &[], None);
        assert!(cache.get(&key).is_some());

        let count = cache.invalidate_table("users");
        assert_eq!(count, 1);
        assert!(cache.get(&key).is_none());
    }

    #[cfg(feature = "query-result-cache")]
    #[tokio::test]
    async fn test_query_with_result_cache_different_params() {
        use crate::query_result_cache::QueryResultCache;
        use std::collections::HashSet;

        let mut conn = MockConnection::new();
        let cache = QueryResultCache::with_default();
        let deps = HashSet::new();

        let _ = conn
            .query_with_result_cache(
                &cache,
                "SELECT * WHERE id = ?",
                &[Value::I64(1)],
                deps.clone(),
            )
            .await
            .unwrap();
        let _ = conn
            .query_with_result_cache(&cache, "SELECT * WHERE id = ?", &[Value::I64(2)], deps)
            .await
            .unwrap();

        let stats = cache.stats();
        assert!(stats.misses >= 2);
    }

    #[cfg(feature = "query-result-cache")]
    #[tokio::test]
    async fn test_query_with_result_cache_cache_key_tenant() {
        use crate::query_result_cache::{CacheKey, QueryResultCache};
        use std::collections::HashSet;

        let cache = QueryResultCache::with_default();
        let k1 = CacheKey::new("SELECT 1", &[], Some(1));
        let k2 = CacheKey::new("SELECT 1", &[], Some(2));
        let rows = vec![HashMap::new()];
        cache.put(k1.clone(), rows.clone(), HashSet::new());
        cache.put(k2.clone(), rows, HashSet::new());
        assert!(cache.get(&k1).is_some());
        assert!(cache.get(&k2).is_some());
        let stats = cache.stats();
        assert_eq!(stats.hits, 2);
    }

    #[cfg(feature = "query-result-cache")]
    #[tokio::test]
    async fn test_query_with_result_cache_stats() {
        use crate::query_result_cache::QueryResultCache;
        use std::collections::HashSet;

        let mut conn = MockConnection::new();
        let cache = QueryResultCache::with_default();
        let deps = HashSet::new();

        for _ in 0..3 {
            let _ = conn
                .query_with_result_cache(&cache, "SELECT 1", &[], deps.clone())
                .await
                .unwrap();
        }
        let stats = cache.stats();
        assert!(stats.hits >= 2);
        assert!(stats.misses >= 1);
        assert!(stats.hit_rate() > 0.0);
    }

    #[cfg(all(feature = "query-result-cache", feature = "prepared-stmt-cache"))]
    #[tokio::test]
    async fn test_query_with_result_cache_and_prepared_cache_coexist() {
        use crate::prepared_cache::PreparedStatementCache;
        use crate::query_result_cache::QueryResultCache;
        use std::collections::HashSet;

        let mut conn = MockConnection::new();
        let result_cache = QueryResultCache::with_default();
        let stmt_cache = PreparedStatementCache::new(256);
        let deps = HashSet::new();

        let rows1 = conn
            .query_with_result_cache(&result_cache, "SELECT 1", &[], deps.clone())
            .await
            .unwrap();
        assert_eq!(rows1.len(), 1);

        let rows2 = conn
            .prepare_cached(&stmt_cache, "SELECT 1", &[Value::I64(1)])
            .await
            .unwrap();
        assert_eq!(rows2.len(), 1);

        let stats = result_cache.stats();
        assert!(stats.total_queries() >= 1);
    }
}
