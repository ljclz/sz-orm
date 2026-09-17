//! v7.3.0 任务 3.6：AI 深度集成端到端演示
//!
//! 演示四项 AI 能力的完整调用链：
//! 1. 查询改写（RewriteEngine 规则路径）
//! 2. 索引推荐（IndexAdvisor 负载建模 + 收益预估）
//! 3. NL2SQL 多轮对话（SimpleNl2SqlEngine + 意图分析 + 注入防护）
//! 4. 向量 ANN 检索（InMemoryVectorStore + 租户隔离 + 召回率）
//!
//! 每项能力产出 AiDecision 审计记录，最终汇总打印。
//!
//! 运行方式：
//! ```bash
//! cargo run --example ai_integration_demo
//! ```

use std::collections::HashMap;

use sz_orm_ai::{
    ai_config::{AiConfig, AiDecision, AiDecisionType, AnnIndexType, RewritePath},
    index_advisor::{IndexAdvisor, QueryPattern, TableStats, WorkloadModel},
    nl2sql::{
        ColumnInfo, MultiTurnContext, Nl2SqlEngine, SchemaContext, SimpleNl2SqlEngine, TableInfo,
    },
    rewrite_advisor::RewriteEngine,
};
use sz_orm_vector::{
    AnnAccelerated, InMemoryVectorStore, PgVectorStore, VectorRecord,
};

/// 构造演示用 Schema（users + orders 两表）
fn build_schema() -> SchemaContext {
    SchemaContext {
        tables: vec![
            TableInfo {
                name: "users".to_string(),
                columns: vec![
                    ColumnInfo {
                        name: "id".to_string(),
                        data_type: "INTEGER".to_string(),
                        nullable: false,
                        is_primary_key: true,
                    },
                    ColumnInfo {
                        name: "name".to_string(),
                        data_type: "TEXT".to_string(),
                        nullable: true,
                        is_primary_key: false,
                    },
                    ColumnInfo {
                        name: "age".to_string(),
                        data_type: "INTEGER".to_string(),
                        nullable: true,
                        is_primary_key: false,
                    },
                ],
            },
            TableInfo {
                name: "orders".to_string(),
                columns: vec![
                    ColumnInfo {
                        name: "id".to_string(),
                        data_type: "INTEGER".to_string(),
                        nullable: false,
                        is_primary_key: true,
                    },
                    ColumnInfo {
                        name: "user_id".to_string(),
                        data_type: "INTEGER".to_string(),
                        nullable: false,
                        is_primary_key: false,
                    },
                    ColumnInfo {
                        name: "amount".to_string(),
                        data_type: "DECIMAL".to_string(),
                        nullable: true,
                        is_primary_key: false,
                    },
                ],
            },
        ],
    }
}

/// 演示 1：查询改写（规则路径）
fn demo_rewrite() -> AiDecision {
    println!("\n=== 1. 查询改写（规则路径）===");
    let engine = RewriteEngine::new();

    // 含子查询的 SQL，SubqueryFlatteningRule 应命中
    let sql = "SELECT * FROM users WHERE id IN (SELECT user_id FROM orders WHERE amount > 100)";
    println!("输入 SQL: {}", sql);

    let result = engine.rewrite(sql);
    match &result.suggestion {
        Some(s) => {
            println!("改写后 SQL: {}", s.rewritten_sql);
            println!("命中规则: {}", s.transform_type.name());
            println!("延迟: {}ms", result.latency_ms);
            AiDecision::new(
                AiDecisionType::Rewrite,
                sql,
                &s.rewritten_sql,
                s.transform_type.name(),
                0.9,
                result.latency_ms,
            )
        }
        None => {
            let reason = result.fallback_reason.as_deref().unwrap_or("无规则命中");
            println!("未改写: {}", reason);
            AiDecision::new(
                AiDecisionType::Rewrite,
                sql,
                sql,
                reason,
                0.0,
                result.latency_ms,
            )
        }
    }
}

