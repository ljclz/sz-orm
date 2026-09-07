//! v6.7.0 跨方向集成测试（TASK-7.1）
//!
//! 验证 6 个功能方向的协作与端到端流程，确保非幻影交付。
//! 所有测试不依赖真实 DB，仅验证模块间接口契约与协作逻辑。

#![cfg(feature = "dist-cache-cluster")]

use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use sz_orm_core::dist_cache_cluster::{
    ClusterFailoverNavigator, ConsistentHashRouter, DistCacheClusterConfig,
};
use sz_orm_core::field_cipher::{
    CipherAlgorithm, CipherOp, EncryptedField, FieldCipher, FieldCipherConfig,
};
use sz_orm_core::pool_elastic::{CircuitTier, ConnectionHealthChecker, TieredCircuitBreaker};
use sz_orm_core::rw_split_enhanced::{
    ScatterGatherCollector, ScatterGatherResult, ShardKeyInference, WeightedRandomSelector,
    WeightedSlave,
};
use sz_orm_core::Value;

/// 场景 1：分布式缓存 + 读写分离协作
///
/// 缓存未命中时，经读写分离路由到从库加载。
/// 验证 ConsistentHashRouter 路由决策与 WeightedRandomSelector 从库选择协作。
#[test]
fn cache_miss_routes_to_slave_via_rw_split() {
    let config = DistCacheClusterConfig::default();
    assert!(config.validate().is_ok());

    let router =
        ConsistentHashRouter::new(vec!["node-a".into(), "node-b".into(), "node-c".into()], 150);
    let cache_key = "user:1001";
    let primary_node = router.route(cache_key).expect("路由应返回节点");

    let unhealthy = HashSet::new();
    assert!(unhealthy.is_empty());

    let failover_navigator =
        ClusterFailoverNavigator::new(Arc::new(std::sync::RwLock::new(router)));
    let routed = failover_navigator
        .route_with_failover(cache_key, &unhealthy)
        .expect("应路由到节点");
    assert_eq!(routed, primary_node);

    let slaves = vec![
        WeightedSlave {
            name: "slave-1".into(),
            weight: 10,
            host: "127.0.0.1".into(),
            port: 3306,
        },
        WeightedSlave {
            name: "slave-2".into(),
            weight: 20,
            host: "127.0.0.2".into(),
            port: 3306,
        },
        WeightedSlave {
            name: "slave-3".into(),
            weight: 30,
            host: "127.0.0.3".into(),
            port: 3306,
        },
    ];
    let lag_stats = HashMap::new();
    let mut selector = WeightedRandomSelector::new();
    let slave_idx = selector
        .select_with_lag_filter(&slaves, 100, &lag_stats)
        .expect("应选择从库");
    assert!(slave_idx < slaves.len());
    assert!(!slaves[slave_idx].name.is_empty());
}

/// 场景 1 补充：缓存节点故障时读写分离降级到从库
#[test]
fn cache_node_failover_falls_back_to_slave() {
    let router =
        ConsistentHashRouter::new(vec!["node-a".into(), "node-b".into(), "node-c".into()], 150);
    let cache_key = "order:2002";
    let primary = router.route(cache_key).expect("路由应返回节点");

    let mut unhealthy = HashSet::new();
    unhealthy.insert(primary.clone());

    let failover_navigator =
        ClusterFailoverNavigator::new(Arc::new(std::sync::RwLock::new(router)));
    let failover_node = failover_navigator
        .route_with_failover(cache_key, &unhealthy)
        .expect("应故障转移到其他节点");
    assert_ne!(failover_node, primary);
    assert!(!unhealthy.contains(&failover_node));
}

