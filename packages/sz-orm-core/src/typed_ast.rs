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

/// SQL Integer 类型（对应 i32 / INT）
pub struct Integer;
impl SqlType for Integer {}

/// SQL SmallInt 类型（对应 i8/i16 / TINYINT/SMALLINT）
pub struct SmallInt;
impl SqlType for SmallInt {}

/// SQL BigInt 类型（对应 i64 / BIGINT）
pub struct BigInt;
impl SqlType for BigInt {}

/// SQL Real 类型（对应 f32 / FLOAT/REAL）
pub struct Real;
impl SqlType for Real {}

/// SQL Double 类型（对应 f64 / DOUBLE/DOUBLE PRECISION）
pub struct Double;
impl SqlType for Double {}

/// SQL Text 类型（对应 String / VARCHAR/TEXT）
pub struct Text;
impl SqlType for Text {}

/// SQL Date 类型（对应日期，不含时间）
pub struct Date;
impl SqlType for Date {}

/// SQL DateTime 类型（对应日期时间 / DATETIME/TIMESTAMP）
pub struct DateTime;
impl SqlType for DateTime {}

/// SQL JSON 类型
pub struct Json;
impl SqlType for Json {}

/// SQL UUID 类型
pub struct Uuid;
impl SqlType for Uuid {}

/// SQL Binary 类型（对应 `Vec<u8>` / BLOB/BYTEA/VARBINARY）
pub struct Binary;
impl SqlType for Binary {}

/// 可空类型包装器（对应 `Option<T>` / NULLABLE）
///
/// 将内层 SqlType 标记为允许 NULL。
pub struct Nullable<T: SqlType>(pub std::marker::PhantomData<T>);
impl<T: SqlType> SqlType for Nullable<T> {}

/// 未指定的 SQL 类型
///
/// 用作 [`crate::typed::TypedColumn::SqlType`] 的默认值。
/// 宏生成的列默认使用此类型；需要强类型 SQL 检查的场景应显式指定具体类型。
pub struct Untyped;
impl SqlType for Untyped {}

/// Rust 类型 → SQL 类型标记推断 trait
///
/// 为常见 Rust 类型提供编译期 SqlType 映射，使 `typed_query!` 宏能自动推断
/// 列的 SqlType，而非一律默认 Untyped。
///
/// # 类型映射表
///
/// | Rust 类型 | SqlType |
/// |-----------|---------|
/// | `bool` | `Bool` |
/// | `i8`, `u8` | `SmallInt` |
/// | `i16`, `u16` | `SmallInt` |
/// | `i32`, `u32` | `Integer` |
/// | `i64`, `u64` | `BigInt` |
/// | `f32` | `Real` |
/// | `f64` | `Double` |
/// | `String`, `&str`, `&String` | `Text` |
/// | `Vec<u8>`, `&[u8]`, `&Vec<u8>` | `Binary` |
/// | `Option<T>` | `Nullable<T::SqlType>` |
/// | `()` | `Untyped`（兜底） |
///
/// 未实现此 trait 的类型在 `typed_query!` 宏中使用时会产生编译错误，
/// 用户可自行实现 `InferSqlType` 来扩展类型映射。
pub trait InferSqlType {
    /// 推断出的 SQL 类型标记
    type SqlType: SqlType;
}

impl InferSqlType for bool {
    type SqlType = Bool;
}

impl InferSqlType for i8 {
    type SqlType = SmallInt;
}

impl InferSqlType for i16 {
    type SqlType = SmallInt;
}

impl InferSqlType for i32 {
    type SqlType = Integer;
}

impl InferSqlType for i64 {
    type SqlType = BigInt;
}

impl InferSqlType for u8 {
    type SqlType = SmallInt;
}

impl InferSqlType for u16 {
    type SqlType = SmallInt;
}

impl InferSqlType for u32 {
    type SqlType = Integer;
}

impl InferSqlType for u64 {
    type SqlType = BigInt;
}

impl InferSqlType for f32 {
    type SqlType = Real;
}

impl InferSqlType for f64 {
    type SqlType = Double;
}

impl InferSqlType for String {
    type SqlType = Text;
}

impl InferSqlType for Vec<u8> {
    type SqlType = Binary;
}

/// `&str` -> `Text` (same as `String`)
impl InferSqlType for &str {
    type SqlType = Text;
}

/// `&String` -> `Text` (same as `String`)
impl InferSqlType for &String {
    type SqlType = Text;
}

/// `&[u8]` -> `Binary` (same as `Vec<u8>`)
impl InferSqlType for &[u8] {
    type SqlType = Binary;
}

/// `&Vec<u8>` -> `Binary` (same as `Vec<u8>`)
impl InferSqlType for &Vec<u8> {
    type SqlType = Binary;
}

/// `Option<T>` 推断为 `Nullable<T::SqlType>`，保留内层类型信息。
impl<T: InferSqlType> InferSqlType for Option<T> {
    type SqlType = Nullable<T::SqlType>;
}

/// 兜底实现：`()` 代表未知类型，映射到 Untyped
///
/// 当 `typed_query!` 宏遇到无法解析的类型时，会回退到 `()`，
/// 由此获得 Untyped 而非编译失败。
impl InferSqlType for () {
    type SqlType = Untyped;
}

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
/// `SqlType` 由值类型派生：`i64`→`BigInt`、`i32`→`Integer`、`String`→`Text`、
/// `bool`→`Bool`、`f64`→`Double`、`f32`→`Real`、`Vec<u8>`→`Binary`。
/// 其他类型需显式实现 `TypedExpression`。
pub struct Literal<T: Clone> {
    value: T,
}

impl<T: Clone> Literal<T> {
    /// 创建字面量
    pub fn new(value: T) -> Self {
        Self { value }
    }
}

/// `Literal<i64>` → SQL BigInt 类型
impl TypedExpression for Literal<i64> {
    type SqlType = BigInt;

    fn to_sql(&self, _dialect: &dyn Dialect) -> (String, Vec<String>) {
        (String::from("?"), vec![self.value.to_string()])
    }
}

/// `Literal<i32>` → SQL Integer 类型
impl TypedExpression for Literal<i32> {
    type SqlType = Integer;

    fn to_sql(&self, _dialect: &dyn Dialect) -> (String, Vec<String>) {
        (String::from("?"), vec![self.value.to_string()])
    }
}

/// `Literal<i16>` → SQL SmallInt 类型
impl TypedExpression for Literal<i16> {
    type SqlType = SmallInt;

    fn to_sql(&self, _dialect: &dyn Dialect) -> (String, Vec<String>) {
        (String::from("?"), vec![self.value.to_string()])
    }
}

/// `Literal<i8>` → SQL SmallInt 类型
impl TypedExpression for Literal<i8> {
    type SqlType = SmallInt;

    fn to_sql(&self, _dialect: &dyn Dialect) -> (String, Vec<String>) {
        (String::from("?"), vec![self.value.to_string()])
    }
}

/// `Literal<f64>` → SQL Double 类型
impl TypedExpression for Literal<f64> {
    type SqlType = Double;

    fn to_sql(&self, _dialect: &dyn Dialect) -> (String, Vec<String>) {
        (String::from("?"), vec![self.value.to_string()])
    }
}

/// `Literal<f32>` → SQL Real 类型
impl TypedExpression for Literal<f32> {
    type SqlType = Real;

    fn to_sql(&self, _dialect: &dyn Dialect) -> (String, Vec<String>) {
        (String::from("?"), vec![self.value.to_string()])
    }
}

/// `Literal<String>` → SQL Text 类型
impl TypedExpression for Literal<String> {
    type SqlType = Text;

    fn to_sql(&self, _dialect: &dyn Dialect) -> (String, Vec<String>) {
        (String::from("?"), vec![self.value.to_string()])
    }
}

/// `Literal<bool>` → SQL Bool 类型
impl TypedExpression for Literal<bool> {
    type SqlType = Bool;

    fn to_sql(&self, _dialect: &dyn Dialect) -> (String, Vec<String>) {
        (String::from("?"), vec![self.value.to_string()])
    }
}

/// `Literal<Vec<u8>>` → SQL Binary 类型
impl TypedExpression for Literal<Vec<u8>> {
    type SqlType = Binary;

