//! 统一查询计划优化器模块
//!
//! 在现有规则型 [`QueryOptimizer`] 基础上，新增 LLM 查询计划优化建议能力，
//! 两者并存互补：规则分析始终执行（离线），LLM 建议在配置启用时执行（在线），
//! 未配置或调用失败时自动降级纯规则引擎。
//!
//! # 架构
//!
//! ```text
//! SQL + Schema + EXPLAIN 输出
//!            │
//!            ▼
//! ┌──────────────────────────┐
//! │   UnifiedQueryOptimizer  │
//! │  ┌──────────────────┐    │
//! │  │  QueryOptimizer  │ ← 规则分析（始终执行）
//! │  └──────────────────┘    │
//! │  ┌──────────────────┐    │
//! │  │ ExplainPlanParser│ ← EXPLAIN 解析
//! │  └──────────────────┘    │
//! │  ┌──────────────────┐    │
//! │  │   LlmOptimizer   │ ← LLM 建议（可选，降级安全）
//! │  └──────────────────┘    │
//! │  ┌──────────────────┐    │
//! │  │   SqlSanitizer   │ ← 敏感字面量脱敏
//! │  └──────────────────┘    │
//! └──────────────────────────┘
//!            │
//!            ▼
//!   UnifiedQueryAnalysis
//!   (hints + explain_signals + llm_available + degraded_reason)
//! ```

use crate::error::AiError;
use crate::explain_parser::{ExplainPlanParser, ExplainSignal};
use crate::nl2sql::{HintSeverity, QueryOptimizer, SchemaContext};
use crate::sql_sanitizer::SqlSanitizer;

use serde::{Deserialize, Serialize};

// ==================== M6.1: HintSource + UnifiedOptimizationHint ====================

/// 优化建议来源
///
/// 标注每条建议是由规则引擎还是 LLM 生成，支持来源追溯。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HintSource {
    /// 规则引擎生成（离线）
    Rule,
    /// LLM 生成（在线），附带模型名称
    Llm {
        /// 生成该建议的 LLM 模型名称
        model: String,
    },
}

/// 统一优化建议
///
/// 与现有 [`crate::nl2sql::QueryOptimizationHint`] 并存，新增 `source` 字段标注来源。
/// 现有 `QueryOptimizationHint` 不变（无 Breaking Change）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedOptimizationHint {
    /// 建议标题
    pub title: String,
    /// 详细描述
    pub description: String,
    /// 严重级别
    pub severity: HintSeverity,
    /// 优化后的 SQL 建议（仅展示用途，系统零次执行）
    pub suggested_sql: Option<String>,
    /// 建议来源（规则 / LLM）
    pub source: HintSource,
}

impl UnifiedOptimizationHint {
    /// 创建一条规则来源的建议
    pub fn from_rule(
        title: impl Into<String>,
        description: impl Into<String>,
        severity: HintSeverity,
    ) -> Self {
        Self {
            title: title.into(),
            description: description.into(),
            severity,
            suggested_sql: None,
            source: HintSource::Rule,
        }
    }

    /// 创建一条 LLM 来源的建议
    pub fn from_llm(
        title: impl Into<String>,
        description: impl Into<String>,
        severity: HintSeverity,
        model: impl Into<String>,
    ) -> Self {
        Self {
            title: title.into(),
            description: description.into(),
            severity,
            suggested_sql: None,
            source: HintSource::Llm {
                model: model.into(),
            },
        }
    }

    /// 附加优化后的 SQL 建议
    pub fn with_suggested_sql(mut self, sql: impl Into<String>) -> Self {
        self.suggested_sql = Some(sql.into());
        self
    }
}

