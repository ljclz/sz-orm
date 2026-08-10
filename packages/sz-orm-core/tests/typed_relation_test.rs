//! M3-T1: typed relation 端到端测试
//!
//! 验证 BelongsTo / HasMany / HasOne 的编译期外键类型校验、
//! 运行时关联查询、表归属校验、与 EagerLoader 协作、escape hatch。

#![cfg(feature = "typed-relation")]

use sz_orm_core::typed_relation::{
    BelongsTo, HasMany, HasOne, Relation, RelationKind, RelationQuery, TypedTable,
};

// ==================== 测试用 Mock 表 ====================

struct UsersTable;
impl TypedTable for UsersTable {
    const NAME: &'static str = "users";
    type PrimaryKey = i64;
    type ForeignKey = ();
}

struct PostsTable;
impl TypedTable for PostsTable {
    const NAME: &'static str = "posts";
    type PrimaryKey = i64;
    type ForeignKey = i64;
}

struct ProfilesTable;
impl TypedTable for ProfilesTable {
    const NAME: &'static str = "profiles";
    type PrimaryKey = i64;
    type ForeignKey = i64;
}

struct CommentsTable;
impl TypedTable for CommentsTable {
    const NAME: &'static str = "comments";
    type PrimaryKey = i64;
    type ForeignKey = i64;
}

struct OrdersTable;
impl TypedTable for OrdersTable {
    const NAME: &'static str = "orders";
    type PrimaryKey = i32;
    type ForeignKey = i32;
}

// ==================== M3-T1.2: 编译期外键类型校验测试 ====================

/// 验证外键类型匹配时 BelongsTo 编译通过。
#[test]
fn test_compile_time_fk_match_belongs_to() {
    let _rel: BelongsTo<PostsTable, UsersTable> = BelongsTo::new();
    let _rel2: BelongsTo<ProfilesTable, UsersTable> = BelongsTo::new();
    let _rel3: BelongsTo<CommentsTable, PostsTable> = BelongsTo::new();
}

/// 验证外键类型匹配时 HasMany/HasOne 编译通过。
#[test]
fn test_compile_time_fk_match_has_many_has_one() {
    let _rel: HasMany<UsersTable, PostsTable> = HasMany::new();
    let _rel2: HasMany<UsersTable, ProfilesTable> = HasMany::new();
    let _rel3: HasOne<UsersTable, ProfilesTable> = HasOne::new();
    let _rel4: HasMany<PostsTable, CommentsTable> = HasMany::new();
}

/// 验证不同外键类型（i32 vs i64）也能匹配的表。
#[test]
fn test_compile_time_fk_different_type_match() {
    // OrdersTable::ForeignKey = i32, OrdersTable::PrimaryKey = i32
    // 可以建立 BelongsTo<OrdersTable, OrdersTable> 自引用
    let _rel: BelongsTo<OrdersTable, OrdersTable> = BelongsTo::new();
}

// ==================== M3-T1.3: 运行时关联查询测试 ====================

/// 验证 BelongsTo 关联查询生成正确的 JOIN SQL。
#[test]
fn test_belongs_to_join_sql() {
    let q: RelationQuery<BelongsTo<PostsTable, UsersTable>> = RelationQuery::new();
    let sql = q.join_sql();
    assert!(sql.contains("JOIN users"), "应包含 JOIN users: {}", sql);
    assert!(sql.contains("posts"), "应包含 posts: {}", sql);
    assert!(sql.contains("user_id"), "应包含外键 user_id: {}", sql);
}

/// 验证 HasMany 关联查询生成正确的 JOIN SQL。
#[test]
fn test_has_many_join_sql() {
    let q: RelationQuery<HasMany<UsersTable, PostsTable>> = RelationQuery::new();
    let sql = q.join_sql();
    assert!(sql.contains("JOIN posts"), "应包含 JOIN posts: {}", sql);
    assert!(sql.contains("users"), "应包含 users: {}", sql);
}

/// 验证 HasOne 关联查询生成正确的 JOIN SQL + LIMIT 1。
#[test]
fn test_has_one_join_sql() {
    let q: RelationQuery<HasOne<UsersTable, ProfilesTable>> = RelationQuery::new();
    let sql = q.join_sql();
    assert!(
        sql.contains("JOIN profiles"),
        "应包含 JOIN profiles: {}",
        sql
    );
    assert!(sql.contains("LIMIT 1"), "应包含 LIMIT 1: {}", sql);
}