/// 演示 2：索引推荐（负载建模 + 收益预估）
async fn demo_index_advisor() -> AiDecision {
    println!("\n=== 2. 索引推荐（负载建模 + 收益预估）===");
    let advisor = IndexAdvisor::new();

    let workload = WorkloadModel {
        patterns: vec![QueryPattern {
            sql_template: "SELECT * FROM users WHERE age > $1".to_string(),
            frequency: 100,
            columns_accessed: vec!["age".to_string()],
        }],
        table_stats: {
            let mut stats = HashMap::new();
            stats.insert(
                "users".to_string(),
                TableStats {
                    row_count: 100_000,
                    existing_indexes: vec!["id".to_string()],
                },
            );
            stats
        },
    };

    println!("负载: 1 个查询模式，users 表 100000 行");
    let recommendations = advisor.recommend(&workload).await.unwrap();

    if recommendations.is_empty() {
        println!("无索引推荐");
        return AiDecision::new(
            AiDecisionType::IndexRecommend,
            "workload with 1 pattern",
            "无推荐",
            "无候选索引",
            0.0,
            0,
        );
    }

    let rec = &recommendations[0];
    println!("推荐索引: {}", rec.suggestion.ddl_text);
    println!(
        "扫描降低: {:.1}%, 延迟降低: {:.1}%",
        rec.estimated_scan_reduction * 100.0,
        rec.estimated_latency_reduction * 100.0
    );
    println!("理由: {}", rec.reason);

    AiDecision::new(
        AiDecisionType::IndexRecommend,
        "workload: SELECT users WHERE age",
        &rec.suggestion.ddl_text,
        &rec.reason,
        0.85,
        0,
    )
}

/// 演示 3：NL2SQL 多轮对话（意图分析 + 注入防护）
async fn demo_nl2sql() -> AiDecision {
    println!("\n=== 3. NL2SQL 多轮对话（意图分析 + 注入防护）===");
    let engine = SimpleNl2SqlEngine::new();
    let schema = build_schema();
    let mut ctx = MultiTurnContext::new(schema);

    // 第一轮
    let nl1 = "show all users";
    println!("第一轮: \"{}\"", nl1);
    let result1 = engine.nl2sql(nl1, &ctx).await.unwrap();
    println!("  生成 SQL: {}", result1.sql.sql);
    println!("  意图: {}, 实体: {:?}", result1.intent.intent, result1.intent.entities);
    ctx.add_turn(nl1, result1.sql);

    // 第二轮（带上下文）
    let nl2 = "show users where age > 25";
    println!("第二轮: \"{}\"", nl2);
    let result2 = engine.nl2sql(nl2, &ctx).await.unwrap();
    println!("  生成 SQL: {}", result2.sql.sql);
    println!("  意图: {}, 实体: {:?}", result2.intent.intent, result2.intent.entities);
    println!("  延迟: {}ms", result2.latency_ms);

    AiDecision::new(
        AiDecisionType::Nl2sql,
        nl2,
        &result2.sql.sql,
        &format!("intent={}", result2.intent.intent),
        result2.sql.confidence as f64,
        result2.latency_ms,
    )
}

