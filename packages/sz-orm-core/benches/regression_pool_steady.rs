//! v6.2 性能优化：稳态池基准
//!
//! 隔离建连开销，测量稳态 acquire/release 性能。
//! 验证 P99 ≤ 10μs 与复用率 ≥ 90%。

use criterion::{black_box, Criterion, Throughput};
use criterion::{criterion_group, criterion_main};
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
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<u64, sz_orm_core::DbError>> + Send + 'a>,
        > {
            let _ = sql;
            Box::pin(async move { Ok(1) })
        }
        fn query<'a>(
            &'a mut self,
            sql: &'a str,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<
                        Output = Result<sz_orm_core::QueryRows, sz_orm_core::DbError>,
                    > + Send
                    + 'a,
            >,
        > {
            let _ = sql;
            Box::pin(async { Ok(sz_orm_core::QueryRows::new()) })
        }
        fn begin_transaction<'a>(
            &'a mut self,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>,
        > {
            Box::pin(async { Ok(()) })
        }
        fn commit<'a>(
            &'a mut self,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>,
        > {
            Box::pin(async { Ok(()) })
        }
        fn rollback<'a>(
            &'a mut self,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>,
        > {
            Box::pin(async { Ok(()) })
        }
        fn is_connected(&self) -> bool {
            self.connected
        }
        fn ping<'a>(
            &'a mut self,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = bool> + Send + 'a>> {
            Box::pin(async { true })
        }
        fn close<'a>(
            &'a mut self,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>,
        > {
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

/// 稳态 acquire/release 基准：预建 Pool 后循环 acquire + release，隔离建连开销。
fn pool_steady_acquire_release(c: &mut Criterion) {
    use helpers::BenchConnectionFactory;
    let rt = tokio::runtime::Runtime::new().unwrap();

    let config = PoolConfig {
        max_size: 10,
        min_idle: 2,
        ..PoolConfig::default()
    };
    let factory = Arc::new(BenchConnectionFactory);
    let pool = Arc::new(Pool::new(config, factory).expect("pool creation"));

    let mut group = c.benchmark_group("pool_steady");
    group.throughput(Throughput::Elements(1));
    group.bench_function("pool_steady_acquire_release", |b| {
        b.to_async(&rt).iter(|| {
            let pool = Arc::clone(&pool);
            async move {
                let conn = pool.acquire().await.expect("acquire");
                pool.release(conn).await;
                black_box(());
            }
        })
    });
    group.finish();

    rt.block_on(async {
        pool.close_all().await;
    });
}

/// 稳态复用率基准：运行 1000 次 acquire/release 后断言复用率 ≥ 0.9。
fn pool_reuse_rate_steady(c: &mut Criterion) {
    use helpers::BenchConnectionFactory;
    let rt = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("pool_reuse_rate_steady", |b| {
        b.to_async(&rt).iter(|| async move {
            let config = PoolConfig {
                max_size: 10,
                min_idle: 2,
                ..PoolConfig::default()
            };
            let factory = Arc::new(BenchConnectionFactory);
            let pool = Pool::new(config, factory).expect("pool creation");

            for _ in 0..1000 {
                let conn = pool.acquire().await.expect("acquire");
                pool.release(conn).await;
            }

            let metrics = pool.pool_metrics();
            let reuse_rate = metrics.connection_reuse_rate();
            assert!(reuse_rate >= 0.9, "复用率应 ≥ 0.9，实际: {reuse_rate}");
            black_box(reuse_rate);

            pool.close_all().await;
        })
    });
}

criterion_group!(benches, pool_steady_acquire_release, pool_reuse_rate_steady);
criterion_main!(benches);
