//! Chaos 故障鲁棒测试套件
//!
//! 模拟生产环境中的故障场景，验证系统在故障下的鲁棒性：
//! - 网络分区：连接突然断开、操作中途失败
//! - 磁盘满：写入操作返回错误
//! - 时钟漂移：连接因 max_lifetime/idle_timeout 过期被回收
//! - 主从切换：主节点故障后从节点接管
//! - 级联故障：多个故障同时发生
//!
//! 不变量：
//! - 任意时刻 active_count <= max_size
//! - 故障连接不能进入 idle 队列
//! - close_all 后 release 必须关闭连接并递减 active_count
//! - 故障期间事务状态机保持正确（Active → RolledBack）
//! - 故障恢复后池能正常服务新请求

mod common;

use async_trait::async_trait;
use common::{MockConnectionFactory, Rng};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use sz_orm_core::TransactOptions;
use sz_orm_core::Transaction;
use sz_orm_core::{Connection, ConnectionFactory, DbError, Pool, PoolConfigBuilder};
use tokio::sync::Mutex;

// ===================== Chaos 专用 Mock =====================

/// 可断开的连接：模拟网络分区
/// 每个连接持有独立的 connected flag（per-instance），
/// 由工厂统一管理。工厂可调用 partition_all() 断开所有现有连接。
/// close() 只关闭自身（per-instance closed flag），不影响 connected flag。
struct DisconnectableConnection {
    /// per-instance 连通性标志（true=连通，false=分区）
    connected: Arc<AtomicBool>,
    /// per-instance 关闭标志（close() 后设为 true）
    closed: AtomicBool,
}

impl DisconnectableConnection {
    fn new(connected: Arc<AtomicBool>) -> Self {
        Self {
            connected,
            closed: AtomicBool::new(false),
        }
    }

    fn is_alive(&self) -> bool {
        self.connected.load(Ordering::SeqCst) && !self.closed.load(Ordering::SeqCst)
    }
}

impl Connection for DisconnectableConnection {
    fn execute<'a>(
        &'a mut self,
        _sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<u64, DbError>> + Send + 'a>> {
        Box::pin(async move {
            if !self.is_alive() {
                return Err(DbError::ConnectionError("network partition".to_string()));
            }
            Ok(1)
        })
    }

    fn query<'a>(
        &'a mut self,
        _sql: &'a str,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<
                        Vec<std::collections::HashMap<String, sz_orm_core::Value>>,
                        DbError,
                    >,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            if !self.is_alive() {
                return Err(DbError::ConnectionError("network partition".to_string()));
            }
            Ok(vec![])
        })
    }

    fn begin_transaction<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move {
            if !self.is_alive() {
                return Err(DbError::ConnectionError("network partition".to_string()));
            }
            Ok(())
        })
    }

    fn commit<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move {
            if !self.is_alive() {
                return Err(DbError::ConnectionError("network partition".to_string()));
            }
            Ok(())
        })
    }

    fn rollback<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        // rollback 即使在分区时也应成功（事务被丢弃）
        Box::pin(async move { Ok(()) })
    }

    fn is_connected(&self) -> bool {
        self.is_alive()
    }

    fn ping<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move { self.is_alive() })
    }

    fn close<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move {
            // R1 修复：只关闭自身，不污染共享 connected flag
            self.closed.store(true, Ordering::SeqCst);
            Ok(())
        })
    }
}

/// 磁盘满连接：execute 在第 N 次后返回磁盘满错误
struct DiskFullConnection {
    connected: bool,
    fail_after: u32,
    execute_count: AtomicU32,
}

impl DiskFullConnection {
    fn new(fail_after: u32) -> Self {
        Self {
            connected: true,
            fail_after,
            execute_count: AtomicU32::new(0),
        }
    }
}

