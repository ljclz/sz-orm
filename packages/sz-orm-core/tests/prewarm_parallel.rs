//! v7.3.0 任务 1.4：异步并行连接池预热测试
//!
//! 使用 mock ConnectionFactory 验证 prewarm_parallel 的并行预建、
//! 部分失败不阻塞、失败原因记录。

#![cfg(feature = "auto-prewarm")]

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use sz_orm_core::prewarm::{prewarm_parallel, PrewarmResult, PrewarmStrategy, ProgressiveConfig};
use sz_orm_core::{Connection, ConnectionFactory, Pool, PoolConfigBuilder};

// ─── Mock Connection ──────────────────────────────────────────

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
        _sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<u64, sz_orm_core::DbError>> + Send + 'a>> {
        Box::pin(async { Ok(0) })
    }

    fn query<'a>(
        &'a mut self,
        _sql: &'a str,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<
                        Vec<std::collections::HashMap<String, sz_orm_core::Value>>,
                        sz_orm_core::DbError,
                    >,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(async { Ok(Vec::new()) })
    }

    fn begin_transaction<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }

    fn commit<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }

    fn rollback<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>> {
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
    ) -> Pin<Box<dyn Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>> {
        self.connected = false;
        Box::pin(async { Ok(()) })
    }
}

// ─── Mock ConnectionFactory（可配置失败） ────────────────────

struct MockFactory {
    fail_after: AtomicU32,
    call_count: AtomicU32,
    should_fail: AtomicBool,
}

impl MockFactory {
    fn always_success() -> Self {
        Self {
            fail_after: AtomicU32::new(u32::MAX),
            call_count: AtomicU32::new(0),
            should_fail: AtomicBool::new(false),
        }
    }

    fn fail_after(n: u32) -> Self {
        Self {
            fail_after: AtomicU32::new(n),
            call_count: AtomicU32::new(0),
            should_fail: AtomicBool::new(false),
        }
    }

    fn always_fail() -> Self {
        Self {
            fail_after: AtomicU32::new(0),
            call_count: AtomicU32::new(0),
            should_fail: AtomicBool::new(true),
        }
    }
}

#[async_trait]
impl ConnectionFactory for MockFactory {
    async fn create(&self) -> Result<Box<dyn Connection>, sz_orm_core::DbError> {
        let count = self.call_count.fetch_add(1, Ordering::SeqCst);
        if self.should_fail.load(Ordering::Relaxed)
            || count >= self.fail_after.load(Ordering::Relaxed)
        {
            return Err(sz_orm_core::DbError::ConnectionRefused(
                "mock failure".to_string(),
            ));
        }
        Ok(Box::new(MockConnection::new()))
    }
}

fn build_pool(factory: Arc<dyn ConnectionFactory>, max_size: u32) -> Pool {
    let config = PoolConfigBuilder::new()
        .max_size(max_size)
        .min_idle(0)
        .build()
        .expect("config");
    Pool::new(config, factory).expect("pool")
}

// ─── 测试 ─────────────────────────────────────────────────────

/// 并行预建 8 连接成功
#[tokio::test]
async fn parallel_prewarm_8_connections_success() {
    let factory = Arc::new(MockFactory::always_success());
    let pool = build_pool(factory, 20);

    let result = prewarm_parallel(&pool, 8, PrewarmStrategy::Parallel(4)).await;
    assert_eq!(result.success_count, 8, "应成功预建 8 连接");
    assert_eq!(result.failure_count, 0, "不应有失败");
    assert!(result.all_succeeded());
    assert_eq!(result.total(), 8);
}

/// 串行预建成功
#[tokio::test]
async fn serial_prewarm_success() {
    let factory = Arc::new(MockFactory::always_success());
    let pool = build_pool(factory, 10);

    let result = prewarm_parallel(&pool, 5, PrewarmStrategy::Serial).await;
    assert_eq!(result.success_count, 5);
    assert_eq!(result.failure_count, 0);
}

/// 渐进式预建成功
#[tokio::test]
async fn progressive_prewarm_success() {
    let factory = Arc::new(MockFactory::always_success());
    let pool = build_pool(factory, 20);

    let config = ProgressiveConfig::new(
        3,
        std::time::Duration::from_millis(1),
        std::time::Duration::from_secs(5),
    );
    let result = prewarm_parallel(&pool, 9, PrewarmStrategy::Progressive(config)).await;
    assert_eq!(result.success_count, 9, "应成功预建 9 连接");
    assert_eq!(result.failure_count, 0);
}

/// 部分失败不阻塞 + 失败原因记录
#[tokio::test]
async fn partial_failure_does_not_block() {
    let factory = Arc::new(MockFactory::fail_after(3));
    let pool = build_pool(factory, 20);

    let result = prewarm_parallel(&pool, 8, PrewarmStrategy::Parallel(2)).await;
    assert!(result.failure_count > 0, "应有失败");
    assert!(!result.failures.is_empty(), "应记录失败原因");
    assert!(result.success_count > 0, "应有成功");
    assert!(!result.all_succeeded());
    for f in &result.failures {
        assert!(!f.reason.is_empty(), "失败原因不应为空");
    }
}

/// 全部失败时 success_count=0
#[tokio::test]
async fn all_failures_recorded() {
    let factory = Arc::new(MockFactory::always_fail());
    let pool = build_pool(factory, 10);

    let result = prewarm_parallel(&pool, 4, PrewarmStrategy::Parallel(2)).await;
    assert_eq!(result.success_count, 0, "全部失败时 success_count=0");
    assert_eq!(result.failure_count, 4, "应记录 4 个失败");
    assert_eq!(result.failures.len(), 4);
}

/// PrewarmResult 默认值
#[test]
fn prewarm_result_default() {
    let result = PrewarmResult::default();
    assert_eq!(result.success_count, 0);
    assert_eq!(result.failure_count, 0);
    assert!(result.failures.is_empty());
    assert!(result.all_succeeded());
    assert_eq!(result.total(), 0);
}
