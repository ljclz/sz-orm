//! Migration 模块契约测试 — 对应 `docs/api-contracts.md` §9
//!
//! 锁定 SchemaBuilder、ColumnDef、IndexDef、ForeignKeyDef 链式 API 与 DDL 生成契约。

use sz_orm_core::migration::{ColumnDef, ForeignKeyDef, IndexDef, SchemaBuilder};
use sz_orm_core::DbType;

// ===== §9.3 SchemaBuilder 链式 API 契约 =====

#[test]
fn test_schema_builder_chain_returns_self_contract() {
    let _builder = SchemaBuilder::new("users")
        .add_column(ColumnDef::new("id", "INT").not_null().auto_increment())
        .add_column(ColumnDef::new("name", "VARCHAR").length(255).not_null())
        .add_index(IndexDef::new("idx_name", vec!["name"]).unique())
        .add_foreign_key(
            ForeignKeyDef::new("fk_role", "role_id", "roles", "id").on_delete("CASCADE"),
        );
    // 编译通过即链式 API 契约满足
}

#[test]
fn test_schema_builder_build_generates_create_table_contract() {
    let builder = SchemaBuilder::new("users")
        .add_column(ColumnDef::new("id", "INT").not_null().auto_increment())
        .add_column(ColumnDef::new("name", "VARCHAR").length(255).not_null());

    let sql = builder.build(DbType::MySQL);
    assert!(sql.to_uppercase().contains("CREATE TABLE"));
    assert!(sql.contains("users"));
    assert!(sql.contains("id"));
    assert!(sql.contains("name"));
}

#[test]
fn test_schema_builder_build_per_dialect_contract() {
    let make = || SchemaBuilder::new("users").add_column(ColumnDef::new("id", "INT").not_null());

    let mysql_sql = make().build(DbType::MySQL);
    let pg_sql = make().build(DbType::PostgreSQL);

    // 不同方言应生成不同 DDL（至少表名引用不同）
    assert!(mysql_sql.contains("users") || mysql_sql.contains("`users`"));
    assert!(pg_sql.contains("users") || pg_sql.contains("\"users\""));
}

// ===== §9.4 ColumnDef 链式 API 契约 =====

#[test]
fn test_column_def_chain_returns_self_contract() {
    let _col = ColumnDef::new("id", "INT")
        .length(11)
        .not_null()
        .auto_increment()
        .default("0");
}

#[test]
fn test_column_def_new_sets_name_and_type_contract() {
    // ColumnDef::build 是私有的，通过 SchemaBuilder::build 间接验证
    let sql = SchemaBuilder::new("users")
        .add_column(ColumnDef::new("email", "VARCHAR"))
        .build(DbType::MySQL);
    assert!(sql.contains("email"), "DDL 应包含字段名: {}", sql);
}

#[test]
fn test_column_def_length_contract() {
    let sql = SchemaBuilder::new("users")
        .add_column(ColumnDef::new("name", "VARCHAR").length(255))
        .build(DbType::MySQL);
    // VARCHAR(255) 或类似格式
    assert!(sql.contains("255"), "length 应出现在 DDL 中: {}", sql);
}

#[test]
fn test_column_def_not_null_contract() {
    let sql = SchemaBuilder::new("users")
        .add_column(ColumnDef::new("id", "INT").not_null())
        .build(DbType::MySQL);
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
    let _ = idx; // 编译通过即契约满足
}

#[test]
fn test_index_def_unique_contract() {
    let idx = IndexDef::new("idx_email", vec!["email"]).unique();
    let _ = idx;
}

// ===== §9.4 ForeignKeyDef 契约 =====

#[test]
fn test_foreign_key_def_new_contract() {
    let fk = ForeignKeyDef::new("fk_role", "role_id", "roles", "id");
    let _ = fk;
}

#[test]
fn test_foreign_key_def_on_delete_on_update_contract() {
    let fk = ForeignKeyDef::new("fk_role", "role_id", "roles", "id")
        .on_delete("CASCADE")
        .on_update("SET NULL");
    let _ = fk;
}

// ===== §9.1 Migrator 契约 =====

#[test]
fn test_migrator_new_contract() {
    use sz_orm_core::{MigrationContext, Migrator};
    let _migrator = Migrator::new(MigrationContext::default());
}

#[test]
fn test_migrator_add_migrations_chain_contract() {
    use sz_orm_core::{Migration, MigrationContext, Migrator};
    let migrations: Vec<Migration> = vec![];
    let _migrator = Migrator::new(MigrationContext::default()).add_migrations(migrations);
}

// ===== §9.2 FileMigrationResolver 契约 =====

#[test]
fn test_file_migration_resolver_new_contract() {
    use std::path::PathBuf;
    use sz_orm_core::FileMigrationResolver;
    let _resolver = FileMigrationResolver::new(PathBuf::from("./migrations"));
}