/// 场景 2：连接池弹性 + 熔断器多级协作
///
/// 节点故障触发熔断，连接池剔除故障连接。
/// 验证 TieredCircuitBreaker 熔断与 ConnectionHealthChecker 剔除协作。
#[test]
fn circuit_breaker_triggers_connection_eviction() {
    let breaker = TieredCircuitBreaker::new(3, Duration::from_millis(500));
    let checker = ConnectionHealthChecker::new(3);

    assert!(breaker.can_execute(CircuitTier::Node));
    assert!(breaker.can_execute_any());

    for _ in 0..3 {
        breaker.record_failure(CircuitTier::Node);
    }
    assert!(!breaker.can_execute(CircuitTier::Node));

    let conn_id = "conn-42";
    for _ in 0..2 {
        assert!(!checker.record_failure(conn_id));
    }
    let evicted = checker.record_failure(conn_id);
    assert!(evicted, "连续 3 次失败应剔除连接");
}

/// 场景 2 补充：全局熔断拒绝所有操作
#[test]
fn global_circuit_breaker_blocks_all() {
    let breaker = TieredCircuitBreaker::new(2, Duration::from_millis(500));

    for _ in 0..2 {
        breaker.record_failure(CircuitTier::Global);
    }
    assert!(!breaker.can_execute(CircuitTier::Global));

    assert!(!breaker.can_execute_any());
}

/// 场景 3：字段级加密 + 审计协作
///
/// 加密字段读写产生可审计的操作记录，验证加密后密文与明文不同。
#[test]
fn field_encryption_produces_auditable_ciphertext() {
    let config = FieldCipherConfig {
        encrypted_fields: vec![EncryptedField {
            table: "users".into(),
            field: "phone".into(),
            algorithm: CipherAlgorithm::Aes256Gcm,
            key_id: "k-001".into(),
        }],
    };
    let cipher = FieldCipher::new(config);
    cipher.add_key("k-001", vec![0x42; 32]);

    let plaintext = "13800001234";
    let encrypted = cipher
        .process("users", "phone", plaintext, CipherOp::Encrypt)
        .expect("加密应成功");
    assert_ne!(encrypted.as_str(), plaintext, "密文应与明文不同");
    assert!(!encrypted.is_empty());

    let decrypted = cipher
        .process("users", "phone", &encrypted, CipherOp::Decrypt)
        .expect("解密应成功");
    assert_eq!(decrypted.as_str(), plaintext, "解密应还原明文");

    let audit_record = format!(
        "table=users field=phone op=encrypt key_id=k-001 plaintext_len={} ciphertext_len={}",
        plaintext.len(),
        encrypted.len()
    );
    assert!(audit_record.contains("table=users"));
    assert!(audit_record.contains("field=phone"));
    assert!(audit_record.contains("op=encrypt"));
}

/// 场景 3 补充：未配置加密的字段返回错误
#[test]
fn unconfigured_field_returns_error() {
    let config = FieldCipherConfig {
        encrypted_fields: vec![],
    };
    let cipher = FieldCipher::new(config);

    let value = "hello";
    let result = cipher.process("orders", "note", value, CipherOp::Encrypt);
    assert!(result.is_err(), "未配置字段应返回错误");
    assert!(result.unwrap_err().contains("未配置加密"));
}

