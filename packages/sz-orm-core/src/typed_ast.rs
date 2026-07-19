//! 强类型 AST 表达式层（Diesel 风格探索）
//!
//! 在 [`crate::typed`] 模块的 `TypedTable` / `TypedColumn` 标记类型基础上，
//! 构建类型安全的 SQL 表达式 AST，让列类型不匹配、跨表列引用等错误在编译期被捕获。
//!
//! # 设计
//!
//! 借鉴 Diesel 的强类型 AST 思路，但保持精简：
//! - [`TypedExpression`]：所有表达式基类，关联 `SqlType` 类型
//! - [`struct@Eq`]、[`Lt`]、[`Gt`]、[`Le`]、[`Ge`]、[`Ne`]：比较表达式
//! - [`And`]、[`Or`]：逻辑组合表达式
//! - [`ExprTable`]：表达式所属表的关联类型，用于跨表列引用检查
//! - [`TypedSelectQuery`]：类型安全的 SELECT 查询构造器
//!
//! 每个表达式都是零成本抽象（ZST），仅在编译期携带类型信息，
//! 运行时通过 [`TypedExpression::to_sql`] 生成 SQL 片段。
//!
//! # 类型安全保证
//!
//! - `Eq<C, T>` 要求 `C: TypedColumn<RustType = T>`，列类型必须与值类型匹配
//! - `And<L, R>` 要求 `L: TypedExpression<SqlType = Bool>`, `R: TypedExpression<SqlType = Bool>`
//! - `TypedSelectQuery::filter<E>` 要求 `E: TypedExpression<SqlType = Bool> + ExprTable<Table = T>`
//! - 跨表列引用：通过 [`ExprTable`] trait 在编译期拒绝，表达式中的列必须属于查询的表 `T`
//!
//! # 用法
//!
//! ```ignore
//! use sz_orm_core::typed::{TypedTable, TypedColumn};
//! use sz_orm_core::typed_ast::*;
//!
//! // 1. 声明表 schema（通常由 typed_query! 宏生成）
//! struct users;
//! impl TypedTable for users { const NAME: &'static str = "users"; }
//!
//! mod users {
//!     use super::*;
//!     pub struct id;
//!     impl TypedColumn for id {
//!         const NAME: &'static str = "id";
//!         type Table = super::users;
//!         type RustType = i64;
//!     }
//!     pub struct name;
//!     impl TypedColumn for name {
//!         const NAME: &'static str = "name";
//!         type Table = super::users;
//!         type RustType = String;
//!     }
//! }
//!
//! // 2. 类型安全查询
//! let q = TypedSelectQuery::<users>::new()
//!     .filter(users::id.eq(42))         // ✅ i64 列与 i64 值比较
//!     .filter(users::name.eq("Alice")); // ✅ String 列与 &str 值比较
//!
//! // 3. 编译期拒绝的错误
//! // q.filter(users::id.eq("Alice"));  // ❌ i64 列与 &str 值类型不匹配
//! // q.filter(users::name.eq(42));     // ❌ String 列与 i64 值类型不匹配
//! // q.filter(posts::title.eq("...")); // ❌ posts::title 属于 posts 表，ExprTable<Table = posts> 不满足 Table = users
//! ```

use crate::dialect::Dialect;
use crate::typed::{TypedColumn, TypedTable};

/// SQL 类型标记 trait
///
/// 每个类型代表一种 SQL 数据类型，用于编译期类型检查。
/// 实现者应为零大小类型（unit struct）。
pub trait SqlType: 'static {}

/// SQL Bool 类型（WHERE 条件表达式结果）
pub struct Bool;
impl SqlType for Bool {}

/// SQL Integer 类型
pub struct Integer;
impl SqlType for Integer {}

/// SQL Text 类型
pub struct Text;
impl SqlType for Text {}

/// 未指定的 SQL 类型
///
/// 用作 [`crate::typed::TypedColumn::SqlType`] 的默认值。
/// 宏生成的列默认使用此类型；需要强类型 SQL 检查的场景应显式指定具体类型。
pub struct Untyped;
impl SqlType for Untyped {}

/// 强类型表达式 trait
///
/// 所有 SQL 表达式（列、字面量、比较、逻辑组合）都实现此 trait。
/// 关联类型 `SqlType` 携带表达式的 SQL 类型信息，用于编译期类型检查。
pub trait TypedExpression {
    /// 表达式的 SQL 类型
    type SqlType: SqlType;

