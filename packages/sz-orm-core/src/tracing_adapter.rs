//! # Tracing Adapter — sz-orm-core 分布式追踪适配层
//!
//! v5.0.0：将 sz-orm-tracing 的 SzTracer 接入 sz-orm-core，
//! 提供 `tracing_start_span` / `tracing_end_span` / `tracing_span_count` 三个入口，
//! 使分布式追踪能力从"幻影交付"变为"生产可达"。
//!
//! ## 设计
//!
//! - 全局 Tracer：`OnceLock<parking_lot::RwLock<SzTracer>>`
//! - 首次调用时惰性初始化默认 Tracer
//! - `tracing_start_span` 创建新 Span，`tracing_end_span` 完成 Span 并存储
//! - Span 计数通过 `SzTracer::get_spans().len()` 获取

use std::sync::OnceLock;

use parking_lot::RwLock;
use sz_orm_tracing::{Span, SzTracer, Tracer};

static TRACER: OnceLock<RwLock<SzTracer>> = OnceLock::new();

fn tracer() -> &'static RwLock<SzTracer> {
    TRACER.get_or_init(|| RwLock::new(SzTracer::new("sz-orm-core")))
}

/// 启动一个新的 Span
///
/// 内部取读锁，调用 `SzTracer::start_span`。
pub fn tracing_start_span(operation_name: &str) -> Span {
    let tracer = tracer().read();
    tracer.start_span(operation_name)
}

/// 结束一个 Span（完成并存储）
///
/// 内部取写锁，调用 `SzTracer::end_span`。
pub fn tracing_end_span(span: Span) {
    let tracer = tracer().read();
    tracer.end_span(span);
}

/// 获取当前已存储的 Span 数量（用于测试验证真实执行）
pub fn tracing_span_count() -> usize {
    let tracer = tracer().read();
    tracer.get_spans().len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tracing_start_end_span() {
        let span = tracing_start_span("test_op");
        assert_eq!(span.operation_name(), "test_op");
        tracing_end_span(span);
        assert!(tracing_span_count() > 0);
    }

    #[test]
    fn test_tracing_count_increments() {
        let before = tracing_span_count();
        let span = tracing_start_span("count_op");
        tracing_end_span(span);
        let after = tracing_span_count();
        assert!(after > before);
    }

    #[test]
    fn test_tracing_span_has_trace_id() {
        let span = tracing_start_span("id_check");
        assert!(!span.trace_id().is_empty());
        assert_eq!(span.trace_id().len(), 32);
        tracing_end_span(span);
    }
}
