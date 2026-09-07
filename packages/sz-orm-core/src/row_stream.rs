//! v6.5.0 异步流式结果集 trait
//!
//! 统一异步行流 trait `AsyncRowStream`，消费方针对此 trait 编写代码，
//! 可在 MySQL/PG/SQLite/Oracle 间切换。提供 `CursorRowStream` 降级实现，
//! 包装既有 `query_stream_cursor` 返回的 `futures::Stream`。
//!
//! 特性：
//! - `next_row` 返回 `Some(Ok(row))`：有行可消费
//! - `next_row` 返回 `Some(Err(e))`：fetch 错误
//! - `next_row` 返回 `None`：流结束
//! - `close` 后再调用 `next_row` 返回 `None`
//! - `CursorRowStream` 降级：包装 `query_stream_cursor` 的全量收集流

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use futures::stream::{Stream, StreamExt};

use crate::error::DbError;
use crate::pool::QueryStreamItem;
use crate::value::Value;

/// 单行结果（列名 → 值）
pub type RowResult = HashMap<String, Value>;

/// 异步行流 trait
///
/// 统一 MySQL/PG/SQLite/Oracle 流式查询接口。
/// 消费方针对此 trait 编写代码，可在后端间切换。
///
/// # 生命周期
///
/// `next_row` / `close` 借用 `&mut self`，返回 `Pin<Box<dyn Future + Send + 'a>>`。
/// 签名遵循 `Connection` trait 的 `Pin<Box<dyn Future>>` 模式（HRTB 约束）。
pub trait AsyncRowStream: Send {
    /// 拉取下一行
    ///
    /// - `Some(Ok(row))`：有行可消费
    /// - `Some(Err(e))`：fetch 错误
    /// - `None`：流结束
    fn next_row<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Option<Result<RowResult, DbError>>> + Send + 'a>>;

    /// 关闭流，释放游标资源
    ///
    /// 默认实现：no-op（drop 时释放）
    fn close<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }
}

/// 降级适配器：包装既有 `query_stream_cursor` 返回的 `futures::Stream`
///
/// 用于不支持真游标的后端降级（spec.md 5.3.1 业务规则 5）。
/// 逐行转发底层 `Stream` 的 `next()` 结果。
pub struct CursorRowStream<S: Stream<Item = QueryStreamItem> + Send + Unpin> {
    inner: S,
    closed: bool,
}

impl<S: Stream<Item = QueryStreamItem> + Send + Unpin> CursorRowStream<S> {
    /// 创建降级流
    #[must_use]
    pub fn new(stream: S) -> Self {
        Self {
            inner: stream,
            closed: false,
        }
    }
}

impl<S: Stream<Item = QueryStreamItem> + Send + Unpin> AsyncRowStream for CursorRowStream<S> {
    fn next_row<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Option<Result<RowResult, DbError>>> + Send + 'a>> {
        if self.closed {
            return Box::pin(async { None });
        }
        Box::pin(async move { self.inner.next().await })
    }

    fn close<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move {
            self.closed = true;
            Ok(())
        })
    }
}

/// Boxed stream 的降级适配器（用于 `Pin<Box<dyn Stream + 'a>>` 场景）
pub struct BoxedCursorRowStream<'a> {
    inner: Pin<Box<dyn Stream<Item = QueryStreamItem> + Send + 'a>>,
    closed: bool,
}

impl<'a> BoxedCursorRowStream<'a> {
    /// 创建降级流（从 boxed stream）
    #[must_use]
    pub fn new(stream: Pin<Box<dyn Stream<Item = QueryStreamItem> + Send + 'a>>) -> Self {
        Self {
            inner: stream,
            closed: false,
        }
    }
}

impl<'a> AsyncRowStream for BoxedCursorRowStream<'a> {
    fn next_row<'b>(
        &'b mut self,
    ) -> Pin<Box<dyn Future<Output = Option<Result<RowResult, DbError>>> + Send + 'b>> {
        if self.closed {
            return Box::pin(async { None });
        }
        Box::pin(async move { self.inner.next().await })
    }

    fn close<'b>(&'b mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'b>> {
        Box::pin(async move {
            self.closed = true;
            Ok(())
        })
    }
}

impl<'a> AsyncRowStream for Box<dyn AsyncRowStream + 'a> {
    fn next_row<'b>(
        &'b mut self,
    ) -> Pin<Box<dyn Future<Output = Option<Result<RowResult, DbError>>> + Send + 'b>> {
        (**self).next_row()
    }

    fn close<'b>(&'b mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'b>> {
        (**self).close()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream;

    fn make_row(id: i64) -> RowResult {
        let mut row = HashMap::new();
        row.insert("id".to_string(), Value::I64(id));
        row
    }

    #[tokio::test]
    async fn test_next_row_yields_rows() {
        let rows: Vec<QueryStreamItem> = vec![Ok(make_row(1)), Ok(make_row(2)), Ok(make_row(3))];
        let stream = stream::iter(rows);
        let mut s = CursorRowStream::new(stream);

        let row = s.next_row().await.unwrap().unwrap();
        assert_eq!(row.get("id"), Some(&Value::I64(1)));
        let row = s.next_row().await.unwrap().unwrap();
        assert_eq!(row.get("id"), Some(&Value::I64(2)));
        let row = s.next_row().await.unwrap().unwrap();
        assert_eq!(row.get("id"), Some(&Value::I64(3)));
    }

    #[tokio::test]
    async fn test_stream_end_none() {
        let rows: Vec<QueryStreamItem> = vec![Ok(make_row(1))];
        let stream = stream::iter(rows);
        let mut s = CursorRowStream::new(stream);

        s.next_row().await;
        assert!(s.next_row().await.is_none());
    }

    #[tokio::test]
    async fn test_error_propagation() {
        let rows: Vec<QueryStreamItem> = vec![Err(DbError::QueryError("test error".into()))];
        let stream = stream::iter(rows);
        let mut s = CursorRowStream::new(stream);

        let result = s.next_row().await;
        assert!(matches!(result, Some(Err(_))));
    }

    #[tokio::test]
    async fn test_close_then_none() {
        let rows: Vec<QueryStreamItem> = vec![Ok(make_row(1)), Ok(make_row(2))];
        let stream = stream::iter(rows);
        let mut s = CursorRowStream::new(stream);

        s.close().await.unwrap();
        assert!(s.next_row().await.is_none());
    }

    #[tokio::test]
    async fn test_cursor_row_stream_degradation() {
        let rows: Vec<QueryStreamItem> = vec![Ok(make_row(1))];
        let stream = stream::iter(rows);
        let mut s: BoxedCursorRowStream<'_> = BoxedCursorRowStream::new(Box::pin(stream));

        let row = s.next_row().await.unwrap().unwrap();
        assert_eq!(row.get("id"), Some(&Value::I64(1)));
        assert!(s.next_row().await.is_none());
    }

    #[tokio::test]
    async fn test_drop_releases() {
        let rows: Vec<QueryStreamItem> = vec![Ok(make_row(1))];
        let stream = stream::iter(rows);
        let s = CursorRowStream::new(stream);
        drop(s);
    }
}
