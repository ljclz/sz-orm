//! # SZ-ORM Masking — 数据脱敏
//!
//! 提供手机号、邮箱、身份证、银行卡、姓名、地址等敏感字段脱敏，并支持自定义
//! 前缀/后缀保留规则。实现 Unicode 安全，对短输入有合理兜底，不会 panic。
//!
//! ## 主要类型
//!
//! - [`MaskingRule`] — 脱敏规则枚举
//! - [`DataMasker`] — 脱敏执行器

use serde::{Deserialize, Serialize};

/// Masking rules supported by [`DataMasker`].
///
/// `Custom(String)` expects a configuration of the form `"prefix,suffix"`
/// where `prefix` and `suffix` are the number of characters (Unicode scalar
/// values) to retain from the start and end of the input. Example:
/// `Custom("3,2".to_string())` keeps the first 3 and last 2 characters and
/// replaces everything in between with `*`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MaskingRule {
    Phone,
    Email,
    IdCard,
    BankCard,
    Name,
    Address,
    Custom(String),
}

pub struct DataMasker;

impl DataMasker {
    /// Applies the given masking `rule` to `value`. The implementation is
    /// Unicode-safe (works on `char` boundaries rather than byte slices) and
    /// never panics: inputs shorter than the rule's required visible prefix
    /// return a sensible fallback (the original value, or `"***"` when even
    /// the original cannot be safely revealed).
    pub fn apply(rule: &MaskingRule, value: &str) -> String {
        match rule {
            MaskingRule::Phone => mask_prefix_suffix(value, 3, 4),
            MaskingRule::Email => mask_email(value),
            MaskingRule::IdCard => mask_prefix_suffix(value, 4, 4),
            MaskingRule::BankCard => mask_prefix_suffix(value, 4, 4),
            MaskingRule::Name => mask_name(value),
            MaskingRule::Address => mask_address(value, 6),
            MaskingRule::Custom(spec) => mask_custom(value, spec),
        }
    }
}

/// Masks the middle of the input, keeping the first `prefix` and last
/// `suffix` characters visible. Returns `"***"` when the input is too short
/// to reveal `prefix + suffix` characters (or when `prefix`/`suffix` are
/// zero, the rule degrades gracefully).
fn mask_prefix_suffix(value: &str, prefix: usize, suffix: usize) -> String {
    let chars: Vec<char> = value.chars().collect();
    let len = chars.len();
    if len == 0 {
        return "***".to_string();
    }
    // Need at least one extra char beyond prefix+suffix to mask; otherwise
    // the value has nothing to hide and we return the original.
    if len <= prefix + suffix {
        // Too short to safely mask without revealing the structure; return "***".
        return "***".to_string();
    }
    let hidden = len - prefix - suffix;
    let mut out = String::with_capacity(len);
    for &c in &chars[..prefix] {
        out.push(c);
    }
    for _ in 0..hidden {
        out.push('*');
    }
    for &c in &chars[len - suffix..] {
        out.push(c);
    }
    out
}

fn mask_email(value: &str) -> String {
    let parts: Vec<&str> = value.splitn(2, '@').collect();
    if parts.len() != 2 {
        // Not a valid email; do not attempt to mask structurally.
        return "***".to_string();
    }
    let local = parts[0];
    let domain = parts[1];
    let local_chars: Vec<char> = local.chars().collect();
    if local_chars.is_empty() {
        return "***".to_string();
    }
    let mut out = String::with_capacity(value.len());
    out.push(local_chars[0]);
    // Hide the rest of the local part with one `*` per hidden character.
    for _ in 1..local_chars.len() {
        out.push('*');
    }
    out.push('@');
    out.push_str(domain);
    out
}

fn mask_name(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    if chars.is_empty() {
        return String::new();
    }
    let mut out = String::with_capacity(chars.len());
    out.push(chars[0]);
    for _ in 1..chars.len() {
        out.push('*');
    }
    out
}

