//! 增强查询构建器、连接池配置、错误处理。
//!
//! - [`QueryBuilderEnhanced`] — 支持 JOIN、GROUP BY、HAVING 的增强查询
//! - [`PoolConfig`] — 连接池配置（带验证）
//! - [`ErrorHandler`] — 错误分类、重试策略
//! - [`JoinClause`] — JOIN 子句
//! - [`RetryPolicy`] — 重试策略

use napi_derive::napi;
use sz_orm_core::dialect::get_dialect;
use sz_orm_core::DbType;

type Result<T> = napi::bindgen_prelude::Result<T>;

fn parse_db_type(s: &str) -> Result<DbType> {
    DbType::from_str(s).ok_or_else(|| napi::Error::from_reason(format!("unknown DbType: {}", s)))
}

fn dialect_or_err(db_type: DbType) -> Result<Box<dyn sz_orm_core::dialect::Dialect>> {
    get_dialect(db_type).map_err(|e| napi::Error::from_reason(e.to_string()))
}

// ============================================================================
// JOIN 子句
// ============================================================================

/// JOIN 类型
#[napi]
#[derive(Debug, PartialEq, Eq)]
pub enum JoinType {
    Inner,
    Left,
    Right,
    Full,
}

impl JoinType {
    /// SQL 关键字
    pub fn keyword(&self) -> &'static str {
        match self {
            JoinType::Inner => "INNER JOIN",
            JoinType::Left => "LEFT JOIN",
            JoinType::Right => "RIGHT JOIN",
            JoinType::Full => "FULL OUTER JOIN",
        }
    }
}

/// JOIN 子句
#[napi]
pub struct JoinClause {
    join_type: JoinType,
    table: String,
    on_condition: String,
    alias: String,
}

#[napi]
impl JoinClause {
    /// 创建 JOIN 子句
    #[napi(constructor)]
    pub fn new(join_type: JoinType, table: String, on_condition: String) -> Self {
        Self {
            join_type,
            table,
            on_condition,
            alias: String::new(),
        }
    }

    /// 设置表别名（链式）
    #[napi]
    pub fn set_alias(&mut self, alias: String) {
        self.alias = alias;
    }

    /// 生成 SQL 片段
    pub fn to_sql(&self, dialect: &dyn sz_orm_core::dialect::Dialect) -> String {
        let table_ref = if self.alias.is_empty() {
            dialect.quote(&self.table)
        } else {
            format!("{} AS {}", dialect.quote(&self.table), self.alias)
        };
        format!(
            "{} {} ON {}",
            self.join_type.keyword(),
            table_ref,
            self.on_condition
        )
    }
}

// ============================================================================
// 增强查询构建器
// ============================================================================

/// 增强查询结果
#[napi(object)]
pub struct EnhancedQueryResult {
    pub sql: String,
    pub param_count: u32,
}

/// 增强查询构建器：支持 JOIN、GROUP BY、HAVING、UNION。
#[napi]
pub struct QueryBuilderEnhanced {
    db_type: DbType,
    table: Option<String>,
    table_alias: String,
    select_columns: Vec<String>,
    joins: Vec<JoinClause>,
    where_clauses: Vec<String>,
    group_by: Vec<String>,
    having_clauses: Vec<String>,
    order_by: Vec<String>,
    limit_val: Option<u32>,
    offset_val: Option<u32>,
    distinct: bool,
}

#[napi]
impl QueryBuilderEnhanced {
    /// 创建增强查询构建器
    #[napi(constructor)]
    pub fn new(db_type: Option<String>) -> Result<Self> {
        let dt = db_type.unwrap_or_else(|| "mysql".to_string());
        Ok(Self {
            db_type: parse_db_type(&dt)?,
            table: None,
            table_alias: String::new(),
            select_columns: vec![],
            joins: vec![],
            where_clauses: vec![],
            group_by: vec![],
            having_clauses: vec![],
            order_by: vec![],
            limit_val: None,
            offset_val: None,
            distinct: false,
        })
    }

    /// 设置主表
    #[napi]
    pub fn set_table(&mut self, table: String) {
        self.table = Some(table);
    }

    /// 设置表别名
    #[napi]
    pub fn set_alias(&mut self, alias: String) {
        self.table_alias = alias;
    }

