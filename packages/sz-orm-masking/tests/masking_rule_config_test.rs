//! 5.6 端到端测试：字段级脱敏 + 不可逆验证 + 多策略冲突 + 未启用时原样返回。

use std::collections::HashMap;

use sz_orm_masking::{
    apply_strategy, EncryptAlgorithm, EncryptConfig, HashConfig, MaskConfig, MaskingHashAlgorithm,
    MaskingRuleConfig, MaskingRuleSet, MaskingStrategy,
};

#[test]
fn field_level_masking_phone() {
    let set = MaskingRuleSet::new().add_rule(MaskingRuleConfig::new(
        "phone",
        MaskingStrategy::Mask(MaskConfig::new(3, 4)),
    ));
    let (result, conflict) = set.apply("phone", "13812345678");
    assert_eq!(result, "138****5678");
    assert!(conflict.is_none());
}

#[test]
fn field_level_masking_email() {
    let set = MaskingRuleSet::new().add_rule(MaskingRuleConfig::new(
        "email",
        MaskingStrategy::Mask(MaskConfig::new(1, 11)),
    ));
    let (result, _) = set.apply("email", "user@example.com");
    assert!(result.starts_with('u'));
    assert!(result.ends_with("example.com"));
    assert!(result.contains('*'));
}

#[test]
fn field_level_hash_irreversible() {
    let set = MaskingRuleSet::new().add_rule(MaskingRuleConfig::new(
        "ssn",
        MaskingStrategy::Hash(HashConfig::new(MaskingHashAlgorithm::Sha256, "salt")),
    ));
    let original = "123-45-6789";
    let (result, _) = set.apply("ssn", original);
    assert_ne!(result, original);
    assert!(!result.contains("123"));
    assert!(!result.contains("6789"));
}

#[test]
fn field_level_truncate_irreversible() {
    let set = MaskingRuleSet::new().add_rule(MaskingRuleConfig::new(
        "long_text",
        MaskingStrategy::Truncate(10),
    ));
    let original = "This is a very long text that should be truncated";
    let (result, _) = set.apply("long_text", original);
    assert_ne!(result, original);
    assert_eq!(result.len(), 10);
}

#[test]
fn field_level_replace_irreversible() {
    let set = MaskingRuleSet::new().add_rule(MaskingRuleConfig::new(
        "password",
        MaskingStrategy::Replace("***".to_string()),
    ));
    let (result, _) = set.apply("password", "my_secret_password");
    assert_eq!(result, "***");
}

#[test]
fn field_level_encrypt_key_separation() {
    let set = MaskingRuleSet::new().add_rule(MaskingRuleConfig::new(
        "credit_card",
        MaskingStrategy::Encrypt(EncryptConfig::new(
            EncryptAlgorithm::Aes256Gcm,
            "vault://credit_card_key",
        )),
    ));
    let original = "4532123456789012";
    let (result, _) = set.apply("credit_card", original);
    assert!(!result.contains(original));
    assert!(result.contains("vault://credit_card_key"));
    assert!(result.contains("AES-256-GCM"));
}

#[test]
fn multi_strategy_conflict_resolves_to_highest_priority() {
    let set = MaskingRuleSet::new()
        .add_rule(
            MaskingRuleConfig::new("phone", MaskingStrategy::Mask(MaskConfig::new(3, 4)))
                .with_priority(10),
        )
        .add_rule(
            MaskingRuleConfig::new("phone", MaskingStrategy::Hash(HashConfig::default()))
                .with_priority(50),
        )
        .add_rule(
            MaskingRuleConfig::new("phone", MaskingStrategy::Replace("***".to_string()))
                .with_priority(100),
        );
    let (result, conflict) = set.apply("phone", "13812345678");
    assert_eq!(result, "138****5678");
    let c = conflict.expect("should have conflict warning");
    assert_eq!(c.field_name, "phone");
    assert_eq!(c.resolved_priority, 10);
    assert_eq!(c.conflicting_priorities, vec![10, 50, 100]);
    assert!(c.message().contains("MASKING_POLICY_CONFLICT"));
}

#[test]
fn no_rule_returns_original_unchanged() {
    let set = MaskingRuleSet::new();
    let original = "13812345678";
    let (result, conflict) = set.apply("phone", original);
    assert_eq!(result, original);
    assert!(conflict.is_none());
}

