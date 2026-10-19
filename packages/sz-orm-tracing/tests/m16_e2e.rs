//! M16.8：OTel 集成增强端到端测试

#[cfg(feature = "otlp-export")]
mod tests {
    use sz_orm_tracing::otlp_exporter::{SpanBuilder, SpanName};
    use sz_orm_tracing::sampling::SlowQueryAlwaysSampler;

    #[tokio::test]
    async fn test_otel_span_attributes_e2e() {
        let span = SpanBuilder::root(SpanName::QueryExecution)
            .db_system("sqlite")
            .db_operation("SELECT")
            .db_statement_sanitized("SELECT * FROM users WHERE id = 123");
        let attrs = span.build().attributes;
        assert_eq!(attrs.get("db.system").unwrap(), "sqlite");
        assert_eq!(attrs.get("db.operation").unwrap(), "SELECT");
        let stmt = attrs.get("db.statement").unwrap();
        assert!(!stmt.contains("123"), "literal should be sanitized to ?");
        assert!(stmt.contains("?"));
    }

    #[tokio::test]
    async fn test_slow_query_always_sampler_e2e() {
        let sampler = SlowQueryAlwaysSampler::new(std::time::Duration::from_millis(100));

        let slow_decision =
            sampler.should_sample_with_latency(std::time::Duration::from_millis(200));
        assert!(slow_decision.is_sampled());

        let fast_decision =
            sampler.should_sample_with_latency(std::time::Duration::from_millis(10));
        assert!(!fast_decision.is_sampled());
    }
}
