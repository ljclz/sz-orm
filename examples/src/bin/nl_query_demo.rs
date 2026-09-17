//! AI 自然语言查询示例
//!
//! 演示 NlQueryGateway + SafetyGate + Formatter + HotSwap + MultiTurn 完整流程。

use std::time::Duration;

use sz_orm_advisor::*;

fn main() {
    let mut gateway = NlQueryGateway::new().max_display_rows(10);

    let result = gateway.process_sql(
        "查询所有活跃用户",
        "SELECT id, name, email FROM users WHERE active = ?",
    );
    println!("NL 查询: {}", result.nl_query);
    println!("生成 SQL: {}", result.sql);
    println!("安全判定: {:?}", result.verdict);
    println!("通过: {}", result.is_safe);

    let result_with_data = gateway.process_with_result(
        "查询用户列表",
        "SELECT name, age FROM users WHERE dept = ? ORDER BY age",
        &["name".into(), "age".into()],
        &[
            vec!["Alice".into(), "30".into()],
            vec!["Bob".into(), "25".into()],
            vec!["Charlie".into(), "35".into()],
        ],
    );
    if let Some(_formatted) = &result_with_data.formatted {
        println!("\n格式化结果:");
        println!("{}", gateway.to_markdown(&result_with_data).unwrap());
    }

    let blocked = gateway.process_sql("恶意", "SELECT * FROM users; DROP TABLE users;");
    println!("\n恶意查询被拦截: {:?}", blocked.verdict);

    let swapper = LlmHotSwapper::new();
    swapper.register("openai", "gpt-4");
    swapper.register("claude", "claude-3-opus");
    println!("\n当前 LLM: {}", swapper.active().unwrap().name);
    swapper.swap_to(1).unwrap();
    println!("切换后 LLM: {}", swapper.active().unwrap().name);

    let mut ctx = MultiTurnContext::new("demo-session", 10, Duration::from_secs(300));
    ctx.set_var("target_table", "users");
    ctx.add_turn("查询用户", "SELECT * FROM users WHERE id = ?", true)
        .unwrap();
    ctx.add_turn("按年龄排序", "SELECT * FROM users ORDER BY age", true)
        .unwrap();
    let prompt = ctx.build_prompt("只看前10条");
    println!("\n多轮对话提示词:\n{}", prompt);

    println!("\n缓存命中率: {:.1}%", gateway.cache_hit_rate() * 100.0);
}
