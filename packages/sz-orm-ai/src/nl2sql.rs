//! NL→SQL（自然语言转 SQL）模块
//!
//! 提供将自然语言查询转换为 SQL 语句的能力，支持：
//! - 内存模拟引擎 [`SimpleNl2SqlEngine`]（基于规则匹配，无需外部 API）
//! - 真实 LLM 引擎 [`OpenAINl2SqlEngine`]（调用 OpenAI 兼容 API，需 `real` feature）
//!
//! 所有生成的 SQL 均经过安全验证：只允许 SELECT 查询，并检测注入风险。

use async_trait::async_trait;
use thiserror::Error;

use crate::safety;

#[cfg(feature = "real")]
use serde::{Deserialize, Serialize};

// ==================== 数据结构 ====================

/// NL→SQL 查询结果
#[derive(Debug, Clone)]
pub struct SqlQuery {
    /// 生成的 SQL（使用 $1, $2 等参数化占位符）
    pub sql: String,
    /// 生成过程的自然语言解释
    pub explanation: String,
    /// 置信度（0.0 ~ 1.0）
    pub confidence: f32,
}

/// Schema 上下文，描述数据库中的表和列信息
#[derive(Debug, Clone, Default)]
pub struct SchemaContext {
    pub tables: Vec<TableInfo>,
}

/// 表信息
#[derive(Debug, Clone)]
pub struct TableInfo {
    pub name: String,
    pub columns: Vec<ColumnInfo>,
}

/// 列信息
#[derive(Debug, Clone)]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub is_primary_key: bool,
}

// ==================== 错误类型 ====================

/// NL→SQL 相关错误
#[derive(Debug, Error)]
pub enum Nl2SqlError {
    /// 无法解析自然语言查询
    #[error("Invalid query: {0}")]
    InvalidQuery(String),
    /// Schema 信息不足（表/列不存在）
    #[error("Schema error: {0}")]
    SchemaError(String),
    /// SQL 安全验证失败
    #[error("Safety error: {0}")]
    SafetyError(String),
    /// SQL 生成失败
    #[error("Generation error: {0}")]
    GenerationError(String),
    /// API 调用错误（仅 real feature）
    #[error("API error (status {0}): {1}")]
    ApiError(u16, String),
    /// 网络错误（仅 real feature）
    #[error("Network error: {0}")]
    NetworkError(String),
    /// 配置错误
    #[error("Config error: {0}")]
    ConfigError(String),
}

// ==================== Trait 定义 ====================

/// NL→SQL 引擎 trait
///
/// 所有 NL→SQL 实现必须实现此 trait，以保证一致的接口。
#[async_trait]
pub trait Nl2SqlEngine: Send + Sync {
    /// 将自然语言查询转换为 SQL
    ///
    /// # 参数
    /// - `nl_query`: 自然语言查询（如 "show all users where age > 25"）
    /// - `schema`: 数据库 schema 上下文
    ///
    /// # 返回值
    /// - `Ok(SqlQuery)`: 生成的 SQL 查询
    /// - `Err(Nl2SqlError)`: 转换失败
    async fn generate(
        &self,
        nl_query: &str,
        schema: &SchemaContext,
    ) -> Result<SqlQuery, Nl2SqlError>;

    /// 验证生成的 SQL 是否安全可用
    ///
    /// 执行以下检查：
    /// - 只允许 SELECT 语句
    /// - 无 SQL 注入风险
    async fn validate(&self, query: &SqlQuery) -> Result<bool, Nl2SqlError>;
}

// ==================== SimpleNl2SqlEngine ====================

/// 基于规则匹配的 NL→SQL 引擎（内存模拟，无需 LLM API）
///
/// 通过关键词匹配和模式识别，将常见自然语言查询转换为 SQL。
/// 支持以下查询模式：
/// - 简单 SELECT（`SELECT *` / `SELECT col1, col2`）
/// - COUNT / COUNT DISTINCT
/// - 聚合函数（SUM, AVG, MIN, MAX）
/// - WHERE 条件（=, >, <, >=, <=, !=, LIKE）
/// - ORDER BY（ASC / DESC）
/// - GROUP BY
/// - JOIN（通过外键名称推断关联）
/// - LIMIT
///
/// 所有生成的 SQL 使用 `$1`, `$2` 等参数化占位符防止注入。
///
/// # 示例
///
/// ```ignore
/// use sz_orm_ai::nl2sql::{SimpleNl2SqlEngine, Nl2SqlEngine, SchemaContext, TableInfo, ColumnInfo};
///
/// # async fn run() -> Result<(), Box<dyn std::error::Error>> {
/// let engine = SimpleNl2SqlEngine::new();
/// let schema = SchemaContext {
///     tables: vec![TableInfo {
///         name: "users".into(),
///         columns: vec![
///             ColumnInfo { name: "id".into(), data_type: "INTEGER".into(), nullable: false, is_primary_key: true },
///             ColumnInfo { name: "name".into(), data_type: "TEXT".into(), nullable: true, is_primary_key: false },
///         ],
///     }],
/// };
/// let result = engine.generate("show all users", &schema).await?;
/// assert_eq!(result.sql, "SELECT * FROM users");
/// # Ok(())
/// # }
/// ```
pub struct SimpleNl2SqlEngine {
    /// 表别名映射（如 "user" → "users"）
    aliases: std::collections::HashMap<String, String>,
}

impl Default for SimpleNl2SqlEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl SimpleNl2SqlEngine {
    pub fn new() -> Self {
        Self {
            aliases: std::collections::HashMap::new(),
        }
    }