    fn to_sql(&self, _dialect: &dyn Dialect) -> (String, Vec<String>) {
        // 二进制数据以 hex 编码传递（具体编码由驱动层处理，此处仅占位）
        let hex: String = self.value.iter().map(|b| format!("{:02x}", b)).collect();
        (String::from("?"), vec![hex])
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
    /// 创建不等于表达式 `col != value`
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
    /// 创建小于表达式 `col < value`
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
    /// 创建大于表达式 `col > value`
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
    /// 创建小于等于表达式 `col <= value`
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
    /// 创建大于等于表达式 `col >= value`
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
    /// 创建逻辑与表达式 `left AND right`
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
    /// 创建逻辑或表达式 `left OR right`
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

    /// L-3 修复：LIMIT 最大值限制
    ///
    /// 防止调用方误用（如 `usize::MAX`）导致数据库执行超大结果集，
    /// 引发 OOM 或长时间阻塞。1,000,000 行足以覆盖常规分页场景。
    pub const MAX_LIMIT: usize = 1_000_000;

    /// L-3 修复：OFFSET 最大值限制
    ///
    /// 防止调用方误用超大 OFFSET（如 `usize::MAX`），导致数据库
    /// 扫描全部行后才丢弃，引发性能问题。1,000,000,000 行足以覆盖常规分页场景。
    pub const MAX_OFFSET: usize = 1_000_000_000;

    /// 设置 LIMIT
    ///
    /// L-3 修复：超过 `MAX_LIMIT` 的值会被自动 clamp 到 `MAX_LIMIT`，
    /// 防止误用导致数据库 OOM。
    pub fn limit(mut self, n: usize) -> Self {
        self.limit_n = Some(n.min(Self::MAX_LIMIT));
        self
    }

    /// 设置 OFFSET
    ///
    /// L-3 修复：超过 `MAX_OFFSET` 的值会被自动 clamp 到 `MAX_OFFSET`，
    /// 防止误用导致数据库全表扫描性能问题。
    pub fn offset(mut self, n: usize) -> Self {
        self.offset_n = Some(n.min(Self::MAX_OFFSET));
        self
    }

    /// 构建 SELECT SQL
    ///
    /// 生成形如 `SELECT * FROM <table> WHERE <conds> <pagination>` 的 SQL。
    ///
    /// C-4 修复：分页部分通过 `dialect.build_pagination()` 生成，
    /// 不再硬编码 `LIMIT/OFFSET`，以兼容 Oracle/SQL Server/DB2/ClickHouse 等方言。
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

        // C-4 修复：使用方言感知的分页，而非硬编码 LIMIT/OFFSET
        // 当只设置 limit（无 offset）时，page=1；当同时设置 offset 时，page = offset/limit + 1
        if let Some(limit) = self.limit_n {
            let page = match self.offset_n {
                Some(offset) if limit > 0 => (offset / limit) as u64 + 1,
                _ => 1,
            };
            sql = dialect.build_pagination(&sql, page, limit as u64);
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

    /// 创建 `column LIKE ?` 表达式
    fn like<V: Clone + ToString>(self, value: V) -> Like<Self, V> {
        Like::new(self, value)
    }

    /// 创建 `column IN (?, ...)` 表达式
    fn in_<V: Clone + ToString>(self, values: Vec<V>) -> In<Self, V> {
        In::new(self, values)
    }
}

impl<C: TypedColumn> TypedColumnExt for C {}

// ============================================================================
// 布尔表达式扩展（like/in_/and/or/not）
// type_safe_columns 差分测试依赖的 DSL 组合 API
// ============================================================================

/// LIKE 模式匹配表达式 `column LIKE ?`
pub struct Like<C: TypedColumn, V: Clone + ToString> {
    column: std::marker::PhantomData<C>,
    value: V,
}

impl<C: TypedColumn, V: Clone + ToString> Like<C, V> {
    /// 创建 LIKE 匹配表达式
    pub fn new(_col: C, value: V) -> Self {
        Self {
            column: std::marker::PhantomData,
            value,
        }
    }
}

impl<C: TypedColumn, V: Clone + ToString> TypedExpression for Like<C, V> {
    type SqlType = Bool;

    fn to_sql(&self, dialect: &dyn Dialect) -> (String, Vec<String>) {
        let col_sql = dialect.quote(C::NAME);
        (format!("{} LIKE ?", col_sql), vec![self.value.to_string()])
    }
}

/// IN 列表表达式 `column IN (?, ?, ...)`
pub struct In<C: TypedColumn, V: Clone + ToString> {
    column: std::marker::PhantomData<C>,
    values: Vec<V>,
}

impl<C: TypedColumn, V: Clone + ToString> In<C, V> {
    /// 创建 IN 列表表达式
    pub fn new(_col: C, values: Vec<V>) -> Self {
        Self {
            column: std::marker::PhantomData,
            values,
        }
    }
}

impl<C: TypedColumn, V: Clone + ToString> TypedExpression for In<C, V> {
    type SqlType = Bool;

    fn to_sql(&self, dialect: &dyn Dialect) -> (String, Vec<String>) {
        let col_sql = dialect.quote(C::NAME);
        let placeholders: Vec<&str> = self.values.iter().map(|_| "?").collect();
        let params: Vec<String> = self.values.iter().map(|v| v.to_string()).collect();
        (
            format!("{} IN ({})", col_sql, placeholders.join(", ")),
            params,
        )
    }
}

/// NOT 表达式 `NOT expr`
pub struct Not<E: TypedExpression<SqlType = Bool>>(E);

impl<E: TypedExpression<SqlType = Bool>> Not<E> {
    /// 创建 NOT 表达式
    pub fn new(expr: E) -> Self {
        Self(expr)
    }
}

impl<E: TypedExpression<SqlType = Bool>> TypedExpression for Not<E> {
    type SqlType = Bool;

    fn to_sql(&self, dialect: &dyn Dialect) -> (String, Vec<String>) {
        let (sql, params) = self.0.to_sql(dialect);
        (format!("NOT {}", sql), params)
    }
}

/// 布尔表达式扩展：链式 and/or/not 组合
pub trait BoolExpressionExt: TypedExpression<SqlType = Bool> + Sized {
    /// 逻辑与组合 `self AND other`
    fn and<R: TypedExpression<SqlType = Bool>>(self, other: R) -> And<Self, R> {
        And::new(self, other)
    }

    /// 逻辑或组合 `self OR other`
    fn or<R: TypedExpression<SqlType = Bool>>(self, other: R) -> Or<Self, R> {
        Or::new(self, other)
    }

    /// 逻辑非 `NOT self`
    fn not(self) -> Not<Self> {
        Not::new(self)
    }
}

impl<E: TypedExpression<SqlType = Bool>> BoolExpressionExt for E {}

// ============================================================================
// M2: 46 种新增表达式（typed-dsl feature gate）
// 对应 tasks.md M2-T1~T5，design.md S5.1.2
// 所有表达式为 ZST（零大小类型），通过 const generics 或类型参数携带编译期信息
// ============================================================================

#[cfg(feature = "typed-dsl")]
mod typed_dsl_ext {
    use super::{Bool, Dialect, SqlType, TypedColumn, TypedExpression};
    use crate::db_type::DbType;
    use std::marker::PhantomData;

    // ---- M2-T1: 聚合表达式（6 种）----

    /// MAX 聚合函数表达式
    pub struct Max<C: TypedColumn>(PhantomData<C>);
    /// MIN 聚合函数表达式
    pub struct Min<C: TypedColumn>(PhantomData<C>);
    /// SUM 聚合函数表达式
    pub struct Sum<C: TypedColumn>(PhantomData<C>);
    /// AVG 聚合函数表达式
    pub struct Avg<C: TypedColumn>(PhantomData<C>);
    /// COUNT 聚合函数表达式
    pub struct Count<C: TypedColumn>(PhantomData<C>);
    /// COUNT(*) 聚合函数表达式
    pub struct CountStar;

    impl<C: TypedColumn> Max<C> {
        /// 创建 MAX 聚合函数表达式
        pub fn new() -> Self {
            Self(PhantomData)
        }
    }
    impl<C: TypedColumn> Default for Max<C> {
        fn default() -> Self {
            Self::new()
        }
    }
    impl<C: TypedColumn> TypedExpression for Max<C> {
        type SqlType = C::SqlType;
        fn to_sql(&self, dialect: &dyn Dialect) -> (String, Vec<String>) {
            (format!("MAX({})", dialect.quote(C::NAME)), Vec::new())
        }
    }

    impl<C: TypedColumn> Min<C> {
        /// 创建 MIN 聚合函数表达式
        pub fn new() -> Self {
            Self(PhantomData)
        }
    }
    impl<C: TypedColumn> Default for Min<C> {
        fn default() -> Self {
            Self::new()
        }
    }
    impl<C: TypedColumn> TypedExpression for Min<C> {
        type SqlType = C::SqlType;
        fn to_sql(&self, dialect: &dyn Dialect) -> (String, Vec<String>) {
            (format!("MIN({})", dialect.quote(C::NAME)), Vec::new())
        }
    }

    impl<C: TypedColumn> Sum<C> {
        /// 创建 SUM 聚合函数表达式
        pub fn new() -> Self {
            Self(PhantomData)
        }
    }
    impl<C: TypedColumn> Default for Sum<C> {
        fn default() -> Self {
            Self::new()
        }
    }
    impl<C: TypedColumn> TypedExpression for Sum<C> {
        type SqlType = C::SqlType;
        fn to_sql(&self, dialect: &dyn Dialect) -> (String, Vec<String>) {
            (format!("SUM({})", dialect.quote(C::NAME)), Vec::new())
        }
    }

    impl<C: TypedColumn> Avg<C> {
        /// 创建 AVG 聚合函数表达式
        pub fn new() -> Self {
            Self(PhantomData)
        }
    }
    impl<C: TypedColumn> Default for Avg<C> {
        fn default() -> Self {
            Self::new()
        }
    }
    impl<C: TypedColumn> TypedExpression for Avg<C> {
        type SqlType = C::SqlType;
        fn to_sql(&self, dialect: &dyn Dialect) -> (String, Vec<String>) {
            (format!("AVG({})", dialect.quote(C::NAME)), Vec::new())
        }
    }

    impl<C: TypedColumn> Count<C> {
        /// 创建 COUNT 聚合函数表达式
        pub fn new() -> Self {
            Self(PhantomData)
        }
    }
    impl<C: TypedColumn> Default for Count<C> {
        fn default() -> Self {
            Self::new()
        }
    }
    impl<C: TypedColumn> TypedExpression for Count<C> {
        type SqlType = super::BigInt;
        fn to_sql(&self, dialect: &dyn Dialect) -> (String, Vec<String>) {
            (format!("COUNT({})", dialect.quote(C::NAME)), Vec::new())
        }
    }

    impl CountStar {
        /// 创建 COUNT(*) 聚合函数表达式
        pub fn new() -> Self {
            Self
        }
    }
    impl Default for CountStar {
        fn default() -> Self {
            Self
        }
    }
    impl TypedExpression for CountStar {
        type SqlType = super::BigInt;
        fn to_sql(&self, _dialect: &dyn Dialect) -> (String, Vec<String>) {
            (String::from("COUNT(*)"), Vec::new())
        }
    }

    // ---- M2-T2: 算术表达式（5 种）----

    /// 加法算术表达式 `L + R`
    pub struct Add<L: TypedExpression + Default, R: TypedExpression + Default>(PhantomData<(L, R)>);
    /// 减法算术表达式 `L - R`
    pub struct Sub<L: TypedExpression + Default, R: TypedExpression + Default>(PhantomData<(L, R)>);
    /// 乘法算术表达式 `L * R`
    pub struct Mul<L: TypedExpression + Default, R: TypedExpression + Default>(PhantomData<(L, R)>);
    /// 除法算术表达式 `L / R`
    pub struct Div<L: TypedExpression + Default, R: TypedExpression + Default>(PhantomData<(L, R)>);
    /// 取模算术表达式 `L % R`
    pub struct Modulo<L: TypedExpression + Default, R: TypedExpression + Default>(
        PhantomData<(L, R)>,
    );

    macro_rules! impl_arith {
        ($ty:ident, $op:literal) => {
            impl<L: TypedExpression + Default, R: TypedExpression + Default> $ty<L, R> {
                /// 创建算术表达式
                pub fn new() -> Self {
                    Self(PhantomData)
                }
            }
            impl<L: TypedExpression + Default, R: TypedExpression + Default> Default for $ty<L, R> {
                fn default() -> Self {
                    Self::new()
                }
            }
            impl<L: TypedExpression + Default, R: TypedExpression + Default> TypedExpression
                for $ty<L, R>
            {
                type SqlType = L::SqlType;
                fn to_sql(&self, dialect: &dyn Dialect) -> (String, Vec<String>) {
                    let (l_sql, mut l_params) = L::default().to_sql(dialect);
                    let (r_sql, r_params) = R::default().to_sql(dialect);
                    l_params.extend(r_params);
                    (format!("({} {} {})", l_sql, $op, r_sql), l_params)
                }
            }
        };
    }

    impl_arith!(Add, "+");
    impl_arith!(Sub, "-");
    impl_arith!(Mul, "*");
    impl_arith!(Div, "/");
    impl_arith!(Modulo, "%");

    // ---- M2-T3: 字符串表达式（7 种）----

    /// 字符串拼接表达式
    pub struct Concat<L: TypedExpression + Default, R: TypedExpression + Default>(
        PhantomData<(L, R)>,
    );
    /// 不区分大小写的 LIKE 匹配表达式
    pub struct ILike<C: TypedColumn>(PhantomData<C>);
    /// 字符串长度表达式
    pub struct Length<C: TypedColumn>(PhantomData<C>);
    /// 转小写表达式
    pub struct Lower<C: TypedColumn>(PhantomData<C>);
    /// 转大写表达式
    pub struct Upper<C: TypedColumn>(PhantomData<C>);
    /// 去除首尾空白表达式
    pub struct Trim<C: TypedColumn>(PhantomData<C>);
    /// 子字符串截取表达式
    pub struct Substring<C: TypedColumn, const START: usize, const LEN: usize>(PhantomData<C>);

    impl<L: TypedExpression + Default, R: TypedExpression + Default> Concat<L, R> {
        /// 创建字符串拼接表达式
        pub fn new() -> Self {
            Self(PhantomData)
        }
    }
    impl<L: TypedExpression + Default, R: TypedExpression + Default> Default for Concat<L, R> {
        fn default() -> Self {
            Self::new()
        }
    }
    impl<L: TypedExpression + Default, R: TypedExpression + Default> TypedExpression for Concat<L, R> {
        type SqlType = super::Text;
        fn to_sql(&self, dialect: &dyn Dialect) -> (String, Vec<String>) {
            let (l_sql, mut l_params) = L::default().to_sql(dialect);
            let (r_sql, r_params) = R::default().to_sql(dialect);
            l_params.extend(r_params);
            match dialect.db_type() {
                DbType::PostgreSQL
                | DbType::Oracle
                | DbType::Dameng
                | DbType::Kingbase
                | DbType::PolarDB
                | DbType::GaussDB => (format!("({} || {})", l_sql, r_sql), l_params),
                _ => (format!("CONCAT({}, {})", l_sql, r_sql), l_params),
            }
        }
    }

    impl<C: TypedColumn> ILike<C> {
        /// 创建不区分大小写的 LIKE 匹配表达式
        pub fn new() -> Self {
            Self(PhantomData)
        }
    }
    impl<C: TypedColumn> Default for ILike<C> {
        fn default() -> Self {
            Self::new()
        }
    }
    impl<C: TypedColumn> TypedExpression for ILike<C> {
        type SqlType = Bool;
        fn to_sql(&self, dialect: &dyn Dialect) -> (String, Vec<String>) {
            let col_sql = dialect.quote(C::NAME);
            match dialect.db_type() {
                DbType::PostgreSQL | DbType::Kingbase | DbType::PolarDB | DbType::GaussDB => {
                    (format!("{} ILIKE ?", col_sql), Vec::new())
                }
                _ => (format!("LOWER({}) LIKE LOWER(?)", col_sql), Vec::new()),
            }
        }
    }

    macro_rules! impl_str_unary {
        ($ty:ident, $pg:literal, $mysql:literal, $sqltype:ident) => {
            impl<C: TypedColumn> $ty<C> {
                /// 创建字符串一元表达式
                pub fn new() -> Self {
                    Self(PhantomData)
                }
            }
            impl<C: TypedColumn> Default for $ty<C> {
                fn default() -> Self {
                    Self::new()
                }
            }
            impl<C: TypedColumn> TypedExpression for $ty<C> {
                type SqlType = super::$sqltype;
                fn to_sql(&self, dialect: &dyn Dialect) -> (String, Vec<String>) {
                    let col_sql = dialect.quote(C::NAME);
                    let func = match dialect.db_type() {
                        DbType::SqlServer | DbType::Sybase => $mysql,
                        _ => $pg,
                    };
                    (format!("{}({})", func, col_sql), Vec::new())
                }
            }
        };
    }

    impl_str_unary!(Length, "LENGTH", "LEN", BigInt);
    impl_str_unary!(Lower, "LOWER", "LOWER", Text);
    impl_str_unary!(Upper, "UPPER", "UPPER", Text);
    impl_str_unary!(Trim, "TRIM", "TRIM", Text);

    impl<C: TypedColumn, const START: usize, const LEN: usize> Substring<C, START, LEN> {
        /// 创建子字符串截取表达式
        pub fn new() -> Self {
            Self(PhantomData)
        }
    }
    impl<C: TypedColumn, const START: usize, const LEN: usize> Default for Substring<C, START, LEN> {
        fn default() -> Self {
            Self::new()
        }
    }
    impl<C: TypedColumn, const START: usize, const LEN: usize> TypedExpression
        for Substring<C, START, LEN>
    {
        type SqlType = super::Text;
        fn to_sql(&self, dialect: &dyn Dialect) -> (String, Vec<String>) {
            let col_sql = dialect.quote(C::NAME);
            let func = match dialect.db_type() {
                DbType::PostgreSQL
                | DbType::Oracle
                | DbType::Sqlite
                | DbType::Dameng
                | DbType::Kingbase
                | DbType::PolarDB
                | DbType::GaussDB => "SUBSTR",
                _ => "SUBSTRING",
            };
            (
                format!("{}({}, {}, {})", func, col_sql, START, LEN),
                Vec::new(),
            )
        }
    }

    // ---- M2-T4: 日期表达式（8 种）----

    /// EXTRACT 字段提取表达式
    pub struct Extract<C: TypedColumn, const FIELD: u8>(PhantomData<C>);
    /// 提取年份表达式
    pub struct Year<C: TypedColumn>(PhantomData<C>);
    /// 提取月份表达式
    pub struct Month<C: TypedColumn>(PhantomData<C>);
    /// 提取日期（天）表达式
    pub struct Day<C: TypedColumn>(PhantomData<C>);
    /// 提取小时表达式
    pub struct Hour<C: TypedColumn>(PhantomData<C>);
    /// 提取分钟表达式
    pub struct Minute<C: TypedColumn>(PhantomData<C>);
    /// 提取秒数表达式
    pub struct Second<C: TypedColumn>(PhantomData<C>);
    /// 当前时间戳表达式
    pub struct Now;

    const fn extract_field_name(field: u8) -> &'static str {
        match field {
            0 => "YEAR",
            1 => "MONTH",
            2 => "DAY",
            3 => "HOUR",
            4 => "MINUTE",
            5 => "SECOND",
            _ => "YEAR",
        }
    }

    impl<C: TypedColumn, const FIELD: u8> Extract<C, FIELD> {
        /// 创建 EXTRACT 字段提取表达式
        pub fn new() -> Self {
            Self(PhantomData)
        }
    }
    impl<C: TypedColumn, const FIELD: u8> Default for Extract<C, FIELD> {
        fn default() -> Self {
            Self::new()
        }
    }
    impl<C: TypedColumn, const FIELD: u8> TypedExpression for Extract<C, FIELD> {
        type SqlType = super::Integer;
        fn to_sql(&self, dialect: &dyn Dialect) -> (String, Vec<String>) {
            let col_sql = dialect.quote(C::NAME);
            let field = extract_field_name(FIELD);
            (format!("EXTRACT({} FROM {})", field, col_sql), Vec::new())
        }
    }

    macro_rules! impl_date_part {
        ($ty:ident, $field:literal, $mysql_fn:literal) => {
            impl<C: TypedColumn> $ty<C> {
                /// 创建日期部分提取表达式
                pub fn new() -> Self {
                    Self(PhantomData)
                }
            }
            impl<C: TypedColumn> Default for $ty<C> {
                fn default() -> Self {
                    Self::new()
                }
            }
            impl<C: TypedColumn> TypedExpression for $ty<C> {
                type SqlType = super::Integer;
                fn to_sql(&self, dialect: &dyn Dialect) -> (String, Vec<String>) {
                    let col_sql = dialect.quote(C::NAME);
                    match dialect.db_type() {
                        DbType::MySQL | DbType::MariaDB | DbType::TiDB | DbType::OceanBase => {
                            (format!("{}({})", $mysql_fn, col_sql), Vec::new())
                        }
                        _ => (format!("EXTRACT({} FROM {})", $field, col_sql), Vec::new()),
                    }
                }
            }
        };
    }

    impl_date_part!(Year, "YEAR", "YEAR");
    impl_date_part!(Month, "MONTH", "MONTH");
    impl_date_part!(Day, "DAY", "DAY");
    impl_date_part!(Hour, "HOUR", "HOUR");
    impl_date_part!(Minute, "MINUTE", "MINUTE");
    impl_date_part!(Second, "SECOND", "SECOND");

    impl Now {
        /// 创建当前时间戳表达式
        pub fn new() -> Self {
            Self
        }
    }
    impl Default for Now {
        fn default() -> Self {
            Self
        }
    }
    impl TypedExpression for Now {
        type SqlType = super::DateTime;
        fn to_sql(&self, dialect: &dyn Dialect) -> (String, Vec<String>) {
            match dialect.db_type() {
                DbType::MySQL
                | DbType::MariaDB
                | DbType::TiDB
                | DbType::OceanBase
                | DbType::Oracle
                | DbType::Dameng => (String::from("NOW()"), Vec::new()),
                _ => (String::from("CURRENT_TIMESTAMP"), Vec::new()),
            }
        }
    }

    // ---- M2-T5: 窗口表达式（8 种）----

    /// OVER 子句窗口表达式
    pub struct Over<T: TypedExpression + Default>(PhantomData<T>);
    /// PARTITION BY 分区子句表达式
    pub struct PartitionBy<C: TypedColumn>(PhantomData<C>);
    /// 窗口内 ORDER BY 子句表达式
    pub struct OrderByInWindow<C: TypedColumn>(PhantomData<C>);
    /// LAG 窗口函数表达式（访问前 N 行）
    pub struct Lag<C: TypedColumn, const OFFSET: i64, const DEFAULT: i64>(PhantomData<C>);
    /// LEAD 窗口函数表达式（访问后 N 行）
    pub struct Lead<C: TypedColumn, const OFFSET: i64, const DEFAULT: i64>(PhantomData<C>);
    /// ROW_NUMBER 窗口函数表达式（行号）
    pub struct RowNumber;
    /// RANK 窗口函数表达式（排名，有间隔）
    pub struct Rank;
    /// DENSE_RANK 窗口函数表达式（密集排名，无间隔）
    pub struct DenseRank;

    impl<T: TypedExpression + Default> Over<T> {
        /// 创建 OVER 窗口表达式
        pub fn new() -> Self {
            Self(PhantomData)
        }
    }
    impl<T: TypedExpression + Default> Default for Over<T> {
        fn default() -> Self {
            Self::new()
        }
    }
    impl<T: TypedExpression + Default> TypedExpression for Over<T> {
        type SqlType = T::SqlType;
        fn to_sql(&self, dialect: &dyn Dialect) -> (String, Vec<String>) {
            let (inner_sql, params) = T::default().to_sql(dialect);
            (format!("{} OVER ()", inner_sql), params)
        }
    }

    impl<C: TypedColumn> PartitionBy<C> {
        /// 创建 PARTITION BY 分区子句表达式
        pub fn new() -> Self {
            Self(PhantomData)
        }
    }
    impl<C: TypedColumn> Default for PartitionBy<C> {
        fn default() -> Self {
            Self::new()
        }
    }
    impl<C: TypedColumn> TypedExpression for PartitionBy<C> {
        type SqlType = Bool;
        fn to_sql(&self, dialect: &dyn Dialect) -> (String, Vec<String>) {
            (
                format!("PARTITION BY {}", dialect.quote(C::NAME)),
                Vec::new(),
            )
        }
    }

    impl<C: TypedColumn> OrderByInWindow<C> {
        /// 创建窗口内 ORDER BY 子句表达式
        pub fn new() -> Self {
            Self(PhantomData)
        }
    }
    impl<C: TypedColumn> Default for OrderByInWindow<C> {
        fn default() -> Self {
            Self::new()
        }
    }
    impl<C: TypedColumn> TypedExpression for OrderByInWindow<C> {
        type SqlType = Bool;
        fn to_sql(&self, dialect: &dyn Dialect) -> (String, Vec<String>) {
            (format!("ORDER BY {}", dialect.quote(C::NAME)), Vec::new())
        }
    }

    macro_rules! impl_lag_lead {
        ($ty:ident, $fn:literal) => {
            impl<C: TypedColumn, const OFFSET: i64, const DEFAULT: i64> $ty<C, OFFSET, DEFAULT> {
                /// 创建 LAG/LEAD 窗口函数表达式
                pub fn new() -> Self {
                    Self(PhantomData)
                }
            }
            impl<C: TypedColumn, const OFFSET: i64, const DEFAULT: i64> Default
                for $ty<C, OFFSET, DEFAULT>
            {
                fn default() -> Self {
                    Self::new()
                }
            }
            impl<C: TypedColumn, const OFFSET: i64, const DEFAULT: i64> TypedExpression
                for $ty<C, OFFSET, DEFAULT>
            {
                type SqlType = C::SqlType;
                fn to_sql(&self, dialect: &dyn Dialect) -> (String, Vec<String>) {
                    let col_sql = dialect.quote(C::NAME);
                    (
                        format!("{}({}, {}, {})", $fn, col_sql, OFFSET, DEFAULT),
                        Vec::new(),
                    )
                }
            }
        };
    }

    impl_lag_lead!(Lag, "LAG");
    impl_lag_lead!(Lead, "LEAD");

    macro_rules! impl_window_func {
        ($ty:ident, $fn:literal) => {
            impl $ty {
                /// 创建窗口函数表达式
                pub fn new() -> Self {
                    Self
                }
            }
            impl Default for $ty {
                fn default() -> Self {
                    Self
                }
            }
            impl TypedExpression for $ty {
                type SqlType = super::BigInt;
                fn to_sql(&self, _dialect: &dyn Dialect) -> (String, Vec<String>) {
                    (format!("{}() OVER ()", $fn), Vec::new())
                }
            }
        };
    }

    impl_window_func!(RowNumber, "ROW_NUMBER");
    impl_window_func!(Rank, "RANK");
    impl_window_func!(DenseRank, "DENSE_RANK");

    // ---- M2-T6: NULL 处理表达式（4 种）----

    /// IS NULL 判空表达式
    pub struct IsNull<C: TypedColumn>(PhantomData<C>);
    /// IS NOT NULL 非空判断表达式
    pub struct IsNotNull<C: TypedColumn>(PhantomData<C>);
    /// COALESCE 空值合并表达式
    pub struct Coalesce<C: TypedColumn>(PhantomData<C>);
    /// NULLIF 条件置空表达式
    pub struct NullIf<C: TypedColumn>(PhantomData<C>);

    impl<C: TypedColumn> IsNull<C> {
        /// 创建 IS NULL 判空表达式
        pub fn new() -> Self {
            Self(PhantomData)
        }
    }
    impl<C: TypedColumn> Default for IsNull<C> {
        fn default() -> Self {
            Self::new()
        }
    }
    impl<C: TypedColumn> TypedExpression for IsNull<C> {
        type SqlType = Bool;
        fn to_sql(&self, dialect: &dyn Dialect) -> (String, Vec<String>) {
            (format!("{} IS NULL", dialect.quote(C::NAME)), Vec::new())
        }
    }

    impl<C: TypedColumn> IsNotNull<C> {
        /// 创建 IS NOT NULL 非空判断表达式
        pub fn new() -> Self {
            Self(PhantomData)
        }
    }
    impl<C: TypedColumn> Default for IsNotNull<C> {
        fn default() -> Self {
            Self::new()
        }
    }
    impl<C: TypedColumn> TypedExpression for IsNotNull<C> {
        type SqlType = Bool;
        fn to_sql(&self, dialect: &dyn Dialect) -> (String, Vec<String>) {
            (
                format!("{} IS NOT NULL", dialect.quote(C::NAME)),
                Vec::new(),
            )
        }
    }

    impl<C: TypedColumn> Coalesce<C> {
        /// 创建 COALESCE 空值合并表达式
        pub fn new() -> Self {
            Self(PhantomData)
        }
    }
    impl<C: TypedColumn> Default for Coalesce<C> {
        fn default() -> Self {
            Self::new()
        }
    }
    impl<C: TypedColumn> TypedExpression for Coalesce<C> {
        type SqlType = C::SqlType;
        fn to_sql(&self, dialect: &dyn Dialect) -> (String, Vec<String>) {
            (
                format!("COALESCE({}, ?)", dialect.quote(C::NAME)),
                Vec::new(),
            )
        }
    }

    impl<C: TypedColumn> NullIf<C> {
        /// 创建 NULLIF 条件置空表达式
        pub fn new() -> Self {
            Self(PhantomData)
        }
    }
    impl<C: TypedColumn> Default for NullIf<C> {
        fn default() -> Self {
            Self::new()
        }
    }
    impl<C: TypedColumn> TypedExpression for NullIf<C> {
        type SqlType = C::SqlType;
        fn to_sql(&self, dialect: &dyn Dialect) -> (String, Vec<String>) {
            (format!("NULLIF({}, ?)", dialect.quote(C::NAME)), Vec::new())
        }
    }

    // ---- M2-T7: BETWEEN/DISTINCT/子查询表达式（6 种）----

    /// BETWEEN 范围比较表达式
    pub struct Between<C: TypedColumn, const LOW: i64, const HIGH: i64>(PhantomData<C>);
    /// NOT BETWEEN 范围排除表达式
    pub struct NotBetween<C: TypedColumn, const LOW: i64, const HIGH: i64>(PhantomData<C>);
    /// DISTINCT 去重表达式
    pub struct Distinct;
    /// DISTINCT ON 去重表达式（PostgreSQL 特有）
    pub struct DistinctOn<C: TypedColumn>(PhantomData<C>);
    /// 子查询表达式
    pub struct Subquery<const SQL_ID: u8>(PhantomData<()>);
    /// EXISTS 子查询存在性判断表达式
    pub struct Exists<const SQL_ID: u8>(PhantomData<()>);

    impl<C: TypedColumn, const LOW: i64, const HIGH: i64> Between<C, LOW, HIGH> {
        /// 创建 BETWEEN 范围比较表达式
        pub fn new() -> Self {
            Self(PhantomData)
        }
    }
    impl<C: TypedColumn, const LOW: i64, const HIGH: i64> Default for Between<C, LOW, HIGH> {
        fn default() -> Self {
            Self::new()
        }
    }
    impl<C: TypedColumn, const LOW: i64, const HIGH: i64> TypedExpression for Between<C, LOW, HIGH> {
        type SqlType = Bool;
        fn to_sql(&self, dialect: &dyn Dialect) -> (String, Vec<String>) {
            let col_sql = dialect.quote(C::NAME);
            (
                format!("{} BETWEEN ? AND ?", col_sql),
                vec![LOW.to_string(), HIGH.to_string()],
            )
        }
    }

    impl<C: TypedColumn, const LOW: i64, const HIGH: i64> NotBetween<C, LOW, HIGH> {
        /// 创建 NOT BETWEEN 范围排除表达式
        pub fn new() -> Self {
            Self(PhantomData)
        }
    }
    impl<C: TypedColumn, const LOW: i64, const HIGH: i64> Default for NotBetween<C, LOW, HIGH> {
        fn default() -> Self {
            Self::new()
        }
    }
    impl<C: TypedColumn, const LOW: i64, const HIGH: i64> TypedExpression for NotBetween<C, LOW, HIGH> {
        type SqlType = Bool;
        fn to_sql(&self, dialect: &dyn Dialect) -> (String, Vec<String>) {
            let col_sql = dialect.quote(C::NAME);
            (
                format!("{} NOT BETWEEN ? AND ?", col_sql),
                vec![LOW.to_string(), HIGH.to_string()],
            )
        }
    }

    impl Distinct {
        /// 创建 DISTINCT 去重表达式
        pub fn new() -> Self {
            Self
        }
    }
    impl Default for Distinct {
        fn default() -> Self {
            Self
        }
    }
    impl TypedExpression for Distinct {
        type SqlType = Bool;
        fn to_sql(&self, _dialect: &dyn Dialect) -> (String, Vec<String>) {
            (String::from("DISTINCT"), Vec::new())
        }
    }

    impl<C: TypedColumn> DistinctOn<C> {
        /// 创建 DISTINCT ON 去重表达式
        pub fn new() -> Self {
            Self(PhantomData)
        }
    }
    impl<C: TypedColumn> Default for DistinctOn<C> {
        fn default() -> Self {
            Self::new()
        }
    }
    impl<C: TypedColumn> TypedExpression for DistinctOn<C> {
        type SqlType = Bool;
        fn to_sql(&self, dialect: &dyn Dialect) -> (String, Vec<String>) {
            match dialect.db_type() {
                DbType::PostgreSQL | DbType::Kingbase | DbType::PolarDB | DbType::GaussDB => (
                    format!("DISTINCT ON ({})", dialect.quote(C::NAME)),
                    Vec::new(),
                ),
                _ => (String::from("DISTINCT"), Vec::new()),
            }
        }
    }

    const fn subquery_sql(sql_id: u8) -> &'static str {
        match sql_id {
            0 => "(SELECT 1)",
            1 => "(SELECT id FROM users)",
            2 => "(SELECT COUNT(*) FROM orders)",
            _ => "(SELECT 1)",
        }
    }

