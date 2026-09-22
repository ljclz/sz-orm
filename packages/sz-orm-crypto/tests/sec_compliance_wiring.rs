//! v8.0.0 安全合规深化端到端接线测试
//!
//! 验证 5 个子能力的跨模块接线：
//! 1. DekRotationManager 90 天轮换重叠期并行不中断服务
//! 2. KmsHaManager 主 KMS 故障切换备 KMS DEK 缓存兜底
//! 3. ComplianceEvidenceExporter GDPR 证据包导出哈希链校验脱敏加密
//! 4. MaskingPolicyEngine 按角色字段场景脱敏热更新
//! 5. AbacProductionEngine 默认拒绝兼容 RBAC 策略变更审计

use std::sync::Arc;

// ============================================================================
// 测试 1：DekRotationManager 90 天轮换重叠期并行不中断服务
// ============================================================================

#[test]
fn dek_rotation_90day_overlap_no_service_interruption() {
    use sz_orm_crypto::dek_buffer::{DekBuffer, EncryptionAlgo};
    use sz_orm_crypto::key_rotation_enhanced::KeyRotationEnhanced;
    use sz_orm_crypto::tde_mgmt::{DekRotationConfig, DekRotationManager};

    let key_mgr = Arc::new(KeyRotationEnhanced::new("dek-v1", vec![0x42u8; 32]));
    let initial_dek = Arc::new(DekBuffer::new(vec![0x42u8; 32]));
    let mgr = DekRotationManager::new(key_mgr, initial_dek, DekRotationConfig::default());

    // 用旧 DEK 加密敏感数据
    let old_dek = mgr.current_dek();
    let plaintext = b"sensitive-payment-data";
    let ciphertext = old_dek
        .encrypt(plaintext, EncryptionAlgo::Aes256Gcm)
        .expect("旧 DEK 加密应成功");

    // 执行 90 天周期轮换，切换到新 DEK
    let new_dek = Arc::new(DekBuffer::new(vec![0x43u8; 32]));
    let record = mgr.rotate("dek-v2", new_dek).expect("轮换应成功");

    // 验证重叠期 ≥ 24h
    let overlap_secs = record.overlap_until - record.rotated_at;
    assert!(
        overlap_secs >= 24 * 60 * 60,
        "重叠期应 ≥ 24h，实际 {}s",
        overlap_secs
    );

    // 重叠期内旧 DEK 仍可解密（不中断服务）
    let decrypted = old_dek
        .decrypt(&ciphertext, EncryptionAlgo::Aes256Gcm)
        .expect("重叠期内旧 DEK 解密应成功");
    assert_eq!(decrypted, plaintext, "重叠期内旧 DEK 解密结果应与原文一致");

    // 验证状态：当前写 DEK 已切换到新 DEK，活跃 DEK 数 ≥ 2
    let status = mgr.status();
    assert_eq!(status.current_dek_id, "dek-v2");
    assert!(
        status.active_dek_count >= 2,
        "重叠期活跃 DEK 应 ≥ 2，实际 {}",
        status.active_dek_count
    );
}

// ============================================================================
// 测试 2：KmsHaManager 主 KMS 故障切换备 KMS DEK 缓存兜底
// ============================================================================

#[tokio::test]
async fn kms_ha_failover_and_cache_fallback() {
    use sz_orm_crypto::dek_buffer::DekBuffer;
    use sz_orm_crypto::kms_client::LocalKmsClient;
    use sz_orm_crypto::tde_mgmt::{KmsHaConfig, KmsHaManager, KmsNode};

    // 主备 KMS 节点
    let primary = KmsNode::new(
        "primary",
        Arc::new(LocalKmsClient::with_dek("ssn", 1, vec![0x42u8; 32])),
    );
    let backup = KmsNode::new(
        "backup",
        Arc::new(LocalKmsClient::with_dek("ssn", 1, vec![0x42u8; 32])),
    );
    let mgr = KmsHaManager::new(vec![primary, backup], KmsHaConfig::default())
        .expect("至少一个 KMS 节点");

    // 预热 DEK 缓存
    mgr.prewarm_cache("ssn", 1, DekBuffer::new(vec![0x42u8; 32]));
    assert_eq!(mgr.cache_size(), 1);
    assert_eq!(mgr.healthy_node_count(), 2);

    // 主 KMS 故障 → 切换到备 KMS
    let failover_record = mgr.failover().expect("应切换到备 KMS");
    assert_eq!(failover_record.from_kms, "primary");
    assert_eq!(failover_record.to_kms, "backup");
    assert_eq!(mgr.active_node_id(), "backup");
    assert_eq!(mgr.healthy_node_count(), 1);

    // 备 KMS 也故障 → 全部不可用
    let all_fail = mgr.failover();
    assert!(all_fail.is_err(), "主备全故障应返回错误");
    assert_eq!(mgr.healthy_node_count(), 0);

    // DEK 缓存兜底：主备全故障时仍可获取 DEK
    let dek = mgr.get_dek("ssn", 1).await.expect("缓存兜底应成功返回 DEK");
    assert_eq!(
        dek.as_bytes(),
        &vec![0x42u8; 32][..],
        "缓存返回的 DEK 应与预热的一致"
    );
}

