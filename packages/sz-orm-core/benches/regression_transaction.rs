//! 路径 4：事务 benchmark（3 基准点）
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::sync::Arc;
use sz_orm_core::{Connection, ConnectionFactory, Pool, PoolConfig};

mod helpers {
    use super::*;
    pub struct BenchConnection {
        pub connected: bool,
    }

    #[async_trait::async_trait]
    impl Connection for BenchConnection {
        fn execute<'a>(
            &'a mut self,
            sql: &'a str,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<u64, sz_orm_core::DbError>> + Send + 'a>>
        {
            let _ = sql;
            Box::pin(async move { Ok(1) })
        }
        fn query<'a>(
            &'a mut self,
            sql: &'a str,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<sz_orm_core::QueryRows, sz_orm_core::DbError>> + Send + 'a>>
        {
            let _ = sql;
            Box::pin(async { Ok(sz_orm_core::QueryRows::new()) })
        }
        fn begin_transaction<'a>(
            &'a mut self,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>> {
            Box::pin(async { Ok(()) })
        }
        fn commit<'a>(
            &'a mut self,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>> {
            Box::pin(async { Ok(()) })
        }
        fn rollback<'a>(
            &'a mut self,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>> {
            Box::pin(async { Ok(()) })
        }
        fn is_connected(&self) -> bool {
            self.connected
        }
        fn ping<'a>(&'a mut self) -> std::pin::Pin<Box<dyn std::future::Future<Output = bool> + Send + 'a>> {
            Box::pin(async { true })
        }
        fn close<'a>(
            &'a mut self,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>> {
            Box::pin(async move {
                self.connected = false;
                Ok(())
            })
        }
    }

    pub struct BenchConnectionFactory;

    #[async_trait::async_trait]
    impl ConnectionFactory for BenchConnectionFactory {
        async fn create(&self) -> Result<Box<dyn Connection>, sz_orm_core::DbError> {
            Ok(Box::new(BenchConnection { connected: true }))
        }
    }
}

fn begin_commit(c: &mut Criterion) {
    use helpers::BenchConnectionFactory;
    let rt = tokio::runtime::Runtime::new().unwrap();
    c.bench_function("begin_commit", |b| {
        b.to_async(&rt).iter(|| async move {
            let factory = Arc::new(BenchConnectionFactory);
            let mut conn = factory.create().await.unwrap();
            let _ = conn.begin_transaction().await;
            let _ = conn.commit().await;
            black_box(());
        })
    });
}

fn begin_rollback(c: &mut Criterion) {
    use helpers::BenchConnectionFactory;
    let rt = tokio::runtime::Runtime::new().unwrap();
    c.bench_function("begin_rollback", |b| {
        b.to_async(&rt).iter(|| async move {
            let factory = Arc::new(BenchConnectionFactory);
            let mut conn = factory.create().await.unwrap();
            let _ = conn.begin_transaction().await;
            let _ = conn.rollback().await;
            black_box(());
        })
    });
}

fn nested(c: &mut Criterion) {
    use helpers::BenchConnectionFactory;
    let rt = tokio::runtime::Runtime::new().unwrap();
    c.bench_function("nested", |b| {
        b.to_async(&rt).iter(|| async move {
            let factory = Arc::new(BenchConnectionFactory);
            let mut conn = factory.create().await.unwrap();
            let _ = conn.begin_transaction().await;
            let _ = conn.begin_transaction().await;
            let _ = conn.commit().await;
            let _ = conn.commit().await;
            black_box(());
        })
    });
}

criterion_group!(benches, begin_commit, begin_rollback, nested);
criterion_main!(benches);
