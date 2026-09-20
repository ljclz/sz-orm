//! v6.7.0 动态脱敏策略热更新。
//!
//! v7.5.0 扩展字段级脱敏规则配置：`MaskingStrategy` 枚举 + `MaskingRuleConfig` +
//! 多策略冲突检测（`MASKING_POLICY_CONFLICT`）。

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MaskingRule {
    MaskMiddle,
    MaskAll,
    Hash,
    Truncate(usize),
}

// ============================================================================
// v7.5.0 字段级脱敏规则配置
// ============================================================================

/// 掩码配置：保留前缀/后缀，中间用 `mask_char` 替换（不可逆）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaskConfig {
    pub keep_prefix: usize,
    pub keep_suffix: usize,
    pub mask_char: char,
}

impl Default for MaskConfig {
    fn default() -> Self {
        Self {
            keep_prefix: 0,
            keep_suffix: 0,
            mask_char: '*',
        }
    }
}

impl MaskConfig {
    pub fn new(keep_prefix: usize, keep_suffix: usize) -> Self {
        Self {
            keep_prefix,
            keep_suffix,
            mask_char: '*',
        }
    }

    pub fn with_mask_char(mut self, c: char) -> Self {
        self.mask_char = c;
        self
    }
}

/// 哈希算法标识。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MaskingHashAlgorithm {
    Sha256,
    Sha512,
}

/// 哈希配置：算法 + 盐（不可逆）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HashConfig {
    pub algorithm: MaskingHashAlgorithm,
    pub salt: String,
}

impl Default for HashConfig {
    fn default() -> Self {
        Self {
            algorithm: MaskingHashAlgorithm::Sha256,
            salt: String::new(),
        }
    }
}

impl HashConfig {
    pub fn new(algorithm: MaskingHashAlgorithm, salt: &str) -> Self {
        Self {
            algorithm,
            salt: salt.to_string(),
        }
    }
}

/// 加密算法标识。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EncryptAlgorithm {
    Aes256Gcm,
    ChaCha20Poly1305,
}

/// 加密配置：算法 + 密钥引用（`key_ref` 不存储密钥本身，密钥分离存储）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncryptConfig {
    pub algorithm: EncryptAlgorithm,
    pub key_ref: String,
}

impl EncryptConfig {
    pub fn new(algorithm: EncryptAlgorithm, key_ref: &str) -> Self {
        Self {
            algorithm,
            key_ref: key_ref.to_string(),
        }
    }
}

/// 脱敏策略。
///
/// - `Mask` / `Hash` / `Truncate` / `Replace`：不可逆，无法从脱敏值还原原值。
/// - `Encrypt`：可逆，但密钥分离存储（`key_ref` 仅引用），脱敏结果不包含密钥。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MaskingStrategy {
    /// 掩码（不可逆）
    Mask(MaskConfig),
    /// 哈希（不可逆）
    Hash(HashConfig),
    /// 加密脱敏（可逆，密钥分离存储）
    Encrypt(EncryptConfig),
    /// 截断（不可逆）
    Truncate(usize),
    /// 替换为固定值（不可逆）
    Replace(String),
}

impl MaskingStrategy {
    /// 是否不可逆（无法从脱敏值还原原值）。
    pub fn is_irreversible(&self) -> bool {
        matches!(
            self,
            MaskingStrategy::Mask(_)
                | MaskingStrategy::Hash(_)
                | MaskingStrategy::Truncate(_)
                | MaskingStrategy::Replace(_)
        )
    }

    /// 是否为加密脱敏（可逆，需密钥分离）。
    pub fn is_encrypt(&self) -> bool {
        matches!(self, MaskingStrategy::Encrypt(_))
    }
}

/// 字段级脱敏规则配置：字段名 + 策略 + 优先级。
///
/// 多策略冲突时取最高优先级（数值最小）的规则，并产生 `MASKING_POLICY_CONFLICT` 告警。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaskingRuleConfig {
    pub field_name: String,
    pub strategy: MaskingStrategy,
    pub priority: u32,
}

impl MaskingRuleConfig {
    pub fn new(field_name: &str, strategy: MaskingStrategy) -> Self {
        Self {
            field_name: field_name.to_string(),
            strategy,
            priority: 100,
        }
    }

    pub fn with_priority(mut self, priority: u32) -> Self {
        self.priority = priority;
        self
    }
}

