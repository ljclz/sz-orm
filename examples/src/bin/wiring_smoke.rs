//! wiring_smoke — v4.7.0 接线冒烟示例（生产入口）
//!
//! 目的：为门禁 15 的接线断言（W1~W2 与符号断言）提供**可运行的
//! 生产入口证据**——从真实二进制调用 v4.7.0 接线组件：
//!   1. 租户配额：`Pool::acquire_with_tenant` / `release_with_tenant`
//!      （QuotaEnforcer 接入连接池路径）
//!   2. 缓存预热：`ProcessL1Cache::warmup`（CacheWarmer 接入）
//!   3. 查询缓存：`QueryBuilder::execute_with_cache`（cache_ttl → L2Cache）
//!   4. 观测导出：`Pool::metrics_snapshot_json`（运行时指标 JSON）
//!
//! 运行：`cargo run -p sz-orm-examples --bin wiring_smoke`
//! 说明：使用内存 Mock 连接工厂，无需真实数据库。
//! 输出：各接线点的成功标志 + Pool 指标 JSON（观测闭环演示）。

use std::pin::Pin;
use std::sync::Arc;
use sz_orm_core::tenant_quota_rls::{QuotaEnforcer, QuotaResource, TenantResourceQuota};
use sz_orm_core::Value;
use sz_orm_core::{Connection, ConnectionFactory, Pool, PoolConfigBuilder};

// 说明：sz_orm_core 的 pool/query 模块私有，公开 API 均从 crate 根重导出（lib.rs:575/484）

// ============ 冒烟用最小 Model ============

#[derive(Debug, Clone, Default)]
struct SmokeUser {
    id: i64,
}

impl sz_orm_core::Model for SmokeUser {
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

// ============ 内存 Mock 连接工厂（无 DB 依赖） ============

struct MockConn;

impl Connection for MockConn {
    fn execute<'a>(
        &'a mut self,
        _sql: &'a str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<u64, sz_orm_core::DbError>> + Send + 'a>>
    {
        Box::pin(async { Ok(1) })
    }
    fn query<'a>(
        &'a mut self,
        _sql: &'a str,
    ) -> Pin<
        Box<
            dyn std::future::Future<Output = Result<sz_orm_core::QueryRows, sz_orm_core::DbError>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async { Ok(vec![]) })
    }
    fn begin_transaction<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>>
    {
        Box::pin(async { Ok(()) })
    }
    fn commit<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>>
    {
        Box::pin(async { Ok(()) })
    }
    fn rollback<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>>
    {
        Box::pin(async { Ok(()) })
    }
    fn is_connected(&self) -> bool {
        true
    }
    fn ping<'a>(&'a mut self) -> Pin<Box<dyn std::future::Future<Output = bool> + Send + 'a>> {
        Box::pin(async { true })
    }
    fn close<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), sz_orm_core::DbError>> + Send + 'a>>
    {
        Box::pin(async { Ok(()) })
    }
}

struct MockFactory;

#[async_trait::async_trait]
impl ConnectionFactory for MockFactory {
    async fn create(&self) -> Result<Box<dyn Connection>, sz_orm_core::DbError> {
        Ok(Box::new(MockConn))
    }
}

// ============ 接线冒烟 ============

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("== v4.7.0 接线冒烟（wiring_smoke）==");

    // 1. 租户配额接线：QuotaEnforcer → Pool::acquire_with_tenant / release_with_tenant
    let config = PoolConfigBuilder::new().max_size(5).build()?;
    let pool = Pool::new(config, Arc::new(MockFactory))?;
    let enforcer = Arc::new(QuotaEnforcer::new());
    enforcer.set_quota(TenantResourceQuota::new("t1").with_max_connections(3));
    pool.set_quota_enforcer(Some(enforcer.clone()));

    let conn = pool.acquire_with_tenant("t1").await?;
    assert!(conn.is_connected());
    let usage_after_acquire = enforcer.current_usage("t1", QuotaResource::Connection);
    assert_eq!(usage_after_acquire, 1);
    pool.release_with_tenant("t1", conn).await;
    let usage_after_release = enforcer.current_usage("t1", QuotaResource::Connection);
    assert_eq!(
        usage_after_release, 0,
        "release 必须递减配额（P0 修复回归）"
    );
    println!("✅ 1. 租户配额接线：acquire +1 / release -1（使用量 {usage_after_acquire}→{usage_after_release}）");

    // 2. 缓存预热接线：CacheWarmer → ProcessL1Cache::warmup
    use sz_orm_core::cache_warmup_protection::{WarmupConfig, WarmupStrategy};
    use sz_orm_core::process_l1_cache::{ProcessL1Cache, ProcessL1Config};
    let cache: Arc<ProcessL1Cache<String>> = Arc::new(ProcessL1Cache::new(
        ProcessL1Config::new().with_capacity(100),
    ));
    let wconfig = WarmupConfig::new("users", WarmupStrategy::HotspotKey(vec!["1".to_string()]));
    let result = cache
        .warmup(&wconfig, |_| async {
            Ok(vec![(Value::I64(1), "Alice".to_string())])
        })
        .await;
    assert!(result.is_ok(), "预热应成功");
    assert!(
        cache.get("users", &Value::I64(1)).await.is_some(),
        "预热后 key 应命中"
    );
    println!("✅ 2. 缓存预热接线：warmup 后 L1 命中");

    // 3. 查询缓存接线：cache_ttl → L2Cache（execute_with_cache）
    use sz_orm_core::dialect::MySqlDialect;
    use sz_orm_core::l2_cache::L2Cache;
    use sz_orm_core::QueryBuilder;
    let l2 = L2Cache::new();
    let qb = QueryBuilder::<SmokeUser>::new(Box::new(MySqlDialect))
        .table("users")
        .cache_ttl(std::time::Duration::from_secs(300));
    let rows = qb
        .execute_with_cache(&l2, "users", || async { Ok(vec![]) })
        .await;
    assert!(rows.is_ok(), "execute_with_cache 应成功");
    println!("✅ 3. 查询缓存接线：execute_with_cache 消费 cache_ttl");

    // 4. 观测导出：Pool 指标 JSON（观测闭环演示——monitoring/grafana 数据源）
    let snapshot = pool.metrics_snapshot_json();
    assert!(snapshot.contains("acquire_count"), "JSON 快照应含指标字段");
    println!("✅ 4. 观测导出：metrics_snapshot_json = {snapshot}");

    println!("== 接线冒烟全部通过（配额/预热/查询缓存/观测）==");
    Ok(())
}