    /// 设置 SELECT 列
    #[napi]
    pub fn set_select(&mut self, columns: Vec<String>) {
        self.select_columns = columns;
    }

    /// 设置 DISTINCT
    #[napi]
    pub fn set_distinct(&mut self) {
        self.distinct = true;
    }

    /// 添加 JOIN
    pub fn add_join(&mut self, join: JoinClause) {
        self.joins.push(join);
    }

    /// 添加 WHERE 条件（参数化占位符 ?）
    #[napi]
    pub fn add_where(&mut self, condition: String) {
        self.where_clauses.push(condition);
    }

    /// 添加 GROUP BY 列
    #[napi]
    pub fn add_group_by(&mut self, column: String) {
        self.group_by.push(column);
    }

    /// 添加 HAVING 条件
    #[napi]
    pub fn add_having(&mut self, condition: String) {
        self.having_clauses.push(condition);
    }

    /// 添加 ORDER BY
    #[napi]
    pub fn add_order_by(&mut self, column: String) {
        self.order_by.push(column);
    }

    /// 添加降序 ORDER BY
    #[napi]
    pub fn add_order_desc(&mut self, column: String) {
        self.order_by.push(format!("{} DESC", column));
    }

    /// 设置 LIMIT
    #[napi]
    pub fn set_limit(&mut self, limit: u32) {
        self.limit_val = Some(limit);
    }

    /// 设置 OFFSET
    #[napi]
    pub fn set_offset(&mut self, offset: u32) {
        self.offset_val = Some(offset);
    }

    /// 构建 SELECT SQL
    #[napi]
    pub fn build(&self) -> Result<EnhancedQueryResult> {
        let dialect = dialect_or_err(self.db_type)?;
        let table = self
            .table
            .as_ref()
            .ok_or_else(|| napi::Error::from_reason("table not set"))?;

        let distinct_kw = if self.distinct { "DISTINCT " } else { "" };
        let cols = if self.select_columns.is_empty() {
            "*".to_string()
        } else {
            self.select_columns.join(", ")
        };

        let table_ref = if self.table_alias.is_empty() {
            dialect.quote(table)
        } else {
            format!("{} AS {}", dialect.quote(table), self.table_alias)
        };

        let mut sql = format!("SELECT {}{} FROM {}", distinct_kw, cols, table_ref);

        // JOINs
        for join in &self.joins {
            sql.push(' ');
            sql.push_str(&join.to_sql(&*dialect));
        }

        // WHERE
        if !self.where_clauses.is_empty() {
            sql.push_str(&format!(" WHERE {}", self.where_clauses.join(" AND ")));
        }

        // GROUP BY
        if !self.group_by.is_empty() {
            let groups: Vec<String> = self.group_by.iter().map(|c| dialect.quote(c)).collect();
            sql.push_str(&format!(" GROUP BY {}", groups.join(", ")));
        }

        // HAVING
        if !self.having_clauses.is_empty() {
            sql.push_str(&format!(" HAVING {}", self.having_clauses.join(" AND ")));
        }

        // ORDER BY
        if !self.order_by.is_empty() {
            sql.push_str(&format!(" ORDER BY {}", self.order_by.join(", ")));
        }

        // LIMIT / OFFSET
        if let Some(limit) = self.limit_val {
            sql.push_str(&format!(" LIMIT {}", limit));
        }
        if let Some(offset) = self.offset_val {
            sql.push_str(&format!(" OFFSET {}", offset));
        }

        // 参数数 = WHERE 条件数 + HAVING 条件数
        let param_count = (self.where_clauses.len() + self.having_clauses.len()) as u32;

        Ok(EnhancedQueryResult { sql, param_count })
    }

    /// 构建 COUNT 查询
    #[napi]
    pub fn build_count(&self) -> Result<EnhancedQueryResult> {
        let dialect = dialect_or_err(self.db_type)?;
        let table = self
            .table
            .as_ref()
            .ok_or_else(|| napi::Error::from_reason("table not set"))?;

        let table_ref = if self.table_alias.is_empty() {
            dialect.quote(table)
        } else {
            format!("{} AS {}", dialect.quote(table), self.table_alias)
        };

        let mut sql = format!("SELECT COUNT(*) AS count FROM {}", table_ref);

        for join in &self.joins {
            sql.push(' ');
            sql.push_str(&join.to_sql(&*dialect));
        }

        if !self.where_clauses.is_empty() {
            sql.push_str(&format!(" WHERE {}", self.where_clauses.join(" AND ")));
        }

        if !self.group_by.is_empty() {
            let groups: Vec<String> = self.group_by.iter().map(|c| dialect.quote(c)).collect();
            sql.push_str(&format!(" GROUP BY {}", groups.join(", ")));
        }

        Ok(EnhancedQueryResult {
            sql,
            param_count: self.where_clauses.len() as u32,
        })
    }
}

