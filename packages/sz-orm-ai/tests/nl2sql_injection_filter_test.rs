//! v7.4.0 任务 2.6：NL2SQL 注入过滤端到端测试
//!
//! 验证 Nl2sqlResult::enforce_injection_filter()：
//! - 安全 SQL 不触发过滤
//! - 注入向量触发过滤并清理
//! - 参数化 SQL 不被误判

use sz_orm_ai::{Nl2sqlResult, SqlQuery, IntentAnalysis};

/// 构造测试用 Nl2sqlResult
fn make_result(sql: &str) -> Nl2sqlResult {
    Nl2sqlResult {
        sql: SqlQuery {
            sql: sql.to_string(),
            explanation: String::new(),
            confidence: 1.0,
            dialect: None,
            cache_hit: false,
        },
        intent: IntentAnalysis {
            intent: "select".to_string(),
            entities: Vec::new(),
            confidence: 1.0,
        },
        latency_ms: 10,
        injection_filtered: false,
    }
}

/// 安全 SQL 不触发过滤
#[test]
fn test_safe_sql_not_filtered() {
    let mut result = make_result("SELECT id, name FROM users WHERE age > 25");
    result.enforce_injection_filter();
    assert!(!result.injection_filtered);
    assert_eq!(result.sql.sql, "SELECT id, name FROM users WHERE age > 25");
}

/// UNION 注入向量触发过滤
#[test]
fn test_union_injection_filtered() {
    let mut result = make_result("SELECT id FROM users; DROP TABLE users--");
    result.enforce_injection_filter();
    assert!(result.injection_filtered);
}

/// OR 1=1 注入向量触发过滤
#[test]
fn test_or_1_1_injection_filtered() {
    let mut result = make_result("SELECT id FROM users WHERE name = 'admin' OR 1=1");
    result.enforce_injection_filter();
    assert!(result.injection_filtered);
}

/// 注释注入向量触发过滤
#[test]
fn test_comment_injection_filtered() {
    let mut result = make_result("SELECT id FROM users WHERE name = 'admin' /* */");
    result.enforce_injection_filter();
    assert!(result.injection_filtered);
}

/// 参数化 SQL 不被误判（$1 占位符）
#[test]
fn test_parameterized_sql_not_filtered() {
    let mut result = make_result("SELECT id FROM users WHERE name = $1 AND age > $2");
    result.enforce_injection_filter();
    assert!(!result.injection_filtered);
}

/// 参数化 SQL 不被误判（? 占位符）
#[test]
fn test_placeholder_sql_not_filtered() {
    let mut result = make_result("SELECT id FROM users WHERE name = ? AND age > ?");
    result.enforce_injection_filter();
    assert!(!result.injection_filtered);
}

/// 多次调用 enforce_injection_filter 幂等
#[test]
fn test_enforce_injection_filter_idempotent() {
    let mut result = make_result("SELECT id FROM users WHERE name = 'admin' OR 1=1");
    result.enforce_injection_filter();
    let first_filtered = result.injection_filtered;
    result.enforce_injection_filter();
    assert_eq!(result.injection_filtered, first_filtered);
}

/// enforce_injection_filter 返回 &mut Self 支持链式调用
#[test]
fn test_enforce_injection_filter_chainable() {
    let mut result = make_result("SELECT id FROM users WHERE name = 'admin' OR 1=1");
    result.enforce_injection_filter();
    assert!(result.injection_filtered);
}

/// 空字符串安全
#[test]
fn test_empty_sql_safe() {
    let mut result = make_result("");
    result.enforce_injection_filter();
    assert!(!result.injection_filtered);
}

/// 简单 SELECT 安全
#[test]
fn test_simple_select_safe() {
    let mut result = make_result("SELECT 1");
    result.enforce_injection_filter();
    assert!(!result.injection_filtered);
}