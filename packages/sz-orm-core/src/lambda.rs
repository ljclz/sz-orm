//! Lambda 类型安全 Wrapper
//!
//! 对应文档 6.8 节改进项 38（Lambda 类型安全 Wrapper）。
//!
//! # 核心概念
//!
//! - **Column**：字段标记 trait，关联 Model 类型 M，提供字段名与表名
//! - **`LambdaWrapper<M>`**：类型安全的查询构造器，所有字段引用都通过 `Column` 类型而非 `&str`
//! - **define_columns!**：宏，为 Model 定义所有字段的类型安全标记
//!
//! # 设计灵感
//!
//! - MyBatis-Plus `LambdaQueryWrapper` / `LambdaUpdateWrapper`
//! - JOOQ 类型安全 DSL
//! - Diesel 强类型 schema
//! - SeaORM `Column` trait
//!
//! # 优势
//!
//! 1. **编译期字段名检查**：拼写错误直接编译失败
//! 2. **IDE 自动补全**：`UserColumns::` 后可补全所有字段
//! 3. **重构友好**：字段重命名后所有引用处编译失败，便于定位修改
//! 4. **表名隔离**：不同 Model 的字段不会混淆
//!
//! # 使用示例
//!
//! ```
//! use sz_orm_core::lambda::{LambdaWrapper, Column};
//! use sz_orm_core::define_columns;
//! use sz_orm_core::Value;
//!
//! // 1. 定义 Model（示例）
//! struct User;
//!
//! // 2. 为 User 定义字段标记
//! define_columns! {
//!     UserColumns for User table = "users" {
//!         Id => "id",
//!         Name => "name",
//!         Age => "age",
//!     }
//! }
//!
//! // 3. 使用 LambdaWrapper 构造类型安全查询
//! let mut wrapper = LambdaWrapper::<User>::new("users");
//! wrapper
//!     .select(UserColumns::Id)
//!     .select(UserColumns::Name)
//!     .eq(UserColumns::Id, Value::I64(1))
//!     .gt(UserColumns::Age, Value::I64(18));
//!
//! let sql = wrapper.build_select();
//! assert!(sql.contains("SELECT `id`, `name`"));
//! assert!(sql.contains("FROM `users`"));
//! assert!(sql.contains("`id` = 1"));
//! assert!(sql.contains("`age` > 18"));
//! ```

use crate::dialect::{Dialect, MySqlDialect};
use crate::Value;
use std::marker::PhantomData;

// ============================================================================
// Column trait — 字段标记
// ============================================================================

/// 字段标记 trait
///
/// 实现此 trait 的类型作为 Model 字段的类型安全引用。
/// 一个 `Column<M>` 实例携带：
/// - 字段名（`name()`）
/// - 所属表名（`table()`，从 Model 关联）
///
/// # 实现方式
///
/// 通常通过 `define_columns!` 宏自动生成实现，无需手动实现。
pub trait Column<M>: Send + Sync + Clone {
    /// 返回字段名
    fn name(&self) -> &'static str;

    /// 返回字段所属的表名
    fn table(&self) -> &'static str;
}

// ============================================================================
// WhereClause — WHERE 条件子句
// ============================================================================

/// WHERE 条件子句（内部表示）
#[derive(Debug, Clone)]
pub enum WhereClause {
    /// `col = value`
    Eq(String, Value),
    /// `col != value`
    Ne(String, Value),
    /// `col > value`
    Gt(String, Value),
    /// `col >= value`
    Ge(String, Value),
    /// `col < value`
    Lt(String, Value),
    /// `col <= value`
    Le(String, Value),
    /// `col LIKE value`
    Like(String, Value),
    /// `col IS NULL`
    IsNull(String),
    /// `col IS NOT NULL`
    IsNotNull(String),
    /// `col IN (v1, v2, ...)`
    In(String, Vec<Value>),
    /// `col NOT IN (v1, v2, ...)`
    NotIn(String, Vec<Value>),
    /// `col BETWEEN v1 AND v2`
    Between(String, Value, Value),
    /// 原始 SQL（用于 OR 等复杂条件）
    Raw(String),
}

