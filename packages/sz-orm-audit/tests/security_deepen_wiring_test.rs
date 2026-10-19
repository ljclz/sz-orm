//! 安全合规深化接线端到端测试（v10.10.0 T4.1~T4.5）

#![cfg(feature = "compliance-auto-scan")]

use sz_orm_audit::compliance_scan_desensitizer::{
    ComplianceScanDesensitizer, DesensitizeInput, DesensitizeMatcher, DesensitizeRule,
    DesensitizeStrategy, FieldTypeRecognizer,
};
use sz_orm_audit::compliance_auto_checker::{
    AlertLevel, ComplianceAutoChecker, ComplianceAutoCheckConfig, Violation, ViolationSeverity,
};
use sz_orm_audit::gdpr_ccpa::{
    ComplianceRegion, CrossBorderTransferControl, DataFormat, DataSubjectRight, GdprCcpaProcessor,
};

// ============================================================================
// T4.1: 数据脱敏增强（DesensitizeRule/Strategy 脱敏扩展）
// ============================================================================

#[test]
fn test_desensitize_email_field_masked() {
    let mut desensitizer = ComplianceScanDesensitizer::new(false);
    desensitizer
        .add_rule(DesensitizeRule {
            name: "email_mask".to_string(),
            field_type: FieldTypeRecognizer::Email,
            matcher: DesensitizeMatcher::FieldType(FieldTypeRecognizer::Email),
            strategy: DesensitizeStrategy::Mask {
                keep_prefix: 2,
                keep_suffix: 5,
            },
        })
        .unwrap();

    let results = desensitizer
        .desensitize_batch(vec![DesensitizeInput {
            field_name: "email".to_string(),
            value: "user@example.com".to_string(),
        }])
        .unwrap();

    assert_eq!(results.len(), 1);
    assert_ne!(
        results[0].desensitized_value, "user@example.com",
        "Email 应被脱敏"
    );
    assert!(
        results[0].desensitized_value.contains('*'),
        "脱敏后应包含掩码字符 *"
    );
}

#[test]
fn test_desensitize_phone_field_masked() {
    let mut desensitizer = ComplianceScanDesensitizer::new(false);
    desensitizer
        .add_rule(DesensitizeRule {
            name: "phone_mask".to_string(),
            field_type: FieldTypeRecognizer::Phone,
            matcher: DesensitizeMatcher::FieldType(FieldTypeRecognizer::Phone),
            strategy: DesensitizeStrategy::Mask {
                keep_prefix: 3,
                keep_suffix: 4,
            },
        })
        .unwrap();

    let results = desensitizer
        .desensitize_batch(vec![DesensitizeInput {
            field_name: "phone".to_string(),
            value: "13812345678".to_string(),
        }])
        .unwrap();

    assert_ne!(
        results[0].desensitized_value, "13812345678",
        "Phone 应被脱敏"
    );
    assert!(
        results[0].desensitized_value.contains('*'),
        "脱敏后应包含掩码字符"
    );
}

#[test]
fn test_desensitize_id_card_field_hashed() {
    let mut desensitizer = ComplianceScanDesensitizer::new(false);
    desensitizer
        .add_rule(DesensitizeRule {
            name: "idcard_hash".to_string(),
            field_type: FieldTypeRecognizer::IdCard,
            matcher: DesensitizeMatcher::FieldType(FieldTypeRecognizer::IdCard),
            strategy: DesensitizeStrategy::Hash {
                algorithm: "SHA256".to_string(),
            },
        })
        .unwrap();

    let id_card = "110101199001011234";
    let results = desensitizer
        .desensitize_batch(vec![DesensitizeInput {
            field_name: "id_card".to_string(),
            value: id_card.to_string(),
        }])
        .unwrap();

    assert_ne!(
        results[0].desensitized_value, id_card,
        "IdCard 应被哈希脱敏"
    );
    assert!(
        results[0].desensitized_value.starts_with("SHA256"),
        "哈希脱敏应标注算法"
    );
}

