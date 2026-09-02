//! TASK-019 集成测试：质量规则生成端到端验证

use sz_orm_governance::quality_rule::{QualityRuleGenerator, QualityRuleType};

#[test]
fn test_generate_rules_for_mixed_columns() {
    let generator = QualityRuleGenerator::new();
    let rules = generator.generate(
        "users",
        &[("id", "BIGINT"), ("name", "VARCHAR"), ("email", "VARCHAR")],
    );

    let null_count = rules
        .iter()
        .filter(|r| r.rule_type == QualityRuleType::NullCheck)
        .count();
    assert_eq!(null_count, 3, "每列应有一条空值检查");

    let range_count = rules
        .iter()
        .filter(|r| r.rule_type == QualityRuleType::RangeCheck)
        .count();
    assert_eq!(range_count, 1, "只有 BIGINT 列应有范围检查");

    let format_count = rules
        .iter()
        .filter(|r| r.rule_type == QualityRuleType::FormatCheck)
        .count();
    assert_eq!(format_count, 2, "VARCHAR 列应有格式检查");
}

#[test]
fn test_rule_ids_are_unique() {
    let generator = QualityRuleGenerator::new();
    let rules = generator.generate("orders", &[("id", "BIGINT"), ("amount", "INTEGER")]);

    let mut ids: Vec<_> = rules.iter().map(|r| &r.rule_id).collect();
    let total = ids.len();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), total, "规则 ID 应唯一");
}

#[test]
fn test_confidence_in_valid_range() {
    let generator = QualityRuleGenerator::new();
    let rules = generator.generate("products", &[("sku", "VARCHAR"), ("price", "INTEGER")]);
    for rule in &rules {
        assert!(
            rule.confidence > 0.0 && rule.confidence <= 1.0,
            "置信度应在 (0, 1] 范围: {}",
            rule.confidence
        );
    }
}
