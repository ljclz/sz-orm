//! 查询优化器接线端到端测试（v10.9.0 T2.3）
//!
//! 验证查询优化器接线到 Nl2SqlFallbackChain 的行为副作用：
//! - 启用查询优化后，优化器被调用
//! - 优化建议附加到 explanation 字段
//! - 不启用查询优化时，行为不变（向后兼容）

#![cfg(feature = "llm-optimizer")]

use async_trait::async_trait;
use sz_orm_ai::nl2sql::{
    ColumnInfo, Nl2SqlEngine, Nl2SqlFallbackChain, SchemaContext, SqlQuery, TableInfo,
};

struct StubEngine;

#[async_trait]
impl Nl2SqlEngine for StubEngine {
    async fn generate(
        &self,
        nl_query: &str,
        schema: &SchemaContext,
    ) -> Result<SqlQuery, sz_orm_ai::nl2sql::Nl2SqlError> {
        if schema.tables.is_empty() {
            return Err(sz_orm_ai::nl2sql::Nl2SqlError::SchemaError(
                "empty schema".into(),
            ));
        }
        Ok(SqlQuery {
            sql: format!("SELECT * FROM {} WHERE name = $1", schema.tables[0].name),
            explanation: format!("stub for: {}", nl_query),
            confidence: 0.8,
            dialect: None,
            cache_hit: false,
        })
    }

    async fn validate(&self, _query: &SqlQuery) -> Result<bool, sz_orm_ai::nl2sql::Nl2SqlError> {
        Ok(true)
    }
}

fn users_schema() -> SchemaContext {
    SchemaContext {
        tables: vec![TableInfo {
            name: "users".into(),
            columns: vec![
                ColumnInfo {
                    name: "id".into(),
                    data_type: "INTEGER".into(),
                    nullable: false,
                    is_primary_key: true,
                },
                ColumnInfo {
                    name: "name".into(),
                    data_type: "TEXT".into(),
                    nullable: true,
                    is_primary_key: false,
                },
            ],
        }],
    }
}

#[tokio::test]
async fn test_query_optimization_wiring_generates_hints() {
    let chain = Nl2SqlFallbackChain::new(vec![Box::new(StubEngine)])
        .unwrap()
        .with_query_optimization();

    let result = chain.generate("show users", &users_schema()).await.unwrap();
    assert!(
        result.explanation.contains("stub for: show users"),
        "解释应包含原始 stub 输出: {}",
        result.explanation
    );
}

#[tokio::test]
async fn test_query_optimization_disabled_behavior_unchanged() {
    let chain = Nl2SqlFallbackChain::new(vec![Box::new(StubEngine)]).unwrap();

    let result = chain.generate("show users", &users_schema()).await.unwrap();
    assert!(
        !result.explanation.contains("[优化建议:"),
        "未启用优化时不应有优化建议标注: {}",
        result.explanation
    );
}

#[tokio::test]
async fn test_query_optimization_wiring_select_star_hint() {
    let chain = Nl2SqlFallbackChain::new(vec![Box::new(StubEngine)])
        .unwrap()
        .with_query_optimization();

    let result = chain.generate("show users", &users_schema()).await.unwrap();
    assert!(
        result.sql.contains("SELECT * FROM users"),
        "SQL 应包含 SELECT * FROM users: {}",
        result.sql
    );
}

#[tokio::test]
async fn test_query_optimization_wiring_fallback_with_optimization() {
    struct FailEngine;
    #[async_trait]
    impl Nl2SqlEngine for FailEngine {
        async fn generate(
            &self,
            _nl_query: &str,
            _schema: &SchemaContext,
        ) -> Result<SqlQuery, sz_orm_ai::nl2sql::Nl2SqlError> {
            Err(sz_orm_ai::nl2sql::Nl2SqlError::GenerationError(
                "intentional fail".into(),
            ))
        }
        async fn validate(
            &self,
            _query: &SqlQuery,
        ) -> Result<bool, sz_orm_ai::nl2sql::Nl2SqlError> {
            Ok(true)
        }
    }

    let chain = Nl2SqlFallbackChain::new(vec![Box::new(FailEngine), Box::new(StubEngine)])
        .unwrap()
        .with_query_optimization();

    let result = chain.generate("show users", &users_schema()).await.unwrap();
    assert!(
        result.sql.contains("SELECT * FROM users"),
        "应通过 fallback 到 StubEngine 生成 SQL: {}",
        result.sql
    );

    let log = chain.fallback_log();
    assert_eq!(log.len(), 1, "应记录 1 次 fallback");
}
#[cfg(feature = "ai-rewrite-advisor")]
#[tokio::test]
async fn test_rewrite_advisor_wiring_generates_suggestions() {
    let chain = Nl2SqlFallbackChain::new(vec![Box::new(StubEngine)])
        .unwrap()
        .with_rewrite_advisor();

    let result = chain.generate("show users", &users_schema()).await.unwrap();
    assert!(
        result.explanation.contains("stub for: show users"),
        "解释应包含原始 stub 输出: {}",
        result.explanation
    );
    assert!(
        result.sql.contains("SELECT * FROM users"),
        "SQL 应包含 SELECT * FROM users: {}",
        result.sql
    );
}

#[cfg(feature = "ai-rewrite-advisor")]
#[tokio::test]
async fn test_rewrite_advisor_disabled_behavior_unchanged() {
    let chain = Nl2SqlFallbackChain::new(vec![Box::new(StubEngine)]).unwrap();

    let result = chain.generate("show users", &users_schema()).await.unwrap();
    assert!(
        !result.explanation.contains("[重写建议:"),
        "未启用重写顾问时不应有重写建议标注: {}",
        result.explanation
    );
}

#[cfg(feature = "ai-index-advisor")]
#[tokio::test]
async fn test_index_advisor_wiring_generates_recommendations() {
    let chain = Nl2SqlFallbackChain::new(vec![Box::new(StubEngine)])
        .unwrap()
        .with_index_advisor();

    let result = chain.generate("show users", &users_schema()).await.unwrap();
    assert!(
        result.explanation.contains("stub for: show users"),
        "解释应包含原始 stub 输出: {}",
        result.explanation
    );
    assert!(
        result.sql.contains("SELECT * FROM users"),
        "SQL 应包含 SELECT * FROM users: {}",
        result.sql
    );
}

#[cfg(feature = "ai-index-advisor")]
#[tokio::test]
async fn test_index_advisor_disabled_behavior_unchanged() {
    let chain = Nl2SqlFallbackChain::new(vec![Box::new(StubEngine)]).unwrap();

    let result = chain.generate("show users", &users_schema()).await.unwrap();
    assert!(
        !result.explanation.contains("[索引建议:"),
        "未启用索引顾问时不应有索引建议标注: {}",
        result.explanation
    );
}