// ============================================================================
// 测试 3：ComplianceEvidenceExporter GDPR 证据包导出哈希链校验脱敏加密
// ============================================================================

#[test]
fn evidence_exporter_gdpr_chain_valid_desensitized_encrypted() {
    use sz_orm_audit::evidence_chain::{
        ComplianceEvidenceExporter, ComplianceStandard, EvidenceExportConfig,
    };
    use sz_orm_audit::hash_chain_enhanced::{
        AuditOpType, AuditResult, EnhancedAuditEntry, HashChainEnhancedAuditor,
    };

    let auditor = Arc::new(HashChainEnhancedAuditor::new());

    // 记入含敏感字段的审计记录
    auditor
        .log(EnhancedAuditEntry {
            subject: "alice".to_string(),
            object: "users".to_string(),
            timestamp: 1000,
            op_type: AuditOpType::Select,
            result: AuditResult::Success,
            source_ip: "10.0.0.1".to_string(),
            sql: "SELECT name, email FROM users WHERE id = 1".to_string(),
        })
        .expect("日志记录应成功");
    auditor
        .log(EnhancedAuditEntry {
            subject: "bob".to_string(),
            object: "orders".to_string(),
            timestamp: 2000,
            op_type: AuditOpType::Insert,
            result: AuditResult::Success,
            source_ip: "10.0.0.2".to_string(),
            sql: "INSERT INTO orders (credit_card) VALUES ('1234')".to_string(),
        })
        .expect("日志记录应成功");

    let exporter = ComplianceEvidenceExporter::new(auditor, EvidenceExportConfig::default());

    // 导出 GDPR 合规证据包
    let pkg = exporter
        .export(&[ComplianceStandard::Gdpr])
        .expect("导出应成功");

    // 验证关联合规标准
    assert_eq!(pkg.standards, vec!["GDPR".to_string()]);

    // 验证哈希链校验通过
    assert!(pkg.hash_chain_valid, "哈希链应校验通过");

    // 验证哈希链连续性（前一条 hash == 下一条 prev_hash）
    assert_eq!(pkg.entries.len(), 2);
    assert_eq!(
        pkg.entries[1].prev_hash, pkg.entries[0].hash,
        "哈希链应连续"
    );

    // 验证敏感字段已脱敏（email / credit_card → ******）
    assert!(
        pkg.entries[0].sql.contains("******"),
        "email 应被脱敏为 ******"
    );
    assert!(
        !pkg.entries[0].sql.contains("email"),
        "脱敏后不应包含 email"
    );
    assert!(
        pkg.entries[1].sql.contains("******"),
        "credit_card 应被脱敏为 ******"
    );
    assert!(
        !pkg.entries[1].sql.contains("credit_card"),
        "脱敏后不应包含 credit_card"
    );

    // 验证加密 + 脱敏标记恒为 true
    assert!(pkg.encrypted, "证据包应标记为已加密");
    assert!(pkg.desensitized, "证据包应标记为已脱敏");
}

// ============================================================================
// 测试 4：MaskingPolicyEngine 按角色字段场景脱敏热更新
// ============================================================================

#[test]
fn masking_policy_engine_role_field_scene_hot_reload() {
    use sz_orm_masking::dynamic_masking::{DataFlow, HotUpdateCoordinator, MaskingContext};
    use sz_orm_masking::policy_engine::{
        MaskingPolicy as MaskingPolicyRule, MaskingPolicyConfig, MaskingPolicyEngine, MaskingScene,
        MaskingStrategyKind,
    };

    // 初始策略：viewer 角色对 phone 字段在 API 响应中掩码脱敏
    let policies = vec![MaskingPolicyRule::new(
        "p1",
        "viewer",
        "phone",
        MaskingScene::ApiResponse,
        MaskingStrategyKind::Mask {
            keep_prefix: 3,
            keep_suffix: 4,
        },
    )
    .with_priority(1)];
    let hot_updater = Arc::new(HotUpdateCoordinator::new(vec![]));
    let engine = MaskingPolicyEngine::new(policies, hot_updater, MaskingPolicyConfig::default());

    // viewer 角色 phone 字段应被脱敏
    let ctx_viewer = MaskingContext::new("viewer", "phone", DataFlow::Outbound);
    let masked = engine.mask("13812345678", &ctx_viewer).expect("脱敏应成功");
    assert_ne!(masked, "13812345678", "viewer phone 应被脱敏");
    assert!(masked.starts_with("138"), "应保留前 3 位");
    assert!(masked.ends_with("5678"), "应保留后 4 位");

    // admin 角色不脱敏（管理后台场景）
    let ctx_admin = MaskingContext::new("admin", "phone", DataFlow::Outbound);
    let original = engine
        .mask("13812345678", &ctx_admin)
        .expect("管理后台不脱敏");
    assert_eq!(original, "13812345678", "admin 场景不应脱敏");

    // 热更新策略：切换为 Replace 策略
    let new_policies = vec![MaskingPolicyRule::new(
        "p2",
        "viewer",
        "phone",
        MaskingScene::ApiResponse,
        MaskingStrategyKind::Replace("***".to_string()),
    )];
    engine.hot_reload(&new_policies).expect("热更新应成功");
    assert_eq!(engine.policy_count(), 1, "热更新后策略数应为 1");

    // 热更新后脱敏策略已变更
    let masked_after = engine
        .mask("13812345678", &ctx_viewer)
        .expect("热更新后脱敏应成功");
    assert_eq!(masked_after, "***", "热更新后应使用 Replace 策略");

    // 验证底层 HotUpdateCoordinator 已同步
    let rules = engine.hot_updater().current_rules();
    assert_eq!(rules.len(), 1, "底层规则应已同步");
    assert_eq!(rules[0].field_name, "phone");
}