impl WhereClause {
    /// 渲染为 SQL 片段（不带前缀 AND/OR）
    fn render(&self, dialect: &dyn Dialect) -> String {
        match self {
            // v0.2.2 修复 H-1：使用方言感知的转义
            WhereClause::Eq(col, v) => format!(
                "{} = {}",
                dialect.quote(col),
                v.to_param_with_dialect(dialect)
            ),
            WhereClause::Ne(col, v) => format!(
                "{} != {}",
                dialect.quote(col),
                v.to_param_with_dialect(dialect)
            ),
            WhereClause::Gt(col, v) => format!(
                "{} > {}",
                dialect.quote(col),
                v.to_param_with_dialect(dialect)
            ),
            WhereClause::Ge(col, v) => format!(
                "{} >= {}",
                dialect.quote(col),
                v.to_param_with_dialect(dialect)
            ),
            WhereClause::Lt(col, v) => format!(
                "{} < {}",
                dialect.quote(col),
                v.to_param_with_dialect(dialect)
            ),
            WhereClause::Le(col, v) => format!(
                "{} <= {}",
                dialect.quote(col),
                v.to_param_with_dialect(dialect)
            ),
            WhereClause::Like(col, v) => format!(
                "{} LIKE {}",
                dialect.quote(col),
                v.to_param_with_dialect(dialect)
            ),
            WhereClause::IsNull(col) => format!("{} IS NULL", dialect.quote(col)),
            WhereClause::IsNotNull(col) => format!("{} IS NOT NULL", dialect.quote(col)),
            WhereClause::In(col, vs) => {
                let values: Vec<String> = vs
                    .iter()
                    .map(|v| v.to_param_with_dialect(dialect).to_string())
                    .collect();
                format!("{} IN ({})", dialect.quote(col), values.join(", "))
            }
            WhereClause::NotIn(col, vs) => {
                let values: Vec<String> = vs
                    .iter()
                    .map(|v| v.to_param_with_dialect(dialect).to_string())
                    .collect();
                format!("{} NOT IN ({})", dialect.quote(col), values.join(", "))
            }
            WhereClause::Between(col, a, b) => format!(
                "{} BETWEEN {} AND {}",
                dialect.quote(col),
                a.to_param_with_dialect(dialect),
                b.to_param_with_dialect(dialect)
            ),
            WhereClause::Raw(sql) => sql.clone(),
        }
    }
}

// ============================================================================
// OrderBy — 排序子句
// ============================================================================

/// 排序方向
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderDirection {
    /// 升序
    Asc,
    /// 降序
    Desc,
}

/// 排序子句
#[derive(Debug, Clone)]
pub struct OrderBy {
    /// 字段名
    pub column: String,
    /// 排序方向
    pub direction: OrderDirection,
}

// ============================================================================
// LambdaWrapper — 类型安全查询构造器
// ============================================================================

/// Lambda 类型安全查询构造器
///
/// 泛型参数 `M` 是 Model 类型（仅用于类型隔离，不实际存储实例）。
///
/// # 字段引用方式
///
/// 与原始 `QueryBuilder` 使用 `&str` 字段名不同，`LambdaWrapper` 接受 `Column<M>` 实例，
/// 从而在编译期检查字段拼写错误。
///
/// # 示例
///
/// ```
/// use sz_orm_core::lambda::{LambdaWrapper, Column};
/// use sz_orm_core::define_columns;
/// use sz_orm_core::Value;
///
/// struct User;
/// define_columns! {
///     UserColumns for User table = "users" {
///         Id => "id",
///         Name => "name",
///     }
/// }
///
/// let mut w = LambdaWrapper::<User>::new("users");
/// w.eq(UserColumns::Id, Value::I64(1));
/// ```
pub struct LambdaWrapper<M> {
    /// 表名
    table: String,
    /// SELECT 字段列表（空表示 SELECT *）
    selects: Vec<String>,
    /// WHERE 条件列表（AND 连接）
    wheres: Vec<WhereClause>,
    /// ORDER BY 子句
    orders: Vec<OrderBy>,
    /// LIMIT
    limit: Option<u64>,
    /// OFFSET
    offset: Option<u64>,
    /// 数据库方言
    dialect: Box<dyn Dialect>,
    _marker: PhantomData<M>,
}

impl<M> LambdaWrapper<M> {
    /// 创建 LambdaWrapper，默认使用 MySQL 方言
    pub fn new(table: impl Into<String>) -> Self {
        Self {
            table: table.into(),
            selects: Vec::new(),
            wheres: Vec::new(),
            orders: Vec::new(),
            limit: None,
            offset: None,
            dialect: Box::new(MySqlDialect),
            _marker: PhantomData,
        }
    }