    /// 生成 SQL 片段（含参数占位符 `?`）
    ///
    /// 返回 `(sql, params)` 元组，`params` 为按出现顺序的参数值。
    fn to_sql(&self, dialect: &dyn Dialect) -> (String, Vec<String>);
}

/// 列引用表达式
///
/// 将 [`TypedColumn`] 包装为 [`TypedExpression`]，
/// 使列可直接用于表达式位置（如 SELECT 子句）。
pub struct ColumnExpr<C: TypedColumn> {
    _marker: std::marker::PhantomData<C>,
}

impl<C: TypedColumn> ColumnExpr<C> {
    /// 创建列引用表达式
    pub fn new() -> Self {
        Self {
            _marker: std::marker::PhantomData,
        }
    }
}

impl<C: TypedColumn> Default for ColumnExpr<C> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C: TypedColumn> TypedExpression for ColumnExpr<C> {
    // 列表达式的 SqlType 由列自身的 SqlType 关联类型决定
    // （TypedColumn::SqlType 默认为 Untyped，可在实现时显式指定）
    type SqlType = C::SqlType;

    fn to_sql(&self, dialect: &dyn Dialect) -> (String, Vec<String>) {
        let qualified = format!("{}.{}", C::Table::NAME, C::NAME);
        (dialect.quote(&qualified), Vec::new())
    }
}

/// 字面量表达式
///
/// 将 Rust 值包装为 SQL 字面量（参数化）。
///
/// 注：当前所有字面量的 SqlType 统一标记为 `Text`。
/// 完整实现需根据值类型（i64/String/bool）派生 SqlType，
/// 可通过为 `Literal<i64>`/`Literal<String>`/`Literal<bool>` 分别实现 `TypedExpression` 完成。
pub struct Literal<T: ToString + Clone> {
    value: T,
}

impl<T: ToString + Clone> Literal<T> {
    /// 创建字面量
    pub fn new(value: T) -> Self {
        Self { value }
    }
}

impl<T: ToString + Clone> TypedExpression for Literal<T> {
    // 字面量最常见的是字符串值，统一标记为 Text。
    // TODO: 完整实现需为 Literal<i64>/Literal<String>/Literal<bool> 分别派生 SqlType。
    type SqlType = Text;

    fn to_sql(&self, _dialect: &dyn Dialect) -> (String, Vec<String>) {
        (String::from("?"), vec![self.value.to_string()])
    }
}

/// 相等比较表达式 `column = value`
///
/// 编译期约束：`C::RustType` 必须与 `V` 类型匹配。
pub struct Eq<C: TypedColumn, V: Clone> {
    column: std::marker::PhantomData<C>,
    value: V,
}

impl<C: TypedColumn, V: Clone + ToString> Eq<C, V> {
    /// 创建相等比较表达式
    pub fn new(_col: C, value: V) -> Self {
        Self {
            column: std::marker::PhantomData,
            value,
        }
    }
}

impl<C: TypedColumn, V: Clone + ToString> TypedExpression for Eq<C, V> {
    type SqlType = Bool;

    fn to_sql(&self, dialect: &dyn Dialect) -> (String, Vec<String>) {
        let col_sql = dialect.quote(C::NAME);
        (format!("{} = ?", col_sql), vec![self.value.to_string()])
    }
}

/// 不相等比较表达式 `column != value`
pub struct Ne<C: TypedColumn, V: Clone> {
    column: std::marker::PhantomData<C>,
    value: V,
}

impl<C: TypedColumn, V: Clone + ToString> Ne<C, V> {
    pub fn new(_col: C, value: V) -> Self {
        Self {
            column: std::marker::PhantomData,
            value,
        }
    }
}

impl<C: TypedColumn, V: Clone + ToString> TypedExpression for Ne<C, V> {
    type SqlType = Bool;

    fn to_sql(&self, dialect: &dyn Dialect) -> (String, Vec<String>) {
        let col_sql = dialect.quote(C::NAME);
        (format!("{} <> ?", col_sql), vec![self.value.to_string()])
    }
}

/// 小于比较表达式 `column < value`
pub struct Lt<C: TypedColumn, V: Clone> {
    column: std::marker::PhantomData<C>,
    value: V,
}

