//! 动态 Filter（Hibernate @Filter / @FilterDef 风格）
//!
//! 对应文档 6.8 节改进项 40（@Filter 动态 Filter）。
//!
//! # 核心概念
//!
//! - **FilterDef**：Filter 定义（名称 + 参数列表 + WHERE 子句模板）
//! - **FilterRegistry**：Filter 注册表（管理 FilterDef + 启用状态 + 参数）
//! - **apply_filters**：将所有启用的 Filter 追加到 SQL 的 WHERE 子句
//!
//! 与 GlobalScope 的区别：
//! - GlobalScope 是 Model 类型系统层面的"编译期"作用域
//! - @Filter 是运行时可启用/禁用的动态 Filter，支持参数化
//!
//! # 设计灵感
//!
//! - Hibernate `@FilterDef` + `@Filter`
//! - JPA `EntityGraph`
//! - Rails `default_scope` + `unscoped`
//!
//! # 使用示例
//!
//! ```no_run
//! use sz_orm_core::dynamic_filter::{FilterDef, FilterRegistry, FilterParam};
//! use sz_orm_core::Value;
//! use std::collections::HashMap;
//!
//! // 1. 定义 Filter（类似 @FilterDef + @Filter）
//! let filter = FilterDef::new("active_users")
//!     .with_condition("status = :status")
//!     .with_param(FilterParam::new("status", "active"));
//!
//! // 2. 注册并启用
//! let mut registry = FilterRegistry::new();
//! registry.register(filter);
//! registry.enable("active_users", HashMap::new());
//!
//! // 3. 应用到 SQL
//! let sql = registry.apply("SELECT * FROM users");
//! // → SELECT * FROM users WHERE status = 'active'
//! ```

use std::collections::HashMap;
use std::sync::RwLock;

// ============================================================================
// FilterParam — Filter 参数定义
// ============================================================================

/// Filter 参数定义
///
/// 描述 Filter WHERE 子句中 `:param_name` 占位符的类型与默认值。
#[derive(Debug, Clone)]
pub struct FilterParam {
    /// 参数名（对应 `:param_name` 占位符）
    pub name: String,
    /// 默认值（启用 Filter 但未传参时使用）
    pub default_value: Option<crate::Value>,
    /// 参数描述（用于文档）
    pub description: String,
}

impl FilterParam {
    /// 创建参数定义
    pub fn new(name: impl Into<String>, default_value: impl Into<crate::Value>) -> Self {
        Self {
            name: name.into(),
            default_value: Some(default_value.into()),
            description: String::new(),
        }
    }

    /// 创建无默认值的参数
    pub fn required(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            default_value: None,
            description: String::new(),
        }
    }

    /// 添加描述
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }
}

// ============================================================================
// FilterDef — Filter 定义
// ============================================================================

/// Filter 定义
///
/// 类似 Hibernate `@FilterDef` + `@Filter` 的组合。
/// 定义一个具名 Filter，包含 WHERE 子句模板和参数列表。
///
/// WHERE 子句模板中使用 `:param_name` 作为参数占位符。
#[derive(Debug, Clone)]
pub struct FilterDef {
    /// Filter 名称（唯一标识）
    pub name: String,
    /// WHERE 子句模板（如 `"status = :status"`、`"dept_id IN :dept_ids"`）
    pub condition: String,
    /// 参数列表
    pub params: Vec<FilterParam>,
    /// 描述（用于文档/调试）
    pub description: String,
    /// 默认表名（可选，用于多表 SQL 时指定应用到哪个表）
    pub table: Option<String>,
}

impl FilterDef {
    /// 创建 Filter 定义
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            condition: String::new(),
            params: Vec::new(),
            description: String::new(),
            table: None,
        }
    }

    /// 设置 WHERE 子句模板
    pub fn with_condition(mut self, condition: impl Into<String>) -> Self {
        self.condition = condition.into();
        self
    }

    /// 添加参数
    pub fn with_param(mut self, param: FilterParam) -> Self {
        self.params.push(param);
        self
    }

    /// 设置描述
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    /// 设置目标表名
    pub fn with_table(mut self, table: impl Into<String>) -> Self {
        self.table = Some(table.into());
        self
    }

    /// 查找参数定义
    pub fn find_param(&self, name: &str) -> Option<&FilterParam> {
        self.params.iter().find(|p| p.name == name)
    }
}

// ============================================================================
// FilterError — Filter 错误类型
// ============================================================================