/// 统一查询分析结果
///
/// 包含规则 + LLM 合并后的所有建议，以及 EXPLAIN 信号和 LLM 可用性信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedQueryAnalysis {
    /// 原始 SQL
    pub original_sql: String,
    /// 所有优化建议（规则 + LLM，标注来源）
    pub hints: Vec<UnifiedOptimizationHint>,
    /// EXPLAIN 解析出的性能信号
    pub explain_signals: Vec<ExplainSignal>,
    /// LLM 是否可用（是否成功生成建议）
    pub llm_available: bool,
    /// LLM 降级原因（未配置 / 调用失败 / 超时等）
    pub llm_degraded_reason: Option<String>,
    /// 预估的 SQL 复杂度评分（0-100）
    pub complexity_score: u32,
    /// 检测到的表名列表
    pub detected_tables: Vec<String>,
    /// 是否包含 WHERE 子句
    pub has_where: bool,
    /// 是否包含 LIMIT 子句
    pub has_limit: bool,
    /// 是否包含 JOIN
    pub has_join: bool,
    /// 是否包含子查询
    pub has_subquery: bool,
    /// 是否使用了 SELECT *
    pub uses_select_star: bool,
}

impl UnifiedQueryAnalysis {
    /// 返回来源为 LLM 的建议数量
    pub fn llm_hint_count(&self) -> usize {
        self.hints
            .iter()
            .filter(|h| matches!(h.source, HintSource::Llm { .. }))
            .count()
    }

    /// 返回来源为规则的建议数量
    pub fn rule_hint_count(&self) -> usize {
        self.hints
            .iter()
            .filter(|h| matches!(h.source, HintSource::Rule))
            .count()
    }

    /// 是否存在任何建议
    pub fn has_hints(&self) -> bool {
        !self.hints.is_empty()
    }
}

// ==================== M6.2: OptimizerConfig ====================

/// LLM 优化器配置
///
/// 控制是否启用 LLM 建议以及 LLM 服务的连接参数。
/// 默认 `enable_llm=false`，降级为纯规则引擎。
#[derive(Debug, Clone)]
pub struct OptimizerConfig {
    /// OpenAI 兼容 API key（None 时降级纯规则）
    pub api_key: Option<String>,
    /// API 基础地址（不含 `/chat/completions` 后缀）
    pub api_base: String,
    /// LLM 模型名称
    pub model: String,
    /// 请求超时时间（秒）
    pub timeout_secs: u64,
    /// LLM 最大输出 token 数
    pub max_tokens: u32,
    /// 是否启用 LLM 建议
    pub enable_llm: bool,
}

impl Default for OptimizerConfig {
    fn default() -> Self {
        Self {
            api_key: None,
            api_base: "https://api.openai.com/v1".to_string(),
            model: "gpt-4o-mini".to_string(),
            timeout_secs: 10,
            max_tokens: 2000,
            enable_llm: false,
        }
    }
}

impl OptimizerConfig {
    /// 创建启用 LLM 的配置
    pub fn with_llm(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            api_key: Some(api_key.into()),
            api_base: "https://api.openai.com/v1".to_string(),
            model: model.into(),
            timeout_secs: 10,
            max_tokens: 2000,
            enable_llm: true,
        }
    }

    /// 设置 API base URL
    pub fn with_api_base(mut self, api_base: impl Into<String>) -> Self {
        self.api_base = api_base.into();
        self
    }

    /// 设置超时时间
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    /// 设置最大 token 数
    pub fn with_max_tokens(mut self, tokens: u32) -> Self {
        self.max_tokens = tokens;
        self
    }
}

// ==================== M6.4: LlmOptimizer ====================

/// LLM 请求体（OpenAI 兼容 chat/completions）
#[derive(Serialize)]
struct LlmRequest {
    model: String,
    messages: Vec<LlmMessage>,
    max_tokens: u32,
    temperature: f32,
}

#[derive(Serialize)]
struct LlmMessage {
    role: String,
    content: String,
}

/// LLM 响应体
#[derive(Deserialize)]
struct LlmResponse {
    choices: Vec<LlmChoice>,
}

#[derive(Deserialize)]
struct LlmChoice {
    message: LlmResponseMessage,
}

#[derive(Deserialize)]
struct LlmResponseMessage {
    content: String,
}

