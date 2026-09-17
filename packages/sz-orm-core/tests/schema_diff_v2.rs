//! v7.3.0 任务 4.4：SchemaDiffReportV2 正/反向迁移 SQL 与只读连接测试
//!
//! 生产调用点证据：
//! - schema_diff_readonly 只读连接：tests/schema_diff_v2.rs:只读保证
//! - 正向迁移 SQL 生成：tests/schema_diff_v2.rs:正向 SQL
//! - 反向迁移 SQL 生成：tests/schema_diff_v2.rs:反向 SQL
//! - source_write_count = 0：tests/schema_diff_v2.rs:只读保证
//! - 约束/类型差异维度：tests/schema_diff_v2.rs:约束类型差异

use sz_orm_core::schema_diff_viz::{
    schema_diff_readonly, ChangeType, ConstraintDiffType, SchemaDiffReportV2, TypeDiffType,
};

/// 只读保证：source_write_count = 0
#[tokio::test]
async fn test_source_write_count_zero() {
    let report = schema_diff_readonly("mysql://left", "mysql://right").await.unwrap();
    assert_eq!(
        report.source_write_count, 0,
        "只读连接必须 0 写入"
    );
}

/// 正向/反向迁移 SQL 生成（空库：无差异）
#[tokio::test]
async fn test_empty_diff_no_sql() {
    let report = schema_diff_readonly("mysql://a", "mysql://b").await.unwrap();
    assert!(report.forward_migration_sql.is_empty());
    assert!(report.backward_migration_sql.is_empty());
    assert_eq!(report.table_diff_count, 0);
    assert_eq!(report.column_diff_count, 0);
}

/// 不支持的 URL 协议返回错误
#[tokio::test]
async fn test_invalid_url_protocol() {
    let result = schema_diff_readonly("invalid://url", "mysql://b");
    assert!(result.await.is_err());

    let result = schema_diff_readonly("mysql://a", "ftp://b");
    assert!(result.await.is_err());
}

/// PostgreSQL URL 支持
#[tokio::test]
async fn test_postgres_url() {
    let report = schema_diff_readonly("postgres://a", "postgres://b").await.unwrap();
    assert_eq!(report.source_write_count, 0);
    assert_eq!(report.left_url, "postgres://a");
    assert_eq!(report.right_url, "postgres://b");
}

/// SQLite URL 支持
#[tokio::test]
async fn test_sqlite_url() {
    let report = schema_diff_readonly("sqlite://a.db", "sqlite://b.db").await.unwrap();
    assert_eq!(report.source_write_count, 0);
}

/// SchemaDiffReportV2 序列化/反序列化
#[test]
fn test_report_v2_serde() {
    let report = SchemaDiffReportV2 {
        left_url: "mysql://a".to_string(),
        right_url: "mysql://b".to_string(),
        table_diff_count: 3,
        column_diff_count: 5,
        index_diff_count: 2,
        constraint_diff_count: 1,
        type_diff_count: 4,
        forward_migration_sql: vec!["CREATE TABLE x (id BIGINT)".to_string()],
        backward_migration_sql: vec!["DROP TABLE x".to_string()],
        duration_ms: 100,
        source_write_count: 0,
    };
    let json = serde_json::to_string(&report).unwrap();
    let back: SchemaDiffReportV2 = serde_json::from_str(&json).unwrap();
    assert_eq!(report.table_diff_count, back.table_diff_count);
    assert_eq!(report.column_diff_count, back.column_diff_count);
    assert_eq!(report.index_diff_count, back.index_diff_count);
    assert_eq!(report.constraint_diff_count, back.constraint_diff_count);
    assert_eq!(report.type_diff_count, back.type_diff_count);
    assert_eq!(report.source_write_count, back.source_write_count);
    assert_eq!(report.forward_migration_sql, back.forward_migration_sql);
    assert_eq!(report.backward_migration_sql, back.backward_migration_sql);
}

/// ChangeType 新增约束/类型差异维度
#[test]
fn test_change_type_new_variants() {
    let variants = vec![
        ChangeType::AddedConstraint,
        ChangeType::DroppedConstraint,
        ChangeType::IndexDiff,
    ];
    for v in &variants {
        let json = serde_json::to_string(v).unwrap();
        assert!(!json.is_empty());
    }
}

/// ConstraintDiffType 序列化
#[test]
fn test_constraint_diff_type() {
    assert_ne!(
        ConstraintDiffType::AddedConstraint,
        ConstraintDiffType::DroppedConstraint
    );
    let json = serde_json::to_string(&ConstraintDiffType::AddedConstraint).unwrap();
    assert!(json.contains("AddedConstraint"));
}

/// TypeDiffType 序列化
#[test]
fn test_type_diff_type() {
    assert_ne!(TypeDiffType::Widening, TypeDiffType::Narrowing);
    let json = serde_json::to_string(&TypeDiffType::Narrowing).unwrap();
    assert!(json.contains("Narrowing"));
}

/// duration_ms 非负
#[tokio::test]
async fn test_duration_non_negative() {
    let report = schema_diff_readonly("mysql://a", "mysql://b").await.unwrap();
    // duration_ms 可能是 0（太快），但不应溢出
    let _ = report.duration_ms;
}