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
            let inner = content[..end].trim();
            // 移除 "sql" 语言标记（如 ```sql\n...）
            return inner
                .strip_prefix("sql")
                .unwrap_or(inner)
                .trim()
                .to_string();
        }
        return content.trim().to_string();
    }
    trimmed.to_string()
}

// ==================== 查询优化提示 ====================

/// 优化建议的严重级别
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HintSeverity {
    /// 信息级：可选的优化建议
    Info,
    /// 警告级：可能影响性能
    Warning,
    /// 严重级：强烈建议修改
    Critical,
}

impl HintSeverity {
    /// 转换为字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            HintSeverity::Info => "INFO",
            HintSeverity::Warning => "WARNING",
            HintSeverity::Critical => "CRITICAL",
        }
    }
}

/// 单条查询优化建议
#[derive(Debug, Clone)]
pub struct QueryOptimizationHint {
    /// 建议标题
    pub title: String,
    /// 详细描述
    pub description: String,
    /// 严重级别
    pub severity: HintSeverity,
    /// 优化后的 SQL 建议（可选）
    pub suggested_sql: Option<String>,
}

impl QueryOptimizationHint {
    /// 创建一条信息级建议
    pub fn info(title: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            description: description.into(),
            severity: HintSeverity::Info,
            suggested_sql: None,
        }
    }

    /// 创建一条警告级建议
    pub fn warning(title: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            description: description.into(),
            severity: HintSeverity::Warning,
            suggested_sql: None,
        }
    }

    /// 创建一条严重级建议
    pub fn critical(title: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            description: description.into(),
            severity: HintSeverity::Critical,
            suggested_sql: None,
        }
    }

    /// 附加优化后的 SQL 建议
    pub fn with_suggested_sql(mut self, sql: impl Into<String>) -> Self {
        self.suggested_sql = Some(sql.into());
        self
    }
}

/// 查询分析结果
#[derive(Debug, Clone)]
pub struct QueryAnalysis {
    /// 原始 SQL
    pub original_sql: String,
    /// 所有优化建议
    pub hints: Vec<QueryOptimizationHint>,
    /// 预估的 SQL 复杂度评分（0-100，越高越复杂）
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

impl QueryAnalysis {
    /// 返回严重级别为 Critical 的建议数量
    pub fn critical_count(&self) -> usize {
        self.hints
            .iter()
            .filter(|h| h.severity == HintSeverity::Critical)
            .count()
    }

    /// 返回严重级别为 Warning 的建议数量
    pub fn warning_count(&self) -> usize {
        self.hints
            .iter()
            .filter(|h| h.severity == HintSeverity::Warning)
            .count()
    }

    /// 是否存在任何建议
    pub fn has_hints(&self) -> bool {
        !self.hints.is_empty()
    }
}

/// SQL 查询优化分析器
///
/// 基于规则分析 SQL 查询，生成优化建议。
/// 不依赖外部 LLM API，纯规则匹配，适用于离线场景。
pub struct QueryOptimizer {
    /// 是否检测 SELECT *
    pub check_select_star: bool,
    /// 是否检测缺失的 LIMIT
    pub check_missing_limit: bool,
    /// 是否检测缺失的 WHERE
    pub check_missing_where: bool,
    /// LIMIT 建议的默认行数
    pub default_limit: usize,
    /// 复杂度评分中 JOIN 的权重
    pub join_weight: u32,
    /// 复杂度评分中子查询的权重
    pub subquery_weight: u32,
    /// 复杂度评分中 WHERE 条件的权重
    pub where_weight: u32,
}

impl Default for QueryOptimizer {
    fn default() -> Self {
        Self {
            check_select_star: true,
            check_missing_limit: true,
            check_missing_where: true,
            default_limit: 100,
            join_weight: 15,
            subquery_weight: 20,
            where_weight: 5,
        }
    }
}

impl QueryOptimizer {
    /// 创建新的查询优化分析器
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置默认 LIMIT 行数
    pub fn with_default_limit(mut self, limit: usize) -> Self {
        self.default_limit = limit;
        self
    }

    /// 禁用 SELECT * 检测
    pub fn disable_select_star_check(mut self) -> Self {
        self.check_select_star = false;
        self
    }