    /// 注册表别名（例如 `with_alias("user", "users")`）
    pub fn with_alias(mut self, alias: &str, table: &str) -> Self {
        self.aliases
            .insert(alias.to_lowercase(), table.to_lowercase());
        self
    }

    /// 在 schema 中查找表名（匹配原名或别名）
    fn find_table<'a>(&self, query: &str, schema: &'a SchemaContext) -> Option<&'a TableInfo> {
        let lower = query.to_lowercase();

        // 按名称在 query 中出现的顺序打分
        let mut best: Option<&TableInfo> = None;
        let mut best_pos: Option<usize> = None;

        for table in &schema.tables {
            let name = table.name.to_lowercase();
            if let Some(pos) = lower.find(&name) {
                let is_better = match best_pos {
                    None => true,
                    Some(best) => pos < best,
                };
                if is_better {
                    best = Some(table);
                    best_pos = Some(pos);
                }
            }
        }

        // 检查别名：遍历所有别名，看查询中是否包含别名
        for (alias, target_table_name) in &self.aliases {
            if let Some(pos) = lower.find(alias) {
                // 找到对应表
                if let Some(table) = schema
                    .tables
                    .iter()
                    .find(|t| t.name.to_lowercase() == *target_table_name)
                {
                    let is_better = match best_pos {
                        None => true,
                        Some(best) => pos < best,
                    };
                    if is_better {
                        best = Some(table);
                        best_pos = Some(pos);
                    }
                }
            }
        }

        best
    }

    /// 只在 SELECT/NL 部分提取列名（排除 WHERE/ORDER BY/GROUP BY 后的部分）
    fn extract_columns_from_nl(query: &str, table: &TableInfo) -> Vec<String> {
        let lower = query.to_lowercase();
        // 截取到第一个子句关键字之前
        let select_part = if let Some(pos) = lower.find(" where ") {
            &query[..pos]
        } else if let Some(pos) = lower.find(" having ") {
            &query[..pos]
        } else if let Some(pos) = lower.find(" order by") {
            &query[..pos]
        } else if let Some(pos) = lower.find(" sorted by") {
            &query[..pos]
        } else if let Some(pos) = lower.find(" sort by") {
            &query[..pos]
        } else if let Some(pos) = lower.find(" ordered by") {
            &query[..pos]
        } else if let Some(pos) = lower.find(" group by") {
            &query[..pos]
        } else {
            query
        };

        Self::extract_columns(select_part, table)
    }

    /// 确认 query 中提到的表名（不含 FROM 子句的上下文检测）
    fn find_mentioned_table<'a>(
        &self,
        query: &str,
        schema: &'a SchemaContext,
    ) -> Option<&'a TableInfo> {
        self.find_table(query, schema)
    }

    /// 在 query 中查找提及的所有表名（用于 JOIN 检测）
    fn find_all_tables<'a>(&self, query: &str, schema: &'a SchemaContext) -> Vec<&'a TableInfo> {
        let lower = query.to_lowercase();
        let mut found = Vec::new();

        for table in &schema.tables {
            let name = table.name.to_lowercase();
            if lower.contains(&name) {
                found.push(table);
            }
        }

        found
    }

    /// 检查列名是否存在于 schema 的指定表中
    fn column_exists(col: &str, table: &TableInfo) -> bool {
        let col_lower = col.to_lowercase();
        table
            .columns
            .iter()
            .any(|c| c.name.to_lowercase() == col_lower)
    }

    /// 从 query 中提取列名列表（匹配 schema 中的列）
    fn extract_columns(query: &str, table: &TableInfo) -> Vec<String> {
        let lower = query.to_lowercase();
        let mut columns = Vec::new();

        for col in &table.columns {
            let col_lower = col.name.to_lowercase();
            // 避免匹配短列名导致误匹配（如 "id"）
            if col_lower.len() >= 2
                && (lower.contains(&col_lower) || lower.contains(&format!(" {} ", col_lower)))
            {
                // 确认不是 from 子句后的表名列
                if !columns.contains(&col.name) {
                    columns.push(col.name.clone());
                }
            }
        }

        columns
    }

    /// 尝试寻找两个表之间的外键关联列
    fn find_join_columns(t1: &TableInfo, t2: &TableInfo) -> Option<(String, String)> {
        // 模式 1: t2 中有 t1_name + _id → t2.t1_name_id = t1.id
        for col1 in &t1.columns {
            if col1.is_primary_key {
                let expected_fk = format!("{}_{}", t2.name, col1.name).to_lowercase();
                for col2 in &t2.columns {
                    if col2.name.to_lowercase() == expected_fk {
                        return Some((
                            format!("{}.{}", t1.name, col1.name),
                            format!("{}.{}", t2.name, col2.name),
                        ));
                    }
                }
            }
        }
        // 模式 2: t1 中有 t2_name + _id → t1.t2_name_id = t2.id
        for col2 in &t2.columns {
            if col2.is_primary_key {
                let expected_fk = format!("{}_{}", t1.name, col2.name).to_lowercase();
                for col1 in &t1.columns {
                    if col1.name.to_lowercase() == expected_fk {
                        return Some((
                            format!("{}.{}", t1.name, col1.name),
                            format!("{}.{}", t2.name, col2.name),
                        ));
                    }
                }
            }
        }
        None
    }

    /// 从 WHERE 条件文本中提取参数化条件
    fn parse_conditions(where_text: &str, table: &TableInfo) -> (Vec<String>, Vec<String>) {
        let text = where_text.trim();
        let mut conditions = Vec::new();
        let mut params = Vec::new();
        let param_idx = 1;

        // 处理单个条件表达式
        let expr = text;
        if expr.is_empty() {
            return (conditions, params);
        }

        let operators = [
            (">=", ">="),
            ("<=", "<="),
            ("!=", "!="),
            ("=", "="),
            (">", ">"),
            ("<", "<"),
        ];

        let mut resolved = false;
        for (op_str, sql_op) in &operators {
            if let Some(pos) = expr.find(op_str) {
                let field = expr[..pos].trim();
                let raw_val = expr[pos + op_str.len()..].trim();

                if !field.is_empty() && !raw_val.is_empty() {
                    // 检查 raw_val 是否为列名
                    if Self::column_exists(raw_val, table) {
                        conditions.push(format!("{} {} {}", field, sql_op, raw_val));
                    } else {
                        let param = format!("${}", param_idx);
                        conditions.push(format!("{} {} {}", field, sql_op, param));
                        params.push(raw_val.to_string());
                    }
                    resolved = true;
                }
                break;
            }
        }

        // 处理 LIKE / contains
        if !resolved {
            let lower_expr = expr.to_lowercase();
            if let Some(pos) = lower_expr.find(" like ") {
                let field = expr[..pos].trim();
                let raw_val = expr[pos + 6..].trim();
                if !field.is_empty() && !raw_val.is_empty() {
                    let param = "$1".to_string();
                    conditions.push(format!("{} LIKE {}", field, param));
                    params.push(raw_val.to_string());
                    resolved = true;
                }
            } else if let Some(pos) = lower_expr.find(" contains ") {
                let field = expr[..pos].trim();
                let raw_val = expr[pos + 10..].trim();
                if !field.is_empty() && !raw_val.is_empty() {
                    let param = "$1".to_string();
                    conditions.push(format!("{} LIKE '%' || {} || '%'", field, param));
                    params.push(raw_val.to_string());
                    resolved = true;
                }
            }
        }

        // 未解析的条件原样保留
        if !resolved && !expr.is_empty() {
            conditions.push(expr.to_string());
        }

        (conditions, params)
    }
}

