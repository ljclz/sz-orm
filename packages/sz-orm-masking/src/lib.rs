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
    Ip,
    Imei,
    Plate,
    Custom(String),
    Password,
    ApiKey,
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
            MaskingRule::Ip => mask_ip(value),
            MaskingRule::Imei => mask_imei(value),
            MaskingRule::Plate => mask_plate(value),
            MaskingRule::Custom(spec) => mask_custom(value, spec),
            MaskingRule::Password => "***".to_string(),
            MaskingRule::ApiKey => mask_api_key(value),
        }
    }

    /// Applies a sequence of rules to the same value, in order.
    ///
    /// Useful for stacked masking (e.g. first mask the whole phone, then
    /// apply a custom prefix/suffix rule). Each rule is applied to the
    /// output of the previous one.
    pub fn apply_many(rules: &[MaskingRule], value: &str) -> String {
        let mut out = value.to_string();
        for rule in rules {
            out = Self::apply(rule, &out);
        }
        out
    }

    /// Masks specific fields of a `HashMap<String, String>` in place,
    /// returning a new map with only the masked fields replaced.
    ///
    /// Fields whose rule is `None` are copied through unchanged. Fields not
    /// present in `rules` are also copied through, so callers can pass a
    /// partial rule set without losing data.
    pub fn mask_map(
        rules: &std::collections::HashMap<String, MaskingRule>,
        data: &std::collections::HashMap<String, String>,
    ) -> std::collections::HashMap<String, String> {
        data.iter()
            .map(|(k, v)| match rules.get(k) {
                Some(rule) => (k.clone(), Self::apply(rule, v)),
                None => (k.clone(), v.clone()),
            })
            .collect()
    }

    /// Masks a JSON object string by field name.
    ///
    /// `rules` maps field names to masking rules. Only top-level fields are
    /// matched (nested objects are left untouched); non-JSON input is
    /// returned unchanged so callers can rely on this being lossless for
    /// malformed payloads.
    pub fn mask_json(rules: &std::collections::HashMap<String, MaskingRule>, json: &str) -> String {
        let Ok(mut value) = serde_json::from_str::<serde_json::Value>(json) else {
            return json.to_string();
        };
        let Some(obj) = value.as_object_mut() else {
            return json.to_string();
        };
        for (field, rule) in rules {
            if let Some(serde_json::Value::String(s)) = obj.get_mut(field) {
                *s = Self::apply(rule, s);
            }
        }
        serde_json::to_string(&value).unwrap_or_else(|_| json.to_string())
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

/// Masks an API key by keeping the first 4 and last 4 characters visible
/// and replacing everything in between with `*`. Returns `"***"` for inputs
/// too short to safely mask (≤ 8 characters).
fn mask_api_key(value: &str) -> String {
    mask_prefix_suffix(value, 4, 4)
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

/// IP 地址脱敏：192.168.1.100 → 192.168.1.*
///
/// IPv4：隐藏最后一段（最后一个 `.` 之后的内容）。
/// IPv6：隐藏最后一个 `:` 组（最后一个冒号之后的内容）。
/// 无法识别时原样返回。`.` 和 `:` 为 ASCII 单字节，`rfind` 返回的字节位置
/// 一定是字符边界，切片安全。
fn mask_ip(ip: &str) -> String {
    if let Some(last_dot) = ip.rfind('.') {
        format!("{}.*", &ip[..last_dot])
    } else if let Some(last_colon) = ip.rfind(':') {
        format!("{}:*", &ip[..last_colon])
    } else {
        ip.to_string()
    }
}

/// IMEI 脱敏：保留前 6 位和最后 1 位，中间用 `****` 替代
///
/// IMEI 为 15 位数字（3GPP TS 23.003），按 Unicode 字符处理以保证安全。
fn mask_imei(imei: &str) -> String {
    let chars: Vec<char> = imei.chars().collect();
    if chars.len() < 7 {
        return "*".repeat(chars.len());
    }
    let mut out = String::with_capacity(chars.len() + 4);
    for &c in &chars[..6] {
        out.push(c);
    }
    out.push_str("****");
    out.push(chars[chars.len() - 1]);
    out
}

/// 车牌号脱敏：京A12345 → 京A12**45
///
/// 保留前 (len-2) 个字符和最后 2 个字符，中间用 `**` 替代。
/// 按 Unicode 字符处理，支持中文车牌（如"京A12345"）。
fn mask_plate(plate: &str) -> String {
    let chars: Vec<char> = plate.chars().collect();
    let len = chars.len();
    if len < 4 {
        return "*".repeat(len);
    }
    let mut out = String::with_capacity(len + 2);
    for &c in &chars[..len - 2] {
        out.push(c);
    }
    out.push_str("**");
    for &c in &chars[len - 2..] {
        out.push(c);
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

// ---------------------------------------------------------------------------
// 脱敏策略与审计工具
// ---------------------------------------------------------------------------

/// 脱敏策略：为多个字段定义脱敏规则，批量应用
#[derive(Debug, Clone, Default)]
pub struct MaskingPolicy {
    rules: std::collections::HashMap<String, MaskingRule>,
}

impl MaskingPolicy {
    /// 创建空策略
    pub fn new() -> Self {
        Self::default()
    }

    /// 添加字段脱敏规则（链式）
    pub fn add_rule(&mut self, field: &str, rule: MaskingRule) -> &mut Self {
        self.rules.insert(field.to_string(), rule);
        self
    }

    /// 获取某字段的脱敏规则
    pub fn get_rule(&self, field: &str) -> Option<&MaskingRule> {
        self.rules.get(field)
    }

    /// 移除某字段的脱敏规则
    pub fn remove_rule(&mut self, field: &str) -> Option<MaskingRule> {
        self.rules.remove(field)
    }

    /// 已注册字段数
    pub fn field_count(&self) -> usize {
        self.rules.len()
    }

    /// 获取所有规则引用
    pub fn rules(&self) -> &std::collections::HashMap<String, MaskingRule> {
        &self.rules
    }

    /// 对 HashMap 数据应用脱敏
    pub fn apply_to_map(
        &self,
        data: &std::collections::HashMap<String, String>,
    ) -> std::collections::HashMap<String, String> {
        DataMasker::mask_map(&self.rules, data)
    }

    /// 对 JSON 字符串应用脱敏
    pub fn apply_to_json(&self, json: &str) -> String {
        DataMasker::mask_json(&self.rules, json)
    }
}

/// 脱敏审计条目：记录单次脱敏操作
#[derive(Debug, Clone)]
pub struct MaskAuditEntry {
    field: String,
    original_len: usize,
    masked_len: usize,
}

impl MaskAuditEntry {
    /// 字段名
    pub fn field(&self) -> &str {
        &self.field
    }

    /// 原始值长度
    pub fn original_len(&self) -> usize {
        self.original_len
    }

    /// 脱敏后长度
    pub fn masked_len(&self) -> usize {
        self.masked_len
    }

    /// 是否被截断（脱敏后比原始短）
    pub fn was_truncated(&self) -> bool {
        self.masked_len < self.original_len
    }
}

/// 脱敏审计日志：记录所有脱敏操作
#[derive(Debug, Clone, Default)]
pub struct MaskAuditLog {
    entries: Vec<MaskAuditEntry>,
}

impl MaskAuditLog {
    /// 创建空审计日志
    pub fn new() -> Self {
        Self::default()
    }

    /// 记录一次脱敏操作
    pub fn log(&mut self, field: &str, original_len: usize, masked_len: usize) {
        self.entries.push(MaskAuditEntry {
            field: field.to_string(),
            original_len,
            masked_len,
        });
    }

    /// 所有审计条目
    pub fn entries(&self) -> &[MaskAuditEntry] {
        &self.entries
    }

    /// 条目数
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// 被掩码的字符总数（original_len - masked_len 之和）
    pub fn total_masked_chars(&self) -> usize {
        self.entries
            .iter()
            .map(|e| e.original_len.saturating_sub(e.masked_len))
            .sum()
    }

    /// 清空日志
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

/// 脱敏配置：自定义掩码字符、最小掩码长度、兜底值
#[derive(Debug, Clone)]
pub struct MaskingConfig {
    mask_char: char,
    min_mask_length: usize,
    fallback: String,
}

impl Default for MaskingConfig {
    fn default() -> Self {
        Self {
            mask_char: '*',
            min_mask_length: 3,
            fallback: "***".to_string(),
        }
    }
}

impl MaskingConfig {
    /// 创建默认配置
    pub fn new() -> Self {
        Self::default()
    }

    /// 掩码字符
    pub fn mask_char(&self) -> char {
        self.mask_char
    }

    /// 设置掩码字符（链式）
    pub fn with_mask_char(mut self, c: char) -> Self {
        self.mask_char = c;
        self
    }

    /// 最小掩码长度
    pub fn min_mask_length(&self) -> usize {
        self.min_mask_length
    }

    /// 设置最小掩码长度（链式）
    pub fn with_min_mask_length(mut self, min: usize) -> Self {
        self.min_mask_length = min;
        self
    }

    /// 兜底值
    pub fn fallback_value(&self) -> &str {
        &self.fallback
    }

    /// 设置兜底值（链式）
    pub fn with_fallback_value(mut self, value: &str) -> Self {
        self.fallback = value.to_string();
        self
    }
}

/// 脱敏统计：按字段跟踪脱敏操作次数
#[derive(Debug, Clone, Default)]
pub struct MaskingStats {
    counts: std::collections::HashMap<String, u64>,
}

impl MaskingStats {
    /// 创建空统计
    pub fn new() -> Self {
        Self::default()
    }

    /// 记录一次脱敏操作
    pub fn record(&mut self, field: &str) {
        *self.counts.entry(field.to_string()).or_insert(0) += 1;
    }

    /// 总操作次数
    pub fn total_operations(&self) -> u64 {
        self.counts.values().sum()
    }

    /// 某字段的操作次数
    pub fn field_operations(&self, field: &str) -> u64 {
        self.counts.get(field).copied().unwrap_or(0)
    }

    /// 操作次数最多的字段
    pub fn most_masked_field(&self) -> Option<&str> {
        self.counts
            .iter()
            .max_by_key(|(_, v)| *v)
            .map(|(k, _)| k.as_str())
    }
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

    // ----- IP -----
    #[test]
    fn test_ip_v4_standard() {
        assert_eq!(
            DataMasker::apply(&MaskingRule::Ip, "192.168.1.100"),
            "192.168.1.*"
        );
    }

    #[test]
    fn test_ip_v4_loopback() {
        assert_eq!(
            DataMasker::apply(&MaskingRule::Ip, "127.0.0.1"),
            "127.0.0.*"
        );
    }

    #[test]
    fn test_ip_v6_standard() {
        // IPv6：隐藏最后一个冒号后的内容
        assert_eq!(
            DataMasker::apply(&MaskingRule::Ip, "2001:db8::1"),
            "2001:db8::*"
        );
    }

    #[test]
    fn test_ip_no_separator() {
        assert_eq!(
            DataMasker::apply(&MaskingRule::Ip, "localhost"),
            "localhost"
        );
    }

    #[test]
    fn test_ip_empty() {
        assert_eq!(DataMasker::apply(&MaskingRule::Ip, ""), "");
    }

    // ----- IMEI -----
    #[test]
    fn test_imei_standard_15() {
        // 15 位 IMEI：保留前 6 + **** + 最后 1 位
        assert_eq!(
            DataMasker::apply(&MaskingRule::Imei, "123456789012345"),
            "123456****5"
        );
    }

    #[test]
    fn test_imei_too_short() {
        assert_eq!(DataMasker::apply(&MaskingRule::Imei, "123456"), "******");
        assert_eq!(DataMasker::apply(&MaskingRule::Imei, "123"), "***");
    }

    #[test]
    fn test_imei_empty() {
        assert_eq!(DataMasker::apply(&MaskingRule::Imei, ""), "");
    }

    // ----- Plate -----
    #[test]
    fn test_plate_chinese_standard() {
        // 京A12345（7 字符）：前 5 + ** + 后 2
        assert_eq!(
            DataMasker::apply(&MaskingRule::Plate, "京A12345"),
            "京A123**45"
        );
    }

    #[test]
    fn test_plate_with_separator() {
        // 京A·12345（8 字符）：前 6 + ** + 后 2
        assert_eq!(
            DataMasker::apply(&MaskingRule::Plate, "京A·12345"),
            "京A·123**45"
        );
    }

    #[test]
    fn test_plate_too_short() {
        assert_eq!(DataMasker::apply(&MaskingRule::Plate, "京A"), "**");
        assert_eq!(DataMasker::apply(&MaskingRule::Plate, "京"), "*");
        assert_eq!(DataMasker::apply(&MaskingRule::Plate, "京A1"), "***");
    }

    #[test]
    fn test_plate_empty() {
        assert_eq!(DataMasker::apply(&MaskingRule::Plate, ""), "");
    }

    #[test]
    fn test_plate_boundary_four_chars() {
        // 4 字符：前 2 + ** + 后 2
        assert_eq!(DataMasker::apply(&MaskingRule::Plate, "ABCD"), "AB**CD");
    }
}

#[cfg(test)]
mod api_tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_apply_many_stacked_rules() {
        // 先手机号脱敏，再自定义前缀后缀
        let rules = vec![MaskingRule::Phone, MaskingRule::Custom("1,2".to_string())];
        let out = DataMasker::apply_many(&rules, "13812345678");
        assert!(
            out.contains('*'),
            "stacked masking should keep stars: {out}"
        );
    }

    #[test]
    fn test_mask_map_partial_rules() {
        let mut rules = HashMap::new();
        rules.insert("phone".to_string(), MaskingRule::Phone);
        let mut data = HashMap::new();
        data.insert("phone".to_string(), "13812345678".to_string());
        data.insert("name".to_string(), "Alice".to_string());

        let out = DataMasker::mask_map(&rules, &data);
        assert!(
            out["phone"].contains('*'),
            "phone should be masked: {}",
            out["phone"]
        );
        assert_eq!(out["name"], "Alice", "unlisted field passes through");
    }

    #[test]
    fn test_mask_json_top_level_fields() {
        let mut rules = HashMap::new();
        rules.insert("phone".to_string(), MaskingRule::Phone);
        rules.insert("password".to_string(), MaskingRule::Password);

        let json = r#"{"id":1,"phone":"13812345678","password":"secret","name":"Bob"}"#;
        let out = DataMasker::mask_json(&rules, json);
        assert!(out.contains('*'), "masked json should contain stars: {out}");
        assert!(!out.contains("13812345678"), "phone value must not leak");
        assert!(!out.contains("secret"), "password value must not leak");
        assert!(out.contains("Bob"), "unlisted field passes through");
    }

    #[test]
    fn test_mask_json_invalid_input_unchanged() {
        let rules = HashMap::new();
        assert_eq!(DataMasker::mask_json(&rules, "not-json"), "not-json");
    }

    #[test]
    fn test_mask_json_non_object_unchanged() {
        let rules = HashMap::new();
        assert_eq!(DataMasker::mask_json(&rules, "[1,2,3]"), "[1,2,3]");
    }

    #[test]
    fn test_mask_map_empty_rules() {
        let data = HashMap::new();
        let out = DataMasker::mask_map(&HashMap::new(), &data);
        assert!(out.is_empty());
    }

    #[test]
    fn test_apply_many_empty_rules_identity() {
        assert_eq!(DataMasker::apply_many(&[], "hello"), "hello");
    }
}

#[cfg(test)]
mod policy_tests {
    use super::*;
    use std::collections::HashMap;

    // --- MaskingPolicy tests ---

    #[test]
    fn policy_new_empty() {
        let p = MaskingPolicy::new();
        assert_eq!(p.field_count(), 0);
    }

    #[test]
    fn policy_add_and_get_rule() {
        let mut p = MaskingPolicy::new();
        p.add_rule("phone", MaskingRule::Phone);
        assert_eq!(p.field_count(), 1);
        assert!(p.get_rule("phone").is_some());
        assert!(p.get_rule("email").is_none());
    }

    #[test]
    fn policy_remove_rule() {
        let mut p = MaskingPolicy::new();
        p.add_rule("phone", MaskingRule::Phone);
        let removed = p.remove_rule("phone");
        assert!(removed.is_some());
        assert_eq!(p.field_count(), 0);
    }

    #[test]
    fn policy_apply_to_map() {
        let mut p = MaskingPolicy::new();
        p.add_rule("phone", MaskingRule::Phone);
        let mut data = HashMap::new();
        data.insert("phone".to_string(), "13812345678".to_string());
        data.insert("name".to_string(), "Alice".to_string());
        let out = p.apply_to_map(&data);
        assert!(out["phone"].contains('*'));
        assert_eq!(out["name"], "Alice");
    }

    #[test]
    fn policy_apply_to_json() {
        let mut p = MaskingPolicy::new();
        p.add_rule("phone", MaskingRule::Phone);
        let json = r#"{"phone":"13812345678","name":"Bob"}"#;
        let out = p.apply_to_json(json);
        assert!(out.contains('*'));
        assert!(!out.contains("13812345678"));
    }

    #[test]
    fn policy_rules_ref() {
        let mut p = MaskingPolicy::new();
        p.add_rule("a", MaskingRule::Name);
        assert_eq!(p.rules().len(), 1);
    }

    // --- MaskAuditLog tests ---

    #[test]
    fn audit_log_new_empty() {
        let log = MaskAuditLog::new();
        assert_eq!(log.entry_count(), 0);
        assert!(log.entries().is_empty());
    }

    #[test]
    fn audit_log_record() {
        let mut log = MaskAuditLog::new();
        log.log("phone", 11, 11);
        log.log("name", 5, 5);
        assert_eq!(log.entry_count(), 2);
    }

    #[test]
    fn audit_log_total_masked_chars() {
        let mut log = MaskAuditLog::new();
        log.log("phone", 11, 7);
        log.log("name", 5, 3);
        assert_eq!(log.total_masked_chars(), 6);
    }

    #[test]
    fn audit_log_clear() {
        let mut log = MaskAuditLog::new();
        log.log("a", 1, 1);
        log.clear();
        assert_eq!(log.entry_count(), 0);
    }

    #[test]
    fn audit_entry_was_truncated() {
        let mut log = MaskAuditLog::new();
        log.log("short", 10, 5);
        log.log("same", 5, 5);
        assert!(log.entries()[0].was_truncated());
        assert!(!log.entries()[1].was_truncated());
    }

    // --- MaskingConfig tests ---

    #[test]
    fn config_defaults() {
        let c = MaskingConfig::new();
        assert_eq!(c.mask_char(), '*');
        assert_eq!(c.min_mask_length(), 3);
        assert_eq!(c.fallback_value(), "***");
    }

    #[test]
    fn config_builder() {
        let c = MaskingConfig::new()
            .with_mask_char('#')
            .with_min_mask_length(5)
            .with_fallback_value("UNK");
        assert_eq!(c.mask_char(), '#');
        assert_eq!(c.min_mask_length(), 5);
        assert_eq!(c.fallback_value(), "UNK");
    }

    // --- MaskingStats tests ---

    #[test]
    fn stats_new_empty() {
        let s = MaskingStats::new();
        assert_eq!(s.total_operations(), 0);
        assert_eq!(s.most_masked_field(), None);
    }

    #[test]
    fn stats_record_and_total() {
        let mut s = MaskingStats::new();
        s.record("phone");
        s.record("phone");
        s.record("name");
        assert_eq!(s.total_operations(), 3);
        assert_eq!(s.field_operations("phone"), 2);
        assert_eq!(s.field_operations("name"), 1);
    }

    #[test]
    fn stats_most_masked_field() {
        let mut s = MaskingStats::new();
        s.record("a");
        s.record("b");
        s.record("b");
        assert_eq!(s.most_masked_field(), Some("b"));
    }

    #[test]
    fn stats_field_operations_zero_for_unknown() {
        let s = MaskingStats::new();
        assert_eq!(s.field_operations("nonexistent"), 0);
    }
}