/// 多策略冲突告警（`MASKING_POLICY_CONFLICT`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaskingPolicyConflict {
    pub field_name: String,
    pub resolved_priority: u32,
    pub conflicting_priorities: Vec<u32>,
}

impl MaskingPolicyConflict {
    /// 告警码
    pub const CODE: &'static str = "MASKING_POLICY_CONFLICT";

    /// 格式化告警消息
    pub fn message(&self) -> String {
        format!(
            "{}: field '{}' has {} conflicting rules (priorities: {:?}), resolved to priority {}",
            Self::CODE,
            self.field_name,
            self.conflicting_priorities.len(),
            self.conflicting_priorities,
            self.resolved_priority,
        )
    }
}

/// 脱敏规则集：管理多条 `MaskingRuleConfig`，处理冲突。
#[derive(Debug, Clone, Default)]
pub struct MaskingRuleSet {
    rules: Vec<MaskingRuleConfig>,
}

impl MaskingRuleSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_rule(mut self, rule: MaskingRuleConfig) -> Self {
        self.rules.push(rule);
        self
    }

    pub fn add_rules(mut self, rules: Vec<MaskingRuleConfig>) -> Self {
        self.rules.extend(rules);
        self
    }

    pub fn rules(&self) -> &[MaskingRuleConfig] {
        &self.rules
    }

    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// 获取某字段的有效规则（优先级最高，数值最小）。
    ///
    /// 返回 `(有效规则, 冲突告警)`。当多个规则匹配同一字段时产生 `MASKING_POLICY_CONFLICT`。
    pub fn effective_rule(
        &self,
        field_name: &str,
    ) -> (Option<&MaskingRuleConfig>, Option<MaskingPolicyConflict>) {
        let candidates: Vec<&MaskingRuleConfig> = self
            .rules
            .iter()
            .filter(|r| r.field_name == field_name)
            .collect();

        if candidates.is_empty() {
            return (None, None);
        }

        let mut sorted = candidates;
        sorted.sort_by_key(|r| r.priority);
        let effective = sorted[0];

        let priorities: Vec<u32> = sorted.iter().map(|r| r.priority).collect();
        let conflict = if priorities.len() > 1 {
            Some(MaskingPolicyConflict {
                field_name: field_name.to_string(),
                resolved_priority: effective.priority,
                conflicting_priorities: priorities,
            })
        } else {
            None
        };

        (Some(effective), conflict)
    }

    /// 对字段值应用脱敏。未配置规则时原样返回（与 v7.4.0 一致）。
    pub fn apply(&self, field_name: &str, value: &str) -> (String, Option<MaskingPolicyConflict>) {
        let (rule, conflict) = self.effective_rule(field_name);
        match rule {
            Some(r) => (apply_strategy(value, &r.strategy), conflict),
            None => (value.to_string(), None),
        }
    }

    /// 对 HashMap 应用脱敏，返回脱敏后的 map 和所有冲突告警。
    pub fn apply_to_map(
        &self,
        data: &HashMap<String, String>,
    ) -> (HashMap<String, String>, Vec<MaskingPolicyConflict>) {
        let mut result = HashMap::with_capacity(data.len());
        let mut conflicts = Vec::new();
        for (k, v) in data {
            let (masked, conflict) = self.apply(k, v);
            result.insert(k.clone(), masked);
            if let Some(c) = conflict {
                conflicts.push(c);
            }
        }
        (result, conflicts)
    }
}

/// 应用脱敏策略到单个值。
pub fn apply_strategy(value: &str, strategy: &MaskingStrategy) -> String {
    match strategy {
        MaskingStrategy::Mask(config) => apply_mask_config(value, config),
        MaskingStrategy::Hash(config) => apply_hash_config(value, config),
        MaskingStrategy::Encrypt(config) => apply_encrypt_config(value, config),
        MaskingStrategy::Truncate(n) => value.chars().take(*n).collect(),
        MaskingStrategy::Replace(s) => s.clone(),
    }
}

fn apply_mask_config(value: &str, config: &MaskConfig) -> String {
    let chars: Vec<char> = value.chars().collect();
    let len = chars.len();
    if len == 0 {
        return String::new();
    }
    if len <= config.keep_prefix + config.keep_suffix {
        return config.mask_char.to_string().repeat(len);
    }
    let hidden = len - config.keep_prefix - config.keep_suffix;
    let mut out = String::with_capacity(len);
    for &c in &chars[..config.keep_prefix] {
        out.push(c);
    }
    for _ in 0..hidden {
        out.push(config.mask_char);
    }
    for &c in &chars[len - config.keep_suffix..] {
        out.push(c);
    }
    out
}

