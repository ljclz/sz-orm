//! 分布式追踪适配层端到端测试
//!
//! 验证 tracing_adapter 的三个入口函数真实调用 sz-orm-tracing 的 SzTracer。

use sz_orm_core::tracing_adapter::{tracing_end_span, tracing_span_count, tracing_start_span};

#[test]
fn test_tracing_start_end_span() {
    let span = tracing_start_span("e2e_op");
    assert_eq!(span.operation_name(), "e2e_op");
    tracing_end_span(span);
    assert!(
        tracing_span_count() > 0,
        "should have at least 1 span after end"
    );
}

#[test]
fn test_tracing_count_increments() {
    let before = tracing_span_count();
    let span = tracing_start_span("e2e_count_op");
    tracing_end_span(span);
    let after = tracing_span_count();
    assert!(after > before, "span count should increment after end_span");
}

#[test]
fn test_tracing_span_has_valid_ids() {
    let span = tracing_start_span("e2e_id_check");
    assert_eq!(span.trace_id().len(), 32, "trace_id should be 32 hex chars");
    assert_eq!(span.span_id().len(), 16, "span_id should be 16 hex chars");
    tracing_end_span(span);
}