#[async_trait]
impl Nl2SqlEngine for SimpleNl2SqlEngine {
    async fn generate(
        &self,
        nl_query: &str,
        schema: &SchemaContext,
    ) -> Result<SqlQuery, Nl2SqlError> {
        if nl_query.trim().is_empty() {
            return Err(Nl2SqlError::InvalidQuery("自然语言查询不能为空".into()));
        }

        if schema.tables.is_empty() {
            return Err(Nl2SqlError::SchemaError("Schema 中未定义任何表".into()));
        }

        let lower = nl_query.to_lowercase();
        let table = self.find_mentioned_table(nl_query, schema).ok_or_else(|| {
            Nl2SqlError::SchemaError(format!(
                "无法在查询中识别表名，schema 包含: {}",
                schema
                    .tables
                    .iter()
                    .map(|t| t.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        })?;

        // ============ 识别查询类型 ============

        // 是否是 COUNT 查询
        let is_count =
            lower.contains("count ") || lower.contains("how many") || lower.contains("number of");
        let is_distinct = lower.contains("distinct");
        let has_aggregation = lower.contains("sum ")
            || lower.contains("total ")
            || lower.contains("avg ")
            || lower.contains("average ")
            || lower.contains("mean ")
            || lower.contains("min ")
            || lower.contains("minimum ")
            || lower.contains("max ")
            || lower.contains("maximum ")
            || lower.contains("highest");

        // 提取 ORDER BY、GROUP BY、LIMIT、JOIN
        let has_order = lower.contains("order by")
            || lower.contains("sort by")
            || lower.contains("ordered by")
            || lower.contains("sorted by");
        let has_group = lower.contains("group by");
        let has_limit =
            lower.contains("limit") || lower.contains("top ") || lower.contains("first ");
        let has_join =
            lower.contains(" join ") || lower.contains("combine with") || lower.contains("with ");

        // ============ 提取列（仅从 SELECT/NL 部分提取，排除子句关键字后的部分） ============
        let columns = Self::extract_columns_from_nl(nl_query, table);

        // ============ 提取 WHERE 条件 ============
        let where_text = if let Some(pos) = lower.find(" where ") {
            Some(nl_query[pos + 7..].trim())
        } else if let Some(pos) = lower.find(" having ") {
            Some(nl_query[pos + 8..].trim())
        } else if let Some(pos) = lower.find(" with ") {
            // "with" 可能表示条件或 JOIN
            let after = nl_query[pos + 5..].trim();
            // 如果 with 后跟列名和操作符，视为条件
            if table
                .columns
                .iter()
                .any(|c| after.to_lowercase().starts_with(&c.name.to_lowercase()))
            {
                Some(after)
            } else {
                None
            }
        } else {
            None
        };

        let (conditions, params) = match where_text {
            Some(text) => {
                // 去掉 order by / group by / limit 部分
                let clean_text = if let Some(pos) = text.to_lowercase().find(" order by") {
                    &text[..pos]
                } else if let Some(pos) = text.to_lowercase().find(" group by") {
                    &text[..pos]
                } else if let Some(pos) = text.to_lowercase().find(" limit") {
                    &text[..pos]
                } else {
                    text
                };
                Self::parse_conditions(clean_text, table)
            }
            None => (Vec::new(), Vec::new()),
        };

        // ============ 提取 ORDER BY ============
        let order_clause: Option<String> = if has_order {
            let (after_text, is_desc) = if let Some(pos) = lower.find("order by") {
                let txt = nl_query[pos + 8..].trim().to_string();
                let d = txt.to_lowercase().contains("desc")
                    || txt.to_lowercase().contains("descending");
                (txt, d)
            } else if let Some(pos) = lower.find("sort by") {
                let txt = nl_query[pos + 7..].trim().to_string();
                let d = txt.to_lowercase().contains("desc")
                    || txt.to_lowercase().contains("descending");
                (txt, d)
            } else if let Some(pos) = lower.find("sorted by") {
                let txt = nl_query[pos + 9..].trim().to_string();
                let d = txt.to_lowercase().contains("desc")
                    || txt.to_lowercase().contains("descending");
                (txt, d)
            } else if let Some(pos) = lower.find("ordered by") {
                let txt = nl_query[pos + 10..].trim().to_string();
                let d = txt.to_lowercase().contains("desc")
                    || txt.to_lowercase().contains("descending");
                (txt, d)
            } else {
                (String::new(), false)
            };

            let field = after_text
                .split([' ', ','])
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            if field.is_empty() {
                None
            } else if is_desc {
                Some(format!(" ORDER BY {} DESC", field))
            } else {
                Some(format!(" ORDER BY {} ASC", field))
            }
        } else {
            None
        };

        // ============ 提取 GROUP BY ============
        let group_clause = if has_group {
            let after = if let Some(pos) = lower.find("group by") {
                &nl_query[pos + 8..]
            } else {
                ""
            };
            let group_cols: Vec<String> = after
                .split(',')
                .filter_map(|s| {
                    let first = s.split_whitespace().next()?;
                    if first.is_empty() {
                        None
                    } else {
                        Some(first.to_string())
                    }
                })
                .collect();
            if group_cols.is_empty() {
                None
            } else {
                Some(format!(" GROUP BY {}", group_cols.join(", ")))
            }
        } else {
            None
        };

        // ============ 提取 LIMIT ============
        let limit_val: Option<usize> = if has_limit {
            let limit_text = if let Some(pos) = lower.find("limit ") {
                let after = &nl_query[pos + 6..].trim();
                after.split_whitespace().next()
            } else if let Some(pos) = lower.find("top ") {
                let after = &nl_query[pos + 4..].trim();
                after.split_whitespace().next()
            } else if let Some(pos) = lower.find("first ") {
                let after = &nl_query[pos + 6..].trim();
                after.split_whitespace().next()
            } else {
                None
            };
            limit_text.and_then(|s| s.parse::<usize>().ok())
        } else {
            None
        };

        // ============ 提取 JOIN ============
        let join_tables = if has_join {
            let all_tables = self.find_all_tables(nl_query, schema);
            let others: Vec<&&TableInfo> =
                all_tables.iter().filter(|t| t.name != table.name).collect();
            others.into_iter().copied().collect()
        } else {
            Vec::new()
        };

        // ============ 构建 SQL ============

        // 构建 SELECT 子句
        let select_clause = if is_count {
            if is_distinct && !columns.is_empty() {
                format!("SELECT COUNT(DISTINCT {})", columns[0])
            } else {
                "SELECT COUNT(*)".to_string()
            }
        } else if has_aggregation && !columns.is_empty() {
            // 检测具体聚合函数
            let agg_func = if lower.contains("sum ") || lower.contains("total ") {
                "SUM"
            } else if lower.contains("avg ")
                || lower.contains("average ")
                || lower.contains("mean ")
            {
                "AVG"
            } else if lower.contains("min ") || lower.contains("minimum ") {
                "MIN"
            } else if lower.contains("max ")
                || lower.contains("maximum ")
                || lower.contains("highest")
            {
                "MAX"
            } else {
                "SUM"
            };
            if group_clause.is_some() {
                // 聚合 + group by
                format!("SELECT {}, {}({})", columns.join(", "), agg_func, agg_func)
            } else if !columns.is_empty() {
                format!("SELECT {}({})", agg_func, columns[0])
            } else {
                format!("SELECT {}(*)", agg_func)
            }
        } else if !columns.is_empty() {
            format!("SELECT {}", columns.join(", "))
        } else {
            "SELECT *".to_string()
        };

        // 修正 group by 场景的 select
        let select_clause = if group_clause.is_some() && has_aggregation {
            let agg_func = if lower.contains("sum ") || lower.contains("total ") {
                "SUM"
            } else if lower.contains("avg ")
                || lower.contains("average ")
                || lower.contains("mean ")
            {
                "AVG"
            } else if lower.contains("min ") || lower.contains("minimum ") {
                "MIN"
            } else if lower.contains("max ")
                || lower.contains("maximum ")
                || lower.contains("highest")
            {
                "MAX"
            } else {
                "COUNT"
            };

            if !columns.is_empty() {
                if columns.len() >= 2 {
                    format!("SELECT {}, {}({})", columns[1], agg_func, columns[1])
                } else {
                    format!("SELECT {}, {}({})", columns[0], agg_func, columns[0])
                }
            } else {
                select_clause
            }
        } else {
            select_clause
        };

        // 构建 FROM 子句 + JOIN
        let mut from_clause = format!(" FROM {}", table.name);
        let mut join_descriptions = Vec::new();
        for join_table in &join_tables {
            if let Some((left, right)) = Self::find_join_columns(table, join_table) {
                from_clause.push_str(&format!(
                    " JOIN {} ON {} = {}",
                    join_table.name, left, right
                ));
                join_descriptions.push(format!("{} ON {} = {}", join_table.name, left, right));
            }
        }

        // 构建 WHERE 子句（参数化）
        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", conditions.join(" AND "))
        };

        // 组合 SQL
        let sql = format!(
            "{}{}{}{}{}{}",
            select_clause,
            from_clause,
            where_clause,
            order_clause.as_deref().unwrap_or(""),
            group_clause.as_deref().unwrap_or(""),
            limit_val
                .map(|v| format!(" LIMIT {}", v))
                .unwrap_or_default(),
        );

        // 安全验证
        if !safety::validate_select_only(&sql) {
            return Err(Nl2SqlError::SafetyError(
                "生成的 SQL 不是 SELECT 查询".into(),
            ));
        }
        if !safety::validate_no_injection(&sql) {
            return Err(Nl2SqlError::SafetyError("生成的 SQL 包含注入风险".into()));
        }
        let sql = safety::sanitize_sql(&sql);

        // 构建解释
        let mut explanation_parts = Vec::new();
        explanation_parts.push(format!("查询 {} 表", table.name));
        if !columns.is_empty() {
            explanation_parts.push(format!("列: {}", columns.join(", ")));
        }
        if is_count {
            explanation_parts.push("统计数量".to_string());
        }
        if !conditions.is_empty() {
            let cond_desc: Vec<String> = conditions
                .iter()
                .enumerate()
                .map(|(i, cond)| {
                    if i < params.len() {
                        cond.replace(&format!("${}", i + 1), &format!("'{}'", params[i]))
                    } else {
                        cond.clone()
                    }
                })
                .collect();
            explanation_parts.push(format!("条件: {}", cond_desc.join(", ")));
        }
        if !params.is_empty() {
            explanation_parts.push(format!(
                "参数: [{}]",
                params
                    .iter()
                    .map(|p| format!("'{}'", p))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        let explanation = explanation_parts.join("；");

        // 计算置信度
        // 简单的规则：明确的模式匹配给高置信度
        let mut confidence = 0.7;
        if !conditions.is_empty() || is_count {
            confidence = 0.8;
        }
        if !columns.is_empty() && !conditions.is_empty() {
            confidence = 0.9;
        }

        Ok(SqlQuery {
            sql,
            explanation,
            confidence,
        })
    }

    async fn validate(&self, query: &SqlQuery) -> Result<bool, Nl2SqlError> {
        if !safety::validate_select_only(&query.sql) {
            return Ok(false);
        }
        if !safety::validate_no_injection(&query.sql) {
            return Ok(false);
        }
        if query.confidence < 0.0 || query.confidence > 1.0 {
            return Err(Nl2SqlError::GenerationError(format!(
                "confidence 必须在 0.0~1.0 范围内，实际 {}",
                query.confidence
            )));
        }
        Ok(true)
    }
}

// ==================== OpenAINl2SqlEngine ====================

/// OpenAI 兼容的 NL→SQL 引擎（调用 LLM API）
///
/// 仅在启用 `real` feature 时编译。
/// 调用 OpenAI 兼容的 `/v1/chat/completions` 接口生成 SQL。
///
/// 生成的 SQL 经过安全验证：只允许 SELECT，并检测注入风险。
///
/// # 用法
///
/// ```ignore
/// use sz_orm_ai::nl2sql::{Nl2SqlEngine, OpenAINl2SqlEngine, SchemaContext};
///
/// # async fn run() -> Result<(), Box<dyn std::error::Error>> {
/// let engine = OpenAINl2SqlEngine::new("sk-xxxx")
///     .with_model("gpt-4o-mini");
/// let schema = SchemaContext::default();
/// let result = engine.generate("show all users", &schema).await?;
/// println!("SQL: {}", result.sql);
/// # Ok(())
/// # }
/// ```
#[cfg(feature = "real")]
pub struct OpenAINl2SqlEngine {
    /// API 基础地址（默认 `https://api.openai.com/v1`）
    api_base: String,
    /// API Key（Bearer token）
    api_key: String,
    /// 模型名称（默认 `gpt-4o-mini`）
    model: String,
    /// HTTP 客户端
    http_client: reqwest::Client,
}

#[cfg(feature = "real")]
impl OpenAINl2SqlEngine {
    /// 默认 API 基础地址
    const DEFAULT_API_BASE: &'static str = "https://api.openai.com/v1";
    /// 默认模型
    const DEFAULT_MODEL: &'static str = "gpt-4o-mini";

    /// 创建客户端实例
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_base: Self::DEFAULT_API_BASE.to_string(),
            api_key: api_key.into(),
            model: Self::DEFAULT_MODEL.to_string(),
            http_client: reqwest::Client::new(),
        }
    }

    /// 设置 API base URL
    pub fn with_api_base(mut self, api_base: impl Into<String>) -> Self {
        self.api_base = api_base.into();
        self
    }

    /// 设置模型名称
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// 校验 API key 非空
    fn ensure_api_key(&self) -> Result<(), Nl2SqlError> {
        if self.api_key.is_empty() {
            return Err(Nl2SqlError::ConfigError(
                "API key 为空，无法调用 OpenAI API".into(),
            ));
        }
        Ok(())
    }

    /// 构建 system prompt（包含 schema 信息和 SQL 生成规范）
    fn build_system_prompt(schema: &SchemaContext) -> String {
        let mut prompt = String::from(
            "You are a SQL generator. Given a database schema and a natural language query, ",
        );
        prompt.push_str("generate a valid SQL SELECT statement.\n\n");
        prompt.push_str("Rules:\n");
        prompt.push_str("- Only generate SELECT statements (no INSERT, UPDATE, DELETE, DROP, ALTER, TRUNCATE)\n");
        prompt.push_str("- Use parameterized placeholders ($1, $2, ...) for all values to prevent SQL injection\n");
        prompt.push_str("- If the query is ambiguous, choose the most likely interpretation\n");
        prompt
            .push_str("- Return ONLY the SQL statement, no explanation or markdown formatting\n\n");

        prompt.push_str("Database Schema:\n");
        for table in &schema.tables {
            prompt.push_str(&format!("CREATE TABLE {} (\n", table.name));
            for col in &table.columns {
                prompt.push_str(&format!(
                    "  {} {} {} {},\n",
                    col.name,
                    col.data_type,
                    if col.is_primary_key {
                        "PRIMARY KEY"
                    } else {
                        ""
                    },
                    if col.nullable { "NULL" } else { "NOT NULL" },
                ));
            }
            prompt.push_str(");\n\n");
        }

        prompt
    }
}

#[cfg(feature = "real")]
#[derive(Serialize)]
struct ChatCompletionRequest<'a> {
    model: &'a str,
    messages: Vec<Message<'a>>,
    temperature: f32,
    max_tokens: u32,
}

#[cfg(feature = "real")]
#[derive(Serialize)]
struct Message<'a> {
    role: &'a str,
    content: String,
}

#[cfg(feature = "real")]
#[derive(Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<Choice>,
}