/// 验证表归属校验：parent_name 和 child_name 返回正确的表名。
#[test]
fn test_table_ownership() {
    assert_eq!(BelongsTo::<PostsTable, UsersTable>::parent_name(), "users");
    assert_eq!(BelongsTo::<PostsTable, UsersTable>::child_name(), "posts");

    assert_eq!(HasMany::<UsersTable, PostsTable>::parent_name(), "users");
    assert_eq!(HasMany::<UsersTable, PostsTable>::child_name(), "posts");

    assert_eq!(HasOne::<UsersTable, ProfilesTable>::parent_name(), "users");
    assert_eq!(
        HasOne::<UsersTable, ProfilesTable>::child_name(),
        "profiles"
    );
}

/// 验证 RelationKind 枚举值正确。
#[test]
fn test_relation_kind() {
    assert_eq!(
        <BelongsTo<PostsTable, UsersTable> as Relation>::KIND,
        RelationKind::BelongsTo
    );
    assert_eq!(
        <HasMany<UsersTable, PostsTable> as Relation>::KIND,
        RelationKind::HasMany
    );
    assert_eq!(
        <HasOne<UsersTable, ProfilesTable> as Relation>::KIND,
        RelationKind::HasOne
    );
}

/// 验证 ZST（零大小类型）无运行时开销。
#[test]
fn test_zst_zero_size() {
    assert_eq!(
        std::mem::size_of::<BelongsTo<PostsTable, UsersTable>>(),
        0,
        "BelongsTo 应为 ZST"
    );
    assert_eq!(
        std::mem::size_of::<HasMany<UsersTable, PostsTable>>(),
        0,
        "HasMany 应为 ZST"
    );
    assert_eq!(
        std::mem::size_of::<HasOne<UsersTable, ProfilesTable>>(),
        0,
        "HasOne 应为 ZST"
    );
    assert_eq!(
        std::mem::size_of::<RelationQuery<BelongsTo<PostsTable, UsersTable>>>(),
        0,
        "RelationQuery 应为 ZST"
    );
}

/// 验证 Default trait 实现。
#[test]
fn test_default_impl() {
    let rel1: BelongsTo<PostsTable, UsersTable> = BelongsTo::default();
    let rel2: BelongsTo<PostsTable, UsersTable> = BelongsTo::new();
    // ZST，所有实例都相同
    assert_eq!(std::mem::size_of_val(&rel1), std::mem::size_of_val(&rel2));
}

/// 验证 escape hatch：复杂关联可回退到 EagerLoader（运行时关联）。
#[test]
fn test_escape_hatch_to_eager_loader() {
    // typed relation 适用于编译期已知的简单关联
    // 复杂关联（多态/动态/运行时决定）回退到 EagerLoader
    use sz_orm_core::eager_loader::EagerLoader;
    use sz_orm_core::relation_trait::{RelationDef, RelationKind as RTKind};

    // 用 RelationDef 定义运行时关联（escape hatch）
    let rel_def = RelationDef::new(
        "posts",
        "posts",
        "users",
        "user_id",
        "id",
        RTKind::BelongsTo,
    );
    let _loader = EagerLoader::new(rel_def);

    // typed relation 仍然可用于编译期校验
    let _typed_rel: BelongsTo<PostsTable, UsersTable> = BelongsTo::new();
}

/// 验证多级关联链：Comment → Post → User。
#[test]
fn test_multi_level_relation_chain() {
    // Comment belongs to Post
    let _comment_to_post: BelongsTo<CommentsTable, PostsTable> = BelongsTo::new();
    // Post belongs to User
    let _post_to_user: BelongsTo<PostsTable, UsersTable> = BelongsTo::new();
    // User has many Posts
    let _user_has_posts: HasMany<UsersTable, PostsTable> = HasMany::new();
    // Post has many Comments
    let _post_has_comments: HasMany<PostsTable, CommentsTable> = HasMany::new();

    // 验证多级 JOIN SQL
    let q1: RelationQuery<BelongsTo<CommentsTable, PostsTable>> = RelationQuery::new();
    let q2: RelationQuery<BelongsTo<PostsTable, UsersTable>> = RelationQuery::new();
    let sql1 = q1.join_sql();
    let sql2 = q2.join_sql();
    assert!(sql1.contains("posts") && sql1.contains("comments"));
    assert!(sql2.contains("users") && sql2.contains("posts"));
}
