#![cfg(feature = "otlp-export")]

use sz_orm_tracing::{BoundedSpanQueue, OtlpExporter, SpanBuilder, SpanName, SpanStatus};

#[test]
fn test_span_builder_root() {
    let span = SpanBuilder::root(SpanName::QueryExecution).build();
    assert!(span.parent_span_id.is_none());
    assert_eq!(span.span_name, "query_execution");
    assert!(matches!(span.status, SpanStatus::Ok));
}

#[test]
fn test_span_builder_child() {
    let root = SpanBuilder::root(SpanName::QueryExecution).build();
    let child = SpanBuilder::child(&root, SpanName::SqlExecute).build();
    assert_eq!(child.parent_span_id, Some(root.span_id.clone()));
    assert_eq!(child.trace_id, root.trace_id);
}

#[test]
fn test_span_builder_attribute() {
    let span = SpanBuilder::root(SpanName::QueryExecution)
        .attribute("db.system", "mysql")
        .attribute("db.statement", "SELECT")
        .build();
    assert_eq!(span.attributes.get("db.system"), Some(&"mysql".to_string()));
}

#[test]
fn test_otlp_exporter_export() {
    let root = SpanBuilder::root(SpanName::QueryExecution).build();
    let child = SpanBuilder::child(&root, SpanName::SqlExecute).build();
    let result = OtlpExporter::export(&[root, child]);
    assert_eq!(result.unwrap(), 2);
}

#[test]
fn test_otlp_exporter_validate_hierarchy() {
    let root = SpanBuilder::root(SpanName::QueryExecution).build();
    let child = SpanBuilder::child(&root, SpanName::SqlExecute).build();
    assert!(OtlpExporter::validate_hierarchy(&[root, child]).is_ok());
}

#[test]
fn test_otlp_exporter_broken_hierarchy() {
    let root = SpanBuilder::root(SpanName::QueryExecution).build();
    let mut child = SpanBuilder::child(&root, SpanName::SqlExecute).build();
    child.parent_span_id = Some("nonexistent".into());
    let result = OtlpExporter::validate_hierarchy(&[child]);
    assert!(result.is_err());
}

#[test]
fn test_otlp_exporter_sanitize_attributes() {
    let mut span = SpanBuilder::root(SpanName::QueryExecution)
        .attribute("password", "secret123")
        .attribute("db.system", "mysql")
        .build();
    OtlpExporter::sanitize_attributes(&mut span);
    assert_eq!(
        span.attributes.get("password"),
        Some(&"[REDACTED]".to_string())
    );
    assert_eq!(span.attributes.get("db.system"), Some(&"mysql".to_string()));
}

#[test]
fn test_bounded_span_queue_basic() {
    let mut q = BoundedSpanQueue::new(100);
    let span = SpanBuilder::root(SpanName::QueryExecution).build();
    q.enqueue(span);
    assert_eq!(q.len(), 1);
}

#[test]
fn test_bounded_span_queue_overflow() {
    let mut q = BoundedSpanQueue::new(2);
    q.enqueue(SpanBuilder::root(SpanName::QueryExecution).build());
    q.enqueue(SpanBuilder::root(SpanName::SqlExecute).build());
    q.enqueue(SpanBuilder::root(SpanName::CacheHit).build());
    assert_eq!(q.len(), 2);
    assert_eq!(q.overflow_count(), 1);
}

#[test]
fn test_bounded_span_queue_drain() {
    let mut q = BoundedSpanQueue::new(100);
    q.enqueue(SpanBuilder::root(SpanName::QueryExecution).build());
    q.enqueue(SpanBuilder::root(SpanName::SqlExecute).build());
    let drained = q.drain();
    assert_eq!(drained.len(), 2);
    assert!(q.is_empty());
}

#[test]
fn test_span_name_as_str() {
    assert_eq!(SpanName::QueryExecution.as_str(), "query_execution");
    assert_eq!(SpanName::ConnectionAcquire.as_str(), "connection_acquire");
    assert_eq!(SpanName::SqlExecute.as_str(), "sql_execute");
    assert_eq!(SpanName::ResultMap.as_str(), "result_map");
    assert_eq!(SpanName::CacheHit.as_str(), "cache_hit");
    assert_eq!(SpanName::AiTuning.as_str(), "ai_tuning");
}