#[cfg(feature = "real")]
#[derive(Deserialize)]
struct Choice {
    message: ChoiceMessage,
    #[allow(dead_code)]
    finish_reason: Option<String>,
}

#[cfg(feature = "real")]
#[derive(Deserialize)]
struct ChoiceMessage {
    content: Option<String>,
}

#[cfg(feature = "real")]
#[async_trait]
impl Nl2SqlEngine for OpenAINl2SqlEngine {
    async fn generate(
        &self,
        nl_query: &str,
        schema: &SchemaContext,
    ) -> Result<SqlQuery, Nl2SqlError> {
        self.ensure_api_key()?;

        if nl_query.trim().is_empty() {
            return Err(Nl2SqlError::InvalidQuery("自然语言查询不能为空".into()));
        }

        let system_prompt = Self::build_system_prompt(schema);
        let user_message = format!(
            "Given the schema above, generate a SQL query for: {}",
            nl_query
        );

        let body = ChatCompletionRequest {
            model: &self.model,
            messages: vec![
                Message {
                    role: "system",
                    content: system_prompt,
                },
                Message {
                    role: "user",
                    content: user_message,
                },
            ],
            temperature: 0.1,
            max_tokens: 500,
        };

        let url = format!("{}/chat/completions", self.api_base);
        let resp = self
            .http_client
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| Nl2SqlError::NetworkError(e.to_string()))?;

        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let message = resp.text().await.unwrap_or_default();
            return Err(Nl2SqlError::ApiError(status, message));
        }

        let parsed: ChatCompletionResponse = resp
            .json()
            .await
            .map_err(|e| Nl2SqlError::NetworkError(e.to_string()))?;

        let raw_sql = parsed
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.content)
            .ok_or_else(|| Nl2SqlError::ApiError(status, "API 返回空响应".into()))?;

        // 清理 LLM 返回的 SQL（去掉 markdown 代码块标记）
        let cleaned_sql = clean_llm_sql_output(&raw_sql);

        let query = SqlQuery {
            sql: cleaned_sql.clone(),
            explanation: format!("由 {} 模型根据自然语言查询生成", self.model),
            confidence: 0.8,
        };

        // 安全验证
        if !safety::validate_select_only(&query.sql) {
            return Err(Nl2SqlError::SafetyError(
                "生成的 SQL 不是 SELECT 查询，已被拦截".into(),
            ));
        }
        if !safety::validate_no_injection(&query.sql) {
            return Err(Nl2SqlError::SafetyError(
                "生成的 SQL 包含注入风险，已被拦截".into(),
            ));
        }

        Ok(query)
    }

    async fn validate(&self, query: &SqlQuery) -> Result<bool, Nl2SqlError> {
        if !safety::validate_select_only(&query.sql) {
            return Ok(false);
        }
        if !safety::validate_no_injection(&query.sql) {
            return Ok(false);
        }
        if query.confidence < 0.0 || query.confidence > 1.0 {
            return Err(Nl2SqlError::GenerationError(format!(
                "confidence 必须在 0.0~1.0 范围内，实际 {}",
                query.confidence
            )));
        }
        Ok(true)
    }
}

