//! 脱敏配置管理与敏感字段自动检测。
//!
//! - [`MaskingConfigManager`] — 管理多套脱敏配置档案（按场景/租户隔离）
//! - [`MaskingProfile`] — 单套脱敏配置档案
//! - [`SensitiveFieldDetector`] — 基于字段名模式自动检测敏感字段并推荐脱敏规则

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::MaskingRule;

// ============================================================================
// 脱敏配置档案
// ============================================================================

/// 脱敏配置档案：一套命名的字段脱敏规则集合。
///
/// 用于多租户/多场景隔离，例如 `default`、`strict`、`gdpr` 等不同脱敏强度。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaskingProfile {
    name: String,
    description: String,
    rules: HashMap<String, MaskingRule>,
    enabled: bool,
}

impl MaskingProfile {
    /// 创建空档案
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            description: String::new(),
            rules: HashMap::new(),
            enabled: true,
        }
    }

    /// 创建档案并设置描述
    pub fn with_description(name: &str, description: &str) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
            rules: HashMap::new(),
            enabled: true,
        }
    }

    /// 档案名
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 描述
    pub fn description(&self) -> &str {
        &self.description
    }

    /// 是否启用
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// 设置启用状态（链式）
    pub fn set_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// 添加字段规则（链式）
    pub fn with_rule(mut self, field: &str, rule: MaskingRule) -> Self {
        self.rules.insert(field.to_string(), rule);
        self
    }

    /// 添加多条规则（链式）
    pub fn with_rules(mut self, rules: HashMap<String, MaskingRule>) -> Self {
        self.rules.extend(rules);
        self
    }

    /// 规则数
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// 获取字段规则
    pub fn get_rule(&self, field: &str) -> Option<&MaskingRule> {
        self.rules.get(field)
    }

    /// 移除字段规则
    pub fn remove_rule(&mut self, field: &str) -> Option<MaskingRule> {
        self.rules.remove(field)
    }

    /// 所有规则引用
    pub fn rules(&self) -> &HashMap<String, MaskingRule> {
        &self.rules
    }

    /// 对 HashMap 应用本档案脱敏
    pub fn apply_to_map(&self, data: &HashMap<String, String>) -> HashMap<String, String> {
        if !self.enabled {
            return data.clone();
        }
        crate::DataMasker::mask_map(&self.rules, data)
    }

    /// 对 JSON 应用本档案脱敏
    pub fn apply_to_json(&self, json: &str) -> String {
        if !self.enabled {
            return json.to_string();
        }
        crate::DataMasker::mask_json(&self.rules, json)
    }
}

// ============================================================================
// 脱敏配置管理器
// ============================================================================

/// 脱敏配置管理器：管理多套脱敏档案，按名称切换。
///
/// 支持默认档案、按名称获取档案、合并档案等操作。
#[derive(Debug, Clone, Default)]
pub struct MaskingConfigManager {
    profiles: HashMap<String, MaskingProfile>,
    default_profile: String,
}

impl MaskingConfigManager {
    /// 创建空管理器
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册档案（链式）
    pub fn add_profile(mut self, profile: MaskingProfile) -> Self {
        let name = profile.name().to_string();
        if self.profiles.is_empty() {
            self.default_profile = name.clone();
        }
        self.profiles.insert(name, profile);
        self
    }

    /// 设置默认档案名
    pub fn set_default(&mut self, name: &str) -> bool {
        if self.profiles.contains_key(name) {
            self.default_profile = name.to_string();
            true
        } else {
            false
        }
    }

    /// 默认档案名
    pub fn default_profile_name(&self) -> &str {
        &self.default_profile
    }

    /// 获取档案
    pub fn get_profile(&self, name: &str) -> Option<&MaskingProfile> {
        self.profiles.get(name)
    }

    /// 获取默认档案
    pub fn default_profile(&self) -> Option<&MaskingProfile> {
        self.profiles.get(&self.default_profile)
    }