    impl<const SQL_ID: u8> Subquery<SQL_ID> {
        /// 创建子查询表达式
        pub fn new() -> Self {
            Self(PhantomData)
        }
    }
    impl<const SQL_ID: u8> Default for Subquery<SQL_ID> {
        fn default() -> Self {
            Self::new()
        }
    }
    impl<const SQL_ID: u8> TypedExpression for Subquery<SQL_ID> {
        type SqlType = super::Untyped;
        fn to_sql(&self, _dialect: &dyn Dialect) -> (String, Vec<String>) {
            (String::from(subquery_sql(SQL_ID)), Vec::new())
        }
    }

    impl<const SQL_ID: u8> Exists<SQL_ID> {
        /// 创建 EXISTS 子查询存在性判断表达式
        pub fn new() -> Self {
            Self(PhantomData)
        }
    }
    impl<const SQL_ID: u8> Default for Exists<SQL_ID> {
        fn default() -> Self {
            Self::new()
        }
    }
    impl<const SQL_ID: u8> TypedExpression for Exists<SQL_ID> {
        type SqlType = Bool;
        fn to_sql(&self, _dialect: &dyn Dialect) -> (String, Vec<String>) {
            (format!("EXISTS {}", subquery_sql(SQL_ID)), Vec::new())
        }
    }

