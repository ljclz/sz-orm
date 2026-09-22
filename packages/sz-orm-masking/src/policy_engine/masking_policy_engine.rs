//! 脱敏策略引擎
//!
//! 复用 `HotUpdateCoordinator` 进行原子热更新。
//! 按角色/字段/场景细粒度脱敏，策略冲突按优先级仲裁。

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::dynamic_masking::{
    apply_strategy, HotUpdateCoordinator, MaskConfig, MaskingContext, MaskingRuleConfig,
    MaskingStrategy,
};

/// 脱敏场景
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MaskingScene {
    /// API 出站响应
    ApiResponse,
    /// 日志记录
    Logging,
    /// 报表导出
    ReportExport,
    /// 管理后台展示
    AdminDisplay,
}

impl MaskingScene {
    /// 是否需要脱敏（管理后台默认不脱敏）
    pub fn needs_masking(&self) -> bool {
        !matches!(self, MaskingScene::AdminDisplay)
    }
}

/// 脱敏策略种类（简化版，映射到 MaskingStrategy）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MaskingStrategyKind {
    /// 掩码（保留前缀/后缀）
    Mask {
        keep_prefix: usize,
        keep_suffix: usize,
    },
    /// 哈希（不可逆）
    Hash,
    /// 替换为固定值
    Replace(String),
}

impl MaskingStrategyKind {
    /// 转换为底层 MaskingStrategy
    fn to_strategy(&self) -> MaskingStrategy {
        match self {
            Self::Mask {
                keep_prefix,
                keep_suffix,
            } => MaskingStrategy::Mask(MaskConfig::new(*keep_prefix, *keep_suffix)),
            Self::Hash => MaskingStrategy::Hash(crate::dynamic_masking::HashConfig::default()),
            Self::Replace(s) => MaskingStrategy::Replace(s.clone()),
        }
    }

    /// 是否不可逆
    pub fn is_irreversible(&self) -> bool {
        matches!(self, Self::Mask { .. } | Self::Hash | Self::Replace(_))
    }
}

/// 脱敏策略：按角色/字段/场景匹配
#[derive(Debug, Clone)]
pub struct MaskingPolicy {
    /// 策略 ID
    pub policy_id: String,
    /// 适用角色（空表示所有角色）
    pub role: String,
    /// 适用字段
    pub field: String,
    /// 适用场景
    pub scene: MaskingScene,
    /// 脱敏策略
    pub strategy: MaskingStrategyKind,
    /// 优先级（数值越小优先级越高）
    pub priority: u32,
}

impl MaskingPolicy {
    /// 创建脱敏策略
    pub fn new(
        policy_id: &str,
        role: &str,
        field: &str,
        scene: MaskingScene,
        strategy: MaskingStrategyKind,
    ) -> Self {
        Self {
            policy_id: policy_id.to_string(),
            role: role.to_string(),
            field: field.to_string(),
            scene,
            strategy,
            priority: 100,
        }
    }

    /// 设置优先级
    pub fn with_priority(mut self, priority: u32) -> Self {
        self.priority = priority;
        self
    }

    /// 是否匹配给定上下文
    pub fn matches(&self, role: &str, field: &str, scene: &MaskingScene) -> bool {
        let role_match = self.role.is_empty() || self.role == role;
        role_match && self.field == field && &self.scene == scene
    }
}

/// 脱敏策略引擎配置
#[derive(Debug, Clone)]
pub struct MaskingPolicyConfig {
    /// 热更新 SLA（默认 10s）
    pub hot_reload_sla_secs: u64,
}

impl Default for MaskingPolicyConfig {
    fn default() -> Self {
        Self {
            hot_reload_sla_secs: 10,
        }
    }
}

impl MaskingPolicyConfig {
    pub fn new() -> Self {
        Self::default()
    }
}

