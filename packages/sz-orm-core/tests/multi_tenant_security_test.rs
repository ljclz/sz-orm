//! v9.3.0 T10：multi_tenant_pool 安全行为测试
//!
//! 2026-10-27 SDET 整改：原断言为字符串字面量比较（`assert_ne!("tenant_a", "tenant_b")`），
//! 未触及产品代码；现改为验证 `MultiTenantPoolManager` 的配额隔离、自动注册与限流副作用。

use sz_orm_core::multi_tenant_pool::{MultiTenantPoolManager, TenantQuota};

fn quota(max_connections: usize, max_qps: Option<u32>) -> TenantQuota {
    TenantQuota {
        max_connections,
        max_queries_per_second: max_qps,
    }
}

#[test]
fn test_tenant_quota_isolation_exhausted_tenant_does_not_affect_others() {
    let mgr = MultiTenantPoolManager::with_default();
    mgr.register_tenant(1, quota(1, None));
    mgr.register_tenant(2, quota(5, None));

    assert!(mgr.acquire_connection(1).is_ok());
    assert!(
        mgr.acquire_connection(1).is_err(),
        "租户 1 超过 max_connections 必须被拒绝"
    );
    assert!(
        mgr.acquire_connection(2).is_ok(),
        "租户 2 不应受租户 1 配额耗尽影响（租户隔离）"
    );

    mgr.release_connection(1);
    assert!(
        mgr.acquire_connection(1).is_ok(),
        "释放连接后租户 1 应可再次获取"
    );
}

#[test]
fn test_unknown_tenant_auto_registered_with_default_quota() {
    let mgr = MultiTenantPoolManager::with_default();
    assert!(mgr.quota(42).is_none(), "未注册租户不应有配额记录");
    assert!(mgr.check_query(42).is_ok(), "默认配额下首次查询应放行");
    assert!(
        mgr.quota(42).is_some(),
        "未知租户查询后应按默认配额自动注册（行为副作用）"
    );
}

#[test]
fn test_rate_limit_throttles_and_records_stats() {
    let mgr = MultiTenantPoolManager::with_default();
    mgr.register_tenant(7, quota(1, Some(2)));

    assert!(mgr.check_query(7).is_ok());
    assert!(mgr.check_query(7).is_ok());
    assert!(mgr.check_query(7).is_err(), "超过 QPS 上限的查询必须被限流");

    let stats = mgr.stats(7).expect("已注册租户应有统计");
    assert_eq!(stats.throttled_queries, 1, "限流次数应被记录为副作用");
    assert_eq!(stats.total_queries, 3, "总查询数应包含被限流的查询");
}
