//! M16.8：慢查询分析增强端到端测试

#[cfg(feature = "query-advisor")]
mod tests {
    use sz_orm_advisor::slow_query_analyzer::{SlowQueryAnalyzer, SlowQueryEntry};

    #[tokio::test]
    async fn test_slow_query_analyze_with_plan_e2e() {
        let analyzer = SlowQueryAnalyzer::new(100);
        let entry = SlowQueryEntry::new("SELECT * FROM orders WHERE customer_id = 123", 5000, 1000);
        let plan = "TABLE SCAN ON orders (cost=10000 rows=1000000)";

        let report = analyzer.analyze_with_plan(&entry, plan).await;
        assert!(report.root_cause.is_some());
        assert!(!report.optimization_suggestions.is_empty());
        assert!(report.execution_plan.is_some());
    }
}