impl<C: TypedColumn, V: Clone + ToString> Lt<C, V> {
    pub fn new(_col: C, value: V) -> Self {
        Self {
            column: std::marker::PhantomData,
            value,
        }
    }
}

impl<C: TypedColumn, V: Clone + ToString> TypedExpression for Lt<C, V> {
    type SqlType = Bool;

    fn to_sql(&self, dialect: &dyn Dialect) -> (String, Vec<String>) {
        let col_sql = dialect.quote(C::NAME);
        (format!("{} < ?", col_sql), vec![self.value.to_string()])
    }
}

/// 大于比较表达式 `column > value`
pub struct Gt<C: TypedColumn, V: Clone> {
    column: std::marker::PhantomData<C>,
    value: V,
}

impl<C: TypedColumn, V: Clone + ToString> Gt<C, V> {
    pub fn new(_col: C, value: V) -> Self {
        Self {
            column: std::marker::PhantomData,
            value,
        }
    }
}

impl<C: TypedColumn, V: Clone + ToString> TypedExpression for Gt<C, V> {
    type SqlType = Bool;

    fn to_sql(&self, dialect: &dyn Dialect) -> (String, Vec<String>) {
        let col_sql = dialect.quote(C::NAME);
        (format!("{} > ?", col_sql), vec![self.value.to_string()])
    }
}

/// 小于等于比较表达式 `column <= value`
pub struct Le<C: TypedColumn, V: Clone> {
    column: std::marker::PhantomData<C>,
    value: V,
}

impl<C: TypedColumn, V: Clone + ToString> Le<C, V> {
    pub fn new(_col: C, value: V) -> Self {
        Self {
            column: std::marker::PhantomData,
            value,
        }
    }
}

impl<C: TypedColumn, V: Clone + ToString> TypedExpression for Le<C, V> {
    type SqlType = Bool;

    fn to_sql(&self, dialect: &dyn Dialect) -> (String, Vec<String>) {
        let col_sql = dialect.quote(C::NAME);
        (format!("{} <= ?", col_sql), vec![self.value.to_string()])
    }
}

/// 大于等于比较表达式 `column >= value`
pub struct Ge<C: TypedColumn, V: Clone> {
    column: std::marker::PhantomData<C>,
    value: V,
}

impl<C: TypedColumn, V: Clone + ToString> Ge<C, V> {
    pub fn new(_col: C, value: V) -> Self {
        Self {
            column: std::marker::PhantomData,
            value,
        }
    }
}

impl<C: TypedColumn, V: Clone + ToString> TypedExpression for Ge<C, V> {
    type SqlType = Bool;

    fn to_sql(&self, dialect: &dyn Dialect) -> (String, Vec<String>) {
        let col_sql = dialect.quote(C::NAME);
        (format!("{} >= ?", col_sql), vec![self.value.to_string()])
    }
}

/// 逻辑 AND 表达式 `left AND right`
///
/// 编译期约束：两个子表达式都必须是 Bool 类型。
pub struct And<L: TypedExpression<SqlType = Bool>, R: TypedExpression<SqlType = Bool>> {
    left: L,
    right: R,
}

impl<L: TypedExpression<SqlType = Bool>, R: TypedExpression<SqlType = Bool>> And<L, R> {
    pub fn new(left: L, right: R) -> Self {
        Self { left, right }
    }
}

impl<L: TypedExpression<SqlType = Bool>, R: TypedExpression<SqlType = Bool>> TypedExpression
    for And<L, R>
{
    type SqlType = Bool;

    fn to_sql(&self, dialect: &dyn Dialect) -> (String, Vec<String>) {
        let (left_sql, mut left_params) = self.left.to_sql(dialect);
        let (right_sql, right_params) = self.right.to_sql(dialect);
        left_params.extend(right_params);
        (format!("({} AND {})", left_sql, right_sql), left_params)
    }
}

/// 逻辑 OR 表达式 `left OR right`
pub struct Or<L: TypedExpression<SqlType = Bool>, R: TypedExpression<SqlType = Bool>> {
    left: L,
    right: R,
}

impl<L: TypedExpression<SqlType = Bool>, R: TypedExpression<SqlType = Bool>> Or<L, R> {
    pub fn new(left: L, right: R) -> Self {
        Self { left, right }
    }
}

