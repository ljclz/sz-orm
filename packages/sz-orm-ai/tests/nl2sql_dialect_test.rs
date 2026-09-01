//! NL2SQL 方言感知生成测试
//!
//! 验证 5 种方言的分页查询生成正确语法的 SQL。

#![cfg(feature = "ai-nl2sql-enhanced")]

use sz_orm_ai::nl2sql::{
    ColumnInfo, Nl2SqlEngine, SchemaContext, SimpleNl2SqlEngine, SqlDialect, TableInfo,
};

fn test_schema() -> SchemaContext {
    SchemaContext {
        tables: vec![TableInfo {
            name: "users".into(),
            columns: vec![
                ColumnInfo {
                    name: "id".into(),
                    data_type: "INTEGER".into(),
                    nullable: false,
                    is_primary_key: true,
                },
                ColumnInfo {
                    name: "name".into(),
                    data_type: "TEXT".into(),
                    nullable: true,
                    is_primary_key: false,
                },
                ColumnInfo {
                    name: "age".into(),
                    data_type: "INTEGER".into(),
                    nullable: true,
                    is_primary_key: false,
                },
            ],
        }],
    }
}

#[tokio::test]
async fn test_dialect_mysql_limit() {
    let engine = SimpleNl2SqlEngine::new();
    let schema = test_schema();
    let result = engine
        .generate_with_dialect("show all users limit 10", &schema, SqlDialect::MySQL)
        .await
        .unwrap();
    assert_eq!(result.dialect, Some(SqlDialect::MySQL));
    assert!(
        result.sql.contains("LIMIT"),
        "MySQL should use LIMIT: {}",
        result.sql
    );
}

#[tokio::test]
async fn test_dialect_postgresql_limit() {
    let engine = SimpleNl2SqlEngine::new();
    let schema = test_schema();
    let result = engine
        .generate_with_dialect("show all users limit 10", &schema, SqlDialect::PostgreSQL)
        .await
        .unwrap();
    assert_eq!(result.dialect, Some(SqlDialect::PostgreSQL));
    assert!(
        result.sql.contains("LIMIT"),
        "PG should use LIMIT: {}",
        result.sql
    );
}

#[tokio::test]
async fn test_dialect_sqlite_limit() {
    let engine = SimpleNl2SqlEngine::new();
    let schema = test_schema();
    let result = engine
        .generate_with_dialect("show all users limit 10", &schema, SqlDialect::Sqlite)
        .await
        .unwrap();
    assert_eq!(result.dialect, Some(SqlDialect::Sqlite));
    assert!(
        result.sql.contains("LIMIT"),
        "SQLite should use LIMIT: {}",
        result.sql
    );
}

#[tokio::test]
async fn test_dialect_oracle_offset_fetch() {
    let engine = SimpleNl2SqlEngine::new();
    let schema = test_schema();
    let result = engine
        .generate_with_dialect("show all users limit 10", &schema, SqlDialect::Oracle)
        .await
        .unwrap();
    assert_eq!(result.dialect, Some(SqlDialect::Oracle));
    assert!(
        result.sql.contains("FETCH NEXT") || !result.sql.contains("LIMIT"),
        "Oracle should use FETCH NEXT or no LIMIT: {}",
        result.sql
    );
}

#[tokio::test]
async fn test_dialect_mssql_offset_fetch() {
    let engine = SimpleNl2SqlEngine::new();
    let schema = test_schema();
    let result = engine
        .generate_with_dialect("show all users limit 10", &schema, SqlDialect::SqlServer)
        .await
        .unwrap();
    assert_eq!(result.dialect, Some(SqlDialect::SqlServer));
    assert!(
        result.sql.contains("FETCH NEXT") || !result.sql.contains("LIMIT"),
        "SQL Server should use FETCH NEXT or no LIMIT: {}",
        result.sql
    );
}

#[tokio::test]
async fn test_dialect_default_uses_generate() {
    let engine = SimpleNl2SqlEngine::new();
    let schema = test_schema();
    let result_default = engine.generate("show all users", &schema).await.unwrap();
    assert_eq!(result_default.dialect, None);

    let result_dialect = engine
        .generate_with_dialect("show all users", &schema, SqlDialect::MySQL)
        .await
        .unwrap();
    assert_eq!(result_dialect.dialect, Some(SqlDialect::MySQL));
}

#[tokio::test]
async fn test_limit_clause_dialects() {
    assert_eq!(SqlDialect::MySQL.limit_clause(10, 0), "LIMIT 10");
    assert_eq!(SqlDialect::MySQL.limit_clause(10, 20), "LIMIT 10 OFFSET 20");
    assert_eq!(SqlDialect::PostgreSQL.limit_clause(10, 0), "LIMIT 10");
    assert_eq!(
        SqlDialect::PostgreSQL.limit_clause(10, 20),
        "LIMIT 10 OFFSET 20"
    );
    assert_eq!(SqlDialect::Sqlite.limit_clause(10, 0), "LIMIT 10");
    assert_eq!(
        SqlDialect::Sqlite.limit_clause(10, 20),
        "LIMIT 10 OFFSET 20"
    );
    assert_eq!(
        SqlDialect::Oracle.limit_clause(10, 0),
        "OFFSET 0 ROWS FETCH NEXT 10 ROWS ONLY"
    );
    assert_eq!(
        SqlDialect::Oracle.limit_clause(10, 20),
        "OFFSET 20 ROWS FETCH NEXT 10 ROWS ONLY"
    );
    assert_eq!(
        SqlDialect::SqlServer.limit_clause(10, 0),
        "OFFSET 0 ROWS FETCH NEXT 10 ROWS ONLY"
    );
}

#[tokio::test]
async fn test_param_placeholder_dialects() {
    assert_eq!(SqlDialect::MySQL.param_placeholder(1), "?");
    assert_eq!(SqlDialect::MySQL.param_placeholder(2), "?");
    assert_eq!(SqlDialect::PostgreSQL.param_placeholder(1), "$1");
    assert_eq!(SqlDialect::PostgreSQL.param_placeholder(2), "$2");
    assert_eq!(SqlDialect::Oracle.param_placeholder(1), ":1");
    assert_eq!(SqlDialect::Oracle.param_placeholder(2), ":2");
    assert_eq!(SqlDialect::SqlServer.param_placeholder(1), "@p1");
    assert_eq!(SqlDialect::SqlServer.param_placeholder(2), "@p2");
}
