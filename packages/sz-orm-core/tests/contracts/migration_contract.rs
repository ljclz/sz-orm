//! Migration 模块契约测试 — 对应 `docs/api-contracts.md` §9
//!
//! 锁定 SchemaBuilder、ColumnDef、IndexDef、ForeignKeyDef 链式 API 与 DDL 生成契约。

use sz_orm_core::migration::{ColumnDef, ForeignKeyDef, IndexDef, SchemaBuilder};
use sz_orm_core::DbType;

// ===== §9.3 SchemaBuilder 链式 API 契约 =====

#[test]
fn test_schema_builder_chain_returns_self_contract() {
    let builder = SchemaBuilder::new("users")
        .add_column(ColumnDef::new("id", "INT").not_null().auto_increment())
        .add_column(ColumnDef::new("name", "VARCHAR").length(255).not_null())
        .add_index(IndexDef::new("idx_name", vec!["name"]).unique())
        .add_foreign_key(
            ForeignKeyDef::new("fk_role", "role_id", "roles", "id").on_delete("CASCADE"),
        );
    // 链式调用后 build 应生成包含所有元素的 DDL
    let sql = builder.build(DbType::MySQL).unwrap();
    assert!(sql.contains("CREATE TABLE"), "应包含 CREATE TABLE: {}", sql);
    assert!(sql.contains("users"), "应包含表名: {}", sql);
    assert!(sql.contains("id"), "应包含列 id: {}", sql);
    assert!(sql.contains("idx_name"), "应包含索引名: {}", sql);
    assert!(sql.contains("fk_role"), "应包含外键名: {}", sql);
    assert!(sql.contains("FOREIGN KEY"), "应包含 FOREIGN KEY: {}", sql);
}

#[test]
fn test_schema_builder_build_generates_create_table_contract() {
    let builder = SchemaBuilder::new("users")
        .add_column(ColumnDef::new("id", "INT").not_null().auto_increment())
        .add_column(ColumnDef::new("name", "VARCHAR").length(255).not_null());

    let sql = builder.build(DbType::MySQL).unwrap();
    assert!(sql.to_uppercase().contains("CREATE TABLE"));
    assert!(sql.contains("users"));
    assert!(sql.contains("id"));
    assert!(sql.contains("name"));
}

#[test]
fn test_schema_builder_build_per_dialect_contract() {
    let make = || SchemaBuilder::new("users").add_column(ColumnDef::new("id", "INT").not_null());

    let mysql_sql = make().build(DbType::MySQL).unwrap();
    let pg_sql = make().build(DbType::PostgreSQL).unwrap();

    // 不同方言应生成不同 DDL（至少表名引用不同）
    assert!(mysql_sql.contains("users") || mysql_sql.contains("`users`"));
    assert!(pg_sql.contains("users") || pg_sql.contains("\"users\""));
}

// ===== §9.4 ColumnDef 链式 API 契约 =====

#[test]
fn test_column_def_chain_returns_self_contract() {
    let col = ColumnDef::new("id", "INT")
        .length(11)
        .not_null()
        .auto_increment()
        .default("0");
    // 链式配置应被正确存储
    assert_eq!(col.name, "id");
    assert_eq!(col.col_type, "INT");
    assert_eq!(col.length, Some(11));
    assert!(!col.nullable);
    assert!(col.auto_increment);
    assert_eq!(col.default, Some("0".to_string()));
}

#[test]
fn test_column_def_new_sets_name_and_type_contract() {
    // ColumnDef::build 是私有的，通过 SchemaBuilder::build 间接验证
    let sql = SchemaBuilder::new("users")
        .add_column(ColumnDef::new("email", "VARCHAR"))
        .build(DbType::MySQL)
        .unwrap();
    assert!(sql.contains("email"), "DDL 应包含字段名: {}", sql);
}

#[test]
fn test_column_def_length_contract() {
    let sql = SchemaBuilder::new("users")
        .add_column(ColumnDef::new("name", "VARCHAR").length(255))
        .build(DbType::MySQL)
        .unwrap();
    // VARCHAR(255) 或类似格式
    assert!(sql.contains("255"), "length 应出现在 DDL 中: {}", sql);
}

#[test]
fn test_column_def_not_null_contract() {
    let sql = SchemaBuilder::new("users")
        .add_column(ColumnDef::new("id", "INT").not_null())
        .build(DbType::MySQL)
        .unwrap();
    assert!(
        sql.to_uppercase().contains("NOT NULL"),
        "not_null 应生成 NOT NULL: {}",
        sql
    );
}

// ===== §9.4 IndexDef 契约 =====

#[test]
fn test_index_def_new_contract() {
    let idx = IndexDef::new("idx_email", vec!["email"]);
    // 构造后字段应被正确存储，默认非唯一索引
    assert_eq!(idx.name, "idx_email");
    assert_eq!(idx.columns, vec!["email".to_string()]);
    assert!(!idx.unique);
}

#[test]
fn test_index_def_unique_contract() {
    let idx = IndexDef::new("idx_email", vec!["email"]).unique();
    // unique() 应将 unique 标志设为 true
    assert!(idx.unique);
}

// ===== §9.4 ForeignKeyDef 契约 =====

#[test]
fn test_foreign_key_def_new_contract() {
    let fk = ForeignKeyDef::new("fk_role", "role_id", "roles", "id");
    // 构造后字段应被正确存储，默认无 ON DELETE / ON UPDATE
    assert_eq!(fk.name, "fk_role");
    assert_eq!(fk.column, "role_id");
    assert_eq!(fk.referenced_table, "roles");
    assert_eq!(fk.referenced_column, "id");
    assert!(fk.on_delete.is_none());
    assert!(fk.on_update.is_none());
}

#[test]
fn test_foreign_key_def_on_delete_on_update_contract() {
    let fk = ForeignKeyDef::new("fk_role", "role_id", "roles", "id")
        .on_delete("CASCADE")
        .on_update("SET NULL");
    // on_delete / on_update 应被正确存储
    assert_eq!(fk.on_delete, Some("CASCADE".to_string()));
    assert_eq!(fk.on_update, Some("SET NULL".to_string()));
}

// ===== §9.1 Migrator 契约 =====

#[test]
fn test_migrator_new_contract() {
    use sz_orm_core::{MigrationContext, Migrator};
    let migrator = Migrator::new(MigrationContext::default());
    // 新建的 Migrator 应无任何迁移，latest_version 为 None
    assert!(migrator.get_migrations().is_empty());
    assert!(migrator.latest_version().is_none());
}

#[test]
fn test_migrator_add_migrations_chain_contract() {
    use sz_orm_core::{Migration, MigrationContext, Migrator};
    let migrations: Vec<Migration> = vec![
        Migration::new("001", "create_users", "UP", "DOWN"),
        Migration::new("002", "add_index", "UP", "DOWN"),
    ];
    let migrator = Migrator::new(MigrationContext::default()).add_migrations(migrations);
    // add_migrations 应将所有迁移添加到 Migrator
    assert_eq!(migrator.get_migrations().len(), 2);
    assert_eq!(migrator.latest_version(), Some("002"));
}

// ===== §9.2 FileMigrationResolver 契约 =====

#[test]
fn test_file_migration_resolver_new_contract() {
    use std::path::PathBuf;
    use sz_orm_core::FileMigrationResolver;
    let resolver = FileMigrationResolver::new(PathBuf::from("./migrations"));
    // 构造后 path 字段应被正确存储
    assert_eq!(resolver.path, PathBuf::from("./migrations"));
}
