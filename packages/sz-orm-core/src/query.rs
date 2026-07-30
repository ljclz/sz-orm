//! 查询构造器
//!
//! 提供类似 ThinkORM 的链式查询构造 API
//!
//! # P0-1 软删除集成（v1.3.0+）
//!
//! 当 `M: Model` 实现了 `soft_delete_field()` 返回 `Some(field)` 时，
//! `QueryBuilder` 会在以下场景自动追加 `WHERE {field} IS NULL`：
//! - `build_select` / `build_select_with_params`
//! - `build_count` / `build_exists` / `build_max` / `build_min` / `build_sum` / `build_avg`
//! - `build_update` / `build_update_with_params`（防止更新已删除记录）
//! - `build_delete` / `build_delete_with_params`（自动转为 `UPDATE SET {field} = NOW()`）
//!
//! 使用 `without_soft_delete()` 可临时禁用软删除过滤（用于查询已删除记录）。
//!
//! # P0-2 参数化 WHERE 条件（v1.3.0+）
//!
//! 新增类型安全的参数化 WHERE API：
//! - `where_eq(field, value)` / `where_ne` / `where_gt` / `where_ge` / `where_lt` / `where_le`
//! - `where_like(field, pattern)`
//!
//! 这些方法使用 `?` 占位符 + `Value` 绑定，杜绝 SQL 注入。
//! 原有 `where_cond(condition: impl Into<String>)` 因字符串拼接存在注入风险，
//! 保留以兼容复杂表达式（如 `age > 18 AND status = 'active'`），但文档标记为不推荐。

use crate::dialect::Dialect;
use crate::model::Model;
use crate::value::Value;
use std::fmt;

/// 用于构造 SQL 查询的查询构造器
pub struct QueryBuilder<M: Model> {
    table: Option<String>,
    select_columns: Vec<String>,
    where_conditions: Vec<WhereCondition>,
    order_by: Vec<OrderClause>,
    group_by: Vec<String>,
    having_conditions: Vec<WhereCondition>,
    limit_value: Option<usize>,
    offset_value: Option<usize>,
    joins: Vec<JoinClause>,
    dialect: Box<dyn Dialect>,
    /// P0-1：是否禁用软删除过滤（true 表示禁用，查询包含已删除记录）
    soft_delete_disabled: bool,
    /// P0-3：当前租户 ID（运行时注入）。设置后自动追加 `WHERE {tenant_field} = ?`
    tenant_id_value: Option<i64>,
    /// P0-3：是否禁用租户过滤（true 表示禁用，跨租户查询）
    tenant_disabled: bool,
    /// P2-5：Keyset 分页游标条件（field, value, direction）
    ///
    /// 设置后，`build_select`/`build_select_with_params` 会追加 `WHERE {field} > ?` 或
    /// `WHERE {field} < ?` 条件（取决于排序方向），实现基于游标的高效分页。
    /// 与 OFFSET 分页相比，Keyset 分页在大数据集下性能稳定，不受数据插入/删除影响。
    keyset_cursor: Option<KeysetCursor>,
    #[allow(dead_code)]
    model: std::marker::PhantomData<M>,
}

/// P2-5：Keyset 分页游标
///
/// 表示一个基于排序字段值的分页游标。结合 `ORDER BY {field} {direction}` 和
/// `WHERE {field} {op} ?` 实现游标分页。
///
/// - `After(value)` + `Asc`：查询 `field > value` 的记录（下一页）
/// - `Before(value)` + `Desc`：查询 `field < value` 的记录（上一页）
#[derive(Debug, Clone)]
struct KeysetCursor {
    /// 排序字段名
    field: String,
    /// 游标值（上一页/下一页最后一行的该字段值）
    value: Value,
    /// 游标方向：After = 下一页（field > value），Before = 上一页（field < value）
    direction: KeysetDirection,
}

