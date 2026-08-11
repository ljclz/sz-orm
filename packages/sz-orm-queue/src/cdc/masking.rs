//! ChangeEvent 脱敏：对 before/after 敏感字段应用 DataMasker 脱敏

use std::collections::HashMap;

use serde_json::Value;

use super::{ChangeEvent, MaskingRule, MaskingRuleMap};
use sz_orm_masking::DataMasker;

/// 对 ChangeEvent 应用脱敏规则
pub fn apply_masking(event: &mut ChangeEvent, rules: &MaskingRuleMap) {
    if let Some(before) = &mut event.before {
        mask_row(before, rules);
    }
    if let Some(after) = &mut event.after {
        mask_row(after, rules);
    }
}

/// 对行数据应用脱敏
fn mask_row(row: &mut HashMap<String, Value>, rules: &MaskingRuleMap) {
    for (field, rule) in rules {
        if let Some(Value::String(s)) = row.get(field) {
            let masked = DataMasker::apply(rule, s);
            row.insert(field.clone(), Value::String(masked));
        }
    }
}

/// 构建脱敏规则映射
pub fn build_masking_rules() -> MaskingRuleMap {
    let mut rules = HashMap::new();
    rules.insert("phone".to_string(), MaskingRule::Phone);
    rules.insert("email".to_string(), MaskingRule::Email);
    rules.insert("id_card".to_string(), MaskingRule::IdCard);
    rules.insert("bank_card".to_string(), MaskingRule::BankCard);
    rules.insert("name".to_string(), MaskingRule::Name);
    rules.insert("address".to_string(), MaskingRule::Address);
    rules.insert("password".to_string(), MaskingRule::Password);
    rules.insert("api_key".to_string(), MaskingRule::ApiKey);
    rules
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cdc::ChangeOp;
    use std::collections::HashMap;

    fn make_event_with_phone(phone: &str) -> ChangeEvent {
        let mut after = HashMap::new();
        after.insert("phone".to_string(), Value::String(phone.to_string()));
        after.insert("name".to_string(), Value::String("张三".to_string()));
        ChangeEvent {
            op: ChangeOp::Insert,
            before: None,
            after: Some(after),
            timestamp: 0,
            transaction_id: "tx-001".to_string(),
            table: "users".to_string(),
            schema: "public".to_string(),
        }
    }

    #[test]
    fn test_apply_masking_phone() {
        let mut event = make_event_with_phone("13812348888");
        let mut rules = HashMap::new();
        rules.insert("phone".to_string(), MaskingRule::Phone);

        apply_masking(&mut event, &rules);

        let after = event.after.unwrap();
        let phone = after.get("phone").unwrap();
        assert_eq!(phone, &Value::String("138****8888".to_string()));
    }

    #[test]
    fn test_apply_masking_email() {
        let mut after = HashMap::new();
        after.insert(
            "email".to_string(),
            Value::String("user@example.com".to_string()),
        );
        let mut event = ChangeEvent {
            op: ChangeOp::Update,
            before: None,
            after: Some(after),
            timestamp: 0,
            transaction_id: "tx-001".to_string(),
            table: "users".to_string(),
            schema: "public".to_string(),
        };

        let mut rules = HashMap::new();
        rules.insert("email".to_string(), MaskingRule::Email);

        apply_masking(&mut event, &rules);

        let after = event.after.unwrap();
        let email = after.get("email").unwrap();
        if let Value::String(s) = email {
            assert!(s.contains("*"));
        } else {
            panic!("expected string");
        }
    }

    #[test]
    fn test_apply_masking_multiple_fields() {
        let mut event = make_event_with_phone("13812348888");
        let rules = build_masking_rules();

        apply_masking(&mut event, &rules);

        let after = event.after.unwrap();
        let phone = after.get("phone").unwrap();
        assert_eq!(phone, &Value::String("138****8888".to_string()));
        let name = after.get("name").unwrap();
        if let Value::String(s) = name {
            assert!(s.contains('*'));
        } else {
            panic!("expected string");
        }
    }

    #[test]
    fn test_apply_masking_before_and_after() {
        let mut before = HashMap::new();
        before.insert(
            "phone".to_string(),
            Value::String("13812348888".to_string()),
        );
        let mut after = HashMap::new();
        after.insert(
            "phone".to_string(),
            Value::String("13912347777".to_string()),
        );

        let mut event = ChangeEvent {
            op: ChangeOp::Update,
            before: Some(before),
            after: Some(after),
            timestamp: 0,
            transaction_id: "tx-001".to_string(),
            table: "users".to_string(),
            schema: "public".to_string(),
        };

        let mut rules = HashMap::new();
        rules.insert("phone".to_string(), MaskingRule::Phone);

        apply_masking(&mut event, &rules);

        let before = event.before.unwrap();
        assert_eq!(
            before.get("phone").unwrap(),
            &Value::String("138****8888".to_string())
        );
        let after = event.after.unwrap();
        assert_eq!(
            after.get("phone").unwrap(),
            &Value::String("139****7777".to_string())
        );
    }

    #[test]
    fn test_apply_masking_no_rules() {
        let mut event = make_event_with_phone("13812348888");
        let rules = HashMap::new();
        apply_masking(&mut event, &rules);
        let after = event.after.unwrap();
        assert_eq!(
            after.get("phone").unwrap(),
            &Value::String("13812348888".to_string())
        );
    }

    #[test]
    fn test_apply_masking_password() {
        let mut after = HashMap::new();
        after.insert(
            "password".to_string(),
            Value::String("secret123".to_string()),
        );
        let mut event = ChangeEvent {
            op: ChangeOp::Insert,
            before: None,
            after: Some(after),
            timestamp: 0,
            transaction_id: "tx-001".to_string(),
            table: "users".to_string(),
            schema: "public".to_string(),
        };

        let mut rules = HashMap::new();
        rules.insert("password".to_string(), MaskingRule::Password);

        apply_masking(&mut event, &rules);

        let after = event.after.unwrap();
        assert_eq!(
            after.get("password").unwrap(),
            &Value::String("***".to_string())
        );
    }

    #[test]
    fn test_apply_masking_non_string_value() {
        let mut after = HashMap::new();
        after.insert("age".to_string(), Value::Number(30.into()));
        let mut event = ChangeEvent {
            op: ChangeOp::Insert,
            before: None,
            after: Some(after),
            timestamp: 0,
            transaction_id: "tx-001".to_string(),
            table: "users".to_string(),
            schema: "public".to_string(),
        };

        let mut rules = HashMap::new();
        rules.insert("age".to_string(), MaskingRule::Phone);

        apply_masking(&mut event, &rules);

        let after = event.after.unwrap();
        assert_eq!(after.get("age").unwrap(), &Value::Number(30.into()));
    }

    #[test]
    fn test_build_masking_rules() {
        let rules = build_masking_rules();
        assert_eq!(rules.len(), 8);
        assert!(rules.contains_key("phone"));
        assert!(rules.contains_key("email"));
        assert!(rules.contains_key("password"));
    }
}