    /// 档案数
    pub fn profile_count(&self) -> usize {
        self.profiles.len()
    }

    /// 所有档案名
    pub fn profile_names(&self) -> Vec<&str> {
        self.profiles.keys().map(|s| s.as_str()).collect()
    }

    /// 移除档案
    pub fn remove_profile(&mut self, name: &str) -> Option<MaskingProfile> {
        let removed = self.profiles.remove(name);
        if removed.is_some() && self.default_profile == name {
            self.default_profile = self.profiles.keys().next().cloned().unwrap_or_default();
        }
        removed
    }

    /// 用指定档案脱敏 HashMap
    pub fn apply_to_map(
        &self,
        profile_name: &str,
        data: &HashMap<String, String>,
    ) -> Option<HashMap<String, String>> {
        self.profiles
            .get(profile_name)
            .map(|p| p.apply_to_map(data))
    }

    /// 用默认档案脱敏 HashMap
    pub fn apply_with_default(
        &self,
        data: &HashMap<String, String>,
    ) -> Option<HashMap<String, String>> {
        self.default_profile().map(|p| p.apply_to_map(data))
    }

    /// 用指定档案脱敏 JSON
    pub fn apply_to_json(&self, profile_name: &str, json: &str) -> Option<String> {
        self.profiles
            .get(profile_name)
            .map(|p| p.apply_to_json(json))
    }

    /// 合并两个档案：将 `source` 的规则补充到 `target`（不覆盖已有规则）
    pub fn merge_profiles(&self, target: &str, source: &str) -> Option<MaskingProfile> {
        let target_profile = self.profiles.get(target)?;
        let source_profile = self.profiles.get(source)?;
        let mut merged = target_profile.clone();
        for (field, rule) in source_profile.rules() {
            if !merged.rules().contains_key(field) {
                merged = merged.with_rule(field, rule.clone());
            }
        }
        Some(merged)
    }
}

// ============================================================================
// 敏感字段检测器
// ============================================================================

/// 字段名匹配模式：关键词列表 + 对应脱敏规则
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldPattern {
    keywords: Vec<String>,
    rule: MaskingRule,
}

impl FieldPattern {
    /// 创建字段模式
    pub fn new(keywords: Vec<String>, rule: MaskingRule) -> Self {
        Self { keywords, rule }
    }

    /// 关键词列表
    pub fn keywords(&self) -> &[String] {
        &self.keywords
    }

    /// 脱敏规则
    pub fn rule(&self) -> &MaskingRule {
        &self.rule
    }

    /// 检查字段名是否匹配本模式（大小写不敏感，包含任一关键词即匹配）
    pub fn matches(&self, field: &str) -> bool {
        let field_lower = field.to_lowercase();
        self.keywords
            .iter()
            .any(|kw| field_lower.contains(&kw.to_lowercase()))
    }
}

/// 敏感字段检测器：基于字段名模式自动检测敏感字段并推荐脱敏规则。
///
/// 内置常见敏感字段模式（phone/mobile/tel、email/mail、idcard/identity、
/// bankcard/card、password/pwd、name、address/addr、ip 等），
/// 也支持自定义模式。
#[derive(Debug, Clone, Default)]
pub struct SensitiveFieldDetector {
    patterns: Vec<FieldPattern>,
}

impl SensitiveFieldDetector {
    /// 创建包含内置模式的检测器
    pub fn new() -> Self {
        Self {
            patterns: Self::builtin_patterns(),
        }
    }

    /// 创建空检测器
    pub fn empty() -> Self {
        Self::default()
    }

    /// 添加自定义模式（链式）
    pub fn add_pattern(mut self, pattern: FieldPattern) -> Self {
        self.patterns.push(pattern);
        self
    }

    /// 模式数
    pub fn pattern_count(&self) -> usize {
        self.patterns.len()
    }

