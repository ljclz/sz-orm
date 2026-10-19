//! AI 配置聚合与决策结果聚合模块（v7.3.0 任务 3.1）
//!
//! 统一管理四项 AI 能力（查询改写/索引推荐/NL2SQL/向量检索）的开关与参数，
//! 并聚合决策结果用于审计与可观测性。
//!
//! 启用 `ai-config` feature 时编译，默认不启用，不改变 v7.2.0 既有行为。

use serde::{Deserialize, Serialize};

/// 查询改写路径策略
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RewritePath {
    /// 纯规则路径（≤ 50ms）
    #[default]
    Rule,
    /// 纯 LLM 路径（≤ 3s）
    Llm,
    /// 规则 + LLM 混合路径（规则优先，无建议时回退 LLM）
    Hybrid,
}

impl RewritePath {
    /// 转为人类可读名称
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Rule => "Rule",
            Self::Llm => "Llm",
            Self::Hybrid => "Hybrid",
        }
    }
}

/// 向量 ANN 索引类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AnnIndexType {
    /// HNSW 索引（层次化可导航小世界图，高精度）
    #[default]
    Hnsw,
    /// IVF 索引（倒排文件 + 扁平量化）
    Ivf,
    /// 量化索引（乘积量化/标量量化，低存储高吞吐）
    Quantized,
}

impl AnnIndexType {
    /// 转为人类可读名称
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Hnsw => "HNSW",
            Self::Ivf => "IVF",
            Self::Quantized => "Quantized",
        }
    }
}

/// AI 配置聚合（四项 AI 能力的统一开关与参数）
///
/// 所有开关默认 `false`，启用 `ai-config` feature 后需显式构造启用。
/// 不改变 v7.2.0 既有行为（默认全关闭）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConfig {
    /// 查询改写开关
    pub rewrite_enabled: bool,
    /// 查询改写路径策略
    pub rewrite_path: RewritePath,
    /// 索引推荐开关
    pub index_advisor_enabled: bool,
    /// NL2SQL 开关
    pub nl2sql_enabled: bool,
    /// NL2SQL 多轮对话开关
    pub nl2sql_multi_turn: bool,
    /// 向量 ANN 索引类型
    pub vector_ann_index: AnnIndexType,
    /// 向量召回率阈值（0.0 ~ 1.0，低于此值告警）
    pub vector_recall_threshold: f64,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            rewrite_enabled: false,
            rewrite_path: RewritePath::default(),
            index_advisor_enabled: false,
            nl2sql_enabled: false,
            nl2sql_multi_turn: false,
            vector_ann_index: AnnIndexType::default(),
            vector_recall_threshold: 0.9,
        }
    }
}

impl AiConfig {
    /// 校验配置合法性
    ///
    /// # 校验规则
    /// - `vector_recall_threshold` 必须在 `[0.0, 1.0]` 范围内
    /// - `nl2sql_multi_turn` 为 `true` 时 `nl2sql_enabled` 必须为 `true`
    ///
    /// # 返回值
    /// - `Ok(())`: 配置合法
    /// - `Err(String)`: 配置非法，返回错误描述
    pub fn validate(&self) -> Result<(), String> {
        if self.vector_recall_threshold < 0.0 || self.vector_recall_threshold > 1.0 {
            return Err(format!(
                "vector_recall_threshold 必须在 [0.0, 1.0] 范围内，当前值：{}",
                self.vector_recall_threshold
            ));
        }
        if self.nl2sql_multi_turn && !self.nl2sql_enabled {
            return Err(
                "nl2sql_multi_turn 为 true 时 nl2sql_enabled 必须为 true（多轮对话依赖 NL2SQL 基础能力）"
                    .to_string(),
            );
        }
        Ok(())
    }

    /// 是否启用了任一 AI 能力
    pub fn any_enabled(&self) -> bool {
        self.rewrite_enabled
            || self.index_advisor_enabled
            || self.nl2sql_enabled
            || self.nl2sql_multi_turn
    }
}

/// AI 决策类型（对应四项 AI 能力）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AiDecisionType {
    /// 查询改写
    Rewrite,
    /// 索引推荐
    IndexRecommend,
    /// NL2SQL 转换
    Nl2sql,
    /// 向量检索
    VectorSearch,
}

impl AiDecisionType {
    /// 转为人类可读名称
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Rewrite => "Rewrite",
            Self::IndexRecommend => "IndexRecommend",
            Self::Nl2sql => "Nl2sql",
            Self::VectorSearch => "VectorSearch",
        }
    }
}

// ============================================================================
// v10.11.0 T3.5：AI 决策可解释性
// 所有 AI 自主决策附决策依据（特征重要性 + 决策路径 + 风险评估 + 回滚方案），
// 决策过程可追溯可审计。AiDecision 持有 explainer: Option<DecisionExplainer>。
// ============================================================================

/// 风险等级
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskLevel {
    /// 低风险（可自动应用）
    Low,
    /// 中风险（需记录但可应用）
    Medium,
    /// 高风险（须人工确认）
    High,
}

