//! v7.3.0 任务 2.6：OTLP db.* 属性测试
//!
//! 验证：span 属性 + 跨阶段关联（parent_id 链）+ 采样率 + 导出失败缓冲

use sz_orm_tracing::{
    db_attrs, span_for_cache_lookup, span_for_failover, span_for_query, OtlpExportBuffer, Span,
};

#[test]
fn test_db_attrs_constants() {
    assert_eq!(db_attrs::STATEMENT, "db.statement");
    assert_eq!(db_attrs::CONNECTION_ID, "db.connection_id");
    assert_eq!(db_attrs::CACHE_HIT, "db.cache_hit");
    assert_eq!(db_attrs::FAILOVER, "db.failover");
}

#[test]
fn test_span_for_query_sets_db_attrs() {
    let parent = Span::new("trace-001", "span-parent", "parent_op");
    let span = span_for_query(&parent, "SELECT * FROM users", "conn-42");

    assert_eq!(span.trace_id, "trace-001");
    assert_ne!(span.span_id, "span-parent");
    assert_eq!(span.parent_id, Some("span-parent".to_string()));
    assert_eq!(
        span.tags.get(db_attrs::STATEMENT),
        Some(&"SELECT * FROM users".to_string())
    );
    assert_eq!(
        span.tags.get(db_attrs::CONNECTION_ID),
        Some(&"conn-42".to_string())
    );
}

#[test]
fn test_span_for_cache_lookup() {
    let parent = Span::new("trace-002", "span-parent", "parent_op");
    let span = span_for_cache_lookup(&parent, "user:123");

    assert_eq!(span.trace_id, "trace-002");
    assert_eq!(span.parent_id, Some("span-parent".to_string()));
    assert_eq!(span.tags.get("db.cache.key"), Some(&"user:123".to_string()));
}

#[test]
fn test_span_for_failover() {
    let parent = Span::new("trace-003", "span-parent", "parent_op");
    let span = span_for_failover(&parent, "primary->replica");

    assert_eq!(span.trace_id, "trace-003");
    assert_eq!(span.parent_id, Some("span-parent".to_string()));
    assert_eq!(
        span.tags.get(db_attrs::FAILOVER),
        Some(&"primary->replica".to_string())
    );
}

#[test]
fn test_cross_stage_span_chain() {
    // 模拟跨阶段 span 链：root → query → cache_lookup
    let root = Span::new("trace-chain", "span-root", "request");
    let query = span_for_query(&root, "SELECT 1", "conn-1");
    let cache = span_for_cache_lookup(&query, "result:hash");

    // trace_id 应一致
    assert_eq!(root.trace_id, "trace-chain");
    assert_eq!(query.trace_id, "trace-chain");
    assert_eq!(cache.trace_id, "trace-chain");

    // parent_id 链应正确
    assert_eq!(query.parent_id, Some("span-root".to_string()));
    assert_eq!(cache.parent_id, Some(query.span_id.clone()));
}

#[test]
fn test_otlp_export_buffer_failed_export() {
    let buffer = OtlpExportBuffer::new(3);
    let span = Span::new("trace-1", "span-1", "op");

    // 导出失败，应缓冲
    buffer.try_export(span.clone(), false);
    assert_eq!(buffer.len(), 1);
    assert_eq!(buffer.export_failed_count(), 1);
}

#[test]
fn test_otlp_export_buffer_drops_oldest_when_full() {
    let buffer = OtlpExportBuffer::new(2);

    buffer.try_export(Span::new("t1", "s1", "op"), false);
    buffer.try_export(Span::new("t2", "s2", "op"), false);
    // 缓冲满，丢弃最旧
    buffer.try_export(Span::new("t3", "s3", "op"), false);

    assert_eq!(buffer.len(), 2);
    assert_eq!(buffer.dropped_count(), 1);
}

#[test]
fn test_otlp_export_buffer_successful_export_no_buffer() {
    let buffer = OtlpExportBuffer::new(10);
    let span = Span::new("trace-1", "span-1", "op");

    // 导出成功，不应缓冲
    buffer.try_export(span, true);
    assert!(buffer.is_empty());
    assert_eq!(buffer.export_failed_count(), 0);
}