    /// 检测字段名对应的脱敏规则
    pub fn detect(&self, field: &str) -> Option<&MaskingRule> {
        self.patterns
            .iter()
            .find(|p| p.matches(field))
            .map(|p| p.rule())
    }

    /// 批量检测字段名，返回 (字段名, 规则) 列表
    pub fn detect_fields(&self, fields: &[String]) -> Vec<(String, &MaskingRule)> {
        fields
            .iter()
            .filter_map(|f| self.detect(f).map(|r| (f.clone(), r)))
            .collect()
    }

    /// 为字段列表自动生成脱敏规则 HashMap
    pub fn auto_rules(&self, fields: &[String]) -> HashMap<String, MaskingRule> {
        fields
            .iter()
            .filter_map(|f| self.detect(f).map(|r| (f.clone(), r.clone())))
            .collect()
    }

    /// 对 HashMap 自动检测并脱敏
    pub fn auto_mask(&self, data: &HashMap<String, String>) -> HashMap<String, String> {
        let fields: Vec<String> = data.keys().cloned().collect();
        let rules = self.auto_rules(&fields);
        crate::DataMasker::mask_map(&rules, data)
    }

    /// 内置敏感字段模式
    fn builtin_patterns() -> Vec<FieldPattern> {
        vec![
            FieldPattern::new(
                vec![
                    "phone".to_string(),
                    "mobile".to_string(),
                    "tel".to_string(),
                    "telephone".to_string(),
                ],
                MaskingRule::Phone,
            ),
            FieldPattern::new(
                vec![
                    "email".to_string(),
                    "mail".to_string(),
                    "email_addr".to_string(),
                ],
                MaskingRule::Email,
            ),
            FieldPattern::new(
                vec![
                    "idcard".to_string(),
                    "identity".to_string(),
                    "id_number".to_string(),
                    "id_card".to_string(),
                ],
                MaskingRule::IdCard,
            ),
            FieldPattern::new(
                vec![
                    "bankcard".to_string(),
                    "bank_card".to_string(),
                    "card_number".to_string(),
                    "cardno".to_string(),
                ],
                MaskingRule::BankCard,
            ),
            FieldPattern::new(
                vec![
                    "password".to_string(),
                    "pwd".to_string(),
                    "passwd".to_string(),
                    "secret".to_string(),
                ],
                MaskingRule::Password,
            ),
            FieldPattern::new(
                vec![
                    "apikey".to_string(),
                    "api_key".to_string(),
                    "access_token".to_string(),
                    "auth_token".to_string(),
                ],
                MaskingRule::ApiKey,
            ),
            FieldPattern::new(
                vec![
                    "name".to_string(),
                    "username".to_string(),
                    "fullname".to_string(),
                ],
                MaskingRule::Name,
            ),
            FieldPattern::new(
                vec![
                    "ip".to_string(),
                    "ip_addr".to_string(),
                    "ipaddress".to_string(),
                ],
                MaskingRule::Ip,
            ),
            FieldPattern::new(
                vec![
                    "address".to_string(),
                    "addr".to_string(),
                    "home_addr".to_string(),
                ],
                MaskingRule::Address,
            ),
            FieldPattern::new(
                vec!["imei".to_string(), "device_imei".to_string()],
                MaskingRule::Imei,
            ),
            FieldPattern::new(
                vec![
                    "plate".to_string(),
                    "license_plate".to_string(),
                    "car_plate".to_string(),
                ],
                MaskingRule::Plate,
            ),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ----- MaskingProfile -----

    #[test]
    fn profile_new() {
        let p = MaskingProfile::new("default");
        assert_eq!(p.name(), "default");
        assert_eq!(p.description(), "");
        assert!(p.is_enabled());
        assert_eq!(p.rule_count(), 0);
    }

    #[test]
    fn profile_with_description() {
        let p = MaskingProfile::with_description("strict", "Strict GDPR masking");
        assert_eq!(p.description(), "Strict GDPR masking");
    }

    #[test]
    fn profile_with_rule() {
        let p = MaskingProfile::new("default").with_rule("phone", MaskingRule::Phone);
        assert_eq!(p.rule_count(), 1);
        assert!(p.get_rule("phone").is_some());
    }

    #[test]
    fn profile_with_rules_batch() {
        let mut rules = HashMap::new();
        rules.insert("phone".to_string(), MaskingRule::Phone);
        rules.insert("email".to_string(), MaskingRule::Email);
        let p = MaskingProfile::new("default").with_rules(rules);
        assert_eq!(p.rule_count(), 2);
    }

    #[test]
    fn profile_set_disabled() {
        let p = MaskingProfile::new("default").set_enabled(false);
        assert!(!p.is_enabled());
    }

    #[test]
    fn profile_remove_rule() {
        let mut p = MaskingProfile::new("default").with_rule("phone", MaskingRule::Phone);
        let removed = p.remove_rule("phone");
        assert!(removed.is_some());
        assert_eq!(p.rule_count(), 0);
    }

    #[test]
    fn profile_apply_to_map() {
        let p = MaskingProfile::new("default").with_rule("phone", MaskingRule::Phone);
        let mut data = HashMap::new();
        data.insert("phone".to_string(), "13812345678".to_string());
        let result = p.apply_to_map(&data);
        assert_eq!(result["phone"], "138****5678");
    }

    #[test]
    fn profile_apply_to_map_disabled() {
        let p = MaskingProfile::new("default")
            .with_rule("phone", MaskingRule::Phone)
            .set_enabled(false);
        let mut data = HashMap::new();
        data.insert("phone".to_string(), "13812345678".to_string());
        let result = p.apply_to_map(&data);
        assert_eq!(result["phone"], "13812345678");
    }

    #[test]
    fn profile_apply_to_json() {
        let p = MaskingProfile::new("default").with_rule("phone", MaskingRule::Phone);
        let json = r#"{"phone":"13812345678"}"#;
        let result = p.apply_to_json(json);
        assert!(result.contains("138****5678"));
    }

    #[test]
    fn profile_apply_to_json_disabled() {
        let p = MaskingProfile::new("default")
            .with_rule("phone", MaskingRule::Phone)
            .set_enabled(false);
        let json = r#"{"phone":"13812345678"}"#;
        assert_eq!(p.apply_to_json(json), json);
    }

    // ----- MaskingConfigManager -----

    #[test]
    fn config_manager_default_empty() {
        let m = MaskingConfigManager::new();
        assert_eq!(m.profile_count(), 0);
    }

    #[test]
    fn config_manager_add_profile() {
        let m = MaskingConfigManager::new().add_profile(MaskingProfile::new("default"));
        assert_eq!(m.profile_count(), 1);
        assert_eq!(m.default_profile_name(), "default");
    }

    #[test]
    fn config_manager_first_profile_is_default() {
        let m = MaskingConfigManager::new()
            .add_profile(MaskingProfile::new("first"))
            .add_profile(MaskingProfile::new("second"));
        assert_eq!(m.default_profile_name(), "first");
    }

    #[test]
    fn config_manager_set_default() {
        let mut m = MaskingConfigManager::new()
            .add_profile(MaskingProfile::new("a"))
            .add_profile(MaskingProfile::new("b"));
        assert!(m.set_default("b"));
        assert_eq!(m.default_profile_name(), "b");
    }

    #[test]
    fn config_manager_set_default_nonexistent() {
        let mut m = MaskingConfigManager::new().add_profile(MaskingProfile::new("a"));
        assert!(!m.set_default("nonexistent"));
    }

    #[test]
    fn config_manager_get_profile() {
        let m = MaskingConfigManager::new().add_profile(MaskingProfile::new("default"));
        assert!(m.get_profile("default").is_some());
        assert!(m.get_profile("nonexistent").is_none());
    }

    #[test]
    fn config_manager_remove_profile() {
        let mut m = MaskingConfigManager::new()
            .add_profile(MaskingProfile::new("a"))
            .add_profile(MaskingProfile::new("b"));
        let removed = m.remove_profile("a");
        assert!(removed.is_some());
        assert_eq!(m.profile_count(), 1);
        // 默认切换到剩余档案
        assert_eq!(m.default_profile_name(), "b");
    }

    #[test]
    fn config_manager_apply_to_map() {
        let m = MaskingConfigManager::new()
            .add_profile(MaskingProfile::new("default").with_rule("phone", MaskingRule::Phone));
        let mut data = HashMap::new();
        data.insert("phone".to_string(), "13812345678".to_string());
        let result = m.apply_to_map("default", &data).unwrap();
        assert_eq!(result["phone"], "138****5678");
    }

    #[test]
    fn config_manager_apply_with_default() {
        let m = MaskingConfigManager::new()
            .add_profile(MaskingProfile::new("default").with_rule("phone", MaskingRule::Phone));
        let mut data = HashMap::new();
        data.insert("phone".to_string(), "13812345678".to_string());
        let result = m.apply_with_default(&data).unwrap();
        assert_eq!(result["phone"], "138****5678");
    }

    #[test]
    fn config_manager_apply_to_json() {
        let m = MaskingConfigManager::new()
            .add_profile(MaskingProfile::new("default").with_rule("phone", MaskingRule::Phone));
        let json = r#"{"phone":"13812345678"}"#;
        let result = m.apply_to_json("default", json).unwrap();
        assert!(result.contains("138****5678"));
    }

    #[test]
    fn config_manager_merge_profiles() {
        let m = MaskingConfigManager::new()
            .add_profile(MaskingProfile::new("base").with_rule("phone", MaskingRule::Phone))
            .add_profile(MaskingProfile::new("extra").with_rule("email", MaskingRule::Email));
        let merged = m.merge_profiles("base", "extra").unwrap();
        assert_eq!(merged.rule_count(), 2);
    }

    #[test]
    fn config_manager_profile_names() {
        let m = MaskingConfigManager::new()
            .add_profile(MaskingProfile::new("a"))
            .add_profile(MaskingProfile::new("b"));
        let names = m.profile_names();
        assert_eq!(names.len(), 2);
    }

    // ----- FieldPattern -----

    #[test]
    fn field_pattern_matches() {
        let p = FieldPattern::new(
            vec!["phone".to_string(), "mobile".to_string()],
            MaskingRule::Phone,
        );
        assert!(p.matches("phone_number"));
        assert!(p.matches("user_mobile"));
        assert!(!p.matches("email"));
    }

    #[test]
    fn field_pattern_case_insensitive() {
        let p = FieldPattern::new(vec!["phone".to_string()], MaskingRule::Phone);
        assert!(p.matches("PHONE"));
        assert!(p.matches("Phone"));
    }

    #[test]
    fn field_pattern_keywords() {
        let p = FieldPattern::new(
            vec!["phone".to_string(), "tel".to_string()],
            MaskingRule::Phone,
        );
        assert_eq!(p.keywords().len(), 2);
    }

    // ----- SensitiveFieldDetector -----

    #[test]
    fn detector_new_has_builtin_patterns() {
        let d = SensitiveFieldDetector::new();
        assert!(d.pattern_count() > 0);
    }

    #[test]
    fn detector_empty() {
        let d = SensitiveFieldDetector::empty();
        assert_eq!(d.pattern_count(), 0);
    }

    #[test]
    fn detector_add_pattern() {
        let d = SensitiveFieldDetector::empty().add_pattern(FieldPattern::new(
            vec!["custom".to_string()],
            MaskingRule::Password,
        ));
        assert_eq!(d.pattern_count(), 1);
    }

    #[test]
    fn detector_phone() {
        let d = SensitiveFieldDetector::new();
        let rule = d.detect("phone_number").unwrap();
        assert_eq!(rule, &MaskingRule::Phone);
    }

    #[test]
    fn detector_mobile() {
        let d = SensitiveFieldDetector::new();
        assert!(d.detect("user_mobile").is_some());
    }

    #[test]
    fn detector_email() {
        let d = SensitiveFieldDetector::new();
        let rule = d.detect("email_addr").unwrap();
        assert_eq!(rule, &MaskingRule::Email);
    }

    #[test]
    fn detector_password() {
        let d = SensitiveFieldDetector::new();
        let rule = d.detect("user_password").unwrap();
        assert_eq!(rule, &MaskingRule::Password);
    }

    #[test]
    fn detector_idcard() {
        let d = SensitiveFieldDetector::new();
        let rule = d.detect("id_card").unwrap();
        assert_eq!(rule, &MaskingRule::IdCard);
    }

    #[test]
    fn detector_bankcard() {
        let d = SensitiveFieldDetector::new();
        let rule = d.detect("bank_card_no").unwrap();
        assert_eq!(rule, &MaskingRule::BankCard);
    }

    #[test]
    fn detector_apikey() {
        let d = SensitiveFieldDetector::new();
        let rule = d.detect("api_key").unwrap();
        assert_eq!(rule, &MaskingRule::ApiKey);
    }

    #[test]
    fn detector_name() {
        let d = SensitiveFieldDetector::new();
        let rule = d.detect("username").unwrap();
        assert_eq!(rule, &MaskingRule::Name);
    }

    #[test]
    fn detector_address() {
        let d = SensitiveFieldDetector::new();
        let rule = d.detect("home_address").unwrap();
        assert_eq!(rule, &MaskingRule::Address);
    }

    #[test]
    fn detector_ip() {
        let d = SensitiveFieldDetector::new();
        let rule = d.detect("ip_addr").unwrap();
        assert_eq!(rule, &MaskingRule::Ip);
    }

    #[test]
    fn detector_imei() {
        let d = SensitiveFieldDetector::new();
        let rule = d.detect("device_imei").unwrap();
        assert_eq!(rule, &MaskingRule::Imei);
    }

    #[test]
    fn detector_plate() {
        let d = SensitiveFieldDetector::new();
        let rule = d.detect("license_plate").unwrap();
        assert_eq!(rule, &MaskingRule::Plate);
    }

    #[test]
    fn detector_non_sensitive() {
        let d = SensitiveFieldDetector::new();
        assert!(d.detect("created_at").is_none());
        assert!(d.detect("order_id").is_none());
    }

    #[test]
    fn detector_detect_fields() {
        let d = SensitiveFieldDetector::new();
        let fields = vec![
            "phone".to_string(),
            "email".to_string(),
            "created_at".to_string(),
        ];
        let detected = d.detect_fields(&fields);
        assert_eq!(detected.len(), 2);
    }

    #[test]
    fn detector_auto_rules() {
        let d = SensitiveFieldDetector::new();
        let fields = vec![
            "phone".to_string(),
            "email".to_string(),
            "created_at".to_string(),
        ];
        let rules = d.auto_rules(&fields);
        assert_eq!(rules.len(), 2);
        assert!(rules.contains_key("phone"));
        assert!(rules.contains_key("email"));
    }

    #[test]
    fn detector_auto_mask() {
        let d = SensitiveFieldDetector::new();
        let mut data = HashMap::new();
        data.insert("phone".to_string(), "13812345678".to_string());
        data.insert("email".to_string(), "test@example.com".to_string());
        data.insert("created_at".to_string(), "2024-01-01".to_string());
        let result = d.auto_mask(&data);
        assert_eq!(result["phone"], "138****5678");
        assert_eq!(result["email"], "t***@example.com");
        assert_eq!(result["created_at"], "2024-01-01");
    }
}