    /// 创建 LambdaWrapper 并指定方言
    pub fn with_dialect(table: impl Into<String>, dialect: Box<dyn Dialect>) -> Self {
        Self {
            table: table.into(),
            selects: Vec::new(),
            wheres: Vec::new(),
            orders: Vec::new(),
            limit: None,
            offset: None,
            dialect,
            _marker: PhantomData,
        }
    }

    // -------------------- SELECT 字段 --------------------

    /// 添加 SELECT 字段（类型安全）
    pub fn select<C: Column<M>>(&mut self, col: C) -> &mut Self {
        self.selects.push(col.name().to_string());
        self
    }

    /// 批量添加 SELECT 字段
    pub fn select_many<C: Column<M>>(&mut self, cols: &[C]) -> &mut Self {
        for c in cols {
            self.selects.push(c.name().to_string());
        }
        self
    }

    /// SELECT *（清空已有字段选择）
    pub fn select_all(&mut self) -> &mut Self {
        self.selects.clear();
        self
    }

    // -------------------- WHERE 条件 --------------------

    /// `col = value`
    pub fn eq<C: Column<M>>(&mut self, col: C, value: Value) -> &mut Self {
        self.wheres
            .push(WhereClause::Eq(col.name().to_string(), value));
        self
    }

    /// `col != value`
    pub fn ne<C: Column<M>>(&mut self, col: C, value: Value) -> &mut Self {
        self.wheres
            .push(WhereClause::Ne(col.name().to_string(), value));
        self
    }

    /// `col > value`
    pub fn gt<C: Column<M>>(&mut self, col: C, value: Value) -> &mut Self {
        self.wheres
            .push(WhereClause::Gt(col.name().to_string(), value));
        self
    }

    /// `col >= value`
    pub fn ge<C: Column<M>>(&mut self, col: C, value: Value) -> &mut Self {
        self.wheres
            .push(WhereClause::Ge(col.name().to_string(), value));
        self
    }

    /// `col < value`
    pub fn lt<C: Column<M>>(&mut self, col: C, value: Value) -> &mut Self {
        self.wheres
            .push(WhereClause::Lt(col.name().to_string(), value));
        self
    }

    /// `col <= value`
    pub fn le<C: Column<M>>(&mut self, col: C, value: Value) -> &mut Self {
        self.wheres
            .push(WhereClause::Le(col.name().to_string(), value));
        self
    }

    /// `col LIKE value`
    pub fn like<C: Column<M>>(&mut self, col: C, value: Value) -> &mut Self {
        self.wheres
            .push(WhereClause::Like(col.name().to_string(), value));
        self
    }

    /// `col IS NULL`
    pub fn is_null<C: Column<M>>(&mut self, col: C) -> &mut Self {
        self.wheres
            .push(WhereClause::IsNull(col.name().to_string()));
        self
    }

    /// `col IS NOT NULL`
    pub fn is_not_null<C: Column<M>>(&mut self, col: C) -> &mut Self {
        self.wheres
            .push(WhereClause::IsNotNull(col.name().to_string()));
        self
    }