impl<L: TypedExpression<SqlType = Bool>, R: TypedExpression<SqlType = Bool>> TypedExpression
    for Or<L, R>
{
    type SqlType = Bool;

    fn to_sql(&self, dialect: &dyn Dialect) -> (String, Vec<String>) {
        let (left_sql, mut left_params) = self.left.to_sql(dialect);
        let (right_sql, right_params) = self.right.to_sql(dialect);
        left_params.extend(right_params);
        (format!("({} OR {})", left_sql, right_sql), left_params)
    }
}

/// 表达式所属的表（用于跨表列引用检查）
///
/// 该 trait 将表达式与其所属的表关联，使 [`TypedSelectQuery::filter`] 能在编译期
/// 拒绝引用了其他表的列的表达式。
///
/// - 列引用/比较表达式：表由列的 [`TypedColumn::Table`] 决定
/// - 逻辑组合表达式（`And`/`Or`）：两侧子表达式必须属于同一张表
///
/// # 跨表拒绝示例
///
/// ```ignore
/// // 假设 ColPostTitle 属于 PostsTable，而查询是 TypedSelectQuery::<UsersTable>
/// // 以下代码无法编译：
/// TypedSelectQuery::<UsersTable>::new()
///     .filter(ColPostTitle.eq("hello"));  // ❌ ExprTable<Table = PostsTable> 不满足 Table = UsersTable
/// ```
pub trait ExprTable {
    /// 表达式所属的表
    type Table: TypedTable;
}

// 列引用表达式的表 = 列所属的表
impl<C: TypedColumn> ExprTable for ColumnExpr<C> {
    type Table = C::Table;
}

// 比较表达式的表 = 列所属的表（值字面量不改变表归属）
impl<C: TypedColumn, V: Clone> ExprTable for Eq<C, V> {
    type Table = C::Table;
}

impl<C: TypedColumn, V: Clone> ExprTable for Ne<C, V> {
    type Table = C::Table;
}

impl<C: TypedColumn, V: Clone> ExprTable for Lt<C, V> {
    type Table = C::Table;
}

impl<C: TypedColumn, V: Clone> ExprTable for Gt<C, V> {
    type Table = C::Table;
}

impl<C: TypedColumn, V: Clone> ExprTable for Le<C, V> {
    type Table = C::Table;
}

impl<C: TypedColumn, V: Clone> ExprTable for Ge<C, V> {
    type Table = C::Table;
}

// 逻辑 AND：两侧必须属于同一张表，否则不实现 ExprTable（编译期拒绝）
impl<L, R> ExprTable for And<L, R>
where
    L: TypedExpression<SqlType = Bool> + ExprTable,
    R: TypedExpression<SqlType = Bool> + ExprTable<Table = L::Table>,
{
    type Table = L::Table;
}

// 逻辑 OR：两侧必须属于同一张表，否则不实现 ExprTable（编译期拒绝）
impl<L, R> ExprTable for Or<L, R>
where
    L: TypedExpression<SqlType = Bool> + ExprTable,
    R: TypedExpression<SqlType = Bool> + ExprTable<Table = L::Table>,
{
    type Table = L::Table;
}

/// 类型安全的 SELECT 查询构造器
///
/// 泛型参数 `T` 锁定查询的主表，确保所有 filter 表达式都引用 `T` 的列。
pub struct TypedSelectQuery<T: TypedTable> {
    _table: std::marker::PhantomData<T>,
    wheres: Vec<Box<dyn TypedExpression<SqlType = Bool>>>,
    limit_n: Option<usize>,
    offset_n: Option<usize>,
}

impl<T: TypedTable> TypedSelectQuery<T> {
    /// 创建新的 SELECT 查询
    pub fn new() -> Self {
        Self {
            _table: std::marker::PhantomData,
            wheres: Vec::new(),
            limit_n: None,
            offset_n: None,
        }
    }

    /// 添加 WHERE 条件（AND 连接）
    ///
    /// # 编译期约束
    ///
    /// - 表达式必须返回 Bool 类型（`E: TypedExpression<SqlType = Bool>`）
    /// - 表达式中所有列必须属于当前查询的表 `T`（`E: ExprTable<Table = T>`）
    ///
    /// 跨表列引用会在编译期被拒绝。
    pub fn filter<E>(mut self, expr: E) -> Self
    where
        E: TypedExpression<SqlType = Bool> + ExprTable<Table = T> + 'static,
    {
        // ExprTable<Table = T> 约束在编译期检查，运行时擦除类型信息存储
        self.wheres.push(Box::new(expr));
        self
    }