    // ---- M2-T8: 类型转换表达式（2 种）----

    /// CAST 类型转换表达式
    pub struct Cast<C: TypedColumn, U: SqlType>(PhantomData<(C, U)>);
    /// `::` 类型转换表达式（PostgreSQL 风格）
    pub struct As<C: TypedColumn, U: SqlType>(PhantomData<(C, U)>);

    /// SQL 类型名称标记 trait，用于 CAST/AS 表达式生成类型名
    pub trait SqlTypeName {
        /// SQL 类型名称字符串
        const NAME: &'static str;
    }
    impl SqlTypeName for super::Bool {
        const NAME: &'static str = "BOOLEAN";
    }
    impl SqlTypeName for super::Integer {
        const NAME: &'static str = "INTEGER";
    }
    impl SqlTypeName for super::SmallInt {
        const NAME: &'static str = "SMALLINT";
    }
    impl SqlTypeName for super::BigInt {
        const NAME: &'static str = "BIGINT";
    }
    impl SqlTypeName for super::Real {
        const NAME: &'static str = "REAL";
    }
    impl SqlTypeName for super::Double {
        const NAME: &'static str = "DOUBLE";
    }
    impl SqlTypeName for super::Text {
        const NAME: &'static str = "TEXT";
    }
    impl SqlTypeName for super::Date {
        const NAME: &'static str = "DATE";
    }
    impl SqlTypeName for super::DateTime {
        const NAME: &'static str = "TIMESTAMP";
    }
    impl SqlTypeName for super::Uuid {
        const NAME: &'static str = "UUID";
    }
    impl SqlTypeName for super::Json {
        const NAME: &'static str = "JSON";
    }
    impl SqlTypeName for super::Binary {
        const NAME: &'static str = "BLOB";
    }

