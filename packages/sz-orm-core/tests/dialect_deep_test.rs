//! dialect.rs 深度路径测试 — 覆盖各方言的 SQL 生成方法

use sz_orm_core::{get_dialect, DbType, Dialect};
use sz_orm_core::dialect::{ColumnDef, TableChange};

fn dialect(db: DbType) -> Box<dyn Dialect> {
    get_dialect(db).unwrap()
}

fn col(name: &str, sql_type: &str, nullable: bool, pk: bool, ai: bool) -> ColumnDef {
    ColumnDef {
        name: name.to_string(),
        sql_type: sql_type.to_string(),
        nullable,
        default: None,
        auto_increment: ai,
        primary_key: pk,
    }
}

#[test]
fn test_mysql_quote() {
    let d = dialect(DbType::MySQL);
    assert_eq!(d.quote("col"), "`col`");
    assert_eq!(d.quote("a`b"), "`a``b`");
}

#[test]
fn test_pg_quote() {
    let d = dialect(DbType::PostgreSQL);
    assert_eq!(d.quote("col"), "\"col\"");
}

#[test]
fn test_sqlite_quote() {
    let d = dialect(DbType::Sqlite);
    assert_eq!(d.quote("col"), "\"col\"");
}

#[test]
fn test_mysql_escape_string() {
    let d = dialect(DbType::MySQL);
    let escaped = d.escape_string("it's");
    assert!(escaped.contains("''") || escaped.contains("\\'"));
}

#[test]
fn test_pg_escape_string() {
    let d = dialect(DbType::PostgreSQL);
    assert!(d.escape_string("it's").contains("''"));
}

#[test]
fn test_sqlite_escape_string() {
    let d = dialect(DbType::Sqlite);
    assert!(d.escape_string("it's").contains("''"));
}

#[test]
fn test_supports_returning() {
    assert!(dialect(DbType::PostgreSQL).supports_returning());
    assert!(dialect(DbType::Sqlite).supports_returning());
}

#[test]
fn test_supports_if_exists() {
    assert!(dialect(DbType::MySQL).supports_if_exists());
    assert!(dialect(DbType::PostgreSQL).supports_if_exists());
    assert!(dialect(DbType::Sqlite).supports_if_exists());
}

#[test]
fn test_supports_if_not_exists() {
    assert!(dialect(DbType::MySQL).supports_if_not_exists());
    assert!(dialect(DbType::Sqlite).supports_if_not_exists());
}

#[test]
fn test_auto_increment_keyword() {
    let mysql = dialect(DbType::MySQL);
    assert!(mysql.auto_increment_keyword().contains("AUTO_INCREMENT"));
    let pg = dialect(DbType::PostgreSQL);
    assert!(!pg.auto_increment_keyword().is_empty());
}

#[test]
fn test_last_insert_id_sql() {
    let mysql = dialect(DbType::MySQL);
    assert!(mysql.last_insert_id_sql().is_some());
    let pg = dialect(DbType::PostgreSQL);
    let _ = pg.last_insert_id_sql();
}

#[test]
fn test_build_pagination_mysql() {
    let d = dialect(DbType::MySQL);
    let sql = d.build_pagination("SELECT * FROM t", 2, 10);
    assert!(sql.contains("LIMIT"));
    assert!(sql.contains("OFFSET"));
}

#[test]
fn test_build_pagination_pg() {
    let d = dialect(DbType::PostgreSQL);
    let sql = d.build_pagination("SELECT * FROM t", 3, 20);
    assert!(sql.contains("LIMIT"));
    assert!(sql.contains("OFFSET"));
}

#[test]
fn test_json_type() {
    let mysql = dialect(DbType::MySQL);
    assert!(!mysql.json_type().is_empty());
    let pg = dialect(DbType::PostgreSQL);
    assert!(!pg.json_type().is_empty());
}

#[test]
fn test_json_extract() {
    let d = dialect(DbType::MySQL);
    let sql = d.json_extract("data", "$.name");
    assert!(!sql.is_empty());
}

