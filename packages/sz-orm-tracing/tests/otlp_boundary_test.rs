//! OtlpExporter 边界与极端场景测试（v7.5.0 组7.1）
//!
//! 验证 BoundedSpanQueue / OtlpExporter / SpanBuilder 在空队列、满队列、最大容量等边界条件下的行为。

#![cfg(feature = "otlp-export")]

use sz_orm_tracing::otlp_exporter::{BoundedSpanQueue, OtlpExporter, SpanBuilder, SpanName};

#[test]
fn test_bounded_queue_zero_capacity() {
    let queue = BoundedSpanQueue::new(0);
    assert_eq!(queue.len(), 0);
    assert!(queue.is_empty());
}

#[test]
fn test_bounded_queue_one_capacity() {
    let queue = BoundedSpanQueue::new(1);
    assert_eq!(queue.len(), 0);
    assert!(queue.is_empty());
}

#[test]
fn test_bounded_queue_empty() {
    let queue = BoundedSpanQueue::new(10);
    assert!(queue.is_empty());
}

#[test]
fn test_span_builder_root() {
    let span = SpanBuilder::root(SpanName::QueryExecution).build();
    assert_eq!(span.span_name, "query_execution");
}

#[test]
fn test_span_builder_with_attribute() {
    let span = SpanBuilder::root(SpanName::SqlExecute)
        .attribute("key", "value")
        .build();
    assert_eq!(span.span_name, "sql_execute");
    assert!(span.attributes.contains_key("key"));
    assert_eq!(span.attributes.get("key").unwrap(), "value");
}

#[test]
fn test_otlp_exporter_empty_export() {
    let result = OtlpExporter::export(&[]);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 0);
}

#[test]
fn test_otlp_exporter_validate_empty() {
    let result = OtlpExporter::validate_hierarchy(&[]);
    assert!(result.is_ok());
}

#[test]
fn test_otlp_exporter_export_single_span() {
    let span = SpanBuilder::root(SpanName::QueryExecution).build();
    let result = OtlpExporter::export(&[span]);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 1);
}

#[test]
fn test_otlp_exporter_export_multiple_spans() {
    let spans = vec![
        SpanBuilder::root(SpanName::QueryExecution).build(),
        SpanBuilder::root(SpanName::ConnectionAcquire).build(),
        SpanBuilder::root(SpanName::SqlExecute).build(),
    ];
    let result = OtlpExporter::export(&spans);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 3);
}

#[test]
fn test_span_name_all_variants() {
    let names = [
        SpanName::QueryExecution,
        SpanName::ConnectionAcquire,
        SpanName::SqlExecute,
        SpanName::ResultMap,
        SpanName::CacheHit,
        SpanName::AiTuning,
    ];
    assert_eq!(names.len(), 6);
    for i in 0..names.len() {
        for j in (i + 1)..names.len() {
            assert_ne!(names[i], names[j]);
        }
    }
}
