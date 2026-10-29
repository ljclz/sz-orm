//! # 查询指标装饰器连接（2026-10-28 架构评测 P1-3 可观测性接线）
//!
//! 包装 `Box<dyn Connection>`，对全部数据路径方法（execute/query 及 *_with_params/
//! query_values* 变体、事务 begin/commit/rollback）做耗时测量，经
//! [`sz_orm_core::metrics_hook::record_query`] 上报（未安装钩子时空操作）。
//!
//! 三后端（SQLite/MySQL/PostgreSQL）的 [`sz_orm_core::ConnectionFactory`] `create()`
//! 统一返回包装连接，因此指标覆盖所有经工厂创建的连接。流式查询
//! （`query_stream*`）生命周期由消费方决定，不做整流计时（如实委托）。

use std::future::Future;
use std::pin::Pin;
use std::time::Instant;

use sz_orm_core::DbError;
use sz_orm_core::{Connection, QueryRows, QueryStreamItem, QueryValues, Value};

pub(crate) struct MetricsConn {
    inner: Box<dyn Connection>,
}

impl MetricsConn {
    pub(crate) fn new(inner: Box<dyn Connection>) -> Self {
        Self { inner }
    }

    fn record<T>(t0: Instant, r: &Result<T, DbError>) {
        sz_orm_core::metrics_hook::record_query(t0.elapsed().as_micros() as u64, r.is_err());
    }
}

impl Connection for MetricsConn {
    fn execute<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<u64, DbError>> + Send + 'a>> {
        let t0 = Instant::now();
        Box::pin(async move {
            let r = self.inner.execute(sql).await;
            Self::record(t0, &r);
            r
        })
    }

    fn query<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<QueryRows, DbError>> + Send + 'a>> {
        let t0 = Instant::now();
        Box::pin(async move {
            let r = self.inner.query(sql).await;
            Self::record(t0, &r);
            r
        })
    }

    fn begin_transaction<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        let t0 = Instant::now();
        Box::pin(async move {
            let r = self.inner.begin_transaction().await;
            Self::record(t0, &r);
            r
        })
    }

    fn commit<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        let t0 = Instant::now();
        Box::pin(async move {
            let r = self.inner.commit().await;
            Self::record(t0, &r);
            r
        })
    }

    fn rollback<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        let t0 = Instant::now();
        Box::pin(async move {
            let r = self.inner.rollback().await;
            Self::record(t0, &r);
            r
        })
    }

    fn is_connected(&self) -> bool {
        self.inner.is_connected()
    }

    fn ping<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move { self.inner.ping().await })
    }

    fn close<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move { self.inner.close().await })
    }

    fn db_type(&self) -> Option<sz_orm_core::DbType> {
        self.inner.db_type()
    }

    fn in_transaction(&self) -> bool {
        self.inner.in_transaction()
    }

    fn session_reset_sql(&self) -> Option<&'static str> {
        self.inner.session_reset_sql()
    }

    fn execute_with_params<'a>(
        &'a mut self,
        sql: &'a str,
        params: &'a [Value],
    ) -> Pin<Box<dyn Future<Output = Result<u64, DbError>> + Send + 'a>> {
        let t0 = Instant::now();
        Box::pin(async move {
            let r = self.inner.execute_with_params(sql, params).await;
            Self::record(t0, &r);
            r
        })
    }

    fn query_with_params<'a>(
        &'a mut self,
        sql: &'a str,
        params: &'a [Value],
    ) -> Pin<Box<dyn Future<Output = Result<QueryRows, DbError>> + Send + 'a>> {
        let t0 = Instant::now();
        Box::pin(async move {
            let r = self.inner.query_with_params(sql, params).await;
            Self::record(t0, &r);
            r
        })
    }

    fn query_values<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<QueryValues, DbError>> + Send + 'a>> {
        let t0 = Instant::now();
        Box::pin(async move {
            let r = self.inner.query_values(sql).await;
            Self::record(t0, &r);
            r
        })
    }

    fn query_values_with_params<'a>(
        &'a mut self,
        sql: &'a str,
        params: &'a [Value],
    ) -> Pin<Box<dyn Future<Output = Result<QueryValues, DbError>> + Send + 'a>> {
        let t0 = Instant::now();
        Box::pin(async move {
            let r = self.inner.query_values_with_params(sql, params).await;
            Self::record(t0, &r);
            r
        })
    }

    fn query_stream<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn futures::Stream<Item = QueryStreamItem> + Send + 'a>> {
        self.inner.query_stream(sql)
    }

    fn query_stream_cursor<'a>(
        &'a mut self,
        sql: &'a str,
        batch_size: usize,
    ) -> Pin<Box<dyn futures::Stream<Item = QueryStreamItem> + Send + 'a>> {
        self.inner.query_stream_cursor(sql, batch_size)
    }
}