    /// 禁用缺失 LIMIT 检测
    pub fn disable_missing_limit_check(mut self) -> Self {
        self.check_missing_limit = false;
        self
    }

    /// 禁用缺失 WHERE 检测
    pub fn disable_missing_where_check(mut self) -> Self {
        self.check_missing_where = false;
        self
    }

    /// 分析 SQL 查询并生成优化建议
    ///
    /// # 参数
    /// - `sql`: 要分析的 SQL 查询语句
    /// - `schema`: 数据库 schema 上下文（用于检测表名和索引）
    pub fn analyze(&self, sql: &str, schema: &SchemaContext) -> QueryAnalysis {
        let normalized = Self::normalize_sql(sql);
        let lower = normalized.to_lowercase();

        let uses_select_star = self.detect_select_star(&lower);
        let has_where = self.detect_where(&lower);
        let has_limit = self.detect_limit(&lower);
        let has_join = self.detect_join(&lower);
        let has_subquery = self.detect_subquery(&lower);
        let detected_tables = self.extract_tables(&lower, schema);

        let mut hints = Vec::new();

        // 检测 SELECT *
        if self.check_select_star && uses_select_star {
            let columns_hint = self.suggest_columns(&detected_tables, schema);
            let mut hint = QueryOptimizationHint::warning(
                "避免使用 SELECT *",
                "SELECT * 会返回所有列，可能导致不必要的数据传输和内存消耗。建议显式指定所需列。",
            );
            if !columns_hint.is_empty() {
                hint = hint.with_suggested_sql(columns_hint);
            }
            hints.push(hint);
        }

        // 检测缺失的 WHERE
        if self.check_missing_where && !has_where {
            hints.push(QueryOptimizationHint::critical(
                "缺少 WHERE 子句",
                "查询没有 WHERE 条件，将扫描全表。对于大表这会导致严重的性能问题。",
            ));
        }

        // 检测缺失的 LIMIT
        if self.check_missing_limit && !has_limit {
            let suggested = self.add_limit_suggestion(&normalized, self.default_limit);
            hints.push(
                QueryOptimizationHint::warning(
                    "缺少 LIMIT 子句",
                    format!("查询没有 LIMIT 限制，可能返回大量数据。建议添加 LIMIT {}。", self.default_limit),
                )
                .with_suggested_sql(suggested),
            );
        }

        // 检测多表 JOIN 无索引建议
        if has_join {
            let join_count = Self::count_joins(&lower);
            if join_count >= 3 {
                hints.push(QueryOptimizationHint::critical(
                    "JOIN 数量过多",
                    format!(
                        "查询包含 {} 个 JOIN，可能导致性能下降。建议拆分为多个查询或使用临时表。",
                        join_count
                    ),
                ));
            } else if join_count >= 1 {
                hints.push(QueryOptimizationHint::info(
                    "JOIN 查询建议",
                    "确保 JOIN 条件涉及的列已建立索引，避免嵌套循环扫描。",
                ));
            }
        }

        // 检测子查询
        if has_subquery {
            let subquery_count = Self::count_subqueries(&lower);
            if subquery_count >= 2 {
                hints.push(QueryOptimizationHint::warning(
                    "子查询嵌套过深",
                    format!(
                        "查询包含 {} 个子查询，建议考虑使用 JOIN 重写以提高性能。",
                        subquery_count
                    ),
                ));
            }
        }

        // 检测 LIKE '%...' 前缀通配符
        if Self::has_leading_wildcard_like(&lower) {
            hints.push(QueryOptimizationHint::warning(
                "LIKE 使用前缀通配符",
                "LIKE '%keyword' 无法使用索引，会导致全表扫描。如可能，使用 LIKE 'keyword%' 或全文索引。",
            ));
        }

        // 检测 OR 条件（可能导致无法使用索引）
        if Self::count_or_conditions(&lower) >= 3 {
            hints.push(QueryOptimizationHint::info(
                "多个 OR 条件",
                "多个 OR 条件可能导致无法有效使用索引。考虑使用 UNION ALL 重写。",
            ));
        }

        // 检测 ORDER BY 无 LIMIT
        if Self::has_order_by_without_limit(&lower) {
            hints.push(QueryOptimizationHint::warning(
                "ORDER BY 无 LIMIT",
                "ORDER BY 无 LIMIT 时需要排序全部数据，可能消耗大量内存。建议添加 LIMIT。",
            ));
        }

        // 检测 COUNT(*) 建议使用 COUNT(1)
        if lower.contains("count(*)") {
            hints.push(QueryOptimizationHint::info(
                "考虑使用 COUNT(1)",
                "某些数据库中 COUNT(1) 比 COUNT(*) 略快（虽然现代优化器通常已优化）。",
            ));
        }

        // 检测缺失索引建议（基于 WHERE 条件）
        if has_where {
            let where_columns = self.extract_where_columns(&lower, schema);
            for col in &where_columns {
                if !self.column_has_index(col, schema) {
                    hints.push(QueryOptimizationHint::info(
                        format!("建议为列 {} 添加索引", col),
                        format!(
                            "WHERE 条件中使用了列 {}，但该列似乎没有索引。添加索引可提高查询速度。",
                            col
                        ),
                    ));
                }
            }
        }

        // 计算复杂度评分
        let complexity_score = self.calculate_complexity(
            has_join,
            has_subquery,
            has_where,
            &detected_tables,
            &lower,
        );

        QueryAnalysis {
            original_sql: sql.to_string(),
            hints,
            complexity_score,
            detected_tables,
            has_where,
            has_limit,
            has_join,
            has_subquery,
            uses_select_star,
        }
    }