// ============================================================================
// 测试 5：AbacProductionEngine 默认拒绝兼容 RBAC 策略变更审计
// ============================================================================

#[test]
fn abac_production_default_deny_rbac_compat_audit() {
    use parking_lot::RwLock;
    use sz_orm_auth::abac::composite_authorizer::{CombineMode, CompositeAuthorizer};
    use sz_orm_auth::abac::policy_engine::{
        AbacPolicy, AbacPolicyEngine, AccessRequest, AttributeScope, AttributeValue, Condition,
        Effect,
    };
    use sz_orm_auth::abac_prod::{AbacProductionConfig, AbacProductionEngine};
    use sz_orm_auth::auth::User;
    use sz_orm_auth::authorizer::RbacAuthorizer;

    // ABAC 策略：admin 角色可 read（CompositeAuthorizer 持有独立副本）
    let mut abac_for_composite = AbacPolicyEngine::new();
    abac_for_composite.add_policy(AbacPolicy::new(
        "p1",
        "read",
        Condition::Eq {
            scope: AttributeScope::Subject,
            key: "role".into(),
            value: AttributeValue::str_val("admin"),
        },
        Effect::Allow,
    ));

    // AbacProductionEngine 持有的策略引擎副本
    let mut abac_for_engine = AbacPolicyEngine::new();
    abac_for_engine.add_policy(AbacPolicy::new(
        "p1",
        "read",
        Condition::Eq {
            scope: AttributeScope::Subject,
            key: "role".into(),
            value: AttributeValue::str_val("admin"),
        },
        Effect::Allow,
    ));

    // RBAC 授权 admin read
    let rbac = RbacAuthorizer::new().with_role_permission("admin", "read");
    let composite = Arc::new(CompositeAuthorizer::new(
        rbac,
        abac_for_composite,
        CombineMode::All,
    ));

    let engine = AbacProductionEngine::new(
        Arc::new(RwLock::new(abac_for_engine)),
        composite,
        AbacProductionConfig::default(),
    );

    let admin_user = User::new(1, "alice").with_roles(vec!["admin".to_string()]);
    let guest_user = User::new(2, "bob").with_roles(vec!["guest".to_string()]);

    let admin_req =
        AccessRequest::new("read").with_subject_attr("role", AttributeValue::str_val("admin"));
    let guest_req =
        AccessRequest::new("read").with_subject_attr("role", AttributeValue::str_val("guest"));

    // admin 角色：ABAC + RBAC 均允许 → 允许
    let allow_decision = engine.authorize(&admin_user, "read", "data", &admin_req);
    assert_eq!(allow_decision.effect, Effect::Allow);
    assert!(allow_decision.audit_logged, "决策应记录审计日志");

    // guest 角色：无匹配策略 → 默认拒绝
    let deny_decision = engine.authorize(&guest_user, "read", "data", &guest_req);
    assert_eq!(deny_decision.effect, Effect::Deny, "无匹配策略应默认拒绝");
    assert!(deny_decision.audit_logged, "拒绝决策也应记录审计");

    // 验证策略变更审计：两次授权决策均记录
    assert_eq!(engine.audit_log_count(), 2, "应记录 2 条审计日志");

    // 验证按用户查询审计记录
    let alice_records = engine.audit_records_for_user("alice");
    assert_eq!(alice_records.len(), 1, "alice 应有 1 条审计记录");
    let bob_records = engine.audit_records_for_user("bob");
    assert_eq!(bob_records.len(), 1, "bob 应有 1 条审计记录");

    // 验证 ABAC 禁用后回退纯 RBAC（兼容性）
    let rbac_only = RbacAuthorizer::new().with_role_permission("admin", "read");
    let empty_abac_for_composite = AbacPolicyEngine::new();
    let empty_abac_for_engine = AbacPolicyEngine::new();
    let composite_rbac = Arc::new(CompositeAuthorizer::new(
        rbac_only,
        empty_abac_for_composite,
        CombineMode::All,
    ));
    let engine_rbac = AbacProductionEngine::new(
        Arc::new(RwLock::new(empty_abac_for_engine)),
        composite_rbac,
        AbacProductionConfig::default().with_abac_disabled(),
    );

    let rbac_decision = engine_rbac.authorize(&admin_user, "read", "data", &admin_req);
    assert_eq!(
        rbac_decision.effect,
        Effect::Allow,
        "RBAC 回退应允许 admin read"
    );
}
