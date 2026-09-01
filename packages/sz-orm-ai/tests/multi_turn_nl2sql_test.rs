//! TASK-007: NL2SQL 多轮对话测试
//!
//! 验证 ConversationContext 上下文累积 + MultiTurnNl2SqlEngine 多轮生成。

use std::sync::{atomic::AtomicUsize, Arc};

use async_trait::async_trait;
use parking_lot::Mutex;
use sz_orm_ai::{
    ConversationContext, MultiTurnNl2SqlEngine, Nl2SqlEngine, Nl2SqlError, SchemaContext,
    SimpleNl2SqlEngine, SqlQuery,
};

// ==================== Mock 引擎 ====================

/// 共享调用记录（测试持有，Mock 引擎通过 Arc 克隆引用）
#[derive(Default, Clone)]
struct CallLog {
    call_count: Arc<AtomicUsize>,
    received_prompts: Arc<Mutex<Vec<String>>>,
}

impl CallLog {
    fn new() -> Self {
        Self::default()
    }

    fn call_count(&self) -> usize {
        self.call_count.load(std::sync::atomic::Ordering::SeqCst)
    }

    fn received_prompts(&self) -> Vec<String> {
        self.received_prompts.lock().clone()
    }
}

/// 计数 Mock 引擎：记录 generate 被调用的次数和接收到的提示词
struct CountingMockEngine {
    log: CallLog,
}

impl CountingMockEngine {
    fn new(log: CallLog) -> Self {
        Self { log }
    }
}

#[async_trait]
impl Nl2SqlEngine for CountingMockEngine {
    async fn generate(
        &self,
        nl_query: &str,
        _schema: &SchemaContext,
    ) -> Result<SqlQuery, Nl2SqlError> {
        self.log
            .call_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.log.received_prompts.lock().push(nl_query.to_string());
        Ok(SqlQuery {
            sql: format!("SELECT * FROM t WHERE q = '{}'", nl_query),
            explanation: "mock".to_string(),
            confidence: 0.9,
            dialect: None,
        })
    }

    async fn validate(&self, _query: &SqlQuery) -> Result<bool, Nl2SqlError> {
        Ok(true)
    }
}

// ==================== ConversationContext 测试 ====================

#[test]
fn test_conversation_context_default() {
    let ctx = ConversationContext::new();
    assert!(ctx.history.is_empty());
    assert_eq!(ctx.max_turns, 10);
    assert_eq!(ctx.timeout, std::time::Duration::from_secs(30 * 60));
}

#[test]
fn test_conversation_context_add_turn() {
    let mut ctx = ConversationContext::new();
    ctx.add_turn("show users", "SELECT * FROM users");
    ctx.add_turn("where age > 25", "SELECT * FROM users WHERE age > 25");

    assert_eq!(ctx.history.len(), 2);
    assert_eq!(ctx.history[0].nl_query, "show users");
    assert_eq!(ctx.history[0].generated_sql, "SELECT * FROM users");
    assert_eq!(ctx.history[1].nl_query, "where age > 25");
}

#[test]
fn test_conversation_context_max_turns_eviction() {
    let mut ctx = ConversationContext::new().with_max_turns(3);
    for i in 0..5 {
        ctx.add_turn(&format!("q{}", i), &format!("SQL {}", i));
    }
    assert_eq!(ctx.history.len(), 3);
    assert_eq!(ctx.history[0].nl_query, "q2");
    assert_eq!(ctx.history[2].nl_query, "q4");
}

#[test]
fn test_conversation_context_build_prompt_empty_history() {
    let ctx = ConversationContext::new();
    let prompt = ctx.build_context_prompt("show users");
    assert_eq!(prompt, "show users");
}

#[test]
fn test_conversation_context_build_prompt_with_history() {
    let mut ctx = ConversationContext::new();
    ctx.add_turn("show users", "SELECT * FROM users");
    ctx.add_turn("where age > 25", "SELECT * FROM users WHERE age > 25");

    let prompt = ctx.build_context_prompt("order by name");

    assert!(prompt.contains("Previous conversation context"));
    assert!(prompt.contains("Turn 1"));
    assert!(prompt.contains("show users"));
    assert!(prompt.contains("SELECT * FROM users"));
    assert!(prompt.contains("Turn 2"));
    assert!(prompt.contains("where age > 25"));
    assert!(prompt.contains("order by name"));
    assert!(prompt.contains("incremental modification"));
}

#[test]
fn test_conversation_context_with_max_turns_builder() {
    let ctx = ConversationContext::new().with_max_turns(5);
    assert_eq!(ctx.max_turns, 5);
}

#[test]
fn test_conversation_context_with_timeout_builder() {
    let ctx = ConversationContext::new().with_timeout(std::time::Duration::from_secs(60));
    assert_eq!(ctx.timeout, std::time::Duration::from_secs(60));
}