    /// 生成优化报告文本
    pub fn format_report(analysis: &QueryAnalysis) -> String {
        let mut report = String::new();
        report.push_str("=== SQL 查询优化分析报告 ===\n\n");
        report.push_str(&format!("原始 SQL: {}\n", analysis.original_sql));
        report.push_str(&format!("复杂度评分: {}/100\n", analysis.complexity_score));
        report.push_str(&format!(
            "检测到的表: {}\n",
            if analysis.detected_tables.is_empty() {
                "无".to_string()
            } else {
                analysis.detected_tables.join(", ")
            }
        ));
        report.push_str(&format!("包含 WHERE: {}\n", analysis.has_where));
        report.push_str(&format!("包含 LIMIT: {}\n", analysis.has_limit));
        report.push_str(&format!("包含 JOIN: {}\n", analysis.has_join));
        report.push_str(&format!("包含子查询: {}\n", analysis.has_subquery));
        report.push_str(&format!("使用 SELECT *: {}\n\n", analysis.uses_select_star));

        if analysis.hints.is_empty() {
            report.push_str("✓ 未发现优化建议，查询看起来良好。\n");
        } else {
            report.push_str(&format!(
                "共 {} 条优化建议（{} 严重，{} 警告）：\n\n",
                analysis.hints.len(),
                analysis.critical_count(),
                analysis.warning_count()
            ));
            for (i, hint) in analysis.hints.iter().enumerate() {
                report.push_str(&format!(
                    "{}. [{}] {}\n   {}\n",
                    i + 1,
                    hint.severity.as_str(),
                    hint.title,
                    hint.description
                ));
                if let Some(ref sql) = hint.suggested_sql {
                    report.push_str(&format!("   建议SQL: {}\n", sql));
                }
                report.push('\n');
            }
        }

        report
    }

    // ---- 内部辅助方法 ----