// ============================================================================
// 连接池配置
// ============================================================================

/// 连接池配置（带验证）
#[napi(object)]
pub struct PoolConfig {
    pub max_size: u32,
    pub min_idle: u32,
    pub acquire_timeout_secs: i64,
    pub idle_timeout_secs: i64,
    pub max_lifetime_secs: i64,
    pub health_check_interval_secs: i64,
    pub max_queue_size: u32,
}

impl PoolConfig {
    /// 创建默认配置
    pub fn defaults() -> Self {
        Self {
            max_size: 100,
            min_idle: 10,
            acquire_timeout_secs: 30,
            idle_timeout_secs: 600,
            max_lifetime_secs: 1800,
            health_check_interval_secs: 60,
            max_queue_size: 1000,
        }
    }

    /// 验证配置是否合法
    pub fn validate(&self) -> Result<()> {
        if self.max_size == 0 {
            return Err(napi::Error::from_reason("max_size must be > 0"));
        }
        if self.min_idle > self.max_size {
            return Err(napi::Error::from_reason("min_idle cannot exceed max_size"));
        }
        if self.acquire_timeout_secs == 0 {
            return Err(napi::Error::from_reason("acquire_timeout must be > 0"));
        }
        if self.max_queue_size == 0 {
            return Err(napi::Error::from_reason("max_queue_size must be > 0"));
        }
        Ok(())
    }

    /// 是否合法
    pub fn is_valid(&self) -> bool {
        self.validate().is_ok()
    }
}

/// 连接池配置构建器
#[napi]
#[allow(clippy::new_without_default)]
pub struct PoolConfigBuilder {
    config: PoolConfig,
}

#[napi]
#[allow(clippy::new_without_default)]
impl PoolConfigBuilder {
    /// 创建默认配置构建器
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            config: PoolConfig::defaults(),
        }
    }

    /// 设置最大连接数
    #[napi]
    pub fn set_max_size(&mut self, size: u32) {
        self.config.max_size = size;
    }

    /// 设置最小空闲连接
    #[napi]
    pub fn set_min_idle(&mut self, idle: u32) {
        self.config.min_idle = idle;
    }

    /// 设置获取超时（秒）
    #[napi]
    pub fn set_acquire_timeout(&mut self, secs: i64) {
        self.config.acquire_timeout_secs = secs;
    }

    /// 设置空闲超时（秒）
    #[napi]
    pub fn set_idle_timeout(&mut self, secs: i64) {
        self.config.idle_timeout_secs = secs;
    }

    /// 设置最大生命周期（秒）
    #[napi]
    pub fn set_max_lifetime(&mut self, secs: i64) {
        self.config.max_lifetime_secs = secs;
    }

    /// 设置健康检查间隔（秒）
    #[napi]
    pub fn set_health_check_interval(&mut self, secs: i64) {
        self.config.health_check_interval_secs = secs;
    }

    /// 设置最大队列大小
    #[napi]
    pub fn set_max_queue_size(&mut self, size: u32) {
        self.config.max_queue_size = size;
    }

    /// 构建配置（验证后返回）
    #[napi]
    pub fn build(&self) -> Result<PoolConfig> {
        self.config.validate()?;
        Ok(PoolConfig {
            max_size: self.config.max_size,
            min_idle: self.config.min_idle,
            acquire_timeout_secs: self.config.acquire_timeout_secs,
            idle_timeout_secs: self.config.idle_timeout_secs,
            max_lifetime_secs: self.config.max_lifetime_secs,
            health_check_interval_secs: self.config.health_check_interval_secs,
            max_queue_size: self.config.max_queue_size,
        })
    }
}

// ============================================================================
// 错误处理
// ============================================================================

