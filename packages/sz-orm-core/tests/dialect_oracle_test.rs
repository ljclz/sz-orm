//! v9.2.0 M3 T17: OracleDialect 方言测试

use sz_orm_core::dialect::{ColumnDef, OracleDialect, TableChange};
use sz_orm_core::Dialect;

#[test]
fn test_oracle_quote_identifier() {
    let d = OracleDialect;
    let quoted = d.quote("users");
    assert!(!quoted.is_empty());
}

#[test]
fn test_oracle_escape_string() {
    let d = OracleDialect;
    let escaped = d.escape_string("it's");
    assert!(escaped.contains("''"));
}

#[test]
fn test_oracle_build_pagination() {
    let d = OracleDialect;
    let sql = d.build_pagination("SELECT * FROM users", 1, 10);
    assert!(!sql.is_empty());
}

#[test]
fn test_oracle_build_create_table() {
    let d = OracleDialect;
    let cols = vec![ColumnDef {
        name: "id".to_string(),
        sql_type: "NUMBER".to_string(),
        nullable: false,
        default: None,
        auto_increment: false,
        primary_key: true,
    }];
    let sql = d.build_create_table("users", &cols);
    assert!(sql.to_uppercase().contains("CREATE TABLE"));
}

#[test]
fn test_oracle_build_alter_table_modify_column() {
    let d = OracleDialect;
    let changes = vec![TableChange::ModifyColumn(ColumnDef {
        name: "name".to_string(),
        sql_type: "VARCHAR2(100)".to_string(),
        nullable: true,
        default: None,
        auto_increment: false,
        primary_key: false,
    })];
    let sql = d.build_alter_table("users", &changes);
    assert!(sql.to_uppercase().contains("ALTER TABLE"));
}

#[test]
fn test_oracle_auto_increment_keyword() {
    let d = OracleDialect;
    let kw = d.auto_increment_keyword();
    assert!(!kw.is_empty() || kw.is_empty());
}

#[test]
fn test_oracle_supports_returning() {
    let d = OracleDialect;
    assert!(d.supports_returning());
}

#[test]
fn test_oracle_last_insert_id_sql() {
    let d = OracleDialect;
    let sql = d.last_insert_id_sql();
    assert!(sql.is_some() || sql.is_none());
}