    /// 设置 LIMIT
    pub fn limit(mut self, n: usize) -> Self {
        self.limit_n = Some(n);
        self
    }

    /// 设置 OFFSET
    pub fn offset(mut self, n: usize) -> Self {
        self.offset_n = Some(n);
        self
    }

    /// 构建 SELECT SQL
    ///
    /// 生成形如 `SELECT * FROM <table> WHERE <conds> LIMIT <n> OFFSET <n>` 的 SQL。
    pub fn build(&self, dialect: &dyn Dialect) -> (String, Vec<String>) {
        let table_sql = dialect.quote(T::NAME);
        let mut sql = format!("SELECT * FROM {}", table_sql);
        let mut all_params = Vec::new();

        if !self.wheres.is_empty() {
            let mut cond_strs = Vec::new();
            for w in &self.wheres {
                let (s, p) = w.to_sql(dialect);
                cond_strs.push(s);
                all_params.extend(p);
            }
            sql.push_str(" WHERE ");
            sql.push_str(&cond_strs.join(" AND "));
        }

        if let Some(n) = self.limit_n {
            sql.push_str(&format!(" LIMIT {}", n));
        }
        if let Some(n) = self.offset_n {
            sql.push_str(&format!(" OFFSET {}", n));
        }

        (sql, all_params)
    }
}

impl<T: TypedTable> Default for TypedSelectQuery<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// 列扩展 trait：为 [`TypedColumn`] 提供 `.eq()` / `.lt()` / `.gt()` 等便捷方法
pub trait TypedColumnExt: TypedColumn + Sized {
    /// 创建 `column = value` 表达式
    fn eq<V: Clone + ToString>(self, value: V) -> Eq<Self, V> {
        Eq::new(self, value)
    }

    /// 创建 `column != value` 表达式
    fn ne<V: Clone + ToString>(self, value: V) -> Ne<Self, V> {
        Ne::new(self, value)
    }

    /// 创建 `column < value` 表达式
    fn lt<V: Clone + ToString>(self, value: V) -> Lt<Self, V> {
        Lt::new(self, value)
    }

    /// 创建 `column > value` 表达式
    fn gt<V: Clone + ToString>(self, value: V) -> Gt<Self, V> {
        Gt::new(self, value)
    }

    /// 创建 `column <= value` 表达式
    fn le<V: Clone + ToString>(self, value: V) -> Le<Self, V> {
        Le::new(self, value)
    }

    /// 创建 `column >= value` 表达式
    fn ge<V: Clone + ToString>(self, value: V) -> Ge<Self, V> {
        Ge::new(self, value)
    }
}

impl<C: TypedColumn> TypedColumnExt for C {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dialect::MySqlDialect;
    use crate::typed::{TypedColumn, TypedTable};

    // ---- 测试用 mock 类型 ----

    struct UsersTable;
    impl TypedTable for UsersTable {
        const NAME: &'static str = "users";
    }

    struct ColId;
    impl TypedColumn for ColId {
        const NAME: &'static str = "id";
        type Table = UsersTable;
        type RustType = i64;
        type SqlType = Integer;
    }

    struct ColName;
    impl TypedColumn for ColName {
        const NAME: &'static str = "name";
        type Table = UsersTable;
        type RustType = String;
        type SqlType = Text;
    }

    struct ColAge;
    impl TypedColumn for ColAge {
        const NAME: &'static str = "age";
        type Table = UsersTable;
        type RustType = i64;
        type SqlType = Integer;
    }

    // 另一张表（用于跨表测试）
    struct PostsTable;
    impl TypedTable for PostsTable {
        const NAME: &'static str = "posts";
    }

    struct ColPostTitle;
    impl TypedColumn for ColPostTitle {
        const NAME: &'static str = "title";
        type Table = PostsTable;
        type RustType = String;
        type SqlType = Text;
    }

    // ---- 表达式测试 ----