#[test]
fn test_desensitize_bank_card_field_masked() {
    let mut desensitizer = ComplianceScanDesensitizer::new(false);
    desensitizer
        .add_rule(DesensitizeRule {
            name: "bankcard_mask".to_string(),
            field_type: FieldTypeRecognizer::BankCard,
            matcher: DesensitizeMatcher::FieldType(FieldTypeRecognizer::BankCard),
            strategy: DesensitizeStrategy::Mask {
                keep_prefix: 4,
                keep_suffix: 4,
            },
        })
        .unwrap();

    let results = desensitizer
        .desensitize_batch(vec![DesensitizeInput {
            field_name: "bank_card".to_string(),
            value: "6222021234567890123".to_string(),
        }])
        .unwrap();

    assert_ne!(
        results[0].desensitized_value, "6222021234567890123",
        "BankCard 应被脱敏"
    );
    assert!(
        results[0].desensitized_value.contains('*'),
        "脱敏后应包含掩码字符"
    );
}

#[test]
fn test_desensitize_regex_matcher_applies_rule() {
    let mut desensitizer = ComplianceScanDesensitizer::new(false);
    desensitizer
        .add_rule(DesensitizeRule {
            name: "token_rule".to_string(),
            field_type: FieldTypeRecognizer::Custom("token".to_string()),
            matcher: DesensitizeMatcher::Regex("token".to_string()),
            strategy: DesensitizeStrategy::Replace {
                replacement: "[REDACTED]".to_string(),
            },
        })
        .unwrap();

    let results = desensitizer
        .desensitize_batch(vec![DesensitizeInput {
            field_name: "auth_header".to_string(),
            value: "Bearer token_abc123".to_string(),
        }])
        .unwrap();

    assert_eq!(
        results[0].desensitized_value, "[REDACTED]",
        "Regex 匹配应应用替换策略"
    );
    assert_eq!(
        results[0].rule_applied,
        Some("token_rule".to_string()),
        "应标注应用的规则名"
    );
}

#[test]
fn test_desensitize_dict_matcher_applies_rule() {
    let mut desensitizer = ComplianceScanDesensitizer::new(false);
    let mut dict = std::collections::HashSet::new();
    dict.insert("secret".to_string());
    dict.insert("password".to_string());

    desensitizer
        .add_rule(DesensitizeRule {
            name: "dict_rule".to_string(),
            field_type: FieldTypeRecognizer::Custom("dict".to_string()),
            matcher: DesensitizeMatcher::Dict(dict),
            strategy: DesensitizeStrategy::Replace {
                replacement: "***".to_string(),
            },
        })
        .unwrap();

    let results = desensitizer
        .desensitize_batch(vec![DesensitizeInput {
            field_name: "keyword".to_string(),
            value: "secret".to_string(),
        }])
        .unwrap();

    assert_eq!(
        results[0].desensitized_value, "***",
        "Dict 匹配应应用替换策略"
    );
}

#[test]
fn test_desensitize_batch_processes_multiple_records() {
    let mut desensitizer = ComplianceScanDesensitizer::new(false);
    desensitizer
        .add_rule(DesensitizeRule {
            name: "email_mask".to_string(),
            field_type: FieldTypeRecognizer::Email,
            matcher: DesensitizeMatcher::FieldType(FieldTypeRecognizer::Email),
            strategy: DesensitizeStrategy::Mask {
                keep_prefix: 1,
                keep_suffix: 3,
            },
        })
        .unwrap();

    let inputs = vec![
        DesensitizeInput {
            field_name: "email1".to_string(),
            value: "a@example.com".to_string(),
        },
        DesensitizeInput {
            field_name: "email2".to_string(),
            value: "b@test.org".to_string(),
        },
        DesensitizeInput {
            field_name: "email3".to_string(),
            value: "c@demo.net".to_string(),
        },
    ];

    let results = desensitizer.desensitize_batch(inputs).unwrap();
    assert_eq!(results.len(), 3, "应处理所有 3 条记录");

    for r in &results {
        assert_ne!(r.desensitized_value, r.field_name);
        assert!(r.rule_applied.is_some(), "每条记录应标注应用的规则");
    }
}