/// 错误类别
#[napi]
#[derive(Debug, PartialEq, Eq)]
pub enum ErrorCategory {
    /// 连接错误
    Connection,
    /// 查询错误
    Query,
    /// 超时错误
    Timeout,
    /// 约束违反
    ConstraintViolation,
    /// 权限错误
    Permission,
    /// 未知错误
    Unknown,
}

impl ErrorCategory {
    /// 从错误消息推断类别
    pub fn infer(message: &str) -> Self {
        let lower = message.to_lowercase();
        if lower.contains("connection") || lower.contains("connect") {
            Self::Connection
        } else if lower.contains("timeout") || lower.contains("timed out") {
            Self::Timeout
        } else if lower.contains("constraint")
            || lower.contains("unique")
            || lower.contains("foreign key")
        {
            Self::ConstraintViolation
        } else if lower.contains("permission") || lower.contains("access denied") {
            Self::Permission
        } else if lower.contains("syntax") || lower.contains("query") {
            Self::Query
        } else {
            Self::Unknown
        }
    }

    /// 是否可重试
    pub fn is_retryable(&self) -> bool {
        matches!(self, ErrorCategory::Connection | ErrorCategory::Timeout)
    }

    /// 类别名
    pub fn as_str(&self) -> &'static str {
        match self {
            ErrorCategory::Connection => "connection",
            ErrorCategory::Query => "query",
            ErrorCategory::Timeout => "timeout",
            ErrorCategory::ConstraintViolation => "constraint_violation",
            ErrorCategory::Permission => "permission",
            ErrorCategory::Unknown => "unknown",
        }
    }
}

/// 重试策略
#[napi]
#[allow(clippy::new_without_default)]
pub struct RetryPolicy {
    max_retries: u32,
    base_delay_ms: i64,
    max_delay_ms: i64,
    retryable_categories: Vec<ErrorCategory>,
}

#[napi]
#[allow(clippy::new_without_default)]
impl RetryPolicy {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            max_retries: 3,
            base_delay_ms: 100,
            max_delay_ms: 5000,
            retryable_categories: vec![ErrorCategory::Connection, ErrorCategory::Timeout],
        }
    }

    /// 设置最大重试次数
    #[napi]
    pub fn set_max_retries(&mut self, n: u32) {
        self.max_retries = n;
    }

    /// 设置基础延迟（毫秒）
    #[napi]
    pub fn set_base_delay(&mut self, ms: i64) {
        self.base_delay_ms = ms;
    }

    /// 设置最大延迟（毫秒）
    #[napi]
    pub fn set_max_delay(&mut self, ms: i64) {
        self.max_delay_ms = ms;
    }

    /// 最大重试次数
    #[napi(getter)]
    pub fn max_retries(&self) -> u32 {
        self.max_retries
    }

    /// 计算第 n 次重试的延迟（指数退避：base * 2^n，上限 max_delay）
    #[napi]
    pub fn delay_for_retry(&self, retry: u32) -> i64 {
        let delay = self
            .base_delay_ms
            .saturating_mul(2i64.saturating_pow(retry));
        delay.min(self.max_delay_ms)
    }

    /// 判断错误类别是否可重试
    #[napi]
    pub fn should_retry(&self, category: ErrorCategory) -> bool {
        self.retryable_categories.contains(&category)
    }

    /// 是否还有重试机会
    #[napi]
    pub fn can_retry(&self, current_retry: u32) -> bool {
        current_retry < self.max_retries
    }
}

/// 错误处理器：分类错误、决定重试策略
#[napi]
#[allow(clippy::new_without_default)]
pub struct ErrorHandler {
    policy: RetryPolicy,
}

