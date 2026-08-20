//! 类型安全的 JOIN 选择器（SeaORM 风格）
//!
//! 通过类型参数跟踪已 JOIN 的表，在编译期区分单表查询、双表查询、三表查询。
//!
//! # 设计
//!
//! - [`SelectOne<M>`] — 单表查询（起点），M 为主表模型
//! - [`SelectTwo<M, N>`] — 一次 JOIN 后的查询，N 为第一个被 JOIN 的表
//! - [`SelectThree<M, N, O>`] — 两次 JOIN 后的查询，N、O 为被 JOIN 的表
//!
//! 每次调用 `join_inner` / `join_left` 都会将类型提升到下一个 arity，
//! 从而在类型层面记录"已经 JOIN 了几张表"。
//!
//! # 用法
//!
//! ```ignore
//! use sz_orm_core::select_types::SelectOne;
//! use sz_orm_core::dialect::{get_dialect, DbType};
//!
//! // 从主表 Users 开始
//! let select_one = SelectOne::<Users>::new(get_dialect(DbType::MySQL)?);
//!
//! // 一次 JOIN → SelectTwo<Users, Orders>
//! let select_two = select_one
//!     .join_inner::<OrdersTable, UsersColId, OrdersColUserId>();
//!
//! // 两次 JOIN → SelectThree<Users, Orders, Profiles>
//! let select_three = select_two
//!     .join_inner::<ProfilesTable, OrdersColUserId, ProfilesColUserId>();
//!
//! let (sql, params) = select_three.build_with_params();
//! ```

use crate::dialect::{get_dialect, DbType, Dialect};
use crate::error::DbError;
use crate::model::Model;
use crate::query::QueryBuilder;
use crate::typed::{TypedColumn, TypedTable};
use crate::value::Value;
use std::marker::PhantomData;

/// 单表查询选择器（JOIN 链的起点）
///
/// 类型参数 `M` 是主表模型（实现 `Model` trait）。
pub struct SelectOne<M: Model> {
    inner: QueryBuilder<M>,
}

impl<M: Model> SelectOne<M> {
    /// 创建新的单表选择器，使用指定方言
    pub fn new(dialect: Box<dyn Dialect>) -> Self {
        Self {
            inner: QueryBuilder::new(dialect).table(M::table_name()),
        }
    }

    /// 使用默认方言（MySQL）创建选择器（便捷方法）
    pub fn mysql() -> Result<Self, DbError> {
        Ok(Self::new(get_dialect(DbType::MySQL)?))
    }

    /// 添加 INNER JOIN，类型提升为 [`SelectTwo<M, N>`]
    ///
    /// # 类型参数
    ///
    /// - `N`: 被 JOIN 的右表（实现 `TypedTable`）
    /// - `LCol`: ON 条件左侧列（应属于主表 M，编译期不强制，由文档约束）
    /// - `RCol`: ON 条件右侧列（必须属于表 N，编译期通过 `TypedColumn::Table = N` 约束）
    pub fn join_inner<N, LCol, RCol>(self) -> SelectTwo<M, N>
    where
        N: TypedTable,
        LCol: TypedColumn,
        RCol: TypedColumn<Table = N>,
    {
        let inner = self.inner.join_inner(N::NAME, LCol::NAME, RCol::NAME);
        SelectTwo {
            inner,
            _joined: PhantomData,
        }
    }

    /// 添加 LEFT JOIN，类型提升为 [`SelectTwo<M, N>`]
    pub fn join_left<N, LCol, RCol>(self) -> SelectTwo<M, N>
    where
        N: TypedTable,
        LCol: TypedColumn,
        RCol: TypedColumn<Table = N>,
    {
        let inner = self.inner.join_left(N::NAME, LCol::NAME, RCol::NAME);
        SelectTwo {
            inner,
            _joined: PhantomData,
        }
    }

    /// 生成 SELECT SQL（纯 SQL，参数内联渲染）
    pub fn build_select(&self) -> String {
        self.inner.sql()
    }

    /// 生成 SELECT SQL 及参数绑定列表
    pub fn build_with_params(&self) -> (String, Vec<Value>) {
        self.inner.build_select_with_params()
    }

    /// 透传：添加 WHERE = 条件
    pub fn where_eq(mut self, field: impl Into<String>, value: Value) -> Self {
        self.inner = self.inner.where_eq(field, value);
        self
    }

    /// 透传：添加 ORDER BY
    pub fn order_by(mut self, field: impl Into<String>) -> Self {
        self.inner = self.inner.order_by(field);
        self
    }

    /// 透传：添加 LIMIT
    pub fn limit(mut self, limit: usize) -> Self {
        self.inner = self.inner.limit(limit);
        self
    }