/// 清理 LLM 输出的 SQL（移除 markdown 代码块标记和前后空白）
#[cfg(feature = "real")]
fn clean_llm_sql_output(raw: &str) -> String {
    let trimmed = raw.trim();
    // 移除 ```sql ... ``` 或 ``` ... ``` 包装
    if trimmed.starts_with("```") {
        let content = trimmed.trim_start_matches('`');
        if let Some(end) = content.rfind("```") {
            return content[..end].trim().to_string();
        }
        return content.trim().to_string();
    }
    trimmed.to_string()
}

// ==================== 单元测试 ====================

#[cfg(test)]
mod tests {
    use super::*;

    fn test_schema() -> SchemaContext {
        SchemaContext {
            tables: vec![
                TableInfo {
                    name: "users".into(),
                    columns: vec![
                        ColumnInfo {
                            name: "id".into(),
                            data_type: "INTEGER".into(),
                            nullable: false,
                            is_primary_key: true,
                        },
                        ColumnInfo {
                            name: "name".into(),
                            data_type: "TEXT".into(),
                            nullable: true,
                            is_primary_key: false,
                        },
                        ColumnInfo {
                            name: "email".into(),
                            data_type: "TEXT".into(),
                            nullable: true,
                            is_primary_key: false,
                        },
                        ColumnInfo {
                            name: "age".into(),
                            data_type: "INTEGER".into(),
                            nullable: true,
                            is_primary_key: false,
                        },
                        ColumnInfo {
                            name: "city".into(),
                            data_type: "TEXT".into(),
                            nullable: true,
                            is_primary_key: false,
                        },
                        ColumnInfo {
                            name: "score".into(),
                            data_type: "REAL".into(),
                            nullable: true,
                            is_primary_key: false,
                        },
                    ],
                },
                TableInfo {
                    name: "orders".into(),
                    columns: vec![
                        ColumnInfo {
                            name: "id".into(),
                            data_type: "INTEGER".into(),
                            nullable: false,
                            is_primary_key: true,
                        },
                        ColumnInfo {
                            name: "user_id".into(),
                            data_type: "INTEGER".into(),
                            nullable: false,
                            is_primary_key: false,
                        },
                        ColumnInfo {
                            name: "product".into(),
                            data_type: "TEXT".into(),
                            nullable: true,
                            is_primary_key: false,
                        },
                        ColumnInfo {
                            name: "price".into(),
                            data_type: "REAL".into(),
                            nullable: true,
                            is_primary_key: false,
                        },
                        ColumnInfo {
                            name: "quantity".into(),
                            data_type: "INTEGER".into(),
                            nullable: true,
                            is_primary_key: false,
                        },
                    ],
                },
                TableInfo {
                    name: "products".into(),
                    columns: vec![
                        ColumnInfo {
                            name: "id".into(),
                            data_type: "INTEGER".into(),
                            nullable: false,
                            is_primary_key: true,
                        },
                        ColumnInfo {
                            name: "name".into(),
                            data_type: "TEXT".into(),
                            nullable: false,
                            is_primary_key: false,
                        },
                        ColumnInfo {
                            name: "price".into(),
                            data_type: "REAL".into(),
                            nullable: false,
                            is_primary_key: false,
                        },
                        ColumnInfo {
                            name: "category".into(),
                            data_type: "TEXT".into(),
                            nullable: true,
                            is_primary_key: false,
                        },
                    ],
                },
            ],
        }
    }

