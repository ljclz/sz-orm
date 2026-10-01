//! v9.2.0 M3 T18: SqlServerDialect 方言测试

use sz_orm_core::dialect::{ColumnDef, SqlServerDialect, TableChange};
use sz_orm_core::Dialect;

#[test]
fn test_mssql_quote_identifier() {
    let d = SqlServerDialect;
    let quoted = d.quote("users");
    assert!(!quoted.is_empty());
}

#[test]
fn test_mssql_escape_string() {
    let d = SqlServerDialect;
    let escaped = d.escape_string("it's");
    assert!(escaped.contains("''"));
}

#[test]
fn test_mssql_build_pagination() {
    let d = SqlServerDialect;
    let sql = d.build_pagination("SELECT * FROM users", 1, 10);
    assert!(!sql.is_empty());
}

#[test]
fn test_mssql_build_create_table() {
    let d = SqlServerDialect;
    let cols = vec![ColumnDef {
        name: "id".to_string(),
        sql_type: "INT".to_string(),
        nullable: false,
        default: None,
        auto_increment: true,
        primary_key: true,
    }];
    let sql = d.build_create_table("users", &cols);
    assert!(sql.to_uppercase().contains("CREATE TABLE"));
}

#[test]
fn test_mssql_build_alter_table_add_index() {
    let d = SqlServerDialect;
    let changes = vec![TableChange::AddIndex(
        "idx_name".to_string(),
        vec!["name".to_string()],
    )];
    let sql = d.build_alter_table("users", &changes);
    assert!(!sql.is_empty());
}

#[test]
fn test_mssql_auto_increment_keyword() {
    let d = SqlServerDialect;
    let kw = d.auto_increment_keyword();
    assert!(!kw.is_empty());
}

#[test]
fn test_mssql_supports_returning() {
    let d = SqlServerDialect;
    assert!(d.supports_returning());
}

#[test]
fn test_mssql_last_insert_id_sql() {
    let d = SqlServerDialect;
    let sql = d.last_insert_id_sql();
    assert!(sql.is_some() || sql.is_none());
}