/// 脱敏策略引擎
///
/// 注入 `Arc<HotUpdateCoordinator>` 支持底层原子热更新。
/// `mask()` 按角色/字段/场景匹配策略，冲突时按优先级仲裁。
/// `hot_reload()` 原子替换策略集，同步底层规则，≤ 10s 生效。
pub struct MaskingPolicyEngine {
    policies: RwLock<Vec<MaskingPolicy>>,
    hot_updater: Arc<HotUpdateCoordinator>,
    config: MaskingPolicyConfig,
    /// 策略冲突计数
    conflict_count: RwLock<u64>,
}

impl MaskingPolicyEngine {
    /// 创建脱敏策略引擎
    pub fn new(
        initial_policies: Vec<MaskingPolicy>,
        hot_updater: Arc<HotUpdateCoordinator>,
        config: MaskingPolicyConfig,
    ) -> Self {
        Self {
            policies: RwLock::new(initial_policies),
            hot_updater,
            config,
            conflict_count: RwLock::new(0),
        }
    }

    /// 对值进行脱敏
    ///
    /// 策略匹配 → 按优先级仲裁 → 脱敏。无匹配策略时返回原值。
    /// 管理后台场景默认不脱敏。
    pub fn mask(&self, value: &str, context: &MaskingContext) -> Result<String, PolicyEngineError> {
        let role = &context.user_role;
        let field = &context.query_context;
        let scene = if context.is_admin() {
            MaskingScene::AdminDisplay
        } else if context.is_outbound() {
            MaskingScene::ApiResponse
        } else {
            MaskingScene::Logging
        };

        if !scene.needs_masking() {
            return Ok(value.to_string());
        }

        let policies = self.policies.read().expect("policies lock");
        let candidates: Vec<&MaskingPolicy> = policies
            .iter()
            .filter(|p| p.matches(role, field, &scene))
            .collect();

        if candidates.is_empty() {
            return Ok(value.to_string());
        }

        let mut sorted = candidates;
        sorted.sort_by_key(|p| p.priority);

        if sorted.len() > 1 {
            *self.conflict_count.write().expect("conflict_count lock") += 1;
        }

        let effective = sorted[0];
        let strategy = effective.strategy.to_strategy();
        let masked = apply_strategy(value, &strategy);
        Ok(masked)
    }

    /// 热更新策略集
    ///
    /// 原子替换全部策略，同步底层有效规则到 HotUpdateCoordinator，≤ 10s 生效。
    /// 底层同步按字段去重（取最高优先级），避免同字段多策略冲突。
    pub fn hot_reload(&self, policies: &[MaskingPolicy]) -> Result<(), PolicyEngineError> {
        // 按字段去重，取最高优先级策略，同步到底层 HotUpdateCoordinator
        let mut field_best: HashMap<String, &MaskingPolicy> = HashMap::new();
        for p in policies {
            match field_best.get(&p.field) {
                Some(existing) if existing.priority <= p.priority => {}
                _ => {
                    field_best.insert(p.field.clone(), p);
                }
            }
        }
        let rules: Vec<MaskingRuleConfig> = field_best
            .values()
            .map(|p| {
                MaskingRuleConfig::new(&p.field, p.strategy.to_strategy()).with_priority(p.priority)
            })
            .collect();
        self.hot_updater
            .hot_update(rules)
            .map_err(|e| PolicyEngineError::HotReloadFailed(format!("{:?}", e)))?;

        *self.policies.write().expect("policies lock") = policies.to_vec();
        Ok(())
    }

    /// 当前策略数
    pub fn policy_count(&self) -> usize {
        self.policies.read().expect("policies lock").len()
    }

    /// 策略冲突次数
    pub fn conflict_count(&self) -> u64 {
        *self.conflict_count.read().expect("conflict_count lock")
    }

    /// 配置引用
    pub fn config(&self) -> &MaskingPolicyConfig {
        &self.config
    }

    /// 底层热更新协调器引用
    pub fn hot_updater(&self) -> &Arc<HotUpdateCoordinator> {
        &self.hot_updater
    }
}

/// 策略引擎错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyEngineError {
    /// 热更新失败
    HotReloadFailed(String),
    /// 策略冲突
    StrategyConflict(String),
}

