use sz_orm_tracing::*;

#[test]
fn test_span_new() {
    let span = Span::new("trace1", "span1", "operation1");
    assert_eq!(span.trace_id, "trace1");
    assert_eq!(span.span_id, "span1");
    assert_eq!(span.operation_name, "operation1");
    assert!(span.parent_id.is_none());
    assert!(span.end_time.is_none());
}

#[test]
fn test_span_with_parent() {
    let mut span = Span::new("trace1", "span1", "op");
    span.parent_id = Some("parent_span".to_string());
    assert_eq!(span.parent_id, Some("parent_span".to_string()));
}

#[test]
fn test_span_with_service() {
    let mut span = Span::new("t", "s", "op");
    span.service_name = "my-service".to_string();
    assert_eq!(span.service_name, "my-service");
}

#[test]
fn test_span_with_tags() {
    let mut span = Span::new("t", "s", "op");
    span.tags.insert("key".to_string(), "value".to_string());
    assert_eq!(span.tags.get("key"), Some(&"value".to_string()));
}

#[test]
fn test_span_end_time() {
    let mut span = Span::new("t", "s", "op");
    span.end_time = Some(span.start_time + 100);
    assert!(span.end_time.is_some());
    assert!(span.end_time.unwrap() > span.start_time);
}

#[test]
fn test_span_serialization() {
    let span = Span::new("trace1", "span1", "op");
    let json = serde_json::to_string(&span).unwrap();
    let deserialized: Span = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.trace_id, "trace1");
    assert_eq!(deserialized.operation_name, "op");
}

#[test]
fn test_always_on_sampler() {
    let sampler = AlwaysOnSampler::new();
    let decision = sampler.should_sample("any_trace_id", None);
    assert_eq!(decision, SamplingDecision::RecordAndSample);
}

#[test]
fn test_always_off_sampler() {
    let sampler = AlwaysOffSampler::new();
    let decision = sampler.should_sample("any_trace_id", None);
    assert_eq!(decision, SamplingDecision::NotRecord);
}

#[test]
fn test_trace_id_ratio_sampler_full() {
    let sampler = TraceIdRatioSampler::new(1.0);
    let decision = sampler.should_sample("any_trace_id", None);
    assert_eq!(decision, SamplingDecision::RecordAndSample);
}

#[test]
fn test_trace_id_ratio_sampler_zero() {
    let sampler = TraceIdRatioSampler::new(0.0);
    let decision = sampler.should_sample("any_trace_id", None);
    assert_eq!(decision, SamplingDecision::NotRecord);
}

#[test]
fn test_sampler_names() {
    assert_eq!(AlwaysOnSampler::new().name(), "always_on");
    assert_eq!(AlwaysOffSampler::new().name(), "always_off");
}