fn mask_address(value: &str, keep: usize) -> String {
    let chars: Vec<char> = value.chars().collect();
    if chars.is_empty() {
        return String::new();
    }
    if chars.len() <= keep {
        // Nothing meaningful to mask: hide everything to avoid leaking
        // the structure of very short addresses.
        return "*".repeat(chars.len());
    }
    let hidden = chars.len() - keep;
    let mut out = String::with_capacity(chars.len());
    for &c in &chars[..keep] {
        out.push(c);
    }
    for _ in 0..hidden {
        out.push('*');
    }
    out
}

fn mask_custom(value: &str, spec: &str) -> String {
    let (prefix, suffix) = match parse_custom_spec(spec) {
        Some(parsed) => parsed,
        None => return "***".to_string(),
    };
    mask_prefix_suffix(value, prefix, suffix)
}

/// Parses a `"prefix,suffix"` spec into `(prefix, suffix)`. Returns `None`
/// on malformed input or negative/overflowing values.
fn parse_custom_spec(spec: &str) -> Option<(usize, usize)> {
    let parts: Vec<&str> = spec.split(',').collect();
    if parts.len() != 2 {
        return None;
    }
    let prefix: usize = parts[0].trim().parse().ok()?;
    let suffix: usize = parts[1].trim().parse().ok()?;
    Some((prefix, suffix))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ----- Phone -----
    #[test]
    fn test_phone_standard() {
        let result = DataMasker::apply(&MaskingRule::Phone, "13812345678");
        assert_eq!(result, "138****5678");
    }

    #[test]
    fn test_phone_too_short() {
        // Less than 3+4 chars -> cannot safely reveal structure -> "***"
        assert_eq!(DataMasker::apply(&MaskingRule::Phone, "12345"), "***");
        assert_eq!(DataMasker::apply(&MaskingRule::Phone, "1234567"), "***");
    }

    #[test]
    fn test_phone_boundary_seven_plus_one() {
        // 8 chars: prefix=3, suffix=4, hidden=1
        assert_eq!(
            DataMasker::apply(&MaskingRule::Phone, "12345678"),
            "123*5678"
        );
    }

    #[test]
    fn test_phone_empty() {
        assert_eq!(DataMasker::apply(&MaskingRule::Phone, ""), "***");
    }

    // ----- Email -----
    #[test]
    fn test_email_standard() {
        assert_eq!(
            DataMasker::apply(&MaskingRule::Email, "test@example.com"),
            "t***@example.com"
        );
    }

    #[test]
    fn test_email_single_char_local() {
        assert_eq!(
            DataMasker::apply(&MaskingRule::Email, "a@example.com"),
            "a@example.com"
        );
    }

    #[test]
    fn test_email_no_at() {
        assert_eq!(DataMasker::apply(&MaskingRule::Email, "notanemail"), "***");
    }

    #[test]
    fn test_email_empty_local() {
        assert_eq!(
            DataMasker::apply(&MaskingRule::Email, "@example.com"),
            "***"
        );
    }

    // ----- IdCard -----
    #[test]
    fn test_idcard_standard_18() {
        let id = "110101199001012345";
        let masked = DataMasker::apply(&MaskingRule::IdCard, id);
        // First 4 + 10 stars + last 4 ("2345").
        assert_eq!(masked, "1101**********2345");
        assert_eq!(masked.len(), id.len());
    }

    #[test]
    fn test_idcard_too_short() {
        assert_eq!(DataMasker::apply(&MaskingRule::IdCard, "1234567"), "***");
        assert_eq!(DataMasker::apply(&MaskingRule::IdCard, "12345678"), "***");
    }

    #[test]
    fn test_idcard_empty() {
        assert_eq!(DataMasker::apply(&MaskingRule::IdCard, ""), "***");
    }

    // ----- BankCard -----
    #[test]
    fn test_bankcard_standard_16() {
        let card = "6222020200112345";
        let masked = DataMasker::apply(&MaskingRule::BankCard, card);
        assert_eq!(masked, "6222********2345");
    }

    #[test]
    fn test_bankcard_too_short() {
        assert_eq!(DataMasker::apply(&MaskingRule::BankCard, "1234567"), "***");
    }

    #[test]
    fn test_bankcard_empty() {
        assert_eq!(DataMasker::apply(&MaskingRule::BankCard, ""), "***");
    }

    // ----- Name -----
    #[test]
    fn test_name_chinese_two_chars() {
        assert_eq!(DataMasker::apply(&MaskingRule::Name, "张三"), "张*");
    }

    #[test]
    fn test_name_chinese_three_chars() {
        assert_eq!(DataMasker::apply(&MaskingRule::Name, "诸葛亮"), "诸**");
    }

    #[test]
    fn test_name_single_char() {
        assert_eq!(DataMasker::apply(&MaskingRule::Name, "李"), "李");
    }

    #[test]
    fn test_name_empty() {
        assert_eq!(DataMasker::apply(&MaskingRule::Name, ""), "");
    }

    #[test]
    fn test_name_english() {
        assert_eq!(DataMasker::apply(&MaskingRule::Name, "Alice"), "A****");
    }

    // ----- Address -----
    #[test]
    fn test_address_standard() {
        let addr = "北京市海淀区中关村大街1号";
        let masked = DataMasker::apply(&MaskingRule::Address, addr);
        // First 6 chars kept ("北京市海淀区"), the rest replaced with one `*` per char.
        let expected = "北京市海淀区*******";
        assert_eq!(masked, expected);
        assert_eq!(masked.chars().count(), addr.chars().count());
    }

    #[test]
    fn test_address_exactly_six_chars() {
        let addr = "北京市海淀区";
        assert_eq!(DataMasker::apply(&MaskingRule::Address, addr), "******");
    }

    #[test]
    fn test_address_short() {
        assert_eq!(DataMasker::apply(&MaskingRule::Address, "北京"), "**");
    }

    #[test]
    fn test_address_empty() {
        assert_eq!(DataMasker::apply(&MaskingRule::Address, ""), "");
    }

    // ----- Custom -----
    #[test]
    fn test_custom_prefix_suffix() {
        let rule = MaskingRule::Custom("3,2".to_string());
        assert_eq!(DataMasker::apply(&rule, "ABCDEFGHIJ"), "ABC*****IJ");
    }

    #[test]
    fn test_custom_too_short() {
        let rule = MaskingRule::Custom("4,4".to_string());
        assert_eq!(DataMasker::apply(&rule, "ABC"), "***");
    }

    #[test]
    fn test_custom_invalid_spec() {
        let rule = MaskingRule::Custom("not_a_number".to_string());
        assert_eq!(DataMasker::apply(&rule, "ABCDEF"), "***");
    }

    #[test]
    fn test_custom_invalid_spec_two_parts() {
        let rule = MaskingRule::Custom("1,2,3".to_string());
        assert_eq!(DataMasker::apply(&rule, "ABCDEF"), "***");
    }

    #[test]
    fn test_custom_empty_value() {
        let rule = MaskingRule::Custom("2,2".to_string());
        assert_eq!(DataMasker::apply(&rule, ""), "***");
    }

    // ----- Unicode safety -----
    #[test]
    fn test_unicode_no_panic() {
        // Mixing CJK + emoji + ascii - just verify no panic and contains stars.
        let value = "你好🌍世界AB";
        let masked = DataMasker::apply(&MaskingRule::Address, value);
        assert!(masked.contains('*'));
    }

    #[test]
    fn test_long_string() {
        let value = "1".repeat(10000);
        let masked = DataMasker::apply(&MaskingRule::Phone, &value);
        // Should start with first 3, end with last 4, all stars in between.
        assert!(masked.starts_with("111"));
        assert!(masked.ends_with("1111"));
        assert_eq!(masked.matches('*').count(), 10000 - 7);
    }

    #[test]
    fn test_single_char_inputs() {
        assert_eq!(DataMasker::apply(&MaskingRule::Phone, "1"), "***");
        assert_eq!(DataMasker::apply(&MaskingRule::IdCard, "1"), "***");
        assert_eq!(DataMasker::apply(&MaskingRule::BankCard, "1"), "***");
        assert_eq!(DataMasker::apply(&MaskingRule::Name, "张"), "张");
        assert_eq!(DataMasker::apply(&MaskingRule::Address, "张"), "*");
    }
}
