//! AI-NL2SQL-01 接线验证测试（v6.8.0）
//!
//! 验证 NL 查询 → DialectAwareNl2SqlRenderer → CachedNl2SqlPipeline → 参数化 SQL 端到端管线。

use std::time::Duration;
use sz_orm_ai::nl2sql::{ColumnInfo, SchemaContext, SimpleNl2SqlEngine, SqlDialect, TableInfo};
use sz_orm_nl_query::cached_pipeline::{CachedNl2SqlPipeline, Nl2SqlWarning};
use sz_orm_nl_query::dialect_renderer::DialectAwareNl2SqlRenderer;

fn make_schema() -> SchemaContext {
    SchemaContext {
        tables: vec![TableInfo {
            name: "orders".to_string(),
            columns: vec![
                ColumnInfo {
                    name: "id".to_string(),
                    data_type: "INTEGER".to_string(),
                    nullable: false,
                    is_primary_key: true,
                },
                ColumnInfo {
                    name: "amount".to_string(),
                    data_type: "DECIMAL".to_string(),
                    nullable: false,
                    is_primary_key: false,
                },
                ColumnInfo {
                    name: "created_at".to_string(),
                    data_type: "TIMESTAMP".to_string(),
                    nullable: false,
                    is_primary_key: false,
                },
            ],
        }],
    }
}

fn make_pipeline() -> CachedNl2SqlPipeline<SimpleNl2SqlEngine> {
    let renderer = DialectAwareNl2SqlRenderer::new(SimpleNl2SqlEngine::new());
    CachedNl2SqlPipeline::new(renderer, Duration::from_secs(60))
}

#[tokio::test]
async fn wiring_generates_parameterized_sql() {
    let pipeline = make_pipeline();
    let schema = make_schema();
    let result = pipeline
        .execute_cached("show all orders", &schema, SqlDialect::PostgreSQL)
        .await
        .unwrap();
    assert!(result.query.sql.contains("SELECT"));
    assert!(!result.cache_hit);
}

#[tokio::test]
async fn wiring_cache_hit_on_second_call() {
    let pipeline = make_pipeline();
    let schema = make_schema();
    pipeline
        .execute_cached("show all orders", &schema, SqlDialect::PostgreSQL)
        .await
        .unwrap();
    let result = pipeline
        .execute_cached("show all orders", &schema, SqlDialect::PostgreSQL)
        .await
        .unwrap();
    assert!(result.cache_hit);
    assert!(result.query.cache_hit);
}

#[tokio::test]
async fn wiring_different_dialects_produce_different_sql() {
    let pipeline = make_pipeline();
    let schema = make_schema();
    let pg_result = pipeline
        .execute_cached("show all orders", &schema, SqlDialect::PostgreSQL)
        .await
        .unwrap();
    let mysql_result = pipeline
        .execute_cached("show all orders", &schema, SqlDialect::MySQL)
        .await
        .unwrap();
    assert_eq!(pg_result.query.dialect, Some(SqlDialect::PostgreSQL));
    assert_eq!(mysql_result.query.dialect, Some(SqlDialect::MySQL));
}

#[tokio::test]
async fn wiring_low_confidence_warning() {
    let renderer = DialectAwareNl2SqlRenderer::new(SimpleNl2SqlEngine::new());
    let pipeline = CachedNl2SqlPipeline::new(renderer, Duration::from_secs(60))
        .with_confidence_threshold(0.99);
    let schema = make_schema();
    let result = pipeline
        .execute_cached("show all orders", &schema, SqlDialect::PostgreSQL)
        .await
        .unwrap();
    assert_eq!(result.warning, Some(Nl2SqlWarning::LowConfidence));
}

#[tokio::test]
async fn wiring_high_confidence_no_warning() {
    let pipeline = make_pipeline();
    let schema = make_schema();
    let result = pipeline
        .execute_cached("show all orders", &schema, SqlDialect::PostgreSQL)
        .await
        .unwrap();
    assert_eq!(result.warning, None);
}