fn apply_hash_config(value: &str, config: &HashConfig) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let algo_tag = match config.algorithm {
        MaskingHashAlgorithm::Sha256 => "sha256:",
        MaskingHashAlgorithm::Sha512 => "sha512:",
    };
    let salted = format!("{}{}{}", algo_tag, value, config.salt);
    let mut hasher = DefaultHasher::new();
    salted.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// 加密脱敏：不实际执行加密（密钥分离存储），返回算法标识 + 密钥引用占位。
///
/// 实际加密由外部密钥管理服务根据 `key_ref` 执行，脱敏结果不包含密钥本身。
fn apply_encrypt_config(value: &str, config: &EncryptConfig) -> String {
    let _ = value;
    let algo = match config.algorithm {
        EncryptAlgorithm::Aes256Gcm => "AES-256-GCM",
        EncryptAlgorithm::ChaCha20Poly1305 => "ChaCha20-Poly1305",
    };
    format!("ENC({}:{})", algo, config.key_ref)
}

pub struct DynamicMaskingConfig {
    rules: Arc<RwLock<HashMap<String, MaskingRule>>>,
}

impl DynamicMaskingConfig {
    pub fn new() -> Self {
        Self {
            rules: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn set_rule(&self, field: &str, rule: MaskingRule) {
        self.rules.write().unwrap().insert(field.to_string(), rule);
    }

    pub fn hot_update(&self, new_rules: HashMap<String, MaskingRule>) {
        let mut rules = self.rules.write().unwrap();
        *rules = new_rules;
    }

    pub fn get_rule(&self, field: &str) -> Option<MaskingRule> {
        self.rules.read().unwrap().get(field).cloned()
    }

    pub fn snapshot(&self) -> HashMap<String, MaskingRule> {
        self.rules.read().unwrap().clone()
    }
}

impl Default for DynamicMaskingConfig {
    fn default() -> Self {
        Self::new()
    }
}

pub fn apply_mask(value: &str, rule: &MaskingRule) -> String {
    match rule {
        MaskingRule::MaskAll => "*".repeat(value.chars().count()),
        MaskingRule::MaskMiddle => mask_middle(value),
        MaskingRule::Hash => {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut hasher = DefaultHasher::new();
            value.hash(&mut hasher);
            format!("{:016x}", hasher.finish())
        }
        MaskingRule::Truncate(n) => {
            let chars: Vec<char> = value.chars().take(*n).collect();
            chars.into_iter().collect()
        }
    }
}

fn mask_middle(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    let len = chars.len();
    if len <= 4 {
        return "*".repeat(len);
    }
    let prefix = len / 3;
    let suffix = len / 3;
    let mut result = String::new();
    for (i, ch) in chars.iter().enumerate() {
        if i < prefix || i >= len - suffix {
            result.push(*ch);
        } else {
            result.push('*');
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_middle_phone() {
        let masked = apply_mask("13800001234", &MaskingRule::MaskMiddle);
        assert!(masked.starts_with("13"));
        assert!(masked.ends_with("34"));
        assert!(masked.contains('*'));
    }

    #[test]
    fn mask_all_replaces_everything() {
        let masked = apply_mask("secret", &MaskingRule::MaskAll);
        assert_eq!(masked, "******");
    }

    #[test]
    fn hash_produces_consistent_output() {
        let h1 = apply_mask("test", &MaskingRule::Hash);
        let h2 = apply_mask("test", &MaskingRule::Hash);
        assert_eq!(h1, h2);
        assert_ne!(h1, "test");
    }

    #[test]
    fn truncate_limits_length() {
        let truncated = apply_mask("hello world", &MaskingRule::Truncate(5));
        assert_eq!(truncated, "hello");
    }

    #[test]
    fn hot_update_replaces_all_rules() {
        let config = DynamicMaskingConfig::new();
        config.set_rule("phone", MaskingRule::MaskAll);
        let mut new_rules = HashMap::new();
        new_rules.insert("email".to_string(), MaskingRule::Hash);
        config.hot_update(new_rules);
        assert!(config.get_rule("phone").is_none());
        assert!(config.get_rule("email").is_some());
    }

    #[test]
    fn snapshot_is_consistent() {
        let config = DynamicMaskingConfig::new();
        config.set_rule("a", MaskingRule::MaskAll);
        config.set_rule("b", MaskingRule::Hash);
        let snap = config.snapshot();
        assert_eq!(snap.len(), 2);
    }

    #[test]
    fn mask_is_irreversible() {
        let original = "13800001234";
        let masked = apply_mask(original, &MaskingRule::MaskMiddle);
        assert_ne!(masked, original);
        assert!(!masked.contains("0000"));
    }

    #[test]
    fn short_string_mask_middle() {
        assert_eq!(apply_mask("ab", &MaskingRule::MaskMiddle), "**");
    }

    // ----- v7.5.0 MaskingRuleConfig 测试 -----

    #[test]
    fn mask_config_default() {
        let c = MaskConfig::default();
        assert_eq!(c.keep_prefix, 0);
        assert_eq!(c.keep_suffix, 0);
        assert_eq!(c.mask_char, '*');
    }

    #[test]
    fn mask_config_with_mask_char() {
        let c = MaskConfig::new(3, 4).with_mask_char('#');
        assert_eq!(c.keep_prefix, 3);
        assert_eq!(c.keep_suffix, 4);
        assert_eq!(c.mask_char, '#');
    }

    #[test]
    fn hash_config_new() {
        let c = HashConfig::new(MaskingHashAlgorithm::Sha256, "mysalt");
        assert_eq!(c.algorithm, MaskingHashAlgorithm::Sha256);
        assert_eq!(c.salt, "mysalt");
    }

    #[test]
    fn encrypt_config_key_ref_only() {
        let c = EncryptConfig::new(EncryptAlgorithm::Aes256Gcm, "vault://key1");
        assert_eq!(c.key_ref, "vault://key1");
        assert!(!c.key_ref.contains("secret"));
    }

    #[test]
    fn strategy_is_irreversible() {
        assert!(MaskingStrategy::Mask(MaskConfig::new(3, 4)).is_irreversible());
        assert!(MaskingStrategy::Hash(HashConfig::default()).is_irreversible());
        assert!(MaskingStrategy::Truncate(5).is_irreversible());
        assert!(MaskingStrategy::Replace("***".to_string()).is_irreversible());
        assert!(
            !MaskingStrategy::Encrypt(EncryptConfig::new(EncryptAlgorithm::Aes256Gcm, "k"))
                .is_irreversible()
        );
    }

    #[test]
    fn strategy_encrypt_detected() {
        let s = MaskingStrategy::Encrypt(EncryptConfig::new(EncryptAlgorithm::Aes256Gcm, "k"));
        assert!(s.is_encrypt());
        assert!(!MaskingStrategy::Mask(MaskConfig::default()).is_encrypt());
    }

    #[test]
    fn apply_mask_strategy() {
        let s = MaskingStrategy::Mask(MaskConfig::new(3, 4));
        let result = apply_strategy("13812345678", &s);
        assert_eq!(result, "138****5678");
    }

    #[test]
    fn apply_mask_strategy_custom_char() {
        let s = MaskingStrategy::Mask(MaskConfig::new(2, 2).with_mask_char('#'));
        let result = apply_strategy("abcdef", &s);
        assert_eq!(result, "ab##ef");
    }

    #[test]
    fn apply_hash_strategy_irreversible() {
        let s = MaskingStrategy::Hash(HashConfig::new(MaskingHashAlgorithm::Sha256, "salt"));
        let result = apply_strategy("secret_value", &s);
        assert_ne!(result, "secret_value");
        assert!(!result.contains("secret_value"));
    }

    #[test]
    fn apply_hash_strategy_different_algos_differ() {
        let s256 = MaskingStrategy::Hash(HashConfig::new(MaskingHashAlgorithm::Sha256, ""));
        let s512 = MaskingStrategy::Hash(HashConfig::new(MaskingHashAlgorithm::Sha512, ""));
        let r256 = apply_strategy("test", &s256);
        let r512 = apply_strategy("test", &s512);
        assert_ne!(r256, r512);
    }

    #[test]
    fn apply_hash_strategy_same_input_same_output() {
        let s = MaskingStrategy::Hash(HashConfig::new(MaskingHashAlgorithm::Sha256, "salt"));
        let r1 = apply_strategy("test", &s);
        let r2 = apply_strategy("test", &s);
        assert_eq!(r1, r2);
    }

    #[test]
    fn apply_truncate_strategy() {
        let s = MaskingStrategy::Truncate(5);
        assert_eq!(apply_strategy("hello world", &s), "hello");
    }

    #[test]
    fn apply_replace_strategy() {
        let s = MaskingStrategy::Replace("***".to_string());
        assert_eq!(apply_strategy("anything", &s), "***");
    }

    #[test]
    fn apply_encrypt_strategy_no_key_in_output() {
        let s = MaskingStrategy::Encrypt(EncryptConfig::new(
            EncryptAlgorithm::Aes256Gcm,
            "vault://mykey",
        ));
        let result = apply_strategy("secret", &s);
        assert!(result.contains("AES-256-GCM"));
        assert!(result.contains("vault://mykey"));
        assert!(!result.contains("secret"));
    }

    #[test]
    fn rule_config_new_default_priority() {
        let r = MaskingRuleConfig::new("phone", MaskingStrategy::Replace("***".to_string()));
        assert_eq!(r.field_name, "phone");
        assert_eq!(r.priority, 100);
    }

    #[test]
    fn rule_config_with_priority() {
        let r = MaskingRuleConfig::new("phone", MaskingStrategy::Replace("***".to_string()))
            .with_priority(10);
        assert_eq!(r.priority, 10);
    }

    #[test]
    fn rule_set_effective_rule_single() {
        let set = MaskingRuleSet::new().add_rule(MaskingRuleConfig::new(
            "phone",
            MaskingStrategy::Mask(MaskConfig::new(3, 4)),
        ));
        let (rule, conflict) = set.effective_rule("phone");
        assert!(rule.is_some());
        assert!(conflict.is_none());
    }

    #[test]
    fn rule_set_effective_rule_none() {
        let set = MaskingRuleSet::new();
        let (rule, conflict) = set.effective_rule("phone");
        assert!(rule.is_none());
        assert!(conflict.is_none());
    }

    #[test]
    fn rule_set_conflict_detected() {
        let set = MaskingRuleSet::new()
            .add_rule(
                MaskingRuleConfig::new("phone", MaskingStrategy::Mask(MaskConfig::new(3, 4)))
                    .with_priority(50),
            )
            .add_rule(
                MaskingRuleConfig::new("phone", MaskingStrategy::Hash(HashConfig::default()))
                    .with_priority(100),
            );
        let (rule, conflict) = set.effective_rule("phone");
        assert!(rule.is_some());
        assert_eq!(rule.unwrap().priority, 50);
        let c = conflict.unwrap();
        assert_eq!(c.field_name, "phone");
        assert_eq!(c.resolved_priority, 50);
        assert_eq!(c.conflicting_priorities, vec![50, 100]);
        assert!(c.message().contains("MASKING_POLICY_CONFLICT"));
    }

    #[test]
    fn rule_set_apply_no_rule_returns_original() {
        let set = MaskingRuleSet::new();
        let (result, conflict) = set.apply("phone", "13812345678");
        assert_eq!(result, "13812345678");
        assert!(conflict.is_none());
    }

    #[test]
    fn rule_set_apply_with_rule() {
        let set = MaskingRuleSet::new().add_rule(MaskingRuleConfig::new(
            "phone",
            MaskingStrategy::Mask(MaskConfig::new(3, 4)),
        ));
        let (result, conflict) = set.apply("phone", "13812345678");
        assert_eq!(result, "138****5678");
        assert!(conflict.is_none());
    }

    #[test]
    fn rule_set_apply_to_map() {
        let set = MaskingRuleSet::new()
            .add_rule(MaskingRuleConfig::new(
                "phone",
                MaskingStrategy::Mask(MaskConfig::new(3, 4)),
            ))
            .add_rule(MaskingRuleConfig::new(
                "email",
                MaskingStrategy::Replace("***".to_string()),
            ));
        let mut data = HashMap::new();
        data.insert("phone".to_string(), "13812345678".to_string());
        data.insert("email".to_string(), "a@b.com".to_string());
        data.insert("name".to_string(), "Alice".to_string());
        let (result, conflicts) = set.apply_to_map(&data);
        assert_eq!(result["phone"], "138****5678");
        assert_eq!(result["email"], "***");
        assert_eq!(result["name"], "Alice");
        assert!(conflicts.is_empty());
    }

    #[test]
    fn masking_irreversible_mask() {
        let original = "13812345678";
        let masked = apply_strategy(original, &MaskingStrategy::Mask(MaskConfig::new(3, 4)));
        assert_ne!(masked, original);
        assert!(!masked.contains("1234"));
    }

    #[test]
    fn masking_irreversible_hash() {
        let original = "secret_data";
        let hashed = apply_strategy(
            original,
            &MaskingStrategy::Hash(HashConfig::new(MaskingHashAlgorithm::Sha256, "salt")),
        );
        assert_ne!(hashed, original);
        assert!(!hashed.contains("secret_data"));
    }

    #[test]
    fn masking_irreversible_truncate() {
        let original = "hello world";
        let truncated = apply_strategy(original, &MaskingStrategy::Truncate(5));
        assert_ne!(truncated, original);
        assert!(!truncated.contains("world"));
    }

    #[test]
    fn masking_irreversible_replace() {
        let original = "secret";
        let replaced = apply_strategy(original, &MaskingStrategy::Replace("***".to_string()));
        assert_ne!(replaced, original);
        assert!(!replaced.contains("secret"));
    }
}
// =====================================================================
// v7.6.0 组4.1+4.2：上下文感知脱敏 + 原子热更新
// =====================================================================

/// v7.6.0 脱敏错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MaskingError {
    /// 规则冲突
    RuleConflict(String),
    /// 策略不适用
    StrategyNotApplicable(String),
    /// 热更新冲突
    HotUpdateConflict(String),
}

/// v7.6.0 数据流向
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataFlow {
    /// 入站（写入 DB）
    Inbound,
    /// 出站（返回用户）
    Outbound,
    /// 内部（服务间调用）
    Internal,
}

/// v7.6.0 脱敏上下文
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaskingContext {
    /// 用户角色
    pub user_role: String,
    /// 查询上下文
    pub query_context: String,
    /// 数据流向
    pub data_flow: DataFlow,
}

impl MaskingContext {
    pub fn new(user_role: &str, query_context: &str, data_flow: DataFlow) -> Self {
        Self {
            user_role: user_role.to_string(),
            query_context: query_context.to_string(),
            data_flow,
        }
    }

    /// 是否为管理员角色（管理员可查看原始数据）
    pub fn is_admin(&self) -> bool {
        matches!(self.user_role.as_str(), "admin" | "root" | "superuser")
    }

    /// 是否为出站数据（需要脱敏）
    pub fn is_outbound(&self) -> bool {
        self.data_flow == DataFlow::Outbound
    }
}

/// v7.6.0 上下文感知脱敏器
///
/// 根据查询上下文 / 用户角色 / 数据流向动态选择脱敏策略，
/// 不同上下文/角色返回不同脱敏结果。
pub struct ContextAwareMasker {
    rules: Vec<MaskingRuleConfig>,
}

impl ContextAwareMasker {
    pub fn new(rules: Vec<MaskingRuleConfig>) -> Self {
        Self { rules }
    }

    /// 根据上下文脱敏
    ///
    /// - 管理员 + 内部调用：不脱敏
    /// - 出站数据：按规则脱敏
    /// - 入站数据：不脱敏（写入原始值，读取时脱敏）
    pub fn mask_with_context(
        &self,
        value: &str,
        field_name: &str,
        context: &MaskingContext,
    ) -> String {
        if context.is_admin() {
            return value.to_string();
        }
        if !context.is_outbound() {
            return value.to_string();
        }

        let rule = self
            .rules
            .iter()
            .filter(|r| r.field_name == field_name)
            .min_by_key(|r| r.priority);

        match rule {
            Some(r) => apply_strategy(value, &r.strategy),
            None => value.to_string(),
        }
    }
}

/// v7.6.0 热更新协调器
///
/// 原子热更新脱敏策略（无需重启），新策略对后续查询生效，
/// 旧查询继续用旧策略。热更新与正在执行的查询冲突时采用原子切换。
pub struct HotUpdateCoordinator {
    current_rules: Arc<RwLock<Vec<MaskingRuleConfig>>>,
}

impl HotUpdateCoordinator {
    pub fn new(initial_rules: Vec<MaskingRuleConfig>) -> Self {
        Self {
            current_rules: Arc::new(RwLock::new(initial_rules)),
        }
    }

    /// 原子热更新脱敏策略
    pub fn hot_update(&self, new_rules: Vec<MaskingRuleConfig>) -> Result<(), MaskingError> {
        let mut conflicts = Vec::new();
        for (i, rule_i) in new_rules.iter().enumerate() {
            for (j, rule_j) in new_rules.iter().enumerate() {
                if i < j
                    && rule_i.field_name == rule_j.field_name
                    && rule_i.strategy != rule_j.strategy
                {
                    conflicts.push(format!(
                        "字段 {} 存在冲突策略（优先级 {} vs {}）",
                        rule_i.field_name, rule_i.priority, rule_j.priority
                    ));
                }
            }
        }

        if !conflicts.is_empty() {
            eprintln!("MASKING_HOT_UPDATE_CONFLICT: {} 个冲突", conflicts.len());
            return Err(MaskingError::HotUpdateConflict(conflicts.join("; ")));
        }

        let mut current = self
            .current_rules
            .write()
            .map_err(|_| MaskingError::HotUpdateConflict("锁中毒".to_string()))?;
        *current = new_rules;
        Ok(())
    }

    /// 获取当前规则快照
    pub fn current_rules(&self) -> Vec<MaskingRuleConfig> {
        self.current_rules
            .read()
            .map(|r| r.clone())
            .unwrap_or_default()
    }

    /// 创建使用当前规则的 ContextAwareMasker
    pub fn create_masker(&self) -> ContextAwareMasker {
        ContextAwareMasker::new(self.current_rules())
    }
}

#[cfg(test)]
mod v760_context_aware_tests {
    use super::*;

    fn make_rules() -> Vec<MaskingRuleConfig> {
        vec![
            MaskingRuleConfig::new("phone", MaskingStrategy::Mask(MaskConfig::new(3, 4))),
            MaskingRuleConfig::new("email", MaskingStrategy::Hash(HashConfig::default())),
        ]
    }

    #[test]
    fn test_context_aware_admin_no_mask() {
        let masker = ContextAwareMasker::new(make_rules());
        let ctx = MaskingContext::new("admin", "select", DataFlow::Outbound);
        let result = masker.mask_with_context("13800138000", "phone", &ctx);
        assert_eq!(result, "13800138000");
    }

    #[test]
    fn test_context_aware_user_masked() {
        let masker = ContextAwareMasker::new(make_rules());
        let ctx = MaskingContext::new("user", "select", DataFlow::Outbound);
        let result = masker.mask_with_context("13800138000", "phone", &ctx);
        assert_ne!(result, "13800138000");
    }

    #[test]
    fn test_context_aware_inbound_no_mask() {
        let masker = ContextAwareMasker::new(make_rules());
        let ctx = MaskingContext::new("user", "insert", DataFlow::Inbound);
        let result = masker.mask_with_context("13800138000", "phone", &ctx);
        assert_eq!(result, "13800138000");
    }

    #[test]
    fn test_context_aware_no_rule() {
        let masker = ContextAwareMasker::new(make_rules());
        let ctx = MaskingContext::new("user", "select", DataFlow::Outbound);
        let result = masker.mask_with_context("value", "unknown_field", &ctx);
        assert_eq!(result, "value");
    }

    #[test]
    fn test_hot_update_success() {
        let coord = HotUpdateCoordinator::new(make_rules());
        let new_rules = vec![MaskingRuleConfig::new(
            "phone",
            MaskingStrategy::Mask(MaskConfig::default()),
        )];
        let result = coord.hot_update(new_rules);
        assert!(result.is_ok());
        assert_eq!(coord.current_rules().len(), 1);
    }

    #[test]
    fn test_hot_update_conflict() {
        let coord = HotUpdateCoordinator::new(vec![]);
        let new_rules = vec![
            MaskingRuleConfig::new("phone", MaskingStrategy::Mask(MaskConfig::default())),
            MaskingRuleConfig::new("phone", MaskingStrategy::Hash(HashConfig::default())),
        ];
        let result = coord.hot_update(new_rules);
        assert!(matches!(result, Err(MaskingError::HotUpdateConflict(_))));
    }

    #[test]
    fn test_hot_update_create_masker() {
        let coord = HotUpdateCoordinator::new(make_rules());
        let masker = coord.create_masker();
        let ctx = MaskingContext::new("user", "select", DataFlow::Outbound);
        let result = masker.mask_with_context("13800138000", "phone", &ctx);
        assert_ne!(result, "13800138000");
    }

    #[test]
    fn test_data_flow_enum() {
        assert_ne!(DataFlow::Inbound, DataFlow::Outbound);
        assert_ne!(DataFlow::Outbound, DataFlow::Internal);
    }
}
