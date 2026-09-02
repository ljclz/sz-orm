//! TASK-032 集成测试：查询历史学习端到端验证

use sz_orm_nl_query::history_learner::{HistoryLearner, QueryHistoryEntry};

fn make_entry(nl: &str, sql: &str, success: bool) -> QueryHistoryEntry {
    QueryHistoryEntry {
        nl_query: nl.to_string(),
        generated_sql: sql.to_string(),
        success,
        user_feedback: None,
        timestamp: "2024-01-01T00:00:00Z".to_string(),
    }
}

#[test]
fn test_record_and_learn_pattern() {
    let mut learner = HistoryLearner::new();
    learner.record(make_entry("查询所有用户", "SELECT * FROM users", true));
    learner.record(make_entry("查询所有订单", "SELECT * FROM orders", true));
    learner.record(make_entry("查询所有产品", "SELECT * FROM products", true));

    let patterns = learner.patterns();
    assert!(!patterns.is_empty(), "应学习到模式");
    let top_pattern = patterns[0];
    assert!(top_pattern.frequency >= 3, "频率应 >= 3");
}

#[test]
fn test_recommend_from_learned_pattern() {
    let mut learner = HistoryLearner::new();
    learner.record(make_entry("查询所有用户", "SELECT * FROM users", true));
    learner.record(make_entry("查询所有订单", "SELECT * FROM orders", true));
    learner.record(make_entry("查询所有产品", "SELECT * FROM products", true));

    let recommended = learner.recommend("查询所有订单");
    assert!(recommended.is_some(), "应能推荐相似模式");
    let pattern = recommended.unwrap();
    assert!(pattern.success_rate > 0.5, "成功率应 > 0.5");
}

#[test]
fn test_no_recommendation_for_low_success_rate() {
    let mut learner = HistoryLearner::new();
    learner.record(make_entry("统计用户", "SELECT COUNT(*) FROM users", false));
    learner.record(make_entry("统计用户", "SELECT COUNT(*) FROM users", false));
    learner.record(make_entry("统计用户", "SELECT COUNT(*) FROM users", false));

    let recommended = learner.recommend("统计订单");
    assert!(recommended.is_none(), "成功率低不应推荐");
}

#[test]
fn test_history_accumulation() {
    let mut learner = HistoryLearner::new();
    for i in 0..10 {
        learner.record(make_entry(
            &format!("查询 {}", i),
            &format!("SELECT * FROM table_{}", i),
            true,
        ));
    }
    assert_eq!(learner.history().len(), 10);
}

#[test]
fn test_export_learning_data() {
    let mut learner = HistoryLearner::new();
    learner.record(make_entry("查询用户", "SELECT * FROM users", true));
    learner.record(make_entry("查询订单", "SELECT * FROM orders", true));

    let exported = learner.export();
    assert!(exported["history"].is_array());
    assert!(exported["history"].as_array().unwrap().len() == 2);
    assert!(exported["patterns"].is_array());
}

#[test]
fn test_pattern_extraction_consistency() {
    let mut learner = HistoryLearner::new();
    learner.record(make_entry("查询所有用户", "SELECT * FROM users", true));
    learner.record(make_entry("查询所有订单", "SELECT * FROM orders", true));

    let patterns = learner.patterns();
    assert_eq!(patterns.len(), 1, "相似查询应归为同一模式");
}
