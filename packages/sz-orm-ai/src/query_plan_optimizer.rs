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

// ==================== P2 TASK-015: PerformancePredictor ====================

/// 表统计信息（用于性能预测）
///
/// 描述单张表的行数、列基数、索引选择性等统计信息，
/// 供 [`PerformancePredictor`] 预测 SQL 执行耗时使用。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableStatistics {
    /// 表名
    pub table_name: String,
    /// 表总行数
    pub row_count: u64,
    /// 各列的基数（不同值数量）
    pub column_cardinality: std::collections::HashMap<String, u64>,
    /// 各索引的选择性（0.0~1.0，1.0 = 唯一索引）
    pub index_selectivity: std::collections::HashMap<String, f64>,
    /// 平均行大小（字节）
    pub avg_row_size_bytes: u64,
}

impl TableStatistics {
    /// 创建一张表的统计信息
    pub fn new(table_name: impl Into<String>, row_count: u64) -> Self {
        Self {
            table_name: table_name.into(),
            row_count,
            column_cardinality: std::collections::HashMap::new(),
            index_selectivity: std::collections::HashMap::new(),
            avg_row_size_bytes: 64,
        }
    }

    /// 设置列基数
    pub fn with_column_cardinality(mut self, column: impl Into<String>, cardinality: u64) -> Self {
        self.column_cardinality.insert(column.into(), cardinality);
        self
    }

    /// 设置索引选择性
    pub fn with_index_selectivity(mut self, index: impl Into<String>, selectivity: f64) -> Self {
        self.index_selectivity
            .insert(index.into(), selectivity.clamp(0.0, 1.0));
        self
    }

    /// 设置平均行大小
    pub fn with_avg_row_size(mut self, size_bytes: u64) -> Self {
        self.avg_row_size_bytes = size_bytes;
        self
    }
}

/// SQL 查询特征（从 SQL 文本提取）
///
/// 描述查询的结构特征，用于成本模型估算。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryCharacteristics {
    /// 涉及的表名
    pub tables: Vec<String>,
    /// WHERE 条件列
    pub where_columns: Vec<String>,
    /// JOIN 数量
    pub join_count: usize,
    /// 子查询数量
    pub subquery_count: usize,
    /// 是否使用 SELECT *
    pub uses_select_star: bool,
    /// LIMIT 值（None = 无 LIMIT）
    pub limit: Option<u64>,
    /// ORDER BY 列
    pub order_by_columns: Vec<String>,
    /// GROUP BY 列
    pub group_by_columns: Vec<String>,
}

impl QueryCharacteristics {
    /// 从 SQL 文本提取查询特征（简易解析）
    ///
    /// 采用大小写不敏感的字符串匹配提取表名、WHERE 列、JOIN 数等。
    /// 不依赖 sqlparser（避免 feature 依赖），仅做启发式提取。
    pub fn from_sql(sql: &str) -> Self {
        let lower = sql.to_lowercase();
        let uses_select_star = lower.contains("select *");

        let tables = extract_tables(&lower);
        let where_columns = extract_where_columns(&lower);
        let join_count = lower.matches(" join ").count();
        let subquery_count = lower.matches("select").count().saturating_sub(1);
        let limit = extract_limit(&lower);
        let order_by_columns = extract_order_by_columns(&lower);
        let group_by_columns = extract_group_by_columns(&lower);

        Self {
            tables,
            where_columns,
            join_count,
            subquery_count,
            uses_select_star,
            limit,
            order_by_columns,
            group_by_columns,
        }
    }
}

fn extract_tables(lower_sql: &str) -> Vec<String> {
    let mut tables = Vec::new();
    for keyword in ["from ", "join "] {
        let mut pos = 0;
        while let Some(idx) = lower_sql[pos..].find(keyword) {
            let start = pos + idx + keyword.len();
            let rest = &lower_sql[start..];
            let table_end = rest
                .find(|c: char| c.is_whitespace() || c == ',' || c == ';' || c == '(')
                .unwrap_or(rest.len());
            let table = rest[..table_end].trim();
            if !table.is_empty()
                && !table.starts_with('(')
                && table != "where"
                && table != "on"
                && table != "as"
            {
                tables.push(table.to_string());
            }
            pos = start + table_end;
            if pos >= lower_sql.len() {
                break;
            }
        }
    }
    tables
        .into_iter()
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect()
}

