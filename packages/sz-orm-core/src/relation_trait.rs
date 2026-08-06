//! RelationTrait — 类型安全的关联关系定义与 JOIN 链式 API
//!
//! 提供 `RelationKind` / `RelationDef` / `RelationTrait` 核心类型，
//! 配合 `#[derive(Relation)]` 宏自动生成 `RelationTrait` 实现，
//! 追平 SeaORM `User::find().join(Posts)` 链式关联查询体验。
//!
//! # 设计
//!
//! - `RelationDef` 使用 `&'static str` 零分配描述关联关系
//! - `RelationTrait` 提供 `def()` / `all_relations()` 方法
//! - `RelationKind::default_join_type()` 决定 JOIN 类型（HasOne/BelongsTo → INNER，HasMany/ManyToMany → LEFT）
//!
//! # 用法
//!
//! ```ignore
//! use sz_orm_core::relation_trait::{RelationDef, RelationKind, RelationTrait};
//!
//! struct User;
//!
//! impl RelationTrait for User {
//!     fn def(&self) -> &'static RelationDef { &RELATIONS[0] }
//!     fn all_relations() -> &'static [RelationDef] { RELATIONS }
//! }
//!
//! static RELATIONS: &[RelationDef] = &[
//!     RelationDef::new("orders", "users", "orders", "id", "user_id", RelationKind::HasMany),
//! ];
//! ```

/// 关联关系类型
///
/// 决定 JOIN 策略和数据加载方式：
/// - `HasOne` / `BelongsTo` → INNER JOIN（一条关联记录）
/// - `HasMany` / `ManyToMany` → LEFT JOIN（多条关联记录，双查询策略避免行膨胀）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RelationKind {
    /// 一对一：当前实体拥有一个关联实体（如 User → Profile）
    HasOne,
    /// 一对多：当前实体拥有多个关联实体（如 User → Orders）
    HasMany,
    /// 多对一：当前实体属于一个父实体（如 Order → User）
    BelongsTo,
    /// 多对多：通过中间表关联（如 User ↔ Role，通过 user_roles）
    ManyToMany,
}

impl RelationKind {
    /// 返回该关系类型默认的 JOIN 类型
    ///
    /// - `HasOne` / `BelongsTo` → `JoinKind::Inner`（关联记录存在性要求）
    /// - `HasMany` / `ManyToMany` → `JoinKind::Left`（允许零关联记录）
    pub fn default_join_type(self) -> JoinKind {
        match self {
            RelationKind::HasOne | RelationKind::BelongsTo => JoinKind::Inner,
            RelationKind::HasMany | RelationKind::ManyToMany => JoinKind::Left,
        }
    }
}

/// JOIN 类型（与 `join_dsl::JoinKind` 对齐，独立定义避免循环依赖）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinKind {
    /// INNER JOIN
    Inner,
    /// LEFT [OUTER] JOIN
    Left,
}

impl JoinKind {
    /// 转换为 SQL 关键字
    pub fn as_sql(self) -> &'static str {
        match self {
            JoinKind::Inner => "INNER JOIN",
            JoinKind::Left => "LEFT JOIN",
        }
    }
}

/// 关联关系定义（零分配，编译期常量）
///
/// 描述两个实体间的关联关系，包含外键映射信息。
/// 所有字段为 `&'static str`，运行时零分配。
#[derive(Debug, Clone)]
pub struct RelationDef {
    /// 关联名称（如 "orders"、"profile"）
    pub name: &'static str,
    /// 源实体表名（如 "users"）
    pub from_entity: &'static str,
    /// 目标实体表名（如 "orders"）
    pub to_entity: &'static str,
    /// 源实体键列名（通常为主键，如 "id"）
    pub from_key: &'static str,
    /// 目标实体外键列名（如 "user_id"）
    pub to_key: &'static str,
    /// 关联类型
    pub kind: RelationKind,
}

