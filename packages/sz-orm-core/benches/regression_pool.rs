//! 路径 2：连接池 benchmark（3 基准点）
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

fn acquire_release(c: &mut Criterion) {
    use helpers::BenchConnectionFactory;
    let rt = tokio::runtime::Runtime::new().unwrap();
    c.bench_function("acquire_release", |b| {
        b.to_async(&rt).iter(|| async move {
            let config = PoolConfig {
                max_size: 4,
                min_idle: 1,
                ..PoolConfig::default()
            };
            let factory = Arc::new(BenchConnectionFactory);
            let pool = Pool::new(config, factory).unwrap();
            let conn = pool.acquire().await.unwrap();
            pool.release(conn).await;
            pool.close_all().await;
            black_box(());
        })
    });
}

fn acquire_reuse(c: &mut Criterion) {
    use helpers::BenchConnectionFactory;
    let rt = tokio::runtime::Runtime::new().unwrap();
    c.bench_function("acquire_reuse", |b| {
        b.to_async(&rt).iter(|| async move {
            let config = PoolConfig {
                max_size: 1,
                min_idle: 1,
                ..PoolConfig::default()
            };
            let factory = Arc::new(BenchConnectionFactory);
            let pool = Pool::new(config, factory).unwrap();
            let conn = pool.acquire().await.unwrap();
            pool.release(conn).await;
            let conn2 = pool.acquire().await.unwrap();
            pool.release(conn2).await;
            pool.close_all().await;
        })
    });
}

fn acquire_contention(c: &mut Criterion) {
    use helpers::BenchConnectionFactory;
    let rt = tokio::runtime::Runtime::new().unwrap();
    c.bench_function("acquire_contention", |b| {
        b.to_async(&rt).iter(|| async move {
            let config = PoolConfig {
                max_size: 2,
                min_idle: 1,
                ..PoolConfig::default()
            };
            let factory = Arc::new(BenchConnectionFactory);
            let pool = Pool::new(config, factory).unwrap();
            let c1 = pool.acquire().await.unwrap();
            let c2 = pool.acquire().await.unwrap();
            pool.release(c1).await;
            pool.release(c2).await;
            pool.close_all().await;
        })
    });
}

criterion_group!(benches, acquire_release, acquire_reuse, acquire_contention);
criterion_main!(benches);
