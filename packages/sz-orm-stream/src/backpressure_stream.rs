//! v6.5.0 BackpressureRowStream 装饰器
//!
//! 为任意 `AsyncRowStream` 添加背压控制。
//! `next_row` 内部 `allow_push().await` 暂停生产者（背压满时），
//! 消费者处理完行后调用 `ack()` 减少积压量。
//!
//! 特性：
//! - 阈值内不暂停（`pending < threshold`）
//! - 阈值处暂停（`pending >= threshold`，等待 `ack`）
//! - 消费者慢时生产者暂停（内存不堆积）
//! - `drop` 释放内部流

use std::future::Future;
use std::pin::Pin;

use sz_orm_core::row_stream::{AsyncRowStream, RowResult};
use sz_orm_core::DbError;

use crate::backpressure::AsyncBackpressureController;

/// 背压行流装饰器
///
/// 包装任意 `AsyncRowStream`，添加背压控制。
/// `next_row` 拉取行前检查背压，满时暂停；`ack` 确认处理完一行。
pub struct BackpressureRowStream<S: AsyncRowStream> {
    inner: S,
    backpressure: AsyncBackpressureController,
}

impl<S: AsyncRowStream> BackpressureRowStream<S> {
    /// 创建背压流
    ///
    /// `threshold` 为背压阈值（积压量上限），`threshold == 0` 拒绝（始终暂停）。
    #[must_use]
    pub fn new(inner: S, threshold: usize) -> Self {
        Self {
            inner,
            backpressure: AsyncBackpressureController::new(threshold),
        }
    }

    /// 确认处理完一行（减少积压量）
    ///
    /// 消费者处理完 `next_row` 返回的行后调用此方法，
    /// 减少积压量，唤醒可能等待的生产者。
    pub fn ack(&self) {
        self.backpressure.pop();
    }

    /// 当前积压量
    #[must_use]
    pub fn pending(&self) -> usize {
        self.backpressure.pending()
    }

    /// 背压阈值
    #[must_use]
    pub fn threshold(&self) -> usize {
        self.backpressure.threshold()
    }

    /// 是否超过阈值
    #[must_use]
    pub fn is_over_threshold(&self) -> bool {
        self.backpressure.is_over_threshold()
    }
}

impl<S: AsyncRowStream> AsyncRowStream for BackpressureRowStream<S> {
    fn next_row<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Option<Result<RowResult, DbError>>> + Send + 'a>> {
        if self.backpressure.threshold() == 0 {
            return Box::pin(async { None });
        }
        Box::pin(async move {
            if !self.backpressure.allow_push().await {
                return None;
            }
            match self.inner.next_row().await {
                Some(Ok(row)) => {
                    self.backpressure.push();
                    Some(Ok(row))
                }
                Some(Err(e)) => Some(Err(e)),
                None => None,
            }
        })
    }

    fn close<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        self.inner.close()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream;
    use sz_orm_core::row_stream::CursorRowStream;
    use sz_orm_core::QueryStreamItem;

    fn make_row(id: i64) -> RowResult {
        let mut row = std::collections::HashMap::new();
        row.insert("id".to_string(), sz_orm_core::Value::I64(id));
        row
    }

    fn make_stream(
        rows: Vec<QueryStreamItem>,
    ) -> CursorRowStream<impl futures::Stream<Item = QueryStreamItem> + Send + Unpin> {
        CursorRowStream::new(stream::iter(rows))
    }

    #[tokio::test]
    async fn test_within_threshold_no_pause() {
        let rows = vec![Ok(make_row(1)), Ok(make_row(2)), Ok(make_row(3))];
        let mut bp_stream = BackpressureRowStream::new(make_stream(rows), 10);

        let row = bp_stream.next_row().await.unwrap().unwrap();
        assert_eq!(row.get("id"), Some(&sz_orm_core::Value::I64(1)));
        assert_eq!(bp_stream.pending(), 1);

        bp_stream.ack();
        assert_eq!(bp_stream.pending(), 0);
    }

    #[tokio::test]
    async fn test_threshold_pause() {
        let rows = vec![Ok(make_row(1)), Ok(make_row(2))];
        let mut bp_stream = BackpressureRowStream::new(make_stream(rows), 1);

        let _row = bp_stream.next_row().await;
        assert_eq!(bp_stream.pending(), 1);
        assert!(bp_stream.is_over_threshold());

        bp_stream.ack();
        assert_eq!(bp_stream.pending(), 0);

        let _row = bp_stream.next_row().await;
        assert_eq!(bp_stream.pending(), 1);
    }

    #[tokio::test]
    async fn test_slow_consumer_backpressure() {
        let rows = vec![Ok(make_row(1)), Ok(make_row(2)), Ok(make_row(3))];
        let mut bp_stream = BackpressureRowStream::new(make_stream(rows), 2);

        let _r1 = bp_stream.next_row().await;
        assert_eq!(bp_stream.pending(), 1);

        let _r2 = bp_stream.next_row().await;
        assert_eq!(bp_stream.pending(), 2);
        assert!(bp_stream.is_over_threshold());

        bp_stream.ack();
        bp_stream.ack();
        assert_eq!(bp_stream.pending(), 0);

        let _r3 = bp_stream.next_row().await;
        assert_eq!(bp_stream.pending(), 1);
    }

    #[tokio::test]
    async fn test_drop_releases() {
        let rows = vec![Ok(make_row(1))];
        let bp_stream = BackpressureRowStream::new(make_stream(rows), 10);
        drop(bp_stream);
    }

    #[tokio::test]
    async fn test_threshold_zero_rejects() {
        let rows = vec![Ok(make_row(1))];
        let mut bp_stream = BackpressureRowStream::new(make_stream(rows), 0);

        let result = bp_stream.next_row().await;
        assert!(result.is_none(), "threshold=0 应始终暂停");
    }

    #[tokio::test]
    async fn test_stream_end_none() {
        let rows = vec![Ok(make_row(1))];
        let mut bp_stream = BackpressureRowStream::new(make_stream(rows), 10);

        let _row = bp_stream.next_row().await;
        bp_stream.ack();

        assert!(bp_stream.next_row().await.is_none());
    }
}