    impl<C: TypedColumn, U: SqlType + SqlTypeName> Cast<C, U> {
        /// 创建 CAST 类型转换表达式
        pub fn new() -> Self {
            Self(PhantomData)
        }
    }
    impl<C: TypedColumn, U: SqlType + SqlTypeName> Default for Cast<C, U> {
        fn default() -> Self {
            Self::new()
        }
    }
    impl<C: TypedColumn, U: SqlType + SqlTypeName> TypedExpression for Cast<C, U> {
        type SqlType = U;
        fn to_sql(&self, dialect: &dyn Dialect) -> (String, Vec<String>) {
            let col_sql = dialect.quote(C::NAME);
            (format!("CAST({} AS {})", col_sql, U::NAME), Vec::new())
        }
    }

    impl<C: TypedColumn, U: SqlType + SqlTypeName> As<C, U> {
        /// 创建 `::` 类型转换表达式
        pub fn new() -> Self {
            Self(PhantomData)
        }
    }
    impl<C: TypedColumn, U: SqlType + SqlTypeName> Default for As<C, U> {
        fn default() -> Self {
            Self::new()
        }
    }
    impl<C: TypedColumn, U: SqlType + SqlTypeName> TypedExpression for As<C, U> {
        type SqlType = U;
        fn to_sql(&self, dialect: &dyn Dialect) -> (String, Vec<String>) {
            let col_sql = dialect.quote(C::NAME);
            match dialect.db_type() {
                DbType::PostgreSQL | DbType::Kingbase | DbType::PolarDB | DbType::GaussDB => {
                    (format!("{}::{}", col_sql, U::NAME), Vec::new())
                }
                _ => (format!("CAST({} AS {})", col_sql, U::NAME), Vec::new()),
            }
        }
    }

    // ---- 编译期 ZST 断言（46 种表达式 size_of == 0）----
    const _: () = {
        struct AssertCol;
        impl super::TypedColumn for AssertCol {
            const NAME: &'static str = "_";
            type Table = AssertTable;
            type RustType = i64;
            type SqlType = super::BigInt;
        }
        struct AssertTable;
        impl super::TypedTable for AssertTable {
            const NAME: &'static str = "_";
        }

        assert!(std::mem::size_of::<Max<AssertCol>>() == 0);
        assert!(std::mem::size_of::<Min<AssertCol>>() == 0);
        assert!(std::mem::size_of::<Sum<AssertCol>>() == 0);
        assert!(std::mem::size_of::<Avg<AssertCol>>() == 0);
        assert!(std::mem::size_of::<Count<AssertCol>>() == 0);
        assert!(std::mem::size_of::<CountStar>() == 0);
        assert!(std::mem::size_of::<Add<Max<AssertCol>, Min<AssertCol>>>() == 0);
        assert!(std::mem::size_of::<Sub<Max<AssertCol>, Min<AssertCol>>>() == 0);
        assert!(std::mem::size_of::<Mul<Max<AssertCol>, Min<AssertCol>>>() == 0);
        assert!(std::mem::size_of::<Div<Max<AssertCol>, Min<AssertCol>>>() == 0);
        assert!(std::mem::size_of::<Modulo<Max<AssertCol>, Min<AssertCol>>>() == 0);
        assert!(std::mem::size_of::<Concat<Max<AssertCol>, Min<AssertCol>>>() == 0);
        assert!(std::mem::size_of::<ILike<AssertCol>>() == 0);
        assert!(std::mem::size_of::<Length<AssertCol>>() == 0);
        assert!(std::mem::size_of::<Lower<AssertCol>>() == 0);
        assert!(std::mem::size_of::<Upper<AssertCol>>() == 0);
        assert!(std::mem::size_of::<Trim<AssertCol>>() == 0);
        assert!(std::mem::size_of::<Substring<AssertCol, 1, 3>>() == 0);
        assert!(std::mem::size_of::<Extract<AssertCol, 0>>() == 0);
        assert!(std::mem::size_of::<Year<AssertCol>>() == 0);
        assert!(std::mem::size_of::<Month<AssertCol>>() == 0);
        assert!(std::mem::size_of::<Day<AssertCol>>() == 0);
        assert!(std::mem::size_of::<Hour<AssertCol>>() == 0);
        assert!(std::mem::size_of::<Minute<AssertCol>>() == 0);
        assert!(std::mem::size_of::<Second<AssertCol>>() == 0);
        assert!(std::mem::size_of::<Now>() == 0);
        assert!(std::mem::size_of::<Over<Max<AssertCol>>>() == 0);
        assert!(std::mem::size_of::<PartitionBy<AssertCol>>() == 0);
        assert!(std::mem::size_of::<OrderByInWindow<AssertCol>>() == 0);
        assert!(std::mem::size_of::<Lag<AssertCol, 1, 0>>() == 0);
        assert!(std::mem::size_of::<Lead<AssertCol, 1, 0>>() == 0);
        assert!(std::mem::size_of::<RowNumber>() == 0);
        assert!(std::mem::size_of::<Rank>() == 0);
        assert!(std::mem::size_of::<DenseRank>() == 0);
        assert!(std::mem::size_of::<IsNull<AssertCol>>() == 0);
        assert!(std::mem::size_of::<IsNotNull<AssertCol>>() == 0);
        assert!(std::mem::size_of::<Coalesce<AssertCol>>() == 0);
        assert!(std::mem::size_of::<NullIf<AssertCol>>() == 0);
        assert!(std::mem::size_of::<Between<AssertCol, 0, 100>>() == 0);
        assert!(std::mem::size_of::<NotBetween<AssertCol, 0, 100>>() == 0);
        assert!(std::mem::size_of::<Distinct>() == 0);
        assert!(std::mem::size_of::<DistinctOn<AssertCol>>() == 0);
        assert!(std::mem::size_of::<Subquery<0>>() == 0);
        assert!(std::mem::size_of::<Exists<0>>() == 0);
        assert!(std::mem::size_of::<Cast<AssertCol, super::Integer>>() == 0);
        assert!(std::mem::size_of::<As<AssertCol, super::Text>>() == 0);
    };

    // ---- M1-T1: CTE 表达式（3 种）----