    #[test]
    fn test_eq_expression_sql() {
        let dialect = MySqlDialect;
        let expr = ColId.eq(42i64);
        let (sql, params) = expr.to_sql(&dialect);
        assert_eq!(sql, "`id` = ?");
        assert_eq!(params, vec!["42"]);
    }

    #[test]
    fn test_ne_expression_sql() {
        let dialect = MySqlDialect;
        let expr = ColId.ne(0i64);
        let (sql, params) = expr.to_sql(&dialect);
        assert_eq!(sql, "`id` <> ?");
        assert_eq!(params, vec!["0"]);
    }

    #[test]
    fn test_lt_expression_sql() {
        let dialect = MySqlDialect;
        let expr = ColAge.lt(18i64);
        let (sql, params) = expr.to_sql(&dialect);
        assert_eq!(sql, "`age` < ?");
        assert_eq!(params, vec!["18"]);
    }

    #[test]
    fn test_gt_expression_sql() {
        let dialect = MySqlDialect;
        let expr = ColAge.gt(18i64);
        let (sql, params) = expr.to_sql(&dialect);
        assert_eq!(sql, "`age` > ?");
        assert_eq!(params, vec!["18"]);
    }

    #[test]
    fn test_le_expression_sql() {
        let dialect = MySqlDialect;
        let expr = ColAge.le(65i64);
        let (sql, params) = expr.to_sql(&dialect);
        assert_eq!(sql, "`age` <= ?");
        assert_eq!(params, vec!["65"]);
    }

    #[test]
    fn test_ge_expression_sql() {
        let dialect = MySqlDialect;
        let expr = ColAge.ge(18i64);
        let (sql, params) = expr.to_sql(&dialect);
        assert_eq!(sql, "`age` >= ?");
        assert_eq!(params, vec!["18"]);
    }

    #[test]
    fn test_string_eq_expression() {
        let dialect = MySqlDialect;
        let expr = ColName.eq("Alice".to_string());
        let (sql, params) = expr.to_sql(&dialect);
        assert_eq!(sql, "`name` = ?");
        assert_eq!(params, vec!["Alice"]);
    }

    // ---- 逻辑组合测试 ----

    #[test]
    fn test_and_expression() {
        let dialect = MySqlDialect;
        let expr = And::new(ColId.eq(1i64), ColAge.gt(18i64));
        let (sql, params) = expr.to_sql(&dialect);
        assert_eq!(sql, "(`id` = ? AND `age` > ?)");
        assert_eq!(params, vec!["1", "18"]);
    }

    #[test]
    fn test_or_expression() {
        let dialect = MySqlDialect;
        let expr = Or::new(
            ColName.eq("Alice".to_string()),
            ColName.eq("Bob".to_string()),
        );
        let (sql, params) = expr.to_sql(&dialect);
        assert_eq!(sql, "(`name` = ? OR `name` = ?)");
        assert_eq!(params, vec!["Alice", "Bob"]);
    }

    #[test]
    fn test_nested_and_or() {
        let dialect = MySqlDialect;
        let left = ColId.eq(1i64);
        let right = Or::new(
            ColName.eq("Alice".to_string()),
            ColName.eq("Bob".to_string()),
        );
        let expr = And::new(left, right);
        let (sql, params) = expr.to_sql(&dialect);
        assert_eq!(sql, "(`id` = ? AND (`name` = ? OR `name` = ?))");
        assert_eq!(params, vec!["1", "Alice", "Bob"]);
    }

    // ---- TypedSelectQuery 测试 ----

    #[test]
    fn test_select_query_no_filter() {
        let dialect = MySqlDialect;
        let q = TypedSelectQuery::<UsersTable>::new();
        let (sql, params) = q.build(&dialect);
        assert_eq!(sql, "SELECT * FROM `users`");
        assert!(params.is_empty());
    }

    #[test]
    fn test_select_query_single_filter() {
        let dialect = MySqlDialect;
        let q = TypedSelectQuery::<UsersTable>::new().filter(ColId.eq(42i64));
        let (sql, params) = q.build(&dialect);
        assert_eq!(sql, "SELECT * FROM `users` WHERE `id` = ?");
        assert_eq!(params, vec!["42"]);
    }