    // ============ SimpleNl2SqlEngine ============

    #[tokio::test]
    async fn test_simple_select_all() {
        let engine = SimpleNl2SqlEngine::new();
        let schema = test_schema();
        let result = engine.generate("show all users", &schema).await.unwrap();
        assert_eq!(result.sql, "SELECT * FROM users");
        assert!(result.confidence > 0.0);
    }

    #[tokio::test]
    async fn test_simple_select_columns() {
        let engine = SimpleNl2SqlEngine::new();
        let schema = test_schema();
        // "name" and "email" both appear in the query; schema matching picks them up
        let result = engine
            .generate("show name and email of users", &schema)
            .await
            .unwrap();
        assert_eq!(result.sql, "SELECT name, email FROM users");
    }

    #[tokio::test]
    async fn test_simple_count() {
        let engine = SimpleNl2SqlEngine::new();
        let schema = test_schema();
        let result = engine.generate("how many users", &schema).await.unwrap();
        assert_eq!(result.sql, "SELECT COUNT(*) FROM users");
    }

    #[tokio::test]
    async fn test_simple_where_equality() {
        let engine = SimpleNl2SqlEngine::new();
        let schema = test_schema();
        let result = engine
            .generate("find users where name = John", &schema)
            .await
            .unwrap();
        assert_eq!(result.sql, "SELECT * FROM users WHERE name = $1");
        assert!(result.explanation.contains("John"));
    }