/// P2-5：Keyset 游标方向
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeysetDirection {
    /// 下一页：`WHERE field > cursor_value`（配合 ASC 排序）
    After,
    /// 上一页：`WHERE field < cursor_value`（配合 DESC 排序）
    Before,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
enum WhereCondition {
    /// 原始字符串条件（AND）— **存在注入风险，不推荐使用**
    ///
    /// 保留以兼容复杂表达式如 `age > 18 AND status = 'active'`。
    /// 调用方必须确保字符串来自可信来源。
    And(String),
    /// 原始字符串条件（OR）— **存在注入风险，不推荐使用**
    Or(String),
    /// P0-2：参数化等值条件 `field = ?`
    Eq(String, Value),
    /// P0-2：参数化不等条件 `field != ?`
    Ne(String, Value),
    /// P0-2：参数化大于条件 `field > ?`
    Gt(String, Value),
    /// P0-2：参数化大于等于条件 `field >= ?`
    Ge(String, Value),
    /// P0-2：参数化小于条件 `field < ?`
    Lt(String, Value),
    /// P0-2：参数化小于等于条件 `field <= ?`
    Le(String, Value),
    /// P0-2：参数化 LIKE 条件 `field LIKE ?`
    Like(String, Value),
    /// P0-2：参数化 OR 等值条件 `OR field = ?`
    OrEq(String, Value),
    /// P0-2：参数化 OR 不等条件 `OR field != ?`
    OrNe(String, Value),
    /// P0-2：参数化 OR 大于条件 `OR field > ?`
    OrGt(String, Value),
    /// P0-2：参数化 OR 大于等于条件 `OR field >= ?`
    OrGe(String, Value),
    /// P0-2：参数化 OR 小于条件 `OR field < ?`
    OrLt(String, Value),
    /// P0-2：参数化 OR 小于等于条件 `OR field <= ?`
    OrLe(String, Value),
    /// P0-2：参数化 OR LIKE 条件 `OR field LIKE ?`
    OrLike(String, Value),
    In(String, Vec<Value>),
    NotIn(String, Vec<Value>),
    Between(String, Value, Value),
    NotBetween(String, Value, Value),
    Null(String),
    NotNull(String),
    Exists(String),
    NotExists(String),
}

#[derive(Debug, Clone)]
struct OrderClause {
    field: String,
    direction: OrderDirection,
}

#[derive(Debug, Clone)]
enum OrderDirection {
    Asc,
    Desc,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
enum JoinClause {
    Inner(String, String, String),
    Left(String, String, String),
    Right(String, String, String),
    Cross(String, String),
}

impl<M: Model> QueryBuilder<M> {
    pub fn new(dialect: Box<dyn Dialect>) -> Self {
        Self {
            table: None,
            select_columns: vec!["*".to_string()],
            where_conditions: Vec::new(),
            order_by: Vec::new(),
            group_by: Vec::new(),
            having_conditions: Vec::new(),
            limit_value: None,
            offset_value: None,
            joins: Vec::new(),
            dialect,
            soft_delete_disabled: false,
            tenant_id_value: None,
            tenant_disabled: false,
            keyset_cursor: None,
            model: std::marker::PhantomData,
        }
    }

    pub fn table(mut self, table: impl Into<String>) -> Self {
        self.table = Some(table.into());
        self
    }

    /// P0-1：临时禁用软删除过滤，用于查询已删除的记录。
    ///
    /// 等价于 SeaORM 的 `Entity::find().filter(Column::DeletedAt.is_not_null())`
    /// 或 Laravel Eloquent 的 `Model::withTrashed()`。
    ///
    /// # 示例
    ///
    /// ```ignore
    /// use sz_orm_core::query::QueryBuilder;
    /// use sz_orm_core::dialect::MySqlDialect;
    ///
    /// // 查询包含已软删除的用户
    /// let sql = QueryBuilder::<User>::new(Box::new(MySqlDialect))
    ///     .table("users")
    ///     .without_soft_delete()
    ///     .build_select();
    /// // 不会自动追加 WHERE deleted_at IS NULL
    /// ```
    pub fn without_soft_delete(mut self) -> Self {
        self.soft_delete_disabled = true;
        self
    }

    /// P0-1：返回软删除过滤是否被禁用
    pub fn is_soft_delete_disabled(&self) -> bool {
        self.soft_delete_disabled
    }

    /// P0-1：返回当前 Model 的软删除字段名（若启用）
    ///
    /// 内部使用，用于 `build_*` 方法决定是否追加 `WHERE {field} IS NULL`。
    fn soft_delete_field(&self) -> Option<&'static str> {
        if self.soft_delete_disabled {
            return None;
        }
        M::soft_delete_field()
    }

    /// P0-1：构造软删除过滤条件 SQL 片段（不含 `AND` 前缀）
    ///
    /// 返回 `None` 表示无需过滤；返回 `Some(sql)` 表示追加 `AND {sql}` 到 WHERE 子句。
    fn build_soft_delete_condition(&self) -> Option<String> {
        self.soft_delete_field()
            .map(|field| format!("{} IS NULL", self.dialect.quote(field)))
    }

    // ===================== P0-3 多租户过滤 =====================

    /// P0-3：设置当前租户 ID，启用多租户自动过滤。
    ///
    /// 当 `M::tenant_field()` 返回 `Some(field)` 时，`QueryBuilder` 会在以下场景
    /// 自动追加 `WHERE {field} = ?`（参数化，值通过 `params` 绑定）：
    /// - `build_select` / `build_select_with_params`
    /// - `build_count` / `build_exists` / `build_max` / `build_min` / `build_sum` / `build_avg`
    /// - `build_update` / `build_update_with_params`（防止跨租户更新）
    /// - `build_delete` / `build_delete_with_params`（防止跨租户删除）
    ///
    /// 使用 `without_tenant()` 可临时禁用租户过滤（用于跨租户管理查询）。
    ///
    /// # 示例
    ///
    /// ```ignore
    /// use sz_orm_core::query::QueryBuilder;
    /// use sz_orm_core::dialect::MySqlDialect;
    ///
    /// let (sql, params) = QueryBuilder::<Order>::new(Box::new(MySqlDialect))
    ///     .table("orders")
    ///     .with_tenant_id(42)
    ///     .build_select_with_params();
    /// // sql => "SELECT * FROM `orders` WHERE `tenant_id` = ?"
    /// // params => [Value::I64(42)]
    /// ```
    pub fn with_tenant_id(mut self, tenant_id: i64) -> Self {
        self.tenant_id_value = Some(tenant_id);
        self
    }

    /// P0-3：临时禁用租户过滤，用于跨租户管理查询。
    ///
    /// 等价于 Laravel Eloquent 的全局作用域禁用。
    pub fn without_tenant(mut self) -> Self {
        self.tenant_disabled = true;
        self
    }

    /// P0-3：返回租户过滤是否被禁用
    pub fn is_tenant_disabled(&self) -> bool {
        self.tenant_disabled
    }

    /// P0-3：返回当前 Model 的租户字段名（若启用且未禁用）
    ///
    /// 内部使用，用于 `build_*` 方法决定是否追加租户条件。
    fn tenant_field(&self) -> Option<&'static str> {
        if self.tenant_disabled {
            return None;
        }
        M::tenant_field()
    }

    /// P0-3：返回当前租户 ID（若设置了且未禁用）
    fn tenant_id_value(&self) -> Option<i64> {
        if self.tenant_disabled {
            return None;
        }
        self.tenant_id_value
    }

    /// P0-3：构造租户过滤条件（SQL 片段 + 参数值）
    ///
    /// 返回 `None` 表示无需过滤；返回 `Some((sql, value))` 表示追加 `AND {sql}` 到 WHERE 子句，
    /// 并将 `value` 加入参数列表。
    fn build_tenant_condition(&self) -> Option<(String, Value)> {
        let field = self.tenant_field()?;
        let tid = self.tenant_id_value()?;
        Some((
            format!("{} = ?", self.dialect.quote(field)),
            Value::I64(tid),
        ))
    }

    /// 设置 SELECT 列。
    ///
    /// **M-3 安全警告**：本方法直接拼接 `columns` 到 SQL，**不**进行标识符校验或 quote。
    /// 调用方必须确保 `columns` 来自可信来源（硬编码或经 `sql_safety::validate_identifier`
    /// 校验）。若列名可能来自不可信输入，请使用 [`QueryBuilder::select_quoted`]。
    ///
    /// 本方法保留原行为以兼容复杂表达式（如 `COUNT(*)`、`users.id AS uid`）。
    pub fn select(mut self, columns: Vec<&str>) -> Self {
        self.select_columns = columns.into_iter().map(|s| s.to_string()).collect();
        self
    }

    /// M-3 修复：安全的 SELECT 列设置，自动校验每个列名并 quote。
    ///
    /// 每个 `column` 必须通过 `sql_safety::validate_identifier` 校验
    /// （仅允许 ASCII 字母数字 + 下划线，不以数字开头，长度 1-63）。
    /// 校验失败时返回 `DbError::InvalidInput`。
    ///
    /// 对于复杂表达式（如 `COUNT(*)`、`users.id AS uid`），请使用 [`QueryBuilder::select`]
    /// 并自行确保安全。
    pub fn select_quoted(mut self, columns: Vec<&str>) -> Result<Self, crate::DbError> {
        let mut quoted = Vec::with_capacity(columns.len());
        for col in columns {
            crate::sql_safety::validate_identifier(col, "select column")?;
            quoted.push(self.dialect.quote(col));
        }
        self.select_columns = quoted;
        Ok(self)
    }

    /// 添加原始字符串 WHERE 条件（AND 关系）。
    ///
    /// **⚠️ P0-2 安全警告（v1.3.0+）**：本方法直接拼接 `condition` 到 SQL，
    /// **存在 SQL 注入风险**。仅在以下场景使用：
    /// - 条件来自硬编码字符串（如 `where_cond("age > 18")`）
    /// - 条件含复杂表达式（如 `where_cond("status = 'active' AND role = 'admin'")`）
    ///
    /// **禁止**将用户输入拼接到 `condition` 中。若值来自不可信来源，
    /// 必须使用参数化方法：[`where_eq`](Self::where_eq) / [`where_ne`](Self::where_ne) /
    /// [`where_gt`](Self::where_gt) / [`where_lt`](Self::where_lt) / [`where_like`](Self::where_like)。
    ///
    /// # 推荐迁移
    ///
    /// ```ignore
    /// // ❌ 危险：字符串拼接
    /// builder.where_cond(format!("name = '{}'", user_input));
    ///
    /// // ✅ 安全：参数化绑定
    /// builder.where_eq("name", Value::String(user_input.to_string()));
    /// ```
    #[deprecated(
        since = "1.3.0",
        note = "P0-2: 字符串拼接存在 SQL 注入风险，请使用 where_eq/where_ne/where_gt/where_lt/where_like 等参数化方法"
    )]
    pub fn where_cond(mut self, condition: impl Into<String>) -> Self {
        self.where_conditions
            .push(WhereCondition::And(condition.into()));
        self
    }

    /// 添加原始字符串 WHERE 条件（OR 关系）。
    ///
    /// **⚠️ P0-2 安全警告**：同 [`where_cond`](Self::where_cond)，存在注入风险。
    #[deprecated(
        since = "1.3.0",
        note = "P0-2: 字符串拼接存在 SQL 注入风险，请使用参数化方法"
    )]
    pub fn or_where(mut self, condition: impl Into<String>) -> Self {
        self.where_conditions
            .push(WhereCondition::Or(condition.into()));
        self
    }

    /// P0-2：参数化等值条件 `field = ?`（AND 关系）。
    ///
    /// 值通过 `?` 占位符绑定，杜绝 SQL 注入。
    ///
    /// # 示例
    ///
    /// ```ignore
    /// use sz_orm_core::Value;
    ///
    /// builder
    ///     .where_eq("status", Value::String("active".into()))
    ///     .where_eq("tenant_id", Value::I64(42));
    /// ```
    pub fn where_eq(mut self, field: impl Into<String>, value: Value) -> Self {
        self.where_conditions
            .push(WhereCondition::Eq(field.into(), value));
        self
    }

    /// P0-2：参数化不等条件 `field != ?`（AND 关系）。
    pub fn where_ne(mut self, field: impl Into<String>, value: Value) -> Self {
        self.where_conditions
            .push(WhereCondition::Ne(field.into(), value));
        self
    }

    /// P0-2：参数化大于条件 `field > ?`（AND 关系）。
    pub fn where_gt(mut self, field: impl Into<String>, value: Value) -> Self {
        self.where_conditions
            .push(WhereCondition::Gt(field.into(), value));
        self
    }

    /// P0-2：参数化大于等于条件 `field >= ?`（AND 关系）。
    pub fn where_ge(mut self, field: impl Into<String>, value: Value) -> Self {
        self.where_conditions
            .push(WhereCondition::Ge(field.into(), value));
        self
    }

    /// P0-2：参数化小于条件 `field < ?`（AND 关系）。
    pub fn where_lt(mut self, field: impl Into<String>, value: Value) -> Self {
        self.where_conditions
            .push(WhereCondition::Lt(field.into(), value));
        self
    }

    /// P0-2：参数化小于等于条件 `field <= ?`（AND 关系）。
    pub fn where_le(mut self, field: impl Into<String>, value: Value) -> Self {
        self.where_conditions
            .push(WhereCondition::Le(field.into(), value));
        self
    }

    /// P0-2：参数化 LIKE 条件 `field LIKE ?`（AND 关系）。
    ///
    /// 调用方负责在 `pattern` 中包含 `%` 通配符。
    ///
    /// # 示例
    ///
    /// ```ignore
    /// use sz_orm_core::Value;
    ///
    /// builder.where_like("name", Value::String("%alice%".into()));
    /// ```
    pub fn where_like(mut self, field: impl Into<String>, pattern: Value) -> Self {
        self.where_conditions
            .push(WhereCondition::Like(field.into(), pattern));
        self
    }

    /// P0-2：参数化 OR 等值条件 `OR field = ?`。
    ///
    /// 值通过 `?` 占位符绑定，杜绝 SQL 注入。OR 条件会与相邻的 OR 条件组合成 `(cond1 OR cond2)` 形式。
    pub fn or_where_eq(mut self, field: impl Into<String>, value: Value) -> Self {
        self.where_conditions
            .push(WhereCondition::OrEq(field.into(), value));
        self
    }

    /// P0-2：参数化 OR 不等条件 `OR field != ?`。
    pub fn or_where_ne(mut self, field: impl Into<String>, value: Value) -> Self {
        self.where_conditions
            .push(WhereCondition::OrNe(field.into(), value));
        self
    }

    /// P0-2：参数化 OR 大于条件 `OR field > ?`。
    pub fn or_where_gt(mut self, field: impl Into<String>, value: Value) -> Self {
        self.where_conditions
            .push(WhereCondition::OrGt(field.into(), value));
        self
    }

    /// P0-2：参数化 OR 大于等于条件 `OR field >= ?`。
    pub fn or_where_ge(mut self, field: impl Into<String>, value: Value) -> Self {
        self.where_conditions
            .push(WhereCondition::OrGe(field.into(), value));
        self
    }

    /// P0-2：参数化 OR 小于条件 `OR field < ?`。
    pub fn or_where_lt(mut self, field: impl Into<String>, value: Value) -> Self {
        self.where_conditions
            .push(WhereCondition::OrLt(field.into(), value));
        self
    }

    /// P0-2：参数化 OR 小于等于条件 `OR field <= ?`。
    pub fn or_where_le(mut self, field: impl Into<String>, value: Value) -> Self {
        self.where_conditions
            .push(WhereCondition::OrLe(field.into(), value));
        self
    }

    /// P0-2：参数化 OR LIKE 条件 `OR field LIKE ?`。
    pub fn or_where_like(mut self, field: impl Into<String>, pattern: Value) -> Self {
        self.where_conditions
            .push(WhereCondition::OrLike(field.into(), pattern));
        self
    }

    pub fn where_in(mut self, field: impl Into<String>, values: Vec<Value>) -> Self {
        self.where_conditions
            .push(WhereCondition::In(field.into(), values));
        self
    }

    pub fn where_not_in(mut self, field: impl Into<String>, values: Vec<Value>) -> Self {
        self.where_conditions
            .push(WhereCondition::NotIn(field.into(), values));
        self
    }

    pub fn where_between(mut self, field: impl Into<String>, start: Value, end: Value) -> Self {
        self.where_conditions
            .push(WhereCondition::Between(field.into(), start, end));
        self
    }

    pub fn where_not_between(mut self, field: impl Into<String>, start: Value, end: Value) -> Self {
        self.where_conditions
            .push(WhereCondition::NotBetween(field.into(), start, end));
        self
    }

    pub fn where_null(mut self, field: impl Into<String>) -> Self {
        self.where_conditions
            .push(WhereCondition::Null(field.into()));
        self
    }

    pub fn where_not_null(mut self, field: impl Into<String>) -> Self {
        self.where_conditions
            .push(WhereCondition::NotNull(field.into()));
        self
    }

    pub fn order_by(mut self, field: impl Into<String>) -> Self {
        self.order_by.push(OrderClause {
            field: field.into(),
            direction: OrderDirection::Asc,
        });
        self
    }

    pub fn order_desc(mut self, field: impl Into<String>) -> Self {
        self.order_by.push(OrderClause {
            field: field.into(),
            direction: OrderDirection::Desc,
        });
        self
    }

    pub fn group_by(mut self, field: impl Into<String>) -> Self {
        self.group_by.push(field.into());
        self
    }

    pub fn having(mut self, condition: impl Into<String>) -> Self {
        self.having_conditions
            .push(WhereCondition::And(condition.into()));
        self
    }

    pub fn limit(mut self, limit: usize) -> Self {
        self.limit_value = Some(limit);
        self
    }

    pub fn offset(mut self, offset: usize) -> Self {
        self.offset_value = Some(offset);
        self
    }

    pub fn page(mut self, page: usize, page_size: usize) -> Self {
        self.limit_value = Some(page_size);
        self.offset_value = Some((page.saturating_sub(1)) * page_size);
        self
    }

    /// P2-5：基于游标的 Keyset 分页 — 查询指定字段值之后的记录（下一页）
    ///
    /// 生成 `WHERE {field} > ? ORDER BY {field} ASC LIMIT {page_size}`。
    /// 适用于按主键或时间戳递增遍历大表的场景，性能不受数据插入/删除影响。
    ///
    /// # 参数
    ///
    /// - `field`：排序字段（通常是主键或索引列，如 `id`、`created_at`）
    /// - `cursor_value`：当前页最后一条记录的该字段值
    /// - `page_size`：每页大小
    ///
    /// # 示例
    ///
    /// ```
    /// use sz_orm_core::{QueryBuilder, DbType, dialect::get_dialect, Value};
    /// # use sz_orm_core::{Model, ModelExt};
    /// # #[derive(Clone, Debug)]
    /// # struct User { id: i64 }
    /// # impl Model for User {
    /// #     type PrimaryKey = i64;
    /// #     fn table_name() -> &'static str { "users" }
    /// #     fn pk(&self) -> i64 { self.id }
    /// #     fn set_pk(&mut self, pk: i64) { self.id = pk; }
    /// # }
    /// # impl ModelExt for User {
    /// #     fn columns() -> Vec<&'static str> { vec!["id"] }
    /// #     fn fillable() -> Vec<&'static str> { vec![] }
    /// #     fn guarded() -> Vec<&'static str> { vec!["id"] }
    /// #     fn hidden() -> Vec<&'static str> { vec![] }
    /// #     fn relations() -> std::collections::HashMap<&'static str, sz_orm_core::Relation> { Default::default() }
    /// #     fn fill(&mut self, _: std::collections::HashMap<String, Value>) {}
    /// #     fn to_json(&self) -> serde_json::Value { serde_json::json!({}) }
    /// # }
    /// let dialect = get_dialect(DbType::MySQL).unwrap();
    /// let builder = QueryBuilder::<User>::new(dialect)
    ///     .keyset_after("id", Value::I64(100), 20);
    /// let (sql, params) = builder.build_select_with_params();
    /// // 方言会引用字段名（如 MySQL 的 `id`），去除引号后检查
    /// let sql_clean = sql.replace('`', "").replace('"', "");
    /// assert!(sql_clean.contains("id > ?"));
    /// assert!(sql.to_uppercase().contains("ORDER BY"));
    /// assert!(sql.to_uppercase().contains("ASC"));
    /// assert!(sql.contains("LIMIT 20"));
    /// assert_eq!(params, vec![Value::I64(100)]);
    /// ```
    pub fn keyset_after(
        mut self,
        field: impl Into<String>,
        cursor_value: Value,
        page_size: usize,
    ) -> Self {
        let field_str = field.into();
        // 自动设置 ORDER BY ASC（若字段已存在则更新方向为 ASC，保证 keyset 语义一致）
        if let Some(existing) = self.order_by.iter_mut().find(|o| o.field == field_str) {
            existing.direction = OrderDirection::Asc;
        } else {
            self.order_by.push(OrderClause {
                field: field_str.clone(),
                direction: OrderDirection::Asc,
            });
        }
        self.limit_value = Some(page_size);
        // 清除 offset（keyset 与 offset 互斥）
        self.offset_value = None;
        self.keyset_cursor = Some(KeysetCursor {
            field: field_str,
            value: cursor_value,
            direction: KeysetDirection::After,
        });
        self
    }

    /// P2-5：基于游标的 Keyset 分页 — 查询指定字段值之前的记录（上一页）
    ///
    /// 生成 `WHERE {field} < ? ORDER BY {field} DESC LIMIT {page_size}`。
    /// 适用于反向遍历场景。
    ///
    /// # 参数
    ///
    /// - `field`：排序字段
    /// - `cursor_value`：当前页第一条记录的该字段值
    /// - `page_size`：每页大小
    ///
    /// # 示例
    ///
    /// ```
    /// use sz_orm_core::{QueryBuilder, DbType, dialect::get_dialect, Value};
    /// # use sz_orm_core::{Model, ModelExt};
    /// # #[derive(Clone, Debug)]
    /// # struct User { id: i64 }
    /// # impl Model for User {
    /// #     type PrimaryKey = i64;
    /// #     fn table_name() -> &'static str { "users" }
    /// #     fn pk(&self) -> i64 { self.id }
    /// #     fn set_pk(&mut self, pk: i64) { self.id = pk; }
    /// # }
    /// # impl ModelExt for User {
    /// #     fn columns() -> Vec<&'static str> { vec!["id"] }
    /// #     fn fillable() -> Vec<&'static str> { vec![] }
    /// #     fn guarded() -> Vec<&'static str> { vec!["id"] }
    /// #     fn hidden() -> Vec<&'static str> { vec![] }
    /// #     fn relations() -> std::collections::HashMap<&'static str, sz_orm_core::Relation> { Default::default() }
    /// #     fn fill(&mut self, _: std::collections::HashMap<String, Value>) {}
    /// #     fn to_json(&self) -> serde_json::Value { serde_json::json!({}) }
    /// # }
    /// let dialect = get_dialect(DbType::MySQL).unwrap();
    /// let builder = QueryBuilder::<User>::new(dialect)
    ///     .keyset_before("id", Value::I64(100), 20);
    /// let (sql, params) = builder.build_select_with_params();
    /// // 方言会引用字段名（如 MySQL 的 `id`），去除引号后检查
    /// let sql_clean = sql.replace('`', "").replace('"', "");
    /// assert!(sql_clean.contains("id < ?"));
    /// assert!(sql.to_uppercase().contains("ORDER BY"));
    /// assert!(sql.to_uppercase().contains("DESC"));
    /// assert!(sql.contains("LIMIT 20"));
    /// assert_eq!(params, vec![Value::I64(100)]);
    /// ```
    pub fn keyset_before(
        mut self,
        field: impl Into<String>,
        cursor_value: Value,
        page_size: usize,
    ) -> Self {
        let field_str = field.into();
        // 自动设置 ORDER BY DESC（若字段已存在则更新方向为 DESC，保证 keyset 语义一致）
        if let Some(existing) = self.order_by.iter_mut().find(|o| o.field == field_str) {
            existing.direction = OrderDirection::Desc;
        } else {
            self.order_by.push(OrderClause {
                field: field_str.clone(),
                direction: OrderDirection::Desc,
            });
        }
        self.limit_value = Some(page_size);
        self.offset_value = None;
        self.keyset_cursor = Some(KeysetCursor {
            field: field_str,
            value: cursor_value,
            direction: KeysetDirection::Before,
        });
        self
    }

    pub fn join_inner(
        mut self,
        table: impl Into<String>,
        on_left: impl Into<String>,
        on_right: impl Into<String>,
    ) -> Self {
        self.joins.push(JoinClause::Inner(
            table.into(),
            on_left.into(),
            on_right.into(),
        ));
        self
    }

    pub fn join_left(
        mut self,
        table: impl Into<String>,
        on_left: impl Into<String>,
        on_right: impl Into<String>,
    ) -> Self {
        self.joins.push(JoinClause::Left(
            table.into(),
            on_left.into(),
            on_right.into(),
        ));
        self
    }

    pub fn join_right(
        mut self,
        table: impl Into<String>,
        on_left: impl Into<String>,
        on_right: impl Into<String>,
    ) -> Self {
        self.joins.push(JoinClause::Right(
            table.into(),
            on_left.into(),
            on_right.into(),
        ));
        self
    }

    /// 构建 SELECT SQL 语句
    ///
    /// L-5 修复：补充示例文档
    ///
    /// 根据 `table`、`select_columns`、`where_conditions`、`joins`、`order_by`、
    /// `group_by`、`having`、`limit`、`offset` 等条件拼装最终 SQL。
    /// 若未通过 `table()` 指定表名，则使用 `M::table_name()`。
    ///
    /// # 示例
    ///
    /// ```ignore
    /// use sz_orm_core::query::QueryBuilder;
    /// use sz_orm_core::dialect::MySqlDialect;
    /// use sz_orm_core::model::Model;
    ///
    /// #[derive(Default)]
    /// struct User;
    /// impl Model for User {
    ///     type PrimaryKey = i64;
    ///     fn table_name() -> &'static str { "users" }
    ///     fn pk(&self) -> Self::PrimaryKey { 0 }
    ///     fn set_pk(&mut self, _: Self::PrimaryKey) {}
    /// }
    ///
    /// let sql = QueryBuilder::<User>::new(Box::new(MySqlDialect))
    ///     .select(vec!["id", "name"])
    ///     .where_cond("age > 18")
    ///     .order_by("id DESC")
    ///     .limit(10)
    ///     .build_select();
    /// // sql => "SELECT id, name FROM `users` WHERE age > 18 ORDER BY id DESC LIMIT 10"
    /// ```
    #[tracing::instrument(skip(self), fields(op = "select"))]
    pub fn build_select(&self) -> String {
        let table = self
            .table
            .clone()
            .unwrap_or_else(|| M::table_name().to_string());

        let columns = if self.select_columns.is_empty() {
            "*".to_string()
        } else {
            self.select_columns.join(", ")
        };

        let mut sql = format!("SELECT {} FROM {}", columns, self.dialect.quote(&table));

        for join in &self.joins {
            match join {
                JoinClause::Inner(t, l, r) => {
                    sql.push_str(&format!(
                        " INNER JOIN {} ON {} = {}",
                        self.dialect.quote(t),
                        self.dialect.quote(l),
                        self.dialect.quote(r)
                    ));
                }
                JoinClause::Left(t, l, r) => {
                    sql.push_str(&format!(
                        " LEFT JOIN {} ON {} = {}",
                        self.dialect.quote(t),
                        self.dialect.quote(l),
                        self.dialect.quote(r)
                    ));
                }
                JoinClause::Right(t, l, r) => {
                    sql.push_str(&format!(
                        " RIGHT JOIN {} ON {} = {}",
                        self.dialect.quote(t),
                        self.dialect.quote(l),
                        self.dialect.quote(r)
                    ));
                }
                JoinClause::Cross(t, on) => {
                    sql.push_str(&format!(
                        " CROSS JOIN {} ON {}",
                        self.dialect.quote(t),
                        self.dialect.quote(on)
                    ));
                }
            }
        }

        // P0-1：build_where_clause 内部已处理软删除条件，即使 where_conditions 为空也可能返回非空
        let where_clause = self.build_where_clause();
        if !where_clause.is_empty() {
            sql.push_str(&where_clause);
        }

        if !self.group_by.is_empty() {
            let cols: Vec<String> = self
                .group_by
                .iter()
                .map(|c| self.dialect.quote(c))
                .collect();
            sql.push_str(" GROUP BY ");
            sql.push_str(&cols.join(", "));
        }

        if !self.having_conditions.is_empty() {
            sql.push_str(" HAVING ");
            for (i, cond) in self.having_conditions.iter().enumerate() {
                if i > 0 {
                    sql.push_str(" AND ");
                }
                if let WhereCondition::And(c) = cond {
                    sql.push_str(c);
                }
            }
        }

        if !self.order_by.is_empty() {
            let order_cols: Vec<String> = self
                .order_by
                .iter()
                .map(|o| {
                    let dir = match o.direction {
                        OrderDirection::Asc => " ASC",
                        OrderDirection::Desc => " DESC",
                    };
                    format!("{}{}", self.dialect.quote(&o.field), dir)
                })
                .collect();
            sql.push_str(" ORDER BY ");
            sql.push_str(&order_cols.join(", "));
        }

        if let Some(limit) = self.limit_value {
            sql.push_str(&format!(" LIMIT {}", limit));
        }

        if let Some(offset) = self.offset_value {
            sql.push_str(&format!(" OFFSET {}", offset));
        }

        sql
    }

    /// 构建 WHERE 子句（处理所有条件类型：And/Or/In/NotIn/Between/Null/Eq/Ne/Gt/Lt/Like 等）
    ///
    /// P0-1：自动追加软删除过滤条件（`AND {soft_delete_field} IS NULL`）
    ///
    /// 返回空字符串表示无 WHERE 子句
    fn build_where_clause(&self) -> String {
        self.build_where_clause_with_options(true)
    }

    /// 构建 WHERE 子句（可控制是否追加软删除过滤）。
    ///
    /// `include_soft_delete = true`：追加 `AND {soft_delete_field} IS NULL`（默认行为）
    /// `include_soft_delete = false`：不追加软删除过滤（用于 `build_force_delete`）
    ///
    /// P0-3：租户条件总是追加（若启用），不受 `include_soft_delete` 控制。
    /// 物理删除也应受租户隔离约束，跨租户操作需显式 `without_tenant()`。
    fn build_where_clause_with_options(&self, include_soft_delete: bool) -> String {
        // P0-1：构造软删除条件（若有且启用）
        let soft_delete_cond = if include_soft_delete {
            self.build_soft_delete_condition()
        } else {
            None
        };

        // P0-3：构造租户条件（若有且启用）— 无参数版本内嵌转义值
        let tenant_cond = self.build_tenant_condition().map(|(sql, value)| {
            // sql 形如 "`tenant_id` = ?"，将 ? 替换为内嵌值
            sql.replacen('?', &value.to_param_with_dialect(&*self.dialect), 1)
        });

        // 无用户条件且无软删除条件且无租户条件且无 keyset 游标 → 空 WHERE
        if self.where_conditions.is_empty()
            && soft_delete_cond.is_none()
            && tenant_cond.is_none()
            && self.keyset_cursor.is_none()
        {
            return String::new();
        }

        // 将每个条件转换为字符串，OR 条件标记前缀
        let mut conditions: Vec<String> = self
            .where_conditions
            .iter()
            .map(|cond| match cond {
                WhereCondition::And(c) => c.clone(),
                WhereCondition::Or(c) => format!("OR {}", c),
                // P0-2：参数化条件在无参数版本中直接 inline 值（用于 build_select 等无参数绑定场景）
                WhereCondition::Eq(f, v) => format!(
                    "{} = {}",
                    self.dialect.quote(f),
                    v.to_param_with_dialect(&*self.dialect)
                ),
                WhereCondition::Ne(f, v) => format!(
                    "{} != {}",
                    self.dialect.quote(f),
                    v.to_param_with_dialect(&*self.dialect)
                ),
                WhereCondition::Gt(f, v) => format!(
                    "{} > {}",
                    self.dialect.quote(f),
                    v.to_param_with_dialect(&*self.dialect)
                ),
                WhereCondition::Ge(f, v) => format!(
                    "{} >= {}",
                    self.dialect.quote(f),
                    v.to_param_with_dialect(&*self.dialect)
                ),
                WhereCondition::Lt(f, v) => format!(
                    "{} < {}",
                    self.dialect.quote(f),
                    v.to_param_with_dialect(&*self.dialect)
                ),
                WhereCondition::Le(f, v) => format!(
                    "{} <= {}",
                    self.dialect.quote(f),
                    v.to_param_with_dialect(&*self.dialect)
                ),
                WhereCondition::Like(f, v) => format!(
                    "{} LIKE {}",
                    self.dialect.quote(f),
                    v.to_param_with_dialect(&*self.dialect)
                ),
                WhereCondition::OrEq(f, v) => format!(
                    "OR {} = {}",
                    self.dialect.quote(f),
                    v.to_param_with_dialect(&*self.dialect)
                ),
                WhereCondition::OrNe(f, v) => format!(
                    "OR {} != {}",
                    self.dialect.quote(f),
                    v.to_param_with_dialect(&*self.dialect)
                ),
                WhereCondition::OrGt(f, v) => format!(
                    "OR {} > {}",
                    self.dialect.quote(f),
                    v.to_param_with_dialect(&*self.dialect)
                ),
                WhereCondition::OrGe(f, v) => format!(
                    "OR {} >= {}",
                    self.dialect.quote(f),
                    v.to_param_with_dialect(&*self.dialect)
                ),
                WhereCondition::OrLt(f, v) => format!(
                    "OR {} < {}",
                    self.dialect.quote(f),
                    v.to_param_with_dialect(&*self.dialect)
                ),
                WhereCondition::OrLe(f, v) => format!(
                    "OR {} <= {}",
                    self.dialect.quote(f),
                    v.to_param_with_dialect(&*self.dialect)
                ),
                WhereCondition::OrLike(f, v) => format!(
                    "OR {} LIKE {}",
                    self.dialect.quote(f),
                    v.to_param_with_dialect(&*self.dialect)
                ),
                WhereCondition::In(f, vals) => {
                    // v0.2.2 修复 H-1：使用方言感知的转义
                    let vals_str: Vec<String> = vals
                        .iter()
                        .map(|v| v.to_param_with_dialect(&*self.dialect).to_string())
                        .collect();
                    format!("{} IN ({})", self.dialect.quote(f), vals_str.join(", "))
                }
                WhereCondition::NotIn(f, vals) => {
                    let vals_str: Vec<String> = vals
                        .iter()
                        .map(|v| v.to_param_with_dialect(&*self.dialect).to_string())
                        .collect();
                    format!("{} NOT IN ({})", self.dialect.quote(f), vals_str.join(", "))
                }
                WhereCondition::Between(f, start, end) => {
                    format!(
                        "{} BETWEEN {} AND {}",
                        self.dialect.quote(f),
                        start.to_param_with_dialect(&*self.dialect),
                        end.to_param_with_dialect(&*self.dialect)
                    )
                }
                WhereCondition::NotBetween(f, start, end) => {
                    format!(
                        "{} NOT BETWEEN {} AND {}",
                        self.dialect.quote(f),
                        start.to_param_with_dialect(&*self.dialect),
                        end.to_param_with_dialect(&*self.dialect)
                    )
                }
                WhereCondition::Null(f) => format!("{} IS NULL", self.dialect.quote(f)),
                WhereCondition::NotNull(f) => format!("{} IS NOT NULL", self.dialect.quote(f)),
                WhereCondition::Exists(s) => format!("EXISTS ({})", s),
                WhereCondition::NotExists(s) => format!("NOT EXISTS ({})", s),
            })
            .collect();

        // P0-1：追加软删除条件（作为最后一个 AND 条件）
        if let Some(sd_cond) = soft_delete_cond {
            conditions.push(sd_cond);
        }

        // P0-3：追加租户条件（在软删除之后，作为 AND 条件）
        if let Some(t_cond) = tenant_cond {
            conditions.push(t_cond);
        }

        // P2-5：追加 keyset 游标条件（在租户条件之后，内嵌转义值）
        if let Some(ref cursor) = self.keyset_cursor {
            let op = match cursor.direction {
                KeysetDirection::After => ">",
                KeysetDirection::Before => "<",
            };
            conditions.push(format!(
                "{} {} {}",
                self.dialect.quote(&cursor.field),
                op,
                cursor.value.to_param_with_dialect(&*self.dialect)
            ));
        }

        if conditions.is_empty() {
            return String::new();
        }

        // OR 分组逻辑：将相邻的 OR 条件组合成 (cond1 OR cond2) 形式
        // 边界处理：如果第一个条件就是 OR（不合理但需防御），当作 AND 处理
        let mut groups: Vec<Vec<String>> = Vec::new();
        let mut current_group: Vec<String> = Vec::new();
        for cond in conditions.iter() {
            if let Some(stripped) = cond.strip_prefix("OR ") {
                // OR 条件：无论是否首个，都把 OR 前缀去掉当作普通条件加入当前组
                current_group.push(stripped.to_string());
            } else {
                // AND 条件：如果当前组非空，先保存
                if !current_group.is_empty() {
                    groups.push(std::mem::take(&mut current_group));
                }
                current_group.push(cond.clone());
            }
        }
        if !current_group.is_empty() {
            groups.push(current_group);
        }

        let group_strs: Vec<String> = groups
            .iter()
            .map(|g| {
                if g.len() == 1 {
                    g[0].clone()
                } else {
                    format!("({})", g.join(" OR "))
                }
            })
            .collect();

        format!(" WHERE {}", group_strs.join(" AND "))
    }

    #[tracing::instrument(skip(self, data), fields(op = "insert"))]
    pub fn build_insert(&self, data: &std::collections::HashMap<String, Value>) -> String {
        let table = self
            .table
            .clone()
            .unwrap_or_else(|| M::table_name().to_string());

        if data.is_empty() {
            return String::new();
        }

        let columns: Vec<String> = data.keys().map(|k| self.dialect.quote(k)).collect();
        // v0.2.2 修复 H-1：使用方言感知的转义
        let values: Vec<String> = data
            .values()
            .map(|v| v.to_param_with_dialect(&*self.dialect).to_string())
            .collect();

        format!(
            "INSERT INTO {} ({}) VALUES ({})",
            self.dialect.quote(&table),
            columns.join(", "),
            values.join(", ")
        )
    }

    #[tracing::instrument(skip(self, data), fields(op = "update"))]
    pub fn build_update(&self, data: &std::collections::HashMap<String, Value>) -> String {
        let table = self
            .table
            .clone()
            .unwrap_or_else(|| M::table_name().to_string());

        if data.is_empty() {
            return String::new();
        }

        let set_clauses: Vec<String> = data
            .iter()
            .map(|(k, v)| {
                format!(
                    "{} = {}",
                    self.dialect.quote(k),
                    v.to_param_with_dialect(&*self.dialect)
                )
            })
            .collect();

        let mut sql = format!(
            "UPDATE {} SET {}",
            self.dialect.quote(&table),
            set_clauses.join(", ")
        );

        sql.push_str(&self.build_where_clause());
        sql
    }

    /// 构建 DELETE SQL 语句。
    ///
    /// **P0-1 软删除集成（v1.3.0+）**：当 `M: Model` 实现了 `soft_delete_field()`
    /// 返回 `Some(field)` 且未调用 `without_soft_delete()` 时，本方法自动生成
    /// `UPDATE {table} SET {field} = NOW() WHERE ...` 而非 `DELETE FROM ...`。
    ///
    /// 这与 SeaORM 的 `ActiveModelBehavior::after_delete` + `ActiveValue::Set`
    /// 行为对齐：删除操作实际是软删除 UPDATE。
    ///
    /// 若需物理删除，请使用 [`build_force_delete`](Self::build_force_delete)。
    #[tracing::instrument(skip(self), fields(op = "delete"))]
    pub fn build_delete(&self) -> String {
        let table = self
            .table
            .clone()
            .unwrap_or_else(|| M::table_name().to_string());

        // P0-1：软删除启用时转为 UPDATE SET {field} = NOW()
        if let Some(field) = self.soft_delete_field() {
            let where_clause = self.build_where_clause();
            return format!(
                "UPDATE {} SET {} = NOW(){}",
                self.dialect.quote(&table),
                self.dialect.quote(field),
                where_clause
            );
        }

        let mut sql = format!("DELETE FROM {}", self.dialect.quote(&table));
        sql.push_str(&self.build_where_clause());
        sql
    }

    /// 构建物理 DELETE SQL 语句（绕过软删除）。
    ///
    /// 即使 Model 实现了 `soft_delete_field()`，也生成 `DELETE FROM ...`，
    /// 且**不追加** `WHERE deleted_at IS NULL` 过滤（保留用户指定的 WHERE 条件）。
    /// 用于管理员强制清除场景。
    ///
    /// # 安全警告
    ///
    /// 物理删除不可恢复，请谨慎使用。
    pub fn build_force_delete(&self) -> String {
        let table = self
            .table
            .clone()
            .unwrap_or_else(|| M::table_name().to_string());

        let mut sql = format!("DELETE FROM {}", self.dialect.quote(&table));
        // P0-1：物理删除不追加软删除过滤（include_soft_delete = false）
        sql.push_str(&self.build_where_clause_with_options(false));
        sql
    }

    // ===================== 参数绑定版本（v1.1.0 新增） =====================

    /// 构建 WHERE 子句（参数绑定版本）。
    ///
    /// P0-1：自动追加软删除过滤条件（`AND {soft_delete_field} IS NULL`，无参数）
    ///
    /// 将 `In`/`NotIn`/`Between`/`NotBetween`/`Eq`/`Ne`/`Gt`/`Ge`/`Lt`/`Le`/`Like`
    /// 条件中的值替换为 `?` 占位符，值收集到 `params` 向量中。
    /// `And`/`Or`/`Exists` 等原始字符串条件不提取参数（调用方负责安全）。
    fn build_where_clause_with_params(&self) -> (String, Vec<Value>) {
        // 默认包含软删除条件
        self.build_where_clause_with_params_options(true)
    }

    /// 构建参数化 WHERE 子句（可控是否包含软删除条件）
    ///
    /// # 参数
    ///
    /// - `include_soft_delete`：true 时追加软删除条件；false 时跳过（用于物理删除等场景）
    ///
    /// P0-3：租户条件总是追加（若启用），不受 `include_soft_delete` 控制。
    /// 租户值通过 `?` 占位符绑定，加入 `params` 列表末尾。
    fn build_where_clause_with_params_options(
        &self,
        include_soft_delete: bool,
    ) -> (String, Vec<Value>) {
        // P0-1：构造软删除条件（若有且启用，无参数）
        let soft_delete_cond = if include_soft_delete {
            self.build_soft_delete_condition()
        } else {
            None
        };

        // P0-3：构造租户条件（若有且启用）— 参数化版本保留 (sql, value)
        let tenant_cond = self.build_tenant_condition();

        // 无用户条件且无软删除条件且无租户条件且无 keyset 游标 → 空 WHERE
        if self.where_conditions.is_empty()
            && soft_delete_cond.is_none()
            && tenant_cond.is_none()
            && self.keyset_cursor.is_none()
        {
            return (String::new(), Vec::new());
        }

        let mut params = Vec::new();

        let mut conditions: Vec<String> = self
            .where_conditions
            .iter()
            .map(|cond| match cond {
                WhereCondition::And(c) => c.clone(),
                WhereCondition::Or(c) => format!("OR {}", c),
                // P0-2：参数化条件使用 `?` 占位符
                WhereCondition::Eq(f, v) => {
                    params.push(v.clone());
                    format!("{} = ?", self.dialect.quote(f))
                }
                WhereCondition::Ne(f, v) => {
                    params.push(v.clone());
                    format!("{} != ?", self.dialect.quote(f))
                }
                WhereCondition::Gt(f, v) => {
                    params.push(v.clone());
                    format!("{} > ?", self.dialect.quote(f))
                }
                WhereCondition::Ge(f, v) => {
                    params.push(v.clone());
                    format!("{} >= ?", self.dialect.quote(f))
                }
                WhereCondition::Lt(f, v) => {
                    params.push(v.clone());
                    format!("{} < ?", self.dialect.quote(f))
                }
                WhereCondition::Le(f, v) => {
                    params.push(v.clone());
                    format!("{} <= ?", self.dialect.quote(f))
                }
                WhereCondition::Like(f, v) => {
                    params.push(v.clone());
                    format!("{} LIKE ?", self.dialect.quote(f))
                }
                WhereCondition::OrEq(f, v) => {
                    params.push(v.clone());
                    format!("OR {} = ?", self.dialect.quote(f))
                }
                WhereCondition::OrNe(f, v) => {
                    params.push(v.clone());
                    format!("OR {} != ?", self.dialect.quote(f))
                }
                WhereCondition::OrGt(f, v) => {
                    params.push(v.clone());
                    format!("OR {} > ?", self.dialect.quote(f))
                }
                WhereCondition::OrGe(f, v) => {
                    params.push(v.clone());
                    format!("OR {} >= ?", self.dialect.quote(f))
                }
                WhereCondition::OrLt(f, v) => {
                    params.push(v.clone());
                    format!("OR {} < ?", self.dialect.quote(f))
                }
                WhereCondition::OrLe(f, v) => {
                    params.push(v.clone());
                    format!("OR {} <= ?", self.dialect.quote(f))
                }
                WhereCondition::OrLike(f, v) => {
                    params.push(v.clone());
                    format!("OR {} LIKE ?", self.dialect.quote(f))
                }
                WhereCondition::In(f, vals) => {
                    let placeholders: Vec<&str> = vals.iter().map(|_| "?").collect();
                    params.extend(vals.iter().cloned());
                    format!("{} IN ({})", self.dialect.quote(f), placeholders.join(", "))
                }
                WhereCondition::NotIn(f, vals) => {
                    let placeholders: Vec<&str> = vals.iter().map(|_| "?").collect();
                    params.extend(vals.iter().cloned());
                    format!(
                        "{} NOT IN ({})",
                        self.dialect.quote(f),
                        placeholders.join(", ")
                    )
                }
                WhereCondition::Between(f, start, end) => {
                    params.push(start.clone());
                    params.push(end.clone());
                    format!("{} BETWEEN ? AND ?", self.dialect.quote(f))
                }
                WhereCondition::NotBetween(f, start, end) => {
                    params.push(start.clone());
                    params.push(end.clone());
                    format!("{} NOT BETWEEN ? AND ?", self.dialect.quote(f))
                }
                WhereCondition::Null(f) => format!("{} IS NULL", self.dialect.quote(f)),
                WhereCondition::NotNull(f) => format!("{} IS NOT NULL", self.dialect.quote(f)),
                WhereCondition::Exists(s) => format!("EXISTS ({})", s),
                WhereCondition::NotExists(s) => format!("NOT EXISTS ({})", s),
            })
            .collect();

        // P0-1：追加软删除条件（作为最后一个 AND 条件，无参数）
        if let Some(sd_cond) = soft_delete_cond {
            conditions.push(sd_cond);
        }

        // P0-3：追加租户条件（在软删除之后，参数化绑定）
        if let Some((t_sql, t_value)) = tenant_cond {
            conditions.push(t_sql);
            params.push(t_value);
        }

        // P2-5：追加 keyset 游标条件（在租户条件之后，参数化绑定）
        if let Some(ref cursor) = self.keyset_cursor {
            let op = match cursor.direction {
                KeysetDirection::After => ">",
                KeysetDirection::Before => "<",
            };
            conditions.push(format!("{} {} ?", self.dialect.quote(&cursor.field), op));
            params.push(cursor.value.clone());
        }

        if conditions.is_empty() {
            return (String::new(), params);
        }

        // OR 分组逻辑：与 build_where_clause 相同
        let mut groups: Vec<Vec<String>> = Vec::new();
        let mut current_group: Vec<String> = Vec::new();
        for cond in conditions.iter() {
            if let Some(stripped) = cond.strip_prefix("OR ") {
                current_group.push(stripped.to_string());
            } else {
                if !current_group.is_empty() {
                    groups.push(std::mem::take(&mut current_group));
                }
                current_group.push(cond.clone());
            }
        }
        if !current_group.is_empty() {
            groups.push(current_group);
        }

        let group_strs: Vec<String> = groups
            .iter()
            .map(|g| {
                if g.len() == 1 {
                    g[0].clone()
                } else {
                    format!("({})", g.join(" OR "))
                }
            })
            .collect();

        (format!(" WHERE {}", group_strs.join(" AND ")), params)
    }

    /// 构建 SELECT SQL（参数绑定版本）。
    ///
    /// WHERE 子句中的值使用 `?` 占位符，值通过 `params` 返回。
    /// 适用于 `Connection::query_with_params()`。
    pub fn build_select_with_params(&self) -> (String, Vec<Value>) {
        let table = self
            .table
            .clone()
            .unwrap_or_else(|| M::table_name().to_string());
        let columns = if self.select_columns.is_empty() {
            "*".to_string()
        } else {
            self.select_columns.join(", ")
        };

        let mut sql = format!("SELECT {} FROM {}", columns, self.dialect.quote(&table));

        for join in &self.joins {
            match join {
                JoinClause::Inner(t, l, r) => {
                    sql.push_str(&format!(
                        " INNER JOIN {} ON {} = {}",
                        self.dialect.quote(t),
                        self.dialect.quote(l),
                        self.dialect.quote(r)
                    ));
                }
                JoinClause::Left(t, l, r) => {
                    sql.push_str(&format!(
                        " LEFT JOIN {} ON {} = {}",
                        self.dialect.quote(t),
                        self.dialect.quote(l),
                        self.dialect.quote(r)
                    ));
                }
                JoinClause::Right(t, l, r) => {
                    sql.push_str(&format!(
                        " RIGHT JOIN {} ON {} = {}",
                        self.dialect.quote(t),
                        self.dialect.quote(l),
                        self.dialect.quote(r)
                    ));
                }
                JoinClause::Cross(t, on) => {
                    sql.push_str(&format!(
                        " CROSS JOIN {} ON {}",
                        self.dialect.quote(t),
                        self.dialect.quote(on)
                    ));
                }
            }
        }

        let mut params = Vec::new();
        // P0-1：build_where_clause_with_params 内部已处理软删除条件
        let (where_clause, where_params) = self.build_where_clause_with_params();
        if !where_clause.is_empty() {
            sql.push_str(&where_clause);
            params = where_params;
        }

        if !self.group_by.is_empty() {
            let cols: Vec<String> = self
                .group_by
                .iter()
                .map(|c| self.dialect.quote(c))
                .collect();
            sql.push_str(" GROUP BY ");
            sql.push_str(&cols.join(", "));
        }

        if !self.having_conditions.is_empty() {
            sql.push_str(" HAVING ");
            for (i, cond) in self.having_conditions.iter().enumerate() {
                if i > 0 {
                    sql.push_str(" AND ");
                }
                if let WhereCondition::And(c) = cond {
                    sql.push_str(c);
                }
            }
        }

        if !self.order_by.is_empty() {
            let order_cols: Vec<String> = self
                .order_by
                .iter()
                .map(|o| {
                    let dir = match o.direction {
                        OrderDirection::Asc => " ASC",
                        OrderDirection::Desc => " DESC",
                    };
                    format!("{}{}", self.dialect.quote(&o.field), dir)
                })
                .collect();
            sql.push_str(" ORDER BY ");
            sql.push_str(&order_cols.join(", "));
        }

        if let Some(limit) = self.limit_value {
            sql.push_str(&format!(" LIMIT {}", limit));
        }
        if let Some(offset) = self.offset_value {
            sql.push_str(&format!(" OFFSET {}", offset));
        }

        (sql, params)
    }

    /// 构建 INSERT SQL（参数绑定版本）。
    pub fn build_insert_with_params(
        &self,
        data: &std::collections::HashMap<String, Value>,
    ) -> (String, Vec<Value>) {
        let table = self
            .table
            .clone()
            .unwrap_or_else(|| M::table_name().to_string());
        if data.is_empty() {
            return (String::new(), Vec::new());
        }

        let mut columns = Vec::with_capacity(data.len());
        let mut params = Vec::with_capacity(data.len());
        let placeholders: Vec<&str> = data.iter().map(|_| "?").collect();
        for (k, v) in data.iter() {
            columns.push(self.dialect.quote(k));
            params.push(v.clone());
        }

        let sql = format!(
            "INSERT INTO {} ({}) VALUES ({})",
            self.dialect.quote(&table),
            columns.join(", "),
            placeholders.join(", ")
        );
        (sql, params)
    }

    /// P2-6：构建批量 INSERT SQL（参数绑定版本）。
    ///
    /// 生成 `INSERT INTO t (c1, c2) VALUES (?, ?), (?, ?), ...` 形式的多行插入 SQL。
    /// 所有行的列必须一致（取第一行的列顺序）；空行列表返回空 SQL。
    ///
    /// **L3 实现深度**：使用参数化占位符 `?`，所有值通过 `params` 绑定，杜绝 SQL 注入。
    pub fn build_batch_insert_with_params(
        &self,
        rows: &[std::collections::HashMap<String, Value>],
    ) -> (String, Vec<Value>) {
        let table = self
            .table
            .clone()
            .unwrap_or_else(|| M::table_name().to_string());
        if rows.is_empty() {
            return (String::new(), Vec::new());
        }

        // 取第一行的列作为列顺序（所有行必须一致）
        let first_row = &rows[0];
        let columns: Vec<String> = first_row.keys().cloned().collect();
        let quoted_columns: Vec<String> = columns.iter().map(|c| self.dialect.quote(c)).collect();

        let mut params = Vec::with_capacity(rows.len() * columns.len());
        let mut value_groups: Vec<String> = Vec::with_capacity(rows.len());
        for row in rows {
            let placeholders: Vec<String> = columns
                .iter()
                .map(|col| {
                    match row.get(col) {
                        Some(v) => {
                            params.push(v.clone());
                            "?".to_string()
                        }
                        None => "NULL".to_string(),
                    }
                })
                .collect();
            value_groups.push(format!("({})", placeholders.join(", ")));
        }

        let sql = format!(
            "INSERT INTO {} ({}) VALUES {}",
            self.dialect.quote(&table),
            quoted_columns.join(", "),
            value_groups.join(", ")
        );
        (sql, params)
    }

    /// P2-6：构建批量 Upsert SQL（参数绑定版本）。
    ///
    /// 在 `build_batch_insert_with_params` 基础上追加冲突处理子句：
    /// - MySQL: `ON DUPLICATE KEY UPDATE col=VALUES(col), ...`
    /// - PostgreSQL/SQLite: `ON CONFLICT (conflict_cols) DO UPDATE SET col=EXCLUDED.col, ...`
    /// - Oracle/SQL Server/ClickHouse/Db2: 返回 `Err(DbError::InvalidInput)`（不支持）
    ///
    /// # 参数
    /// - `rows`: 批量数据行（所有行的列必须一致）
    /// - `conflict_columns`: 冲突检测列（主键/唯一键）；MySQL 自动检测可传空
    /// - `update_columns`: 冲突时更新的列；空切片表示更新所有非冲突列
    ///
    /// # 返回
    /// - `Ok((sql, params))`: 生成的 SQL 和参数列表
    /// - `Err(DbError::InvalidInput)`: 方言不支持 upsert 或 rows 为空
    ///
    /// **L3 实现深度**：
    /// 1. SQL 下推：冲突处理由数据库执行，非内存判断
    /// 2. 参数化：所有值通过 `?` 占位符绑定，不拼接用户值
    /// 3. 实际执行：生成标准 INSERT...ON CONFLICT/ON DUPLICATE KEY SQL
    pub fn build_batch_upsert_with_params(
        &self,
        rows: &[std::collections::HashMap<String, Value>],
        conflict_columns: &[&str],
        update_columns: &[&str],
    ) -> Result<(String, Vec<Value>), crate::DbError> {
        if rows.is_empty() {
            return Err(crate::DbError::InvalidInput(
                "build_batch_upsert_with_params: rows cannot be empty".to_string(),
            ));
        }

        // 构建批量 INSERT 部分
        let (insert_sql, params) = self.build_batch_insert_with_params(rows);
        if insert_sql.is_empty() {
            return Err(crate::DbError::InvalidInput(
                "build_batch_upsert_with_params: failed to build INSERT part".to_string(),
            ));
        }

        // 取所有列名（原始未 quote）
        let all_columns: Vec<String> = rows[0].keys().cloned().collect();

        // 调用方言生成冲突处理子句
        let conflict_clause = self
            .dialect
            .build_upsert_on_conflict(conflict_columns, update_columns, &all_columns)
            .ok_or_else(|| {
                crate::DbError::InvalidInput(format!(
                    "build_batch_upsert_with_params: dialect {:?} does not support upsert (ON CONFLICT / ON DUPLICATE KEY UPDATE). Consider using MERGE statement or individual upserts instead.",
                    self.dialect.db_type()
                ))
            })?;

        let sql = format!("{} {}", insert_sql, conflict_clause);
        Ok((sql, params))
    }

    /// 构建 UPDATE SQL（参数绑定版本）。
    /// 参数顺序：SET 参数在前，WHERE 参数在后。
    pub fn build_update_with_params(
        &self,
        data: &std::collections::HashMap<String, Value>,
    ) -> (String, Vec<Value>) {
        let table = self
            .table
            .clone()
            .unwrap_or_else(|| M::table_name().to_string());
        if data.is_empty() {
            return (String::new(), Vec::new());
        }

        let mut set_clauses = Vec::with_capacity(data.len());
        let mut params = Vec::with_capacity(data.len());
        for (k, v) in data.iter() {
            set_clauses.push(format!("{} = ?", self.dialect.quote(k)));
            params.push(v.clone());
        }

        let mut sql = format!(
            "UPDATE {} SET {}",
            self.dialect.quote(&table),
            set_clauses.join(", ")
        );

        // P0-1：build_where_clause_with_params 内部已处理软删除条件
        let (where_clause, where_params) = self.build_where_clause_with_params();
        if !where_clause.is_empty() {
            sql.push_str(&where_clause);
            params.extend(where_params);
        }

        (sql, params)
    }

    /// 构建 DELETE SQL（参数绑定版本）。
    ///
    /// **P0-1 软删除集成（v1.3.0+）**：当 Model 启用软删除时，自动生成
    /// `UPDATE {table} SET {field} = NOW() WHERE ...` 而非 `DELETE FROM ...`。
    /// 参数列表为空（NOW() 由数据库填充）。
    pub fn build_delete_with_params(&self) -> (String, Vec<Value>) {
        let table = self
            .table
            .clone()
            .unwrap_or_else(|| M::table_name().to_string());

        // P0-1：软删除启用时转为 UPDATE SET {field} = NOW()
        if let Some(field) = self.soft_delete_field() {
            let (where_clause, where_params) = self.build_where_clause_with_params();
            let sql = format!(
                "UPDATE {} SET {} = NOW(){}",
                self.dialect.quote(&table),
                self.dialect.quote(field),
                where_clause
            );
            return (sql, where_params);
        }

        let mut sql = format!("DELETE FROM {}", self.dialect.quote(&table));
        let mut params = Vec::new();

        let (where_clause, where_params) = self.build_where_clause_with_params();
        if !where_clause.is_empty() {
            sql.push_str(&where_clause);
            params = where_params;
        }

        (sql, params)
    }

    /// 构建物理 DELETE SQL（参数绑定版本，绕过软删除）。
    ///
    /// 即使 Model 启用软删除，也生成 `DELETE FROM ...`，且不追加软删除过滤。
    pub fn build_force_delete_with_params(&self) -> (String, Vec<Value>) {
        let table = self
            .table
            .clone()
            .unwrap_or_else(|| M::table_name().to_string());

        let mut sql = format!("DELETE FROM {}", self.dialect.quote(&table));
        let mut params = Vec::new();

        // P0-1：物理删除不追加软删除过滤，使用 build_where_clause_with_params_no_soft_delete
        let (where_clause, where_params) = self.build_where_clause_with_params_options(false);
        if !where_clause.is_empty() {
            sql.push_str(&where_clause);
            params = where_params;
        }

        (sql, params)
    }

    pub fn build_count(&self) -> String {
        let table = self
            .table
            .clone()
            .unwrap_or_else(|| M::table_name().to_string());

        let mut sql = format!(
            "SELECT COUNT(*) as total FROM {}",
            self.dialect.quote(&table)
        );
        sql.push_str(&self.build_where_clause());
        sql
    }

    pub fn build_exists(&self) -> String {
        let table = self
            .table
            .clone()
            .unwrap_or_else(|| M::table_name().to_string());

        let mut sql = format!("SELECT 1 FROM {}", self.dialect.quote(&table));
        sql.push_str(&self.build_where_clause());
        sql.push_str(" LIMIT 1");
        format!("SELECT EXISTS({})", sql)
    }

    pub fn build_max(&self, field: &str) -> String {
        let table = self
            .table
            .clone()
            .unwrap_or_else(|| M::table_name().to_string());

        let mut sql = format!(
            "SELECT MAX({}) as max_val FROM {}",
            self.dialect.quote(field),
            self.dialect.quote(&table)
        );
        sql.push_str(&self.build_where_clause());
        sql
    }

    pub fn build_min(&self, field: &str) -> String {
        let table = self
            .table
            .clone()
            .unwrap_or_else(|| M::table_name().to_string());

        let mut sql = format!(
            "SELECT MIN({}) as min_val FROM {}",
            self.dialect.quote(field),
            self.dialect.quote(&table)
        );
        sql.push_str(&self.build_where_clause());
        sql
    }

    pub fn build_sum(&self, field: &str) -> String {
        let table = self
            .table
            .clone()
            .unwrap_or_else(|| M::table_name().to_string());

        let mut sql = format!(
            "SELECT SUM({}) as sum_val FROM {}",
            self.dialect.quote(field),
            self.dialect.quote(&table)
        );
        sql.push_str(&self.build_where_clause());
        sql
    }

    pub fn build_avg(&self, field: &str) -> String {
        let table = self
            .table
            .clone()
            .unwrap_or_else(|| M::table_name().to_string());

        let mut sql = format!(
            "SELECT AVG({}) as avg_val FROM {}",
            self.dialect.quote(field),
            self.dialect.quote(&table)
        );
        sql.push_str(&self.build_where_clause());
        sql
    }

    /// 校验生成的 SELECT SQL 语句
    /// 检查 SQL 语法、JOIN 列名、表名合法性
    pub fn validate(&self) -> Result<(), Vec<sz_orm_sql_validator::SqlValidationError>> {
        let sql = self.build_select();
        let mut errors = Vec::new();

        if let Err(e) = sz_orm_sql_validator::validate_select(&sql) {
            errors.push(e);
        }

        // 校验 JOIN 子句产生的 SQL 是否合法
        if !self.joins.is_empty() {
            for join in &self.joins {
                match join {
                    JoinClause::Inner(_, left, right)
                    | JoinClause::Left(_, left, right)
                    | JoinClause::Right(_, left, right) => {
                        if let Err(e) = sz_orm_sql_validator::validate_column_name(left) {
                            errors.push(e);
                        }
                        if let Err(e) = sz_orm_sql_validator::validate_column_name(right) {
                            errors.push(e);
                        }
                    }
                    _ => {}
                }
            }
        }

        // 校验表名合法性
        let table = self
            .table
            .clone()
            .unwrap_or_else(|| M::table_name().to_string());
        if let Err(e) = sz_orm_sql_validator::validate_table_name(&table) {
            errors.push(e);
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// 校验生成的 INSERT SQL 语句
    /// 含空数据检测（EmptyInsertData 错误）
    pub fn validate_insert(
        &self,
        data: &std::collections::HashMap<String, Value>,
    ) -> Result<(), Vec<sz_orm_sql_validator::SqlValidationError>> {
        let sql = self.build_insert(data);
        let mut errors = Vec::new();

        if sql.is_empty() {
            errors.push(sz_orm_sql_validator::SqlValidationError::EmptyInsertData);
            return Err(errors);
        }

        if let Err(e) = sz_orm_sql_validator::validate_insert(&sql) {
            errors.push(e);
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// 校验生成的 UPDATE SQL 语句
    /// 含空数据检测（EmptyUpdateData 错误）
    pub fn validate_update(
        &self,
        data: &std::collections::HashMap<String, Value>,
    ) -> Result<(), Vec<sz_orm_sql_validator::SqlValidationError>> {
        let sql = self.build_update(data);
        let mut errors = Vec::new();

        if sql.is_empty() {
            errors.push(sz_orm_sql_validator::SqlValidationError::EmptyUpdateData);
            return Err(errors);
        }

        if let Err(e) = sz_orm_sql_validator::validate_update(&sql) {
            errors.push(e);
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// 校验生成的 DELETE SQL 语句
    pub fn validate_delete(&self) -> Result<(), Vec<sz_orm_sql_validator::SqlValidationError>> {
        let sql = self.build_delete();
        let mut errors = Vec::new();

        if let Err(e) = sz_orm_sql_validator::validate_delete(&sql) {
            errors.push(e);
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

impl<M: Model> fmt::Debug for QueryBuilder<M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("QueryBuilder")
            .field("table", &self.table)
            .field("select_columns", &self.select_columns)
            .field("where_conditions", &self.where_conditions.len())
            .field("limit", &self.limit_value)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db_type::DbType;
    use crate::dialect::get_dialect;

    struct TestModel;
    impl Model for TestModel {
        type PrimaryKey = i64;

        fn table_name() -> &'static str {
            "test_models"
        }

        fn pk(&self) -> Self::PrimaryKey {
            1
        }

        fn set_pk(&mut self, _pk: Self::PrimaryKey) {}
    }

    #[test]
    fn test_query_builder_select() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        let sql = builder
            .table("users")
            .select(vec!["id", "name"])
            .build_select();
        assert!(sql.contains("SELECT id, name FROM"));
        assert!(sql.contains("`users`"));
    }

    #[test]
    fn test_query_builder_where() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        let sql = builder
            .table("users")
            .where_cond("status = 'active'")
            .where_cond("age > 18")
            .build_select();

        assert!(sql.contains("WHERE"));
        assert!(sql.contains("status = 'active'"));
        assert!(sql.contains("age > 18"));
    }

    #[test]
    fn test_query_builder_order_by() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        let sql = builder
            .table("users")
            .order_by("created_at")
            .order_desc("id")
            .build_select();

        assert!(sql.contains("ORDER BY"));
        assert!(sql.contains("`created_at` ASC"));
        assert!(sql.contains("`id` DESC"));
    }

    #[test]
    fn test_query_builder_limit_offset() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        let sql = builder.table("users").limit(10).offset(20).build_select();

        assert!(sql.contains("LIMIT 10"));
        assert!(sql.contains("OFFSET 20"));
    }

    #[test]
    fn test_query_builder_page() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        let sql = builder.table("users").page(3, 20).build_select();

        assert!(sql.contains("LIMIT 20"));
        assert!(sql.contains("OFFSET 40"));
    }

    #[test]
    fn test_query_builder_insert() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        let mut data = std::collections::HashMap::new();
        data.insert("name".to_string(), Value::String("test".to_string()));
        data.insert("age".to_string(), Value::I64(25));

        let sql = builder.table("users").build_insert(&data);

        assert!(sql.contains("INSERT INTO"));
        assert!(sql.contains("`name`"));
        assert!(sql.contains("'test'"));
    }

    #[test]
    fn test_query_builder_update() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        let mut data = std::collections::HashMap::new();
        data.insert("name".to_string(), Value::String("updated".to_string()));

        let sql = builder
            .table("users")
            .where_cond("id = 1")
            .build_update(&data);

        assert!(sql.contains("UPDATE"));
        assert!(sql.contains("`name` = 'updated'"));
        assert!(sql.contains("WHERE"));
    }

    #[test]
    fn test_query_builder_delete() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        let sql = builder.table("users").where_cond("id = 1").build_delete();

        assert!(sql.contains("DELETE FROM"));
        assert!(sql.contains("WHERE"));
    }

    #[test]
    fn test_query_builder_count() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        let sql = builder.table("users").build_count();

        assert!(sql.contains("SELECT COUNT(*)"));
        assert!(sql.contains("FROM"));
    }

    #[test]
    fn test_query_builder_where_in() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        let sql = builder
            .table("users")
            .where_in("id", vec![Value::I64(1), Value::I64(2), Value::I64(3)])
            .build_select();

        assert!(sql.contains("IN ("));
    }

    #[test]
    fn test_query_builder_where_between() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        let sql = builder
            .table("users")
            .where_between("age", Value::I64(18), Value::I64(30))
            .build_select();

        assert!(sql.contains("BETWEEN"));
    }

    #[test]
    fn test_query_builder_where_null() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        let sql = builder
            .table("users")
            .where_null("deleted_at")
            .build_select();

        assert!(sql.contains("IS NULL"));
    }

    #[test]
    fn test_query_builder_join() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        let sql = builder
            .table("users")
            .join_inner("posts", "users.id", "posts.user_id")
            .build_select();

        assert!(sql.contains("INNER JOIN"));
        assert!(sql.contains("`posts`"));
    }

    #[test]
    fn test_query_builder_group_by() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        let sql = builder.table("users").group_by("status").build_select();

        assert!(sql.contains("GROUP BY"));
        assert!(sql.contains("`status`"));
    }

    #[test]
    fn test_query_builder_max() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        let sql = builder.table("users").build_max("score");

        assert!(sql.contains("MAX("));
        assert!(sql.contains("`score`"));
    }

    #[test]
    fn test_query_builder_min() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        let sql = builder.table("users").build_min("price");

        assert!(sql.contains("MIN("));
        assert!(sql.contains("`price`"));
    }

    #[test]
    fn test_query_builder_sum() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        let sql = builder.table("orders").build_sum("amount");

        assert!(sql.contains("SUM("));
        assert!(sql.contains("`amount`"));
    }

    #[test]
    fn test_query_builder_avg() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        let sql = builder.table("scores").build_avg("value");

        assert!(sql.contains("AVG("));
        assert!(sql.contains("`value`"));
    }

    #[test]
    fn test_validator_select() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        let result = builder.table("users").select(vec!["id", "name"]).validate();
        assert!(result.is_ok());
    }

    #[test]
    fn test_validator_select_with_join() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        let result = builder
            .table("users")
            .join_inner("posts", "users.id", "posts.user_id")
            .validate();
        assert!(result.is_ok());
    }

    #[test]
    fn test_validator_insert() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        let mut data = std::collections::HashMap::new();
        data.insert("name".to_string(), Value::String("test".to_string()));

        let result = builder.table("users").validate_insert(&data);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validator_insert_empty_data() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        let data = std::collections::HashMap::new();
        let result = builder.table("users").validate_insert(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_validator_update() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        let mut data = std::collections::HashMap::new();
        data.insert("name".to_string(), Value::String("updated".to_string()));

        let result = builder.table("users").validate_update(&data);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validator_update_empty_data() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        let data = std::collections::HashMap::new();
        let result = builder.table("users").validate_update(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_validator_delete() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        let result = builder
            .table("users")
            .where_cond("id = 1")
            .validate_delete();
        assert!(result.is_ok());
    }

    #[test]
    fn test_validator_delete_no_where() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        // DELETE without WHERE still produces valid SQL (just no filter)
        let result = builder.table("users").validate_delete();
        assert!(result.is_ok());
    }

    // ==================== M-3 select_quoted 测试 ====================

    #[test]
    fn test_m3_select_quoted_valid_columns() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);
        let builder = builder
            .table("users")
            .select_quoted(vec!["id", "name"])
            .expect("valid columns should succeed");
        let sql = builder.build_select();
        // 应自动 quote 列名
        assert!(sql.contains("SELECT `id`, `name` FROM"));
        assert!(sql.contains("`users`"));
    }

    #[test]
    fn test_m3_select_quoted_rejects_sql_injection() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);

        // SQL 注入尝试：分号 + DROP TABLE
        let result = builder
            .table("users")
            .select_quoted(vec!["id; DROP TABLE users"]);
        assert!(result.is_err());

        // 含引号
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);
        let result = builder.table("users").select_quoted(vec!["name'"]);
        assert!(result.is_err());

        // 数字开头
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);
        let result = builder.table("users").select_quoted(vec!["1col"]);
        assert!(result.is_err());

        // 含空格
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);
        let result = builder.table("users").select_quoted(vec!["col name"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_m3_select_quoted_postgresql_dialect() {
        let dialect = get_dialect(DbType::PostgreSQL).unwrap();
        let builder = QueryBuilder::<TestModel>::new(dialect);
        let builder = builder
            .table("users")
            .select_quoted(vec!["id", "name"])
            .expect("valid columns should succeed");
        let sql = builder.build_select();
        // PostgreSQL 使用双引号
        assert!(sql.contains("SELECT \"id\", \"name\" FROM"));
        assert!(sql.contains("\"users\""));
    }

    // ==================== P0-1 软删除集成行为测试 ====================

    /// 软删除测试模型：实现 soft_delete_field() 返回 "deleted_at"
    struct SoftDeleteModel;
    impl Model for SoftDeleteModel {
        type PrimaryKey = i64;

        fn table_name() -> &'static str {
            "soft_users"
        }

        fn pk(&self) -> Self::PrimaryKey {
            1
        }

        fn set_pk(&mut self, _pk: Self::PrimaryKey) {}

        fn soft_delete_field() -> Option<&'static str> {
            Some("deleted_at")
        }
    }

    /// 行为级测试 L3-1：软删除模型 build_select 自动追加 `WHERE deleted_at IS NULL`
    ///
    /// 用户视角：查询软删除模型时，自动过滤已删除记录，无需手动写条件。
    #[test]
    fn test_p01_soft_delete_select_auto_filter() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<SoftDeleteModel>::new(dialect);
        let sql = builder.table("soft_users").build_select();
        // 必须自动追加软删除过滤
        assert!(
            sql.contains("`deleted_at` IS NULL"),
            "软删除模型 SELECT 必须自动追加 `deleted_at` IS NULL，实际: {}",
            sql
        );
    }

    /// 行为级测试 L3-2：软删除模型 + 用户 WHERE 条件，软删除条件以 AND 追加
    #[test]
    fn test_p01_soft_delete_select_with_user_where() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let sql = QueryBuilder::<SoftDeleteModel>::new(dialect)
            .table("soft_users")
            .where_eq("status", Value::String("active".into()))
            .build_select();
        // 用户条件 + 软删除条件 同时存在
        assert!(sql.contains("`status` = "), "用户条件应保留: {}", sql);
        assert!(
            sql.contains("`deleted_at` IS NULL"),
            "软删除条件应自动追加: {}",
            sql
        );
    }

    /// 行为级测试 L3-3：without_soft_delete() 临时禁用软删除过滤
    ///
    /// 用户视角：管理员查询已删除记录时，可禁用自动过滤。
    #[test]
    fn test_p01_soft_delete_without_soft_delete() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let sql = QueryBuilder::<SoftDeleteModel>::new(dialect)
            .table("soft_users")
            .without_soft_delete()
            .build_select();
        // 不应包含软删除过滤
        assert!(
            !sql.contains("`deleted_at` IS NULL"),
            "without_soft_delete 应禁用过滤，实际: {}",
            sql
        );
        // 也应无 WHERE 子句（因为用户未提供任何条件）
        assert!(
            !sql.contains("WHERE"),
            "无用户条件 + 禁用软删除应无 WHERE 子句: {}",
            sql
        );
    }

    /// 行为级测试 L3-4：软删除模型 build_delete 自动转为 UPDATE
    ///
    /// 用户视角：调用 delete 实际是软删除 UPDATE，不是物理 DELETE。
    #[test]
    fn test_p01_soft_delete_delete_becomes_update() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let sql = QueryBuilder::<SoftDeleteModel>::new(dialect)
            .table("soft_users")
            .where_eq("id", Value::I64(42))
            .build_delete();
        // 应生成 UPDATE 而非 DELETE
        assert!(
            sql.starts_with("UPDATE"),
            "软删除模型的 build_delete 应生成 UPDATE，实际: {}",
            sql
        );
        assert!(
            !sql.contains("DELETE FROM"),
            "不应生成 DELETE FROM: {}",
            sql
        );
        assert!(
            sql.contains("`deleted_at` = NOW()"),
            "应设置 deleted_at = NOW(): {}",
            sql
        );
        // 软删除条件应自动追加，防止更新已删除记录
        assert!(
            sql.contains("`deleted_at` IS NULL"),
            "软删除 UPDATE 应追加 deleted_at IS NULL 防止重复删除: {}",
            sql
        );
    }

    /// 行为级测试 L3-5：build_force_delete 物理删除，不追加软删除过滤
    ///
    /// 用户视角：管理员强制清除时使用 build_force_delete。
    #[test]
    fn test_p01_soft_delete_force_delete() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let sql = QueryBuilder::<SoftDeleteModel>::new(dialect)
            .table("soft_users")
            .where_eq("id", Value::I64(99))
            .build_force_delete();
        // 应生成 DELETE FROM
        assert!(
            sql.starts_with("DELETE FROM"),
            "build_force_delete 应生成 DELETE FROM，实际: {}",
            sql
        );
        // 不应追加软删除过滤
        assert!(
            !sql.contains("`deleted_at` IS NULL"),
            "物理删除不应追加软删除过滤: {}",
            sql
        );
    }

    /// 行为级测试 L3-6：build_select_with_params 自动追加软删除条件（参数化版本）
    #[test]
    fn test_p01_soft_delete_select_with_params() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let (sql, params) = QueryBuilder::<SoftDeleteModel>::new(dialect)
            .table("soft_users")
            .where_eq("id", Value::I64(1))
            .build_select_with_params();
        assert!(
            sql.contains("`deleted_at` IS NULL"),
            "参数化版本也应自动追加软删除: {}",
            sql
        );
        assert_eq!(params.len(), 1, "参数应为 1 个（用户 where_eq 的值）");
        assert_eq!(params[0], Value::I64(1));
    }

    /// 行为级测试 L3-7：build_delete_with_params 自动转为 UPDATE
    #[test]
    fn test_p01_soft_delete_delete_with_params_becomes_update() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let (sql, params) = QueryBuilder::<SoftDeleteModel>::new(dialect)
            .table("soft_users")
            .where_eq("id", Value::I64(7))
            .build_delete_with_params();
        assert!(sql.starts_with("UPDATE"), "应生成 UPDATE: {}", sql);
        assert!(sql.contains("`deleted_at` = NOW()"), "应设置 NOW(): {}", sql);
        assert_eq!(params.len(), 1, "参数应为 1 个（WHERE 的值）");
    }

    /// 行为级测试 L3-8：build_force_delete_with_params 物理删除（参数化版本）
    #[test]
    fn test_p01_soft_delete_force_delete_with_params() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let (sql, params) = QueryBuilder::<SoftDeleteModel>::new(dialect)
            .table("soft_users")
            .where_eq("id", Value::I64(11))
            .build_force_delete_with_params();
        assert!(sql.starts_with("DELETE FROM"), "应生成 DELETE: {}", sql);
        assert!(
            !sql.contains("`deleted_at` IS NULL"),
            "不应追加软删除过滤: {}",
            sql
        );
        assert_eq!(params.len(), 1);
    }

    /// 行为级测试 L3-9：非软删除模型 TestModel 不追加软删除条件
    ///
    /// 用户视角：未启用软删除的模型行为不变。
    #[test]
    fn test_p01_non_soft_delete_model_unchanged() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let sql = QueryBuilder::<TestModel>::new(dialect)
            .table("users")
            .where_eq("id", Value::I64(1))
            .build_select();
        assert!(
            !sql.contains("deleted_at"),
            "非软删除模型不应追加 deleted_at: {}",
            sql
        );
        // build_delete 仍生成 DELETE FROM
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let del_sql = QueryBuilder::<TestModel>::new(dialect)
            .table("users")
            .where_eq("id", Value::I64(1))
            .build_delete();
        assert!(
            del_sql.starts_with("DELETE FROM"),
            "非软删除模型 build_delete 应生成 DELETE: {}",
            del_sql
        );
    }

    /// 行为级测试 L3-10：build_count 也应自动追加软删除条件
    #[test]
    fn test_p01_soft_delete_count_auto_filter() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let sql = QueryBuilder::<SoftDeleteModel>::new(dialect)
            .table("soft_users")
            .build_count();
        assert!(
            sql.contains("`deleted_at` IS NULL"),
            "build_count 也应追加软删除过滤: {}",
            sql
        );
    }

    // ==================== P0-2 参数化查询注入防护测试 ====================

    /// 行为级测试 L3-11：where_eq 使用 `?` 占位符，值收集到 params
    ///
    /// 用户视角：参数化查询杜绝 SQL 注入。
    #[test]
    fn test_p02_where_eq_uses_placeholder() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let (sql, params) = QueryBuilder::<TestModel>::new(dialect)
            .table("users")
            .where_eq("name", Value::String("alice".into()))
            .build_select_with_params();
        // SQL 中应含 `?` 占位符，不应内嵌值
        assert!(
            sql.contains("`name` = ?"),
            "应使用 ? 占位符: {}",
            sql
        );
        assert!(
            !sql.contains("'alice'"),
            "不应内嵌值到 SQL: {}",
            sql
        );
        assert_eq!(params.len(), 1);
        assert_eq!(params[0], Value::String("alice".into()));
    }

    /// 行为级测试 L3-12：where_like 使用 `?` 占位符
    #[test]
    fn test_p02_where_like_uses_placeholder() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let (sql, params) = QueryBuilder::<TestModel>::new(dialect)
            .table("users")
            .where_like("name", Value::String("%alice%".into()))
            .build_select_with_params();
        assert!(sql.contains("`name` LIKE ?"), "应使用 LIKE ?: {}", sql);
        assert!(!sql.contains("%alice%"), "不应内嵌 pattern: {}", sql);
        assert_eq!(params.len(), 1);
    }

    /// 行为级测试 L3-12a：where_ne 使用 `?` 占位符
    ///
    /// 验证 P0-2 参数化 API where_ne 生成 `field != ?` 且值不内嵌。
    #[test]
    fn test_p02_where_ne_uses_placeholder() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let (sql, params) = QueryBuilder::<TestModel>::new(dialect)
            .table("users")
            .where_ne("status", Value::I64(0))
            .build_select_with_params();
        assert!(
            sql.contains("`status` != ?"),
            "应使用 != ?: {}",
            sql
        );
        assert!(!sql.contains("!= 0"), "不应内嵌值: {}", sql);
        assert_eq!(params.len(), 1);
        assert_eq!(params[0], Value::I64(0));
    }

    /// 行为级测试 L3-12b：where_ge 使用 `?` 占位符
    ///
    /// 验证 P0-2 参数化 API where_ge 生成 `field >= ?` 且值不内嵌。
    #[test]
    fn test_p02_where_ge_uses_placeholder() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let (sql, params) = QueryBuilder::<TestModel>::new(dialect)
            .table("users")
            .where_ge("age", Value::I64(18))
            .build_select_with_params();
        assert!(
            sql.contains("`age` >= ?"),
            "应使用 >= ?: {}",
            sql
        );
        assert!(!sql.contains(">= 18"), "不应内嵌值: {}", sql);
        assert_eq!(params.len(), 1);
        assert_eq!(params[0], Value::I64(18));
    }

    /// 行为级测试 L3-12c：where_lt 使用 `?` 占位符
    ///
    /// 验证 P0-2 参数化 API where_lt 生成 `field < ?` 且值不内嵌。
    #[test]
    fn test_p02_where_lt_uses_placeholder() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let (sql, params) = QueryBuilder::<TestModel>::new(dialect)
            .table("users")
            .where_lt("score", Value::F64(60.0))
            .build_select_with_params();
        assert!(
            sql.contains("`score` < ?"),
            "应使用 < ?: {}",
            sql
        );
        assert!(!sql.contains("< 60"), "不应内嵌值: {}", sql);
        assert_eq!(params.len(), 1);
        assert_eq!(params[0], Value::F64(60.0));
    }

    /// 行为级测试 L3-13：注入攻击防护 - 值含 SQL 关键字也不会被解释执行
    ///
    /// 用户视角：即使用户输入 `'; DROP TABLE users; --`，也不会造成注入。
    #[test]
    fn test_p02_injection_protection_drop_table() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let evil_input = "'; DROP TABLE users; --".to_string();
        let (sql, params) = QueryBuilder::<TestModel>::new(dialect)
            .table("users")
            .where_eq("name", Value::String(evil_input.clone()))
            .build_select_with_params();
        // SQL 中不应出现 DROP TABLE
        assert!(
            !sql.contains("DROP TABLE"),
            "SQL 注入未防护: {}",
            sql
        );
        // 整个恶意字符串应作为单一参数传递
        assert_eq!(params.len(), 1);
        assert_eq!(params[0], Value::String(evil_input));
        // SQL 中只有 1 个 `?`
        assert_eq!(sql.matches('?').count(), 1);
    }

    /// 行为级测试 L3-14：注入攻击防护 - OR 1=1 经典攻击
    #[test]
    fn test_p02_injection_protection_or_one_equals_one() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let evil = "' OR '1'='1".to_string();
        let (sql, params) = QueryBuilder::<TestModel>::new(dialect)
            .table("users")
            .where_eq("name", Value::String(evil.clone()))
            .build_select_with_params();
        assert!(
            !sql.contains("OR '1'='1'"),
            "OR 1=1 注入未防护: {}",
            sql
        );
        assert_eq!(params.len(), 1);
        assert_eq!(params[0], Value::String(evil));
    }

    /// 行为级测试 L3-15：多参数顺序正确（WHERE a = ? AND b = ?）
    #[test]
    fn test_p02_multiple_params_order() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let (sql, params) = QueryBuilder::<TestModel>::new(dialect)
            .table("users")
            .where_eq("name", Value::String("alice".into()))
            .where_gt("age", Value::I64(18))
            .where_le("score", Value::F64(99.5))
            .build_select_with_params();
        assert_eq!(
            sql.matches('?').count(),
            3,
            "应有 3 个占位符: {}",
            sql
        );
        assert_eq!(params.len(), 3);
        // 参数顺序应与 WHERE 子句出现顺序一致
        assert_eq!(params[0], Value::String("alice".into()));
        assert_eq!(params[1], Value::I64(18));
        assert_eq!(params[2], Value::F64(99.5));
    }

    /// 行为级测试 L3-16：where_in 参数化
    #[test]
    fn test_p02_where_in_uses_placeholders() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let (sql, params) = QueryBuilder::<TestModel>::new(dialect)
            .table("users")
            .where_in("id", vec![Value::I64(1), Value::I64(2), Value::I64(3)])
            .build_select_with_params();
        assert!(
            sql.contains("`id` IN (?, ?, ?)"),
            "应使用 3 个占位符: {}",
            sql
        );
        assert_eq!(params.len(), 3);
    }

    /// 行为级测试 L3-17：where_between 参数化
    #[test]
    fn test_p02_where_between_uses_placeholders() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let (sql, params) = QueryBuilder::<TestModel>::new(dialect)
            .table("users")
            .where_between("age", Value::I64(18), Value::I64(65))
            .build_select_with_params();
        assert!(
            sql.contains("`age` BETWEEN ? AND ?"),
            "应使用 2 个占位符: {}",
            sql
        );
        assert_eq!(params.len(), 2);
        assert_eq!(params[0], Value::I64(18));
        assert_eq!(params[1], Value::I64(65));
    }

    /// 行为级测试 L3-18：UPDATE 参数化版本 - SET 参数在前，WHERE 参数在后
    #[test]
    fn test_p02_update_params_order_set_before_where() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let mut data = std::collections::HashMap::new();
        data.insert("name".to_string(), Value::String("bob".into()));
        data.insert("age".to_string(), Value::I64(30));
        let (sql, params) = QueryBuilder::<TestModel>::new(dialect)
            .table("users")
            .where_eq("id", Value::I64(99))
            .build_update_with_params(&data);
        // SET 子句应有 2 个占位符，WHERE 子句 1 个，共 3 个
        assert_eq!(sql.matches('?').count(), 3, "应有 3 个 ?: {}", sql);
        assert_eq!(params.len(), 3);
        // 前 2 个为 SET 参数，最后 1 个为 WHERE 参数
        // 注意：HashMap 迭代顺序未指定，仅校验 WHERE 参数在最后
        assert_eq!(params[2], Value::I64(99));
    }

    /// 行为级测试 L3-19：build_where_clause（无参数版本）参数化条件内嵌值
    ///
    /// 验证无参数版本（build_select）对参数化条件的处理：直接内嵌转义值。
    #[test]
    fn test_p02_build_where_clause_inlines_value() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let sql = QueryBuilder::<TestModel>::new(dialect)
            .table("users")
            .where_eq("name", Value::String("alice".into()))
            .build_select();
        // 无参数版本应内嵌值（依赖 to_param_with_dialect 转义）
        assert!(
            sql.contains("`name` = "),
            "无参数版本应含 WHERE 条件: {}",
            sql
        );
        // 不应含 `?`（无参数版本）
        assert!(
            !sql.contains("`name` = ?"),
            "无参数版本不应使用 ? 占位符: {}",
            sql
        );
    }

    /// 行为级测试 L3-20：is_soft_delete_disabled 反映状态
    #[test]
    fn test_p01_is_soft_delete_disabled_flag() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<SoftDeleteModel>::new(dialect);
        assert!(
            !builder.is_soft_delete_disabled(),
            "默认应启用软删除过滤"
        );
        let builder = QueryBuilder::<SoftDeleteModel>::new(get_dialect(DbType::MySQL).unwrap())
            .without_soft_delete();
        assert!(
            builder.is_soft_delete_disabled(),
            "without_soft_delete 后应反映禁用状态"
        );
    }

    // ==================== P0-3 多租户自动过滤行为测试 ====================

    /// 多租户测试模型：实现 tenant_field() 返回 "tenant_id"
    struct TenantModel;
    impl Model for TenantModel {
        type PrimaryKey = i64;

        fn table_name() -> &'static str {
            "orders"
        }

        fn pk(&self) -> Self::PrimaryKey {
            1
        }

        fn set_pk(&mut self, _pk: Self::PrimaryKey) {}

        fn tenant_field() -> Option<&'static str> {
            Some("tenant_id")
        }
    }

    /// 同时实现软删除 + 多租户的模型
    struct SoftDeleteAndTenantModel;
    impl Model for SoftDeleteAndTenantModel {
        type PrimaryKey = i64;

        fn table_name() -> &'static str {
            "documents"
        }

        fn pk(&self) -> Self::PrimaryKey {
            1
        }

        fn set_pk(&mut self, _pk: Self::PrimaryKey) {}

        fn soft_delete_field() -> Option<&'static str> {
            Some("deleted_at")
        }

        fn tenant_field() -> Option<&'static str> {
            Some("tenant_id")
        }
    }

    /// 行为级测试 L3-21：多租户模型 + with_tenant_id 自动追加 WHERE tenant_id = ?
    ///
    /// 用户视角：设置租户 ID 后，查询自动过滤当前租户数据。
    #[test]
    fn test_p03_tenant_select_auto_filter() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let (sql, params) = QueryBuilder::<TenantModel>::new(dialect)
            .table("orders")
            .with_tenant_id(42)
            .build_select_with_params();
        assert!(
            sql.contains("`tenant_id` = ?"),
            "多租户模型应自动追加 tenant_id = ?: {}",
            sql
        );
        assert_eq!(params.len(), 1, "应有 1 个参数（tenant_id 值）");
        assert_eq!(params[0], Value::I64(42));
    }

    /// 行为级测试 L3-22：多租户模型 + 用户 WHERE 条件 + 租户条件
    #[test]
    fn test_p03_tenant_select_with_user_where() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let (sql, params) = QueryBuilder::<TenantModel>::new(dialect)
            .table("orders")
            .with_tenant_id(7)
            .where_eq("status", Value::String("active".into()))
            .build_select_with_params();
        assert!(sql.contains("`status` = ?"), "用户条件应保留: {}", sql);
        assert!(
            sql.contains("`tenant_id` = ?"),
            "租户条件应自动追加: {}",
            sql
        );
        assert_eq!(params.len(), 2, "应有 2 个参数");
        // 第 1 个为用户 where_eq 的值，第 2 个为 tenant_id
        assert_eq!(params[0], Value::String("active".into()));
        assert_eq!(params[1], Value::I64(7));
    }

    /// 行为级测试 L3-23：without_tenant() 临时禁用租户过滤
    ///
    /// 用户视角：管理员跨租户查询时禁用自动过滤。
    #[test]
    fn test_p03_tenant_without_tenant() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let (sql, params) = QueryBuilder::<TenantModel>::new(dialect)
            .table("orders")
            .with_tenant_id(42)
            .without_tenant()
            .build_select_with_params();
        assert!(
            !sql.contains("`tenant_id` = ?"),
            "without_tenant 应禁用过滤: {}",
            sql
        );
        assert_eq!(params.len(), 0, "不应有租户参数");
    }

    /// 行为级测试 L3-24：多租户模型 build_delete 自动追加租户条件
    ///
    /// 用户视角：删除操作自动限定在当前租户，防止跨租户删除。
    #[test]
    fn test_p03_tenant_delete_auto_filter() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let (sql, params) = QueryBuilder::<TenantModel>::new(dialect)
            .table("orders")
            .with_tenant_id(99)
            .where_eq("id", Value::I64(1))
            .build_delete_with_params();
        assert!(
            sql.contains("`tenant_id` = ?"),
            "删除应自动追加租户条件: {}",
            sql
        );
        // 2 个参数：where_eq(id=1) + tenant_id=99
        assert_eq!(params.len(), 2);
        assert_eq!(params[0], Value::I64(1));
        assert_eq!(params[1], Value::I64(99));
    }

    /// 行为级测试 L3-25：多租户模型 build_update 自动追加租户条件
    #[test]
    fn test_p03_tenant_update_auto_filter() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let mut data = std::collections::HashMap::new();
        data.insert("status".to_string(), Value::String("shipped".into()));
        let (sql, params) = QueryBuilder::<TenantModel>::new(dialect)
            .table("orders")
            .with_tenant_id(5)
            .where_eq("id", Value::I64(10))
            .build_update_with_params(&data);
        assert!(
            sql.contains("`tenant_id` = ?"),
            "更新应自动追加租户条件: {}",
            sql
        );
        // 3 个参数：SET status + WHERE id + tenant_id
        assert_eq!(params.len(), 3);
        // 最后一个应为 tenant_id
        assert_eq!(params[2], Value::I64(5));
    }

    /// 行为级测试 L3-26：多租户模型 build_count 自动追加租户条件
    #[test]
    fn test_p03_tenant_count_auto_filter() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let sql = QueryBuilder::<TenantModel>::new(dialect)
            .table("orders")
            .with_tenant_id(42)
            .build_count();
        assert!(
            sql.contains("`tenant_id` = 42"),
            "build_count 应追加租户条件（无参数版本内嵌值）: {}",
            sql
        );
    }

    /// 行为级测试 L3-27：非多租户模型 TestModel 不追加租户条件
    ///
    /// 用户视角：未启用多租户的模型行为不变。
    #[test]
    fn test_p03_non_tenant_model_unchanged() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        // 即使设置了 with_tenant_id，非多租户模型也不应追加
        let (sql, params) = QueryBuilder::<TestModel>::new(dialect)
            .table("users")
            .with_tenant_id(42)
            .build_select_with_params();
        assert!(
            !sql.contains("tenant_id"),
            "非多租户模型不应追加 tenant_id: {}",
            sql
        );
        assert_eq!(params.len(), 0);
    }

    /// 行为级测试 L3-28：多租户模型未设置 tenant_id 时不追加条件
    ///
    /// 用户视角：未设置租户 ID 时，查询不追加租户过滤（允许跨租户，需调用方保证安全）。
    #[test]
    fn test_p03_tenant_no_id_no_filter() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let (sql, params) = QueryBuilder::<TenantModel>::new(dialect)
            .table("orders")
            .build_select_with_params();
        assert!(
            !sql.contains("tenant_id"),
            "未设置 tenant_id 时不应追加过滤: {}",
            sql
        );
        assert_eq!(params.len(), 0);
    }

    /// 行为级测试 L3-29：软删除 + 多租户组合，两个条件同时追加
    ///
    /// 用户视角：同时启用软删除和多租户时，查询自动追加两个条件。
    #[test]
    fn test_p03_soft_delete_and_tenant_combined() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let (sql, params) = QueryBuilder::<SoftDeleteAndTenantModel>::new(dialect)
            .table("documents")
            .with_tenant_id(100)
            .where_eq("title", Value::String("report".into()))
            .build_select_with_params();
        // 软删除条件
        assert!(
            sql.contains("`deleted_at` IS NULL"),
            "应追加软删除条件: {}",
            sql
        );
        // 租户条件
        assert!(
            sql.contains("`tenant_id` = ?"),
            "应追加租户条件: {}",
            sql
        );
        // 用户条件
        assert!(
            sql.contains("`title` = ?"),
            "用户条件应保留: {}",
            sql
        );
        // 2 个参数：where_eq(title) + tenant_id（软删除 IS NULL 无参数）
        assert_eq!(params.len(), 2);
        assert_eq!(params[0], Value::String("report".into()));
        assert_eq!(params[1], Value::I64(100));
    }

    /// 行为级测试 L3-30：without_tenant + without_soft_delete 同时禁用
    #[test]
    fn test_p03_without_tenant_and_soft_delete() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let (sql, params) = QueryBuilder::<SoftDeleteAndTenantModel>::new(dialect)
            .table("documents")
            .with_tenant_id(100)
            .without_tenant()
            .without_soft_delete()
            .build_select_with_params();
        assert!(
            !sql.contains("`deleted_at` IS NULL"),
            "应禁用软删除: {}",
            sql
        );
        assert!(
            !sql.contains("`tenant_id` = ?"),
            "应禁用租户: {}",
            sql
        );
        assert_eq!(params.len(), 0);
    }

    /// 行为级测试 L3-31：is_tenant_disabled 反映状态
    #[test]
    fn test_p03_is_tenant_disabled_flag() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let builder = QueryBuilder::<TenantModel>::new(dialect);
        assert!(
            !builder.is_tenant_disabled(),
            "默认应启用租户过滤"
        );
        let builder = QueryBuilder::<TenantModel>::new(get_dialect(DbType::MySQL).unwrap())
            .with_tenant_id(1)
            .without_tenant();
        assert!(
            builder.is_tenant_disabled(),
            "without_tenant 后应反映禁用状态"
        );
    }

    /// 行为级测试 L3-32：build_force_delete 保留租户条件（防止跨租户物理删除）
    ///
    /// 用户视角：物理删除也应受租户隔离约束，跨租户操作需显式 without_tenant()。
    #[test]
    fn test_p03_tenant_force_delete_keeps_tenant_filter() {
        let dialect = get_dialect(DbType::MySQL).unwrap();
        let (sql, params) = QueryBuilder::<TenantModel>::new(dialect)
            .table("orders")
            .with_tenant_id(42)
            .where_eq("id", Value::I64(999))
            .build_force_delete_with_params();
        // 物理删除不应追加软删除（TenantModel 未实现软删除，无影响）
        // 但应保留租户条件
        assert!(
            sql.contains("`tenant_id` = ?"),
            "物理删除应保留租户条件: {}",
            sql
        );
        assert_eq!(params.len(), 2);
        assert_eq!(params[0], Value::I64(999));
        assert_eq!(params[1], Value::I64(42));
    }
}