/// Filter 错误类型
#[derive(Debug)]
pub enum FilterError {
    /// Filter 未注册
    NotRegistered(String),
    /// 缺少必需参数
    MissingParam {
        /// Filter 名
        filter: String,
        /// 参数名
        param: String,
    },
    /// 参数类型不匹配
    ParamTypeMismatch {
        /// Filter 名
        filter: String,
        /// 参数名
        param: String,
        /// 期望类型
        expected: String,
    },
    /// Filter 已启用
    AlreadyEnabled(String),
    /// Filter 未启用
    NotEnabled(String),
    /// 条件模板解析错误
    TemplateError(String),
}

impl std::fmt::Display for FilterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FilterError::NotRegistered(n) => write!(f, "Filter `{}` not registered", n),
            FilterError::MissingParam { filter, param } => {
                write!(f, "Filter `{}` missing required param: `{}`", filter, param)
            }
            FilterError::ParamTypeMismatch {
                filter,
                param,
                expected,
            } => write!(
                f,
                "Filter `{}` param `{}` type mismatch, expected: {}",
                filter, param, expected
            ),
            FilterError::AlreadyEnabled(n) => write!(f, "Filter `{}` already enabled", n),
            FilterError::NotEnabled(n) => write!(f, "Filter `{}` not enabled", n),
            FilterError::TemplateError(msg) => write!(f, "Filter template error: {}", msg),
        }
    }
}

impl std::error::Error for FilterError {}

/// Filter 结果
pub type FilterResult<T> = Result<T, FilterError>;

// ============================================================================
// EnabledFilter — 已启用的 Filter 实例
// ============================================================================

/// 已启用的 Filter 实例（携带运行时参数）
#[derive(Debug, Clone)]
pub struct EnabledFilter {
    /// Filter 名称
    pub name: String,
    /// 运行时参数值
    pub params: HashMap<String, crate::Value>,
}

// ============================================================================
// FilterRegistry — Filter 注册表
// ============================================================================

/// Filter 注册表 — 管理 FilterDef + 启用状态 + 运行时参数
///
/// 线程安全：内部使用 RwLock，可在多线程环境下共享。
///
/// # 示例
///
/// ```
/// use sz_orm_core::dynamic_filter::{FilterDef, FilterRegistry, FilterParam};
/// use sz_orm_core::Value;
/// use std::collections::HashMap;
///
/// let mut registry = FilterRegistry::new();
///
/// // 1. 注册 Filter
/// let filter = FilterDef::new("active_only")
///     .with_condition("status = :status")
///     .with_param(FilterParam::new("status", "active"));
/// registry.register(filter);
///
/// // 2. 启用 Filter（可覆盖默认参数）
/// let mut params = HashMap::new();
/// params.insert("status".to_string(), Value::String("pending".to_string()));
/// registry.enable("active_only", params).unwrap();
///
/// // 3. 应用到 SQL
/// let sql = registry.apply("SELECT * FROM orders");
/// assert!(sql.contains("status = 'pending'"));
/// ```
pub struct FilterRegistry {
    /// 已注册的 Filter 定义
    defs: RwLock<HashMap<String, FilterDef>>,
    /// 已启用的 Filter 实例（按启用顺序）
    enabled: RwLock<Vec<EnabledFilter>>,
}

impl Default for FilterRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl FilterRegistry {
    /// 创建空注册表
    pub fn new() -> Self {
        Self {
            defs: RwLock::new(HashMap::new()),
            enabled: RwLock::new(Vec::new()),
        }
    }

    /// 注册 Filter 定义
    pub fn register(&self, def: FilterDef) {
        if let Ok(mut defs) = self.defs.write() {
            defs.insert(def.name.clone(), def);
        }
    }

    /// 注销 Filter 定义
    pub fn unregister(&self, name: &str) -> bool {
        if let Ok(mut defs) = self.defs.write() {
            defs.remove(name).is_some()
        } else {
            false
        }
    }

    /// 启用 Filter
    ///
    /// `params` 中的值会覆盖 FilterDef 中的默认值；缺失参数则使用默认值；
    /// 若必需参数（无默认值）缺失，返回 `Err`。
    pub fn enable(&self, name: &str, params: HashMap<String, crate::Value>) -> FilterResult<()> {
        // 检查是否已注册
        let def = {
            let defs = self
                .defs
                .read()
                .map_err(|_| FilterError::NotRegistered(name.to_string()))?;
            defs.get(name)
                .ok_or_else(|| FilterError::NotRegistered(name.to_string()))?
                .clone()
        };

        // 检查是否已启用
        {
            let enabled = self
                .enabled
                .read()
                .map_err(|_| FilterError::NotRegistered(name.to_string()))?;
            if enabled.iter().any(|e| e.name == name) {
                return Err(FilterError::AlreadyEnabled(name.to_string()));
            }
        }

        // 合并参数：params > default_value
        let mut merged = HashMap::new();
        for param_def in &def.params {
            if let Some(v) = params.get(&param_def.name) {
                merged.insert(param_def.name.clone(), v.clone());
            } else if let Some(dv) = &param_def.default_value {
                merged.insert(param_def.name.clone(), dv.clone());
            } else {
                return Err(FilterError::MissingParam {
                    filter: name.to_string(),
                    param: param_def.name.clone(),
                });
            }
        }

        if let Ok(mut enabled) = self.enabled.write() {
            enabled.push(EnabledFilter {
                name: name.to_string(),
                params: merged,
            });
        }
        Ok(())
    }

