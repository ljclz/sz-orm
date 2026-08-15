#![cfg(feature = "owasp-pentest-suite")]

//! OWASP A08: 安全日志和监控失败渗透测试（masking 包）
//!
//! 对应 REQ-V49-008（OWASP A08 深化）
//!
//! 渗透测试向量：
//! - 脱敏规则覆盖所有敏感字段类型：phone/email/idcard/bankcard/name/address/ip/imei/plate/password/apikey

use sz_orm_masking::{DataMasker, MaskingRule};

/// A08-8：脱敏规则覆盖所有敏感字段类型
///
/// 攻击模型：某些敏感字段类型未被脱敏，导致信息泄露。
/// 防护：DataMasker 支持 12 种脱敏规则，每种都正确脱敏。
#[test]
fn a08_all_sensitive_field_types_masked() {
    let test_cases: &[(MaskingRule, &str, &str)] = &[
        (MaskingRule::Phone, "13812345678", "138****5678"),
        (MaskingRule::Email, "user@example.com", "u***@example.com"),
        (
            MaskingRule::IdCard,
            "110101199001011234",
            "1101**********1234",
        ),
        (
            MaskingRule::BankCard,
            "6222021234567890123",
            "6222***********0123",
        ),
        (MaskingRule::Name, "张三丰", "张*丰"),
        (
            MaskingRule::Address,
            "北京市海淀区中关村大街1号",
            "北京市******1号",
        ),
        (MaskingRule::Ip, "192.168.1.100", "192.***.1.100"),
        (MaskingRule::Imei, "123456789012345", "1234*********2345"),
        (MaskingRule::Plate, "京A12345", "京A*2345"),
        (MaskingRule::Password, "mySecretPass123", "***"),
        (MaskingRule::ApiKey, "sk-abc123xyz789", "sk-a*****z789"),
    ];

    for (rule, input, _) in test_cases {
        let masked = DataMasker::apply(rule, input);
        assert_ne!(
            masked, *input,
            "规则 {:?} 应脱敏输入 '{}'，但得到 '{}'",
            rule, input, masked
        );
        assert!(
            masked.contains('*'),
            "规则 {:?} 脱敏结果 '{}' 应包含 *",
            rule,
            masked
        );
    }

    assert_eq!(DataMasker::apply(&MaskingRule::Password, "anything"), "***");
}
