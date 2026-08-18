//! 高级脱敏器：哈希脱敏、部分显示脱敏、模式匹配脱敏。
//!
//! 这些脱敏器提供比 [`crate::DataMasker`] 更灵活的脱敏方式：
//! - [`HashMasker`] — 确定性哈希脱敏，保留哈希前缀用于数据关联
//! - [`PartialDisplayMasker`] — 灵活的部分显示，自定义掩码字符和保留规则
//! - [`PatternMasker`] — 通配符模式匹配脱敏

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

// ============================================================================
// 哈希脱敏器
// ============================================================================

/// 哈希算法选择
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HashAlgorithm {
    /// SipHash-1-3（std 默认，确定性，64 位）
    SipHash,
    /// FNV-1a 32 位
    Fnv1a32,
    /// FNV-1a 64 位
    Fnv1a64,
}

impl Default for HashAlgorithm {
    fn default() -> Self {
        Self::SipHash
    }
}

impl HashAlgorithm {
    /// 算法名
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SipHash => "siphash",
            Self::Fnv1a32 => "fnv1a32",
            Self::Fnv1a64 => "fnv1a64",
        }
    }

    /// 从名称解析算法
    pub fn parse_name(name: &str) -> Option<Self> {
        match name.to_lowercase().as_str() {
            "siphash" => Some(Self::SipHash),
            "fnv1a32" => Some(Self::Fnv1a32),
            "fnv1a64" => Some(Self::Fnv1a64),
            _ => None,
        }
    }

    /// 计算哈希并返回十六进制字符串
    pub fn hash_hex(&self, input: &str) -> String {
        match self {
            Self::SipHash => {
                let mut hasher = DefaultHasher::new();
                input.hash(&mut hasher);
                format!("{:016x}", hasher.finish())
            }
            Self::Fnv1a32 => format!("{:08x}", fnv1a_32(input.as_bytes())),
            Self::Fnv1a64 => format!("{:016x}", fnv1a_64(input.as_bytes())),
        }
    }
}