    /// `col IN (v1, v2, ...)`
    pub fn r#in<C: Column<M>>(&mut self, col: C, values: Vec<Value>) -> &mut Self {
        self.wheres
            .push(WhereClause::In(col.name().to_string(), values));
        self
    }

    /// `col NOT IN (v1, v2, ...)`
    pub fn not_in<C: Column<M>>(&mut self, col: C, values: Vec<Value>) -> &mut Self {
        self.wheres
            .push(WhereClause::NotIn(col.name().to_string(), values));
        self
    }

    /// `col BETWEEN a AND b`
    pub fn between<C: Column<M>>(&mut self, col: C, a: Value, b: Value) -> &mut Self {
        self.wheres
            .push(WhereClause::Between(col.name().to_string(), a, b));
        self
    }

    /// 追加原始 SQL WHERE 条件（用于 OR 等复杂场景）
    ///
    /// # 安全警告
    ///
    /// 此方法是 escape hatch，传入的 SQL 会**原样拼接**到最终 SQL 中。
    /// **严禁将用户输入直接拼接**到 `sql` 参数中（会引入 SQL 注入风险）。
    /// 若需使用用户输入，请改用 `eq` / `ne` / `lt` 等参数化方法。
    pub fn raw_where(&mut self, sql: impl Into<String>) -> &mut Self {
        self.wheres.push(WhereClause::Raw(sql.into()));
        self
    }

    // -------------------- ORDER BY / LIMIT / OFFSET --------------------

    /// 添加升序排序
    pub fn order_by_asc<C: Column<M>>(&mut self, col: C) -> &mut Self {
        self.orders.push(OrderBy {
            column: col.name().to_string(),
            direction: OrderDirection::Asc,
        });
        self
    }

    /// 添加降序排序
    pub fn order_by_desc<C: Column<M>>(&mut self, col: C) -> &mut Self {
        self.orders.push(OrderBy {
            column: col.name().to_string(),
            direction: OrderDirection::Desc,
        });
        self
    }

    /// 设置 LIMIT
    pub fn limit(&mut self, n: u64) -> &mut Self {
        self.limit = Some(n);
        self
    }

    /// 设置 OFFSET
    pub fn offset(&mut self, n: u64) -> &mut Self {
        self.offset = Some(n);
        self
    }

    /// 分页（设置 LIMIT + OFFSET）
    ///
    /// page 从 1 开始计数，page=1 时无 OFFSET（从第 0 条开始）。
    pub fn page(&mut self, page: u64, page_size: u64) -> &mut Self {
        self.limit = Some(page_size);
        if page > 1 {
            self.offset = Some((page - 1) * page_size);
        } else {
            self.offset = None;
        }
        self
    }

    // -------------------- SQL 生成 --------------------

    /// 生成 SELECT SQL
    pub fn build_select(&self) -> String {
        let quoted_table = self.dialect.quote(&self.table);

        // SELECT 字段
        let select_sql = if self.selects.is_empty() {
            "*".to_string()
        } else {
            self.selects
                .iter()
                .map(|c| self.dialect.quote(c))
                .collect::<Vec<_>>()
                .join(", ")
        };

        let mut sql = format!("SELECT {} FROM {}", select_sql, quoted_table);

        // WHERE
        if !self.wheres.is_empty() {
            let conditions: Vec<String> = self
                .wheres
                .iter()
                .map(|w| w.render(self.dialect.as_ref()))
                .collect();
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }

        // ORDER BY
        if !self.orders.is_empty() {
            let orders: Vec<String> = self
                .orders
                .iter()
                .map(|o| {
                    let dir = match o.direction {
                        OrderDirection::Asc => "ASC",
                        OrderDirection::Desc => "DESC",
                    };
                    format!("{} {}", self.dialect.quote(&o.column), dir)
                })
                .collect();
            sql.push_str(" ORDER BY ");
            sql.push_str(&orders.join(", "));
        }

        // LIMIT / OFFSET
        if let Some(l) = self.limit {
            sql.push_str(&format!(" LIMIT {}", l));
        }
        if let Some(o) = self.offset {
            sql.push_str(&format!(" OFFSET {}", o));
        }

        sql
    }

    /// 生成 COUNT SQL（SELECT COUNT(*) FROM ... WHERE ...）
    pub fn build_count(&self) -> String {
        let quoted_table = self.dialect.quote(&self.table);
        let mut sql = format!("SELECT COUNT(*) FROM {}", quoted_table);

        if !self.wheres.is_empty() {
            let conditions: Vec<String> = self
                .wheres
                .iter()
                .map(|w| w.render(self.dialect.as_ref()))
                .collect();
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }

        sql
    }

    /// 生成 EXISTS SQL（SELECT EXISTS(SELECT 1 FROM ... WHERE ...) AS exists_flag）
    pub fn build_exists(&self) -> String {
        let inner = self.build_select();
        // 把 SELECT 字段部分替换为 SELECT 1
        let inner = if let Some(pos) = inner.find(" FROM ") {
            format!("SELECT 1{}", &inner[pos..])
        } else {
            inner
        };
        format!("SELECT EXISTS({}) AS exists_flag", inner)
    }

    /// 生成 DELETE SQL
    pub fn build_delete(&self) -> String {
        let quoted_table = self.dialect.quote(&self.table);
        let mut sql = format!("DELETE FROM {}", quoted_table);

        if !self.wheres.is_empty() {
            let conditions: Vec<String> = self
                .wheres
                .iter()
                .map(|w| w.render(self.dialect.as_ref()))
                .collect();
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }

        sql
    }

    // -------------------- 状态查询 --------------------

    /// 返回当前 WHERE 条件数量
    pub fn where_count(&self) -> usize {
        self.wheres.len()
    }

    /// 返回当前 SELECT 字段数量
    pub fn select_count(&self) -> usize {
        self.selects.len()
    }

    /// 返回表名
    pub fn table(&self) -> &str {
        &self.table
    }

    /// 重置所有条件（保留表名和方言）
    pub fn reset(&mut self) -> &mut Self {
        self.selects.clear();
        self.wheres.clear();
        self.orders.clear();
        self.limit = None;
        self.offset = None;
        self
    }
}

