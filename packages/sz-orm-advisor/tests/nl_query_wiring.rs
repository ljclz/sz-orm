//! NL 查询端到端接线测试
//!
//! 验证 NlQueryGateway → SafetyGate → Formatter 完整调用链。

use sz_orm_advisor::*;

#[test]
fn test_wiring_gateway_safe_query() {
    let mut gw = NlQueryGateway::new();
    let result = gw.process_sql(
        "查询所有活跃用户",
        "SELECT id, name FROM users WHERE active = ?",
    );
    assert!(result.is_safe);
    assert_eq!(result.verdict, SafetyVerdict::Pass);
    assert!(result.sql.contains("SELECT"));
}

#[test]
fn test_wiring_gateway_injection_blocked() {
    let mut gw = NlQueryGateway::new();
    let result = gw.process_sql("恶意输入", "SELECT * FROM users WHERE name = 'x' OR '1'='1");
    assert!(!result.is_safe);
    assert!(matches!(
        result.verdict,
        SafetyVerdict::InjectionDetected(_)
    ));
}

#[test]
fn test_wiring_gateway_ddl_blocked() {
    let mut gw = NlQueryGateway::new();
    let result = gw.process_sql("建表", "CREATE TABLE evil (id INT)");
    assert!(!result.is_safe);
    assert!(matches!(result.verdict, SafetyVerdict::DdlDetected(_)));
}

#[test]
fn test_wiring_gateway_with_result_format() {
    let mut gw = NlQueryGateway::new();
    let result = gw.process_with_result(
        "查询用户姓名",
        "SELECT name FROM users WHERE id = ?",
        &["name".into()],
        &[
            vec!["Alice".into()],
            vec!["Bob".into()],
            vec!["Charlie".into()],
        ],
    );
    assert!(result.is_safe);
    let formatted = result.formatted.unwrap();
    assert_eq!(formatted.rows.len(), 3);
    assert!(formatted.summary.contains("3 行"));
}

#[test]
fn test_wiring_cache_hit_on_repeat() {
    let mut gw = NlQueryGateway::new();
    gw.process_sql("查询用户", "SELECT * FROM users WHERE id = ?");
    gw.process_sql("查询用户", "SELECT * FROM users WHERE id = ?");
    assert!(gw.cache_hit_rate() > 0.0);
}

#[test]
fn test_wiring_hot_swap() {
    let swapper = LlmHotSwapper::new();
    swapper.register("openai", "gpt-4");
    swapper.register("claude", "claude-3");
    let initial = swapper.active().unwrap();
    assert_eq!(initial.name, "openai");
    swapper.swap_to(1).unwrap();
    let swapped = swapper.active().unwrap();
    assert_eq!(swapped.name, "claude");
}

#[test]
fn test_wiring_multi_turn_context() {
    use std::time::Duration;
    let mut ctx = MultiTurnContext::new("session-1", 5, Duration::from_secs(60));
    ctx.set_var("target_table", "users");
    let turn1 = ctx
        .add_turn("查询用户", "SELECT * FROM users WHERE id = ?", true)
        .unwrap();
    assert_eq!(turn1.turn, 1);
    let turn2 = ctx
        .add_turn("按年龄排序", "SELECT * FROM users ORDER BY age", true)
        .unwrap();
    assert_eq!(turn2.turn, 2);
    let prompt = ctx.build_prompt("只看前10条");
    assert!(prompt.contains("前序对话"));
    assert!(prompt.contains("target_table"));
}
