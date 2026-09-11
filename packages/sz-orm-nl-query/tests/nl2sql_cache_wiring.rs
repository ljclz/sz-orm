//! W2-3 AI-NL2SQL-03 接线验证：NL2SQL 结果缓存联动
//!
//! 同一问句第二次提交 → LLM 调用计数不增加 + 返回与首次一致 SQL + cache_hit=true。

use std::time::Duration;

use sz_orm_ai::nl2sql::{ColumnInfo, SchemaContext, SimpleNl2SqlEngine, SqlDialect, TableInfo};
use sz_orm_nl_query::cached_pipeline::CachedNl2SqlPipeline;
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
            ],
        }],
    }
}

fn make_pipeline() -> CachedNl2SqlPipeline<SimpleNl2SqlEngine> {
    let renderer = DialectAwareNl2SqlRenderer::new(SimpleNl2SqlEngine::new());
    CachedNl2SqlPipeline::new(renderer, Duration::from_secs(60))
}

#[tokio::test]
async fn wiring_cache_second_call_no_llm_increase() {
    let pipeline = make_pipeline();
    let schema = make_schema();

    let first = pipeline
        .execute_cached("show all users", &schema, SqlDialect::PostgreSQL)
        .await
        .unwrap();
    assert!(!first.cache_hit);
    assert_eq!(pipeline.llm_call_count(), 1);

    let second = pipeline
        .execute_cached("show all users", &schema, SqlDialect::PostgreSQL)
        .await
        .unwrap();
    assert!(second.cache_hit);
    assert_eq!(pipeline.llm_call_count(), 1);
}

#[tokio::test]
async fn wiring_cache_returns_same_sql() {
    let pipeline = make_pipeline();
    let schema = make_schema();

    let first = pipeline
        .execute_cached("show all users", &schema, SqlDialect::MySQL)
        .await
        .unwrap();
    let second = pipeline
        .execute_cached("show all users", &schema, SqlDialect::MySQL)
        .await
        .unwrap();

    assert_eq!(first.query.sql, second.query.sql);
    assert!(second.cache_hit);
    assert!(second.query.cache_hit);
}

#[tokio::test]
async fn wiring_cache_different_dialects_separate_entries() {
    let pipeline = make_pipeline();
    let schema = make_schema();

    let pg = pipeline
        .execute_cached("show all users", &schema, SqlDialect::PostgreSQL)
        .await
        .unwrap();
    let mysql = pipeline
        .execute_cached("show all users", &schema, SqlDialect::MySQL)
        .await
        .unwrap();

    assert!(!pg.cache_hit);
    assert!(!mysql.cache_hit);
    assert_eq!(pipeline.llm_call_count(), 2);
    assert_eq!(pipeline.cache_size(), 2);
}

#[tokio::test]
async fn wiring_cache_different_queries_separate_entries() {
    let pipeline = make_pipeline();
    let schema = make_schema();

    pipeline
        .execute_cached("show all users", &schema, SqlDialect::PostgreSQL)
        .await
        .unwrap();
    pipeline
        .execute_cached("count users", &schema, SqlDialect::PostgreSQL)
        .await
        .unwrap();

    assert_eq!(pipeline.llm_call_count(), 2);
    assert_eq!(pipeline.cache_size(), 2);
}

#[tokio::test]
async fn wiring_cache_clear_resets_llm_count_state() {
    let pipeline = make_pipeline();
    let schema = make_schema();

    pipeline
        .execute_cached("show all users", &schema, SqlDialect::PostgreSQL)
        .await
        .unwrap();
    assert_eq!(pipeline.cache_size(), 1);
    pipeline.clear_cache();
    assert_eq!(pipeline.cache_size(), 0);

    let result = pipeline
        .execute_cached("show all users", &schema, SqlDialect::PostgreSQL)
        .await
        .unwrap();
    assert!(!result.cache_hit);
    assert_eq!(pipeline.llm_call_count(), 2);
}

#[tokio::test]
async fn wiring_cache_hit_flag_propagates_to_sql_query() {
    let pipeline = make_pipeline();
    let schema = make_schema();

    pipeline
        .execute_cached("show all users", &schema, SqlDialect::PostgreSQL)
        .await
        .unwrap();
    let second = pipeline
        .execute_cached("show all users", &schema, SqlDialect::PostgreSQL)
        .await
        .unwrap();

    assert!(second.cache_hit);
    assert!(second.query.cache_hit);
}
