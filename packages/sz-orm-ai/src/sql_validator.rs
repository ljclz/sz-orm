//! SQL 结果验证模块
//!
//! 对 NL2SQL 生成的 SQL 进行安全验证 + 语法验证 + EXPLAIN 验证。
//!
//! 启用 `ai-nl2sql-enhanced` feature 后可用 `StaticSqlValidator`（纯文本安全检查）。
//! 启用 `ai-schema-extract` feature 后额外可用 `ExplainSqlValidator`（连 DB 执行 EXPLAIN）。
//!
//! ## 使用方式
//!
//! ```ignore
//! use sz_orm_ai::StaticSqlValidator;
//!
//! let validator = StaticSqlValidator::new();
//! let result = validator.validate("SELECT * FROM users WHERE id = $1", None).await?;
//! assert!(result.is_valid);
//! ```

use crate::nl2sql::{Nl2SqlEngine, Nl2SqlError, SqlDialect};

// ==================== 验证结果 ====================

/// SQL 验证结果
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// 是否通过验证
    pub is_valid: bool,
    /// 错误列表（验证失败的原因）
    pub errors: Vec<String>,
    /// 修正建议列表
    pub fix_suggestions: Vec<String>,
    /// 验证来源（static / explain / llm）
    pub source: ValidationSource,
}

/// 验证来源
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationSource {
    /// 静态文本检查
    Static,
    /// EXPLAIN 执行验证
    Explain,
    /// LLM 辅助验证
    Llm,
}

impl ValidationResult {
    /// 创建通过的结果
    pub fn valid(source: ValidationSource) -> Self {
        Self {
            is_valid: true,
            errors: Vec::new(),
            fix_suggestions: Vec::new(),
            source,
        }
    }

    /// 创建失败的结果
    pub fn invalid(
        errors: Vec<String>,
        fix_suggestions: Vec<String>,
        source: ValidationSource,
    ) -> Self {
        Self {
            is_valid: false,
            errors,
            fix_suggestions,
            source,
        }
    }

    /// 合并两个验证结果（任一失败则失败）
    pub fn merge(self, other: Self) -> Self {
        let is_valid = self.is_valid && other.is_valid;
        let mut errors = self.errors;
        errors.extend(other.errors);
        let mut fix_suggestions = self.fix_suggestions;
        fix_suggestions.extend(other.fix_suggestions);
        Self {
            is_valid,
            errors,
            fix_suggestions,
            source: other.source,
        }
    }
}

// ==================== SqlValidator trait ====================

/// SQL 验证器 trait
///
/// 所有验证器实现此 trait，用于验证 NL2SQL 生成的 SQL。
#[async_trait::async_trait]
pub trait SqlValidator: Send + Sync {
    /// 验证 SQL
    ///
    /// # 参数
    /// - `sql`: 待验证的 SQL
    /// - `dialect`: 目标方言（可选）
    ///
    /// # 返回值
    /// - `Ok(ValidationResult)`: 验证结果
    /// - `Err(Nl2SqlError)`: 验证过程出错
    async fn validate(
        &self,
        sql: &str,
        dialect: Option<SqlDialect>,
    ) -> Result<ValidationResult, Nl2SqlError>;
}

// ==================== StaticSqlValidator ====================

/// 静态 SQL 验证器（纯文本检查，无需 DB 连接）
///
/// 执行以下检查：
/// 1. 只允许 SELECT 语句（拒绝 INSERT/UPDATE/DELETE/DROP/ALTER 等）
/// 2. 无 SQL 注入风险（检测未参数化的字符串拼接）
/// 3. 基本语法检查（包含 FROM 关键字等）
pub struct StaticSqlValidator {
    /// 是否允许非 SELECT 语句
    allow_non_select: bool,
}

impl Default for StaticSqlValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl StaticSqlValidator {
    /// 创建静态验证器（默认禁止非 SELECT）
    pub fn new() -> Self {
        Self {
            allow_non_select: false,
        }
    }

    /// 允许非 SELECT 语句（如 INSERT/UPDATE/DELETE）
    pub fn allow_non_select(mut self) -> Self {
        self.allow_non_select = true;
        self
    }

    /// 检查是否为 SELECT 语句
    fn is_select(sql: &str) -> bool {
        let trimmed = sql.trim().to_lowercase();
        trimmed.starts_with("select") || trimmed.starts_with("with")
    }