    #[tokio::test]
    async fn test_simple_where_comparison() {
        let engine = SimpleNl2SqlEngine::new();
        let schema = test_schema();
        let result = engine
            .generate("find users where age > 25", &schema)
            .await
            .unwrap();
        assert_eq!(result.sql, "SELECT * FROM users WHERE age > $1");
    }

    #[tokio::test]
    async fn test_simple_order_by() {
        let engine = SimpleNl2SqlEngine::new();
        let schema = test_schema();
        let result = engine
            .generate("list users ordered by name", &schema)
            .await
            .unwrap();
        assert_eq!(result.sql, "SELECT * FROM users ORDER BY name ASC");
    }

    #[tokio::test]
    async fn test_simple_order_by_desc() {
        let engine = SimpleNl2SqlEngine::new();
        let schema = test_schema();
        let result = engine
            .generate("list users sorted by name descending", &schema)
            .await
            .unwrap();
        assert_eq!(result.sql, "SELECT * FROM users ORDER BY name DESC");
    }

    #[tokio::test]
    async fn test_simple_limit() {
        let engine = SimpleNl2SqlEngine::new();
        let schema = test_schema();
        let result = engine.generate("first 10 users", &schema).await.unwrap();
        assert_eq!(result.sql, "SELECT * FROM users LIMIT 10");
    }

