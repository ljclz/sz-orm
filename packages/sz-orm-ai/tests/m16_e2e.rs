//! M16.7：AI 能力深化端到端测试

#[cfg(feature = "ai-nl2sql-enhanced")]
#[tokio::test]
async fn test_ai_analyze_enhanced_e2e() {
    use sz_orm_ai::intent_analysis::{IntentAnalyzer, QueryIntent};
    use sz_orm_ai::multi_turn::ConversationContext;
    use sz_orm_ai::nl2sql::SchemaContext;

    let analyzer = IntentAnalyzer::new();
    let schema = SchemaContext::default();
    let ctx = ConversationContext::new();

    let result = analyzer
        .analyze_enhanced("统计用户的订单", &schema, &ctx)
        .await;
    assert!(result.confidence > 0.0, "置信度应大于 0");
    assert_eq!(
        result.intent,
        QueryIntent::AggregateQuery,
        "包含'统计'应识别为聚合查询"
    );
}

#[cfg(feature = "ai-index-advisor")]
#[tokio::test]
async fn test_ai_advise_with_benefit_estimation_e2e() {
    use sz_orm_ai::index_advisor::SlowQueryLog;
    use sz_orm_ai::workload_index::{TimeRange, WorkloadDrivenIndexAdvisor, WorkloadSummary};

    let advisor = WorkloadDrivenIndexAdvisor::new();
    let logs = vec![SlowQueryLog {
        sql: "SELECT * FROM orders WHERE customer_id = 123".to_string(),
        execution_time_ms: 5000,
        timestamp: 1000,
    }];
    let workload = WorkloadSummary::new(
        logs,
        TimeRange {
            start: 0,
            end: 2000,
        },
        100,
    );

    let suggestions = advisor.advise_with_benefit_estimation(&workload).await;
    assert!(!suggestions.is_empty(), "慢查询应产生索引建议");
    let first = &suggestions[0];
    assert!(!first.index_columns.is_empty(), "建议应包含索引列");
    assert!(!first.ddl_text.is_empty(), "建议应包含 DDL 文本");
}

#[cfg(feature = "llm-optimizer")]
#[tokio::test]
async fn test_ai_analyze_bottlenecks_e2e() {
    use sz_orm_ai::nl2sql::SchemaContext;
    use sz_orm_ai::query_plan_optimizer::{OptimizerConfig, UnifiedQueryOptimizer};

    let optimizer = UnifiedQueryOptimizer::new(OptimizerConfig::default());
    let schema = SchemaContext::default();
    let plan = "Seq Scan on orders (cost=10000 rows=1000000)\nHash Join (cost=5000 rows=50000)";

    let bottlenecks = optimizer.analyze_bottlenecks(plan, &schema).await;
    assert!(!bottlenecks.is_empty(), "应识别出瓶颈");
    use sz_orm_ai::query_plan_optimizer::PlanBottleneck;
    let has_seq_scan = bottlenecks
        .iter()
        .any(|b| matches!(b, PlanBottleneck::FullTableScan { .. }));
    assert!(has_seq_scan, "应识别 FullTableScan 瓶颈: {:?}", bottlenecks);
    let has_high_cost = bottlenecks
        .iter()
        .any(|b| matches!(b, PlanBottleneck::HighCostJoin { .. }));
    assert!(has_high_cost, "应识别 HighCostJoin 瓶颈: {:?}", bottlenecks);
}