    /// 透传：添加 OFFSET
    pub fn offset(mut self, offset: usize) -> Self {
        self.inner = self.inner.offset(offset);
        self
    }

    /// 透传：设置 SELECT 列（列名经校验 + quote，审计 M-6）
    pub fn select(mut self, columns: Vec<&str>) -> Result<Self, crate::DbError> {
        self.inner = self.inner.select(columns)?;
        Ok(self)
    }

    /// 获取内部 QueryBuilder 的不可变引用（用于高级用法）
    pub fn as_query_builder(&self) -> &QueryBuilder<M> {
        &self.inner
    }

    /// 消耗 self，返回内部 QueryBuilder（用于高级用法）
    pub fn into_query_builder(self) -> QueryBuilder<M> {
        self.inner
    }
}

/// 一次 JOIN 后的查询选择器
///
/// 类型参数：
/// - `M`: 主表模型
/// - `N`: 第一个被 JOIN 的表（TypedTable）
pub struct SelectTwo<M: Model, N: TypedTable> {
    inner: QueryBuilder<M>,
    _joined: PhantomData<N>,
}

impl<M: Model, N: TypedTable> SelectTwo<M, N> {
    /// 再添加一个 INNER JOIN，类型提升为 [`SelectThree<M, N, O>`]
    ///
    /// # 类型参数
    ///
    /// - `O`: 第二个被 JOIN 的表（实现 `TypedTable`）
    /// - `LCol`: ON 条件左侧列
    /// - `RCol`: ON 条件右侧列（必须属于表 O）
    pub fn join_inner<O, LCol, RCol>(self) -> SelectThree<M, N, O>
    where
        O: TypedTable,
        LCol: TypedColumn,
        RCol: TypedColumn<Table = O>,
    {
        let inner = self.inner.join_inner(O::NAME, LCol::NAME, RCol::NAME);
        SelectThree {
            inner,
            _joined: PhantomData,
        }
    }

    /// 再添加一个 LEFT JOIN，类型提升为 [`SelectThree<M, N, O>`]
    pub fn join_left<O, LCol, RCol>(self) -> SelectThree<M, N, O>
    where
        O: TypedTable,
        LCol: TypedColumn,
        RCol: TypedColumn<Table = O>,
    {
        let inner = self.inner.join_left(O::NAME, LCol::NAME, RCol::NAME);
        SelectThree {
            inner,
            _joined: PhantomData,
        }
    }

    /// 生成 SELECT SQL 及参数绑定列表
    pub fn build_with_params(&self) -> (String, Vec<Value>) {
        self.inner.build_select_with_params()
    }

    /// 生成 SELECT SQL（纯 SQL，参数内联渲染）
    pub fn build_select(&self) -> String {
        self.inner.sql()
    }

    /// 透传：添加 WHERE = 条件
    pub fn where_eq(mut self, field: impl Into<String>, value: Value) -> Self {
        self.inner = self.inner.where_eq(field, value);
        self
    }

    /// 透传：添加 ORDER BY
    pub fn order_by(mut self, field: impl Into<String>) -> Self {
        self.inner = self.inner.order_by(field);
        self
    }

    /// 透传：添加 LIMIT
    pub fn limit(mut self, limit: usize) -> Self {
        self.inner = self.inner.limit(limit);
        self
    }

    /// 透传：添加 OFFSET
    pub fn offset(mut self, offset: usize) -> Self {
        self.inner = self.inner.offset(offset);
        self
    }

    /// 获取内部 QueryBuilder 的不可变引用
    pub fn as_query_builder(&self) -> &QueryBuilder<M> {
        &self.inner
    }

    /// 消耗 self，返回内部 QueryBuilder
    pub fn into_query_builder(self) -> QueryBuilder<M> {
        self.inner
    }
}

/// 两次 JOIN 后的查询选择器
///
/// 类型参数：
/// - `M`: 主表模型
/// - `N`: 第一个被 JOIN 的表
/// - `O`: 第二个被 JOIN 的表
pub struct SelectThree<M: Model, N: TypedTable, O: TypedTable> {
    inner: QueryBuilder<M>,
    _joined: PhantomData<(N, O)>,
}

impl<M: Model, N: TypedTable, O: TypedTable> SelectThree<M, N, O> {
    /// 生成 SELECT SQL 及参数绑定列表
    pub fn build_with_params(&self) -> (String, Vec<Value>) {
        self.inner.build_select_with_params()
    }

    /// 生成 SELECT SQL（纯 SQL，参数内联渲染）
    pub fn build_select(&self) -> String {
        self.inner.sql()
    }