#[test]
fn test_desensitize_irreversible() {
    let mut desensitizer = ComplianceScanDesensitizer::new(false);
    desensitizer
        .add_rule(DesensitizeRule {
            name: "hash_rule".to_string(),
            field_type: FieldTypeRecognizer::Custom("sensitive".to_string()),
            matcher: DesensitizeMatcher::Regex("sensitive".to_string()),
            strategy: DesensitizeStrategy::Hash {
                algorithm: "SHA256".to_string(),
            },
        })
        .unwrap();

    let original = "sensitive_data_123";
    let results = desensitizer
        .desensitize_batch(vec![DesensitizeInput {
            field_name: "data".to_string(),
            value: original.to_string(),
        }])
        .unwrap();

    assert!(
        !results[0].desensitized_value.contains(original),
        "哈希脱敏后不应包含原值"
    );
    assert_ne!(
        results[0].desensitized_value, original,
        "脱敏后值应不同于原值"
    );
}
// ============================================================================
// T4.2: 审计合规自动化（ComplianceAutoChecker 定期扫描 + 报告生成）
// ============================================================================

fn make_checker_with_violations() -> ComplianceAutoChecker {
    let checker = ComplianceAutoChecker::new(ComplianceAutoCheckConfig::default());
    checker.run_check(0.5, false, false, &["ssn".to_string(), "phone".to_string()]);
    checker
}

#[test]
fn test_scheduled_scan_detects_violations() {
    let checker = make_checker_with_violations();
    let result = checker
        .scheduled_scan(std::time::Duration::from_secs(86400))
        .unwrap();

    assert!(
        !result.violations.is_empty(),
        "定期扫描应检测到违规"
    );
    assert!(
        result.compliance_score < 1.0,
        "存在违规时合规分数应 < 1.0"
    );
}

#[test]
fn test_generate_compliance_report_includes_score() {
    let checker = make_checker_with_violations();
    let report = checker.generate_compliance_report();

    assert!(
        (0.0..=1.0).contains(&report.score),
        "合规分数应在 [0.0, 1.0] 范围内"
    );
    assert!(
        report.score < 1.0,
        "存在违规时分数应 < 1.0"
    );
}

#[test]
fn test_generate_compliance_report_includes_violations() {
    let checker = make_checker_with_violations();
    let report = checker.generate_compliance_report();

    assert!(
        !report.violations.is_empty(),
        "合规报告应包含违规清单"
    );
    assert!(
        !report.remediation_suggestions.is_empty(),
        "合规报告应包含整改建议"
    );
}

#[test]
fn test_violation_alert_generates_alert() {
    let checker = ComplianceAutoChecker::new(ComplianceAutoCheckConfig::default());
    let violation = Violation {
        id: "V-1".to_string(),
        description: "field 'ssn' not encrypted".to_string(),
        severity: ViolationSeverity::High,
    };

    let alert = checker.violation_alert(&violation);
    assert_eq!(alert.level, AlertLevel::High);
    assert!(alert.message.contains("ssn"), "告警消息应包含违规描述");
    assert_eq!(alert.violation_id, "V-1");
}

#[test]
fn test_remediation_suggest_generates_suggestions() {
    let checker = ComplianceAutoChecker::new(ComplianceAutoCheckConfig::default());

    let violation = Violation {
        id: "V-1".to_string(),
        description: "field 'ssn' not encrypted".to_string(),
        severity: ViolationSeverity::High,
    };

    let suggestions = checker.remediation_suggest(&violation);
    assert!(!suggestions.is_empty(), "应生成整改建议");
    assert!(
        suggestions.iter().any(|s| s.action.contains("加密")),
        "应包含加密相关整改建议"
    );
}
// ============================================================================
// T4.4: GDPR/CCPA 合规（GdprCcpaProcessor 数据主体权利 + 跨境传输）
// ============================================================================

fn make_processor() -> GdprCcpaProcessor {
    let transfer_control = CrossBorderTransferControl::new(
        vec![ComplianceRegion::Eu, ComplianceRegion::Usa],
        vec![ComplianceRegion::Other],
    );
    GdprCcpaProcessor::new(transfer_control)
}

