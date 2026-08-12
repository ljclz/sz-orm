//! StreamResultSet — 异步流式结果集
//!
//! 实现异步 Stream trait 逐批 yield 结果，避免一次性加载全量。
//! 支持三种分页策略（Keyset/LimitOffset/ServerCursor）+ 背压控制。

use std::pin::Pin;
use std::sync::Arc;

use futures::future::BoxFuture;
use futures::stream::{self, Stream};
use serde_json::Value;

use sz_orm_core::DbType;

use crate::backpressure::AsyncBackpressureController;
use crate::config::{PaginationStrategy, StreamResultSetConfig};
use crate::keyset::KeysetPaginator;

/// 查询执行器闭包类型
pub type QueryExecutor =
    Box<dyn Fn(&str, &[Value]) -> BoxFuture<'static, Result<Vec<Value>, String>> + Send + Sync>;

/// 流式结果集
pub struct StreamResultSet {
    sql: String,
    params: Vec<Value>,
    config: StreamResultSetConfig,
    backpressure: AsyncBackpressureController,
}

impl StreamResultSet {
    pub fn new(sql: impl Into<String>, config: StreamResultSetConfig) -> Self {
        let backpressure = AsyncBackpressureController::new(config.backpressure_threshold);
        Self {
            sql: sql.into(),
            params: Vec::new(),
            config,
            backpressure,
        }
    }

    pub fn with_params(mut self, params: Vec<Value>) -> Self {
        self.params = params;
        self
    }

    pub fn backpressure(&self) -> &AsyncBackpressureController {
        &self.backpressure
    }

    /// 返回异步 Stream
    ///
    /// `executor` 为查询执行闭包：`async fn(sql: &str, params: &[Value]) -> Result<Vec<Value>, String>`
    pub fn stream_query<F>(
        self,
        executor: F,
    ) -> Pin<Box<dyn Stream<Item = Result<Vec<Value>, String>> + Send>>
    where
        F: Fn(&str, &[Value]) -> BoxFuture<'static, Result<Vec<Value>, String>>
            + Send
            + Sync
            + 'static,
    {
        let executor = Box::new(executor);
        Box::pin(self.into_stream(executor))
    }

    fn into_stream(
        self,
        executor: QueryExecutor,
    ) -> impl Stream<Item = Result<Vec<Value>, String>> + Send {
        let Self {
            sql,
            params,
            config,
            backpressure,
        } = self;
        let executor = Arc::new(executor);
        stream::unfold(
            StreamState {
                sql,
                params,
                config,
                backpressure,
                keyset: None,
                offset: 0,
                done: false,
            },
            move |mut state| {
                let executor = Arc::clone(&executor);
                async move {
                    if state.done {
                        return None;
                    }
                    if !state.backpressure.try_allow_push() {
                        return Some((Err("backpressure limit reached".into()), state));
                    }
                    let (page_sql, page_params) = match state.build_next_page() {
                        Ok(page) => page,
                        Err(e) => return Some((Err(e), state)),
                    };
                    let batch = match executor(&page_sql, &page_params).await {
                        Ok(rows) => rows,
                        Err(e) => return Some((Err(e), state)),
                    };
                    if batch.is_empty() {
                        return None;
                    }
                    state.backpressure.push();
                    let batch_len = batch.len();
                    state.advance(batch_len, batch.last());
                    Some((Ok(batch), state))
                }
            },
        )
    }
}

/// 流式状态机
struct StreamState {
    sql: String,
    params: Vec<Value>,
    config: StreamResultSetConfig,
    backpressure: AsyncBackpressureController,
    keyset: Option<KeysetPaginator>,
    offset: u64,
    done: bool,
}

impl StreamState {
    fn build_next_page(&mut self) -> Result<(String, Vec<Value>), String> {
        if self.config.validate().is_err() {
            return Err("invalid config".into());
        }
        match self.config.pagination_strategy {
            PaginationStrategy::Keyset => self.build_keyset_page(),
            PaginationStrategy::LimitOffset => self.build_limit_offset_page(),
            PaginationStrategy::ServerCursor => self.build_server_cursor_page(),
        }
    }