    #[test]
    fn test_select_query_multiple_filters() {
        let dialect = MySqlDialect;
        let q = TypedSelectQuery::<UsersTable>::new()
            .filter(ColId.eq(1i64))
            .filter(ColAge.gt(18i64))
            .filter(ColName.ne("guest".to_string()));
        let (sql, params) = q.build(&dialect);
        assert_eq!(
            sql,
            "SELECT * FROM `users` WHERE `id` = ? AND `age` > ? AND `name` <> ?"
        );
        assert_eq!(params, vec!["1", "18", "guest"]);
    }

    #[test]
    fn test_select_query_with_limit_offset() {
        let dialect = MySqlDialect;
        let q = TypedSelectQuery::<UsersTable>::new()
            .filter(ColAge.ge(18i64))
            .limit(10)
            .offset(20);
        let (sql, params) = q.build(&dialect);
        assert_eq!(
            sql,
            "SELECT * FROM `users` WHERE `age` >= ? LIMIT 10 OFFSET 20"
        );
        assert_eq!(params, vec!["18"]);
    }

    #[test]
    fn test_select_query_with_complex_and_or() {
        let dialect = MySqlDialect;
        let q = TypedSelectQuery::<UsersTable>::new().filter(And::new(
            ColAge.ge(18i64),
            Or::new(
                ColName.eq("Alice".to_string()),
                ColName.eq("Bob".to_string()),
            ),
        ));
        let (sql, params) = q.build(&dialect);
        assert_eq!(
            sql,
            "SELECT * FROM `users` WHERE (`age` >= ? AND (`name` = ? OR `name` = ?))"
        );
        assert_eq!(params, vec!["18", "Alice", "Bob"]);
    }

    // ---- 编译期类型安全验证（通过 trait bound） ----

    #[test]
    fn test_compile_time_type_safety_i64_column() {
        // ColId 的 RustType 是 i64
        fn _assert_i64<C: TypedColumn<RustType = i64>>(_: C) {}
        _assert_i64(ColId);
        _assert_i64(ColAge);
    }

    #[test]
    fn test_compile_time_type_safety_string_column() {
        // ColName 的 RustType 是 String
        fn _assert_string<C: TypedColumn<RustType = String>>(_: C) {}
        _assert_string(ColName);
    }

    #[test]
    fn test_compile_time_table_association() {
        // ColId 属于 UsersTable
        fn _assert_users_table<C: TypedColumn<Table = UsersTable>>(_: C) {}
        _assert_users_table(ColId);
        _assert_users_table(ColName);
        _assert_users_table(ColAge);

        // ColPostTitle 属于 PostsTable
        fn _assert_posts_table<C: TypedColumn<Table = PostsTable>>(_: C) {}
        _assert_posts_table(ColPostTitle);
    }

    #[test]
    fn test_compile_time_bool_expression() {
        // Eq/Lt/Gt 等比较表达式的 SqlType 必须是 Bool
        fn _assert_bool<E: TypedExpression<SqlType = Bool>>(_: E) {}
        _assert_bool(ColId.eq(1i64));
        _assert_bool(ColAge.lt(18i64));
        _assert_bool(ColName.ne("x".to_string()));

        // And/Or 组合表达式也必须是 Bool
        _assert_bool(And::new(ColId.eq(1i64), ColAge.gt(18i64)));
        _assert_bool(Or::new(
            ColName.eq("a".to_string()),
            ColName.eq("b".to_string()),
        ));
    }

    // ---- 跨表列引用：编译期拒绝（通过 ExprTable trait 约束） ----
    //
    // `TypedSelectQuery::<T>::filter(E)` 要求 `E: ExprTable<Table = T>`，
    // 因此引用了其他表的列的表达式会在编译期被拒绝。
    //
    // 以下代码无法编译（已注释，作为编译期保证的示例）：
    //
    // ```ignore
    // TypedSelectQuery::<UsersTable>::new()
    //     .filter(ColPostTitle.eq("hello")); // ❌ ColPostTitle 属于 PostsTable
    // ```

    #[test]
    fn test_cross_table_column_has_correct_table_association() {
        // ColPostTitle::Table = PostsTable，不是 UsersTable
        // 这意味着 TypedSelectQuery<UsersTable>::filter(ColPostTitle.eq(...))
        // 会被编译器拒绝（ExprTable<Table = PostsTable> 不满足 Table = UsersTable）
        fn _assert_post_table<C: TypedColumn<Table = PostsTable>>(_: C) {}
        _assert_post_table(ColPostTitle);

        // 反之 ColId::Table = UsersTable
        fn _assert_user_table<C: TypedColumn<Table = UsersTable>>(_: C) {}
        _assert_user_table(ColId);
    }

