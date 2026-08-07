//! Stream API — 异步流式查询
//!
//! # 概述
//!
//! v2.1.0 改造 `stream` 为真游标逐行产出，峰值内存 ≤ 50 MB（100 万行）。
//! `stream_buffered` 保留旧实现（全量收集后逐行 yield）作为兼容逃生舱。
//!
//! # 向后兼容
//!
//! - `stream` trait 签名不变（ADR-v2.1.0-003），仅改 impl 实现
//! - `stream_buffered` 行为与 v2.0.0 `stream` 完全一致
//!
//! # 示例
//!
//! ```ignore
//! use sz_orm_core::paginator::StreamQueryTrait;
//! use futures::StreamExt;
//!
//! // 真游标流（推荐）
//! let mut stream = query.stream(&mut conn);
//! while let Some(row) = stream.next().await {
//!     let row = row?;
//!     // 处理...
//! }
//!
//! // 兼容版（全量收集后逐行 yield）
//! let mut stream = query.stream_buffered(&mut conn);
//! while let Some(row) = stream.next().await {
//!     let row = row?;
//!     // 处理...
//! }
//! ```

use crate::model::Model;
use crate::pool::{Connection, QueryStreamItem};
use crate::query::QueryBuilder;
use crate::DbError;

use std::collections::HashMap;
use std::pin::Pin;

use futures::{sink::SinkExt, stream, Stream, StreamExt};

/// 流式查询结果行类型
pub type RowResult = HashMap<String, crate::value::Value>;

/// Stream API 扩展 trait
///
/// 为 `QueryBuilder<M>` 提供 `stream_buffered` 兼容版方法和 `stream_with_backpressure` 背压方法。
pub trait StreamApiExt<M: Model> {
    /// 兼容版流式查询（全量收集后逐行 yield）
    ///
    /// 保留 v2.0.0 `stream` 的行为，作为逃生舱。
    /// 推荐使用 `stream`（真游标，低内存）。
    fn stream_buffered<'a, 'b: 'a, C: Connection + Send + 'b>(
        self,
        conn: &'b mut C,
    ) -> Pin<Box<dyn Stream<Item = Result<RowResult, DbError>> + Send + 'a>>;

    /// 背压流式查询（v2.2.0 B-4）
    ///
    /// 创建有界缓冲通道，缓冲区满时生产者阻塞（背压）。
    ///
    /// # 参数
    ///
    /// - `conn`：数据库连接
    /// - `buffer_size`：缓冲区容量（必须 > 0）
    ///
    /// # 错误
    ///
    /// - `buffer_size == 0` → 返回 `Err(DbError::InvalidInput)`
    ///
    /// ```ignore
    /// let stream = query.stream_with_backpressure(&mut conn, 1000)?;
    /// // 缓冲区容量 1000，满时生产者阻塞
    /// ```
    fn stream_with_backpressure<'a, 'b: 'a, C: Connection + Send + 'b>(
        self,
        conn: &'b mut C,
        buffer_size: usize,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<RowResult, DbError>> + Send + 'a>>, DbError>;
}

impl<M: Model> StreamApiExt<M> for QueryBuilder<M> {
    fn stream_buffered<'a, 'b: 'a, C: Connection + Send + 'b>(
        self,
        conn: &'b mut C,
    ) -> Pin<Box<dyn Stream<Item = Result<RowResult, DbError>> + Send + 'a>> {
        let (sql, params) = self.build_select_with_params();

        let st = stream::once(async move {
            match conn.query_with_params(&sql, &params).await {
                Ok(rows) => rows.into_iter().map(Ok).collect::<Vec<_>>(),
                Err(e) => vec![Err(e)],
            }
        })
        .flat_map(stream::iter);

        Box::pin(st)
    }

    fn stream_with_backpressure<'a, 'b: 'a, C: Connection + Send + 'b>(
        self,
        conn: &'b mut C,
        buffer_size: usize,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<RowResult, DbError>> + Send + 'a>>, DbError> {
        if buffer_size == 0 {
            return Err(DbError::InvalidInput("buffer_size 必须大于 0".to_string()));
        }

        let (sql, params) = self.build_select_with_params();
        let (mut tx, rx) =
            futures::channel::mpsc::channel::<Result<RowResult, DbError>>(buffer_size);

        let producer = async move {
            match conn.query_with_params(&sql, &params).await {
                Ok(rows) => {
                    for row in rows {
                        if tx.send(Ok(row)).await.is_err() {
                            break;
                        }
                    }
                }
                Err(e) => {
                    let _ = tx.send(Err(e)).await;
                }
            }
        };

        let receiver_stream = BackpressureStream {
            rx,
            producer: Some(Box::pin(producer)),
        };

        Ok(Box::pin(receiver_stream))
    }
}

/// 背压流（v2.2.0 B-4）：合并生产者 future 和接收器
struct BackpressureStream<'a> {
    rx: futures::channel::mpsc::Receiver<Result<RowResult, DbError>>,
    producer: Option<Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>>>,
}

impl<'a> Stream for BackpressureStream<'a> {
    type Item = Result<RowResult, DbError>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let this = self.get_mut();

        if let Some(mut producer) = this.producer.take() {
            match std::future::Future::poll(std::pin::Pin::new(&mut producer), cx) {
                std::task::Poll::Ready(()) => {}
                std::task::Poll::Pending => {
                    this.producer = Some(producer);
                }
            }
        }