    /// 禁用 Filter
    pub fn disable(&self, name: &str) -> FilterResult<()> {
        if let Ok(mut enabled) = self.enabled.write() {
            let before = enabled.len();
            enabled.retain(|e| e.name != name);
            if enabled.len() == before {
                Err(FilterError::NotEnabled(name.to_string()))
            } else {
                Ok(())
            }
        } else {
            Err(FilterError::NotEnabled(name.to_string()))
        }
    }

    /// 检查 Filter 是否已启用
    pub fn is_enabled(&self, name: &str) -> bool {
        self.enabled
            .read()
            .map(|e| e.iter().any(|f| f.name == name))
            .unwrap_or(false)
    }

    /// 已启用的 Filter 数量
    pub fn enabled_count(&self) -> usize {
        self.enabled.read().map(|e| e.len()).unwrap_or(0)
    }

    /// 已注册的 Filter 数量
    pub fn registered_count(&self) -> usize {
        self.defs.read().map(|d| d.len()).unwrap_or(0)
    }

    /// 列出所有已注册 Filter 名称
    pub fn registered_names(&self) -> Vec<String> {
        self.defs
            .read()
            .map(|d| d.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// 列出所有已启用 Filter 名称
    pub fn enabled_names(&self) -> Vec<String> {
        self.enabled
            .read()
            .map(|e| e.iter().map(|f| f.name.clone()).collect())
            .unwrap_or_default()
    }

    /// 清空所有已启用 Filter（不影响已注册定义）
    pub fn clear_enabled(&self) {
        if let Ok(mut e) = self.enabled.write() {
            e.clear();
        }
    }

    /// 将所有已启用的 Filter 应用到 SQL
    ///
    /// 按启用顺序依次追加 WHERE 子句（用 AND 连接）。
    ///
    /// # 安全性
    ///
    /// 本方法使用 PostgreSQL 方言的转义规则作为默认行为。
    /// **生产环境请使用 [`apply_with_dialect`]** 以获得方言感知的转义。
    pub fn apply(&self, sql: &str) -> String {
        // v0.2.2 修复 H-1：默认使用 PostgreSQL 方言（保持向后兼容）
        self.apply_with_dialect(sql, &crate::dialect::PostgreSqlDialect)
    }

    /// v0.2.2 修复 H-1：方言感知的 Filter 应用
    ///
    /// 与 [`apply`] 的区别：使用 `dialect.escape_string()` 转义参数值，
    /// 确保在所有方言下都安全（特别是 MySQL 默认配置下 backslash 转义）。
    pub fn apply_with_dialect(&self, sql: &str, dialect: &dyn crate::dialect::Dialect) -> String {
        let (clauses, has_error) = self.collect_clauses_with_dialect(dialect);
        if has_error || clauses.is_empty() {
            return sql.to_string();
        }
        append_filter_clauses(sql, &clauses)
    }

    /// 方言感知的子句收集
    fn collect_clauses_with_dialect(
        &self,
        dialect: &dyn crate::dialect::Dialect,
    ) -> (Vec<String>, bool) {
        let mut clauses = Vec::new();
        let mut has_error = false;

        let enabled = match self.enabled.read() {
            Ok(e) => e,
            Err(_) => return (clauses, true),
        };
        let defs = match self.defs.read() {
            Ok(d) => d,
            Err(_) => return (clauses, true),
        };

        for ef in enabled.iter() {
            if let Some(def) = defs.get(&ef.name) {
                let rendered = render_condition_with_dialect(&def.condition, &ef.params, dialect);
                if let Some(rendered) = rendered {
                    if !rendered.trim().is_empty() {
                        clauses.push(rendered);
                    }
                } else {
                    has_error = true;
                }
            }
        }

        (clauses, has_error)
    }
}

// ============================================================================
// render_condition — 渲染 WHERE 子句模板
// ============================================================================

/// 渲染 WHERE 子句模板，将 `:param_name` 替换为实际值
///
/// - `:param_name` → 替换为参数值的 SQL 字面量
/// - 未知参数 → 返回 None（表示渲染失败）
///
/// # 安全性
///
/// 本函数使用 PostgreSQL 方言的转义规则（`'` → `''`）作为默认行为。
/// 对 PostgreSQL/SQLite/Oracle/SQL Server 默认配置安全，
/// 但对 MySQL 默认配置（backslash 是转义字符）不安全。
/// **生产环境请使用 [`render_condition_with_dialect`]** 以获得方言感知的转义。
///
/// # 示例
///
/// ```
/// use sz_orm_core::dynamic_filter::render_condition;
/// use sz_orm_core::Value;
/// use std::collections::HashMap;
///
/// let mut params = HashMap::new();
/// params.insert("status".to_string(), Value::String("active".to_string()));
/// params.insert("min_age".to_string(), Value::I64(18));
///
/// let rendered = render_condition("status = :status AND age >= :min_age", &params);
/// assert_eq!(rendered.unwrap(), "status = 'active' AND age >= 18");
/// ```
pub fn render_condition(template: &str, params: &HashMap<String, crate::Value>) -> Option<String> {
    // v0.2.2 修复 H-1：默认使用 PostgreSQL 方言（保持向后兼容）
    render_condition_with_dialect(template, params, &crate::dialect::PostgreSqlDialect)
}

/// v0.2.2 修复 H-1：方言感知的 WHERE 子句模板渲染
///
/// 与 [`render_condition`] 的区别：使用 `dialect.escape_string()` 转义字符串参数，
/// 确保在所有方言下都安全（特别是 MySQL 默认配置下 backslash 转义）。
///
/// # 示例
///
/// ```
/// use sz_orm_core::dynamic_filter::render_condition_with_dialect;
/// use sz_orm_core::dialect::MySqlDialect;
/// use sz_orm_core::Value;
/// use std::collections::HashMap;
///
/// let mut params = HashMap::new();
/// params.insert("name".to_string(), Value::String("hello\\nworld".to_string()));
///
/// let rendered = render_condition_with_dialect("name = :name", &params, &MySqlDialect);
/// // MySQL 方言下反斜杠会被转义为 \\\\
/// assert!(rendered.unwrap().contains("hello\\\\nworld"));
/// ```
pub fn render_condition_with_dialect(
    template: &str,
    params: &HashMap<String, crate::Value>,
    dialect: &dyn crate::dialect::Dialect,
) -> Option<String> {
    let mut result = String::with_capacity(template.len() + 32);
    let bytes = template.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b':' {
            // 收集参数名（字母/数字/下划线）
            let mut j = i + 1;
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            if j > i + 1 {
                let param_name = &template[i + 1..j];
                // 未知参数时返回 None
                let value = params.get(param_name)?;
                result.push_str(&value.to_param_with_dialect(dialect));
                i = j;
                continue;
            }
        }
        result.push(bytes[i] as char);
        i += 1;
    }

    Some(result)
}

// ============================================================================
// append_filter_clauses — 追加 Filter 子句到 SQL
// ============================================================================

/// 将 Filter 子句追加到 SQL 的 WHERE 部分
///
/// 逻辑与 data_permission::append_where_clauses 相同，但避免循环依赖。
fn append_filter_clauses(sql: &str, clauses: &[String]) -> String {
    if clauses.is_empty() {
        return sql.to_string();
    }

    let combined = clauses.join(" AND ");
    let upper = sql.to_uppercase();

    let where_pos = find_keyword_pos(&upper, "WHERE");
    let group_by_pos = find_keyword_pos(&upper, "GROUP BY");
    let order_by_pos = find_keyword_pos(&upper, "ORDER BY");
    let limit_pos = find_keyword_pos(&upper, "LIMIT");
    let having_pos = find_keyword_pos(&upper, "HAVING");

    let end_pos = [group_by_pos, order_by_pos, limit_pos, having_pos]
        .iter()
        .filter_map(|x| *x)
        .min();

    if let Some(wp) = where_pos {
        let insert_pos = end_pos.unwrap_or(sql.len());
        let before = &sql[..wp + 5];
        let existing_clause = &sql[wp + 5..insert_pos];
        let after = &sql[insert_pos..];

        let trimmed_existing = existing_clause.trim();
        if trimmed_existing.is_empty() {
            format!("{} {}{}", before, combined, after)
        } else {
            format!(
                "{} ({} ) AND ({}){}",
                before, trimmed_existing, combined, after
            )
        }
    } else {
        let insert_pos = end_pos.unwrap_or(sql.len());
        let before = &sql[..insert_pos];
        let after = &sql[insert_pos..];
        let trimmed = before.trim_end();
        let sep = if trimmed.is_empty() { "" } else { " " };
        format!("{}{}WHERE {}{}", trimmed, sep, combined, after)
    }
}

/// 在 SQL 中查找指定关键字的位置（独立词匹配，大小写不敏感）
fn find_keyword_pos(sql: &str, keyword: &str) -> Option<usize> {
    let upper_sql = sql.to_uppercase();
    let kw_upper = keyword.to_uppercase();
    let kw_len = kw_upper.len();
    if kw_len == 0 || upper_sql.len() < kw_len {
        return None;
    }

    let bytes = upper_sql.as_bytes();
    let kw_bytes = kw_upper.as_bytes();

    let mut i = 0;
    while i + kw_len <= bytes.len() {
        if &bytes[i..i + kw_len] == kw_bytes {
            let prev_ok = i == 0 || !bytes[i - 1].is_ascii_alphanumeric() && bytes[i - 1] != b'_';
            let next_idx = i + kw_len;
            let next_ok = next_idx >= bytes.len()
                || !bytes[next_idx].is_ascii_alphanumeric() && bytes[next_idx] != b'_';
            if prev_ok && next_ok {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Value;

    // ===== FilterParam 测试 =====

    #[test]
    fn test_filter_param_new() {
        let p = FilterParam::new("status", "active");
        assert_eq!(p.name, "status");
        assert_eq!(p.default_value, Some(Value::String("active".to_string())));
    }

    #[test]
    fn test_filter_param_required() {
        let p = FilterParam::required("user_id");
        assert_eq!(p.name, "user_id");
        assert_eq!(p.default_value, None);
    }

    #[test]
    fn test_filter_param_with_description() {
        let p = FilterParam::new("status", "active").with_description("用户状态");
        assert_eq!(p.description, "用户状态");
    }

    // ===== FilterDef 测试 =====

    #[test]
    fn test_filter_def_builders() {
        let def = FilterDef::new("active_users")
            .with_condition("status = :status")
            .with_param(FilterParam::new("status", "active"))
            .with_description("只查询活跃用户")
            .with_table("users");

        assert_eq!(def.name, "active_users");
        assert_eq!(def.condition, "status = :status");
        assert_eq!(def.params.len(), 1);
        assert_eq!(def.description, "只查询活跃用户");
        assert_eq!(def.table, Some("users".to_string()));
    }

    #[test]
    fn test_filter_def_find_param() {
        let def = FilterDef::new("test")
            .with_condition("a = :a AND b = :b")
            .with_param(FilterParam::new("a", 1))
            .with_param(FilterParam::new("b", 2));

        assert!(def.find_param("a").is_some());
        assert!(def.find_param("b").is_some());
        assert!(def.find_param("c").is_none());
    }

    // ===== render_condition 测试 =====

    #[test]
    fn test_render_condition_single_string_param() {
        let mut params = HashMap::new();
        params.insert("status".to_string(), Value::String("active".to_string()));

        let rendered = render_condition("status = :status", &params);
        assert_eq!(rendered.unwrap(), "status = 'active'");
    }

    #[test]
    fn test_render_condition_single_i64_param() {
        let mut params = HashMap::new();
        params.insert("min_age".to_string(), Value::I64(18));

        let rendered = render_condition("age >= :min_age", &params);
        assert_eq!(rendered.unwrap(), "age >= 18");
    }

    #[test]
    fn test_render_condition_multiple_params() {
        let mut params = HashMap::new();
        params.insert("status".to_string(), Value::String("active".to_string()));
        params.insert("min_age".to_string(), Value::I64(18));
        params.insert("max_age".to_string(), Value::I64(65));

        let rendered = render_condition(
            "status = :status AND age >= :min_age AND age <= :max_age",
            &params,
        );
        assert_eq!(
            rendered.unwrap(),
            "status = 'active' AND age >= 18 AND age <= 65"
        );
    }

    #[test]
    fn test_render_condition_no_params() {
        let params = HashMap::new();
        let rendered = render_condition("1 = 1", &params);
        assert_eq!(rendered.unwrap(), "1 = 1");
    }

    #[test]
    fn test_render_condition_unknown_param_returns_none() {
        let params = HashMap::new();
        let rendered = render_condition("status = :status", &params);
        assert!(rendered.is_none());
    }

    /// 验证 render_condition 对抗 SQL 注入：
    /// 用户输入 `' OR 1=1 --` 经过 escape_string + to_param 后，
    /// 应被包成一个完整的字符串字面量，不会被解析为 SQL 代码。
    #[test]
    fn test_render_condition_defends_against_sql_injection() {
        let mut params = HashMap::new();
        // 模拟恶意用户输入
        params.insert("name".to_string(), Value::String("' OR 1=1 --".to_string()));

        let rendered = render_condition("name = :name", &params).unwrap();
        // 整个值应是一个字符串字面量，不包含未转义的 SQL 代码
        // 验证：原 `'` 被转义为 `''`，整个值被包在 `'...'` 中
        assert_eq!(rendered, "name = ''' OR 1=1 --'");
        // 关键：渲染结果中不应出现"裸露的 OR 1=1"作为 SQL 代码
        // （即 OR 1=1 应在字符串字面量内部，而非 SQL 操作符）
        // 通过查找第一个 `=` 后的内容来验证（值部分）
        // 注意：不能用 split('=').nth(1) 因为值中可能包含 `=`（如 1=1）
        let eq_pos = rendered.find('=').unwrap();
        let after_eq = rendered[eq_pos + 1..].trim();
        assert!(
            after_eq.starts_with('\''),
            "value should start with quote, got: {}",
            after_eq
        );
        assert!(
            after_eq.ends_with('\''),
            "value should end with quote, got: {}",
            after_eq
        );
        // 验证引号配对：开头的 ''' 中第 1 个 ' 是字面量开始，第 2-3 个 '' 是转义的 '
        // 最后的 ' 是字面量结束。共 4 个 ' = 1 (start) + 2 (escaped) + 1 (end)
        let quote_count = after_eq.matches('\'').count();
        assert_eq!(
            quote_count, 4,
            "expected 4 quotes (1 start + 2 escaped + 1 end), got {}",
            quote_count
        );
    }

    /// 验证 escape_string 对抗 DROP TABLE 注入
    #[test]
    fn test_render_condition_defends_against_drop_table_injection() {
        let mut params = HashMap::new();
        params.insert(
            "name".to_string(),
            Value::String("'; DROP TABLE users; --".to_string()),
        );

        let rendered = render_condition("name = :name", &params).unwrap();
        // 整个 DROP TABLE 应在字符串字面量内部
        assert_eq!(rendered, "name = '''; DROP TABLE users; --'");
        // 不应出现裸露的 DROP TABLE 作为 SQL 语句
        // （应在字符串字面量内部）
        let eq_pos = rendered.find('=').unwrap();
        let after_eq = rendered[eq_pos + 1..].trim();
        assert!(
            after_eq.starts_with('\''),
            "value should start with quote, got: {}",
            after_eq
        );
        assert!(
            after_eq.ends_with('\''),
            "value should end with quote, got: {}",
            after_eq
        );
    }

    #[test]
    fn test_render_condition_partial_unknown_param() {
        let mut params = HashMap::new();
        params.insert("a".to_string(), Value::I64(1));
        // 缺少 b

        let rendered = render_condition("a = :a AND b = :b", &params);
        assert!(rendered.is_none());
    }

    #[test]
    fn test_render_condition_underscore_in_param_name() {
        let mut params = HashMap::new();
        params.insert("user_id".to_string(), Value::I64(100));

        let rendered = render_condition("user_id = :user_id", &params);
        assert_eq!(rendered.unwrap(), "user_id = 100");
    }

    // ===== FilterRegistry 基本操作 =====

    #[test]
    fn test_registry_register_and_count() {
        let r = FilterRegistry::new();
        assert_eq!(r.registered_count(), 0);

        r.register(FilterDef::new("f1").with_condition("1 = 1"));
        assert_eq!(r.registered_count(), 1);

        r.register(FilterDef::new("f2").with_condition("2 = 2"));
        assert_eq!(r.registered_count(), 2);
    }

    #[test]
    fn test_registry_unregister() {
        let r = FilterRegistry::new();
        r.register(FilterDef::new("f1").with_condition("1 = 1"));
        assert_eq!(r.registered_count(), 1);

        let removed = r.unregister("f1");
        assert!(removed);
        assert_eq!(r.registered_count(), 0);

        let removed2 = r.unregister("not_exist");
        assert!(!removed2);
    }

    #[test]
    fn test_registry_registered_names() {
        let r = FilterRegistry::new();
        r.register(FilterDef::new("filter_a").with_condition("1 = 1"));
        r.register(FilterDef::new("filter_b").with_condition("2 = 2"));

        let names = r.registered_names();
        assert!(names.contains(&"filter_a".to_string()));
        assert!(names.contains(&"filter_b".to_string()));
    }

    // ===== enable / disable 测试 =====

    #[test]
    fn test_enable_filter_with_default_param() {
        let r = FilterRegistry::new();
        r.register(
            FilterDef::new("active")
                .with_condition("status = :status")
                .with_param(FilterParam::new("status", "active")),
        );

        let result = r.enable("active", HashMap::new());
        assert!(result.is_ok());
        assert!(r.is_enabled("active"));
        assert_eq!(r.enabled_count(), 1);
    }

    #[test]
    fn test_enable_filter_with_override_param() {
        let r = FilterRegistry::new();
        r.register(
            FilterDef::new("active")
                .with_condition("status = :status")
                .with_param(FilterParam::new("status", "active")),
        );

        let mut params = HashMap::new();
        params.insert("status".to_string(), Value::String("pending".to_string()));
        r.enable("active", params).unwrap();

        let sql = r.apply("SELECT * FROM orders");
        assert!(sql.contains("status = 'pending'"));
    }

    #[test]
    fn test_enable_filter_missing_required_param() {
        let r = FilterRegistry::new();
        r.register(
            FilterDef::new("user_filter")
                .with_condition("user_id = :user_id")
                .with_param(FilterParam::required("user_id")),
        );

        let result = r.enable("user_filter", HashMap::new());
        assert!(matches!(
            result,
            Err(FilterError::MissingParam { filter, param }) if filter == "user_filter" && param == "user_id"
        ));
    }

    #[test]
    fn test_enable_unregistered_filter() {
        let r = FilterRegistry::new();
        let result = r.enable("nonexistent", HashMap::new());
        assert!(matches!(
            result,
            Err(FilterError::NotRegistered(n)) if n == "nonexistent"
        ));
    }

    #[test]
    fn test_enable_already_enabled_filter() {
        let r = FilterRegistry::new();
        r.register(FilterDef::new("f1").with_condition("1 = 1"));
        r.enable("f1", HashMap::new()).unwrap();

        let result = r.enable("f1", HashMap::new());
        assert!(matches!(
            result,
            Err(FilterError::AlreadyEnabled(n)) if n == "f1"
        ));
    }

    #[test]
    fn test_disable_filter() {
        let r = FilterRegistry::new();
        r.register(FilterDef::new("f1").with_condition("1 = 1"));
        r.enable("f1", HashMap::new()).unwrap();
        assert!(r.is_enabled("f1"));

        r.disable("f1").unwrap();
        assert!(!r.is_enabled("f1"));
    }

    #[test]
    fn test_disable_not_enabled_filter() {
        let r = FilterRegistry::new();
        r.register(FilterDef::new("f1").with_condition("1 = 1"));

        let result = r.disable("f1");
        assert!(matches!(
            result,
            Err(FilterError::NotEnabled(n)) if n == "f1"
        ));
    }

    #[test]
    fn test_clear_enabled() {
        let r = FilterRegistry::new();
        r.register(FilterDef::new("f1").with_condition("1 = 1"));
        r.register(FilterDef::new("f2").with_condition("2 = 2"));
        r.enable("f1", HashMap::new()).unwrap();
        r.enable("f2", HashMap::new()).unwrap();
        assert_eq!(r.enabled_count(), 2);

        r.clear_enabled();
        assert_eq!(r.enabled_count(), 0);
        // 已注册定义保留
        assert_eq!(r.registered_count(), 2);
    }

    // ===== apply 测试 =====

    #[test]
    fn test_apply_no_filters() {
        let r = FilterRegistry::new();
        let sql = r.apply("SELECT * FROM users");
        assert_eq!(sql, "SELECT * FROM users");
    }

    #[test]
    fn test_apply_single_filter_no_existing_where() {
        let r = FilterRegistry::new();
        r.register(
            FilterDef::new("active")
                .with_condition("status = :status")
                .with_param(FilterParam::new("status", "active")),
        );
        r.enable("active", HashMap::new()).unwrap();

        let sql = r.apply("SELECT * FROM users");
        assert!(sql.contains("WHERE"));
        assert!(sql.contains("status = 'active'"));
    }

    #[test]
    fn test_apply_multiple_filters() {
        let r = FilterRegistry::new();
        r.register(
            FilterDef::new("active")
                .with_condition("status = :status")
                .with_param(FilterParam::new("status", "active")),
        );
        r.register(
            FilterDef::new("adult")
                .with_condition("age >= :min_age")
                .with_param(FilterParam::new("min_age", 18)),
        );
        r.enable("active", HashMap::new()).unwrap();
        r.enable("adult", HashMap::new()).unwrap();

        let sql = r.apply("SELECT * FROM users");
        assert!(sql.contains("status = 'active'"));
        assert!(sql.contains("age >= 18"));
        assert!(sql.contains("AND"));
    }

    #[test]
    fn test_apply_appends_to_existing_where() {
        let r = FilterRegistry::new();
        r.register(
            FilterDef::new("active")
                .with_condition("status = :status")
                .with_param(FilterParam::new("status", "active")),
        );
        r.enable("active", HashMap::new()).unwrap();

        let sql = r.apply("SELECT * FROM users WHERE id = 1");
        assert!(sql.contains("id = 1"));
        assert!(sql.contains("status = 'active'"));
        assert!(sql.contains("AND"));
    }

    #[test]
    fn test_apply_inserts_before_group_by() {
        let r = FilterRegistry::new();
        r.register(
            FilterDef::new("active")
                .with_condition("status = :status")
                .with_param(FilterParam::new("status", "active")),
        );
        r.enable("active", HashMap::new()).unwrap();

        let sql = r.apply("SELECT dept, COUNT(*) FROM users GROUP BY dept");
        let where_idx = sql.to_uppercase().find("WHERE").unwrap();
        let group_by_idx = sql.to_uppercase().find("GROUP BY").unwrap();
        assert!(where_idx < group_by_idx);
    }

    #[test]
    fn test_apply_inserts_before_order_by() {
        let r = FilterRegistry::new();
        r.register(
            FilterDef::new("active")
                .with_condition("status = :status")
                .with_param(FilterParam::new("status", "active")),
        );
        r.enable("active", HashMap::new()).unwrap();

        let sql = r.apply("SELECT * FROM users ORDER BY id");
        let where_idx = sql.to_uppercase().find("WHERE").unwrap();
        let order_by_idx = sql.to_uppercase().find("ORDER BY").unwrap();
        assert!(where_idx < order_by_idx);
    }

    #[test]
    fn test_apply_inserts_before_limit() {
        let r = FilterRegistry::new();
        r.register(
            FilterDef::new("active")
                .with_condition("status = :status")
                .with_param(FilterParam::new("status", "active")),
        );
        r.enable("active", HashMap::new()).unwrap();

        let sql = r.apply("SELECT * FROM users LIMIT 10");
        let where_idx = sql.to_uppercase().find("WHERE").unwrap();
        let limit_idx = sql.to_uppercase().find("LIMIT").unwrap();
        assert!(where_idx < limit_idx);
    }

    #[test]
    fn test_apply_with_typed_params() {
        let r = FilterRegistry::new();
        r.register(
            FilterDef::new("range")
                .with_condition("age >= :min AND age <= :max")
                .with_param(FilterParam::new("min", 18))
                .with_param(FilterParam::new("max", 65)),
        );
        r.enable("range", HashMap::new()).unwrap();

        let sql = r.apply("SELECT * FROM users");
        assert!(sql.contains("age >= 18"));
        assert!(sql.contains("age <= 65"));
    }

    #[test]
    fn test_apply_disabled_filter_not_applied() {
        let r = FilterRegistry::new();
        r.register(
            FilterDef::new("active")
                .with_condition("status = :status")
                .with_param(FilterParam::new("status", "active")),
        );
        r.enable("active", HashMap::new()).unwrap();
        r.disable("active").unwrap();

        let sql = r.apply("SELECT * FROM users");
        assert_eq!(sql, "SELECT * FROM users");
    }

    // ===== FilterError Display 测试 =====

    #[test]
    fn test_filter_error_display_not_registered() {
        let e = FilterError::NotRegistered("foo".to_string());
        let s = format!("{}", e);
        assert!(s.contains("foo"));
        assert!(s.contains("not registered"));
    }

    #[test]
    fn test_filter_error_display_missing_param() {
        let e = FilterError::MissingParam {
            filter: "f".to_string(),
            param: "p".to_string(),
        };
        let s = format!("{}", e);
        assert!(s.contains("f"));
        assert!(s.contains("p"));
        assert!(s.contains("missing"));
    }

    #[test]
    fn test_filter_error_display_template_error() {
        let e = FilterError::TemplateError("syntax".to_string());
        let s = format!("{}", e);
        assert!(s.contains("syntax"));
    }

    // ===== 默认 Default 测试 =====

    #[test]
    fn test_registry_default_is_empty() {
        let r = FilterRegistry::default();
        assert_eq!(r.registered_count(), 0);
        assert_eq!(r.enabled_count(), 0);
    }

    // ===== 综合场景测试 =====

    #[test]
    fn test_complex_scenario_enable_disable_enable() {
        let r = FilterRegistry::new();
        r.register(
            FilterDef::new("active")
                .with_condition("status = :status")
                .with_param(FilterParam::new("status", "active")),
        );

        // 启用
        r.enable("active", HashMap::new()).unwrap();
        let sql1 = r.apply("SELECT * FROM users");
        assert!(sql1.contains("status = 'active'"));

        // 禁用
        r.disable("active").unwrap();
        let sql2 = r.apply("SELECT * FROM users");
        assert_eq!(sql2, "SELECT * FROM users");

        // 重新启用，使用不同参数
        let mut params = HashMap::new();
        params.insert("status".to_string(), Value::String("pending".to_string()));
        r.enable("active", params).unwrap();
        let sql3 = r.apply("SELECT * FROM users");
        assert!(sql3.contains("status = 'pending'"));
    }
}