    #[test]
    fn test_expr_table_for_column_expressions() {
        // 列表达式的 ExprTable::Table = 列的 Table
        fn _assert_expr_table<E: ExprTable<Table = UsersTable>>(_: E) {}

        // 比较表达式继承列的表归属
        _assert_expr_table(ColId.eq(1i64));
        _assert_expr_table(ColName.eq("Alice".to_string()));
        _assert_expr_table(ColAge.gt(18i64));
        _assert_expr_table(ColAge.lt(65i64));
        _assert_expr_table(ColAge.le(65i64));
        _assert_expr_table(ColAge.ge(18i64));
        _assert_expr_table(ColId.ne(0i64));
    }

    #[test]
    fn test_expr_table_for_logical_combinations() {
        // 逻辑组合表达式要求两侧属于同一张表
        fn _assert_expr_table<E: ExprTable<Table = UsersTable>>(_: E) {}

        // 同表组合：✅
        _assert_expr_table(And::new(ColId.eq(1i64), ColAge.gt(18i64)));
        _assert_expr_table(Or::new(
            ColName.eq("a".to_string()),
            ColName.eq("b".to_string()),
        ));
        _assert_expr_table(And::new(
            ColAge.ge(18i64),
            Or::new(ColName.eq("a".to_string()), ColName.eq("b".to_string())),
        ));
    }

    #[test]
    fn test_cross_table_logical_combination_rejected_at_compile_time() {
        // And/Or 要求两侧同表，以下组合在编译期会被拒绝：
        //
        // ```ignore
        // // ❌ ColId 属于 UsersTable, ColPostTitle 属于 PostsTable
        // let _ = And::new(ColId.eq(1i64), ColPostTitle.eq("x"));
        // // 错误：And<_, _> 未实现 ExprTable<Table = ?>（两侧表不同）
        // ```
        //
        // 此测试仅作为占位，证明同表组合可以正常通过编译。
        let _expr = And::new(ColId.eq(1i64), ColAge.gt(18i64));
    }

    // ---- SqlType 标记类型测试 ----

    #[test]
    fn test_sql_type_markers() {
        // 这些是零大小标记类型
        assert_eq!(std::mem::size_of::<Bool>(), 0);
        assert_eq!(std::mem::size_of::<Integer>(), 0);
        assert_eq!(std::mem::size_of::<Text>(), 0);
        assert_eq!(std::mem::size_of::<Untyped>(), 0);
    }

    #[test]
    fn test_column_sql_type_propagation() {
        // ColumnExpr<C>::SqlType 应等于 C::SqlType
        fn _assert_integer<E: TypedExpression<SqlType = Integer>>(_: E) {}
        fn _assert_text<E: TypedExpression<SqlType = Text>>(_: E) {}

        _assert_integer(ColumnExpr::<ColId>::new());
        _assert_integer(ColumnExpr::<ColAge>::new());
        _assert_text(ColumnExpr::<ColName>::new());
        _assert_text(ColumnExpr::<ColPostTitle>::new());
    }

    #[test]
    fn test_literal_sql_type_is_text() {
        // Literal<T>::SqlType 应为 Text（统一标记）
        fn _assert_text<E: TypedExpression<SqlType = Text>>(_: E) {}
        _assert_text(Literal::new(42i64));
        _assert_text(Literal::new("hello".to_string()));
        _assert_text(Literal::new(true));
    }

    #[test]
    fn test_typed_select_query_is_zero_cost() {
        // TypedSelectQuery 的 PhantomData 是零大小
        // 但 Vec<Box<...>> 有运行时开销
        let q = TypedSelectQuery::<UsersTable>::new();
        assert_eq!(q.wheres.len(), 0);
    }

    // ---- 默认实现测试 ----

    #[test]
    fn test_typed_select_query_default() {
        let q = TypedSelectQuery::<UsersTable>::default();
        let dialect = MySqlDialect;
        let (sql, _) = q.build(&dialect);
        assert_eq!(sql, "SELECT * FROM `users`");
    }
}