#[test]
fn test_turn_record_debug_clone() {
    let mut ctx = ConversationContext::new();
    ctx.add_turn("q1", "SQL1");
    let turn = ctx.history[0].clone();
    assert_eq!(turn.nl_query, "q1");
    assert_eq!(turn.generated_sql, "SQL1");
}

// ==================== MultiTurnNl2SqlEngine 测试 ====================

#[tokio::test]
async fn test_multi_turn_engine_single_query() {
    let log = CallLog::new();
    let mock = CountingMockEngine::new(log.clone());
    let mut engine = MultiTurnNl2SqlEngine::new(mock);
    let schema = SchemaContext::default();

    let result = engine
        .generate_with_context("show users", &schema)
        .await
        .unwrap();

    assert!(result.sql.contains("show users"));
    assert_eq!(engine.conversation().history.len(), 1);
    assert_eq!(log.call_count(), 1);
}

#[tokio::test]
async fn test_multi_turn_engine_context_accumulation() {
    let log = CallLog::new();
    let mock = CountingMockEngine::new(log.clone());
    let mut engine = MultiTurnNl2SqlEngine::new(mock);
    let schema = SchemaContext::default();

    engine
        .generate_with_context("show users", &schema)
        .await
        .unwrap();
    engine
        .generate_with_context("where age > 25", &schema)
        .await
        .unwrap();
    engine
        .generate_with_context("order by name", &schema)
        .await
        .unwrap();

    assert_eq!(engine.conversation().history.len(), 3);

    let prompts = log.received_prompts();
    assert_eq!(prompts.len(), 3);

    // 第 1 轮：无历史，直接返回查询
    assert_eq!(prompts[0], "show users");

    // 第 2 轮：包含第 1 轮历史
    assert!(prompts[1].contains("Previous conversation context"));
    assert!(prompts[1].contains("show users"));
    assert!(prompts[1].contains("where age > 25"));

    // 第 3 轮：包含第 1、2 轮历史
    assert!(prompts[2].contains("Turn 1"));
    assert!(prompts[2].contains("Turn 2"));
    assert!(prompts[2].contains("order by name"));
}

#[tokio::test]
async fn test_multi_turn_engine_reset() {
    let log = CallLog::new();
    let mock = CountingMockEngine::new(log.clone());
    let mut engine = MultiTurnNl2SqlEngine::new(mock);
    let schema = SchemaContext::default();

    engine.generate_with_context("q1", &schema).await.unwrap();
    engine.generate_with_context("q2", &schema).await.unwrap();
    assert_eq!(engine.conversation().history.len(), 2);

    engine.reset();
    assert_eq!(engine.conversation().history.len(), 0);

    engine.generate_with_context("q3", &schema).await.unwrap();
    let prompts = log.received_prompts();
    // q3 的提示词应直接是 "q3"（无历史）
    assert_eq!(prompts[2], "q3");
}

#[tokio::test]
async fn test_multi_turn_engine_with_real_simple_engine() {
    let engine = SimpleNl2SqlEngine::new();
    let mut multi = MultiTurnNl2SqlEngine::new(engine);

    let schema = SchemaContext {
        tables: vec![sz_orm_ai::TableInfo {
            name: "users".to_string(),
            columns: vec![sz_orm_ai::ColumnInfo {
                name: "age".to_string(),
                data_type: "int".to_string(),
                nullable: true,
                is_primary_key: false,
            }],
        }],
    };

    let result = multi
        .generate_with_context("show users where age > 25", &schema)
        .await
        .unwrap();

    assert!(result.sql.to_lowercase().contains("select"));
    assert_eq!(multi.conversation().history.len(), 1);
}

#[test]
fn test_multi_turn_engine_builder_methods() {
    let log = CallLog::new();
    let mock = CountingMockEngine::new(log);
    let engine = MultiTurnNl2SqlEngine::new(mock)
        .with_max_turns(5)
        .with_timeout(std::time::Duration::from_secs(120));

    assert_eq!(engine.conversation().max_turns, 5);
    assert_eq!(
        engine.conversation().timeout,
        std::time::Duration::from_secs(120)
    );
}

#[tokio::test]
async fn test_multi_turn_engine_max_turns_eviction_in_engine() {
    let log = CallLog::new();
    let mock = CountingMockEngine::new(log);
    let mut engine = MultiTurnNl2SqlEngine::new(mock).with_max_turns(2);
    let schema = SchemaContext::default();

    for i in 0..4 {
        engine
            .generate_with_context(&format!("q{}", i), &schema)
            .await
            .unwrap();
    }

    assert_eq!(engine.conversation().history.len(), 2);
    assert_eq!(engine.conversation().history[0].nl_query, "q2");
    assert_eq!(engine.conversation().history[1].nl_query, "q3");
}