    fn build_keyset_page(&mut self) -> Result<(String, Vec<Value>), String> {
        if self.keyset.is_none() {
            let col = self
                .config
                .keyset_column
                .as_ref()
                .ok_or("keyset requires keyset_column")?;
            let mut paginator = KeysetPaginator::new(col.clone(), self.config.batch_size);
            paginator = paginator.with_order_direction(self.config.order_direction);
            self.keyset = Some(paginator);
        }
        let paginator = self.keyset.as_ref().unwrap();
        if !paginator.has_more() {
            return Err("no more data".into());
        }
        let sql = paginator.build_next_page_sql(&self.sql);
        Ok((sql, self.params.clone()))
    }

    fn build_limit_offset_page(&self) -> Result<(String, Vec<Value>), String> {
        let sql = match self.config.db_type {
            DbType::Oracle | DbType::Dameng => {
                let end = self.offset + self.config.batch_size as u64;
                format!("SELECT * FROM ({}) sub WHERE ROWNUM <= {}", self.sql, end)
            }
            _ => format!(
                "{} LIMIT {} OFFSET {}",
                self.sql, self.config.batch_size, self.offset
            ),
        };
        Ok((sql, self.params.clone()))
    }

    fn build_server_cursor_page(&self) -> Result<(String, Vec<Value>), String> {
        match self.config.db_type {
            DbType::PostgreSQL | DbType::GaussDB | DbType::Kingbase | DbType::PolarDB => {
                let sql = format!(
                    "{} LIMIT {} OFFSET {}",
                    self.sql, self.config.batch_size, self.offset
                );
                Ok((sql, self.params.clone()))
            }
            _ => {
                let sql = format!(
                    "{} LIMIT {} OFFSET {}",
                    self.sql, self.config.batch_size, self.offset
                );
                Ok((sql, self.params.clone()))
            }
        }
    }

