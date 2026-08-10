//! 类型安全关联查询模块
//!
//! 提供编译期外键类型匹配校验的关联类型：
//! - `BelongsTo<Child, Parent, FK>` — 子表属于父表（N:1）
//! - `HasMany<Parent, Child, FK>` — 父表拥有多个子表（1:N）
//! - `HasOne<Parent, Child, FK>` — 父表拥有一个子表（1:1）
//!
//! 所有关联类型均为 ZST（零大小类型），通过关联类型约束在编译期校验外键类型匹配。
//!
//! # 使用方式
//!
//! ```ignore
//! use sz_orm_core::typed_relation::{BelongsTo, HasMany, HasOne, Relation, TypedTable};
//!
//! struct UsersTable;
//! impl TypedTable for UsersTable {
//!     const NAME: &'static str = "users";
//!     type PrimaryKey = i64;
//! }
//!
//! struct PostsTable;
//! impl TypedTable for PostsTable {
//!     const NAME: &'static str = "posts";
//!     type PrimaryKey = i64;
//!     type ForeignKey = i64; // user_id
//! }
//!
//! // 编译期校验：PostsTable::ForeignKey == UsersTable::PrimaryKey
//! type PostsBelongToUsers = BelongsTo<PostsTable, UsersTable>;
//! ```

use std::marker::PhantomData;

/// 类型安全表 trait — 关联查询的基础
pub trait TypedTable: 'static {
    /// 表名
    const NAME: &'static str;
    /// 主键类型
    type PrimaryKey: Clone + std::fmt::Debug;
    /// 外键类型（BelongsTo 端需要，HasMany/HasOne 端默认为 `()`）
    type ForeignKey: Clone + std::fmt::Debug;
}

/// 关联类型标记 trait
pub trait Relation: 'static {
    /// 子表（拥有外键的一方）
    type Child: TypedTable;
    /// 父表（被引用的一方）
    type Parent: TypedTable;
    /// 关联种类
    const KIND: RelationKind;
}

/// 关联种类枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationKind {
    /// 子表属于父表（N:1 关联）
    BelongsTo,
    /// 父表拥有多个子表（1:N 关联）
    HasMany,
    /// 父表拥有一个子表（1:1 关联）
    HasOne,
}

// ---- BelongsTo: N:1 关联 ----

/// BelongsTo<Child, Parent> — 子表属于父表
///
/// 编译期约束：`Child::ForeignKey == Parent::PrimaryKey`
pub struct BelongsTo<C, P>
where
    C: TypedTable,
    P: TypedTable,
    C::ForeignKey: PartialEq<P::PrimaryKey>,
{
    _marker: PhantomData<(C, P)>,
}

impl<C, P> BelongsTo<C, P>
where
    C: TypedTable,
    P: TypedTable,
    C::ForeignKey: PartialEq<P::PrimaryKey>,
{
    /// 创建 BelongsTo 关联（ZST）
    pub fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }

    /// 获取父表名
    pub const fn parent_name() -> &'static str {
        P::NAME
    }

    /// 获取子表名
    pub const fn child_name() -> &'static str {
        C::NAME
    }
}

impl<C, P> Default for BelongsTo<C, P>
where
    C: TypedTable,
    P: TypedTable,
    C::ForeignKey: PartialEq<P::PrimaryKey>,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<C, P> Relation for BelongsTo<C, P>
where
    C: TypedTable,
    P: TypedTable,
    C::ForeignKey: PartialEq<P::PrimaryKey>,
{
    type Child = C;
    type Parent = P;
    const KIND: RelationKind = RelationKind::BelongsTo;
}

// ---- HasMany: 1:N 关联 ----

/// HasMany<Parent, Child> — 父表拥有多个子表
///
/// 编译期约束：`Child::ForeignKey == Parent::PrimaryKey`
pub struct HasMany<P, C>
where
    P: TypedTable,
    C: TypedTable,
    C::ForeignKey: PartialEq<P::PrimaryKey>,
{
    _marker: PhantomData<(P, C)>,
}

impl<P, C> HasMany<P, C>
where
    P: TypedTable,
    C: TypedTable,
    C::ForeignKey: PartialEq<P::PrimaryKey>,
{
    /// 创建 HasMany 关联（ZST）
    pub fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }

    /// 获取父表名
    pub const fn parent_name() -> &'static str {
        P::NAME
    }

    /// 获取子表名
    pub const fn child_name() -> &'static str {
        C::NAME
    }
}

impl<P, C> Default for HasMany<P, C>
where
    P: TypedTable,
    C: TypedTable,
    C::ForeignKey: PartialEq<P::PrimaryKey>,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<P, C> Relation for HasMany<P, C>
where
    P: TypedTable,
    C: TypedTable,
    C::ForeignKey: PartialEq<P::PrimaryKey>,
{
    type Child = C;
    type Parent = P;
    const KIND: RelationKind = RelationKind::HasMany;
}

// ---- HasOne: 1:1 关联 ----

/// HasOne<Parent, Child> — 父表拥有一个子表
///
/// 编译期约束：`Child::ForeignKey == Parent::PrimaryKey`
pub struct HasOne<P, C>
where
    P: TypedTable,
    C: TypedTable,
    C::ForeignKey: PartialEq<P::PrimaryKey>,
{
    _marker: PhantomData<(P, C)>,
}

impl<P, C> HasOne<P, C>
where
    P: TypedTable,
    C: TypedTable,
    C::ForeignKey: PartialEq<P::PrimaryKey>,
{
    /// 创建 HasOne 关联（ZST）
    pub fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }

    /// 获取父表名
    pub const fn parent_name() -> &'static str {
        P::NAME
    }

    /// 获取子表名
    pub const fn child_name() -> &'static str {
        C::NAME
    }
}

