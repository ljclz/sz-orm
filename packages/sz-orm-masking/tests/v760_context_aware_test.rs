//! v7.6.0 方向 4 端到端测试：上下文感知脱敏 + 热更新。

#![cfg(feature = "context-aware-masking")]

use sz_orm_masking::dynamic_masking::{
    ContextAwareMasker, DataFlow, HotUpdateCoordinator, MaskConfig, MaskingContext, MaskingError,
    MaskingRuleConfig, MaskingStrategy,
};

#[test]
fn v760_e2e_context_aware_masking_admin_sees_raw() {
    let rules = vec![MaskingRuleConfig::new(
        "phone",
        MaskingStrategy::Mask(MaskConfig::new(3, 4)),
    )];
    let masker = ContextAwareMasker::new(rules);

    let admin_ctx = MaskingContext::new("admin", "select", DataFlow::Outbound);
    let user_ctx = MaskingContext::new("user", "select", DataFlow::Outbound);

    let raw = "13800138000";
    let admin_result = masker.mask_with_context(raw, "phone", &admin_ctx);
    let user_result = masker.mask_with_context(raw, "phone", &user_ctx);

    assert_eq!(admin_result, raw, "admin 应看到原始数据");
    assert_ne!(user_result, raw, "普通用户应看到脱敏数据");
}

#[test]
fn v760_e2e_hot_update_atomic_switch() {
    let initial = vec![MaskingRuleConfig::new(
        "phone",
        MaskingStrategy::Mask(MaskConfig::new(3, 4)),
    )];
    let coord = HotUpdateCoordinator::new(initial);

    let masker_before = coord.create_masker();
    let ctx = MaskingContext::new("user", "select", DataFlow::Outbound);
    let result_before = masker_before.mask_with_context("13800138000", "phone", &ctx);

    let new_rules = vec![MaskingRuleConfig::new(
        "phone",
        MaskingStrategy::Mask(MaskConfig::default()),
    )];
    coord.hot_update(new_rules).unwrap();

    let masker_after = coord.create_masker();
    let result_after = masker_after.mask_with_context("13800138000", "phone", &ctx);

    assert_ne!(result_before, result_after, "热更新后脱敏策略应变化");
}

#[test]
fn v760_e2e_hot_update_conflict_rejected() {
    let coord = HotUpdateCoordinator::new(vec![]);
    let conflicting = vec![
        MaskingRuleConfig::new("phone", MaskingStrategy::Mask(MaskConfig::default())),
        MaskingRuleConfig::new("phone", MaskingStrategy::Mask(MaskConfig::new(1, 1))),
    ];
    let result = coord.hot_update(conflicting);
    assert!(matches!(result, Err(MaskingError::HotUpdateConflict(_))));
}

#[test]
fn v760_e2e_data_flow_inbound_no_mask() {
    let rules = vec![MaskingRuleConfig::new(
        "phone",
        MaskingStrategy::Mask(MaskConfig::new(3, 4)),
    )];
    let masker = ContextAwareMasker::new(rules);

    let inbound_ctx = MaskingContext::new("user", "insert", DataFlow::Inbound);
    let outbound_ctx = MaskingContext::new("user", "select", DataFlow::Outbound);

    let raw = "13800138000";
    let inbound_result = masker.mask_with_context(raw, "phone", &inbound_ctx);
    let outbound_result = masker.mask_with_context(raw, "phone", &outbound_ctx);

    assert_eq!(inbound_result, raw, "入站数据不脱敏");
    assert_ne!(outbound_result, raw, "出站数据脱敏");
}