    fn advance(&mut self, batch_len: usize, last_row: Option<&Value>) {
        match self.config.pagination_strategy {
            PaginationStrategy::Keyset => {
                if let Some(paginator) = &mut self.keyset {
                    if let Some(row) = last_row {
                        paginator.update_last_key(row);
                    }
                    paginator.mark_batch_result(batch_len);
                }
            }
            PaginationStrategy::LimitOffset | PaginationStrategy::ServerCursor => {
                self.offset += batch_len as u64;
                if batch_len < self.config.batch_size {
                    self.done = true;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::OrderDirection;
    use futures::StreamExt;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    fn mock_executor(
        rows_per_batch: usize,
        total_rows: usize,
    ) -> impl Fn(&str, &[Value]) -> BoxFuture<'static, Result<Vec<Value>, String>> + Send + Sync
    {
        let remaining = Arc::new(AtomicUsize::new(total_rows));
        move |_sql, _params| {
            let rem = remaining.load(Ordering::Relaxed);
            let take = rows_per_batch.min(rem);
            remaining.fetch_sub(take, Ordering::Relaxed);
            Box::pin(async move {
                let batch: Vec<Value> = (0..take)
                    .map(|i| serde_json::json!({"id": i, "name": format!("row{i}")}))
                    .collect();
                Ok(batch)
            })
        }
    }

    fn empty_executor(
    ) -> impl Fn(&str, &[Value]) -> BoxFuture<'static, Result<Vec<Value>, String>> + Send + Sync
    {
        |_sql, _params| Box::pin(async { Ok(Vec::new()) })
    }

    fn error_executor(
    ) -> impl Fn(&str, &[Value]) -> BoxFuture<'static, Result<Vec<Value>, String>> + Send + Sync
    {
        |_sql, _params| Box::pin(async { Err("db error".to_string()) })
    }

    #[tokio::test]
    async fn stream_limit_offset_basic() {
        let config = StreamResultSetConfig::new(DbType::PostgreSQL).with_batch_size(10);
        let rs = StreamResultSet::new("SELECT * FROM users", config);
        let mut stream = rs.stream_query(mock_executor(10, 25));
        let mut total = 0;
        while let Some(result) = stream.next().await {
            let batch = result.unwrap();
            total += batch.len();
        }
        assert_eq!(total, 25);
    }

    #[tokio::test]
    async fn stream_keyset_basic() {
        let config = StreamResultSetConfig::new(DbType::PostgreSQL)
            .with_batch_size(10)
            .with_pagination_strategy(PaginationStrategy::Keyset)
            .with_keyset_column("id");
        let rs = StreamResultSet::new("SELECT * FROM users", config);
        let mut stream = rs.stream_query(mock_executor(10, 30));
        let mut total = 0;
        while let Some(result) = stream.next().await {
            let batch = result.unwrap();
            total += batch.len();
        }
        assert_eq!(total, 30);
    }

    #[tokio::test]
    async fn stream_empty_result() {
        let config = StreamResultSetConfig::new(DbType::PostgreSQL).with_batch_size(10);
        let rs = StreamResultSet::new("SELECT * FROM users", config);
        let mut stream = rs.stream_query(empty_executor());
        let result = stream.next().await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn stream_error_propagated() {
        let config = StreamResultSetConfig::new(DbType::PostgreSQL).with_batch_size(10);
        let rs = StreamResultSet::new("SELECT * FROM users", config);
        let mut stream = rs.stream_query(error_executor());
        let result = stream.next().await;
        assert!(result.is_some());
        assert!(result.unwrap().is_err());
    }

    #[tokio::test]
    async fn stream_oracle_rownum() {
        let config = StreamResultSetConfig::new(DbType::Oracle).with_batch_size(10);
        let rs = StreamResultSet::new("SELECT * FROM users", config);
        let mut stream = rs.stream_query(mock_executor(10, 20));
        let mut total = 0;
        while let Some(result) = stream.next().await {
            let batch = result.unwrap();
            total += batch.len();
        }
        assert_eq!(total, 20);
    }

    #[tokio::test]
    async fn stream_server_cursor_pg() {
        let config = StreamResultSetConfig::new(DbType::PostgreSQL)
            .with_batch_size(5)
            .with_pagination_strategy(PaginationStrategy::ServerCursor);
        let rs = StreamResultSet::new("SELECT * FROM users", config);
        let mut stream = rs.stream_query(mock_executor(5, 15));
        let mut total = 0;
        while let Some(result) = stream.next().await {
            let batch = result.unwrap();
            total += batch.len();
        }
        assert_eq!(total, 15);
    }

    #[tokio::test]
    async fn stream_desc_order() {
        let config = StreamResultSetConfig::new(DbType::PostgreSQL)
            .with_batch_size(10)
            .with_pagination_strategy(PaginationStrategy::Keyset)
            .with_keyset_column("id")
            .with_order_direction(OrderDirection::Desc);
        let rs = StreamResultSet::new("SELECT * FROM users", config);
        let mut stream = rs.stream_query(mock_executor(10, 20));
        let mut total = 0;
        while let Some(result) = stream.next().await {
            let batch = result.unwrap();
            total += batch.len();
        }
        assert_eq!(total, 20);
    }

    #[tokio::test]
    async fn stream_backpressure() {
        let config = StreamResultSetConfig::new(DbType::PostgreSQL)
            .with_batch_size(10)
            .with_backpressure_threshold(10000);
        let rs = StreamResultSet::new("SELECT * FROM users", config);
        assert!(rs.backpressure().try_allow_push());
    }

    #[tokio::test]
    async fn stream_multiple_batches() {
        let config = StreamResultSetConfig::new(DbType::MySQL).with_batch_size(5);
        let rs = StreamResultSet::new("SELECT * FROM t", config);
        let mut stream = rs.stream_query(mock_executor(5, 12));
        let mut batches = 0;
        while let Some(result) = stream.next().await {
            let batch = result.unwrap();
            batches += 1;
            if batches <= 2 {
                assert_eq!(batch.len(), 5);
            } else {
                assert_eq!(batch.len(), 2);
            }
        }
        assert_eq!(batches, 3);
    }
}
