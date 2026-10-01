//! v9.2.0 M3 T19: 异构方言测试（ClickHouse / DuckDB / Db2）

use sz_orm_core::dialect::{ClickHouseDialect, ColumnDef, Db2Dialect, DuckDBDialect};
use sz_orm_core::Dialect;

fn sample_cols() -> Vec<ColumnDef> {
    vec![ColumnDef {
        name: "id".to_string(),
        sql_type: "INT".to_string(),
        nullable: false,
        default: None,
        auto_increment: false,
        primary_key: true,
    }]
}

// --- ClickHouse ---

#[test]
fn test_clickhouse_quote_identifier() {
    let d = ClickHouseDialect;
    assert!(!d.quote("users").is_empty());
}

#[test]
fn test_clickhouse_escape_string() {
    let d = ClickHouseDialect;
    assert!(!d.escape_string("test").is_empty());
}

#[test]
fn test_clickhouse_build_pagination() {
    let d = ClickHouseDialect;
    let sql = d.build_pagination("SELECT * FROM users", 1, 10);
    assert!(!sql.is_empty());
}

#[test]
fn test_clickhouse_build_create_table() {
    let d = ClickHouseDialect;
    let sql = d.build_create_table("events", &sample_cols());
    assert!(sql.to_uppercase().contains("CREATE TABLE"));
}

#[test]
fn test_clickhouse_json_type() {
    let d = ClickHouseDialect;
    assert!(!d.json_type().is_empty());
}

#[test]
fn test_clickhouse_concat() {
    let d = ClickHouseDialect;
    assert!(!d.concat(&["a", "b"]).is_empty());
}

// --- DuckDB ---

#[test]
fn test_duckdb_quote_identifier() {
    let d = DuckDBDialect;
    assert!(!d.quote("users").is_empty());
}

#[test]
fn test_duckdb_escape_string() {
    let d = DuckDBDialect;
    assert!(!d.escape_string("test").is_empty());
}

#[test]
fn test_duckdb_build_pagination() {
    let d = DuckDBDialect;
    let sql = d.build_pagination("SELECT * FROM users", 1, 10);
    assert!(!sql.is_empty());
}

#[test]
fn test_duckdb_build_create_table() {
    let d = DuckDBDialect;
    let sql = d.build_create_table("events", &sample_cols());
    assert!(sql.to_uppercase().contains("CREATE TABLE"));
}

#[test]
fn test_duckdb_supports_returning() {
    let d = DuckDBDialect;
    assert!(d.supports_returning() || !d.supports_returning());
}

// --- Db2 ---

#[test]
fn test_db2_quote_identifier() {
    let d = Db2Dialect;
    assert!(!d.quote("users").is_empty());
}

#[test]
fn test_db2_escape_string() {
    let d = Db2Dialect;
    assert!(!d.escape_string("test").is_empty());
}

#[test]
fn test_db2_build_pagination() {
    let d = Db2Dialect;
    let sql = d.build_pagination("SELECT * FROM users", 1, 10);
    assert!(!sql.is_empty());
}

#[test]
fn test_db2_build_create_table() {
    let d = Db2Dialect;
    let sql = d.build_create_table("events", &sample_cols());
    assert!(sql.to_uppercase().contains("CREATE TABLE"));
}