    /// 检测 SQL 注入风险
    ///
    /// 检测模式：
    /// - 单引号后跟 OR 1=1 / AND 1=1
    /// - 注释符号 -- / /* */
    /// - 分号后跟语句（堆叠注入）
    fn detect_injection(sql: &str) -> Vec<String> {
        let lower = sql.to_lowercase();
        let mut risks = Vec::new();

        // 检测 OR 1=1 / AND 1=1 模式
        if lower.contains("' or 1=1") || lower.contains("' or '1'='1") {
            risks.push("检测到 OR 1=1 注入模式".to_string());
        }
        if lower.contains("' and 1=1") || lower.contains("' and '1'='1") {
            risks.push("检测到 AND 1=1 注入模式".to_string());
        }

        // 检测堆叠注入（分号后跟语句）
        if let Some(semicolon_pos) = lower.find(';') {
            let after = &lower[semicolon_pos + 1..].trim();
            if !after.is_empty()
                && (after.starts_with("drop")
                    || after.starts_with("delete")
                    || after.starts_with("update")
                    || after.starts_with("insert")
                    || after.starts_with("alter"))
            {
                risks.push("检测到堆叠注入（分号后跟 DDL/DML）".to_string());
            }
        }

        // 检测 UNION 注入
        if lower.contains("' union select") || lower.contains("') union select") {
            risks.push("检测到 UNION 注入模式".to_string());
        }

        risks
    }

    /// 基本语法检查
    fn check_basic_syntax(sql: &str) -> Vec<String> {
        let lower = sql.trim().to_lowercase();
        let mut errors = Vec::new();

        if lower.is_empty() {
            errors.push("SQL 为空".to_string());
            return errors;
        }

        // SELECT 语句应包含 FROM（除非 SELECT 1 这种常量查询）
        if lower.starts_with("select") && !lower.contains("from") && !lower.contains("select ") {
            errors.push("SELECT 语句缺少 FROM 关键字".to_string());
        }

        // 括号不匹配
        let open_count = sql.matches('(').count();
        let close_count = sql.matches(')').count();
        if open_count != close_count {
            errors.push(format!(
                "括号不匹配：{} 个 '(' vs {} 个 ')'",
                open_count, close_count
            ));
        }

        errors
    }
}

#[async_trait::async_trait]
impl SqlValidator for StaticSqlValidator {
    async fn validate(
        &self,
        sql: &str,
        _dialect: Option<SqlDialect>,
    ) -> Result<ValidationResult, Nl2SqlError> {
        let mut errors = Vec::new();
        let mut fix_suggestions = Vec::new();

        // 1. 检查是否为 SELECT
        if !self.allow_non_select && !Self::is_select(sql) {
            errors.push("非 SELECT 语句被禁止".to_string());
            fix_suggestions.push("仅允许 SELECT 查询，请修改为 SELECT 语句".to_string());
        }

        // 2. 检测 SQL 注入
        let injection_risks = Self::detect_injection(sql);
        errors.extend(injection_risks.iter().map(|r| format!("安全风险：{}", r)));
        if !injection_risks.is_empty() {
            fix_suggestions.push("使用参数化查询（$1, ? 占位符）替代字符串拼接".to_string());
        }

        // 3. 基本语法检查
        let syntax_errors = Self::check_basic_syntax(sql);
        errors.extend(syntax_errors);

        if errors.is_empty() {
            Ok(ValidationResult::valid(ValidationSource::Static))
        } else {
            Ok(ValidationResult::invalid(
                errors,
                fix_suggestions,
                ValidationSource::Static,
            ))
        }
    }
}

// ==================== ExplainSqlValidator（需要 Connection） ====================

#[cfg(feature = "ai-schema-extract")]
mod explain_validator {
    use super::*;
    use sz_orm_core::Connection;

    /// EXPLAIN SQL 验证器（需要 DB 连接）
    ///
    /// 按方言执行 EXPLAIN 验证 SQL 语法正确性 + 执行计划合理性。
    ///
    /// 启用 `ai-schema-extract` feature 后可用。
    pub struct ExplainSqlValidator {
        /// 是否同时执行静态安全检查
        check_safety: bool,
    }

    impl Default for ExplainSqlValidator {
        fn default() -> Self {
            Self::new()
        }
    }

    impl ExplainSqlValidator {
        /// 创建 EXPLAIN 验证器
        pub fn new() -> Self {
            Self { check_safety: true }
        }

        /// 跳过静态安全检查（仅执行 EXPLAIN）
        pub fn skip_safety_check(mut self) -> Self {
            self.check_safety = false;
            self
        }

        /// 按方言构建 EXPLAIN 语句
        fn build_explain(sql: &str, dialect: SqlDialect) -> String {
            match dialect {
                SqlDialect::MySQL | SqlDialect::PostgreSQL => format!("EXPLAIN {}", sql),
                SqlDialect::Sqlite => format!("EXPLAIN QUERY PLAN {}", sql),
                SqlDialect::Oracle => format!("EXPLAIN PLAN FOR {}", sql),
                SqlDialect::SqlServer => format!("SET SHOWPLAN_TEXT ON; {}", sql),
            }
        }
    }

