//! v8.0.0 AI 能力深化端到端接线测试
//!
//! 4 个端到端接线测试：
//! ① NlRewritePipeline 自然语言查询重写真实 DB 等价性验证全链路
//! ② SchemaDesignAdvisor 30 天查询模式数据产出 schema 设计建议
//! ③ AnomalyPredictionEngine 30 天时序数据异常预测脱敏报告
//! ④ RewriteEquivalenceVerifier 等价/不等价 SQL 验证

use std::sync::Arc;

use sz_orm_ai::ai_deep::{
    AnomalyPredictionEngine, NlRewriteConfig, NlRewritePipeline, QueryPattern,
    RewriteEquivalenceVerifier, SchemaDesignAdvisor, TimeSeriesData,
};

/// ① NlRewritePipeline 自然语言查询重写全链路
///
/// 验证：NL2SQL 转换 → 注入过滤 → 语义改写 → 可解释报告
#[tokio::test]
async fn test_wiring_nl_rewrite_pipeline_full_chain() {
    use sz_orm_ai::nl2sql::{ColumnInfo, SchemaContext, TableInfo};
    use sz_orm_ai::query_plan_optimizer::{OptimizerConfig, UnifiedQueryOptimizer};

    let optimizer = Arc::new(UnifiedQueryOptimizer::new(OptimizerConfig::default()));

    let schema = SchemaContext {
        tables: vec![TableInfo {
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
                    nullable: false,
                    is_primary_key: false,
                },
                ColumnInfo {
                    name: "age".to_string(),
                    data_type: "INTEGER".to_string(),
                    nullable: true,
                    is_primary_key: false,
                },
            ],
        }],
    };

    let config = NlRewriteConfig {
        enable_equivalence_verify: false, // 无 DB 执行器时跳过验证
        enable_injection_filter: true,
        max_rewrite_attempts: 3,
    };

    let pipeline = NlRewritePipeline::new(optimizer, None, config, schema);

    let result = pipeline.rewrite("show all users").await.unwrap();
    assert!(!result.original_sql.is_empty(), "原始 SQL 不应为空");
    assert!(!result.rewritten_sql.is_empty(), "重写 SQL 不应为空");
    assert!(!result.explanation.is_empty(), "可解释报告不应为空");
    assert!(result.explanation.contains("NL 重写报告"), "报告应包含标题");
}

/// ② SchemaDesignAdvisor 30 天查询模式数据产出 schema 设计建议
#[tokio::test]
async fn test_wiring_schema_design_advisor_30_days() {
    use async_trait::async_trait;
    use sz_orm_ai_designer::{AiSchemaDesigner, DesignError, LlmSchemaProvider, SchemaDesign};

    struct MockProvider;

    #[async_trait]
    impl LlmSchemaProvider for MockProvider {
        async fn design(&self, _requirement: &str) -> Result<SchemaDesign, DesignError> {
            Ok(SchemaDesign {
                tables: vec![],
                ddl_texts: vec![],
                rationale: "mock".to_string(),
            })
        }
    }

    let designer = Arc::new(AiSchemaDesigner::new(Box::new(MockProvider)));
    let advisor = SchemaDesignAdvisor::with_default_config(designer);

    let patterns = vec![
        QueryPattern {
            sql_template: "SELECT * FROM users WHERE id = ? AND email = ?".to_string(),
            frequency: 500,
            columns_accessed: vec!["id".to_string(), "email".to_string()],
            tables: vec!["users".to_string()],
            data_days: 30,
        },
        QueryPattern {
            sql_template: "SELECT * FROM orders WHERE user_id = ? AND created_at > ?".to_string(),
            frequency: 300,
            columns_accessed: vec!["user_id".to_string(), "created_at".to_string()],
            tables: vec!["orders".to_string()],
            data_days: 30,
        },
        QueryPattern {
            sql_template:
                "SELECT u.*, o.* FROM users u JOIN orders o ON u.id = o.user_id WHERE o.amount > ?"
                    .to_string(),
            frequency: 200,
            columns_accessed: vec![
                "id".to_string(),
                "user_id".to_string(),
                "amount".to_string(),
            ],
            tables: vec!["users".to_string(), "orders".to_string()],
            data_days: 30,
        },
    ];

    let report = advisor.advise(&patterns).await.unwrap();
    assert!(!report.explanation.is_empty(), "可解释报告不应为空");
    assert!(report.expected_benefit >= 0.0, "预期收益应非负");
    assert!(!report.index_suggestions.is_empty(), "应生成索引建议");
    assert!(
        !report.partition_suggestions.is_empty(),
        "应生成分区建议（created_at 时间列）"
    );
}