impl Connection for DiskFullConnection {
    fn execute<'a>(
        &'a mut self,
        _sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<u64, DbError>> + Send + 'a>> {
        Box::pin(async move {
            let n = self.execute_count.fetch_add(1, Ordering::SeqCst);
            if n >= self.fail_after {
                return Err(DbError::IoError("disk full".to_string()));
            }
            Ok(1)
        })
    }

    fn query<'a>(
        &'a mut self,
        _sql: &'a str,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<
                        Vec<std::collections::HashMap<String, sz_orm_core::Value>>,
                        DbError,
                    >,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            let n = self.execute_count.fetch_add(1, Ordering::SeqCst);
            if n >= self.fail_after {
                return Err(DbError::IoError("disk full".to_string()));
            }
            Ok(vec![])
        })
    }

    fn begin_transaction<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(()) })
    }

    fn commit<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move {
            let n = self.execute_count.load(Ordering::SeqCst);
            if n >= self.fail_after {
                return Err(DbError::IoError("disk full on commit".to_string()));
            }
            Ok(())
        })
    }

    fn rollback<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(()) })
    }

    fn is_connected(&self) -> bool {
        self.connected
    }

    fn ping<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move { self.connected })
    }

    fn close<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move {
            self.connected = false;
            Ok(())
        })
    }
}

/// 可切换主从的工厂：primary 失败后切到 secondary
/// primary 不可用时 create 仍返回健康连接（模拟 secondary 接管），
/// 但递增 failover_count 以便测试断言。
struct FailoverFactory {
    primary_available: Arc<AtomicBool>,
    failover_count: AtomicU32,
}

impl FailoverFactory {
    fn new(primary_available: Arc<AtomicBool>) -> Self {
        Self {
            primary_available,
            failover_count: AtomicU32::new(0),
        }
    }