// ============================================================================
// define_columns! 宏 — 为 Model 定义字段标记
// ============================================================================

/// 为 Model 定义一组类型安全的字段标记
///
/// # 示例
///
/// ```
/// use sz_orm_core::define_columns;
///
/// struct User;
///
/// define_columns! {
///     UserColumns for User table = "users" {
///         Id => "id",
///         Name => "name",
///         Age => "age",
///     }
/// }
/// ```
///
/// 生成如下代码：
///
/// ```rust,ignore
/// #[derive(Clone, Copy)]
/// pub struct UserColumns;
///
/// impl UserColumns {
///     pub const Id: UserColumn = UserColumn { name: "id", table: "users" };
///     pub const Name: UserColumn = UserColumn { name: "name", table: "users" };
///     pub const Age: UserColumn = UserColumn { name: "age", table: "users" };
/// }
///
/// #[derive(Clone, Copy)]
/// pub struct UserColumn {
///     pub name: &'static str,
///     pub table: &'static str,
/// }
///
/// impl Column<User> for UserColumn {
///     fn name(&self) -> &'static str { self.name }
///     fn table(&self) -> &'static str { self.table }
/// }
/// ```
#[macro_export]
macro_rules! define_columns {
    (
        $columns_struct:ident for $model:ident table = $table:literal {
            $( $field:ident => $name:literal ),* $(,)?
        }
    ) => {
        /// 字段标记类型（每个 Model 一组）
        ///
        /// 通过 `ModelColumns::FieldName` 引用对应字段，编译期检查拼写错误。
        #[derive(Debug, Clone, Copy)]
        pub struct $columns_struct {
            /// 字段名
            pub name: &'static str,
            /// 表名
            pub table: &'static str,
        }

        impl $crate::lambda::Column<$model> for $columns_struct {
            fn name(&self) -> &'static str {
                self.name
            }

            fn table(&self) -> &'static str {
                self.table
            }
        }

        impl $columns_struct {
            // 允许 PascalCase 常量名（如 Id、Name），符合字段命名习惯
            $(
                #[allow(non_upper_case_globals, dead_code)]
                pub const $field: $columns_struct = $columns_struct { name: $name, table: $table };
            )*
        }
    };
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dialect::PostgreSqlDialect;
    use crate::get_dialect;
    use crate::DbType;
    // 引入宏（#[macro_export] 将宏挂到 crate root，本地模块需显式导入）
    use crate::define_columns;

    // 测试用 Model
    struct User;

    // 为 User 定义字段标记
    define_columns! {
        UserColumns for User table = "users" {
            Id => "id",
            Name => "name",
            Age => "age",
            Email => "email",
        }
    }

    // 另一个 Model 用于测试表名隔离
    struct Order;

    define_columns! {
        OrderColumns for Order table = "orders" {
            OrderId => "order_id",
            UserId => "user_id",
            Total => "total",
        }
    }

    // ===== Column trait 测试 =====

    #[test]
    fn test_column_name_and_table() {
        assert_eq!(UserColumns::Id.name(), "id");
        assert_eq!(UserColumns::Id.table(), "users");
        assert_eq!(UserColumns::Name.name(), "name");
        assert_eq!(UserColumns::Age.name(), "age");
        assert_eq!(UserColumns::Email.name(), "email");
    }

    #[test]
    fn test_column_for_different_models() {
        assert_eq!(OrderColumns::OrderId.name(), "order_id");
        assert_eq!(OrderColumns::OrderId.table(), "orders");
        assert_eq!(OrderColumns::UserId.name(), "user_id");
    }

    // ===== LambdaWrapper 基础测试 =====

    #[test]
    fn test_new_wrapper() {
        let w = LambdaWrapper::<User>::new("users");
        assert_eq!(w.table(), "users");
        assert_eq!(w.where_count(), 0);
        assert_eq!(w.select_count(), 0);
    }

    #[test]
    fn test_select_single() {
        let mut w = LambdaWrapper::<User>::new("users");
        w.select(UserColumns::Id);
        let sql = w.build_select();
        assert!(sql.contains("SELECT `id` FROM `users`"));
    }

    #[test]
    fn test_select_multiple() {
        let mut w = LambdaWrapper::<User>::new("users");
        w.select(UserColumns::Id)
            .select(UserColumns::Name)
            .select(UserColumns::Age);
        let sql = w.build_select();
        assert!(sql.contains("`id`, `name`, `age`"));
    }

    #[test]
    fn test_select_many() {
        let mut w = LambdaWrapper::<User>::new("users");
        w.select_many(&[UserColumns::Id, UserColumns::Name, UserColumns::Age]);
        let sql = w.build_select();
        assert!(sql.contains("`id`, `name`, `age`"));
    }

    #[test]
    fn test_select_all_clears_selects() {
        let mut w = LambdaWrapper::<User>::new("users");
        w.select(UserColumns::Id);
        assert_eq!(w.select_count(), 1);
        w.select_all();
        assert_eq!(w.select_count(), 0);
        let sql = w.build_select();
        assert!(sql.contains("SELECT * FROM"));
    }

    #[test]
    fn test_default_select_is_star() {
        let w = LambdaWrapper::<User>::new("users");
        let sql = w.build_select();
        assert!(sql.contains("SELECT * FROM `users`"));
    }

    // ===== WHERE 条件测试 =====

    #[test]
    fn test_where_eq() {
        let mut w = LambdaWrapper::<User>::new("users");
        w.eq(UserColumns::Id, Value::I64(1));
        let sql = w.build_select();
        assert!(sql.contains("WHERE `id` = 1"));
    }

    #[test]
    fn test_where_ne() {
        let mut w = LambdaWrapper::<User>::new("users");
        w.ne(UserColumns::Id, Value::I64(1));
        let sql = w.build_select();
        assert!(sql.contains("`id` != 1"));
    }

    #[test]
    fn test_where_gt_ge_lt_le() {
        let mut w = LambdaWrapper::<User>::new("users");
        w.gt(UserColumns::Age, Value::I64(18))
            .ge(UserColumns::Age, Value::I64(20))
            .lt(UserColumns::Age, Value::I64(65))
            .le(UserColumns::Age, Value::I64(60));
        let sql = w.build_select();
        assert!(sql.contains("`age` > 18"));
        assert!(sql.contains("`age` >= 20"));
        assert!(sql.contains("`age` < 65"));
        assert!(sql.contains("`age` <= 60"));
    }

    #[test]
    fn test_where_like() {
        let mut w = LambdaWrapper::<User>::new("users");
        w.like(UserColumns::Name, Value::String("%alice%".to_string()));
        let sql = w.build_select();
        assert!(sql.contains("`name` LIKE '%alice%'"));
    }

    #[test]
    fn test_where_is_null() {
        let mut w = LambdaWrapper::<User>::new("users");
        w.is_null(UserColumns::Email);
        let sql = w.build_select();
        assert!(sql.contains("`email` IS NULL"));
    }

    #[test]
    fn test_where_is_not_null() {
        let mut w = LambdaWrapper::<User>::new("users");
        w.is_not_null(UserColumns::Email);
        let sql = w.build_select();
        assert!(sql.contains("`email` IS NOT NULL"));
    }

    #[test]
    fn test_where_in() {
        let mut w = LambdaWrapper::<User>::new("users");
        w.r#in(
            UserColumns::Id,
            vec![Value::I64(1), Value::I64(2), Value::I64(3)],
        );
        let sql = w.build_select();
        assert!(sql.contains("`id` IN (1, 2, 3)"));
    }

    #[test]
    fn test_where_not_in() {
        let mut w = LambdaWrapper::<User>::new("users");
        w.not_in(UserColumns::Id, vec![Value::I64(1), Value::I64(2)]);
        let sql = w.build_select();
        assert!(sql.contains("`id` NOT IN (1, 2)"));
    }

    #[test]
    fn test_where_between() {
        let mut w = LambdaWrapper::<User>::new("users");
        w.between(UserColumns::Age, Value::I64(18), Value::I64(65));
        let sql = w.build_select();
        assert!(sql.contains("`age` BETWEEN 18 AND 65"));
    }

    #[test]
    fn test_where_multiple_anded() {
        let mut w = LambdaWrapper::<User>::new("users");
        w.eq(UserColumns::Id, Value::I64(1))
            .gt(UserColumns::Age, Value::I64(18))
            .like(UserColumns::Name, Value::String("alice%".to_string()));
        let sql = w.build_select();
        assert!(sql.contains("`id` = 1"));
        assert!(sql.contains("`age` > 18"));
        assert!(sql.contains("`name` LIKE 'alice%'"));
        // 所有条件用 AND 连接
        assert!(sql.contains(" AND "));
    }

    #[test]
    fn test_where_raw() {
        let mut w = LambdaWrapper::<User>::new("users");
        w.raw_where("name = 'alice' OR name = 'bob'");
        let sql = w.build_select();
        assert!(sql.contains("name = 'alice' OR name = 'bob'"));
    }

    // ===== ORDER BY / LIMIT / OFFSET 测试 =====

    #[test]
    fn test_order_by_asc() {
        let mut w = LambdaWrapper::<User>::new("users");
        w.order_by_asc(UserColumns::Name);
        let sql = w.build_select();
        assert!(sql.contains("ORDER BY `name` ASC"));
    }

    #[test]
    fn test_order_by_desc() {
        let mut w = LambdaWrapper::<User>::new("users");
        w.order_by_desc(UserColumns::Id);
        let sql = w.build_select();
        assert!(sql.contains("ORDER BY `id` DESC"));
    }

    #[test]
    fn test_order_by_multiple() {
        let mut w = LambdaWrapper::<User>::new("users");
        w.order_by_asc(UserColumns::Name)
            .order_by_desc(UserColumns::Id);
        let sql = w.build_select();
        assert!(sql.contains("ORDER BY `name` ASC, `id` DESC"));
    }

    #[test]
    fn test_limit() {
        let mut w = LambdaWrapper::<User>::new("users");
        w.limit(10);
        let sql = w.build_select();
        assert!(sql.contains("LIMIT 10"));
    }

    #[test]
    fn test_offset() {
        let mut w = LambdaWrapper::<User>::new("users");
        w.limit(10).offset(20);
        let sql = w.build_select();
        assert!(sql.contains("LIMIT 10"));
        assert!(sql.contains("OFFSET 20"));
    }

    #[test]
    fn test_page() {
        let mut w = LambdaWrapper::<User>::new("users");
        w.page(3, 20); // 第 3 页，每页 20 条
        let sql = w.build_select();
        assert!(sql.contains("LIMIT 20"));
        assert!(sql.contains("OFFSET 40")); // (3-1) * 20
    }

    #[test]
    fn test_page_1_no_offset() {
        let mut w = LambdaWrapper::<User>::new("users");
        w.page(1, 10);
        let sql = w.build_select();
        assert!(sql.contains("LIMIT 10"));
        assert!(!sql.contains("OFFSET")); // 第 1 页无 OFFSET
    }

    // ===== SQL 生成测试 =====

    #[test]
    fn test_build_count() {
        let mut w = LambdaWrapper::<User>::new("users");
        w.gt(UserColumns::Age, Value::I64(18));
        let sql = w.build_count();
        assert!(sql.contains("SELECT COUNT(*) FROM `users`"));
        assert!(sql.contains("`age` > 18"));
        // 不应包含 ORDER BY / LIMIT
        assert!(!sql.contains("ORDER BY"));
        assert!(!sql.contains("LIMIT"));
    }

    #[test]
    fn test_build_exists() {
        let mut w = LambdaWrapper::<User>::new("users");
        w.eq(UserColumns::Id, Value::I64(1));
        let sql = w.build_exists();
        assert!(sql.starts_with("SELECT EXISTS("));
        assert!(sql.contains("SELECT 1 FROM `users`"));
        assert!(sql.contains("`id` = 1"));
        assert!(sql.ends_with(") AS exists_flag"));
    }

    #[test]
    fn test_build_delete() {
        let mut w = LambdaWrapper::<User>::new("users");
        w.eq(UserColumns::Id, Value::I64(1));
        let sql = w.build_delete();
        assert!(sql.starts_with("DELETE FROM `users`"));
        assert!(sql.contains("WHERE `id` = 1"));
    }

    // ===== 方言测试 =====

    #[test]
    fn test_with_postgres_dialect() {
        let dialect: Box<dyn Dialect> = Box::new(PostgreSqlDialect);
        let mut w = LambdaWrapper::<User>::with_dialect("users", dialect);
        w.eq(UserColumns::Id, Value::I64(1));
        let sql = w.build_select();
        assert!(sql.contains("\"users\""));
        assert!(sql.contains("\"id\" = 1"));
    }

    #[test]
    fn test_postgres_select() {
        let dialect: Box<dyn Dialect> = Box::new(PostgreSqlDialect);
        let mut w = LambdaWrapper::<User>::with_dialect("users", dialect);
        w.select(UserColumns::Id).select(UserColumns::Name);
        let sql = w.build_select();
        assert!(sql.contains("\"id\", \"name\""));
    }

    // ===== 完整查询测试 =====

    #[test]
    fn test_complex_query() {
        let mut w = LambdaWrapper::<User>::new("users");
        w.select(UserColumns::Id)
            .select(UserColumns::Name)
            .select(UserColumns::Age)
            .gt(UserColumns::Age, Value::I64(18))
            .like(UserColumns::Name, Value::String("a%".to_string()))
            .is_not_null(UserColumns::Email)
            .order_by_desc(UserColumns::Id)
            .limit(10)
            .offset(20);

        let sql = w.build_select();
        assert!(sql.contains("SELECT `id`, `name`, `age` FROM `users`"));
        assert!(sql.contains("`age` > 18"));
        assert!(sql.contains("`name` LIKE 'a%'"));
        assert!(sql.contains("`email` IS NOT NULL"));
        assert!(sql.contains("ORDER BY `id` DESC"));
        assert!(sql.contains("LIMIT 10"));
        assert!(sql.contains("OFFSET 20"));
    }

    #[test]
    fn test_reset_clears_all() {
        let mut w = LambdaWrapper::<User>::new("users");
        w.select(UserColumns::Id)
            .eq(UserColumns::Id, Value::I64(1))
            .order_by_asc(UserColumns::Name)
            .limit(10);

        w.reset();
        assert_eq!(w.select_count(), 0);
        assert_eq!(w.where_count(), 0);
        let sql = w.build_select();
        assert!(sql.contains("SELECT * FROM `users`"));
        assert!(!sql.contains("WHERE"));
        assert!(!sql.contains("ORDER BY"));
        assert!(!sql.contains("LIMIT"));
    }

    // ===== 跨 Model 类型隔离测试 =====

    #[test]
    fn test_different_models_dont_share_columns() {
        let mut user_w = LambdaWrapper::<User>::new("users");
        user_w.eq(UserColumns::Id, Value::I64(1));

        let mut order_w = LambdaWrapper::<Order>::new("orders");
        order_w.eq(OrderColumns::OrderId, Value::I64(100));

        let user_sql = user_w.build_select();
        let order_sql = order_w.build_select();

        assert!(user_sql.contains("`users`"));
        assert!(user_sql.contains("`id` = 1"));
        assert!(order_sql.contains("`orders`"));
        assert!(order_sql.contains("`order_id` = 100"));
    }

    // ===== 编译期类型安全演示（运行时验证）=====

    #[test]
    fn test_type_safety_compile_time_check() {
        // 以下代码若取消注释将无法编译：
        // let mut w = LambdaWrapper::<User>::new("users");
        // w.eq(OrderColumns::OrderId, Value::I64(1)); // OrderColumns 不能用于 User 的 wrapper

        // 这里通过正常调用验证类型隔离工作正常
        let mut w = LambdaWrapper::<User>::new("users");
        w.eq(UserColumns::Id, Value::I64(1));
        assert_eq!(w.where_count(), 1);
    }

    #[test]
    fn test_string_value_escape() {
        // v0.2.2 修复 H-1：默认使用 MySqlDialect，单引号转义为 \'
        let mut w = LambdaWrapper::<User>::new("users");
        w.eq(UserColumns::Name, Value::String("O'Brien".to_string()));
        let sql = w.build_select();
        // MySQL 方言下字符串中的单引号应被转义为 \'
        assert!(sql.contains("'O\\'Brien'"));

        // 使用 PostgreSQL 方言时，单引号转义为 ''
        let pg_dialect: Box<dyn Dialect> = get_dialect(DbType::PostgreSQL).unwrap();
        let mut w_pg = LambdaWrapper::<User>::with_dialect("users", pg_dialect);
        w_pg.eq(UserColumns::Name, Value::String("O'Brien".to_string()));
        let sql_pg = w_pg.build_select();
        assert!(sql_pg.contains("'O''Brien'"));
    }

    // ===== 使用真实方言验证集成 =====

    #[test]
    fn test_with_real_mysql_dialect() {
        let dialect: Box<dyn Dialect> = get_dialect(DbType::MySQL).unwrap();
        let mut w = LambdaWrapper::<User>::with_dialect("users", dialect);
        w.select(UserColumns::Id)
            .select(UserColumns::Name)
            .eq(UserColumns::Id, Value::I64(42))
            .order_by_desc(UserColumns::Id);

        let sql = w.build_select();
        assert!(sql.contains("SELECT `id`, `name` FROM `users`"));
        assert!(sql.contains("`id` = 42"));
        assert!(sql.contains("ORDER BY `id` DESC"));
    }
}