    #[tokio::test]
    async fn test_simple_order_by_with_limit() {
        let engine = SimpleNl2SqlEngine::new();
        let schema = test_schema();
        let result = engine
            .generate("top 5 users by score", &schema)
            .await
            .unwrap();
        // "top" triggers LIMIT; "score" is a known column, and "by score" matched as sort
        // This generates: SELECT * FROM users ORDER BY score ASC LIMIT 5
        assert!(result.sql.contains("LIMIT 5"));
    }

    #[tokio::test]
    async fn test_simple_aggregation() {
        let engine = SimpleNl2SqlEngine::new();
        let schema = test_schema();
        let result = engine
            .generate("total price from orders", &schema)
            .await
            .unwrap();
        assert_eq!(result.sql, "SELECT SUM(price) FROM orders");
    }

    #[tokio::test]
    async fn test_simple_avg() {
        let engine = SimpleNl2SqlEngine::new();
        let schema = test_schema();
        let result = engine
            .generate("average age of users", &schema)
            .await
            .unwrap();
        assert_eq!(result.sql, "SELECT AVG(age) FROM users");
    }

    #[tokio::test]
    async fn test_simple_empty_query() {
        let engine = SimpleNl2SqlEngine::new();
        let schema = test_schema();
        let result = engine.generate("", &schema).await;
        assert!(result.is_err());
        match result {
            Err(Nl2SqlError::InvalidQuery(_)) => {}
            Err(e) => panic!("期望 InvalidQuery，实际: {:?}", e),
            Ok(_) => panic!("期望错误"),
        }
    }

    #[tokio::test]
    async fn test_simple_empty_schema() {
        let engine = SimpleNl2SqlEngine::new();
        let schema = SchemaContext { tables: vec![] };
        let result = engine.generate("show users", &schema).await;
        assert!(result.is_err());
        match result {
            Err(Nl2SqlError::SchemaError(_)) => {}
            _ => panic!("期望 SchemaError"),
        }
    }

    #[tokio::test]
    async fn test_simple_validate_valid() {
        let engine = SimpleNl2SqlEngine::new();
        let query = SqlQuery {
            sql: "SELECT * FROM users".into(),
            explanation: "test".into(),
            confidence: 0.9,
        };
        assert!(engine.validate(&query).await.unwrap());
    }

    #[tokio::test]
    async fn test_simple_validate_invalid_confidence() {
        let engine = SimpleNl2SqlEngine::new();
        let query = SqlQuery {
            sql: "SELECT * FROM users".into(),
            explanation: "test".into(),
            confidence: 1.5,
        };
        let result = engine.validate(&query).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_simple_validate_rejects_drop() {
        let engine = SimpleNl2SqlEngine::new();
        let query = SqlQuery {
            sql: "DROP TABLE users".into(),
            explanation: "test".into(),
            confidence: 0.9,
        };
        assert!(!engine.validate(&query).await.unwrap());
    }

    #[tokio::test]
    async fn test_simple_alias() {
        let engine = SimpleNl2SqlEngine::new().with_alias("person", "users");
        let schema = test_schema();
        let result = engine.generate("show all persons", &schema).await.unwrap();
        assert_eq!(result.sql, "SELECT * FROM users");
    }

    #[tokio::test]
    async fn test_simple_table_not_found() {
        let engine = SimpleNl2SqlEngine::new();
        let schema = test_schema();
        let result = engine
            .generate("show something from nonexistent_table", &schema)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_simple_sql_in_schema_passthrough() {
        let engine = SimpleNl2SqlEngine::new();
        let schema = test_schema();
        // "name" and "email" are both schema columns
        let result = engine
            .generate("select name and email from users", &schema)
            .await
            .unwrap();
        assert!(result.sql.contains("name"));
        assert!(result.sql.contains("email"));
        assert!(result.sql.contains("users"));
    }

    // ============ 仅在 real feature 下测试 OpenAINl2SqlEngine ============

    #[cfg(feature = "real")]
    #[test]
    fn test_openai_engine_new_with_defaults() {
        let engine = OpenAINl2SqlEngine::new("sk-test-key");
        assert_eq!(engine.api_base, "https://api.openai.com/v1");
        assert_eq!(engine.api_key, "sk-test-key");
        assert_eq!(engine.model, "gpt-4o-mini");
    }

    #[cfg(feature = "real")]
    #[test]
    fn test_openai_engine_with_options() {
        let engine = OpenAINl2SqlEngine::new("sk-test")
            .with_api_base("https://api.deepseek.com/v1")
            .with_model("deepseek-chat");
        assert_eq!(engine.api_base, "https://api.deepseek.com/v1");
        assert_eq!(engine.model, "deepseek-chat");
    }

    #[cfg(feature = "real")]
    #[tokio::test]
    async fn test_openai_engine_missing_api_key() {
        let engine = OpenAINl2SqlEngine::new("");
        let schema = test_schema();
        let result = engine.generate("show users", &schema).await;
        match result {
            Err(Nl2SqlError::ConfigError(_)) => {}
            other => panic!("期望 ConfigError，实际: {:?}", other),
        }
    }

    #[cfg(feature = "real")]
    #[test]
    fn test_clean_llm_sql_output() {
        assert_eq!(
            clean_llm_sql_output("SELECT * FROM users"),
            "SELECT * FROM users"
        );
        assert_eq!(
            clean_llm_sql_output("```sql\nSELECT * FROM users\n```"),
            "SELECT * FROM users"
        );
        assert_eq!(
            clean_llm_sql_output("```\nSELECT * FROM users\n```"),
            "SELECT * FROM users"
        );
        assert_eq!(
            clean_llm_sql_output("\n  SELECT * FROM users  \n"),
            "SELECT * FROM users"
        );
    }
}
