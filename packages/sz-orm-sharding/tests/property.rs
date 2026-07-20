//! Property-Based Testing — 一致性哈希路由器属性测试
//!
//! 对 `ConsistentHashRouter` 和 `CompositeRouter` 进行属性测试，
//! 验证一致性哈希的核心不变量：
//!
//! - **确定性**：同一 key 多次路由结果完全一致
//! - **一致性**：添加节点后，已有 key 的路由结果要么不变，要么迁移到新节点，
//!   不会迁移到其他旧节点
//! - **环大小**：`ring_size() == nodes.len() * vnodes_per_node`（哈希无冲突时）
//! - **分布均匀性**：大量随机 key 路由后，各节点负责的比例接近 1/N

use proptest::prelude::*;
use sz_orm_sharding::enhanced::{CompositeRouter, ConsistentHashRouter, ShardGroup};

proptest! {
    /// 属性：一致性哈希的确定性
    ///
    /// 同一 router 对同一 key 的多次路由结果必须完全一致。
    #[test]
    fn prop_consistent_hash_deterministic(
        nodes in prop::collection::vec("[a-z][a-z0-9]{0,7}", 1..10),
        vnodes in 1usize..200,
        key in "[a-zA-Z0-9_:]{1,20}",
    ) {
        let nodes_ref: Vec<&str> = nodes.iter().map(|s| s.as_str()).collect();
        let router = ConsistentHashRouter::new(nodes_ref, vnodes);
        let r1 = router.route(&key).unwrap();
        let r2 = router.route(&key).unwrap();
        let r3 = router.route(&key).unwrap();
        prop_assert!(r1 == r2 && r2 == r3, "路由不一致: r1={:?} r2={:?} r3={:?}", r1, r2, r3);
        // 路由结果必须是已配置的节点之一
        prop_assert!(
            nodes.contains(&r1),
            "路由结果 {:?} 不在节点列表 {:?} 中",
            r1,
            nodes
        );
    }

    /// 属性：一致性哈希的一致性（添加节点后数据迁移最小化）
    ///
    /// 添加新节点后，已有 key 的路由结果要么：
    /// 1. 保持不变（仍路由到原节点）
    /// 2. 迁移到新节点
    /// 绝不会迁移到其他旧节点（这是"一致性"的核心保证）。
    #[test]
    fn prop_consistent_hash_consistency_on_add(
        existing_nodes in prop::collection::vec("[a-z]{1,5}", 2..8),
        new_node in "[a-z]{1,5}",
        keys in prop::collection::vec("[a-zA-Z0-9]{1,15}", 50..200),
    ) {
        // 跳过 new_node 与 existing_nodes 重复的情况
        prop_assume!(!existing_nodes.contains(&new_node));

        let nodes_ref: Vec<&str> = existing_nodes.iter().map(|s| s.as_str()).collect();
        let router_before = ConsistentHashRouter::new(nodes_ref.clone(), 50);
        let router_after = {
            let mut r = ConsistentHashRouter::new(nodes_ref, 50);
            r.add_node(&new_node);
            r
        };

        for key in &keys {
            let before = router_before.route(key).unwrap();
            let after = router_after.route(key).unwrap();
            // 路由结果要么不变，要么迁移到新节点
            prop_assert!(
                after == before || after == new_node,
                "key {:?} 路由从 {:?} 变为 {:?}（新节点 {:?}），违反一致性",
                key,
                before,
                after,
                new_node
            );
        }
    }

    /// 属性：环大小 = 节点数 × 虚拟节点数（无哈希冲突时）
    ///
    /// 注意：FNV-1a + fmix64 在极端情况下可能产生哈希冲突（不同 vnode_key 映射到同一 hash），
    /// 此时 `ring_insert` 用 `entry().or_insert()` 保留先插入的，导致 ring_size < nodes * vnodes。
    /// 所以这里用 <= 而非 ==。
    #[test]
    fn prop_ring_size_bounds(
        node_count in 1usize..20,
        vnodes in 1usize..100,
    ) {
        let nodes: Vec<String> = (0..node_count).map(|i| format!("n{}", i)).collect();
        let nodes_ref: Vec<&str> = nodes.iter().map(|s| s.as_str()).collect();
        let router = ConsistentHashRouter::new(nodes_ref, vnodes);
        let expected = node_count * vnodes;
        let actual = router.ring_size();
        // 哈希冲突时 actual < expected，但不会超过 expected
        prop_assert!(
            actual <= expected,
            "ring_size {} 超过预期 {}（nodes={} × vnodes={}）",
            actual,
            expected,
            node_count,
            vnodes
        );
        prop_assert!(
            actual >= node_count,
            "ring_size {} 小于节点数 {}（至少每个节点一个虚拟节点）",
            actual,
            node_count
        );
    }

    /// 属性：分布均匀性
    ///
    /// 大量随机 key 路由后，每个节点负责的 key 比例应接近 1/N。
    /// 容差设为 ±50%（一致性哈希的虚拟节点数足够时分布较均匀）。
    #[test]
    fn prop_distribution_uniformity(
        node_count in 3usize..8,
    ) {
        let nodes: Vec<String> = (0..node_count).map(|i| format!("node{}", i)).collect();
        let nodes_ref: Vec<&str> = nodes.iter().map(|s| s.as_str()).collect();
        let router = ConsistentHashRouter::new(nodes_ref, 150);

        let key_count = 5000;
        let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for i in 0..key_count {
            let key = format!("key_{}", i);
            let node = router.route(&key).unwrap();
            *counts.entry(node).or_insert(0) += 1;
        }

        let expected = key_count as f64 / node_count as f64;
        let tolerance = expected * 0.5; // ±50%
        for node in &nodes {
            let count = *counts.get(node).unwrap_or(&0) as f64;
            let diff = (count - expected).abs();
            prop_assert!(
                diff <= tolerance,
                "节点 {:?} 负载 {:.0} 偏离期望 {:.1} 超过容差 ±{:.1}",
                node,
                count,
                expected,
                tolerance
            );
        }
    }

    /// 属性：CompositeRouter 的确定性与 group 路由
    ///
    /// 同一 (group_id, key) 多次路由结果一致，且结果属于该 group 的 shards。
    #[test]
    fn prop_composite_router_deterministic(
        shards in prop::collection::vec("[a-z]{1,8}", 2..10),
        key in "[a-zA-Z0-9]{1,20}",
    ) {
        let shards_ref: Vec<&str> = shards.iter().map(|s| s.as_str()).collect();
        let group = ShardGroup::new("g1", shards_ref);
        let router = CompositeRouter::new().add_group(group);

        let r1 = router.route("g1", &key).unwrap();
        let r2 = router.route("g1", &key).unwrap();
        prop_assert!(r1 == r2, "CompositeRouter 路由不一致: r1={:?} r2={:?}", r1, r2);
        prop_assert!(
            shards.contains(&r1),
            "CompositeRouter 路由结果 {:?} 不在 shards {:?} 中",
            r1,
            shards
        );
    }

    /// 属性：CompositeRouter 缓存正确性（P-2 修复验证）
    ///
    /// P-2 修复后，CompositeRouter 缓存了哈希环。此属性验证：
    /// 缓存的环与即时构建的环路由结果完全一致（即缓存不会改变路由行为）。
    #[test]
    fn prop_composite_router_cache_correctness(
        shards in prop::collection::vec("[a-z]{1,8}", 2..10),
        vnodes in 1usize..100,
        keys in prop::collection::vec("[a-zA-Z0-9]{1,15}", 10..50),
    ) {
        let shards_ref: Vec<&str> = shards.iter().map(|s| s.as_str()).collect();
        // 1. CompositeRouter（带缓存）
        let composite = CompositeRouter::new()
            .with_vnodes(vnodes)
            .add_group(ShardGroup::new("g", shards_ref.clone()));
        // 2. 直接用 ConsistentHashRouter（无缓存，每次重建）
        let direct = ConsistentHashRouter::new(shards_ref, vnodes);

        for key in &keys {
            let r_composite = composite.route("g", key).unwrap();
            let r_direct = direct.route(key).unwrap();
            prop_assert!(
                r_composite == r_direct,
                "缓存路由结果 {:?} 与直接路由 {:?} 不一致（key={:?}）",
                r_composite,
                r_direct,
                key
            );
        }
    }
}
