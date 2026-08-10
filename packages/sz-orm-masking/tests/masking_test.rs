use sz_orm_masking::{DataMasker, MaskingRule};

#[test]
fn test_mask_phone() {
    let result = DataMasker::apply(&MaskingRule::Phone, "13812345678");
    assert!(result.starts_with("138"));
    assert!(result.ends_with("5678"));
    assert!(result.contains('*'));
}

#[test]
fn test_mask_phone_short() {
    let result = DataMasker::apply(&MaskingRule::Phone, "123");
    assert_eq!(result, "***");
}

#[test]
fn test_mask_phone_empty() {
    let result = DataMasker::apply(&MaskingRule::Phone, "");
    assert_eq!(result, "***");
}

#[test]
fn test_mask_email() {
    let result = DataMasker::apply(&MaskingRule::Email, "user@example.com");
    assert!(result.contains('*'));
    assert!(result.contains('@'));
}

#[test]
fn test_mask_email_short() {
    let result = DataMasker::apply(&MaskingRule::Email, "a@b");
    assert!(!result.is_empty());
}

#[test]
fn test_mask_id_card() {
    let result = DataMasker::apply(&MaskingRule::IdCard, "110101199001011234");
    assert!(result.starts_with("1101"));
    assert!(result.ends_with("1234"));
    assert!(result.contains('*'));
}

#[test]
fn test_mask_bank_card() {
    let result = DataMasker::apply(&MaskingRule::BankCard, "6222021234567890123");
    assert!(result.starts_with("6222"));
    assert!(result.ends_with("0123"));
    assert!(result.contains('*'));
}

#[test]
fn test_mask_name() {
    let result = DataMasker::apply(&MaskingRule::Name, "张三丰");
    assert!(result.contains('*'));
}

#[test]
fn test_mask_name_single_char() {
    let result = DataMasker::apply(&MaskingRule::Name, "张");
    assert!(!result.is_empty());
}

#[test]
fn test_mask_address() {
    let result = DataMasker::apply(&MaskingRule::Address, "北京市海淀区中关村大街1号");
    assert!(result.contains('*'));
}

#[test]
fn test_mask_ip() {
    let result = DataMasker::apply(&MaskingRule::Ip, "192.168.1.100");
    assert!(result.contains('*'));
}

#[test]
fn test_mask_imei() {
    let result = DataMasker::apply(&MaskingRule::Imei, "123456789012345");
    assert!(result.contains('*'));
}

#[test]
fn test_mask_plate() {
    let result = DataMasker::apply(&MaskingRule::Plate, "京A12345");
    assert!(result.contains('*'));
}

#[test]
fn test_mask_custom() {
    let result = DataMasker::apply(&MaskingRule::Custom("3,2".to_string()), "abcdefghij");
    assert!(result.starts_with("abc"));
    assert!(result.ends_with("ij"));
    assert!(result.contains('*'));
}

#[test]
fn test_mask_custom_short() {
    let result = DataMasker::apply(&MaskingRule::Custom("3,2".to_string()), "abc");
    assert_eq!(result, "***");
}

#[test]
fn test_mask_unicode_safe() {
    let result = DataMasker::apply(&MaskingRule::Phone, "你好世界你好世界");
    assert!(!result.is_empty());
}

#[test]
fn test_mask_rule_serialization() {
    let rule = MaskingRule::Phone;
    let json = serde_json::to_string(&rule).unwrap();
    let deserialized: MaskingRule = serde_json::from_str(&json).unwrap();
    match deserialized {
        MaskingRule::Phone => {}
        _ => panic!("Expected Phone"),
    }
}