#[test]
fn apply_to_map_mixed_fields() {
    let set = MaskingRuleSet::new()
        .add_rule(MaskingRuleConfig::new(
            "phone",
            MaskingStrategy::Mask(MaskConfig::new(3, 4)),
        ))
        .add_rule(MaskingRuleConfig::new(
            "email",
            MaskingStrategy::Hash(HashConfig::new(MaskingHashAlgorithm::Sha256, "s")),
        ))
        .add_rule(MaskingRuleConfig::new(
            "password",
            MaskingStrategy::Replace("***".to_string()),
        ));
    let mut data = HashMap::new();
    data.insert("phone".to_string(), "13812345678".to_string());
    data.insert("email".to_string(), "user@example.com".to_string());
    data.insert("password".to_string(), "secret123".to_string());
    data.insert("name".to_string(), "Alice".to_string());
    data.insert("age".to_string(), "30".to_string());
    let (result, conflicts) = set.apply_to_map(&data);
    assert_eq!(result["phone"], "138****5678");
    assert_ne!(result["email"], "user@example.com");
    assert_eq!(result["password"], "***");
    assert_eq!(result["name"], "Alice");
    assert_eq!(result["age"], "30");
    assert!(conflicts.is_empty());
}

#[test]
fn masking_irreversible_all_strategies() {
    let original = "sensitive_data_12345";

    let masked = apply_strategy(original, &MaskingStrategy::Mask(MaskConfig::new(3, 3)));
    assert_ne!(masked, original);
    assert!(!masked.contains("sensitive_data"));

    let hashed = apply_strategy(
        original,
        &MaskingStrategy::Hash(HashConfig::new(MaskingHashAlgorithm::Sha256, "salt")),
    );
    assert_ne!(hashed, original);
    assert!(!hashed.contains("sensitive_data"));

    let truncated = apply_strategy(original, &MaskingStrategy::Truncate(5));
    assert_ne!(truncated, original);
    assert!(!truncated.contains("sensitive_data"));

    let replaced = apply_strategy(original, &MaskingStrategy::Replace("***".to_string()));
    assert_ne!(replaced, original);
    assert!(!replaced.contains("sensitive_data"));
}

#[test]
fn strategy_is_irreversible_flags() {
    assert!(MaskingStrategy::Mask(MaskConfig::default()).is_irreversible());
    assert!(MaskingStrategy::Hash(HashConfig::default()).is_irreversible());
    assert!(MaskingStrategy::Truncate(5).is_irreversible());
    assert!(MaskingStrategy::Replace("***".to_string()).is_irreversible());
    assert!(
        !MaskingStrategy::Encrypt(EncryptConfig::new(EncryptAlgorithm::Aes256Gcm, "k"))
            .is_irreversible()
    );
}

#[test]
fn hash_with_different_salts_produces_different_results() {
    let s1 = MaskingStrategy::Hash(HashConfig::new(MaskingHashAlgorithm::Sha256, "salt1"));
    let s2 = MaskingStrategy::Hash(HashConfig::new(MaskingHashAlgorithm::Sha256, "salt2"));
    let r1 = apply_strategy("test", &s1);
    let r2 = apply_strategy("test", &s2);
    assert_ne!(r1, r2);
}

#[test]
fn mask_config_custom_mask_char() {
    let set = MaskingRuleSet::new().add_rule(MaskingRuleConfig::new(
        "phone",
        MaskingStrategy::Mask(MaskConfig::new(3, 4).with_mask_char('#')),
    ));
    let (result, _) = set.apply("phone", "13812345678");
    assert_eq!(result, "138####5678");
}

#[test]
fn empty_value_handling() {
    let set = MaskingRuleSet::new().add_rule(MaskingRuleConfig::new(
        "phone",
        MaskingStrategy::Mask(MaskConfig::new(3, 4)),
    ));
    let (result, _) = set.apply("phone", "");
    assert_eq!(result, "");
}

#[test]
fn short_value_mask_all() {
    let set = MaskingRuleSet::new().add_rule(MaskingRuleConfig::new(
        "phone",
        MaskingStrategy::Mask(MaskConfig::new(3, 4)),
    ));
    let (result, _) = set.apply("phone", "123");
    assert_eq!(result, "***");
}