    /// 透传：添加 WHERE = 条件
    pub fn where_eq(mut self, field: impl Into<String>, value: Value) -> Self {
        self.inner = self.inner.where_eq(field, value);
        self
    }

    /// 透传：添加 ORDER BY
    pub fn order_by(mut self, field: impl Into<String>) -> Self {
        self.inner = self.inner.order_by(field);
        self
    }

    /// 透传：添加 LIMIT
    pub fn limit(mut self, limit: usize) -> Self {
        self.inner = self.inner.limit(limit);
        self
    }

    /// 透传：添加 OFFSET
    pub fn offset(mut self, offset: usize) -> Self {
        self.inner = self.inner.offset(offset);
        self
    }

    /// 获取内部 QueryBuilder 的不可变引用
    pub fn as_query_builder(&self) -> &QueryBuilder<M> {
        &self.inner
    }

    /// 消耗 self，返回内部 QueryBuilder
    pub fn into_query_builder(self) -> QueryBuilder<M> {
        self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::typed::{TypedColumn, TypedTable};

    // ---- 测试用 mock 类型 ----

    struct UsersModel;
    impl Model for UsersModel {
        type PrimaryKey = i64;
        fn table_name() -> &'static str {
            "users"
        }
        fn pk(&self) -> Self::PrimaryKey {
            0
        }
        fn set_pk(&mut self, _pk: Self::PrimaryKey) {}
    }

    struct OrdersTable;
    impl TypedTable for OrdersTable {
        const NAME: &'static str = "orders";
    }

    struct ProfilesTable;
    impl TypedTable for ProfilesTable {
        const NAME: &'static str = "profiles";
    }

    struct UsersColId;
    impl TypedColumn for UsersColId {
        const NAME: &'static str = "id";
        type Table = OrdersTable; // 复用：这里 Table 仅用于类型标记
        type RustType = i64;
        type SqlType = crate::typed_ast::Untyped;
    }

    struct OrdersColUserId;
    impl TypedColumn for OrdersColUserId {
        const NAME: &'static str = "user_id";
        type Table = OrdersTable;
        type RustType = i64;
        type SqlType = crate::typed_ast::Untyped;
    }

    struct OrdersColId;
    impl TypedColumn for OrdersColId {
        const NAME: &'static str = "id";
        type Table = OrdersTable;
        type RustType = i64;
        type SqlType = crate::typed_ast::Untyped;
    }

    struct ProfilesColUserId;
    impl TypedColumn for ProfilesColUserId {
        const NAME: &'static str = "user_id";
        type Table = ProfilesTable;
        type RustType = i64;
        type SqlType = crate::typed_ast::Untyped;
    }

    // ---- SelectOne 基础测试 ----

    #[test]
    fn test_select_one_build() {
        let s1 = SelectOne::<UsersModel>::mysql().unwrap();
        let (sql, params) = s1.build_with_params();
        assert!(sql.contains("SELECT"));
        assert!(sql.contains("users"));
        assert!(params.is_empty());
    }

    #[test]
    fn test_select_one_with_where_and_order() {
        let s1 = SelectOne::<UsersModel>::mysql()
            .unwrap()
            .where_eq("name", Value::from("Alice"))
            .order_by("id")
            .limit(10)
            .offset(0);
        let (sql, _params) = s1.build_with_params();
        assert!(sql.contains("WHERE"));
        assert!(sql.contains("ORDER BY"));
        assert!(sql.contains("LIMIT"));
    }

    // ---- SelectOne → SelectTwo（一次 JOIN）测试 ----

    #[test]
    fn test_select_two_after_inner_join() {
        // SelectOne::join_inner 返回 SelectTwo
        let s2: SelectTwo<UsersModel, OrdersTable> = SelectOne::<UsersModel>::mysql()
            .unwrap()
            .join_inner::<OrdersTable, UsersColId, OrdersColUserId>(
        );

        let (sql, _params) = s2.build_with_params();
        assert!(sql.contains("INNER JOIN"));
        assert!(sql.contains("orders"));
        assert!(sql.contains("ON"));
    }

    #[test]
    fn test_select_two_after_left_join() {
        let s2: SelectTwo<UsersModel, OrdersTable> = SelectOne::<UsersModel>::mysql()
            .unwrap()
            .join_left::<OrdersTable, UsersColId, OrdersColUserId>(
        );

        let (sql, _params) = s2.build_with_params();
        assert!(sql.contains("LEFT JOIN"));
        assert!(sql.contains("orders"));
    }

