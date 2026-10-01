//! v9.2.0 M3 T14: MySqlDialect 方言测试

use sz_orm_core::dialect::{ColumnDef, LockType, MySqlDialect, TableChange};
use sz_orm_core::Dialect;

#[test]
fn test_mysql_quote_identifier() {
    let d = MySqlDialect;
    assert_eq!(d.quote("users"), "`users`");
}

#[test]
fn test_mysql_escape_string_single_quote() {
    let d = MySqlDialect;
    let escaped = d.escape_string("it's");
    assert!(escaped.contains("\\'") || escaped.contains("''"));
}

#[test]
fn test_mysql_escape_string_backslash() {
    let d = MySqlDialect;
    let escaped = d.escape_string("a\\b");
    assert!(escaped.contains("\\\\") || escaped.len() > 3);
}

#[test]
fn test_mysql_escape_string_empty() {
    let d = MySqlDialect;
    assert_eq!(d.escape_string(""), "");
}

#[test]
fn test_mysql_build_pagination() {
    let d = MySqlDialect;
    let sql = d.build_pagination("SELECT * FROM users", 1, 10);
    assert!(sql.to_uppercase().contains("LIMIT"));
}

#[test]
fn test_mysql_json_type() {
    let d = MySqlDialect;
    let jt = d.json_type();
    assert!(!jt.is_empty());
}

#[test]
fn test_mysql_json_extract() {
    let d = MySqlDialect;
    let sql = d.json_extract("data", "$.key");
    assert!(!sql.is_empty());
}

#[test]
fn test_mysql_full_text_search() {
    let d = MySqlDialect;
    let sql = d.full_text_search(&["title", "body"], "keyword");
    assert!(!sql.is_empty());
}

#[test]
fn test_mysql_concat() {
    let d = MySqlDialect;
    let sql = d.concat(&["a", "b", "c"]);
    assert!(!sql.is_empty());
}

#[test]
fn test_mysql_build_create_table() {
    let d = MySqlDialect;
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
    assert!(sql.contains("users"));
}

#[test]
fn test_mysql_build_alter_table_add_column() {
    let d = MySqlDialect;
    let changes = vec![TableChange::AddColumn(ColumnDef {
        name: "email".to_string(),
        sql_type: "VARCHAR(255)".to_string(),
        nullable: true,
        default: None,
        auto_increment: false,
        primary_key: false,
    })];
    let sql = d.build_alter_table("users", &changes);
    assert!(sql.to_uppercase().contains("ALTER TABLE"));
    assert!(sql.to_uppercase().contains("ADD"));
}

#[test]
fn test_mysql_build_lock_clause_for_update() {
    let d = MySqlDialect;
    let clause = d.build_lock_clause(LockType::ForUpdate);
    assert!(clause.is_some());
    assert!(clause.unwrap().to_uppercase().contains("FOR UPDATE"));
}

#[test]
fn test_mysql_supports_returning() {
    let d = MySqlDialect;
    assert!(!d.supports_returning());
}