/// FNV-1a 32 位哈希
fn fnv1a_32(bytes: &[u8]) -> u32 {
    const FNV_OFFSET: u32 = 0x811c9dc5;
    const FNV_PRIME: u32 = 0x01000193;
    let mut hash = FNV_OFFSET;
    for &byte in bytes {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// FNV-1a 64 位哈希
fn fnv1a_64(bytes: &[u8]) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x00000100000001b3;
    let mut hash = FNV_OFFSET;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// 哈希脱敏器：将输入计算为确定性哈希值，保留前 N 位用于数据关联。
///
/// 适用于需要脱敏但仍需关联同一实体的场景（如用户 ID 脱敏后仍可聚合统计）。
/// 同一输入始终产生同一输出，可在不同系统间做关联分析而不泄露原始值。
#[derive(Debug, Clone)]
pub struct HashMasker {
    algorithm: HashAlgorithm,
    keep_prefix: usize,
    suffix: String,
}

impl Default for HashMasker {
    fn default() -> Self {
        Self {
            algorithm: HashAlgorithm::default(),
            keep_prefix: 12,
            suffix: "...".to_string(),
        }
    }
}

impl HashMasker {
    /// 创建默认哈希脱敏器（SipHash，保留 12 位前缀）
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置哈希算法（链式）
    pub fn with_algorithm(mut self, algo: HashAlgorithm) -> Self {
        self.algorithm = algo;
        self
    }

    /// 设置保留前缀长度（链式）
    pub fn with_keep_prefix(mut self, n: usize) -> Self {
        self.keep_prefix = n;
        self
    }

    /// 设置后缀（链式）
    pub fn with_suffix(mut self, suffix: &str) -> Self {
        self.suffix = suffix.to_string();
        self
    }

    /// 哈希算法
    pub fn algorithm(&self) -> HashAlgorithm {
        self.algorithm
    }

    /// 保留前缀长度
    pub fn keep_prefix(&self) -> usize {
        self.keep_prefix
    }

    /// 后缀
    pub fn suffix(&self) -> &str {
        &self.suffix
    }

    /// 脱敏单个值
    pub fn mask(&self, value: &str) -> String {
        if value.is_empty() {
            return self.suffix.clone();
        }
        let hex = self.algorithm.hash_hex(value);
        let prefix = if hex.len() <= self.keep_prefix {
            hex.as_str()
        } else {
            &hex[..self.keep_prefix]
        };
        format!("{}{}", prefix, self.suffix)
    }

    /// 脱敏 HashMap 中的指定字段
    pub fn mask_fields(
        &self,
        fields: &[String],
        data: &HashMap<String, String>,
    ) -> HashMap<String, String> {
        data.iter()
            .map(|(k, v)| {
                if fields.contains(k) {
                    (k.clone(), self.mask(v))
                } else {
                    (k.clone(), v.clone())
                }
            })
            .collect()
    }

    /// 脱敏 JSON 字符串中的指定字段
    pub fn mask_json(&self, fields: &[String], json: &str) -> String {
        let Ok(mut value) = serde_json::from_str::<serde_json::Value>(json) else {
            return json.to_string();
        };
        let Some(obj) = value.as_object_mut() else {
            return json.to_string();
        };
        for field in fields {
            if let Some(serde_json::Value::String(s)) = obj.get_mut(field) {
                *s = self.mask(s);
            }
        }
        serde_json::to_string(&value).unwrap_or_else(|_| json.to_string())
    }

    /// 批量脱敏：对同一值列表逐个脱敏，保持顺序
    pub fn mask_batch(&self, values: &[String]) -> Vec<String> {
        values.iter().map(|v| self.mask(v)).collect()
    }
}

// ============================================================================
// 部分显示脱敏器
// ============================================================================

/// 部分显示脱敏器：保留前 N 和后 M 字符，中间用自定义掩码字符替代。
///
/// 比 [`crate::DataMasker`] 的固定规则更灵活：可自定义掩码字符、
/// 最小掩码长度、兜底值，并支持 Unicode 安全操作。
#[derive(Debug, Clone)]
pub struct PartialDisplayMasker {
    prefix_keep: usize,
    suffix_keep: usize,
    mask_char: char,
    min_mask_length: usize,
    fallback: String,
}

impl Default for PartialDisplayMasker {
    fn default() -> Self {
        Self {
            prefix_keep: 3,
            suffix_keep: 4,
            mask_char: '*',
            min_mask_length: 3,
            fallback: "***".to_string(),
        }
    }
}

impl PartialDisplayMasker {
    /// 创建默认脱敏器（前 3 后 4，`*` 掩码）
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置前缀保留数（链式）
    pub fn with_prefix(mut self, n: usize) -> Self {
        self.prefix_keep = n;
        self
    }

    /// 设置后缀保留数（链式）
    pub fn with_suffix_keep(mut self, n: usize) -> Self {
        self.suffix_keep = n;
        self
    }

    /// 设置掩码字符（链式）
    pub fn with_mask_char(mut self, c: char) -> Self {
        self.mask_char = c;
        self
    }

    /// 设置最小掩码长度（链式）
    pub fn with_min_mask_length(mut self, n: usize) -> Self {
        self.min_mask_length = n;
        self
    }

    /// 设置兜底值（链式）
    pub fn with_fallback(mut self, fallback: &str) -> Self {
        self.fallback = fallback.to_string();
        self
    }

    /// 前缀保留数
    pub fn prefix_keep(&self) -> usize {
        self.prefix_keep
    }

    /// 后缀保留数
    pub fn suffix_keep(&self) -> usize {
        self.suffix_keep
    }

    /// 掩码字符
    pub fn mask_char(&self) -> char {
        self.mask_char
    }

    /// 脱敏单个值
    pub fn mask(&self, value: &str) -> String {
        let chars: Vec<char> = value.chars().collect();
        let len = chars.len();
        if len == 0 {
            return self.fallback.clone();
        }
        let need = self.prefix_keep + self.suffix_keep;
        if len <= need {
            return self.fallback.clone();
        }
        let hidden = len - need;
        let mask_len = hidden.max(self.min_mask_length);
        let mut out = String::with_capacity(len + mask_len);
        for &c in &chars[..self.prefix_keep] {
            out.push(c);
        }
        for _ in 0..mask_len {
            out.push(self.mask_char);
        }
        for &c in &chars[len - self.suffix_keep..] {
            out.push(c);
        }
        out
    }

    /// 脱敏 HashMap 中的指定字段
    pub fn mask_fields(
        &self,
        fields: &[String],
        data: &HashMap<String, String>,
    ) -> HashMap<String, String> {
        data.iter()
            .map(|(k, v)| {
                if fields.contains(k) {
                    (k.clone(), self.mask(v))
                } else {
                    (k.clone(), v.clone())
                }
            })
            .collect()
    }
}

// ============================================================================
// 模式匹配脱敏器
// ============================================================================

/// 通配符模式匹配器：支持 `*`（任意序列）和 `?`（单字符）。
///
/// 用于按字段名模式批量应用脱敏规则，例如 `user_*` 匹配所有以 `user_` 开头的字段。
#[derive(Debug, Clone)]
pub struct PatternMasker {
    patterns: Vec<(String, crate::MaskingRule)>,
}

impl Default for PatternMasker {
    fn default() -> Self {
        Self {
            patterns: Vec::new(),
        }
    }
}

impl PatternMasker {
    /// 创建空模式匹配器
    pub fn new() -> Self {
        Self::default()
    }

    /// 添加模式与对应脱敏规则（链式）
    pub fn add_pattern(mut self, pattern: &str, rule: crate::MaskingRule) -> Self {
        self.patterns.push((pattern.to_string(), rule));
        self
    }

    /// 模式数量
    pub fn pattern_count(&self) -> usize {
        self.patterns.len()
    }

    /// 清空所有模式
    pub fn clear(&mut self) {
        self.patterns.clear();
    }

    /// 查找字段名匹配的第一个模式对应的规则
    pub fn match_rule(&self, field: &str) -> Option<&crate::MaskingRule> {
        self.patterns
            .iter()
            .find(|(pat, _)| wildcard_match(pat, field))
            .map(|(_, rule)| rule)
    }

    /// 对 HashMap 应用模式匹配脱敏
    pub fn mask_map(&self, data: &HashMap<String, String>) -> HashMap<String, String> {
        data.iter()
            .map(|(k, v)| match self.match_rule(k) {
                Some(rule) => (k.clone(), crate::DataMasker::apply(rule, v)),
                None => (k.clone(), v.clone()),
            })
            .collect()
    }

    /// 对 JSON 字符串应用模式匹配脱敏
    pub fn mask_json(&self, json: &str) -> String {
        let Ok(mut value) = serde_json::from_str::<serde_json::Value>(json) else {
            return json.to_string();
        };
        let Some(obj) = value.as_object_mut() else {
            return json.to_string();
        };
        let keys: Vec<String> = obj.keys().cloned().collect();
        for key in keys {
            if let Some(rule) = self.match_rule(&key) {
                if let Some(serde_json::Value::String(s)) = obj.get_mut(&key) {
                    *s = crate::DataMasker::apply(rule, s);
                }
            }
        }
        serde_json::to_string(&value).unwrap_or_else(|_| json.to_string())
    }
}

/// 通配符匹配：`*` 匹配任意字符序列，`?` 匹配单个字符。
///
/// 使用动态规划实现，时间复杂度 O(m*n)。
pub fn wildcard_match(pattern: &str, text: &str) -> bool {
    let pat: Vec<char> = pattern.chars().collect();
    let txt: Vec<char> = text.chars().collect();
    let m = pat.len();
    let n = txt.len();

    // dp[i][j] = pattern[..i] 匹配 text[..j]
    let mut dp = vec![vec![false; n + 1]; m + 1];
    dp[0][0] = true;

    // pattern 以 * 开头时可以匹配空串
    for i in 1..=m {
        if pat[i - 1] == '*' {
            dp[i][0] = dp[i - 1][0];
        }
    }

    for i in 1..=m {
        for j in 1..=n {
            match pat[i - 1] {
                '*' => dp[i][j] = dp[i - 1][j] || dp[i][j - 1],
                '?' => dp[i][j] = dp[i - 1][j - 1],
                c => dp[i][j] = dp[i - 1][j - 1] && c == txt[j - 1],
            }
        }
    }

    dp[m][n]
}

#[cfg(test)]
mod tests {
    use super::*;

    // ----- HashAlgorithm -----

    #[test]
    fn hash_algorithm_default_is_siphash() {
        assert_eq!(HashAlgorithm::default(), HashAlgorithm::SipHash);
    }

    #[test]
    fn hash_algorithm_as_str() {
        assert_eq!(HashAlgorithm::SipHash.as_str(), "siphash");
        assert_eq!(HashAlgorithm::Fnv1a32.as_str(), "fnv1a32");
        assert_eq!(HashAlgorithm::Fnv1a64.as_str(), "fnv1a64");
    }

    #[test]
    fn hash_algorithm_parse_name_valid() {
        assert_eq!(
            HashAlgorithm::parse_name("siphash"),
            Some(HashAlgorithm::SipHash)
        );
        assert_eq!(
            HashAlgorithm::parse_name("FNV1A32"),
            Some(HashAlgorithm::Fnv1a32)
        );
        assert_eq!(
            HashAlgorithm::parse_name("fnv1a64"),
            Some(HashAlgorithm::Fnv1a64)
        );
    }

    #[test]
    fn hash_algorithm_parse_name_invalid() {
        assert_eq!(HashAlgorithm::parse_name("md5"), None);
        assert_eq!(HashAlgorithm::parse_name(""), None);
    }

    #[test]
    fn hash_algorithm_siphash_deterministic() {
        let a = HashAlgorithm::SipHash.hash_hex("hello");
        let b = HashAlgorithm::SipHash.hash_hex("hello");
        assert_eq!(a, b);
        assert_eq!(a.len(), 16);
    }

    #[test]
    fn hash_algorithm_fnv1a32_deterministic() {
        let a = HashAlgorithm::Fnv1a32.hash_hex("test");
        let b = HashAlgorithm::Fnv1a32.hash_hex("test");
        assert_eq!(a, b);
        assert_eq!(a.len(), 8);
    }

    #[test]
    fn hash_algorithm_fnv1a64_deterministic() {
        let a = HashAlgorithm::Fnv1a64.hash_hex("test");
        let b = HashAlgorithm::Fnv1a64.hash_hex("test");
        assert_eq!(a, b);
        assert_eq!(a.len(), 16);
    }

    #[test]
    fn hash_algorithm_different_inputs_different_hashes() {
        let a = HashAlgorithm::SipHash.hash_hex("alice");
        let b = HashAlgorithm::SipHash.hash_hex("bob");
        assert_ne!(a, b);
    }

    #[test]
    fn hash_algorithm_empty_input() {
        let h = HashAlgorithm::SipHash.hash_hex("");
        assert!(!h.is_empty());
    }

    #[test]
    fn fnv1a_32_known_values() {
        // FNV-1a 32 位空串 = offset basis
        assert_eq!(fnv1a_32(b""), 0x811c9dc5);
    }

    #[test]
    fn fnv1a_64_known_values() {
        // FNV-1a 64 位空串 = offset basis
        assert_eq!(fnv1a_64(b""), 0xcbf29ce484222325);
    }

    #[test]
    fn fnv1a_32_single_byte() {
        let h = fnv1a_32(b"a");
        // 手动计算：(offset ^ 97) * prime
        let expected = (0x811c9dc5u32 ^ 97).wrapping_mul(0x01000193);
        assert_eq!(h, expected);
    }

    // ----- HashMasker -----

    #[test]
    fn hash_masker_default() {
        let m = HashMasker::new();
        assert_eq!(m.algorithm(), HashAlgorithm::SipHash);
        assert_eq!(m.keep_prefix(), 12);
        assert_eq!(m.suffix(), "...");
    }

    #[test]
    fn hash_masker_mask_basic() {
        let m = HashMasker::new();
        let result = m.mask("13812345678");
        assert!(result.ends_with("..."));
        assert!(result.len() > 3);
    }

    #[test]
    fn hash_masker_mask_empty() {
        let m = HashMasker::new();
        assert_eq!(m.mask(""), "...");
    }

    #[test]
    fn hash_masker_deterministic() {
        let m = HashMasker::new();
        let a = m.mask("same_value");
        let b = m.mask("same_value");
        assert_eq!(a, b);
    }

    #[test]
    fn hash_masker_different_values_different_output() {
        let m = HashMasker::new();
        let a = m.mask("alice");
        let b = m.mask("bob");
        assert_ne!(a, b);
    }

    #[test]
    fn hash_masker_with_algorithm_fnv1a32() {
        let m = HashMasker::new().with_algorithm(HashAlgorithm::Fnv1a32);
        let result = m.mask("test");
        assert!(result.ends_with("..."));
        assert_eq!(m.algorithm(), HashAlgorithm::Fnv1a32);
    }

    #[test]
    fn hash_masker_with_keep_prefix() {
        let m = HashMasker::new().with_keep_prefix(4);
        let result = m.mask("hello");
        // 4 hex chars + "..."
        assert_eq!(result.len(), 7);
    }

    #[test]
    fn hash_masker_with_suffix() {
        let m = HashMasker::new().with_suffix("[hashed]");
        let result = m.mask("value");
        assert!(result.ends_with("[hashed]"));
    }

    #[test]
    fn hash_masker_keep_prefix_exceeds_hash_len() {
        // keep_prefix > hash hex length → use full hex
        let m = HashMasker::new().with_keep_prefix(100);
        let result = m.mask("test");
        // SipHash produces 16 hex chars + "..."
        assert_eq!(result.len(), 19);
    }

    #[test]
    fn hash_masker_mask_fields() {
        let m = HashMasker::new();
        let mut data = HashMap::new();
        data.insert("phone".to_string(), "13812345678".to_string());
        data.insert("name".to_string(), "Alice".to_string());
        let fields = vec!["phone".to_string()];
        let result = m.mask_fields(&fields, &data);
        assert_ne!(result["phone"], "13812345678");
        assert_eq!(result["name"], "Alice");
    }

    #[test]
    fn hash_masker_mask_json() {
        let m = HashMasker::new();
        let json = r#"{"phone":"13812345678","name":"Alice"}"#;
        let fields = vec!["phone".to_string()];
        let result = m.mask_json(&fields, json);
        assert!(result.contains("Alice"));
        assert!(!result.contains("13812345678"));
    }

    #[test]
    fn hash_masker_mask_json_invalid() {
        let m = HashMasker::new();
        let result = m.mask_json(&["phone".to_string()], "not json");
        assert_eq!(result, "not json");
    }

    #[test]
    fn hash_masker_mask_batch() {
        let m = HashMasker::new();
        let values = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let result = m.mask_batch(&values);
        assert_eq!(result.len(), 3);
        assert_ne!(result[0], result[1]);
    }

    // ----- PartialDisplayMasker -----

    #[test]
    fn partial_display_default() {
        let m = PartialDisplayMasker::new();
        assert_eq!(m.prefix_keep(), 3);
        assert_eq!(m.suffix_keep(), 4);
        assert_eq!(m.mask_char(), '*');
    }

    #[test]
    fn partial_display_mask_basic() {
        let m = PartialDisplayMasker::new();
        assert_eq!(m.mask("13812345678"), "138****5678");
    }

    #[test]
    fn partial_display_mask_too_short() {
        let m = PartialDisplayMasker::new();
        assert_eq!(m.mask("123"), "***");
    }

    #[test]
    fn partial_display_mask_empty() {
        let m = PartialDisplayMasker::new();
        assert_eq!(m.mask(""), "***");
    }

    #[test]
    fn partial_display_custom_mask_char() {
        let m = PartialDisplayMasker::new().with_mask_char('#');
        assert_eq!(m.mask("13812345678"), "138####5678");
    }

    #[test]
    fn partial_display_custom_prefix_suffix() {
        let m = PartialDisplayMasker::new()
            .with_prefix(2)
            .with_suffix_keep(2);
        assert_eq!(m.mask("abcdefgh"), "ab****gh");
    }

    #[test]
    fn partial_display_min_mask_length() {
        let m = PartialDisplayMasker::new()
            .with_prefix(3)
            .with_suffix_keep(4)
            .with_min_mask_length(6);
        // 8 chars: 3 prefix + 4 suffix = 7, hidden = 1, min_mask = 6
        assert_eq!(m.mask("12345678"), "123******5678");
    }

    #[test]
    fn partial_display_custom_fallback() {
        let m = PartialDisplayMasker::new().with_fallback("[hidden]");
        assert_eq!(m.mask(""), "[hidden]");
        assert_eq!(m.mask("ab"), "[hidden]");
    }

    #[test]
    fn partial_display_unicode() {
        let m = PartialDisplayMasker::new()
            .with_prefix(1)
            .with_suffix_keep(1);
        // 中文：5 chars, prefix=1, suffix=1, hidden=3
        assert_eq!(m.mask("张三李四王"), "张***王");
    }

    #[test]
    fn partial_display_mask_fields() {
        let m = PartialDisplayMasker::new();
        let mut data = HashMap::new();
        data.insert("phone".to_string(), "13812345678".to_string());
        data.insert("name".to_string(), "Alice".to_string());
        let fields = vec!["phone".to_string()];
        let result = m.mask_fields(&fields, &data);
        assert_eq!(result["phone"], "138****5678");
        assert_eq!(result["name"], "Alice");
    }

    #[test]
    fn partial_display_exact_boundary() {
        // len == prefix + suffix → fallback
        let m = PartialDisplayMasker::new();
        assert_eq!(m.mask("1234567"), "***");
    }

    #[test]
    fn partial_display_one_past_boundary() {
        // len == prefix + suffix + 1 → hidden=1, but min_mask_length=3 → 3 mask chars
        let m = PartialDisplayMasker::new();
        assert_eq!(m.mask("12345678"), "123***5678");
    }

    // ----- PatternMasker / wildcard_match -----

    #[test]
    fn wildcard_exact_match() {
        assert!(wildcard_match("phone", "phone"));
    }

    #[test]
    fn wildcard_no_match() {
        assert!(!wildcard_match("phone", "email"));
    }

    #[test]
    fn wildcard_star_match_prefix() {
        assert!(wildcard_match("user_*", "user_name"));
        assert!(wildcard_match("user_*", "user_id"));
    }

    #[test]
    fn wildcard_star_match_suffix() {
        assert!(wildcard_match("*_id", "user_id"));
        assert!(wildcard_match("*_id", "order_id"));
    }

    #[test]
    fn wildcard_star_match_entire() {
        assert!(wildcard_match("*", "anything"));
        assert!(wildcard_match("*", ""));
    }

    #[test]
    fn wildcard_question_match_single() {
        assert!(wildcard_match("user_?", "user_1"));
        assert!(!wildcard_match("user_?", "user_12"));
    }

    #[test]
    fn wildcard_combined_star_question() {
        assert!(wildcard_match("u*r?", "user_"));
        assert!(wildcard_match("?*?", "abc"));
    }

    #[test]
    fn wildcard_empty_pattern() {
        assert!(wildcard_match("", ""));
        assert!(!wildcard_match("", "a"));
    }

    #[test]
    fn wildcard_star_only() {
        assert!(wildcard_match("***", "test"));
        assert!(wildcard_match("***", ""));
    }

    #[test]
    fn pattern_masker_default_empty() {
        let m = PatternMasker::new();
        assert_eq!(m.pattern_count(), 0);
    }

    #[test]
    fn pattern_masker_add_pattern() {
        let m = PatternMasker::new().add_pattern("phone_*", crate::MaskingRule::Phone);
        assert_eq!(m.pattern_count(), 1);
    }

    #[test]
    fn pattern_masker_match_rule() {
        let m = PatternMasker::new()
            .add_pattern("phone_*", crate::MaskingRule::Phone)
            .add_pattern("*_email", crate::MaskingRule::Email);
        assert!(m.match_rule("phone_primary").is_some());
        assert!(m.match_rule("user_email").is_some());
        assert!(m.match_rule("address").is_none());
    }

    #[test]
    fn pattern_masker_clear() {
        let mut m = PatternMasker::new().add_pattern("*", crate::MaskingRule::Phone);
        m.clear();
        assert_eq!(m.pattern_count(), 0);
    }

    #[test]
    fn pattern_masker_mask_map() {
        let m = PatternMasker::new()
            .add_pattern("phone_*", crate::MaskingRule::Phone)
            .add_pattern("*_email", crate::MaskingRule::Email);
        let mut data = HashMap::new();
        data.insert("phone_primary".to_string(), "13812345678".to_string());
        data.insert("user_email".to_string(), "test@example.com".to_string());
        data.insert("name".to_string(), "Alice".to_string());
        let result = m.mask_map(&data);
        assert_eq!(result["phone_primary"], "138****5678");
        assert_eq!(result["user_email"], "t***@example.com");
        assert_eq!(result["name"], "Alice");
    }

    #[test]
    fn pattern_masker_mask_json() {
        let m = PatternMasker::new().add_pattern("phone", crate::MaskingRule::Phone);
        let json = r#"{"phone":"13812345678","name":"Alice"}"#;
        let result = m.mask_json(json);
        assert!(result.contains("138****5678"));
        assert!(result.contains("Alice"));
    }

    #[test]
    fn pattern_masker_mask_json_invalid() {
        let m = PatternMasker::new().add_pattern("*", crate::MaskingRule::Phone);
        assert_eq!(m.mask_json("not json"), "not json");
    }

    #[test]
    fn pattern_masker_no_match_passthrough() {
        let m = PatternMasker::new().add_pattern("secret_*", crate::MaskingRule::Password);
        let mut data = HashMap::new();
        data.insert("public_field".to_string(), "visible".to_string());
        let result = m.mask_map(&data);
        assert_eq!(result["public_field"], "visible");
    }
}