/// 风险因子
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskFactor {
    /// 因子名称
    pub factor: String,
    /// 权重（0.0 ~ 1.0）
    pub weight: f64,
}

/// 风险评估
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskAssessment {
    /// 风险等级
    pub level: RiskLevel,
    /// 风险因子列表
    pub factors: Vec<RiskFactor>,
    /// 综合风险评分（0.0 ~ 1.0，越高越危险）
    pub score: f64,
}

/// 特征重要性（决策依据之一）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureImportance {
    /// 特征名称
    pub feature: String,
    /// 重要性（0.0 ~ 1.0）
    pub importance: f64,
}

/// 决策步骤
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionStep {
    /// 步骤名称
    pub step: String,
    /// 步骤详情
    pub detail: String,
}

/// 决策路径（决策依据之一）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionPath {
    /// 决策步骤序列
    pub steps: Vec<DecisionStep>,
}

/// 验证条件（回滚方案的一部分）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyCondition {
    /// 验证条件描述
    pub condition: String,
}

/// 回滚步骤
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackStep {
    /// 步骤名称
    pub step: String,
    /// 步骤详情
    pub detail: String,
}

/// 回滚方案（决策依据之一）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackPlan {
    /// 回滚步骤序列
    pub steps: Vec<RollbackStep>,
    /// 回滚验证条件
    pub verify: VerifyCondition,
}

/// 审计条目（决策入审计日志的可追溯记录）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// 决策类型
    pub decision_type: AiDecisionType,
    /// 特征重要性列表
    pub feature_importance: Vec<FeatureImportance>,
    /// 决策路径
    pub decision_path: DecisionPath,
    /// 风险评估
    pub risk_assessment: RiskAssessment,
    /// 回滚方案
    pub rollback_plan: RollbackPlan,
    /// 时间戳（UNIX 毫秒）
    pub timestamp_ms: u64,
}

/// 决策可解释性（v10.11.0 T3.5）
///
/// 所有 AI 自主决策附决策依据：特征重要性 + 决策路径 + 风险评估 + 回滚方案。
/// 决策执行前校验可解释性完整性，缺失则拒绝执行；决策入审计日志可追溯。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionExplainer {
    /// 特征重要性列表
    pub feature_importance: Vec<FeatureImportance>,
    /// 决策路径
    pub decision_path: DecisionPath,
    /// 风险评估
    pub risk_assessment: RiskAssessment,
    /// 回滚方案
    pub rollback_plan: RollbackPlan,
}

impl DecisionExplainer {
    /// 创建决策可解释性
    pub fn new(
        feature_importance: Vec<FeatureImportance>,
        decision_path: DecisionPath,
        risk_assessment: RiskAssessment,
        rollback_plan: RollbackPlan,
    ) -> Self {
        Self {
            feature_importance,
            decision_path,
            risk_assessment,
            rollback_plan,
        }
    }

    /// 转审计条目（决策入审计日志可追溯）
    pub fn to_audit_entry(&self, decision_type: AiDecisionType) -> AuditEntry {
        let timestamp_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        AuditEntry {
            decision_type,
            feature_importance: self.feature_importance.clone(),
            decision_path: self.decision_path.clone(),
            risk_assessment: self.risk_assessment.clone(),
            rollback_plan: self.rollback_plan.clone(),
            timestamp_ms,
        }
    }

    /// 可解释性完整性校验
    ///
    /// 决策执行前校验：特征重要性 + 决策路径 + 回滚方案均非空，
    /// 回滚验证条件非空。缺失则拒绝执行。
    pub fn verify_completeness(&self) -> bool {
        !self.feature_importance.is_empty()
            && !self.decision_path.steps.is_empty()
            && !self.rollback_plan.steps.is_empty()
            && !self.rollback_plan.verify.condition.is_empty()
    }
}

/// AI 决策结果聚合（用于审计与可观测性）
///
/// 每次执行 AI 能力后产出一条决策记录，包含输入摘要、输出、原因、置信度和延迟。
/// v10.11.0 新增 explainer 字段（Option 包裹，向后兼容）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiDecision {
    /// 决策类型
    pub decision_type: AiDecisionType,
    /// 输入摘要（如 SQL 前 200 字符或自然语言查询）
    pub input_summary: String,
    /// 输出（如改写后 SQL、索引 DDL、生成的 SQL、向量检索结果摘要）
    pub output: String,
    /// 决策原因（如命中的规则名称或 LLM 模型标识）
    pub reason: String,
    /// 置信度（0.0 ~ 1.0）
    pub confidence: f64,
    /// 延迟（毫秒）
    pub latency_ms: u64,
    /// 决策可解释性（v10.11.0 新增，Option 包裹向后兼容）
    pub explainer: Option<DecisionExplainer>,
}

impl AiDecision {
    /// 创建决策记录（向后兼容，explainer 默认 None）
    pub fn new(
        decision_type: AiDecisionType,
        input_summary: impl Into<String>,
        output: impl Into<String>,
        reason: impl Into<String>,
        confidence: f64,
        latency_ms: u64,
    ) -> Self {
        Self {
            decision_type,
            input_summary: input_summary.into(),
            output: output.into(),
            reason: reason.into(),
            confidence,
            latency_ms,
            explainer: None,
        }
    }

