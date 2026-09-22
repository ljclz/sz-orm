//! v8.0.0 组4：分布式增强端到端接线测试
//!
//! 4 个端到端测试：
//! ① SagaCoordinator 跨 3 服务编排并行步骤条件分支全链路
//! ② StrongConsistencyCache 写入后立即读强一致验证
//! ③ BiDirectionalSyncCoordinator 双向 CDC 同步 LWW 冲突解决
//! ④ HlcClock 跨节点时间戳单调递增可比

use std::sync::Arc;

use sz_orm_dtx::saga::SagaManager;
use sz_orm_dtx::{
    AuthContext, CrossServiceSagaDef, ParallelGroup, SagaCoordConfig, SagaCoordinator, SagaStepDef,
};

/// ① SagaCoordinator 跨 3 服务编排并行步骤条件分支全链路
#[tokio::test]
async fn test_saga_coordinator_cross_service_orchestration() {
    let mgr = Arc::new(SagaManager::new());
    let coord = SagaCoordinator::new(mgr, SagaCoordConfig::default());

    // 跨 3 服务的 Saga 定义
    let def = CrossServiceSagaDef::new("e2e_saga_3svc")
        // 顺序步骤：3 个服务
        .with_step(SagaStepDef::new("create_order", "order_service"))
        .with_step(SagaStepDef::new("deduct_inventory", "inventory_service"))
        .with_step(SagaStepDef::new("charge_payment", "payment_service"))
        // 并行步骤组：订单服务 + 库存服务并行
        .with_parallel_group(ParallelGroup::new(
            "parallel_notify",
            vec![
                SagaStepDef::new("notify_order", "order_service"),
                SagaStepDef::new("notify_inventory", "inventory_service"),
            ],
        ))
        // 条件分支：VIP 用户走不同路径
        .with_conditional_branch(
            "vip_branch",
            vec![SagaStepDef::new("vip_perks", "payment_service")],
        );

    // 鉴权：3 个服务都授权
    let auth = AuthContext::new(
        "user_001",
        vec![
            "order_service".into(),
            "inventory_service".into(),
            "payment_service".into(),
        ],
    );

    let result = coord
        .orchestrate(&def, &auth)
        .expect("orchestrate must succeed");
    assert!(result.success, "saga should succeed");
    // 3 顺序 + 2 并行 + 1 条件分支 = 6 步骤
    assert_eq!(result.completed_steps.len(), 6);
    // 追踪 span：6 步骤 + 1 总计 = 7
    assert_eq!(result.trace_spans.len(), 7);
    // 验证所有 span 都成功
    assert!(result.trace_spans.iter().all(|s| s.success));
    // 验证涉及 3 个服务
    assert_eq!(def.all_services().len(), 3);
}

/// ② StrongConsistencyCache 写入后立即读强一致验证
#[tokio::test]
async fn test_strong_consistency_cache_write_then_read() {
    use sz_orm_core::StrongConsistencyCache;

    let cache = StrongConsistencyCache::new(Default::default());

    // 写入后立即读返回最新值
    cache
        .write_strong("user:1", b"Alice")
        .expect("write must succeed");
    let val = cache.read_strong("user:1").expect("read must succeed");
    assert_eq!(val, Some(b"Alice".to_vec()));

    // 覆盖写入后读返回最新值
    cache
        .write_strong("user:1", b"Bob")
        .expect("overwrite must succeed");
    let val2 = cache
        .read_strong("user:1")
        .expect("read after overwrite must succeed");
    assert_eq!(val2, Some(b"Bob".to_vec()));

    // 未写入的 key 返回 None
    let val3 = cache
        .read_strong("user:2")
        .expect("read missing must succeed");
    assert_eq!(val3, None);

    // 未降级
    assert!(!cache.is_degraded());
}

/// ③ BiDirectionalSyncCoordinator 双向 CDC 同步 LWW 冲突解决
#[tokio::test]
async fn test_bi_directional_sync_lww() {
    use sz_orm_fusion::{BiDirectionalSyncCoordinator, BiSyncConfig, ConflictStrategy, HlcClock};

    let hlc = Arc::new(HlcClock::new(Default::default()));
    let coord = BiDirectionalSyncCoordinator::new_disconnected(hlc, BiSyncConfig::default());

    // LWW 策略：所有冲突都能自动解决
    let result = coord
        .sync_bidirectional(ConflictStrategy::LastWriteWins)
        .await
        .expect("LWW sync must succeed");
    // 未连接 CDC，降级为 TTL 兜底，synced_count=0
    assert_eq!(result.synced_count, 0);
    // LWW 解决所有冲突
    assert_eq!(result.conflicts_unresolved, 0);

    // CRDT 策略
    let result_crdt = coord
        .sync_bidirectional(ConflictStrategy::Crdt)
        .await
        .expect("CRDT sync must succeed");
    assert_eq!(result_crdt.conflicts_unresolved, 0);

    // Custom 策略：无法自动解决
    let result_custom = coord
        .sync_bidirectional(ConflictStrategy::Custom)
        .await
        .expect("custom sync must return result");
    assert_eq!(result_custom.conflicts_resolved, 0);
    assert!(result_custom
        .warnings
        .iter()
        .any(|w| w.contains("DIST_SYNC_CONFLICT_UNRESOLVED")));
}

/// ④ HlcClock 跨节点时间戳单调递增可比
#[tokio::test]
async fn test_hlc_clock_cross_node_monotonic() {
    use sz_orm_fusion::{HlcClock, HlcConfig};

    let clock_a = Arc::new(HlcClock::new(HlcConfig::default()));
    let clock_b = Arc::new(HlcClock::new(HlcConfig::default()));

    // 节点 A 生成时间戳
    let ts_a1 = clock_a.now();
    let ts_a2 = clock_a.now();
    assert!(ts_a2 > ts_a1, "same node must be monotonic");

    // 节点 B 观察 A 的时间戳
    let ts_b1 = clock_b.observe(ts_a1).expect("observe must succeed");
    assert!(ts_b1 >= ts_a1, "observe must produce >= remote");

    // 节点 B 再生成时间戳，必须 >= 之前观察的
    let ts_b2 = clock_b.now();
    assert!(ts_b2 >= ts_b1, "after observe, now must be >= observed");

    // 节点 A 观察 B 的时间戳
    let ts_a3 = clock_a.observe(ts_b2).expect("observe must succeed");
    assert!(ts_a3 >= ts_b2, "observe must produce >= remote");

    // 跨节点时间戳可比
    assert!(ts_a3 > ts_a1, "cross-node must be monotonic");
}
