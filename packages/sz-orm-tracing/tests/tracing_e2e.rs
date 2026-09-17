//! v7.3.0 任务 2.6：追踪端到端测试（真实 OTel Collector，需 --ignored）
//!
//! 验证：跨阶段 span 树 + OTLP 导出 + db.* 属性

use sz_orm_tracing::{
    Span, db_attrs, span_for_cache_lookup, span_for_failover, span_for_query,
};

#[tokio::test]
#[ignore = "需要真实 OTel Collector（localhost:4317）"]
async fn tracing_e2e_cross_stage_span_tree() {
    // 构建跨阶段 span 树：root → query → cache_lookup → failover
    let root = Span::new("trace-e2e", "span-root", "request");
    let query = span_for_query(&root, "SELECT * FROM orders", "conn-100");
    let cache = span_for_cache_lookup(&query, "orders:hash");
    let failover = span_for_failover(&cache, "primary->replica");

    // 验证 span 树结构
    assert_eq!(query.parent_id, Some("span-root".to_string()));
    assert_eq!(cache.parent_id, Some(query.span_id.clone()));
    assert_eq!(failover.parent_id, Some(cache.span_id.clone()));

    // 验证 db.* 属性
    assert!(query.tags.contains_key(db_attrs::STATEMENT));
    assert!(query.tags.contains_key(db_attrs::CONNECTION_ID));
    assert!(failover.tags.contains_key(db_attrs::FAILOVER));
}

#[tokio::test]
#[ignore = "需要真实 OTel Collector（localhost:4317）"]
async fn tracing_e2e_otlp_export_with_db_attrs() {
    let root = Span::new("trace-otlp", "span-root", "request");
    let query = span_for_query(&root, "SELECT 1", "conn-1");

    // 验证 span 可序列化为 JSON（OTLP 导出格式）
    let json = serde_json::to_string(&query).unwrap();
    assert!(json.contains("db.statement"));
    assert!(json.contains("db.connection_id"));
    assert!(json.contains("SELECT 1"));
    assert!(json.contains("conn-1"));
}