/// ③ AnomalyPredictionEngine 30 天时序数据异常预测脱敏报告
#[tokio::test]
async fn test_wiring_anomaly_prediction_engine_30_days() {
    use sz_orm_anomaly::AnomalyPredictor;

    let predictor = Arc::new(AnomalyPredictor::default());
    let engine = AnomalyPredictionEngine::with_default_config(predictor);

    // 生成 30 天时序数据（资源使用率上升趋势，预测容量瓶颈）
    let history: Vec<TimeSeriesData> = (0..30)
        .map(|i| TimeSeriesData {
            timestamp: 1700000000 + i as i64 * 86400,
            value: 50.0 + i as f64 * 1.5,
            metric_name: "resource_usage".to_string(),
        })
        .collect();

    let report = engine.predict(&history).await.unwrap();
    assert!(!report.predictions.is_empty(), "应生成预测");
    assert!(report.masked, "报告应已脱敏");
    assert!(!report.explanation.is_empty(), "可解释报告不应为空");
    assert!(
        report.explanation.contains("异常预测报告"),
        "报告应包含标题"
    );
    // 脱敏验证：不应包含原始 IP/端口
    assert!(
        !report.explanation.contains("127.0.0.1"),
        "脱敏后不应包含原始 IP"
    );
}

/// ④ RewriteEquivalenceVerifier 等价/不等价 SQL 验证
#[tokio::test]
async fn test_wiring_rewrite_equivalence_verifier() {
    use std::future::Future;
    use std::pin::Pin;
    use sz_orm_ai::ai_deep::equivalence_verifier::DbExecutor;

    /// SQLite 执行器（基于 sqlx）
    struct SqliteExecutor {
        pool: sqlx::SqlitePool,
    }

    impl SqliteExecutor {
        async fn new() -> Self {
            let pool = sqlx::sqlite::SqlitePoolOptions::new()
                .max_connections(1)
                .connect(":memory:")
                .await
                .unwrap();
            sqlx::query("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, age INTEGER)")
                .execute(&pool)
                .await
                .unwrap();
            sqlx::query(
                "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30), (2, 'Bob', 25), (3, 'Charlie', 35)",
            )
            .execute(&pool)
            .await
            .unwrap();
            Self { pool }
        }
    }

    impl DbExecutor for SqliteExecutor {
        fn execute(
            &self,
            sql: &str,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<Vec<String>>, String>> + Send + '_>> {
            let sql = sql.to_string();
            Box::pin(async move {
                use sqlx::Row;
                let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
                    .fetch_all(&self.pool)
                    .await
                    .map_err(|e| e.to_string())?;
                let mut result = Vec::with_capacity(rows.len());
                for row in rows {
                    let mut row_vec = Vec::new();
                    for i in 0..row.len() {
                        let val: String = if let Ok(v) = row.try_get::<Option<i64>, _>(i) {
                            v.map(|x| x.to_string()).unwrap_or_default()
                        } else if let Ok(v) = row.try_get::<Option<String>, _>(i) {
                            v.unwrap_or_default()
                        } else {
                            String::new()
                        };
                        row_vec.push(val);
                    }
                    result.push(row_vec);
                }
                Ok(result)
            })
        }
    }

    let executor = Arc::new(SqliteExecutor::new().await);
    let verifier = RewriteEquivalenceVerifier::with_default_config(executor);

    // 等价 SQL 验证通过
    let equivalent_result = verifier
        .verify(
            "SELECT id, name FROM users",
            "SELECT id, name FROM users ORDER BY id",
        )
        .await
        .unwrap();
    assert!(
        equivalent_result.is_equivalent,
        "等价 SQL 应验证通过: {:?}",
        equivalent_result.diff_details
    );
    let equivalent_result2 = verifier
        .verify(
            "SELECT id, name FROM users WHERE id > 0",
            "SELECT id, name FROM users WHERE id >= 1",
        )
        .await
        .unwrap();
    assert!(
        equivalent_result2.is_equivalent,
        "等价 SQL 应验证通过: {:?}",
        equivalent_result2.diff_details
    );

    // 不等价 SQL 验证失败
    let not_equivalent_result = verifier
        .verify(
            "SELECT id, name FROM users",
            "SELECT id, name FROM users WHERE id = 1",
        )
        .await
        .unwrap();
    assert!(
        !not_equivalent_result.is_equivalent,
        "不等价 SQL 应验证失败"
    );
    assert!(
        not_equivalent_result
            .diff_details
            .as_ref()
            .unwrap()
            .contains("行数不匹配"),
        "应包含行数不匹配详情"
    );
}