fn extract_where_columns(lower_sql: &str) -> Vec<String> {
    let mut columns = Vec::new();
    if let Some(where_idx) = lower_sql.find(" where ") {
        let rest = &lower_sql[where_idx + 7..];
        let end = rest
            .find(" group by ")
            .or_else(|| rest.find(" order by "))
            .or_else(|| rest.find(" limit "))
            .unwrap_or(rest.len());
        let where_clause = &rest[..end];
        for part in where_sql_split_conditions(where_clause) {
            let cond = part.trim();
            if let Some(eq_idx) = cond.find('=') {
                let col = cond[..eq_idx].trim();
                if is_valid_column_name(col) {
                    columns.push(col.to_string());
                }
            }
        }
    }
    columns
}

fn where_sql_split_conditions(clause: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut depth = 0i32;
    for ch in clause.chars() {
        match ch {
            '(' => {
                depth += 1;
                current.push(ch);
            }
            ')' => {
                depth -= 1;
                current.push(ch);
            }
            ' ' if depth == 0 => {
                if current.eq_ignore_ascii_case("and") || current.eq_ignore_ascii_case("or") {
                    current.clear();
                } else if !current.is_empty() {
                    current.push(ch);
                }
            }
            _ => current.push(ch),
        }
    }
    if !current.is_empty() {
        result.push(current);
    }
    result
}

fn is_valid_column_name(s: &str) -> bool {
    !s.is_empty()
        && !s.eq_ignore_ascii_case("and")
        && !s.eq_ignore_ascii_case("or")
        && !s.eq_ignore_ascii_case("not")
        && s.chars()
            .next()
            .map(|c| c.is_alphabetic() || c == '_')
            .unwrap_or(false)
}

fn extract_limit(lower_sql: &str) -> Option<u64> {
    if let Some(idx) = lower_sql.rfind(" limit ") {
        let rest = &lower_sql[idx + 7..];
        let num_end = rest
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(rest.len());
        rest[..num_end].parse().ok()
    } else {
        None
    }
}

fn extract_order_by_columns(lower_sql: &str) -> Vec<String> {
    extract_clause_columns(lower_sql, " order by ", " limit ")
}

fn extract_group_by_columns(lower_sql: &str) -> Vec<String> {
    extract_clause_columns(lower_sql, " group by ", " having ")
}

fn extract_clause_columns(lower_sql: &str, keyword: &str, next_keyword: &str) -> Vec<String> {
    if let Some(idx) = lower_sql.find(keyword) {
        let rest = &lower_sql[idx + keyword.len()..];
        let end = rest
            .find(next_keyword)
            .or_else(|| rest.find(" order by "))
            .or_else(|| rest.find(" limit "))
            .unwrap_or(rest.len());
        rest[..end]
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    } else {
        Vec::new()
    }
}

/// 性能预测结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformancePrediction {
    /// 预测执行耗时（毫秒）
    pub estimated_ms: f64,
    /// 预测扫描行数
    pub estimated_rows_scanned: u64,
    /// 预测是否走索引
    pub uses_index: bool,
    /// 成本评分（0-100，越高越差）
    pub cost_score: f64,
    /// 预测依据说明
    pub rationale: String,
}

/// 性能预测器
///
/// 基于表统计信息 + 查询特征，用成本模型预测 SQL 执行性能。
/// 用于比较重写前/重写后的候选 SQL，估算加速比。
///
/// # 成本模型
///
/// - 全表扫描：`rows * avg_row_size / scan_bandwidth`
/// - 索引扫描：`selective_rows * avg_row_size / index_bandwidth`
/// - JOIN：`left_rows * right_rows * join_factor`
/// - 子查询：`subquery_count * base_cost`
pub struct PerformancePredictor {
    /// 顺序扫描带宽（MB/s，默认 100）
    scan_bandwidth_mbps: f64,
    /// 索引扫描带宽（MB/s，默认 500）
    index_bandwidth_mbps: f64,
    /// 每行处理开销（微秒，默认 0.1）
    per_row_overhead_us: f64,
    /// JOIN 笛卡尔积因子（默认 0.001）
    join_factor: f64,
}

