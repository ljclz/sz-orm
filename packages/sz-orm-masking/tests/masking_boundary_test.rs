//! MaskingRuleConfig 边界与极端场景测试（v7.5.0 组7.1）
//!
//! 验证 MaskingRuleConfig 在空字段名、最大优先级、所有策略等边界条件下的行为。

#![cfg(feature = "dynamic-masking")]

use sz_orm_masking::dynamic_masking::{
    HashConfig, MaskConfig, MaskingHashAlgorithm, MaskingRuleConfig, MaskingRuleSet,
    MaskingStrategy,
};

#[test]
fn test_masking_rule_empty_field() {
    let rule =
        MaskingRuleConfig::new("", MaskingStrategy::Mask(MaskConfig::default())).with_priority(0);
    assert_eq!(rule.field_name, "");
    assert_eq!(rule.priority, 0);
}

#[test]
fn test_masking_rule_max_priority() {
    let rule = MaskingRuleConfig::new("test", MaskingStrategy::Mask(MaskConfig::default()))
        .with_priority(u32::MAX);
    assert_eq!(rule.priority, u32::MAX);
}

#[test]
fn test_masking_strategy_mask_irreversible() {
    let strategy = MaskingStrategy::Mask(MaskConfig::default());
    assert!(strategy.is_irreversible());
    assert!(!strategy.is_encrypt());
}

#[test]
fn test_masking_strategy_hash_irreversible() {
    let strategy = MaskingStrategy::Hash(HashConfig::default());
    assert!(strategy.is_irreversible());
    assert!(!strategy.is_encrypt());
}

#[test]
fn test_masking_strategy_replace_irreversible() {
    let strategy = MaskingStrategy::Replace("***".to_string());
    assert!(strategy.is_irreversible());
}

#[test]
fn test_masking_strategy_truncate_irreversible() {
    let strategy = MaskingStrategy::Truncate(3);
    assert!(strategy.is_irreversible());
}

#[test]
fn test_mask_config_zero_keep() {
    let config = MaskConfig::new(0, 0);
    assert_eq!(config.keep_prefix, 0);
    assert_eq!(config.keep_suffix, 0);
    assert_eq!(config.mask_char, '*');
}

#[test]
fn test_mask_config_max_keep() {
    let config = MaskConfig::new(usize::MAX, usize::MAX).with_mask_char('#');
    assert_eq!(config.keep_prefix, usize::MAX);
    assert_eq!(config.keep_suffix, usize::MAX);
    assert_eq!(config.mask_char, '#');
}

#[test]
fn test_hash_config_empty_salt() {
    let config = HashConfig::new(MaskingHashAlgorithm::Sha256, "");
    assert_eq!(config.salt, "");
    assert_eq!(config.algorithm, MaskingHashAlgorithm::Sha256);
}

#[test]
fn test_hash_config_sha512() {
    let config = HashConfig::new(MaskingHashAlgorithm::Sha512, "salt123");
    assert_eq!(config.algorithm, MaskingHashAlgorithm::Sha512);
    assert_eq!(config.salt, "salt123");
}

#[test]
fn test_masking_rule_set_empty() {
    let rule_set = MaskingRuleSet::new();
    assert!(rule_set.rules().is_empty());
}

#[test]
fn test_masking_rule_set_single_rule() {
    let rule_set = MaskingRuleSet::new().add_rule(MaskingRuleConfig::new(
        "phone",
        MaskingStrategy::Mask(MaskConfig::default()),
    ));
    assert_eq!(rule_set.rules().len(), 1);
}

#[test]
fn test_masking_rule_set_multiple_rules() {
    let rule_set = MaskingRuleSet::new()
        .add_rule(MaskingRuleConfig::new(
            "phone",
            MaskingStrategy::Mask(MaskConfig::default()),
        ))
        .add_rule(MaskingRuleConfig::new(
            "email",
            MaskingStrategy::Hash(HashConfig::default()),
        ))
        .add_rule(MaskingRuleConfig::new(
            "ssn",
            MaskingStrategy::Replace("***".to_string()),
        ));
    assert_eq!(rule_set.rules().len(), 3);
}