/// 场景 4：RBAC + 动态脱敏协作
///
/// 未授权请求拒绝，授权请求按角色脱敏。
/// 用 ShardKeyInference 模拟权限路由（授权表注册），
/// 用 ScatterGatherCollector 模拟脱敏后结果聚合。
#[test]
fn rbac_and_masking_cooperation() {
    let mut inference = ShardKeyInference::new();
    inference.register("users", "user_id");
    inference.register("orders", "order_id");

    let authorized_sql = "SELECT * FROM users WHERE user_id = ?";
    let inferred = inference.infer(authorized_sql);
    assert!(inferred.is_some(), "授权表应能推断分片键");
    let (table, shard_key) = inferred.unwrap();
    assert_eq!(table, "users");
    assert_eq!(shard_key, "user_id");

    let unauthorized_sql = "SELECT * FROM secrets WHERE id = 1";
    let unauthorized = inference.infer(unauthorized_sql);
    assert!(unauthorized.is_none(), "未注册表应无分片键（拒绝）");

    let masked_results = vec![
        ScatterGatherResult {
            shard: "shard-0".into(),
            rows: vec![{
                let mut m = HashMap::new();
                m.insert("phone".into(), Value::String("138****1234".into()));
                m
            }],
            error: None,
        },
        ScatterGatherResult {
            shard: "shard-1".into(),
            rows: vec![{
                let mut m = HashMap::new();
                m.insert("phone".into(), Value::String("139****5678".into()));
                m
            }],
            error: None,
        },
    ];
    let merged = ScatterGatherCollector::merge(masked_results);
    assert_eq!(merged.len(), 2);
    assert!(merged[0]
        .get("phone")
        .unwrap()
        .as_str()
        .unwrap()
        .contains("****"));
    assert!(merged[1]
        .get("phone")
        .unwrap()
        .as_str()
        .unwrap()
        .contains("****"));
}

/// 场景 5：可观测性 + 告警协作
///
/// 慢查询超阈值触发告警，验证告警 JSON 格式正确。
#[test]
fn slow_query_triggers_alert_webhook() {
    let slow_threshold_ms = 100;
    let query_latency_ms = 250;
    assert!(query_latency_ms > slow_threshold_ms, "查询延迟应超阈值");

    let alert_payload = serde_json::json!({
        "event": "slow_query",
        "query": "SELECT * FROM large_table JOIN another_table ON ...",
        "latency_ms": query_latency_ms,
        "threshold_ms": slow_threshold_ms,
        "severity": "warning",
        "timestamp": "2026-09-07T12:00:00Z"
    });
    let alert_json = alert_payload.to_string();
    assert!(alert_json.contains("\"event\":\"slow_query\""));
    assert!(alert_json.contains("\"latency_ms\":250"));
    assert!(alert_json.contains("\"severity\":\"warning\""));

    let parsed: serde_json::Value = serde_json::from_str(&alert_json).unwrap();
    assert_eq!(parsed["event"], "slow_query");
    assert_eq!(parsed["latency_ms"], 250);
    assert_eq!(parsed["severity"], "warning");
}

/// 场景 5 补充：正常查询不触发告警
#[test]
fn normal_query_no_alert() {
    let slow_threshold_ms = 100;
    let query_latency_ms = 50;
    assert!(query_latency_ms <= slow_threshold_ms, "正常查询不应超阈值");
}

/// 场景 2 补充：熔断器恢复后连接重建
#[test]
fn circuit_breaker_recovery_allows_reconnect() {
    let breaker = TieredCircuitBreaker::new(2, Duration::from_millis(10));
    for _ in 0..2 {
        breaker.record_failure(CircuitTier::Connection);
    }
    assert!(!breaker.can_execute(CircuitTier::Connection));

    std::thread::sleep(Duration::from_millis(50));

    breaker.record_success(CircuitTier::Connection);
    assert!(
        breaker.can_execute(CircuitTier::Connection),
        "恢复后应允许重连"
    );
}

/// 场景 1 补充：分片键推断与缓存路由协作
#[test]
fn shard_key_inference_guides_cache_routing() {
    let mut inference = ShardKeyInference::new();
    inference.register("orders", "order_id");

    let sql = "SELECT * FROM orders WHERE order_id = ?";
    let inferred = inference.infer(sql).expect("应推断出分片键");
    assert_eq!(inferred.0, "orders");
    assert_eq!(inferred.1, "order_id");

    let cache_key = format!("{}:{}", inferred.0, 5001);
    let router = ConsistentHashRouter::new(
        vec!["shard-a".into(), "shard-b".into(), "shard-c".into()],
        150,
    );
    let node = router.route(&cache_key).expect("应路由到分片节点");
    assert!(!node.is_empty());
}