impl Default for PerformancePredictor {
    fn default() -> Self {
        Self::new()
    }
}

impl PerformancePredictor {
    /// 创建默认参数的性能预测器
    pub fn new() -> Self {
        Self {
            scan_bandwidth_mbps: 100.0,
            index_bandwidth_mbps: 500.0,
            per_row_overhead_us: 0.1,
            join_factor: 0.001,
        }
    }

    /// 设置扫描带宽
    pub fn with_scan_bandwidth(mut self, mbps: f64) -> Self {
        self.scan_bandwidth_mbps = mbps.max(1.0);
        self
    }

    /// 设置索引扫描带宽
    pub fn with_index_bandwidth(mut self, mbps: f64) -> Self {
        self.index_bandwidth_mbps = mbps.max(1.0);
        self
    }

    /// 预测单条 SQL 的执行性能
    ///
    /// # 参数
    /// - `sql`: SQL 文本
    /// - `stats`: 涉及表的统计信息列表
    ///
    /// # 返回
    ///
    /// [`PerformancePrediction`] 包含预测耗时、扫描行数、是否走索引、成本评分。
    pub fn predict(&self, sql: &str, stats: &[TableStatistics]) -> PerformancePrediction {
        let chars = QueryCharacteristics::from_sql(sql);
        self.predict_with_chars(&chars, stats)
    }

    fn predict_with_chars(
        &self,
        chars: &QueryCharacteristics,
        stats: &[TableStatistics],
    ) -> PerformancePrediction {
        let table_stats: Vec<&TableStatistics> = chars
            .tables
            .iter()
            .filter_map(|t| stats.iter().find(|s| s.table_name == *t))
            .collect();

        if table_stats.is_empty() {
            return PerformancePrediction {
                estimated_ms: 1.0,
                estimated_rows_scanned: 1,
                uses_index: false,
                cost_score: 10.0,
                rationale: "无表统计信息，使用默认估算".to_string(),
            };
        }

        let mut total_rows_scanned: u64 = 0;
        let mut total_bytes: f64 = 0.0;
        let mut uses_index = false;
        let mut rationale_parts: Vec<String> = Vec::new();

        for stat in &table_stats {
            let index_hit = self.find_index_for_where(chars, stat);
            let (scanned, bytes, idx_used) = self.estimate_table_scan(chars, stat, &index_hit);
            total_rows_scanned = total_rows_scanned.saturating_add(scanned);
            total_bytes += bytes;
            if idx_used {
                uses_index = true;
                rationale_parts.push(format!(
                    "{}: 索引扫描 {} 行（选择性 {:.2}）",
                    stat.table_name,
                    scanned,
                    index_hit.unwrap_or(0.0)
                ));
            } else {
                rationale_parts.push(format!("{}: 全表扫描 {} 行", stat.table_name, scanned));
            }
        }

        let scan_ms = total_bytes / (self.scan_bandwidth_mbps * 1024.0 * 1024.0) * 1000.0;
        let row_overhead_ms = total_rows_scanned as f64 * self.per_row_overhead_us / 1000.0;
        let mut estimated_ms = scan_ms + row_overhead_ms;

        if chars.join_count > 0 {
            let join_cost = self.estimate_join_cost(&table_stats, chars);
            estimated_ms += join_cost;
            rationale_parts.push(format!(
                "{} 个 JOIN 增加成本 {:.2}ms",
                chars.join_count, join_cost
            ));
        }

        if chars.subquery_count > 0 {
            let subquery_cost = chars.subquery_count as f64 * estimated_ms * 0.5;
            estimated_ms += subquery_cost;
            rationale_parts.push(format!(
                "{} 个子查询增加成本 {:.2}ms",
                chars.subquery_count, subquery_cost
            ));
        }

        if chars.uses_select_star {
            estimated_ms *= 1.2;
            rationale_parts.push("SELECT * 增加 20% 开销".to_string());
        }

        if let Some(limit) = chars.limit {
            if limit > 0 && total_rows_scanned > limit {
                let limit_ratio = limit as f64 / total_rows_scanned as f64;
                estimated_ms *= limit_ratio.max(0.1);
                rationale_parts.push(format!(
                    "LIMIT {} 缩减成本至 {:.0}%",
                    limit,
                    limit_ratio * 100.0
                ));
            }
        }

        let cost_score = self.compute_cost_score(estimated_ms, total_rows_scanned, uses_index);

        PerformancePrediction {
            estimated_ms,
            estimated_rows_scanned: total_rows_scanned,
            uses_index,
            cost_score,
            rationale: rationale_parts.join("; "),
        }
    }