    impl ExplainSqlValidator {
        /// 使用 Connection 执行 EXPLAIN 验证
        ///
        /// # 参数
        /// - `sql`: 待验证的 SQL
        /// - `conn`: 数据库连接
        /// - `dialect`: 目标方言
        pub async fn validate_with_conn(
            &self,
            sql: &str,
            conn: &mut dyn Connection,
            dialect: SqlDialect,
        ) -> Result<ValidationResult, Nl2SqlError> {
            let mut errors = Vec::new();
            let mut fix_suggestions = Vec::new();

            // 1. 静态安全检查
            if self.check_safety {
                let static_validator = StaticSqlValidator::new();
                let static_result = static_validator.validate(sql, Some(dialect)).await?;
                if !static_result.is_valid {
                    return Ok(static_result);
                }
            }

            // 2. EXPLAIN 验证
            let explain_sql = Self::build_explain(sql, dialect);
            let explain_result = conn.query(&explain_sql).await;

            match explain_result {
                Ok(_) => Ok(ValidationResult::valid(ValidationSource::Explain)),
                Err(e) => {
                    errors.push(format!("EXPLAIN 执行失败：{}", e));
                    fix_suggestions.push("检查 SQL 语法是否正确，确认表名/列名存在".to_string());
                    Ok(ValidationResult::invalid(
                        errors,
                        fix_suggestions,
                        ValidationSource::Explain,
                    ))
                }
            }
        }
    }
}

#[cfg(feature = "ai-schema-extract")]
pub use explain_validator::ExplainSqlValidator;

// ==================== 验证重试包装器 ====================

/// 验证重试包装器
///
/// 在 NL2SQL 生成后自动调用验证器，验证失败时请求重新生成（最多 N 次）。
pub struct ValidatedNl2SqlEngine<E: Nl2SqlEngine, V: SqlValidator> {
    /// 内部 NL2SQL 引擎
    engine: E,
    /// 验证器
    validator: V,
    /// 最大重试次数（默认 3）
    max_retries: usize,
}

impl<E: Nl2SqlEngine, V: SqlValidator> ValidatedNl2SqlEngine<E, V> {
    /// 创建验证包装器
    pub fn new(engine: E, validator: V) -> Self {
        Self {
            engine,
            validator,
            max_retries: 3,
        }
    }

    /// 设置最大重试次数
    pub fn with_max_retries(mut self, max_retries: usize) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// 获取内部引擎引用
    pub fn engine(&self) -> &E {
        &self.engine
    }

    /// 获取验证器引用
    pub fn validator(&self) -> &V {
        &self.validator
    }

    /// 带验证的 SQL 生成
    ///
    /// 生成 SQL 后自动验证，验证失败时重新生成（最多 max_retries 次）。
    /// 安全验证失败（非 SELECT/注入风险）直接拒绝返回。
    pub async fn generate_validated(
        &self,
        nl_query: &str,
        schema: &crate::nl2sql::SchemaContext,
        dialect: Option<SqlDialect>,
    ) -> Result<crate::nl2sql::SqlQuery, Nl2SqlError> {
        let mut last_error = Nl2SqlError::GenerationError("未执行任何尝试".to_string());

        for attempt in 0..=self.max_retries {
            // 生成 SQL
            let query = if let Some(d) = dialect {
                self.engine
                    .generate_with_dialect(nl_query, schema, d)
                    .await?
            } else {
                self.engine.generate(nl_query, schema).await?
            };

            // 验证
            let validation = self.validator.validate(&query.sql, dialect).await?;

            if validation.is_valid {
                return Ok(query);
            }

            // 安全验证失败（非 SELECT/注入）直接拒绝
            let is_safety_failure = validation
                .errors
                .iter()
                .any(|e| e.contains("安全风险") || e.contains("非 SELECT"));
            if is_safety_failure {
                let suggestions = if validation.fix_suggestions.is_empty() {
                    "无".to_string()
                } else {
                    validation.fix_suggestions.join("; ")
                };
                return Err(Nl2SqlError::SafetyError(format!(
                    "安全验证失败：{}；建议：{}",
                    validation.errors.join("; "),
                    suggestions
                )));
            }

            let suggestions = if validation.fix_suggestions.is_empty() {
                "无".to_string()
            } else {
                validation.fix_suggestions.join("; ")
            };
            last_error = Nl2SqlError::GenerationError(format!(
                "第 {} 次验证失败：{}；修正建议：{}",
                attempt + 1,
                validation.errors.join("; "),
                suggestions
            ));
        }

        Err(last_error)
    }
}