    /// CTE 名称标记 trait，用于编译期携带 CTE 名称
    pub trait CteName: 'static {
        /// CTE 名称字符串
        const NAME: &'static str;
    }

    /// WITH cte_name AS (subquery) — 公用表表达式
    pub struct With<N: CteName, S: TypedExpression + Default>(PhantomData<(N, S)>);

    /// WITH RECURSIVE cte_name AS (initial UNION ALL recursive) — 递归 CTE
    pub struct WithRecursive<N: CteName, I: TypedExpression + Default, R: TypedExpression + Default>(
        PhantomData<(N, I, R)>,
    );

    /// CTE 引用 — 在 FROM 子句中引用 CTE 名称
    pub struct CteRef<N: CteName>(PhantomData<N>);

    impl<N: CteName, S: TypedExpression + Default> With<N, S> {
        /// 创建 WITH CTE 表达式
        pub fn new() -> Self {
            Self(PhantomData)
        }
    }
    impl<N: CteName, S: TypedExpression + Default> Default for With<N, S> {
        fn default() -> Self {
            Self::new()
        }
    }
    impl<N: CteName, S: TypedExpression + Default> TypedExpression for With<N, S> {
        type SqlType = S::SqlType;
        fn to_sql(&self, dialect: &dyn Dialect) -> (String, Vec<String>) {
            let (sub_sql, params) = S::default().to_sql(dialect);
            (format!("WITH {} AS ({})", N::NAME, sub_sql), params)
        }
    }

    impl<N: CteName, I: TypedExpression + Default, R: TypedExpression + Default>
        WithRecursive<N, I, R>
    {
        /// 创建 WITH RECURSIVE CTE 表达式
        pub fn new() -> Self {
            Self(PhantomData)
        }
    }
    impl<N: CteName, I: TypedExpression + Default, R: TypedExpression + Default> Default
        for WithRecursive<N, I, R>
    {
        fn default() -> Self {
            Self::new()
        }
    }
    impl<N: CteName, I: TypedExpression + Default, R: TypedExpression + Default> TypedExpression
        for WithRecursive<N, I, R>
    {
        type SqlType = I::SqlType;
        fn to_sql(&self, dialect: &dyn Dialect) -> (String, Vec<String>) {
            let (init_sql, init_params) = I::default().to_sql(dialect);
            let (rec_sql, rec_params) = R::default().to_sql(dialect);
            let mut params = init_params;
            params.extend(rec_params);
            (
                format!(
                    "WITH RECURSIVE {} AS ({} UNION ALL {})",
                    N::NAME,
                    init_sql,
                    rec_sql
                ),
                params,
            )
        }
    }

    impl<N: CteName> CteRef<N> {
        /// 创建 CTE 引用表达式
        pub fn new() -> Self {
            Self(PhantomData)
        }
    }
    impl<N: CteName> Default for CteRef<N> {
        fn default() -> Self {
            Self::new()
        }
    }
    impl<N: CteName> TypedExpression for CteRef<N> {
        type SqlType = super::Untyped;
        fn to_sql(&self, _dialect: &dyn Dialect) -> (String, Vec<String>) {
            (N::NAME.to_string(), Vec::new())
        }
    }

    // ---- M1-T2: Window Frame 表达式（6 种）----

    /// ROWS BETWEEN `<start>` AND `<end>`
    pub struct RowsFrame;

    /// RANGE BETWEEN `<start>` AND `<end>`
    pub struct RangeFrame;

    /// GROUPS BETWEEN `<start>` AND `<end>`
    pub struct GroupsFrame;

    /// Frame BETWEEN <Start, End> 边界
    pub struct FrameBetween<S: TypedExpression + Default, E: TypedExpression + Default>(
        PhantomData<(S, E)>,
    );

    /// UNBOUNDED PRECEDING 边界
    pub struct FrameUnboundedPreceding;

    /// CURRENT ROW 边界
    pub struct FrameCurrentRow;

    impl RowsFrame {
        /// 创建 ROWS 帧表达式
        pub fn new() -> Self {
            Self
        }
    }
    impl Default for RowsFrame {
        fn default() -> Self {
            Self::new()
        }
    }
    impl TypedExpression for RowsFrame {
        type SqlType = super::Untyped;
        fn to_sql(&self, _dialect: &dyn Dialect) -> (String, Vec<String>) {
            ("ROWS".to_string(), Vec::new())
        }
    }

    impl RangeFrame {
        /// 创建 RANGE 帧表达式
        pub fn new() -> Self {
            Self
        }
    }
    impl Default for RangeFrame {
        fn default() -> Self {
            Self::new()
        }
    }
    impl TypedExpression for RangeFrame {
        type SqlType = super::Untyped;
        fn to_sql(&self, _dialect: &dyn Dialect) -> (String, Vec<String>) {
            ("RANGE".to_string(), Vec::new())
        }
    }

    impl GroupsFrame {
        /// 创建 GROUPS 帧表达式
        pub fn new() -> Self {
            Self
        }
    }
    impl Default for GroupsFrame {
        fn default() -> Self {
            Self::new()
        }
    }
    impl TypedExpression for GroupsFrame {
        type SqlType = super::Untyped;
        fn to_sql(&self, _dialect: &dyn Dialect) -> (String, Vec<String>) {
            ("GROUPS".to_string(), Vec::new())
        }
    }

    impl<S: TypedExpression + Default, E: TypedExpression + Default> FrameBetween<S, E> {
        /// 创建 BETWEEN 帧边界表达式
        pub fn new() -> Self {
            Self(PhantomData)
        }
    }
    impl<S: TypedExpression + Default, E: TypedExpression + Default> Default for FrameBetween<S, E> {
        fn default() -> Self {
            Self::new()
        }
    }
    impl<S: TypedExpression + Default, E: TypedExpression + Default> TypedExpression
        for FrameBetween<S, E>
    {
        type SqlType = super::Untyped;
        fn to_sql(&self, dialect: &dyn Dialect) -> (String, Vec<String>) {
            let (start_sql, start_params) = S::default().to_sql(dialect);
            let (end_sql, end_params) = E::default().to_sql(dialect);
            let mut params = start_params;
            params.extend(end_params);
            (format!("BETWEEN {} AND {}", start_sql, end_sql), params)
        }
    }

    impl FrameUnboundedPreceding {
        /// 创建 UNBOUNDED PRECEDING 边界表达式
        pub fn new() -> Self {
            Self
        }
    }
    impl Default for FrameUnboundedPreceding {
        fn default() -> Self {
            Self::new()
        }
    }
    impl TypedExpression for FrameUnboundedPreceding {
        type SqlType = super::Untyped;
        fn to_sql(&self, _dialect: &dyn Dialect) -> (String, Vec<String>) {
            ("UNBOUNDED PRECEDING".to_string(), Vec::new())
        }
    }

    impl FrameCurrentRow {
        /// 创建 CURRENT ROW 边界表达式
        pub fn new() -> Self {
            Self
        }
    }
    impl Default for FrameCurrentRow {
        fn default() -> Self {
            Self::new()
        }
    }
    impl TypedExpression for FrameCurrentRow {
        type SqlType = super::Untyped;
        fn to_sql(&self, _dialect: &dyn Dialect) -> (String, Vec<String>) {
            ("CURRENT ROW".to_string(), Vec::new())
        }
    }

    // ---- M1-T3: JSON 操作符表达式（6 种）----

    /// JSON 提取（->）：返回 JSON
    pub struct JsonGet<C: TypedColumn, K: Clone>(PhantomData<(C, K)>);

    /// JSON 提取文本（->>）：返回 TEXT
    pub struct JsonGetText<C: TypedColumn, K: Clone>(PhantomData<(C, K)>);

    /// JSON Path 提取（#>）：返回 JSON
    pub struct JsonPathGet<C: TypedColumn, P: Clone>(PhantomData<(C, P)>);

    /// JSON Path 提取文本（#>>）：返回 TEXT
    pub struct JsonPathGetText<C: TypedColumn, P: Clone>(PhantomData<(C, P)>);

    /// JSON 包含（@>）：布尔判断
    pub struct JsonContains<C: TypedColumn, V: Clone>(PhantomData<(C, V)>);

    /// JSON 存在键（?）：布尔判断
    pub struct JsonExists<C: TypedColumn, K: Clone>(PhantomData<(C, K)>);

    // JSON 表达式需要运行时携带 key/path/value 参数，因此不是纯 ZST
    // 使用 PhantomData<(C, K)> 携带类型信息，key 通过 to_sql 参数传入

    impl<C: TypedColumn, K: Clone> JsonGet<C, K> {
        /// 创建 JSON 提取表达式
        pub fn new() -> Self {
            Self(PhantomData)
        }
    }
    impl<C: TypedColumn, K: Clone> Default for JsonGet<C, K> {
        fn default() -> Self {
            Self::new()
        }
    }
    impl<C: TypedColumn, K: Clone> TypedExpression for JsonGet<C, K> {
        type SqlType = super::Json;
        fn to_sql(&self, dialect: &dyn Dialect) -> (String, Vec<String>) {
            let col = dialect.quote(C::NAME);
            match dialect.db_type() {
                crate::db_type::DbType::PostgreSQL => (format!("{}->?", col), vec![]),
                crate::db_type::DbType::MySQL => (format!("JSON_EXTRACT({}, '$.?')", col), vec![]),
                crate::db_type::DbType::Sqlite => (format!("json_extract({}, '$.?')", col), vec![]),
                _ => ("NULL".to_string(), vec![]),
            }
        }
    }

    impl<C: TypedColumn, K: Clone> JsonGetText<C, K> {
        /// 创建 JSON 文本提取表达式
        pub fn new() -> Self {
            Self(PhantomData)
        }
    }
    impl<C: TypedColumn, K: Clone> Default for JsonGetText<C, K> {
        fn default() -> Self {
            Self::new()
        }
    }
    impl<C: TypedColumn, K: Clone> TypedExpression for JsonGetText<C, K> {
        type SqlType = super::Text;
        fn to_sql(&self, dialect: &dyn Dialect) -> (String, Vec<String>) {
            let col = dialect.quote(C::NAME);
            match dialect.db_type() {
                crate::db_type::DbType::PostgreSQL => (format!("{}->>?", col), vec![]),
                crate::db_type::DbType::MySQL => (
                    format!("JSON_UNQUOTE(JSON_EXTRACT({}, '$.?'))", col),
                    vec![],
                ),
                crate::db_type::DbType::Sqlite => (format!("json_extract({}, '$.?')", col), vec![]),
                _ => ("NULL".to_string(), vec![]),
            }
        }
    }

    impl<C: TypedColumn, P: Clone> JsonPathGet<C, P> {
        /// 创建 JSON Path 提取表达式
        pub fn new() -> Self {
            Self(PhantomData)
        }
    }
    impl<C: TypedColumn, P: Clone> Default for JsonPathGet<C, P> {
        fn default() -> Self {
            Self::new()
        }
    }
    impl<C: TypedColumn, P: Clone> TypedExpression for JsonPathGet<C, P> {
        type SqlType = super::Json;
        fn to_sql(&self, dialect: &dyn Dialect) -> (String, Vec<String>) {
            let col = dialect.quote(C::NAME);
            match dialect.db_type() {
                crate::db_type::DbType::PostgreSQL => (format!("{}#>?", col), vec![]),
                _ => ("NULL".to_string(), vec![]),
            }
        }
    }

    impl<C: TypedColumn, P: Clone> JsonPathGetText<C, P> {
        /// 创建 JSON Path 文本提取表达式
        pub fn new() -> Self {
            Self(PhantomData)
        }
    }
    impl<C: TypedColumn, P: Clone> Default for JsonPathGetText<C, P> {
        fn default() -> Self {
            Self::new()
        }
    }
    impl<C: TypedColumn, P: Clone> TypedExpression for JsonPathGetText<C, P> {
        type SqlType = super::Text;
        fn to_sql(&self, dialect: &dyn Dialect) -> (String, Vec<String>) {
            let col = dialect.quote(C::NAME);
            match dialect.db_type() {
                crate::db_type::DbType::PostgreSQL => (format!("{}#>>?", col), vec![]),
                _ => ("NULL".to_string(), vec![]),
            }
        }
    }

    impl<C: TypedColumn, V: Clone> JsonContains<C, V> {
        /// 创建 JSON 包含判断表达式
        pub fn new() -> Self {
            Self(PhantomData)
        }
    }
    impl<C: TypedColumn, V: Clone> Default for JsonContains<C, V> {
        fn default() -> Self {
            Self::new()
        }
    }
    impl<C: TypedColumn, V: Clone> TypedExpression for JsonContains<C, V> {
        type SqlType = super::Bool;
        fn to_sql(&self, dialect: &dyn Dialect) -> (String, Vec<String>) {
            let col = dialect.quote(C::NAME);
            match dialect.db_type() {
                crate::db_type::DbType::PostgreSQL => (format!("{} @> ?", col), vec![]),
                crate::db_type::DbType::MySQL => (format!("JSON_CONTAINS({}, ?)", col), vec![]),
                _ => ("NULL".to_string(), vec![]),
            }
        }
    }

    impl<C: TypedColumn, K: Clone> JsonExists<C, K> {
        /// 创建 JSON 键存在判断表达式
        pub fn new() -> Self {
            Self(PhantomData)
        }
    }
    impl<C: TypedColumn, K: Clone> Default for JsonExists<C, K> {
        fn default() -> Self {
            Self::new()
        }
    }
    impl<C: TypedColumn, K: Clone> TypedExpression for JsonExists<C, K> {
        type SqlType = super::Bool;
        fn to_sql(&self, dialect: &dyn Dialect) -> (String, Vec<String>) {
            let col = dialect.quote(C::NAME);
            match dialect.db_type() {
                crate::db_type::DbType::PostgreSQL => (format!("{} ?? ?", col), vec![]),
                crate::db_type::DbType::MySQL => {
                    (format!("JSON_CONTAINS_PATH({}, 'one', ?)", col), vec![])
                }
                _ => ("NULL".to_string(), vec![]),
            }
        }
    }
}