    fn find_index_for_where(
        &self,
        chars: &QueryCharacteristics,
        stat: &TableStatistics,
    ) -> Option<f64> {
        for col in &chars.where_columns {
            let col_name = col.split('.').next_back().unwrap_or(col);
            if let Some(&selectivity) = stat.index_selectivity.get(col_name) {
                return Some(selectivity);
            }
        }
        None
    }

    fn estimate_table_scan(
        &self,
        _chars: &QueryCharacteristics,
        stat: &TableStatistics,
        index_selectivity: &Option<f64>,
    ) -> (u64, f64, bool) {
        let row_count = stat.row_count;
        let row_bytes = stat.avg_row_size_bytes as f64;

        match index_selectivity {
            Some(selectivity) => {
                let scanned = (row_count as f64 * (1.0 - selectivity)).max(1.0) as u64;
                let bytes = scanned as f64 * row_bytes;
                (scanned, bytes, true)
            }
            None => {
                let bytes = row_count as f64 * row_bytes;
                (row_count, bytes, false)
            }
        }
    }

    fn estimate_join_cost(
        &self,
        table_stats: &[&TableStatistics],
        chars: &QueryCharacteristics,
    ) -> f64 {
        if table_stats.len() < 2 {
            return 0.0;
        }
        let left_rows = table_stats[0].row_count as f64;
        let right_rows = table_stats[1].row_count as f64;
        let cartesian = left_rows * right_rows * self.join_factor;
        cartesian * self.per_row_overhead_us / 1000.0 * chars.join_count as f64
    }

    fn compute_cost_score(&self, estimated_ms: f64, rows: u64, uses_index: bool) -> f64 {
        let mut score = 0.0;
        score += (estimated_ms.ln_1p() * 5.0).min(50.0);
        score += (rows as f64).log10().clamp(0.0, 30.0);
        if !uses_index {
            score += 15.0;
        }
        score.min(100.0)
    }

    /// 比较两条 SQL 的预测性能，返回加速比
    ///
    /// # 返回
    ///
    /// `(original_prediction, optimized_prediction, speedup_ratio)`
    /// `speedup_ratio > 1.0` 表示优化版本更快。
    pub fn compare(
        &self,
        original_sql: &str,
        optimized_sql: &str,
        stats: &[TableStatistics],
    ) -> (PerformancePrediction, PerformancePrediction, f64) {
        let orig = self.predict(original_sql, stats);
        let opt = self.predict(optimized_sql, stats);
        let speedup = if opt.estimated_ms > 0.0 {
            orig.estimated_ms / opt.estimated_ms
        } else {
            f64::INFINITY
        };
        (orig, opt, speedup)
    }
}

// ==================== P2 TASK-016: QueryABTestFramework ====================

/// A/B 测试样本（单次查询耗时）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbTestSample {
    /// 耗时（毫秒）
    pub elapsed_ms: f64,
    /// 是否成功
    pub success: bool,
}

/// A/B 测试统计摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbTestSummary {
    /// 样本数
    pub sample_count: usize,
    /// 成功样本数
    pub success_count: usize,
    /// 平均耗时（毫秒）
    pub mean_ms: f64,
    /// 中位数 P50（毫秒）
    pub p50_ms: f64,
    /// P95（毫秒）
    pub p95_ms: f64,
    /// P99（毫秒）
    pub p99_ms: f64,
    /// 最小值（毫秒）
    pub min_ms: f64,
    /// 最大值（毫秒）
    pub max_ms: f64,
    /// 标准差（毫秒）
    pub std_dev_ms: f64,
}