    #[test]
    fn test_select_two_type_params_track_joined_table() {
        // 编译期验证：SelectTwo 的类型参数 N 是 OrdersTable
        // 如果 N 不是 TypedTable，编译会失败
        fn _assert_two<M: Model, N: TypedTable>(_s: &SelectTwo<M, N>) {}

        let s2 = SelectOne::<UsersModel>::mysql()
            .unwrap()
            .join_inner::<OrdersTable, UsersColId, OrdersColUserId>();
        _assert_two(&s2); // 编译通过说明类型约束正确
    }

    // ---- SelectTwo → SelectThree（两次 JOIN）测试 ----

    #[test]
    fn test_select_three_after_second_inner_join() {
        let s3: SelectThree<UsersModel, OrdersTable, ProfilesTable> =
            SelectOne::<UsersModel>::mysql()
                .unwrap()
                .join_inner::<OrdersTable, UsersColId, OrdersColUserId>()
                .join_inner::<ProfilesTable, OrdersColId, ProfilesColUserId>();

        let (sql, _params) = s3.build_with_params();
        // 应包含两个 JOIN 子句
        let join_count = sql.matches("JOIN").count();
        assert_eq!(join_count, 2, "expected 2 JOINs, got: {sql}");
    }

    #[test]
    fn test_select_three_after_mixed_joins() {
        let s3: SelectThree<UsersModel, OrdersTable, ProfilesTable> =
            SelectOne::<UsersModel>::mysql()
                .unwrap()
                .join_inner::<OrdersTable, UsersColId, OrdersColUserId>()
                .join_left::<ProfilesTable, OrdersColId, ProfilesColUserId>();

        let (sql, _params) = s3.build_with_params();
        assert!(sql.contains("INNER JOIN"));
        assert!(sql.contains("LEFT JOIN"));
    }

    #[test]
    fn test_select_three_type_params_track_two_joined_tables() {
        fn _assert_three<M: Model, N: TypedTable, O: TypedTable>(_s: &SelectThree<M, N, O>) {}

        let s3 = SelectOne::<UsersModel>::mysql()
            .unwrap()
            .join_inner::<OrdersTable, UsersColId, OrdersColUserId>()
            .join_inner::<ProfilesTable, OrdersColId, ProfilesColUserId>();
        _assert_three(&s3); // 编译通过说明 N 和 O 都是 TypedTable
    }

    // ---- 链式调用透传测试 ----

    #[test]
    fn test_select_two_chain_where_order_limit() {
        let s2 = SelectOne::<UsersModel>::mysql()
            .unwrap()
            .join_inner::<OrdersTable, UsersColId, OrdersColUserId>()
            .where_eq("status", Value::from(1))
            .order_by("created_at")
            .limit(20)
            .offset(10);

        let (sql, _params) = s2.build_with_params();
        assert!(sql.contains("INNER JOIN"));
        assert!(sql.contains("WHERE"));
        assert!(sql.contains("ORDER BY"));
        assert!(sql.contains("LIMIT"));
    }

    #[test]
    fn test_select_three_chain_where_order_limit() {
        let s3 = SelectOne::<UsersModel>::mysql()
            .unwrap()
            .join_inner::<OrdersTable, UsersColId, OrdersColUserId>()
            .join_left::<ProfilesTable, OrdersColId, ProfilesColUserId>()
            .where_eq("active", Value::from(true))
            .order_by("name")
            .limit(5);

        let (sql, _params) = s3.build_with_params();
        assert_eq!(sql.matches("JOIN").count(), 2);
        assert!(sql.contains("WHERE"));
        assert!(sql.contains("ORDER BY"));
        assert!(sql.contains("LIMIT"));
    }

    // ---- into_query_builder 透传测试 ----

    #[test]
    fn test_select_two_into_query_builder() {
        let s2 = SelectOne::<UsersModel>::mysql()
            .unwrap()
            .join_inner::<OrdersTable, UsersColId, OrdersColUserId>();
        let qb = s2.into_query_builder();
        let (sql, _params) = qb.build_select_with_params();
        assert!(sql.contains("INNER JOIN"));
    }

    // ---- 类型安全：RCol 必须属于右表 ----

    #[test]
    fn test_rcol_must_belong_to_right_table() {
        // 编译期约束：RCol::Table = N
        // 如果传入 RCol 的 Table 不是 OrdersTable，编译会失败
        // 此处通过编译即证明约束生效
        let _s2: SelectTwo<UsersModel, OrdersTable> = SelectOne::<UsersModel>::mysql()
            .unwrap()
            .join_inner::<OrdersTable, UsersColId, OrdersColUserId>(
        );
        // OrdersColUserId::Table = OrdersTable ✓ 编译通过
    }
}