#[napi]
#[allow(clippy::new_without_default)]
impl ErrorHandler {
    /// 创建错误处理器
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            policy: RetryPolicy::new(),
        }
    }

    /// 设置重试策略
    pub fn set_retry_policy(&mut self, policy: RetryPolicy) {
        self.policy = policy;
    }

    /// 分类错误消息
    #[napi]
    pub fn categorize(&self, message: String) -> ErrorCategory {
        ErrorCategory::infer(&message)
    }

    /// 判断错误是否可重试
    #[napi]
    pub fn is_retryable(&self, message: String) -> bool {
        let category = self.categorize(message);
        self.policy.should_retry(category)
    }

    /// 获取重试延迟
    #[napi]
    pub fn retry_delay(&self, retry_count: u32) -> i64 {
        self.policy.delay_for_retry(retry_count)
    }

    /// 是否应该继续重试
    #[napi]
    pub fn should_continue(&self, retry_count: u32, message: String) -> bool {
        self.policy.can_retry(retry_count) && self.is_retryable(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ----- JoinType -----

    #[test]
    fn join_type_keyword() {
        assert_eq!(JoinType::Inner.keyword(), "INNER JOIN");
        assert_eq!(JoinType::Left.keyword(), "LEFT JOIN");
        assert_eq!(JoinType::Right.keyword(), "RIGHT JOIN");
        assert_eq!(JoinType::Full.keyword(), "FULL OUTER JOIN");
    }

    // ----- JoinClause -----

    #[test]
    fn join_clause_to_sql() {
        let join = JoinClause::new(
            JoinType::Inner,
            "posts".to_string(),
            "posts.user_id = users.id".to_string(),
        );
        let dialect = sz_orm_core::dialect::get_dialect(DbType::MySQL).unwrap();
        let sql = join.to_sql(&*dialect);
        assert!(sql.contains("INNER JOIN"));
        assert!(sql.contains("posts"));
        assert!(sql.contains("ON"));
    }

    #[test]
    fn join_clause_with_alias() {
        let mut join = JoinClause::new(
            JoinType::Left,
            "posts".to_string(),
            "p.user_id = u.id".to_string(),
        );
        join.set_alias("p".to_string());
        let dialect = sz_orm_core::dialect::get_dialect(DbType::MySQL).unwrap();
        let sql = join.to_sql(&*dialect);
        assert!(sql.contains("AS p"));
    }

    // ----- QueryBuilderEnhanced -----

    #[test]
    fn query_enhanced_basic_select() {
        let mut q = QueryBuilderEnhanced::new(None).unwrap();
        q.set_table("users".to_string());
        let result = q.build().unwrap();
        assert!(result.sql.contains("SELECT *"));
        assert!(result.sql.contains("FROM"));
    }

    #[test]
    fn query_enhanced_with_columns() {
        let mut q = QueryBuilderEnhanced::new(None).unwrap();
        q.set_table("users".to_string());
        q.set_select(vec!["id".to_string(), "name".to_string()]);
        let result = q.build().unwrap();
        assert!(result.sql.contains("id, name"));
    }

    #[test]
    fn query_enhanced_distinct() {
        let mut q = QueryBuilderEnhanced::new(None).unwrap();
        q.set_table("users".to_string());
        q.set_distinct();
        let result = q.build().unwrap();
        assert!(result.sql.contains("DISTINCT"));
    }

    #[test]
    fn query_enhanced_with_join() {
        let mut q = QueryBuilderEnhanced::new(None).unwrap();
        q.set_table("users".to_string());
        q.add_join(JoinClause::new(
            JoinType::Inner,
            "posts".to_string(),
            "posts.user_id = users.id".to_string(),
        ));
        let result = q.build().unwrap();
        assert!(result.sql.contains("INNER JOIN"));
    }

    #[test]
    fn query_enhanced_with_where() {
        let mut q = QueryBuilderEnhanced::new(None).unwrap();
        q.set_table("users".to_string());
        q.add_where("age > ?".to_string());
        q.add_where("active = ?".to_string());
        let result = q.build().unwrap();
        assert!(result.sql.contains("WHERE"));
        assert!(result.sql.contains("AND"));
        assert_eq!(result.param_count, 2);
    }

    #[test]
    fn query_enhanced_with_group_by() {
        let mut q = QueryBuilderEnhanced::new(None).unwrap();
        q.set_table("users".to_string());
        q.add_group_by("department".to_string());
        let result = q.build().unwrap();
        assert!(result.sql.contains("GROUP BY"));
    }

    #[test]
    fn query_enhanced_with_having() {
        let mut q = QueryBuilderEnhanced::new(None).unwrap();
        q.set_table("users".to_string());
        q.add_group_by("department".to_string());
        q.add_having("COUNT(*) > ?".to_string());
        let result = q.build().unwrap();
        assert!(result.sql.contains("HAVING"));
    }

    #[test]
    fn query_enhanced_with_order() {
        let mut q = QueryBuilderEnhanced::new(None).unwrap();
        q.set_table("users".to_string());
        q.add_order_by("name".to_string());
        let result = q.build().unwrap();
        assert!(result.sql.contains("ORDER BY"));
    }

    #[test]
    fn query_enhanced_with_order_desc() {
        let mut q = QueryBuilderEnhanced::new(None).unwrap();
        q.set_table("users".to_string());
        q.add_order_desc("created_at".to_string());
        let result = q.build().unwrap();
        assert!(result.sql.contains("DESC"));
    }

    #[test]
    fn query_enhanced_with_limit_offset() {
        let mut q = QueryBuilderEnhanced::new(None).unwrap();
        q.set_table("users".to_string());
        q.set_limit(10);
        q.set_offset(20);
        let result = q.build().unwrap();
        assert!(result.sql.contains("LIMIT 10"));
        assert!(result.sql.contains("OFFSET 20"));
    }

    #[test]
    fn query_enhanced_with_alias() {
        let mut q = QueryBuilderEnhanced::new(None).unwrap();
        q.set_table("users".to_string());
        q.set_alias("u".to_string());
        let result = q.build().unwrap();
        assert!(result.sql.contains("AS u"));
    }

    #[test]
    fn query_enhanced_no_table_error() {
        let q = QueryBuilderEnhanced::new(None).unwrap();
        assert!(q.build().is_err());
    }

    #[test]
    fn query_enhanced_build_count() {
        let mut q = QueryBuilderEnhanced::new(None).unwrap();
        q.set_table("users".to_string());
        q.add_where("active = ?".to_string());
        let result = q.build_count().unwrap();
        assert!(result.sql.contains("COUNT(*)"));
    }

    #[test]
    fn query_enhanced_postgres_quoting() {
        let mut q = QueryBuilderEnhanced::new(Some("postgres".to_string())).unwrap();
        q.set_table("users".to_string());
        let result = q.build().unwrap();
        assert!(result.sql.contains("\"users\""));
    }

    #[test]
    fn query_enhanced_full_query() {
        let mut q = QueryBuilderEnhanced::new(None).unwrap();
        q.set_table("users".to_string());
        q.set_alias("u".to_string());
        q.set_select(vec!["u.id".to_string(), "p.title".to_string()]);
        q.add_join(JoinClause::new(
            JoinType::Left,
            "posts".to_string(),
            "p.user_id = u.id".to_string(),
        ));
        q.add_where("u.active = ?".to_string());
        q.add_group_by("u.id".to_string());
        q.add_having("COUNT(p.id) > ?".to_string());
        q.add_order_desc("u.id".to_string());
        q.set_limit(10);
        let result = q.build().unwrap();
        assert!(result.sql.contains("LEFT JOIN"));
        assert!(result.sql.contains("GROUP BY"));
        assert!(result.sql.contains("HAVING"));
        assert!(result.sql.contains("ORDER BY"));
        assert!(result.sql.contains("LIMIT"));
    }

    // ----- PoolConfig -----

    #[test]
    fn pool_config_defaults() {
        let config = PoolConfig::defaults();
        assert!(config.is_valid());
    }

    #[test]
    fn pool_config_validate_max_size_zero() {
        let config = PoolConfig {
            max_size: 0,
            ..PoolConfig::defaults()
        };
        assert!(!config.is_valid());
    }

    #[test]
    fn pool_config_validate_min_idle_exceeds_max() {
        let config = PoolConfig {
            max_size: 10,
            min_idle: 20,
            ..PoolConfig::defaults()
        };
        assert!(!config.is_valid());
    }

    #[test]
    fn pool_config_validate_zero_timeout() {
        let config = PoolConfig {
            acquire_timeout_secs: 0,
            ..PoolConfig::defaults()
        };
        assert!(!config.is_valid());
    }

    #[test]
    fn pool_config_validate_zero_queue() {
        let config = PoolConfig {
            max_queue_size: 0,
            ..PoolConfig::defaults()
        };
        assert!(!config.is_valid());
    }

    // ----- PoolConfigBuilder -----

    #[test]
    fn pool_config_builder_default() {
        let builder = PoolConfigBuilder::new();
        let config = builder.build().unwrap();
        assert_eq!(config.max_size, 100);
    }

    #[test]
    fn pool_config_builder_custom() {
        let mut builder = PoolConfigBuilder::new();
        builder.set_max_size(50);
        builder.set_min_idle(5);
        builder.set_acquire_timeout(60);
        let config = builder.build().unwrap();
        assert_eq!(config.max_size, 50);
        assert_eq!(config.min_idle, 5);
        assert_eq!(config.acquire_timeout_secs, 60);
    }

    #[test]
    fn pool_config_builder_invalid() {
        let mut builder = PoolConfigBuilder::new();
        builder.set_max_size(0);
        assert!(builder.build().is_err());
    }

    // ----- ErrorCategory -----

    #[test]
    fn error_category_infer_connection() {
        assert_eq!(
            ErrorCategory::infer("Connection refused"),
            ErrorCategory::Connection
        );
    }

    #[test]
    fn error_category_infer_timeout() {
        assert_eq!(
            ErrorCategory::infer("Operation timed out"),
            ErrorCategory::Timeout
        );
    }

    #[test]
    fn error_category_infer_constraint() {
        assert_eq!(
            ErrorCategory::infer("UNIQUE constraint violated"),
            ErrorCategory::ConstraintViolation
        );
    }

    #[test]
    fn error_category_infer_permission() {
        assert_eq!(
            ErrorCategory::infer("Access denied for user"),
            ErrorCategory::Permission
        );
    }

    #[test]
    fn error_category_infer_query() {
        assert_eq!(
            ErrorCategory::infer("SQL syntax error"),
            ErrorCategory::Query
        );
    }

    #[test]
    fn error_category_infer_unknown() {
        assert_eq!(
            ErrorCategory::infer("something weird"),
            ErrorCategory::Unknown
        );
    }

    #[test]
    fn error_category_is_retryable() {
        assert!(ErrorCategory::Connection.is_retryable());
        assert!(ErrorCategory::Timeout.is_retryable());
        assert!(!ErrorCategory::Query.is_retryable());
        assert!(!ErrorCategory::ConstraintViolation.is_retryable());
    }

    #[test]
    fn error_category_as_str() {
        assert_eq!(ErrorCategory::Connection.as_str(), "connection");
        assert_eq!(ErrorCategory::Timeout.as_str(), "timeout");
    }

    // ----- RetryPolicy -----

    #[test]
    fn retry_policy_default() {
        let policy = RetryPolicy::new();
        assert_eq!(policy.max_retries(), 3);
    }

    #[test]
    fn retry_policy_delay_exponential() {
        let policy = RetryPolicy::new();
        assert_eq!(policy.delay_for_retry(0), 100);
        assert_eq!(policy.delay_for_retry(1), 200);
        assert_eq!(policy.delay_for_retry(2), 400);
    }

    #[test]
    fn retry_policy_delay_capped() {
        let mut policy = RetryPolicy::new();
        policy.set_max_delay(500);
        assert_eq!(policy.delay_for_retry(10), 500);
    }

    #[test]
    fn retry_policy_should_retry() {
        let policy = RetryPolicy::new();
        assert!(policy.should_retry(ErrorCategory::Connection));
        assert!(policy.should_retry(ErrorCategory::Timeout));
        assert!(!policy.should_retry(ErrorCategory::Query));
    }

    #[test]
    fn retry_policy_can_retry() {
        let policy = RetryPolicy::new();
        assert!(policy.can_retry(0));
        assert!(policy.can_retry(2));
        assert!(!policy.can_retry(3));
    }

    // ----- ErrorHandler -----

    #[test]
    fn error_handler_categorize() {
        let handler = ErrorHandler::new();
        assert_eq!(
            handler.categorize("Connection refused".to_string()),
            ErrorCategory::Connection
        );
    }

    #[test]
    fn error_handler_is_retryable() {
        let handler = ErrorHandler::new();
        assert!(handler.is_retryable("Connection refused".to_string()));
        assert!(!handler.is_retryable("Syntax error".to_string()));
    }

    #[test]
    fn error_handler_retry_delay() {
        let handler = ErrorHandler::new();
        assert_eq!(handler.retry_delay(0), 100);
        assert_eq!(handler.retry_delay(1), 200);
    }

    #[test]
    fn error_handler_should_continue() {
        let handler = ErrorHandler::new();
        assert!(handler.should_continue(0, "Connection refused".to_string()));
        assert!(!handler.should_continue(3, "Connection refused".to_string()));
        assert!(!handler.should_continue(0, "Syntax error".to_string()));
    }
}