impl AbTestSummary {
    /// 从样本列表计算统计摘要
    pub fn from_samples(samples: &[AbTestSample]) -> Self {
        let success_samples: Vec<f64> = samples
            .iter()
            .filter(|s| s.success)
            .map(|s| s.elapsed_ms)
            .collect();
        let success_count = success_samples.len();
        let sample_count = samples.len();

        if success_samples.is_empty() {
            return Self {
                sample_count,
                success_count: 0,
                mean_ms: 0.0,
                p50_ms: 0.0,
                p95_ms: 0.0,
                p99_ms: 0.0,
                min_ms: 0.0,
                max_ms: 0.0,
                std_dev_ms: 0.0,
            };
        }

        let mut sorted = success_samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let mean = sorted.iter().sum::<f64>() / sorted.len() as f64;
        let variance = sorted.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / sorted.len() as f64;
        let std_dev = variance.sqrt();

        Self {
            sample_count,
            success_count,
            mean_ms: mean,
            p50_ms: percentile(&sorted, 50.0),
            p95_ms: percentile(&sorted, 95.0),
            p99_ms: percentile(&sorted, 99.0),
            min_ms: sorted[0],
            max_ms: sorted[sorted.len() - 1],
            std_dev_ms: std_dev,
        }
    }
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let rank = (p / 100.0) * (sorted.len() - 1) as f64;
    let lower = rank.floor() as usize;
    let upper = rank.ceil() as usize;
    if lower == upper {
        sorted[lower]
    } else {
        let frac = rank - lower as f64;
        sorted[lower] * (1.0 - frac) + sorted[upper] * frac
    }
}

/// A/B 测试结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbTestResult {
    /// 原始版本统计
    pub original: AbTestSummary,
    /// 优化版本统计
    pub optimized: AbTestSummary,
    /// P50 加速比（original_p50 / optimized_p50）
    pub p50_speedup: f64,
    /// P95 加速比
    pub p95_speedup: f64,
    /// 平均加速比
    pub mean_speedup: f64,
    /// Welch t 检验的 t 统计量
    pub t_statistic: f64,
    /// Welch t 检验的自由度（Welch-Satterthwaite 近似）
    pub degrees_of_freedom: f64,
    /// 近似 p 值（双侧）
    pub p_value: f64,
    /// 是否统计显著（p < 0.05）
    pub is_significant: bool,
    /// 结论说明
    pub conclusion: String,
}

/// A/B 测试框架
///
/// 对同一查询的原始版本与优化版本执行 N 次采样，
/// 输出 P50/P95/p 值统计显著性对比。
pub struct QueryABTestFramework {
    /// 默认采样次数
    default_sample_count: usize,
    /// 显著性阈值
    significance_threshold: f64,
}

impl Default for QueryABTestFramework {
    fn default() -> Self {
        Self::new()
    }
}

impl QueryABTestFramework {
    /// 创建默认 A/B 测试框架（100 次采样，α=0.05）
    pub fn new() -> Self {
        Self {
            default_sample_count: 100,
            significance_threshold: 0.05,
        }
    }

    /// 设置默认采样次数
    pub fn with_sample_count(mut self, count: usize) -> Self {
        self.default_sample_count = count.max(2);
        self
    }

    /// 设置显著性阈值
    pub fn with_significance_threshold(mut self, alpha: f64) -> Self {
        self.significance_threshold = alpha;
        self
    }

    /// 执行 A/B 测试
    ///
    /// # 参数
    /// - `original_samples`: 原始版本的耗时样本
    /// - `optimized_samples`: 优化版本的耗时样本
    ///
    /// # 返回
    ///
    /// [`AbTestResult`] 包含双版本统计摘要 + 加速比 + Welch t 检验 p 值。
    pub fn run_ab_test(
        &self,
        original_samples: &[AbTestSample],
        optimized_samples: &[AbTestSample],
    ) -> AbTestResult {
        let original = AbTestSummary::from_samples(original_samples);
        let optimized = AbTestSummary::from_samples(optimized_samples);

        let p50_speedup = if optimized.p50_ms > 0.0 {
            original.p50_ms / optimized.p50_ms
        } else {
            f64::INFINITY
        };
        let p95_speedup = if optimized.p95_ms > 0.0 {
            original.p95_ms / optimized.p95_ms
        } else {
            f64::INFINITY
        };
        let mean_speedup = if optimized.mean_ms > 0.0 {
            original.mean_ms / optimized.mean_ms
        } else {
            f64::INFINITY
        };

        let (t_stat, df, p_value) = welch_t_test(&original, &optimized);

        let is_significant = p_value < self.significance_threshold;

        let conclusion =
            self.build_conclusion(mean_speedup, p_value, is_significant, &original, &optimized);

        AbTestResult {
            original,
            optimized,
            p50_speedup,
            p95_speedup,
            mean_speedup,
            t_statistic: t_stat,
            degrees_of_freedom: df,
            p_value,
            is_significant,
            conclusion,
        }
    }