#[tokio::test]
async fn test_execute_right_access_returns_data() {
    let processor = make_processor();
    processor.register_subject("user_001", "name=Alice,age=30,email=alice@example.com");

    let result = processor
        .execute_right(DataSubjectRight::Access {
            subject_id: "user_001".to_string(),
        })
        .await
        .expect("Access right should succeed");

    assert_eq!(result.subject_id, "user_001");
    assert!(
        result.data.as_ref().unwrap().contains("Alice"),
        "Access right should return subject data"
    );
    assert!(result.audit_preserved);
}

#[tokio::test]
async fn test_execute_right_portability_exports_data() {
    let processor = make_processor();
    processor.register_subject("user_002", "name=Bob,age=25");

    let result = processor
        .execute_right(DataSubjectRight::Portability {
            subject_id: "user_002".to_string(),
            format: DataFormat::Json,
        })
        .await
        .expect("Portability right should succeed");

    assert_eq!(result.format, Some(DataFormat::Json));
    let exported = result.data.as_ref().unwrap();
    assert!(
        exported.contains("user_002"),
        "Exported JSON should contain subject_id"
    );
    assert!(
        exported.contains("Bob"),
        "Exported JSON should contain subject data"
    );
    assert!(exported.starts_with('{'), "JSON format should start with {{");
}

#[tokio::test]
async fn test_execute_right_deletion_deletes_data() {
    let processor = make_processor();
    processor.register_subject("user_003", "name=Charlie,age=40");

    let result = processor
        .execute_right(DataSubjectRight::Deletion {
            subject_id: "user_003".to_string(),
        })
        .await
        .expect("Deletion right should succeed");

    assert_eq!(result.affected_records, 1);
    assert!(result.audit_preserved, "Audit should be preserved on deletion");
    assert!(
        processor.get_subject_data("user_003").is_none(),
        "Business data should be deleted after Deletion right"
    );
}

#[tokio::test]
async fn test_execute_right_correction_updates_data() {
    let processor = make_processor();
    processor.register_subject("user_004", "name=Dave,age=35");

    let mut corrections = std::collections::HashMap::new();
    corrections.insert("age".to_string(), "36".to_string());

    let result = processor
        .execute_right(DataSubjectRight::Correction {
            subject_id: "user_004".to_string(),
            corrections,
        })
        .await
        .expect("Correction right should succeed");

    assert_eq!(result.affected_records, 1);
    let updated = result.data.as_ref().unwrap();
    assert!(
        updated.contains("age=36"),
        "Corrected data should contain updated field"
    );
    let current = processor.get_subject_data("user_004").unwrap();
    assert!(
        current.contains("age=36"),
        "Stored data should reflect correction"
    );
}

#[test]
fn test_check_transfer_allowed_region() {
    let processor = make_processor();

    let allowed = processor
        .check_transfer(ComplianceRegion::Eu)
        .expect("EU should not error");
    assert!(allowed, "EU should be in allowed regions");

    let allowed_usa = processor
        .check_transfer(ComplianceRegion::Usa)
        .expect("USA should not error");
    assert!(allowed_usa, "USA should be in allowed regions");
}

#[test]
fn test_check_transfer_denied_region() {
    let processor = make_processor();

    let result = processor.check_transfer(ComplianceRegion::Other);
    assert!(
        result.is_err(),
        "Other region should be denied"
    );
}

#[tokio::test]
async fn test_gdpr_deletion_preserves_audit_record() {
    let processor = make_processor();
    processor.register_subject("user_005", "name=Eve,ssn=123-45-6789");

    let result = processor
        .execute_right(DataSubjectRight::Deletion {
            subject_id: "user_005".to_string(),
        })
        .await
        .expect("Deletion should succeed");

    assert!(
        result.audit_preserved,
        "Deletion result must indicate audit preserved"
    );
    assert_eq!(
        result.affected_records, 1,
        "Deletion must affect exactly 1 record"
    );

    let audit = processor.audit_records();
    assert!(
        !audit.is_empty(),
        "Audit records should be preserved after deletion"
    );
    let record = &audit[0];
    assert!(
        record.contains("DELETION"),
        "Audit record should note deletion action"
    );
    assert!(
        !record.contains("user_005"),
        "Audit record should NOT contain raw subject_id (must be desensitized)"
    );
    assert!(
        record.contains("us*****"),
        "Audit record should contain masked subject_id"
    );
}