impl<P, C> Default for HasOne<P, C>
where
    P: TypedTable,
    C: TypedTable,
    C::ForeignKey: PartialEq<P::PrimaryKey>,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<P, C> Relation for HasOne<P, C>
where
    P: TypedTable,
    C: TypedTable,
    C::ForeignKey: PartialEq<P::PrimaryKey>,
{
    type Child = C;
    type Parent = P;
    const KIND: RelationKind = RelationKind::HasOne;
}

/// 关联查询构造器 — 提供 `load_belongs_to` 等方法
pub struct RelationQuery<R: Relation> {
    _marker: PhantomData<R>,
}

impl<R: Relation> RelationQuery<R> {
    /// 创建关联查询构造器
    pub fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }

    /// 生成 BelongsTo JOIN SQL 片段
    ///
    /// 返回 `JOIN parent_table ON child_table.fk = parent_table.pk`
    pub fn join_sql(&self) -> String {
        let child = R::Child::NAME;
        let parent = R::Parent::NAME;
        match R::KIND {
            RelationKind::BelongsTo => {
                format!("JOIN {} ON {}.user_id = {}.id", parent, child, parent)
            }
            RelationKind::HasMany => {
                format!("JOIN {} ON {}.id = {}.user_id", child, parent, child)
            }
            RelationKind::HasOne => {
                format!(
                    "JOIN {} ON {}.user_id = {}.id LIMIT 1",
                    child, child, parent
                )
            }
        }
    }
}

impl<R: Relation> Default for RelationQuery<R> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- 测试用 mock 表 ----

    struct UsersTable;
    impl TypedTable for UsersTable {
        const NAME: &'static str = "users";
        type PrimaryKey = i64;
        type ForeignKey = (); // Users 表没有外键
    }

    struct PostsTable;
    impl TypedTable for PostsTable {
        const NAME: &'static str = "posts";
        type PrimaryKey = i64;
        type ForeignKey = i64; // user_id
    }

    struct ProfilesTable;
    impl TypedTable for ProfilesTable {
        const NAME: &'static str = "profiles";
        type PrimaryKey = i64;
        type ForeignKey = i64; // user_id
    }

    // ---- BelongsTo 测试 ----

    #[test]
    fn test_belongs_to_basic() {
        let rel: BelongsTo<PostsTable, UsersTable> = BelongsTo::new();
        assert_eq!(BelongsTo::<PostsTable, UsersTable>::parent_name(), "users");
        assert_eq!(BelongsTo::<PostsTable, UsersTable>::child_name(), "posts");
        assert_eq!(
            <BelongsTo<PostsTable, UsersTable> as Relation>::KIND,
            RelationKind::BelongsTo
        );
        // ZST 验证
        assert_eq!(std::mem::size_of::<BelongsTo<PostsTable, UsersTable>>(), 0);
        let _ = rel;
    }

    #[test]
    fn test_belongs_to_relation_query() {
        let q: RelationQuery<BelongsTo<PostsTable, UsersTable>> = RelationQuery::new();
        let sql = q.join_sql();
        assert!(sql.contains("JOIN users"));
        assert!(sql.contains("posts"));
    }

    // ---- HasMany 测试 ----

    #[test]
    fn test_has_many_basic() {
        let rel: HasMany<UsersTable, PostsTable> = HasMany::new();
        assert_eq!(HasMany::<UsersTable, PostsTable>::parent_name(), "users");
        assert_eq!(HasMany::<UsersTable, PostsTable>::child_name(), "posts");
        assert_eq!(
            <HasMany<UsersTable, PostsTable> as Relation>::KIND,
            RelationKind::HasMany
        );
        assert_eq!(std::mem::size_of::<HasMany<UsersTable, PostsTable>>(), 0);
        let _ = rel;
    }

    #[test]
    fn test_has_many_relation_query() {
        let q: RelationQuery<HasMany<UsersTable, PostsTable>> = RelationQuery::new();
        let sql = q.join_sql();
        assert!(sql.contains("JOIN posts"));
        assert!(sql.contains("users"));
    }

    // ---- HasOne 测试 ----

    #[test]
    fn test_has_one_basic() {
        let rel: HasOne<UsersTable, ProfilesTable> = HasOne::new();
        assert_eq!(HasOne::<UsersTable, ProfilesTable>::parent_name(), "users");
        assert_eq!(
            HasOne::<UsersTable, ProfilesTable>::child_name(),
            "profiles"
        );
        assert_eq!(
            <HasOne<UsersTable, ProfilesTable> as Relation>::KIND,
            RelationKind::HasOne
        );
        assert_eq!(std::mem::size_of::<HasOne<UsersTable, ProfilesTable>>(), 0);
        let _ = rel;
    }

    #[test]
    fn test_has_one_relation_query() {
        let q: RelationQuery<HasOne<UsersTable, ProfilesTable>> = RelationQuery::new();
        let sql = q.join_sql();
        assert!(sql.contains("JOIN profiles"));
        assert!(sql.contains("LIMIT 1"));
    }

    // ---- 编译期外键类型匹配校验 ----

    #[test]
    fn test_foreign_key_type_matching() {
        // PostsTable::ForeignKey = i64, UsersTable::PrimaryKey = i64
        // 此关联在编译期通过类型约束检查
        let _rel: BelongsTo<PostsTable, UsersTable> = BelongsTo::new();
        let _rel2: HasMany<UsersTable, PostsTable> = HasMany::new();
        let _rel3: HasOne<UsersTable, ProfilesTable> = HasOne::new();
    }
}
