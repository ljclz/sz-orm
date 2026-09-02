//! TASK-020 集成测试：脱敏推荐+复核端到端验证

use sz_orm_governance::masking_recommend::{MaskingRecommender, MaskingStrategy};

#[test]
fn test_recommend_for_sensitive_fields() {
    let recommender = MaskingRecommender::new();
    let recs = recommender.recommend(&[
        ("phone_number", "VARCHAR"),
        ("email", "VARCHAR"),
        ("id_card", "VARCHAR"),
        ("password", "VARCHAR"),
        ("name", "VARCHAR"),
    ]);

    assert_eq!(recs[0].strategy, MaskingStrategy::Mask, "手机号应掩码");
    assert_eq!(recs[1].strategy, MaskingStrategy::Mask, "邮箱应掩码");
    assert_eq!(recs[2].strategy, MaskingStrategy::Hash, "身份证应哈希");
    assert_eq!(recs[3].strategy, MaskingStrategy::Hash, "密码应哈希");
    assert_eq!(recs[4].strategy, MaskingStrategy::Replace, "普通字段替换");
}

#[test]
fn test_review_detects_missing_sensitive_field() {
    let recommender = MaskingRecommender::new();
    let recs = recommender.recommend(&[("name", "VARCHAR")]);
    let result = recommender.review(
        &recs,
        &[
            ("name", "VARCHAR"),
            ("phone", "VARCHAR"),
            ("email", "VARCHAR"),
        ],
    );
    assert!(result.is_err(), "遗漏 phone 和 email 应报错");
}

#[test]
fn test_review_passes_when_all_sensitive_covered() {
    let recommender = MaskingRecommender::new();
    let all_fields = [
        ("phone", "VARCHAR"),
        ("email", "VARCHAR"),
        ("name", "VARCHAR"),
    ];
    let recs = recommender.recommend(&all_fields);
    let result = recommender.review(&recs, &all_fields);
    assert!(result.is_ok(), "所有敏感字段已覆盖应通过");
}