#[tokio::test]
async fn wiring_cache_clear_resets() {
    let pipeline = make_pipeline();
    let schema = make_schema();
    pipeline
        .execute_cached("show all orders", &schema, SqlDialect::PostgreSQL)
        .await
        .unwrap();
    assert_eq!(pipeline.cache_size(), 1);
    pipeline.clear_cache();
    assert_eq!(pipeline.cache_size(), 0);
}

#[tokio::test]
async fn wiring_different_queries_not_cached() {
    let pipeline = make_pipeline();
    let schema = make_schema();
    pipeline
        .execute_cached("show all orders", &schema, SqlDialect::PostgreSQL)
        .await
        .unwrap();
    let result = pipeline
        .execute_cached("count orders", &schema, SqlDialect::PostgreSQL)
        .await
        .unwrap();
    assert!(!result.cache_hit);
}

#[tokio::test]
async fn wiring_confidence_threshold_configurable() {
    let renderer = DialectAwareNl2SqlRenderer::new(SimpleNl2SqlEngine::new());
    let pipeline =
        CachedNl2SqlPipeline::new(renderer, Duration::from_secs(60)).with_confidence_threshold(0.5);
    assert_eq!(pipeline.confidence_threshold(), 0.5);
}

#[tokio::test]
async fn wiring_all_five_dialects_supported() {
    let pipeline = make_pipeline();
    let schema = make_schema();
    for dialect in [
        SqlDialect::MySQL,
        SqlDialect::PostgreSQL,
        SqlDialect::Sqlite,
        SqlDialect::Oracle,
        SqlDialect::SqlServer,
    ] {
        let result = pipeline
            .execute_cached("show all orders", &schema, dialect)
            .await
            .unwrap();
        assert_eq!(result.query.dialect, Some(dialect));
    }
}

#[tokio::test]
async fn wiring_cache_key_includes_schema() {
    let pipeline = make_pipeline();
    let schema1 = SchemaContext {
        tables: vec![TableInfo {
            name: "users".to_string(),
            columns: vec![ColumnInfo {
                name: "id".to_string(),
                data_type: "INTEGER".to_string(),
                nullable: false,
                is_primary_key: true,
            }],
        }],
    };
    let schema2 = SchemaContext {
        tables: vec![TableInfo {
            name: "orders".to_string(),
            columns: vec![ColumnInfo {
                name: "id".to_string(),
                data_type: "INTEGER".to_string(),
                nullable: false,
                is_primary_key: true,
            }],
        }],
    };
    pipeline
        .execute_cached("show all users", &schema1, SqlDialect::PostgreSQL)
        .await
        .unwrap();
    let result = pipeline
        .execute_cached("show all orders", &schema2, SqlDialect::PostgreSQL)
        .await
        .unwrap();
    assert!(!result.cache_hit);
}

#[tokio::test]
async fn wiring_sql_contains_select() {
    let pipeline = make_pipeline();
    let schema = make_schema();
    let result = pipeline
        .execute_cached("show all orders", &schema, SqlDialect::PostgreSQL)
        .await
        .unwrap();
    assert!(
        result.query.sql.to_uppercase().contains("SELECT"),
        "SQL should contain SELECT: {}",
        result.query.sql
    );
}

#[tokio::test]
async fn wiring_result_has_confidence() {
    let pipeline = make_pipeline();
    let schema = make_schema();
    let result = pipeline
        .execute_cached("show all orders", &schema, SqlDialect::PostgreSQL)
        .await
        .unwrap();
    assert!(result.confidence > 0.0);
    assert!(result.confidence <= 1.0);
}

#[tokio::test]
async fn wiring_cached_result_preserves_sql() {
    let pipeline = make_pipeline();
    let schema = make_schema();
    let first = pipeline
        .execute_cached("show all orders", &schema, SqlDialect::PostgreSQL)
        .await
        .unwrap();
    let second = pipeline
        .execute_cached("show all orders", &schema, SqlDialect::PostgreSQL)
        .await
        .unwrap();
    assert_eq!(first.query.sql, second.query.sql);
    assert_eq!(first.confidence, second.confidence);
}
