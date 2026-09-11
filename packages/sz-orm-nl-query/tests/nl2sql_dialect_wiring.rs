//! W2-2 AI-NL2SQL-02 接线验证：NL2SQL 方言适配
//!
//! 同一自然语言问句，目标方言分别设为 PostgreSQL/MSSQL/Oracle/MySQL/SQLite，
//! 断言生成 SQL 使用对应方言语法。

use sz_orm_ai::nl2sql::{ColumnInfo, SchemaContext, SimpleNl2SqlEngine, SqlDialect, TableInfo};
use sz_orm_nl_query::dialect_renderer::DialectAwareNl2SqlRenderer;

fn make_schema() -> SchemaContext {
    SchemaContext {
        tables: vec![TableInfo {
            name: "users".to_string(),
            columns: vec![
                ColumnInfo {
                    name: "id".to_string(),
                    data_type: "INTEGER".to_string(),
                    nullable: false,
                    is_primary_key: true,
                },
                ColumnInfo {
                    name: "name".to_string(),
                    data_type: "TEXT".to_string(),
                    nullable: true,
                    is_primary_key: false,
                },
                ColumnInfo {
                    name: "age".to_string(),
                    data_type: "INTEGER".to_string(),
                    nullable: true,
                    is_primary_key: false,
                },
            ],
        }],
    }
}

#[tokio::test]
async fn wiring_dialect_mysql_uses_limit() {
    let renderer = DialectAwareNl2SqlRenderer::new(SimpleNl2SqlEngine::new());
    let schema = make_schema();
    let result = renderer
        .render("show top 10 users", &schema, SqlDialect::MySQL)
        .await
        .unwrap();
    assert!(
        result.sql.contains("LIMIT"),
        "MySQL SQL should contain LIMIT: {}",
        result.sql
    );
    assert!(
        !result.sql.contains("FETCH"),
        "MySQL SQL should not contain FETCH: {}",
        result.sql
    );
    assert!(
        !result.sql.contains("ROWNUM"),
        "MySQL SQL should not contain ROWNUM: {}",
        result.sql
    );
    assert!(
        !result.sql.contains("TOP"),
        "MySQL SQL should not contain TOP: {}",
        result.sql
    );
}

#[tokio::test]
async fn wiring_dialect_sqlite_uses_limit() {
    let renderer = DialectAwareNl2SqlRenderer::new(SimpleNl2SqlEngine::new());
    let schema = make_schema();
    let result = renderer
        .render("show top 10 users", &schema, SqlDialect::Sqlite)
        .await
        .unwrap();
    assert!(
        result.sql.contains("LIMIT"),
        "SQLite SQL should contain LIMIT: {}",
        result.sql
    );
    assert!(
        !result.sql.contains("FETCH"),
        "SQLite SQL should not contain FETCH: {}",
        result.sql
    );
}

#[tokio::test]
async fn wiring_dialect_postgresql_uses_fetch_first() {
    let renderer = DialectAwareNl2SqlRenderer::new(SimpleNl2SqlEngine::new());
    let schema = make_schema();
    let result = renderer
        .render("show top 10 users", &schema, SqlDialect::PostgreSQL)
        .await
        .unwrap();
    assert!(
        result.sql.contains("FETCH FIRST"),
        "PostgreSQL SQL should contain FETCH FIRST: {}",
        result.sql
    );
    assert!(
        result.sql.contains("ROWS ONLY"),
        "PostgreSQL SQL should contain ROWS ONLY: {}",
        result.sql
    );
    assert!(
        !result.sql.contains("LIMIT"),
        "PostgreSQL SQL should not contain LIMIT: {}",
        result.sql
    );
}

#[tokio::test]
async fn wiring_dialect_oracle_uses_rownum() {
    let renderer = DialectAwareNl2SqlRenderer::new(SimpleNl2SqlEngine::new());
    let schema = make_schema();
    let result = renderer
        .render("show top 10 users", &schema, SqlDialect::Oracle)
        .await
        .unwrap();
    assert!(
        result.sql.contains("ROWNUM"),
        "Oracle SQL should contain ROWNUM: {}",
        result.sql
    );
    assert!(
        !result.sql.contains("LIMIT"),
        "Oracle SQL should not contain LIMIT: {}",
        result.sql
    );
    assert!(
        !result.sql.contains("FETCH FIRST"),
        "Oracle SQL should not contain FETCH FIRST: {}",
        result.sql
    );
}

#[tokio::test]
async fn wiring_dialect_sqlserver_uses_top() {
    let renderer = DialectAwareNl2SqlRenderer::new(SimpleNl2SqlEngine::new());
    let schema = make_schema();
    let result = renderer
        .render("show top 10 users", &schema, SqlDialect::SqlServer)
        .await
        .unwrap();
    assert!(
        result.sql.contains("TOP"),
        "SQL Server SQL should contain TOP: {}",
        result.sql
    );
    assert!(
        !result.sql.contains("LIMIT"),
        "SQL Server SQL should not contain LIMIT: {}",
        result.sql
    );
    assert!(
        !result.sql.contains("ROWNUM"),
        "SQL Server SQL should not contain ROWNUM: {}",
        result.sql
    );
}

#[tokio::test]
async fn wiring_dialect_all_five_produce_distinct_sql() {
    let renderer = DialectAwareNl2SqlRenderer::new(SimpleNl2SqlEngine::new());
    let schema = make_schema();
    let nl = "show top 10 users";

    let pg = renderer
        .render(nl, &schema, SqlDialect::PostgreSQL)
        .await
        .unwrap();
    let oracle = renderer
        .render(nl, &schema, SqlDialect::Oracle)
        .await
        .unwrap();
    let mssql = renderer
        .render(nl, &schema, SqlDialect::SqlServer)
        .await
        .unwrap();

    assert_ne!(
        pg.sql, oracle.sql,
        "PostgreSQL and Oracle produced same SQL: {}",
        pg.sql
    );
    assert_ne!(
        pg.sql, mssql.sql,
        "PostgreSQL and SQL Server produced same SQL: {}",
        pg.sql
    );
    assert_ne!(
        oracle.sql, mssql.sql,
        "Oracle and SQL Server produced same SQL: {}",
        oracle.sql
    );
}

#[tokio::test]
async fn wiring_dialect_oracle_rownum_with_where_clause() {
    let renderer = DialectAwareNl2SqlRenderer::new(SimpleNl2SqlEngine::new());
    let schema = make_schema();
    let result = renderer
        .render(
            "show top 10 users where age > 18",
            &schema,
            SqlDialect::Oracle,
        )
        .await
        .unwrap();
    assert!(
        result.sql.contains("ROWNUM"),
        "Oracle SQL should contain ROWNUM: {}",
        result.sql
    );
    assert!(
        result.sql.contains("age"),
        "Oracle SQL should contain age column: {}",
        result.sql
    );
}

#[tokio::test]
async fn wiring_dialect_sqlserver_top_preserves_columns() {
    let renderer = DialectAwareNl2SqlRenderer::new(SimpleNl2SqlEngine::new());
    let schema = make_schema();
    let result = renderer
        .render("show top 10 users", &schema, SqlDialect::SqlServer)
        .await
        .unwrap();
    assert!(
        result.sql.contains("TOP"),
        "SQL Server SQL should contain TOP: {}",
        result.sql
    );
    assert!(
        result.sql.contains("users"),
        "SQL Server SQL should contain table name: {}",
        result.sql
    );
}