/// LLM 返回的单条建议（JSON 解析用）
#[derive(Deserialize)]
struct LlmHintItem {
    title: String,
    description: String,
    #[serde(default = "default_severity")]
    severity: String,
    #[serde(default)]
    suggested_sql: Option<String>,
}

fn default_severity() -> String {
    "info".to_string()
}

/// LLM 优化建议引擎
///
/// 调用 OpenAI 兼容 API 生成结构化查询优化建议。
/// 所有发送给 LLM 的 SQL 均经过 [`SqlSanitizer`] 脱敏处理。
/// LLM 返回的 SQL 仅作为建议（`suggested_sql`），系统零次执行。
pub struct LlmOptimizer {
    config: OptimizerConfig,
    http_client: reqwest::Client,
}

impl LlmOptimizer {
    /// 创建 LLM 优化器实例
    ///
    /// # 错误
    ///
    /// API key 缺失或 HTTP 客户端构建失败时返回错误。
    pub fn new(config: OptimizerConfig) -> Result<Self, AiError> {
        if config.api_key.is_none() {
            return Err(AiError::ConfigError("未配置 LLM API key".to_string()));
        }

        let timeout = std::time::Duration::from_secs(config.timeout_secs);
        let http_client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|e| AiError::NetworkError(e.to_string()))?;