    fn failover_count(&self) -> u32 {
        self.failover_count.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl ConnectionFactory for FailoverFactory {
    async fn create(&self) -> Result<Box<dyn Connection>, DbError> {
        // primary 可用时返回健康连接；不可用时计入 failover_count 仍返回连接
        // （模拟 secondary 接管，连接本身是健康的）
        if !self.primary_available.load(Ordering::SeqCst) {
            self.failover_count.fetch_add(1, Ordering::SeqCst);
        }
        let connected = Arc::new(AtomicBool::new(true));
        Ok(Box::new(DisconnectableConnection::new(connected)))
    }
}

/// 周期性故障工厂：在 outage 期间创建失败，恢复后正常
struct FlakyFactory {
    available: Arc<AtomicBool>,
}

impl FlakyFactory {
    fn new(available: Arc<AtomicBool>) -> Self {
        Self { available }
    }
}

#[async_trait]
impl ConnectionFactory for FlakyFactory {
    async fn create(&self) -> Result<Box<dyn Connection>, DbError> {
        if !self.available.load(Ordering::SeqCst) {
            return Err(DbError::ConnectionRefused(
                "service unavailable".to_string(),
            ));
        }
        let connected = Arc::new(AtomicBool::new(true));
        Ok(Box::new(DisconnectableConnection::new(connected)))
    }
}

/// 网络分区工厂：创建的连接持有独立 connected flag（per-instance）
/// 通过 partition_all() 可断开所有已创建的连接（模拟网络分区）
/// 新创建的连接仍持有 flag=true（健康），模拟"新连接通过备用路径成功"
struct NetworkPartitionFactory {
    /// 所有已创建连接的 connected flag（per-instance）
    conns: std::sync::Mutex<Vec<Arc<AtomicBool>>>,
}

impl NetworkPartitionFactory {
    fn new() -> Self {
        Self {
            conns: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// 断开所有现有连接（模拟网络分区）
    fn partition_all(&self) {
        let conns = self.conns.lock().unwrap();
        for c in conns.iter() {
            c.store(false, Ordering::SeqCst);
        }
    }
}

#[async_trait]
impl ConnectionFactory for NetworkPartitionFactory {
    async fn create(&self) -> Result<Box<dyn Connection>, DbError> {
        let flag = Arc::new(AtomicBool::new(true));
        self.conns.lock().unwrap().push(flag.clone());
        Ok(Box::new(DisconnectableConnection::new(flag)))
    }
}

/// 磁盘满工厂：创建的连接在 execute_count >= fail_after 时返回磁盘满错误
struct DiskFullFactory {
    fail_after: u32,
}

impl DiskFullFactory {
    fn new(fail_after: u32) -> Self {
        Self { fail_after }
    }
}

#[async_trait]
impl ConnectionFactory for DiskFullFactory {
    async fn create(&self) -> Result<Box<dyn Connection>, DbError> {
        Ok(Box::new(DiskFullConnection::new(self.fail_after)))
    }
}

// ===================== 辅助函数 =====================

fn make_pool_with_factory(
    max_size: u32,
    acquire_timeout_secs: u64,
    factory: Arc<dyn ConnectionFactory>,
) -> &'static Pool {
    let config = PoolConfigBuilder::new()
        .max_size(max_size)
        .min_idle(0)
        .acquire_timeout(acquire_timeout_secs)
        .build()
        .unwrap();
    let pool = Pool::new(config, factory).unwrap();
    Box::leak(Box::new(pool))
}

// ===================== 网络分区场景 =====================

/// Chaos 1：网络分区 - 空闲连接突然断开
/// 验证：池的 acquire 应跳过 is_connected=false 的 idle 连接，并创建新连接
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn chaos_network_partition_idle_connections_dropped() {
    let factory = Arc::new(NetworkPartitionFactory::new());
    let pool = make_pool_with_factory(10, 5, factory.clone());

    // 创建 3 个连接并释放（变成 idle）
    let mut conns = Vec::new();
    for _ in 0..3 {
        conns.push(pool.acquire().await.unwrap());
    }
    for conn in conns {
        pool.release(conn).await;
    }
    let status = pool.status().await;
    assert_eq!(status.idle, 3, "should have 3 idle connections");

    // 模拟网络分区：断开所有现有连接（per-instance flag=false）
    // 新连接仍持有 flag=true（健康）
    factory.partition_all();

    // acquire 应跳过断开的 idle 连接（close 并递减 active_count），
    // 然后创建新连接（flag=true，健康）
    let conn = pool.acquire().await.unwrap();
    assert!(conn.is_connected(), "acquire should return healthy conn");

    // 验证 idle 队列已清空（3 个断开连接被 close）
    let status = pool.status().await;
    assert_eq!(
        status.idle, 0,
        "idle should be 0 after dropping disconnected"
    );
    assert!(
        status.active <= 10,
        "active must not exceed max_size: {}",
        status.active
    );
}

/// Chaos 2：网络分区 - 操作中途失败（事务级测试，R4：事务行为不依赖池）
/// 验证：事务在 execute 失败后能正确 rollback
#[tokio::test]
async fn chaos_network_partition_mid_operation_rollback() {
    let connected = Arc::new(AtomicBool::new(true));
    let conn = DisconnectableConnection::new(connected.clone());
    let mut tx = Transaction::new(Box::new(conn), TransactOptions::default());
    assert!(tx.is_active());

    // 第一次 execute 成功
    tx.execute("INSERT 1").await.unwrap();

    // 模拟网络分区
    connected.store(false, Ordering::SeqCst);

    // 第二次 execute 应失败
    let result = tx.execute("INSERT 2").await;
    assert!(result.is_err(), "execute during partition should fail");

    // rollback 应成功（即使分区，事务也应能回滚到 RolledBack 状态）
    let rollback_result = tx.rollback().await;
    assert!(rollback_result.is_ok(), "rollback should succeed");

    // 事务不应再 active
    assert!(!tx.is_active());
}

/// Chaos 3：网络分区 - 并发场景下池的一致性
/// 验证：多任务并发 acquire/release 期间网络分区，最终无连接泄漏
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn chaos_network_partition_concurrent_consistency() {
    let factory = Arc::new(NetworkPartitionFactory::new());
    let pool = make_pool_with_factory(20, 2, factory.clone());

    let ops_completed = Arc::new(AtomicU64::new(0));
    let max_active = Arc::new(AtomicU32::new(0));

    let mut handles = Vec::new();
    for task_id in 0..8 {
        let ops = ops_completed.clone();
        let max_act = max_active.clone();
        let factory_c = factory.clone();
        handles.push(tokio::spawn(async move {
            for i in 0..50 {
                // 偶尔模拟分区（断开所有现有连接）
                if i == 25 && task_id == 0 {
                    factory_c.partition_all();
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }

                let conn = match pool.acquire().await {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                let status = pool.status().await;
                max_act.fetch_max(status.active, Ordering::Relaxed);
                tokio::task::yield_now().await;
                pool.release(conn).await;
                ops.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    assert!(ops_completed.load(Ordering::Relaxed) > 0);
    assert!(
        max_active.load(Ordering::Relaxed) <= 20,
        "active must never exceed max_size: {}",
        max_active.load(Ordering::Relaxed)
    );

    // 最终无借出
    let status = pool.status().await;
    assert_eq!(
        status.active, status.idle,
        "after all ops: active={}, idle={}",
        status.active, status.idle
    );
}

// ===================== 磁盘满场景 =====================

/// Chaos 4：磁盘满 - 写入失败
/// 验证：execute 返回 IoError，事务能 rollback
#[tokio::test]
async fn chaos_disk_full_write_fails() {
    let conn = DiskFullConnection::new(1); // 第 1 次 execute 成功，之后失败
    let mut tx = Transaction::new(Box::new(conn), TransactOptions::default());

    // 第一次 execute 成功
    tx.execute("INSERT 1").await.unwrap();

    // 第二次 execute 失败（磁盘满，n=1 >= fail_after=1）
    let result = tx.execute("INSERT 2").await;
    assert!(result.is_err(), "second execute should fail with disk full");
    if let Err(e) = result {
        let msg = format!("{}", e);
        assert!(
            msg.contains("disk full") || msg.contains("Io"),
            "error should mention disk full: {}",
            msg
        );
    }

    // rollback 应成功
    tx.rollback().await.unwrap();
    assert!(!tx.is_active());
}

/// Chaos 5：磁盘满 - commit 失败
/// 验证：commit 失败时事务保持 active，可以 rollback
#[tokio::test]
async fn chaos_disk_full_commit_fails_rollback_succeeds() {
    let conn = DiskFullConnection::new(5); // 前 5 次 execute 成功，之后失败
    let mut tx = Transaction::new(Box::new(conn), TransactOptions::default());

    // 执行 5 次 execute 全部成功（n=0,1,2,3,4 均 < 5）
    for _ in 0..5 {
        tx.execute("INSERT").await.unwrap();
    }

    // commit 应失败（execute_count=5 >= fail_after=5）
    let commit_result = tx.commit().await;
    assert!(commit_result.is_err(), "commit should fail with disk full");

    // 事务仍应能 rollback（commit 失败后保持 active）
    let rollback_result = tx.rollback().await;
    assert!(
        rollback_result.is_ok(),
        "rollback after failed commit should succeed"
    );
    assert!(!tx.is_active());
}

/// Chaos 6：磁盘满 - 池在写入失败后仍可服务新连接（R5：验证 execute 成功）
/// 验证：单个连接磁盘满不影响池的其他连接，新连接可正常 execute
#[tokio::test]
async fn chaos_disk_full_pool_recovers() {
    let factory = Arc::new(DiskFullFactory::new(1)); // 第 1 次成功，之后失败
    let pool = make_pool_with_factory(5, 3, factory);

    // 获取连接，模拟磁盘满
    let conn1 = pool.acquire().await.unwrap();
    let mut tx = Transaction::new(conn1.into_inner(), TransactOptions::default());
    tx.execute("INSERT 1").await.unwrap();
    let _ = tx.execute("INSERT 2").await; // 这次会失败但不影响后续
    let _ = tx.rollback().await;

    // 获取新连接，验证可正常 execute（R5：真正验证恢复后可用性）
    let conn2 = pool.acquire().await;
    assert!(conn2.is_ok(), "pool should still serve new connections");
    let mut conn2 = conn2.unwrap();
    // 新连接第 1 次 execute 应成功（fail_after=1，n=0 < 1）
    let exec_result = conn2.execute("SELECT 1").await;
    assert!(
        exec_result.is_ok(),
        "new connection should execute successfully, got: {:?}",
        exec_result
    );

    let status = pool.status().await;
    assert!(
        status.active <= 5,
        "active must not exceed max_size: {}",
        status.active
    );
}

// ===================== 时钟漂移场景 =====================

/// Chaos 7：时钟漂移 - 连接因 max_lifetime 过期被回收
/// 验证：设置极短的 max_lifetime（1s），acquire 应不返回过期连接
#[tokio::test]
async fn chaos_clock_drift_connection_expired() {
    let db = Arc::new(Mutex::new(common::InMemoryDb::new()));
    let factory = Arc::new(MockConnectionFactory::new(db));
    let config = PoolConfigBuilder::new()
        .max_size(5)
        .min_idle(0)
        .max_lifetime(1) // 1 秒，便于测试
        .idle_timeout(1)
        .acquire_timeout(5)
        .build()
        .unwrap();
    let pool: &'static Pool = Box::leak(Box::new(Pool::new(config, factory).unwrap()));

    // 创建连接并释放
    let conn = pool.acquire().await.unwrap();
    pool.release(conn).await;

    // 等待连接过期（模拟时钟快速前进）
    tokio::time::sleep(Duration::from_millis(1200)).await;

    // acquire 应不返回过期连接，而是创建新连接
    let conn = pool.acquire().await.unwrap();
    assert!(conn.is_connected(), "should get fresh connection");

    let status = pool.status().await;
    assert!(
        status.active <= 5,
        "active must not exceed max_size: {}",
        status.active
    );
}

/// Chaos 8：时钟漂移 - reap_idle 回收过期连接
/// 验证：reap_idle 应回收过期的 idle 连接，active_count 正确递减
#[tokio::test]
async fn chaos_clock_drift_reap_idle_expires_connections() {
    let db = Arc::new(Mutex::new(common::InMemoryDb::new()));
    let factory = Arc::new(MockConnectionFactory::new(db));
    let config = PoolConfigBuilder::new()
        .max_size(5)
        .min_idle(0)
        .max_lifetime(1)
        .idle_timeout(1)
        .acquire_timeout(5)
        .build()
        .unwrap();
    let pool: &'static Pool = Box::leak(Box::new(Pool::new(config, factory).unwrap()));

    // 一次性创建 3 个连接（全部借出），然后释放（变成 idle）
    let mut conns = Vec::new();
    for _ in 0..3 {
        conns.push(pool.acquire().await.unwrap());
    }
    let status = pool.status().await;
    assert_eq!(status.active, 3, "should have 3 active (borrowed)");
    assert_eq!(status.idle, 0, "should have 0 idle while all borrowed");

    for conn in conns {
        pool.release(conn).await;
    }
    let status = pool.status().await;
    assert_eq!(status.idle, 3, "should have 3 idle after release");

    // 等待过期
    tokio::time::sleep(Duration::from_millis(1200)).await;

    // reap_idle 应回收所有过期连接
    pool.reap_idle().await;
    let status = pool.status().await;
    assert_eq!(status.idle, 0, "all expired connections should be reaped");
    assert_eq!(status.active, 0, "active should be 0 after reaping all");
}

// ===================== 主从切换场景 =====================

/// Chaos 9：主从切换 - 主故障后从接管（C2：通过 Pool 测试故障切换）
/// 验证：primary_available=false 后，Pool acquire 触发 create 来自 secondary
#[tokio::test]
async fn chaos_master_slave_failover_to_secondary() {
    let primary_available = Arc::new(AtomicBool::new(true));
    let factory = Arc::new(FailoverFactory::new(primary_available.clone()));
    let pool = make_pool_with_factory(5, 3, factory.clone());

    // 从 primary 获取连接（不 release，保持 borrowed 以触发下次 create）
    let conn1 = pool.acquire().await.unwrap();
    assert!(conn1.is_connected());
    assert_eq!(factory.failover_count(), 0, "no failover yet");

    // 模拟主故障
    primary_available.store(false, Ordering::SeqCst);

    // 通过 Pool acquire 触发 create（primary 不可用，failover_count++）
    let conn2 = pool.acquire().await.unwrap();
    assert!(
        conn2.is_connected(),
        "secondary connection should be healthy"
    );
    assert_eq!(
        factory.failover_count(),
        1,
        "failover_count should be 1 after one secondary create"
    );

    // 恢复主
    primary_available.store(true, Ordering::SeqCst);
    let conn3 = pool.acquire().await.unwrap();
    assert!(conn3.is_connected());
    assert_eq!(
        factory.failover_count(),
        1,
        "no new failover after primary recovered"
    );

    // 释放连接（active_count 递减）
    pool.release(conn1).await;
    pool.release(conn2).await;
    pool.release(conn3).await;

    let status = pool.status().await;
    assert!(
        status.active <= 5,
        "active must not exceed max_size: {}",
        status.active
    );
}

/// Chaos 10：主从切换 - 池在故障后恢复
/// 验证：primary 故障期间 acquire 失败，恢复后 acquire 成功
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn chaos_master_slave_pool_recovery_after_outage() {
    let available = Arc::new(AtomicBool::new(true));
    let factory = Arc::new(FlakyFactory::new(available.clone()));
    let pool = make_pool_with_factory(5, 1, factory);

    // 初始正常工作
    let conn = pool.acquire().await.unwrap();
    pool.release(conn).await;

    // 模拟 outage
    available.store(false, Ordering::SeqCst);

    // acquire 应失败（池内空闲连接可能仍可用，但创建新连接会失败）
    // 先耗尽空闲连接
    let mut held = Vec::new();
    for _ in 0..1 {
        if let Ok(c) = pool.acquire().await {
            held.push(c);
        }
    }

    // 现在池内无空闲，新 acquire 应超时
    let result = tokio::time::timeout(Duration::from_secs(2), pool.acquire()).await;
    assert!(
        result.is_err() || result.unwrap().is_err(),
        "acquire during outage should fail or timeout"
    );

    // 释放持有的连接
    for conn in held {
        pool.release(conn).await;
    }

    // 恢复
    available.store(true, Ordering::SeqCst);

    // 应能再次获取连接
    let conn = pool.acquire().await;
    assert!(conn.is_ok(), "pool should recover after outage");

    let status = pool.status().await;
    assert!(
        status.active <= 5,
        "active must not exceed max_size: {}",
        status.active
    );
}

// ===================== 级联故障场景 =====================

/// Chaos 11：级联故障 - close_all 期间有借出连接
/// 验证：close_all 后 release 借出的连接，active_count 正确递减
#[tokio::test]
async fn chaos_close_all_with_borrowed_connections() {
    let db = Arc::new(Mutex::new(common::InMemoryDb::new()));
    let factory = Arc::new(MockConnectionFactory::new(db));
    let pool = make_pool_with_factory(5, 3, factory);

    // 借出 3 个连接
    let conn1 = pool.acquire().await.unwrap();
    let conn2 = pool.acquire().await.unwrap();
    let conn3 = pool.acquire().await.unwrap();

    let status = pool.status().await;
    assert_eq!(status.active, 3);
    assert_eq!(status.idle, 0);

    // close_all（只关闭 idle，但 idle=0）
    pool.close_all().await;

    // 释放借出的连接（应被直接关闭，active_count 递减）
    pool.release(conn1).await;
    pool.release(conn2).await;
    pool.release(conn3).await;

    let status = pool.status().await;
    assert_eq!(status.active, 0, "active should be 0 after releasing all");
    assert_eq!(status.idle, 0, "idle should be 0 after close_all");
}

/// Chaos 12：级联故障 - 工厂持续失败 + 并发 acquire
/// 验证：工厂持续失败时，acquire 返回错误，不泄漏 active_count
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn chaos_factory_persistent_failure_no_leak() {
    let available = Arc::new(AtomicBool::new(false)); // 一开始就不可用
    let factory = Arc::new(FlakyFactory::new(available.clone()));
    let pool = make_pool_with_factory(10, 1, factory);

    let error_count = Arc::new(AtomicU32::new(0));
    let mut handles = Vec::new();
    for _ in 0..4 {
        let err_count = error_count.clone();
        handles.push(tokio::spawn(async move {
            for _ in 0..10 {
                if pool.acquire().await.is_err() {
                    err_count.fetch_add(1, Ordering::Relaxed);
                }
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    // 所有 acquire 都应失败（工厂不可用）
    assert_eq!(
        error_count.load(Ordering::Relaxed),
        40,
        "all 40 acquires should fail"
    );

    // active_count 必须为 0（失败的 acquire 不应泄漏计数器）
    let status = pool.status().await;
    assert_eq!(
        status.active, 0,
        "active_count must be 0 after all failures, got {}",
        status.active
    );
}

/// Chaos 13：级联故障 - 网络分区 + 时钟漂移
/// 验证：多重故障下池状态仍然一致
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn chaos_cascading_failures_consistency() {
    let factory = Arc::new(NetworkPartitionFactory::new());
    let config = PoolConfigBuilder::new()
        .max_size(8)
        .min_idle(0)
        .max_lifetime(2) // 短 lifetime 模拟时钟漂移
        .idle_timeout(2)
        .acquire_timeout(2)
        .build()
        .unwrap();
    let pool: &'static Pool = Box::leak(Box::new(Pool::new(config, factory.clone()).unwrap()));

    let ops = Arc::new(AtomicU64::new(0));
    let max_active = Arc::new(AtomicU32::new(0));

    let mut handles = Vec::new();
    for task_id in 0..4 {
        let ops_c = ops.clone();
        let max_c = max_active.clone();
        let factory_c = factory.clone();
        handles.push(tokio::spawn(async move {
            for i in 0..30 {
                // 偶尔触发分区（断开所有现有连接）
                if i == 15 && task_id == 0 {
                    factory_c.partition_all();
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }

                if let Ok(conn) = pool.acquire().await {
                    let status = pool.status().await;
                    max_c.fetch_max(status.active, Ordering::Relaxed);
                    tokio::task::yield_now().await;
                    pool.release(conn).await;
                    ops_c.fetch_add(1, Ordering::Relaxed);
                }
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    assert!(ops.load(Ordering::Relaxed) > 0);
    assert!(
        max_active.load(Ordering::Relaxed) <= 8,
        "active must never exceed max_size: {}",
        max_active.load(Ordering::Relaxed)
    );

    // 最终状态一致
    let status = pool.status().await;
    assert_eq!(
        status.active, status.idle,
        "after cascading failures: active={}, idle={}",
        status.active, status.idle
    );
}

/// Chaos 14：随机故障注入 - 多种故障随机组合
/// 验证：随机故障下系统最终能恢复一致状态
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn chaos_random_failure_injection() {
    let available = Arc::new(AtomicBool::new(true));
    let factory = Arc::new(FlakyFactory::new(available.clone()));
    let pool = make_pool_with_factory(15, 2, factory);

    let ops = Arc::new(AtomicU64::new(0));
    let max_active = Arc::new(AtomicU32::new(0));
    let mut rng = Rng::new(42);

    let mut handles = Vec::new();
    for task_id in 0..6 {
        let ops_c = ops.clone();
        let max_c = max_active.clone();
        let avail = available.clone();
        let seed = rng.next_u64();
        handles.push(tokio::spawn(async move {
            let mut local_rng = Rng::new(seed);
            for _ in 0..40 {
                // 偶尔触发工厂 outage
                if task_id == 0 && local_rng.next_f64() < 0.05 {
                    avail.store(false, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    avail.store(true, Ordering::SeqCst);
                }

                if let Ok(conn) = pool.acquire().await {
                    let status = pool.status().await;
                    max_c.fetch_max(status.active, Ordering::Relaxed);
                    // 随机持有时间
                    let hold_ms = local_rng.next_usize(5);
                    tokio::time::sleep(Duration::from_millis(hold_ms as u64)).await;
                    pool.release(conn).await;
                    ops_c.fetch_add(1, Ordering::Relaxed);
                }
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    assert!(ops.load(Ordering::Relaxed) > 0);
    assert!(
        max_active.load(Ordering::Relaxed) <= 15,
        "active must never exceed max_size: {}",
        max_active.load(Ordering::Relaxed)
    );

    let status = pool.status().await;
    assert_eq!(
        status.active, status.idle,
        "after random failures: active={}, idle={}",
        status.active, status.idle
    );
}

// ===================== 事务级联故障 =====================

/// Chaos 15：事务故障级联 - execute 失败 + commit 失败 + rollback 成功
/// 验证：多重故障下事务状态机仍正确
#[tokio::test]
async fn chaos_transaction_cascading_failure_state_machine() {
    let conn = DiskFullConnection::new(4); // 前 4 次 execute 成功，之后失败
    let mut tx = Transaction::new(Box::new(conn), TransactOptions::default());

    // 执行 4 次成功（n=0,1,2,3 均 < 4）
    for _ in 0..4 {
        tx.execute("INSERT").await.unwrap();
    }

    // 第 5 次 execute 失败（磁盘满，n=4 >= fail_after=4）
    let result = tx.execute("INSERT").await;
    assert!(result.is_err(), "5th execute should fail");

    // 事务仍 active（execute 失败不应改变状态）
    assert!(
        tx.is_active(),
        "tx should still be active after execute failure"
    );

    // commit 也应失败（execute_count=5 >= fail_after=4）
    let commit_result = tx.commit().await;
    assert!(commit_result.is_err(), "commit should fail");

    // 事务仍 active（commit 失败不应改变状态）
    assert!(
        tx.is_active(),
        "tx should still be active after failed commit"
    );

    // rollback 应成功
    tx.rollback().await.unwrap();
    assert!(!tx.is_active(), "tx should be inactive after rollback");
}

/// Chaos 16：事务故障 - rollback 后再操作应失败
/// 验证：事务进入终态后所有操作都失败
#[tokio::test]
async fn chaos_transaction_terminal_state_rejects_operations() {
    let db = Arc::new(Mutex::new(common::InMemoryDb::new()));
    let conn = common::MockConnection::new(db);
    let mut tx = Transaction::new(Box::new(conn), TransactOptions::default());

    tx.execute("INSERT").await.unwrap();
    tx.rollback().await.unwrap();

    // 终态后所有操作应失败
    assert!(tx.execute("SELECT").await.is_err());
    assert!(tx.commit().await.is_err());
    assert!(tx.rollback().await.is_err());
}