impl std::fmt::Display for PolicyEngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::HotReloadFailed(msg) => write!(f, "Hot reload failed: {}", msg),
            Self::StrategyConflict(msg) => write!(f, "MASKING_POLICY_CONFLICT: {}", msg),
        }
    }
}

impl std::error::Error for PolicyEngineError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dynamic_masking::DataFlow;

    fn make_engine() -> MaskingPolicyEngine {
        let policies = vec![
            MaskingPolicy::new(
                "p1",
                "viewer",
                "phone",
                MaskingScene::ApiResponse,
                MaskingStrategyKind::Mask {
                    keep_prefix: 3,
                    keep_suffix: 4,
                },
            )
            .with_priority(1),
            MaskingPolicy::new(
                "p2",
                "viewer",
                "email",
                MaskingScene::ApiResponse,
                MaskingStrategyKind::Hash,
            )
            .with_priority(1),
        ];
        let hot_updater = Arc::new(HotUpdateCoordinator::new(vec![]));
        MaskingPolicyEngine::new(policies, hot_updater, MaskingPolicyConfig::new())
    }

    #[test]
    fn role_a_field_x_masked_other_role_original() {
        let engine = make_engine();
        let value = "13812345678";

        let ctx_viewer = MaskingContext::new("viewer", "phone", DataFlow::Outbound);
        let masked = engine.mask(value, &ctx_viewer).unwrap();
        assert_ne!(masked, value);
        assert!(masked.starts_with("138"));
        assert!(masked.ends_with("5678"));

        let ctx_admin = MaskingContext::new("admin", "phone", DataFlow::Outbound);
        let original = engine.mask(value, &ctx_admin).unwrap();
        assert_eq!(original, value);
    }

    #[test]
    fn hot_reload_10s_effective() {
        let engine = make_engine();
        assert_eq!(engine.policy_count(), 2);

        let new_policies = vec![MaskingPolicy::new(
            "p3",
            "guest",
            "ssn",
            MaskingScene::ApiResponse,
            MaskingStrategyKind::Replace("***".to_string()),
        )];
        engine.hot_reload(&new_policies).unwrap();
        assert_eq!(engine.policy_count(), 1);

        let ctx = MaskingContext::new("guest", "ssn", DataFlow::Outbound);
        let masked = engine.mask("123-45-6789", &ctx).unwrap();
        assert_eq!(masked, "***");
    }

    #[test]
    fn strategy_conflict_resolved_by_priority() {
        let policies = vec![
            MaskingPolicy::new(
                "p1",
                "viewer",
                "phone",
                MaskingScene::ApiResponse,
                MaskingStrategyKind::Mask {
                    keep_prefix: 3,
                    keep_suffix: 4,
                },
            )
            .with_priority(1),
            MaskingPolicy::new(
                "p2",
                "viewer",
                "phone",
                MaskingScene::ApiResponse,
                MaskingStrategyKind::Replace("REDACTED".to_string()),
            )
            .with_priority(10),
        ];
        let hot_updater = Arc::new(HotUpdateCoordinator::new(vec![]));
        let engine = MaskingPolicyEngine::new(policies, hot_updater, MaskingPolicyConfig::new());

        let ctx = MaskingContext::new("viewer", "phone", DataFlow::Outbound);
        let masked = engine.mask("13812345678", &ctx).unwrap();
        assert!(masked.starts_with("138"));
        assert!(masked.ends_with("5678"));
        assert_ne!(masked, "REDACTED");
        assert_eq!(engine.conflict_count(), 1);
    }

    #[test]
    fn irreversible_masking() {
        let engine = make_engine();
        let ctx = MaskingContext::new("viewer", "phone", DataFlow::Outbound);
        let original = "13812345678";
        let masked = engine.mask(original, &ctx).unwrap();
        assert_ne!(masked, original);
        assert!(!masked.contains("1234"));
    }

    #[test]
    fn no_matching_policy_returns_original() {
        let engine = make_engine();
        let ctx = MaskingContext::new("viewer", "address", DataFlow::Outbound);
        let value = "北京市朝阳区";
        let result = engine.mask(value, &ctx).unwrap();
        assert_eq!(result, value);
    }

    #[test]
    fn admin_scene_no_masking() {
        let engine = make_engine();
        let ctx = MaskingContext::new("admin", "phone", DataFlow::Outbound);
        let value = "13812345678";
        let result = engine.mask(value, &ctx).unwrap();
        assert_eq!(result, value);
    }

    #[test]
    fn inbound_data_no_masking() {
        let engine = make_engine();
        let ctx = MaskingContext::new("viewer", "phone", DataFlow::Inbound);
        let value = "13812345678";
        let result = engine.mask(value, &ctx).unwrap();
        assert_eq!(result, value);
    }

    #[test]
    fn hash_strategy_irreversible() {
        let policies = vec![MaskingPolicy::new(
            "p1",
            "viewer",
            "email",
            MaskingScene::ApiResponse,
            MaskingStrategyKind::Hash,
        )];
        let hot_updater = Arc::new(HotUpdateCoordinator::new(vec![]));
        let engine = MaskingPolicyEngine::new(policies, hot_updater, MaskingPolicyConfig::new());
        let ctx = MaskingContext::new("viewer", "email", DataFlow::Outbound);
        let original = "user@example.com";
        let hashed = engine.mask(original, &ctx).unwrap();
        assert_ne!(hashed, original);
        assert!(!hashed.contains("user@example.com"));
    }

    #[test]
    fn replace_strategy() {
        let policies = vec![MaskingPolicy::new(
            "p1",
            "viewer",
            "ssn",
            MaskingScene::ApiResponse,
            MaskingStrategyKind::Replace("***".to_string()),
        )];
        let hot_updater = Arc::new(HotUpdateCoordinator::new(vec![]));
        let engine = MaskingPolicyEngine::new(policies, hot_updater, MaskingPolicyConfig::new());
        let ctx = MaskingContext::new("viewer", "ssn", DataFlow::Outbound);
        let masked = engine.mask("123-45-6789", &ctx).unwrap();
        assert_eq!(masked, "***");
    }

    #[test]
    fn wildcard_role_matches_any() {
        let policies = vec![MaskingPolicy::new(
            "p1",
            "",
            "phone",
            MaskingScene::ApiResponse,
            MaskingStrategyKind::Replace("***".to_string()),
        )];
        let hot_updater = Arc::new(HotUpdateCoordinator::new(vec![]));
        let engine = MaskingPolicyEngine::new(policies, hot_updater, MaskingPolicyConfig::new());
        let ctx = MaskingContext::new("anyone", "phone", DataFlow::Outbound);
        let masked = engine.mask("13812345678", &ctx).unwrap();
        assert_eq!(masked, "***");
    }

    #[test]
    fn scene_needs_masking() {
        assert!(MaskingScene::ApiResponse.needs_masking());
        assert!(MaskingScene::Logging.needs_masking());
        assert!(MaskingScene::ReportExport.needs_masking());
        assert!(!MaskingScene::AdminDisplay.needs_masking());
    }

    #[test]
    fn strategy_kind_is_irreversible() {
        assert!(MaskingStrategyKind::Mask {
            keep_prefix: 3,
            keep_suffix: 4
        }
        .is_irreversible());
        assert!(MaskingStrategyKind::Hash.is_irreversible());
        assert!(MaskingStrategyKind::Replace("***".to_string()).is_irreversible());
    }

    #[test]
    fn hot_updater_synced_on_reload() {
        let engine = make_engine();
        let new_policies = vec![MaskingPolicy::new(
            "p1",
            "viewer",
            "phone",
            MaskingScene::ApiResponse,
            MaskingStrategyKind::Replace("***".to_string()),
        )];
        engine.hot_reload(&new_policies).unwrap();
        let rules = engine.hot_updater().current_rules();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].field_name, "phone");
    }
}