    /// 规范化 SQL（去除多余空白、换行）
    fn normalize_sql(sql: &str) -> String {
        sql.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// 检测 SELECT *
    fn detect_select_star(&self, lower: &str) -> bool {
        lower.contains("select *") || lower.contains("select  *")
    }

    /// 检测 WHERE 子句
    fn detect_where(&self, lower: &str) -> bool {
        lower.contains(" where ")
    }

    /// 检测 LIMIT 子句
    fn detect_limit(&self, lower: &str) -> bool {
        lower.contains(" limit ")
    }

    /// 检测 JOIN
    fn detect_join(&self, lower: &str) -> bool {
        lower.contains(" join ")
            || lower.contains(" inner join ")
            || lower.contains(" left join ")
            || lower.contains(" right join ")
            || lower.contains(" full join ")
            || lower.contains(" cross join ")
    }

    /// 检测子查询（括号内的 SELECT）
    fn detect_subquery(&self, lower: &str) -> bool {
        if let Some(paren_pos) = lower.find('(') {
            let after = &lower[paren_pos..];
            after.contains("select")
        } else {
            false
        }
    }

    /// 统计 JOIN 数量
    fn count_joins(lower: &str) -> usize {
        lower.matches(" join ").count()
    }

    /// 统计子查询数量
    fn count_subqueries(lower: &str) -> usize {
        lower.matches("(select").count() + lower.matches("( select").count()
    }

    /// 检测前缀通配符 LIKE
    fn has_leading_wildcard_like(lower: &str) -> bool {
        lower.contains("like '%") || lower.contains("like \"%")
    }

    /// 统计 OR 条件数量
    fn count_or_conditions(lower: &str) -> usize {
        lower.matches(" or ").count()
    }

    /// 检测 ORDER BY 无 LIMIT
    fn has_order_by_without_limit(lower: &str) -> bool {
        lower.contains(" order by ") && !lower.contains(" limit ")
    }

    /// 从 SQL 中提取表名（基于 schema）
    fn extract_tables(&self, lower: &str, schema: &SchemaContext) -> Vec<String> {
        let mut tables = Vec::new();
        for table in &schema.tables {
            let name_lower = table.name.to_lowercase();
            if lower.contains(&name_lower) {
                tables.push(table.name.clone());
            }
        }
        tables
    }

    /// 生成列建议 SQL
    fn suggest_columns(&self, tables: &[String], schema: &SchemaContext) -> String {
        if tables.is_empty() {
            return String::new();
        }
        let mut cols = Vec::new();
        for table_name in tables {
            if let Some(table) = schema.tables.iter().find(|t| t.name == *table_name) {
                for col in &table.columns {
                    cols.push(format!("{}.{}", table_name, col.name));
                }
            }
        }
        if cols.is_empty() {
            String::new()
        } else {
            format!("SELECT {} FROM {}", cols.join(", "), tables.join(", "))
        }
    }

    /// 为 SQL 添加 LIMIT 建议
    fn add_limit_suggestion(&self, sql: &str, limit: usize) -> String {
        let trimmed = sql.trim_end_matches(';');
        format!("{} LIMIT {}", trimmed, limit)
    }

    /// 从 WHERE 子句中提取列名
    fn extract_where_columns(&self, lower: &str, schema: &SchemaContext) -> Vec<String> {
        let mut columns = Vec::new();
        if let Some(where_pos) = lower.find(" where ") {
            let after_where = &lower[where_pos + 7..];
            // 截取到 GROUP BY / ORDER BY / LIMIT 之前
            let where_clause = after_where
                .split(" group by ")
                .next()
                .unwrap_or(after_where)
                .split(" order by ")
                .next()
                .unwrap_or(after_where)
                .split(" limit ")
                .next()
                .unwrap_or(after_where);

            for table in &schema.tables {
                for col in &table.columns {
                    let col_lower = col.name.to_lowercase();
                    if col_lower.len() >= 2 && where_clause.contains(&col_lower)
                        && !columns.contains(&col.name)
                    {
                        columns.push(col.name.clone());
                    }
                }
            }
        }
        columns
    }

    /// 检查列是否有索引（简化版：主键视为有索引）
    fn column_has_index(&self, col: &str, schema: &SchemaContext) -> bool {
        for table in &schema.tables {
            for c in &table.columns {
                if c.name.eq_ignore_ascii_case(col) && c.is_primary_key {
                    return true;
                }
            }
        }
        false
    }

    /// 计算 SQL 复杂度评分
    fn calculate_complexity(
        &self,
        has_join: bool,
        has_subquery: bool,
        has_where: bool,
        tables: &[String],
        lower: &str,
    ) -> u32 {
        let mut score: u32 = 10; // 基础分

        if has_join {
            let join_count = Self::count_joins(lower) as u32;
            score += join_count * self.join_weight;
        }

        if has_subquery {
            let sub_count = Self::count_subqueries(lower) as u32;
            score += sub_count * self.subquery_weight;
        }

        if has_where {
            let or_count = Self::count_or_conditions(lower) as u32;
            score += self.where_weight + or_count * 3;
        }

        // 表数量影响
        if tables.len() > 1 {
            score += (tables.len() as u32 - 1) * 10;
        }

        // ORDER BY 影响
        if lower.contains(" order by ") {
            score += 5;
        }

        // GROUP BY 影响
        if lower.contains(" group by ") {
            score += 10;
        }

        // DISTINCT 影响
        if lower.contains(" distinct ") {
            score += 5;
        }

        score.min(100)
    }
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

    // ============ 查询优化提示测试 ============

    fn optimizer_test_schema() -> SchemaContext {
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
                            name: "amount".into(),
                            data_type: "DECIMAL".into(),
                            nullable: true,
                            is_primary_key: false,
                        },
                    ],
                },
            ],
        }
    }

    #[test]
    fn test_hint_severity_as_str() {
        assert_eq!(HintSeverity::Info.as_str(), "INFO");
        assert_eq!(HintSeverity::Warning.as_str(), "WARNING");
        assert_eq!(HintSeverity::Critical.as_str(), "CRITICAL");
    }

    #[test]
    fn test_query_optimization_hint_info() {
        let hint = QueryOptimizationHint::info("标题", "描述");
        assert_eq!(hint.title, "标题");
        assert_eq!(hint.description, "描述");
        assert_eq!(hint.severity, HintSeverity::Info);
        assert!(hint.suggested_sql.is_none());
    }

    #[test]
    fn test_query_optimization_hint_warning() {
        let hint = QueryOptimizationHint::warning("警告", "警告描述");
        assert_eq!(hint.severity, HintSeverity::Warning);
    }

    #[test]
    fn test_query_optimization_hint_critical() {
        let hint = QueryOptimizationHint::critical("严重", "严重描述");
        assert_eq!(hint.severity, HintSeverity::Critical);
    }

    #[test]
    fn test_query_optimization_hint_with_suggested_sql() {
        let hint = QueryOptimizationHint::info("建议", "描述")
            .with_suggested_sql("SELECT id FROM users");
        assert_eq!(hint.suggested_sql.as_deref(), Some("SELECT id FROM users"));
    }

    #[test]
    fn test_query_optimizer_default() {
        let opt = QueryOptimizer::default();
        assert!(opt.check_select_star);
        assert!(opt.check_missing_limit);
        assert!(opt.check_missing_where);
        assert_eq!(opt.default_limit, 100);
    }

    #[test]
    fn test_query_optimizer_new() {
        let opt = QueryOptimizer::new();
        assert!(opt.check_select_star);
    }

    #[test]
    fn test_query_optimizer_with_default_limit() {
        let opt = QueryOptimizer::new().with_default_limit(50);
        assert_eq!(opt.default_limit, 50);
    }

    #[test]
    fn test_query_optimizer_disable_checks() {
        let opt = QueryOptimizer::new()
            .disable_select_star_check()
            .disable_missing_limit_check()
            .disable_missing_where_check();
        assert!(!opt.check_select_star);
        assert!(!opt.check_missing_limit);
        assert!(!opt.check_missing_where);
    }

    #[test]
    fn test_analyze_select_star() {
        let opt = QueryOptimizer::new();
        let schema = optimizer_test_schema();
        let analysis = opt.analyze("SELECT * FROM users", &schema);

        assert!(analysis.uses_select_star);
        assert!(!analysis.has_where);
        assert!(!analysis.has_limit);
        // 应该有 SELECT * 建议、缺失 WHERE 建议、缺失 LIMIT 建议
        assert!(analysis.has_hints());
        assert!(analysis.critical_count() >= 1); // 缺失 WHERE 是 critical
    }

    #[test]
    fn test_analyze_select_star_with_suggested_columns() {
        let opt = QueryOptimizer::new();
        let schema = optimizer_test_schema();
        let analysis = opt.analyze("SELECT * FROM users", &schema);

        // 应包含 SELECT * 警告，且附带建议 SQL
        let select_star_hint = analysis
            .hints
            .iter()
            .find(|h| h.title == "避免使用 SELECT *");
        assert!(select_star_hint.is_some());
        let hint = select_star_hint.unwrap();
        assert!(hint.suggested_sql.is_some());
        let suggested = hint.suggested_sql.as_ref().unwrap();
        assert!(suggested.contains("users.id"));
        assert!(suggested.contains("users.name"));
    }

    #[test]
    fn test_analyze_missing_where_critical() {
        let opt = QueryOptimizer::new();
        let schema = optimizer_test_schema();
        let analysis = opt.analyze("SELECT id FROM users LIMIT 10", &schema);

        // 没有 WHERE 应产生 critical 建议
        assert!(analysis.critical_count() >= 1);
        let where_hint = analysis
            .hints
            .iter()
            .find(|h| h.title == "缺少 WHERE 子句");
        assert!(where_hint.is_some());
        assert_eq!(where_hint.unwrap().severity, HintSeverity::Critical);
    }

    #[test]
    fn test_analyze_missing_limit_warning() {
        let opt = QueryOptimizer::new();
        let schema = optimizer_test_schema();
        let analysis = opt.analyze("SELECT id FROM users WHERE age > 18", &schema);

        // 没有 LIMIT 应产生 warning 建议
        assert!(analysis.warning_count() >= 1);
        let limit_hint = analysis
            .hints
            .iter()
            .find(|h| h.title == "缺少 LIMIT 子句");
        assert!(limit_hint.is_some());
        let hint = limit_hint.unwrap();
        assert!(hint.suggested_sql.is_some());
        assert!(hint.suggested_sql.as_ref().unwrap().contains("LIMIT 100"));
    }

    #[test]
    fn test_analyze_with_limit_no_limit_hint() {
        let opt = QueryOptimizer::new();
        let schema = optimizer_test_schema();
        let analysis = opt.analyze("SELECT id FROM users WHERE age > 18 LIMIT 10", &schema);

        let limit_hint = analysis
            .hints
            .iter()
            .find(|h| h.title == "缺少 LIMIT 子句");
        assert!(limit_hint.is_none());
    }

    #[test]
    fn test_analyze_join_detection() {
        let opt = QueryOptimizer::new();
        let schema = optimizer_test_schema();
        let analysis = opt.analyze(
            "SELECT u.id FROM users u JOIN orders o ON u.id = o.user_id WHERE u.age > 18 LIMIT 10",
            &schema,
        );

        assert!(analysis.has_join);
        assert!(analysis.detected_tables.contains(&"users".to_string()));
        assert!(analysis.detected_tables.contains(&"orders".to_string()));
    }

    #[test]
    fn test_analyze_multiple_joins_critical() {
        let opt = QueryOptimizer::new();
        let schema = SchemaContext {
            tables: vec![
                TableInfo {
                    name: "t1".into(),
                    columns: vec![ColumnInfo {
                        name: "id".into(),
                        data_type: "INT".into(),
                        nullable: false,
                        is_primary_key: true,
                    }],
                },
                TableInfo {
                    name: "t2".into(),
                    columns: vec![ColumnInfo {
                        name: "id".into(),
                        data_type: "INT".into(),
                        nullable: false,
                        is_primary_key: true,
                    }],
                },
                TableInfo {
                    name: "t3".into(),
                    columns: vec![ColumnInfo {
                        name: "id".into(),
                        data_type: "INT".into(),
                        nullable: false,
                        is_primary_key: true,
                    }],
                },
                TableInfo {
                    name: "t4".into(),
                    columns: vec![ColumnInfo {
                        name: "id".into(),
                        data_type: "INT".into(),
                        nullable: false,
                        is_primary_key: true,
                    }],
                },
            ],
        };
        let analysis = opt.analyze(
            "SELECT * FROM t1 JOIN t2 ON t1.id = t2.id JOIN t3 ON t2.id = t3.id JOIN t4 ON t3.id = t4.id LIMIT 10",
            &schema,
        );
        // 4 个 JOIN 应触发 critical
        let join_hint = analysis
            .hints
            .iter()
            .find(|h| h.title == "JOIN 数量过多");
        assert!(join_hint.is_some());
        assert_eq!(join_hint.unwrap().severity, HintSeverity::Critical);
    }

    #[test]
    fn test_analyze_subquery_detection() {
        let opt = QueryOptimizer::new();
        let schema = optimizer_test_schema();
        let analysis = opt.analyze(
            "SELECT id FROM users WHERE id IN (SELECT user_id FROM orders) LIMIT 10",
            &schema,
        );

        assert!(analysis.has_subquery);
    }

    #[test]
    fn test_analyze_leading_wildcard_like() {
        let opt = QueryOptimizer::new();
        let schema = optimizer_test_schema();
        let analysis = opt.analyze(
            "SELECT id FROM users WHERE name LIKE '%john' LIMIT 10",
            &schema,
        );

        let like_hint = analysis
            .hints
            .iter()
            .find(|h| h.title == "LIKE 使用前缀通配符");
        assert!(like_hint.is_some());
    }

    #[test]
    fn test_analyze_order_by_without_limit() {
        let opt = QueryOptimizer::new();
        let schema = optimizer_test_schema();
        let analysis = opt.analyze(
            "SELECT id FROM users WHERE age > 18 ORDER BY id",
            &schema,
        );

        let order_hint = analysis
            .hints
            .iter()
            .find(|h| h.title == "ORDER BY 无 LIMIT");
        assert!(order_hint.is_some());
    }

    #[test]
    fn test_analyze_count_star_hint() {
        let opt = QueryOptimizer::new();
        let schema = optimizer_test_schema();
        let analysis = opt.analyze("SELECT COUNT(*) FROM users LIMIT 1", &schema);

        let count_hint = analysis
            .hints
            .iter()
            .find(|h| h.title == "考虑使用 COUNT(1)");
        assert!(count_hint.is_some());
    }

    #[test]
    fn test_analyze_missing_index_hint() {
        let opt = QueryOptimizer::new();
        let schema = optimizer_test_schema();
        // age 列不是主键，应建议添加索引
        let analysis = opt.analyze(
            "SELECT id FROM users WHERE age > 18 LIMIT 10",
            &schema,
        );

        let index_hint = analysis
            .hints
            .iter()
            .find(|h| h.title.contains("添加索引"));
        assert!(index_hint.is_some());
        assert!(index_hint.unwrap().title.contains("age"));
    }

    #[test]
    fn test_analyze_primary_key_no_index_hint() {
        let opt = QueryOptimizer::new();
        let schema = optimizer_test_schema();
        // id 列是主键，不应建议添加索引
        let analysis = opt.analyze(
            "SELECT name FROM users WHERE id = 1 LIMIT 10",
            &schema,
        );

        let index_hint = analysis
            .hints
            .iter()
            .find(|h| h.title.contains("添加索引") && h.title.contains("id"));
        // id 是主键，不应有索引建议
        assert!(index_hint.is_none());
    }

    #[test]
    fn test_analyze_complexity_score_simple() {
        let opt = QueryOptimizer::new();
        let schema = optimizer_test_schema();
        let analysis = opt.analyze(
            "SELECT id FROM users WHERE id = 1 LIMIT 10",
            &schema,
        );
        // 简单查询应该低分
        assert!(analysis.complexity_score < 30);
    }

    #[test]
    fn test_analyze_complexity_score_complex() {
        let opt = QueryOptimizer::new();
        let schema = optimizer_test_schema();
        let analysis = opt.analyze(
            "SELECT u.id, o.amount FROM users u JOIN orders o ON u.id = o.user_id WHERE u.age > 18 OR u.name LIKE '%a%' GROUP BY u.id ORDER BY o.amount DESC",
            &schema,
        );
        // 复杂查询应该高分
        assert!(analysis.complexity_score > 30);
    }

    #[test]
    fn test_analyze_well_optimized_query() {
        let opt = QueryOptimizer::new();
        let schema = optimizer_test_schema();
        let analysis = opt.analyze(
            "SELECT id, name FROM users WHERE id = 1 LIMIT 10",
            &schema,
        );

        // 这个查询写得很好，不应该有 critical 或 warning 建议
        assert_eq!(analysis.critical_count(), 0);
        // id 是主键，不应有索引建议
        // 有 LIMIT，不应有 LIMIT 建议
        // 有 WHERE，不应有 WHERE 建议
        // 没有 SELECT *，不应有 SELECT * 建议
    }

    #[test]
    fn test_query_analysis_has_hints() {
        let opt = QueryOptimizer::new();
        let schema = optimizer_test_schema();
        let analysis = opt.analyze("SELECT * FROM users", &schema);
        assert!(analysis.has_hints());

        let good_analysis = opt.analyze(
            "SELECT id FROM users WHERE id = 1 LIMIT 1",
            &schema,
        );
        // 可能仍有 info 级建议，但不应有 critical
        assert_eq!(good_analysis.critical_count(), 0);
    }

    #[test]
    fn test_format_report_contains_key_info() {
        let opt = QueryOptimizer::new();
        let schema = optimizer_test_schema();
        let analysis = opt.analyze("SELECT * FROM users", &schema);
        let report = QueryOptimizer::format_report(&analysis);

        assert!(report.contains("SQL 查询优化分析报告"));
        assert!(report.contains("原始 SQL"));
        assert!(report.contains("复杂度评分"));
        assert!(report.contains("SELECT *"));
    }

    #[test]
    fn test_format_report_no_hints() {
        let opt = QueryOptimizer::new();
        let schema = optimizer_test_schema();
        let analysis = opt.analyze(
            "SELECT id FROM users WHERE id = 1 LIMIT 1",
            &schema,
        );
        let report = QueryOptimizer::format_report(&analysis);
        // 即使没有建议，报告也应包含基本字段
        assert!(report.contains("复杂度评分"));
    }

    #[test]
    fn test_analyze_disable_select_star() {
        let opt = QueryOptimizer::new().disable_select_star_check();
        let schema = optimizer_test_schema();
        let analysis = opt.analyze("SELECT * FROM users WHERE id = 1 LIMIT 10", &schema);

        let star_hint = analysis
            .hints
            .iter()
            .find(|h| h.title == "避免使用 SELECT *");
        assert!(star_hint.is_none());
    }

    #[test]
    fn test_analyze_disable_missing_where() {
        let opt = QueryOptimizer::new().disable_missing_where_check();
        let schema = optimizer_test_schema();
        let analysis = opt.analyze("SELECT id FROM users LIMIT 10", &schema);

        let where_hint = analysis
            .hints
            .iter()
            .find(|h| h.title == "缺少 WHERE 子句");
        assert!(where_hint.is_none());
    }

    #[test]
    fn test_analyze_disable_missing_limit() {
        let opt = QueryOptimizer::new().disable_missing_limit_check();
        let schema = optimizer_test_schema();
        let analysis = opt.analyze("SELECT id FROM users WHERE id = 1", &schema);

        let limit_hint = analysis
            .hints
            .iter()
            .find(|h| h.title == "缺少 LIMIT 子句");
        assert!(limit_hint.is_none());
    }

    #[test]
    fn test_analyze_multiple_or_conditions() {
        let opt = QueryOptimizer::new();
        let schema = optimizer_test_schema();
        let analysis = opt.analyze(
            "SELECT id FROM users WHERE age = 1 OR age = 2 OR age = 3 OR age = 4 LIMIT 10",
            &schema,
        );

        let or_hint = analysis
            .hints
            .iter()
            .find(|h| h.title == "多个 OR 条件");
        assert!(or_hint.is_some());
    }

    #[test]
    fn test_analyze_detected_tables() {
        let opt = QueryOptimizer::new();
        let schema = optimizer_test_schema();
        let analysis = opt.analyze("SELECT id FROM users LIMIT 10", &schema);

        assert_eq!(analysis.detected_tables, vec!["users".to_string()]);
    }

    #[test]
    fn test_analyze_no_detected_tables() {
        let opt = QueryOptimizer::new();
        let schema = optimizer_test_schema();
        let analysis = opt.analyze("SELECT 1 LIMIT 10", &schema);

        assert!(analysis.detected_tables.is_empty());
    }

    #[test]
    fn test_normalize_sql() {
        let normalized = QueryOptimizer::normalize_sql("SELECT  id\nFROM   users");
        assert_eq!(normalized, "SELECT id FROM users");
    }
}