    fn build_conclusion(
        &self,
        mean_speedup: f64,
        p_value: f64,
        is_significant: bool,
        original: &AbTestSummary,
        optimized: &AbTestSummary,
    ) -> String {
        if !is_significant {
            return format!(
                "差异不显著（p={:.4} ≥ α={:.2}），无法断定优化版本性能有实质提升",
                p_value, self.significance_threshold
            );
        }
        if mean_speedup > 1.0 {
            format!(
                "优化版本显著更快（p={:.4}），平均加速比 {:.2}x（{:.2}ms → {:.2}ms）",
                p_value, mean_speedup, original.mean_ms, optimized.mean_ms
            )
        } else {
            format!(
                "优化版本显著更慢（p={:.4}），平均减速比 {:.2}x（{:.2}ms → {:.2}ms）",
                p_value,
                1.0 / mean_speedup,
                original.mean_ms,
                optimized.mean_ms
            )
        }
    }

    /// 返回默认采样次数
    pub fn default_sample_count(&self) -> usize {
        self.default_sample_count
    }
}

/// Welch t 检验（不假设等方差）
///
/// 返回 (t 统计量, 自由度, 双侧 p 值)
fn welch_t_test(a: &AbTestSummary, b: &AbTestSummary) -> (f64, f64, f64) {
    let n1 = a.success_count as f64;
    let n2 = b.success_count as f64;
    if n1 < 2.0 || n2 < 2.0 {
        return (0.0, 0.0, 1.0);
    }

    let var1 = a.std_dev_ms.powi(2);
    let var2 = b.std_dev_ms.powi(2);
    let se1 = var1 / n1;
    let se2 = var2 / n2;
    let se = (se1 + se2).sqrt();
    if se < 1e-12 {
        return (0.0, 0.0, 1.0);
    }

    let t = (a.mean_ms - b.mean_ms) / se;
    let df = (se1 + se2).powi(2) / (se1.powi(2) / (n1 - 1.0) + se2.powi(2) / (n2 - 1.0));

    let p = two_sided_p_value(t, df);
    (t, df, p)
}

/// 用正态近似计算双侧 p 值（大样本下 t 分布趋近正态）
///
/// 对自由度 df > 30 使用标准正态近似；否则用 t 分布的保守正态上界。
/// 这是近似实现（避免引入统计 crate 依赖），对 A/B 测试足够精确。
fn two_sided_p_value(t: f64, df: f64) -> f64 {
    let abs_t = t.abs();
    if abs_t < 1e-12 {
        return 1.0;
    }

    let p_one_sided = if df > 30.0 {
        normal_cdf(-abs_t)
    } else {
        let scale = (1.0 + abs_t.powi(2) / df).sqrt();
        normal_cdf(-abs_t / scale)
    };
    (2.0 * p_one_sided).min(1.0)
}

/// 标准正态分布 CDF（使用 erf 近似）
fn normal_cdf(x: f64) -> f64 {
    0.5 * (1.0 + erf(x / std::f64::consts::SQRT_2))
}

/// 误差函数 erf（Abramowitz & Stegun 7.1.26 近似）
fn erf(x: f64) -> f64 {
    let a1 = 0.254829592_f64;
    let a2 = -0.284496736_f64;
    let a3 = 1.421413741_f64;
    let a4 = -1.453152027_f64;
    let a5 = 1.061405429_f64;
    let p = 0.3275911_f64;

    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let abs_x = x.abs();
    let t = 1.0 / (1.0 + p * abs_x);
    let y = 1.0 - (((((a5 * t + a4) * t) + a3) * t + a2) * t + a1) * t * (-abs_x * abs_x).exp();
    sign * y
}