#[test]
fn test_full_text_search() {
    let d = dialect(DbType::MySQL);
    let sql = d.full_text_search(&["title", "body"], "keyword");
    assert!(!sql.is_empty());
}

#[test]
fn test_bool_to_int() {
    let d = dialect(DbType::MySQL);
    let sql = d.bool_to_int("is_active");
    assert!(!sql.is_empty());
}

#[test]
fn test_concat() {
    let d = dialect(DbType::MySQL);
    let sql = d.concat(&["a", "b", "c"]);
    assert!(!sql.is_empty());
}

#[test]
fn test_build_create_table_mysql() {
    let d = dialect(DbType::MySQL);
    let cols = vec![
        col("id", "BIGINT", false, true, true),
        col("name", "VARCHAR(255)", false, false, false),
    ];
    let sql = d.build_create_table("users", &cols);
    assert!(sql.contains("CREATE TABLE"));
    assert!(sql.contains("id"));
    assert!(sql.contains("name"));
}

#[test]
fn test_build_create_table_pg() {
    let d = dialect(DbType::PostgreSQL);
    let cols = vec![
        col("id", "BIGSERIAL", false, true, true),
        col("name", "TEXT", true, false, false),
    ];
    let sql = d.build_create_table("users", &cols);
    assert!(sql.contains("CREATE TABLE"));
}

#[test]
fn test_build_create_table_sqlite() {
    let d = dialect(DbType::Sqlite);
    let cols = vec![
        col("id", "INTEGER", false, true, true),
        col("name", "TEXT", false, false, false),
    ];
    let sql = d.build_create_table("t", &cols);
    assert!(sql.contains("CREATE TABLE"));
}

#[test]
fn test_build_alter_table_add_column() {
    let d = dialect(DbType::MySQL);
    let changes = vec![TableChange::AddColumn(col("email", "VARCHAR(255)", true, false, false))];
    let sql = d.build_alter_table("users", &changes);
    assert!(sql.contains("ALTER TABLE"));
    assert!(sql.contains("ADD"));
}

#[test]
fn test_build_alter_table_drop_column() {
    let d = dialect(DbType::MySQL);
    let changes = vec![TableChange::DropColumn("old_col".into())];
    let sql = d.build_alter_table("users", &changes);
    assert!(sql.contains("ALTER TABLE"));
    assert!(sql.contains("DROP"));
}

#[test]
fn test_build_alter_table_add_index() {
    let d = dialect(DbType::MySQL);
    let changes = vec![TableChange::AddIndex("idx_email".into(), vec!["email".into()])];
    let sql = d.build_alter_table("users", &changes);
    assert!(sql.contains("INDEX") || sql.contains("index"));
}

#[test]
fn test_build_alter_table_drop_index() {
    let d = dialect(DbType::MySQL);
    let changes = vec![TableChange::DropIndex("idx_old".into())];
    let sql = d.build_alter_table("users", &changes);
    assert!(sql.contains("DROP"));
}

#[test]
fn test_build_alter_table_add_foreign_key() {
    let d = dialect(DbType::MySQL);
    let changes = vec![TableChange::AddForeignKey {
        columns: vec!["user_id".into()],
        reference_table: "users".into(),
        reference_columns: vec!["id".into()],
    }];
    let sql = d.build_alter_table("orders", &changes);
    assert!(sql.contains("FOREIGN KEY") || sql.contains("REFERENCES"));
}

#[test]
fn test_build_alter_table_modify_column() {
    let d = dialect(DbType::MySQL);
    let changes = vec![TableChange::ModifyColumn(col("name", "VARCHAR(500)", false, false, false))];
    let sql = d.build_alter_table("users", &changes);
    assert!(sql.contains("ALTER TABLE"));
}

#[test]
fn test_get_dialect_all_types() {
    assert!(get_dialect(DbType::MySQL).is_ok());
    assert!(get_dialect(DbType::PostgreSQL).is_ok());
    assert!(get_dialect(DbType::Sqlite).is_ok());
    assert!(get_dialect(DbType::Oracle).is_ok());
    assert!(get_dialect(DbType::SqlServer).is_ok());
}