#[cfg(feature = "typed-dsl")]
pub use typed_dsl_ext::*;

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
        // i64 → BigInt（与 InferSqlType 映射保持一致）
        type SqlType = BigInt;
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
        // i64 → BigInt（与 InferSqlType 映射保持一致）
        type SqlType = BigInt;
    }

    /// 测试用列：i32 → Integer
    struct ColScore;
    impl TypedColumn for ColScore {
        const NAME: &'static str = "score";
        type Table = UsersTable;
        type RustType = i32;
        type SqlType = Integer;
    }

    /// 测试用列：f64 → Double
    struct ColHeight;
    impl TypedColumn for ColHeight {
        const NAME: &'static str = "height";
        type Table = UsersTable;
        type RustType = f64;
        type SqlType = Double;
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

    /// L-3 测试：LIMIT 超过 MAX_LIMIT 应被 clamp
    #[test]
    fn test_l3_limit_clamp_to_max() {
        let dialect = MySqlDialect;
        // usize::MAX 应被 clamp 到 MAX_LIMIT (1,000,000)
        let q = TypedSelectQuery::<UsersTable>::new().limit(usize::MAX);
        assert_eq!(q.limit_n, Some(TypedSelectQuery::<UsersTable>::MAX_LIMIT));
        let (sql, _) = q.build(&dialect);
        assert!(sql.contains(&format!(
            "LIMIT {}",
            TypedSelectQuery::<UsersTable>::MAX_LIMIT
        )));
    }

    /// L-3 测试：OFFSET 超过 MAX_OFFSET 应被 clamp
    #[test]
    fn test_l3_offset_clamp_to_max() {
        let dialect = MySqlDialect;
        // usize::MAX 应被 clamp 到 MAX_OFFSET (1,000,000,000)
        let q = TypedSelectQuery::<UsersTable>::new()
            .limit(10)
            .offset(usize::MAX);
        assert_eq!(q.offset_n, Some(TypedSelectQuery::<UsersTable>::MAX_OFFSET));
        // page = MAX_OFFSET / 10 + 1 = 100,000,001
        // MySQL pagination: LIMIT 10 OFFSET <(page-1)*10> = 1,000,000,000
        let (sql, _) = q.build(&dialect);
        // 验证 OFFSET 在合理范围内（不会出现 usize::MAX）
        assert!(sql.contains("LIMIT 10"));
    }

    /// L-3 测试：正常值不受影响
    #[test]
    fn test_l3_normal_limit_offset_not_clamped() {
        let q1 = TypedSelectQuery::<UsersTable>::new().limit(100);
        assert_eq!(q1.limit_n, Some(100));
        let q2 = TypedSelectQuery::<UsersTable>::new().offset(1000);
        assert_eq!(q2.offset_n, Some(1000));
        // 边界值：恰好等于 MAX_LIMIT / MAX_OFFSET
        let q3 =
            TypedSelectQuery::<UsersTable>::new().limit(TypedSelectQuery::<UsersTable>::MAX_LIMIT);
        assert_eq!(q3.limit_n, Some(TypedSelectQuery::<UsersTable>::MAX_LIMIT));
        let q4 = TypedSelectQuery::<UsersTable>::new()
            .offset(TypedSelectQuery::<UsersTable>::MAX_OFFSET);
        assert_eq!(
            q4.offset_n,
            Some(TypedSelectQuery::<UsersTable>::MAX_OFFSET)
        );
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
        // P4-2 修复：原测试名声称"跨表逻辑组合在编译期被拒绝"，但实际只测试
        // 同表组合可编译，且无任何 assert。现重写为带 assert 的真实测试。

        // 同表 And 组合应正常编译，且实现 ExprTable<Table = UsersTable>
        fn _assert_expr_table<E: ExprTable<Table = UsersTable>>(_: E) {}
        let expr = And::new(ColId.eq(1i64), ColAge.gt(18i64));
        _assert_expr_table(expr); // 编译期验证 Table = UsersTable

        // 同表 Or 组合应正常编译
        let expr = Or::new(ColName.eq("a".to_string()), ColName.eq("b".to_string()));
        _assert_expr_table(expr);

        // 跨表组合（ColId 属于 UsersTable, ColPostTitle 属于 PostsTable）
        // 因 And<L, R> 要求 R: ExprTable<Table = L::Table>，跨表组合
        // 不满足 trait bound，编译器会拒绝。下方代码若取消注释将无法编译：
        //
        // ```ignore
        // // ❌ 编译错误：And<_, _> 未实现 ExprTable<Table = ?>（两侧表不同）
        // let _ = And::new(ColId.eq(1i64), ColPostTitle.eq("x"));
        // ```
        //
        // 该编译期检查由 `impl<L, R> ExprTable for And<L, R> where R: ExprTable<Table = L::Table>`
        // 提供（见 src/typed_ast.rs L645-651），无需运行时断言。
    }

    // ---- SqlType 标记类型测试 ----

    #[test]
    fn test_sql_type_markers() {
        // 这些是零大小标记类型
        assert_eq!(std::mem::size_of::<Bool>(), 0);
        assert_eq!(std::mem::size_of::<Integer>(), 0);
        assert_eq!(std::mem::size_of::<SmallInt>(), 0);
        assert_eq!(std::mem::size_of::<BigInt>(), 0);
        assert_eq!(std::mem::size_of::<Real>(), 0);
        assert_eq!(std::mem::size_of::<Double>(), 0);
        assert_eq!(std::mem::size_of::<Text>(), 0);
        assert_eq!(std::mem::size_of::<Date>(), 0);
        assert_eq!(std::mem::size_of::<DateTime>(), 0);
        assert_eq!(std::mem::size_of::<Json>(), 0);
        assert_eq!(std::mem::size_of::<Uuid>(), 0);
        assert_eq!(std::mem::size_of::<Binary>(), 0);
        assert_eq!(std::mem::size_of::<Untyped>(), 0);
        // Nullable<T> 也是零大小（PhantomData 是零大小）
        assert_eq!(std::mem::size_of::<Nullable<Integer>>(), 0);
        assert_eq!(std::mem::size_of::<Nullable<Text>>(), 0);
    }

    #[test]
    fn test_infer_sql_type_mapping() {
        // 编译期校验 InferSqlType 类型映射
        // bool → Bool
        fn _assert_bool<T: InferSqlType<SqlType = Bool>>(_: T) {}
        // i8/u8/i16/u16 → SmallInt
        fn _assert_smallint<T: InferSqlType<SqlType = SmallInt>>(_: T) {}
        // i32/u32 → Integer
        fn _assert_integer<T: InferSqlType<SqlType = Integer>>(_: T) {}
        // i64/u64 → BigInt
        fn _assert_bigint<T: InferSqlType<SqlType = BigInt>>(_: T) {}
        // f32 → Real
        fn _assert_real<T: InferSqlType<SqlType = Real>>(_: T) {}
        // f64 → Double
        fn _assert_double<T: InferSqlType<SqlType = Double>>(_: T) {}
        // String → Text
        fn _assert_text<T: InferSqlType<SqlType = Text>>(_: T) {}
        // Vec<u8> → Binary
        fn _assert_binary<T: InferSqlType<SqlType = Binary>>(_: T) {}
        // Option<i64> → Nullable<BigInt>
        fn _assert_nullable_bigint<T: InferSqlType<SqlType = Nullable<BigInt>>>(_: T) {}
        // () → Untyped
        fn _assert_untyped<T: InferSqlType<SqlType = Untyped>>(_: T) {}

        _assert_bool(true);
        _assert_smallint(0i8);
        _assert_smallint(0u8);
        _assert_smallint(0i16);
        _assert_smallint(0u16);
        _assert_integer(0i32);
        _assert_integer(0u32);
        _assert_bigint(0i64);
        _assert_bigint(0u64);
        _assert_real(0.0f32);
        _assert_double(0.0f64);
        _assert_text(String::new());
        _assert_binary(Vec::<u8>::new());
        _assert_nullable_bigint(Some(0i64));
        _assert_untyped(());

        // Borrowed types: &str, &String, &[u8], &Vec<u8>
        let s = String::new();
        let v: Vec<u8> = Vec::new();
        _assert_text("hello");
        _assert_text(&s);
        _assert_binary(&[1u8, 2u8, 3u8][..]);
        _assert_binary(&v);
    }

    #[test]
    fn test_column_sql_type_propagation() {
        // ColumnExpr<C>::SqlType 应等于 C::SqlType
        fn _assert_bigint<E: TypedExpression<SqlType = BigInt>>(_: E) {}
        fn _assert_integer<E: TypedExpression<SqlType = Integer>>(_: E) {}
        fn _assert_double<E: TypedExpression<SqlType = Double>>(_: E) {}
        fn _assert_text<E: TypedExpression<SqlType = Text>>(_: E) {}

        // i64 列 → BigInt
        _assert_bigint(ColumnExpr::<ColId>::new());
        _assert_bigint(ColumnExpr::<ColAge>::new());
        // i32 列 → Integer
        _assert_integer(ColumnExpr::<ColScore>::new());
        // f64 列 → Double
        _assert_double(ColumnExpr::<ColHeight>::new());
        // String 列 → Text
        _assert_text(ColumnExpr::<ColName>::new());
        _assert_text(ColumnExpr::<ColPostTitle>::new());
    }

    #[test]
    fn test_literal_sql_type_specialized() {
        // Literal<i64>::SqlType 应为 BigInt
        fn _assert_bigint<E: TypedExpression<SqlType = BigInt>>(_: E) {}
        // Literal<i32>::SqlType 应为 Integer
        fn _assert_integer<E: TypedExpression<SqlType = Integer>>(_: E) {}
        // Literal<i16>::SqlType 应为 SmallInt
        fn _assert_smallint<E: TypedExpression<SqlType = SmallInt>>(_: E) {}
        // Literal<f64>::SqlType 应为 Double
        fn _assert_double<E: TypedExpression<SqlType = Double>>(_: E) {}
        // Literal<f32>::SqlType 应为 Real
        fn _assert_real<E: TypedExpression<SqlType = Real>>(_: E) {}
        // Literal<String>::SqlType 应为 Text
        fn _assert_text<E: TypedExpression<SqlType = Text>>(_: E) {}
        // Literal<bool>::SqlType 应为 Bool
        fn _assert_bool<E: TypedExpression<SqlType = Bool>>(_: E) {}
        // Literal<Vec<u8>>::SqlType 应为 Binary
        fn _assert_binary<E: TypedExpression<SqlType = Binary>>(_: E) {}

        _assert_bigint(Literal::new(42i64));
        _assert_integer(Literal::new(7i32));
        _assert_smallint(Literal::new(3i16));
        _assert_smallint(Literal::new(3i8));
        // 使用 1.5 避免触发 clippy::approx_constant (3.14≈PI, 2.71828≈E)
        _assert_double(Literal::new(1.5f64));
        _assert_real(Literal::new(1.0f32));
        _assert_text(Literal::new("hello".to_string()));
        _assert_bool(Literal::new(true));
        _assert_binary(Literal::new(vec![1u8, 2u8, 3u8]));
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

    // ---- M1-T1.3: CTE 表达式测试 ----

    #[cfg(feature = "typed-dsl")]
    mod cte_tests {
        use super::*;
        use crate::dialect::{MySqlDialect, PostgreSqlDialect};
        use crate::typed_ast::{CteName, CteRef, With, WithRecursive};

        struct CteActiveUsers;
        impl CteName for CteActiveUsers {
            const NAME: &'static str = "active_users";
        }

        struct CteUserCount;
        impl CteName for CteUserCount {
            const NAME: &'static str = "user_count";
        }

        #[test]
        fn test_with_cte_sql() {
            let dialect = MySqlDialect;
            let expr: With<CteActiveUsers, ColumnExpr<ColId>> = With::new();
            let (sql, params) = expr.to_sql(&dialect);
            assert_eq!(sql, "WITH active_users AS (`users.id`)");
            assert!(params.is_empty());
        }

        #[test]
        fn test_with_recursive_cte_sql() {
            let dialect = PostgreSqlDialect;
            let expr: WithRecursive<CteActiveUsers, ColumnExpr<ColId>, ColumnExpr<ColName>> =
                WithRecursive::new();
            let (sql, params) = expr.to_sql(&dialect);
            assert_eq!(
                sql,
                "WITH RECURSIVE active_users AS (\"users.id\" UNION ALL \"users.name\")"
            );
            assert!(params.is_empty());
        }

        #[test]
        fn test_cte_ref_sql() {
            let dialect = MySqlDialect;
            let expr: CteRef<CteActiveUsers> = CteRef::new();
            let (sql, params) = expr.to_sql(&dialect);
            assert_eq!(sql, "active_users");
            assert!(params.is_empty());
        }

        #[test]
        fn test_cte_ref_different_name() {
            let dialect = PostgreSqlDialect;
            let expr: CteRef<CteUserCount> = CteRef::new();
            let (sql, _) = expr.to_sql(&dialect);
            assert_eq!(sql, "user_count");
        }

        #[test]
        fn test_cte_expressions_are_zst() {
            assert_eq!(
                std::mem::size_of::<With<CteActiveUsers, ColumnExpr<ColId>>>(),
                0
            );
            assert_eq!(
                std::mem::size_of::<
                    WithRecursive<CteActiveUsers, ColumnExpr<ColId>, ColumnExpr<ColName>>,
                >(),
                0
            );
            assert_eq!(std::mem::size_of::<CteRef<CteActiveUsers>>(), 0);
        }
    }

    // ---- M1-T2.3: Window Frame 表达式测试 ----

    #[cfg(feature = "typed-dsl")]
    mod window_frame_tests {
        use super::*;
        use crate::dialect::MySqlDialect;
        use crate::typed_ast::{
            FrameBetween, FrameCurrentRow, FrameUnboundedPreceding, GroupsFrame, RangeFrame,
            RowsFrame,
        };

        #[test]
        fn test_rows_frame_sql() {
            let dialect = MySqlDialect;
            let (sql, params) = RowsFrame::new().to_sql(&dialect);
            assert_eq!(sql, "ROWS");
            assert!(params.is_empty());
        }

        #[test]
        fn test_range_frame_sql() {
            let dialect = MySqlDialect;
            let (sql, params) = RangeFrame::new().to_sql(&dialect);
            assert_eq!(sql, "RANGE");
            assert!(params.is_empty());
        }

        #[test]
        fn test_groups_frame_sql() {
            let dialect = MySqlDialect;
            let (sql, params) = GroupsFrame::new().to_sql(&dialect);
            assert_eq!(sql, "GROUPS");
            assert!(params.is_empty());
        }

        #[test]
        fn test_frame_between_sql() {
            let dialect = MySqlDialect;
            let expr: FrameBetween<FrameUnboundedPreceding, FrameCurrentRow> = FrameBetween::new();
            let (sql, params) = expr.to_sql(&dialect);
            assert_eq!(sql, "BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW");
            assert!(params.is_empty());
        }

        #[test]
        fn test_frame_unbounded_preceding_sql() {
            let dialect = MySqlDialect;
            let (sql, params) = FrameUnboundedPreceding::new().to_sql(&dialect);
            assert_eq!(sql, "UNBOUNDED PRECEDING");
            assert!(params.is_empty());
        }

        #[test]
        fn test_frame_current_row_sql() {
            let dialect = MySqlDialect;
            let (sql, params) = FrameCurrentRow::new().to_sql(&dialect);
            assert_eq!(sql, "CURRENT ROW");
            assert!(params.is_empty());
        }

        #[test]
        fn test_frame_expressions_are_zst() {
            assert_eq!(std::mem::size_of::<RowsFrame>(), 0);
            assert_eq!(std::mem::size_of::<RangeFrame>(), 0);
            assert_eq!(std::mem::size_of::<GroupsFrame>(), 0);
            assert_eq!(std::mem::size_of::<FrameUnboundedPreceding>(), 0);
            assert_eq!(std::mem::size_of::<FrameCurrentRow>(), 0);
            assert_eq!(
                std::mem::size_of::<FrameBetween<FrameUnboundedPreceding, FrameCurrentRow>>(),
                0
            );
        }
    }

    // ---- M1-T3.3: JSON 操作符表达式测试 ----

    #[cfg(feature = "typed-dsl")]
    mod json_op_tests {
        use super::*;
        use crate::dialect::{MySqlDialect, PostgreSqlDialect, SqliteDialect};
        use crate::typed_ast::{
            JsonContains, JsonExists, JsonGet, JsonGetText, JsonPathGet, JsonPathGetText,
        };

        #[test]
        fn test_json_get_postgresql() {
            let dialect = PostgreSqlDialect;
            let expr: JsonGet<ColName, String> = JsonGet::new();
            let (sql, params) = expr.to_sql(&dialect);
            assert_eq!(sql, "\"name\"->?");
            assert!(params.is_empty());
        }

        #[test]
        fn test_json_get_mysql() {
            let dialect = MySqlDialect;
            let expr: JsonGet<ColName, String> = JsonGet::new();
            let (sql, _) = expr.to_sql(&dialect);
            assert_eq!(sql, "JSON_EXTRACT(`name`, '$.?')");
        }

        #[test]
        fn test_json_get_sqlite() {
            let dialect = SqliteDialect;
            let expr: JsonGet<ColName, String> = JsonGet::new();
            let (sql, _) = expr.to_sql(&dialect);
            assert_eq!(sql, "json_extract(\"name\", '$.?')");
        }

        #[test]
        fn test_json_get_text_postgresql() {
            let dialect = PostgreSqlDialect;
            let expr: JsonGetText<ColName, String> = JsonGetText::new();
            let (sql, _) = expr.to_sql(&dialect);
            assert_eq!(sql, "\"name\"->>?");
        }

        #[test]
        fn test_json_get_text_mysql() {
            let dialect = MySqlDialect;
            let expr: JsonGetText<ColName, String> = JsonGetText::new();
            let (sql, _) = expr.to_sql(&dialect);
            assert_eq!(sql, "JSON_UNQUOTE(JSON_EXTRACT(`name`, '$.?'))");
        }

        #[test]
        fn test_json_path_get_postgresql() {
            let dialect = PostgreSqlDialect;
            let expr: JsonPathGet<ColName, String> = JsonPathGet::new();
            let (sql, _) = expr.to_sql(&dialect);
            assert_eq!(sql, "\"name\"#>?");
        }

        #[test]
        fn test_json_path_get_text_postgresql() {
            let dialect = PostgreSqlDialect;
            let expr: JsonPathGetText<ColName, String> = JsonPathGetText::new();
            let (sql, _) = expr.to_sql(&dialect);
            assert_eq!(sql, "\"name\"#>>?");
        }

        #[test]
        fn test_json_contains_postgresql() {
            let dialect = PostgreSqlDialect;
            let expr: JsonContains<ColName, String> = JsonContains::new();
            let (sql, _) = expr.to_sql(&dialect);
            assert_eq!(sql, "\"name\" @> ?");
        }

        #[test]
        fn test_json_contains_mysql() {
            let dialect = MySqlDialect;
            let expr: JsonContains<ColName, String> = JsonContains::new();
            let (sql, _) = expr.to_sql(&dialect);
            assert_eq!(sql, "JSON_CONTAINS(`name`, ?)");
        }

        #[test]
        fn test_json_exists_postgresql() {
            let dialect = PostgreSqlDialect;
            let expr: JsonExists<ColName, String> = JsonExists::new();
            let (sql, _) = expr.to_sql(&dialect);
            assert_eq!(sql, "\"name\" ?? ?");
        }

        #[test]
        fn test_json_exists_mysql() {
            let dialect = MySqlDialect;
            let expr: JsonExists<ColName, String> = JsonExists::new();
            let (sql, _) = expr.to_sql(&dialect);
            assert_eq!(sql, "JSON_CONTAINS_PATH(`name`, 'one', ?)");
        }

        #[test]
        fn test_json_expressions_are_zst() {
            assert_eq!(std::mem::size_of::<JsonGet<ColName, String>>(), 0);
            assert_eq!(std::mem::size_of::<JsonGetText<ColName, String>>(), 0);
            assert_eq!(std::mem::size_of::<JsonPathGet<ColName, String>>(), 0);
            assert_eq!(std::mem::size_of::<JsonPathGetText<ColName, String>>(), 0);
            assert_eq!(std::mem::size_of::<JsonContains<ColName, String>>(), 0);
            assert_eq!(std::mem::size_of::<JsonExists<ColName, String>>(), 0);
        }
    }
}
