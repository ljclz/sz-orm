//! v9.2.0 M3 T16: SqliteDialect 方言测试

use sz_orm_core::dialect::{ColumnDef, SqliteDialect, TableChange};
use sz_orm_core::Dialect;

#[test]
fn test_sqlite_quote_identifier() {
    let d = SqliteDialect;
    assert_eq!(d.quote("users"), "\"users\"");
}

#[test]
fn test_sqlite_escape_string() {
    let d = SqliteDialect;
    let escaped = d.escape_string("it's");
    assert!(escaped.contains("''"));
}

#[test]
fn test_sqlite_build_pagination() {
    let d = SqliteDialect;
    let sql = d.build_pagination("SELECT * FROM users", 1, 10);
    assert!(sql.to_uppercase().contains("LIMIT"));
}

#[test]
fn test_sqlite_build_create_table() {
    let d = SqliteDialect;
    let cols = vec![ColumnDef {
        name: "id".to_string(),
        sql_type: "INTEGER".to_string(),
        nullable: false,
        default: None,
        auto_increment: true,
        primary_key: true,
    }];
    let sql = d.build_create_table("users", &cols);
    assert!(sql.to_uppercase().contains("CREATE TABLE"));
}

#[test]
fn test_sqlite_build_alter_table_drop_column() {
    let d = SqliteDialect;
    let changes = vec![TableChange::DropColumn("old_col".to_string())];
    let sql = d.build_alter_table("users", &changes);
    assert!(sql.to_uppercase().contains("ALTER TABLE"));
}

#[test]
fn test_sqlite_supports_lock_for_update() {
    let d = SqliteDialect;
    assert!(!d.supports_lock_for_update());
}