#[test]
fn test_quote_into_mysql() {
    let d = dialect(DbType::MySQL);
    let mut buf = String::new();
    d.quote_into("col", &mut buf);
    assert_eq!(buf, "`col`");
}

#[test]
fn test_quote_into_pg() {
    let d = dialect(DbType::PostgreSQL);
    let mut buf = String::new();
    d.quote_into("col", &mut buf);
    assert_eq!(buf, "\"col\"");
}

#[test]
fn test_quote_checked_valid() {
    let d = dialect(DbType::MySQL);
    assert!(d.quote_checked("valid_col").is_ok());
}

#[test]
fn test_quote_checked_empty_returns_err() {
    let d = dialect(DbType::MySQL);
    assert!(d.quote_checked("").is_err());
}

#[test]
fn test_quote_checked_with_semicolon_returns_err() {
    let d = dialect(DbType::MySQL);
    assert!(d.quote_checked("col; DROP").is_err());
}

#[test]
fn test_column_def_with_default() {
    let d = dialect(DbType::MySQL);
    let cols = vec![ColumnDef {
        name: "status".into(),
        sql_type: "VARCHAR(20)".into(),
        nullable: false,
        default: Some("active".into()),
        auto_increment: false,
        primary_key: false,
    }];
    let sql = d.build_create_table("t", &cols);
    assert!(sql.contains("DEFAULT"));
}

#[test]
fn test_oracle_dialect_quote() {
    let d = dialect(DbType::Oracle);
    let quoted = d.quote("col");
    assert!(!quoted.is_empty());
}

#[test]
fn test_mssql_dialect_quote() {
    let d = dialect(DbType::SqlServer);
    let quoted = d.quote("col");
    assert!(!quoted.is_empty());
}

#[test]
fn test_oracle_supports_returning() {
    let d = dialect(DbType::Oracle);
    assert!(d.supports_returning());
}

#[test]
fn test_mssql_auto_increment() {
    let d = dialect(DbType::SqlServer);
    assert!(!d.auto_increment_keyword().is_empty());
}

#[test]
fn test_clone_box() {
    let d = dialect(DbType::MySQL);
    let cloned = d.clone_box();
    assert_eq!(cloned.db_type(), DbType::MySQL);
}

#[test]
fn test_build_pagination_sqlite() {
    let d = dialect(DbType::Sqlite);
    let sql = d.build_pagination("SELECT * FROM t", 1, 5);
    assert!(sql.contains("LIMIT"));
}

#[test]
fn test_build_pagination_oracle() {
    let d = dialect(DbType::Oracle);
    let sql = d.build_pagination("SELECT * FROM t", 1, 5);
    assert!(!sql.is_empty());
}

#[test]
fn test_json_extract_pg() {
    let d = dialect(DbType::PostgreSQL);
    let sql = d.json_extract("data", "name");
    assert!(!sql.is_empty());
}

#[test]
fn test_full_text_search_pg() {
    let d = dialect(DbType::PostgreSQL);
    let sql = d.full_text_search(&["body"], "search");
    assert!(!sql.is_empty());
}

#[test]
fn test_concat_pg() {
    let d = dialect(DbType::PostgreSQL);
    let sql = d.concat(&["a", "b"]);
    assert!(!sql.is_empty());
}

#[test]
fn test_escape_string_with_backslash() {
    let d = dialect(DbType::MySQL);
    let escaped = d.escape_string("a\\b");
    assert!(!escaped.is_empty());
}

#[test]
fn test_build_create_table_with_nullable() {
    let d = dialect(DbType::PostgreSQL);
    let cols = vec![
        col("id", "SERIAL", false, true, true),
        col("bio", "TEXT", true, false, false),
    ];
    let sql = d.build_create_table("users", &cols);
    assert!(sql.contains("NULL"));
}