impl RelationDef {
    /// 创建关联关系定义
    pub const fn new(
        name: &'static str,
        from_entity: &'static str,
        to_entity: &'static str,
        from_key: &'static str,
        to_key: &'static str,
        kind: RelationKind,
    ) -> Self {
        Self {
            name,
            from_entity,
            to_entity,
            from_key,
            to_key,
            kind,
        }
    }
}

/// 关联关系 trait — 由 `#[derive(Relation)]` 自动实现
///
/// 提供关联定义访问和批量关联查询能力。
/// 实体类型实现此 trait 后，可通过 `QueryBuilder::join()` 链式构建 JOIN 查询。
pub trait RelationTrait: Send + Sync {
    /// 返回当前关联的定义
    fn def(&self) -> &'static RelationDef;

    /// 返回实体所有关联定义的静态切片
    fn all_relations() -> &'static [RelationDef]
    where
        Self: Sized;

    /// 按名称查找关联定义
    fn relation_by_name(name: &str) -> Option<&'static RelationDef>
    where
        Self: Sized,
    {
        Self::all_relations().iter().find(|r| r.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_RELATIONS: &[RelationDef] = &[
        RelationDef::new("orders", "users", "orders", "id", "user_id", RelationKind::HasMany),
        RelationDef::new("profile", "users", "profiles", "id", "user_id", RelationKind::HasOne),
        RelationDef::new(
            "owner",
            "orders",
            "users",
            "user_id",
            "id",
            RelationKind::BelongsTo,
        ),
        RelationDef::new(
            "roles",
            "users",
            "roles",
            "id",
            "role_id",
            RelationKind::ManyToMany,
        ),
    ];

    struct User;

    impl RelationTrait for User {
        fn def(&self) -> &'static RelationDef {
            &TEST_RELATIONS[0]
        }
        fn all_relations() -> &'static [RelationDef] {
            TEST_RELATIONS
        }
    }

    #[test]
    fn test_relation_kind_default_join_type() {
        assert_eq!(RelationKind::HasOne.default_join_type(), JoinKind::Inner);
        assert_eq!(RelationKind::BelongsTo.default_join_type(), JoinKind::Inner);
        assert_eq!(RelationKind::HasMany.default_join_type(), JoinKind::Left);
        assert_eq!(
            RelationKind::ManyToMany.default_join_type(),
            JoinKind::Left
        );
    }

    #[test]
    fn test_join_kind_as_sql() {
        assert_eq!(JoinKind::Inner.as_sql(), "INNER JOIN");
        assert_eq!(JoinKind::Left.as_sql(), "LEFT JOIN");
    }

    #[test]
    fn test_relation_def_new() {
        let def = RelationDef::new(
            "orders",
            "users",
            "orders",
            "id",
            "user_id",
            RelationKind::HasMany,
        );
        assert_eq!(def.name, "orders");
        assert_eq!(def.from_entity, "users");
        assert_eq!(def.to_entity, "orders");
        assert_eq!(def.from_key, "id");
        assert_eq!(def.to_key, "user_id");
        assert_eq!(def.kind, RelationKind::HasMany);
    }

    #[test]
    fn test_relation_trait_all_relations() {
        let relations = User::all_relations();
        assert_eq!(relations.len(), 4);
        assert_eq!(relations[0].name, "orders");
        assert_eq!(relations[1].name, "profile");
        assert_eq!(relations[2].name, "owner");
        assert_eq!(relations[3].name, "roles");
    }

    #[test]
    fn test_relation_trait_relation_by_name() {
        let found = User::relation_by_name("orders");
        assert!(found.is_some());
        assert_eq!(found.unwrap().to_entity, "orders");
        assert_eq!(found.unwrap().kind, RelationKind::HasMany);

        let not_found = User::relation_by_name("unknown");
        assert!(not_found.is_none());
    }

    #[test]
    fn test_relation_trait_def() {
        let user = User;
        let def = user.def();
        assert_eq!(def.name, "orders");
        assert_eq!(def.kind, RelationKind::HasMany);
    }
}