/// 演示 4：向量 ANN 检索（租户隔离 + 召回率）
async fn demo_vector_ann() -> AiDecision {
    println!("\n=== 4. 向量 ANN 检索（租户隔离 + 召回率）===");
    let store = InMemoryVectorStore::new();
    store.create_collection("docs", 3, None).await.unwrap();

    // 插入多租户向量
    let records = vec![
        make_tenant_record("a1", vec![1.0, 0.0, 0.0], "tenant_a"),
        make_tenant_record("a2", vec![0.9, 0.1, 0.0], "tenant_a"),
        make_tenant_record("a3", vec![0.8, 0.2, 0.0], "tenant_a"),
        make_tenant_record("b1", vec![1.0, 0.0, 0.0], "tenant_b"),
        make_tenant_record("b2", vec![0.95, 0.05, 0.0], "tenant_b"),
    ];
    store.insert("docs", records).await.unwrap();

    // tenant_a 检索
    let result = store
        .ann_search("docs", &[1.0, 0.0, 0.0], 5, Some("tenant_a"))
        .await
        .unwrap();

    println!("查询: tenant_a 的向量检索");
    println!("返回结果数: {}", result.records.len());
    println!("召回率: {:.1}%", result.recall_rate * 100.0);
    println!("延迟: {}ms", result.latency_ms);
    for r in &result.records {
        let tid = r
            .metadata
            .as_ref()
            .and_then(|m| m.get("tenant_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        println!("  - id={}, score={:.4}, tenant={}", r.id, r.score, tid);
    }

    AiDecision::new(
        AiDecisionType::VectorSearch,
        "ann_search docs tenant_a",
        &format!("{} results, recall={:.1}%", result.records.len(), result.recall_rate * 100.0),
        "InMemoryVectorStore AnnAccelerated",
        result.recall_rate,
        result.latency_ms,
    )
}

/// 构造带租户元数据的向量记录
fn make_tenant_record(id: &str, vector: Vec<f32>, tenant_id: &str) -> VectorRecord {
    let mut meta = HashMap::new();
    meta.insert(
        "tenant_id".to_string(),
        serde_json::Value::String(tenant_id.to_string()),
    );
    VectorRecord::new(id, vector).with_metadata(meta)
}

/// 打印 AiConfig 配置
fn print_config(config: &AiConfig) {
    println!("\n=== AiConfig 配置 ===");
    println!("  查询改写: {} (路径: {})", config.rewrite_enabled, config.rewrite_path.as_str());
    println!("  索引推荐: {}", config.index_advisor_enabled);
    println!("  NL2SQL: {} (多轮: {})", config.nl2sql_enabled, config.nl2sql_multi_turn);
    println!("  向量 ANN 索引: {}", config.vector_ann_index.as_str());
    println!("  召回率阈值: {:.1}%", config.vector_recall_threshold * 100.0);
    println!("  配置合法: {}", config.validate().is_ok());
    println!("  任一能力启用: {}", config.any_enabled());
}

#[tokio::main]
async fn main() {
    println!("========================================");
    println!("sz-orm v7.3.0 AI 深度集成端到端演示");
    println!("========================================");

    // 1. 构造 AiConfig，启用四项 AI 能力
    let config = AiConfig {
        rewrite_enabled: true,
        rewrite_path: RewritePath::Rule,
        index_advisor_enabled: true,
        nl2sql_enabled: true,
        nl2sql_multi_turn: true,
        vector_ann_index: AnnIndexType::Hnsw,
        vector_recall_threshold: 0.9,
    };
    print_config(&config);

    // 2. 执行四项 AI 能力，收集 AiDecision 审计记录
    let mut decisions: Vec<AiDecision> = Vec::new();

    // 2.1 查询改写
    decisions.push(demo_rewrite());

    // 2.2 索引推荐
    decisions.push(demo_index_advisor().await);

    // 2.3 NL2SQL 多轮对话
    decisions.push(demo_nl2sql().await);

    // 2.4 向量 ANN 检索
    decisions.push(demo_vector_ann().await);

    // 3. 汇总打印审计记录
    println!("\n========================================");
    println!("AiDecision 审计记录汇总（{} 条）", decisions.len());
    println!("========================================");
    for (i, d) in decisions.iter().enumerate() {
        println!(
            "\n[{}] 类型: {}",
            i + 1,
            d.decision_type.as_str()
        );
        println!("  输入: {}", d.input_summary);
        println!("  输出: {}", d.output);
        println!("  原因: {}", d.reason);
        println!("  置信度: {:.2}, 延迟: {}ms", d.confidence, d.latency_ms);
    }

    println!("\n演示完成。");
}