    /// 附加决策可解释性（v10.11.0 T3.5 builder 方法）
    pub fn with_explainer(mut self, explainer: DecisionExplainer) -> Self {
        self.explainer = Some(explainer);
        self
    }

    /// 获取审计条目（决策入审计日志可追溯）
    ///
    /// 若 explainer 存在且完整性校验通过，返回 Some(AuditEntry)；否则 None。
    pub fn to_audit_entry(&self) -> Option<AuditEntry> {
        self.explainer
            .as_ref()
            .filter(|e| e.verify_completeness())
            .map(|e| e.to_audit_entry(self.decision_type))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_all_disabled() {
        let config = AiConfig::default();
        assert!(!config.rewrite_enabled);
        assert!(!config.index_advisor_enabled);
        assert!(!config.nl2sql_enabled);
        assert!(!config.nl2sql_multi_turn);
        assert_eq!(config.rewrite_path, RewritePath::Rule);
        assert_eq!(config.vector_ann_index, AnnIndexType::Hnsw);
        assert!((config.vector_recall_threshold - 0.9).abs() < 1e-9);
        assert!(!config.any_enabled());
    }

    #[test]
    fn test_validate_valid() {
        let config = AiConfig {
            rewrite_enabled: true,
            rewrite_path: RewritePath::Hybrid,
            index_advisor_enabled: true,
            nl2sql_enabled: true,
            nl2sql_multi_turn: true,
            vector_ann_index: AnnIndexType::Ivf,
            vector_recall_threshold: 0.95,
        };
        assert!(config.validate().is_ok());
        assert!(config.any_enabled());
    }

    #[test]
    fn test_validate_invalid_recall_threshold() {
        let config = AiConfig {
            vector_recall_threshold: 1.5,
            ..AiConfig::default()
        };
        let err = config.validate().unwrap_err();
        assert!(err.contains("vector_recall_threshold"));
    }

    #[test]
    fn test_validate_invalid_multi_turn_without_nl2sql() {
        let config = AiConfig {
            nl2sql_enabled: false,
            nl2sql_multi_turn: true,
            ..AiConfig::default()
        };
        let err = config.validate().unwrap_err();
        assert!(err.contains("nl2sql_multi_turn"));
    }

    #[test]
    fn test_rewrite_path_as_str() {
        assert_eq!(RewritePath::Rule.as_str(), "Rule");
        assert_eq!(RewritePath::Llm.as_str(), "Llm");
        assert_eq!(RewritePath::Hybrid.as_str(), "Hybrid");
    }

    #[test]
    fn test_ann_index_type_as_str() {
        assert_eq!(AnnIndexType::Hnsw.as_str(), "HNSW");
        assert_eq!(AnnIndexType::Ivf.as_str(), "IVF");
        assert_eq!(AnnIndexType::Quantized.as_str(), "Quantized");
    }

    #[test]
    fn test_ai_decision_new() {
        let decision = AiDecision::new(
            AiDecisionType::Rewrite,
            "SELECT * FROM users",
            "SELECT id, name FROM users",
            "RedundantElimination",
            0.85,
            12,
        );
        assert_eq!(decision.decision_type, AiDecisionType::Rewrite);
        assert_eq!(decision.input_summary, "SELECT * FROM users");
        assert_eq!(decision.output, "SELECT id, name FROM users");
        assert_eq!(decision.reason, "RedundantElimination");
        assert!((decision.confidence - 0.85).abs() < 1e-9);
        assert_eq!(decision.latency_ms, 12);
    }

    #[test]
    fn test_ai_decision_type_as_str() {
        assert_eq!(AiDecisionType::Rewrite.as_str(), "Rewrite");
        assert_eq!(AiDecisionType::IndexRecommend.as_str(), "IndexRecommend");
        assert_eq!(AiDecisionType::Nl2sql.as_str(), "Nl2sql");
        assert_eq!(AiDecisionType::VectorSearch.as_str(), "VectorSearch");
    }

    #[test]
    fn test_config_serde_roundtrip() {
        let config = AiConfig {
            rewrite_enabled: true,
            rewrite_path: RewritePath::Llm,
            index_advisor_enabled: false,
            nl2sql_enabled: true,
            nl2sql_multi_turn: false,
            vector_ann_index: AnnIndexType::Quantized,
            vector_recall_threshold: 0.8,
        };
        let json = serde_json::to_string(&config).unwrap();
        let de: AiConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(de.rewrite_enabled, config.rewrite_enabled);
        assert_eq!(de.rewrite_path, config.rewrite_path);
        assert_eq!(de.vector_ann_index, config.vector_ann_index);
        assert!((de.vector_recall_threshold - 0.8).abs() < 1e-9);
    }

    #[test]
    fn test_ai_config_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<AiConfig>();
        assert_send_sync::<AiDecision>();
        assert_send_sync::<RewritePath>();
        assert_send_sync::<AnnIndexType>();
        assert_send_sync::<AiDecisionType>();
    }
}