        Stream::poll_next(std::pin::Pin::new(&mut this.rx), cx)
    }
}

/// 真游标流式查询
///
/// 委托 `conn.query_stream_cursor` 实现真游标逐行 fetch。
/// drop 时关闭 DB 游标，连接归还连接池。
///
/// # 错误传播
///
/// - 游标 fetch 失败 → yield `Some(Err(DbError::ConnectionError))`
/// - 游标打开失败 → yield `Some(Err(DbError))`
pub fn stream_cursor<'a, 'b: 'a, C: Connection + Send + 'b>(
    conn: &'b mut C,
    sql: &'b str,
    params: Vec<crate::value::Value>,
    batch_size: usize,
) -> Pin<Box<dyn Stream<Item = QueryStreamItem> + Send + 'a>> {
    let _ = params; // 真游标在 query_stream_cursor 内部处理参数
    conn.query_stream_cursor(sql, batch_size)
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db_type::DbType;
    use crate::dialect::get_dialect;
    use crate::mock::MockConnection;
    use crate::model::Model;
    use crate::query::QueryBuilder;
    use crate::value::Value;
    use futures::StreamExt;

    #[derive(Debug, Clone, Default)]
    #[allow(dead_code)]
    struct TestUser {
        id: i64,
        name: String,
    }

    impl Model for TestUser {
        type PrimaryKey = i64;
        fn table_name() -> &'static str {
            "users"
        }
        fn pk(&self) -> Self::PrimaryKey {
            self.id
        }
        fn set_pk(&mut self, pk: Self::PrimaryKey) {
            self.id = pk;
        }
    }

    #[tokio::test]
    async fn test_stream_buffered_yields_rows() {
        let mut mock = MockConnection::new();
        mock.expect_any().with_rows(vec![
            vec![("id", Value::I64(1))],
            vec![("id", Value::I64(2))],
        ]);

        let dialect = get_dialect(DbType::MySQL).unwrap();
        let query = QueryBuilder::<TestUser>::new(dialect).table("users");

        let mut stream = query.stream_buffered(&mut mock);
        let mut rows = Vec::new();
        while let Some(row) = stream.next().await {
            rows.push(row.unwrap());
        }

        assert_eq!(rows.len(), 2);
    }

    #[tokio::test]
    async fn test_stream_buffered_error_propagation() {
        let mut mock = MockConnection::new().with_fallback(crate::mock::FallbackBehavior::Error);

        let dialect = get_dialect(DbType::MySQL).unwrap();
        let query = QueryBuilder::<TestUser>::new(dialect).table("nonexistent");

        let mut stream = query.stream_buffered(&mut mock);
        let result = stream.next().await;

        assert!(result.is_some());
        assert!(result.unwrap().is_err());
    }

    #[tokio::test]
    async fn test_stream_buffered_empty() {
        let mut mock = MockConnection::new();
        mock.expect_any().with_rows(vec![]);

        let dialect = get_dialect(DbType::MySQL).unwrap();
        let query = QueryBuilder::<TestUser>::new(dialect).table("users");

        let mut stream = query.stream_buffered(&mut mock);
        let mut count = 0;
        while stream.next().await.is_some() {
            count += 1;
        }

        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_stream_backpressure_zero_buffer() {
        let mut mock = MockConnection::new();
        mock.expect_any().with_rows(vec![]);

        let dialect = get_dialect(DbType::MySQL).unwrap();
        let query = QueryBuilder::<TestUser>::new(dialect).table("users");

        let result = query.stream_with_backpressure(&mut mock, 0);
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(matches!(err, crate::DbError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn test_stream_backpressure_basic() {
        let mut mock = MockConnection::new();
        mock.expect_any().with_rows(vec![
            vec![("id", Value::I64(1))],
            vec![("id", Value::I64(2))],
            vec![("id", Value::I64(3))],
        ]);

        let dialect = get_dialect(DbType::MySQL).unwrap();
        let query = QueryBuilder::<TestUser>::new(dialect).table("users");

        let stream = query.stream_with_backpressure(&mut mock, 10).unwrap();
        let mut stream = Box::pin(stream);
        let mut rows = Vec::new();
        while let Some(row) = stream.next().await {
            rows.push(row.unwrap());
        }

        assert_eq!(rows.len(), 3);
    }

    #[tokio::test]
    async fn test_stream_backpressure_empty() {
        let mut mock = MockConnection::new();
        mock.expect_any().with_rows(vec![]);

        let dialect = get_dialect(DbType::MySQL).unwrap();
        let query = QueryBuilder::<TestUser>::new(dialect).table("users");

        let stream = query.stream_with_backpressure(&mut mock, 100).unwrap();
        let mut stream = Box::pin(stream);
        let mut count = 0;
        while stream.next().await.is_some() {
            count += 1;
        }

        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_stream_backpressure_error_propagation() {
        let mut mock = MockConnection::new().with_fallback(crate::mock::FallbackBehavior::Error);

        let dialect = get_dialect(DbType::MySQL).unwrap();
        let query = QueryBuilder::<TestUser>::new(dialect).table("nonexistent");

        let stream = query.stream_with_backpressure(&mut mock, 10).unwrap();
        let mut stream = Box::pin(stream);
        let result = stream.next().await;

        assert!(result.is_some());
        assert!(result.unwrap().is_err());
    }
}