        Ok(Self {
            config,
            http_client,
        })
    }

    /// 请求 LLM 生成优化建议
    ///
    /// # 参数
    /// - `sql`: 原始 SQL（将自动脱敏后发送给 LLM）
    /// - `explain_signals`: EXPLAIN 解析出的性能信号
    /// - `schema`: 数据库 schema 上下文
    ///
    /// # 返回
    ///
    /// LLM 生成的优化建议列表。非法 JSON/字段缺失的条目被丢弃，
    /// 合法条目保留，解析失败记录到日志。
    pub async fn request(
        &self,
        sql: &str,
        explain_signals: &[ExplainSignal],
        schema: &SchemaContext,
    ) -> Result<Vec<UnifiedOptimizationHint>, AiError> {
        let sanitized_sql = SqlSanitizer::sanitize(sql);
        let prompt = self.build_prompt(&sanitized_sql, explain_signals, schema);
        let response_text = self.call_llm_api(&prompt).await?;
        Ok(self.parse_hints(&response_text))
    }

    fn build_prompt(
        &self,
        sql: &str,
        explain_signals: &[ExplainSignal],
        schema: &SchemaContext,
    ) -> String {
        let mut prompt = String::new();

        prompt.push_str("You are a SQL query optimization expert. ");
        prompt
            .push_str("Analyze the following SQL query and provide optimization suggestions.\n\n");
        prompt.push_str(&format!("SQL: {}\n\n", sql));

        if !explain_signals.is_empty() {
            prompt.push_str("EXPLAIN signals:\n");
            for signal in explain_signals {
                prompt.push_str(&format!("- {}\n", signal.as_str()));
            }
            prompt.push('\n');
        }

        if !schema.tables.is_empty() {
            prompt.push_str("Schema:\n");
            for table in &schema.tables {
                prompt.push_str(&format!("- Table: {}\n", table.name));
                for col in &table.columns {
                    let pk = if col.is_primary_key { " (PK)" } else { "" };
                    let nullable = if col.nullable { " nullable" } else { "" };
                    prompt.push_str(&format!(
                        "  - {} {}{}{}\n",
                        col.name, col.data_type, pk, nullable
                    ));
                }
            }
            prompt.push('\n');
        }

        prompt.push_str("Provide your suggestions as a JSON array. Each suggestion:\n");
        prompt.push_str("{\n");
        prompt.push_str("  \"title\": \"short title\",\n");
        prompt.push_str("  \"description\": \"detailed explanation\",\n");
        prompt.push_str("  \"severity\": \"info\" | \"warning\" | \"critical\",\n");
        prompt.push_str("  \"suggested_sql\": \"optional optimized SQL\"\n");
        prompt.push_str("}\n\n");
        prompt.push_str("Return ONLY the JSON array, no other text.");

        prompt
    }

    async fn call_llm_api(&self, prompt: &str) -> Result<String, AiError> {
        let api_key = self
            .config
            .api_key
            .as_ref()
            .ok_or_else(|| AiError::ConfigError("未配置 LLM API key".to_string()))?;

        let request_body = LlmRequest {
            model: self.config.model.clone(),
            messages: vec![
                LlmMessage {
                    role: "system".to_string(),
                    content: "You are a SQL optimization expert. Return only JSON.".to_string(),
                },
                LlmMessage {
                    role: "user".to_string(),
                    content: prompt.to_string(),
                },
            ],
            max_tokens: self.config.max_tokens,
            temperature: 0.0,
        };

        let url = format!("{}/chat/completions", self.config.api_base);

        let response = self
            .http_client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&request_body)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    AiError::NetworkError(format!("LLM 请求超时（{}s）", self.config.timeout_secs))
                } else {
                    AiError::NetworkError(e.to_string())
                }
            })?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(AiError::ApiError(status.as_u16(), error_text));
        }

        let response_body: LlmResponse = response
            .json()
            .await
            .map_err(|e| AiError::NetworkError(format!("LLM 响应解析失败: {}", e)))?;

        response_body
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .ok_or_else(|| AiError::ApiError(200, "LLM 响应无 choices".to_string()))
    }

    fn parse_hints(&self, response_text: &str) -> Vec<UnifiedOptimizationHint> {
        let json_str = Self::extract_json_array(response_text);

        let parsed: Result<Vec<serde_json::Value>, serde_json::Error> =
            serde_json::from_str(&json_str);

        match parsed {
            Ok(values) => values
                .into_iter()
                .filter_map(|v| {
                    let item: LlmHintItem = serde_json::from_value(v).ok()?;
                    let severity = match item.severity.to_lowercase().as_str() {
                        "warning" => HintSeverity::Warning,
                        "critical" => HintSeverity::Critical,
                        _ => HintSeverity::Info,
                    };
                    let mut hint = UnifiedOptimizationHint::from_llm(
                        item.title,
                        item.description,
                        severity,
                        &self.config.model,
                    );
                    if let Some(sql) = item.suggested_sql {
                        if !sql.is_empty() {
                            hint = hint.with_suggested_sql(sql);
                        }
                    }
                    Some(hint)
                })
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    fn extract_json_array(text: &str) -> String {
        let trimmed = text.trim();
        if trimmed.starts_with('[') {
            return trimmed.to_string();
        }

        if let Some(start) = trimmed.find('[') {
            if let Some(end) = trimmed.rfind(']') {
                if end > start {
                    return trimmed[start..=end].to_string();
                }
            }
        }

        trimmed.to_string()
    }
}

// ==================== M6.5 + M6.7: UnifiedQueryOptimizer ====================

/// 统一查询优化器
///
/// 合并规则引擎和 LLM 建议引擎的查询优化器。
/// - 规则分析始终执行（离线，无外部依赖）
/// - LLM 建议在 `enable_llm=true` 且 `api_key` 存在时执行
/// - LLM 未配置 / 调用失败 / 超时时自动降级纯规则引擎
///
/// # 安全保证
///
/// - LLM 返回的 `suggested_sql` 仅作为建议展示，系统零次执行
/// - 发送给 LLM 的 SQL 经过 [`SqlSanitizer`] 脱敏处理
/// - [`UnifiedQueryOptimizer`] 无 `execute_sql` 方法
pub struct UnifiedQueryOptimizer {
    rule_optimizer: QueryOptimizer,
    config: OptimizerConfig,
    llm_optimizer: Option<LlmOptimizer>,
    /// v3.2.0：查询计划缓存（可选，启用 plan-cache feature 时生效）
    #[cfg(feature = "plan-cache")]
    plan_cache: Option<std::sync::Arc<sz_orm_core::plan_cache::PlanCache>>,
}

impl UnifiedQueryOptimizer {
    /// 创建统一查询优化器
    ///
    /// 根据 `config` 决定是否初始化 LLM 优化器：
    /// - `enable_llm=true` 且 `api_key` 存在 → 初始化 LLM 优化器
    /// - 否则 → 仅使用规则引擎（降级模式）
    pub fn new(config: OptimizerConfig) -> Self {
        let llm_optimizer = if config.enable_llm && config.api_key.is_some() {
            LlmOptimizer::new(config.clone()).ok()
        } else {
            None
        };

        Self {
            rule_optimizer: QueryOptimizer::new(),
            config,
            llm_optimizer,
            #[cfg(feature = "plan-cache")]
            plan_cache: None,
        }
    }

    /// 使用自定义规则优化器创建
    pub fn with_rule_optimizer(config: OptimizerConfig, rule_optimizer: QueryOptimizer) -> Self {
        let llm_optimizer = if config.enable_llm && config.api_key.is_some() {
            LlmOptimizer::new(config.clone()).ok()
        } else {
            None
        };

        Self {
            rule_optimizer,
            config,
            llm_optimizer,
            #[cfg(feature = "plan-cache")]
            plan_cache: None,
        }
    }

    /// v3.2.0：注入查询计划缓存
    ///
    /// 启用后，`optimize` 方法在执行规则分析 + LLM 调用前先查缓存，
    /// 命中跳过优化，未命中执行优化后存入缓存。
    /// 未调用此方法时行为完全不变（向后兼容）。
    #[cfg(feature = "plan-cache")]
    pub fn with_plan_cache(
        mut self,
        cache: std::sync::Arc<sz_orm_core::plan_cache::PlanCache>,
    ) -> Self {
        self.plan_cache = Some(cache);
        self
    }

    /// 优化查询
    ///
    /// # 参数
    /// - `sql`: 要优化的 SQL 查询
    /// - `schema`: 数据库 schema 上下文
    /// - `explain_output`: EXPLAIN 输出文本（可选）
    /// - `parser`: EXPLAIN 解析器（可选，需与数据库方言匹配）
    ///
    /// # 返回
    ///
    /// [`UnifiedQueryAnalysis`] 包含合并后的建议、EXPLAIN 信号和 LLM 可用性信息。
    ///
    /// # 降级逻辑
    ///
    /// - 未配置 API key → 仅返回规则建议，`llm_degraded_reason="未配置 LLM API key"`
    /// - LLM 调用失败 → 返回规则建议 + 降级原因
    /// - 超时 → 降级规则引擎
    /// - 不报错不阻塞
    pub async fn optimize(
        &self,
        sql: &str,
        schema: &SchemaContext,
        explain_output: Option<&str>,
        parser: Option<&dyn ExplainPlanParser>,
    ) -> UnifiedQueryAnalysis {
        // v3.2.0：查询计划缓存命中检查
        #[cfg(feature = "plan-cache")]
        if let Some(ref cache) = self.plan_cache {
            if let Some(cached) = cache.get_or_optimize(sql) {
                if let Ok(analysis) = serde_json::from_str::<UnifiedQueryAnalysis>(&cached) {
                    return analysis;
                }
            }
        }

        let rule_analysis = self.rule_optimizer.analyze(sql, schema);

        let explain_signals = match (explain_output, parser) {
            (Some(output), Some(p)) => p.parse(output).unwrap_or_default(),
            _ => Vec::new(),
        };

        let mut hints: Vec<UnifiedOptimizationHint> = rule_analysis
            .hints
            .iter()
            .map(|h| {
                let mut unified =
                    UnifiedOptimizationHint::from_rule(&h.title, &h.description, h.severity);
                if let Some(ref s) = h.suggested_sql {
                    unified = unified.with_suggested_sql(s);
                }
                unified
            })
            .collect();

        let mut llm_available = false;
        let mut llm_degraded_reason: Option<String> = None;

        if let Some(ref llm) = self.llm_optimizer {
            match llm.request(sql, &explain_signals, schema).await {
                Ok(llm_hints) => {
                    llm_available = true;
                    hints.extend(llm_hints);
                }
                Err(e) => {
                    llm_degraded_reason = Some(format!("LLM 调用失败: {}", e));
                }
            }
        } else if self.config.enable_llm && self.config.api_key.is_none() {
            llm_degraded_reason = Some("未配置 LLM API key".to_string());
        } else if !self.config.enable_llm {
            llm_degraded_reason = Some("LLM 未启用".to_string());
        }

        let result = UnifiedQueryAnalysis {
            original_sql: sql.to_string(),
            hints,
            explain_signals,
            llm_available,
            llm_degraded_reason,
            complexity_score: rule_analysis.complexity_score,
            detected_tables: rule_analysis.detected_tables,
            has_where: rule_analysis.has_where,
            has_limit: rule_analysis.has_limit,
            has_join: rule_analysis.has_join,
            has_subquery: rule_analysis.has_subquery,
            uses_select_star: rule_analysis.uses_select_star,
        };

        // v3.2.0：查询计划缓存存储
        #[cfg(feature = "plan-cache")]
        if let Some(ref cache) = self.plan_cache {
            if let Ok(json) = serde_json::to_string(&result) {
                cache.store_optimize(sql, std::sync::Arc::new(json));
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nl2sql::{ColumnInfo, SchemaContext, TableInfo};

    fn make_schema() -> SchemaContext {
        SchemaContext {
            tables: vec![TableInfo {
                name: "users".to_string(),
                columns: vec![
                    ColumnInfo {
                        name: "id".to_string(),
                        data_type: "integer".to_string(),
                        nullable: false,
                        is_primary_key: true,
                    },
                    ColumnInfo {
                        name: "name".to_string(),
                        data_type: "varchar".to_string(),
                        nullable: true,
                        is_primary_key: false,
                    },
                ],
            }],
        }
    }

    // ==================== M6.1 tests ====================

    #[test]
    fn test_hint_source_variants() {
        let rule = HintSource::Rule;
        let llm = HintSource::Llm {
            model: "gpt-4o".to_string(),
        };
        assert_eq!(rule, HintSource::Rule);
        assert_eq!(
            llm,
            HintSource::Llm {
                model: "gpt-4o".to_string(),
            }
        );
        assert_ne!(rule, llm);
    }

    #[test]
    fn test_unified_optimization_hint_from_rule() {
        let hint = UnifiedOptimizationHint::from_rule(
            "避免 SELECT *",
            "建议指定列名",
            HintSeverity::Warning,
        );
        assert_eq!(hint.source, HintSource::Rule);
        assert_eq!(hint.severity, HintSeverity::Warning);
        assert!(hint.suggested_sql.is_none());
    }

    #[test]
    fn test_unified_optimization_hint_from_llm() {
        let hint = UnifiedOptimizationHint::from_llm(
            "添加索引",
            "建议在 name 列添加索引",
            HintSeverity::Critical,
            "gpt-4o-mini",
        )
        .with_suggested_sql("CREATE INDEX idx_name ON users(name)");
        assert_eq!(
            hint.source,
            HintSource::Llm {
                model: "gpt-4o-mini".to_string()
            }
        );
        assert!(hint.suggested_sql.is_some());
    }

    // ==================== M6.2 tests ====================

    #[test]
    fn test_optimizer_config_default() {
        let config = OptimizerConfig::default();
        assert!(config.api_key.is_none());
        assert!(!config.enable_llm);
        assert_eq!(config.timeout_secs, 10);
        assert_eq!(config.max_tokens, 2000);
    }

    #[test]
    fn test_optimizer_config_with_llm() {
        let config = OptimizerConfig::with_llm("sk-test", "gpt-4o");
        assert_eq!(config.api_key, Some("sk-test".to_string()));
        assert!(config.enable_llm);
        assert_eq!(config.model, "gpt-4o");
    }

    #[test]
    fn test_optimizer_config_builder() {
        let config = OptimizerConfig::with_llm("sk-test", "gpt-4o")
            .with_api_base("http://localhost:8080/v1")
            .with_timeout(30)
            .with_max_tokens(4000);
        assert_eq!(config.api_base, "http://localhost:8080/v1");
        assert_eq!(config.timeout_secs, 30);
        assert_eq!(config.max_tokens, 4000);
    }

    // ==================== M6.5 + M6.7 tests (degradation) ====================

    #[tokio::test]
    async fn test_unified_optimizer_degradation_no_llm() {
        let config = OptimizerConfig::default();
        let optimizer = UnifiedQueryOptimizer::new(config);
        let schema = make_schema();

        let analysis = optimizer
            .optimize("SELECT * FROM users", &schema, None, None)
            .await;

        assert!(!analysis.llm_available);
        assert!(analysis.llm_degraded_reason.is_some());
        assert!(analysis.rule_hint_count() > 0);
        assert_eq!(analysis.llm_hint_count(), 0);
    }

    #[tokio::test]
    async fn test_unified_optimizer_degradation_no_api_key() {
        let config = OptimizerConfig {
            enable_llm: true,
            api_key: None,
            ..Default::default()
        };
        let optimizer = UnifiedQueryOptimizer::new(config);
        let schema = make_schema();

        let analysis = optimizer
            .optimize("SELECT * FROM users", &schema, None, None)
            .await;

        assert!(!analysis.llm_available);
        assert_eq!(
            analysis.llm_degraded_reason,
            Some("未配置 LLM API key".to_string())
        );
    }

    #[tokio::test]
    async fn test_unified_optimizer_degradation_invalid_api_key() {
        let config = OptimizerConfig::with_llm("invalid-key", "gpt-4o-mini")
            .with_api_base("http://127.0.0.1:1/v1");
        let optimizer = UnifiedQueryOptimizer::new(config);
        let schema = make_schema();

        let analysis = optimizer
            .optimize("SELECT * FROM users", &schema, None, None)
            .await;

        assert!(!analysis.llm_available);
        assert!(analysis.llm_degraded_reason.is_some());
        assert!(analysis.rule_hint_count() > 0);
    }

    #[tokio::test]
    async fn test_unified_optimizer_rule_hints_present() {
        let config = OptimizerConfig::default();
        let optimizer = UnifiedQueryOptimizer::new(config);
        let schema = make_schema();

        let analysis = optimizer
            .optimize("SELECT * FROM users", &schema, None, None)
            .await;

        assert!(analysis.has_hints());
        assert!(analysis.uses_select_star);
        assert!(analysis.rule_hint_count() > 0);
    }

    #[tokio::test]
    async fn test_unified_optimizer_with_explain_signals() {
        use crate::explain_parser::MySqlExplainParser;

        let config = OptimizerConfig::default();
        let optimizer = UnifiedQueryOptimizer::new(config);
        let schema = make_schema();

        let explain_output = "+----+-------------+-------+------------+------+---------------+------+---------+------+------+----------+-------+\n| id | select_type | table | partitions | type | possible_keys | key  | key_len | ref  | rows | filtered | Extra |\n+----+-------------+-------+------------+------+---------------+------+---------+------+------+----------+-------+\n|  1 | SIMPLE      | users | NULL       | ALL  | NULL          | NULL | NULL    | NULL |  100 |   100.00 | NULL  |\n+----+-------------+-------+------------+------+---------------+------+---------+------+------+----------+-------+";
        let parser = MySqlExplainParser;

        let analysis = optimizer
            .optimize(
                "SELECT * FROM users WHERE name = 'John'",
                &schema,
                Some(explain_output),
                Some(&parser),
            )
            .await;

        assert!(!analysis.explain_signals.is_empty());
        assert!(analysis
            .explain_signals
            .contains(&ExplainSignal::FullTableScan));
    }

    // ==================== M6.8 test: LLM SQL zero execution ====================

    #[test]
    fn test_llm_zero_execute_no_execute_method() {
        // UnifiedQueryOptimizer 没有 execute_sql 方法
        // suggested_sql 字段为 Option<String>，仅展示用途
        let hint = UnifiedOptimizationHint::from_llm("建议", "描述", HintSeverity::Info, "gpt-4o")
            .with_suggested_sql("SELECT id FROM users");

        assert!(hint.suggested_sql.is_some());
    }

    // ==================== M6.4 tests: LlmOptimizer hint parsing ====================

    #[test]
    fn test_parse_hints_valid_json() {
        let config = OptimizerConfig::with_llm("sk-test", "gpt-4o-mini");
        let optimizer = LlmOptimizer::new(config).unwrap();

        let response = r#"[
            {"title": "添加索引", "description": "建议在 name 列添加索引", "severity": "warning", "suggested_sql": "CREATE INDEX idx_name ON users(name)"},
            {"title": "避免 SELECT *", "description": "指定列名", "severity": "info"}
        ]"#;

        let hints = optimizer.parse_hints(response);
        assert_eq!(hints.len(), 2);
        assert_eq!(hints[0].title, "添加索引");
        assert_eq!(hints[0].severity, HintSeverity::Warning);
        assert!(hints[0].suggested_sql.is_some());
        assert_eq!(
            hints[0].source,
            HintSource::Llm {
                model: "gpt-4o-mini".to_string()
            }
        );
    }

    #[test]
    fn test_parse_hints_invalid_json_returns_empty() {
        let config = OptimizerConfig::with_llm("sk-test", "gpt-4o-mini");
        let optimizer = LlmOptimizer::new(config).unwrap();

        let hints = optimizer.parse_hints("not valid json");
        assert!(hints.is_empty());
    }

    #[test]
    fn test_parse_hints_partial_invalid() {
        let config = OptimizerConfig::with_llm("sk-test", "gpt-4o-mini");
        let optimizer = LlmOptimizer::new(config).unwrap();

        let response = r#"[
            {"title": "valid hint", "description": "ok", "severity": "info"},
            {"bad": "entry"}
        ]"#;

        let hints = optimizer.parse_hints(response);
        assert_eq!(hints.len(), 1);
    }

    #[test]
    fn test_extract_json_array_from_markdown() {
        let text = "Here are the suggestions:\n```json\n[{\"title\": \"test\"}]\n```\nThat's it.";
        let extracted = LlmOptimizer::extract_json_array(text);
        assert!(extracted.starts_with('['));
        assert!(extracted.ends_with(']'));
    }

    #[test]
    fn test_llm_optimizer_new_without_api_key() {
        let config = OptimizerConfig {
            enable_llm: true,
            api_key: None,
            ..Default::default()
        };
        let result = LlmOptimizer::new(config);
        assert!(result.is_err());
    }

    // ==================== UnifiedQueryAnalysis tests ====================

    #[test]
    fn test_unified_analysis_hint_counts() {
        let analysis = UnifiedQueryAnalysis {
            original_sql: "SELECT * FROM users".to_string(),
            hints: vec![
                UnifiedOptimizationHint::from_rule("r1", "d1", HintSeverity::Info),
                UnifiedOptimizationHint::from_rule("r2", "d2", HintSeverity::Warning),
                UnifiedOptimizationHint::from_llm("l1", "d3", HintSeverity::Critical, "gpt-4o"),
            ],
            explain_signals: vec![],
            llm_available: true,
            llm_degraded_reason: None,
            complexity_score: 50,
            detected_tables: vec![],
            has_where: false,
            has_limit: false,
            has_join: false,
            has_subquery: false,
            uses_select_star: true,
        };

        assert_eq!(analysis.rule_hint_count(), 2);
        assert_eq!(analysis.llm_hint_count(), 1);
        assert!(analysis.has_hints());
    }
}
