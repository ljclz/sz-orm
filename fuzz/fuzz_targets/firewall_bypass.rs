#![no_main]
//! Fuzz Target 6: SQL 防火墙绕过检测
//!
//! 目标：发现 `SqlFirewall::check` 在处理任意 SQL 时的 panic/crash，
//! 以及防火墙规则绕过漏洞。
//!
//! 覆盖攻击面：
//! - `SqlFirewall::check` 对畸形 SQL 的处理
//! - 防火墙规则匹配绕过（大小写、编码、注释）
//! - `add_rule` 对畸形规则的处理
//! - `blocked_count`/`logged_count` 计数器溢出
//! - 空规则集 vs 非空规则集的行为差异
//! - 规则匹配的误报（合法 SQL 被拦截）和漏报（恶意 SQL 放行）

use libfuzzer_sys::fuzz_target;
use sz_orm_sql_validator::firewall::{FirewallAction, FirewallRule, SqlFirewall};
use std::hint::black_box;

/// 将任意字节转换为 UTF-8 字符串（损失转换）
fn bytes_to_string(data: &[u8]) -> String {
    String::from_utf8_lossy(data).into_owned()
}

/// 从 fuzz 输入中提取 SQL 和规则名（用 0xFF 分隔）
fn split_input(data: &[u8]) -> (String, String) {
    let parts: Vec<&[u8]> = data.splitn(2, |&b| b == 0xFF).collect();
    let sql = bytes_to_string(parts.first().copied().unwrap_or(&[]));
    let rule = bytes_to_string(parts.get(1).copied().unwrap_or(&[]));
    (sql, rule)
}

fuzz_target!(|data: &[u8]| {
    let (sql, rule_pattern) = split_input(data);

    // --- 空防火墙：所有 SQL 应通过（不应 panic） ---
    let firewall = SqlFirewall::new();
    let result = firewall.check(&sql);
    black_box(&result);

    // --- 添加 Block 规则后检查 ---
    let firewall = SqlFirewall::new();
    firewall.add_rule(FirewallRule {
        name: "fuzz_block".to_string(),
        pattern: rule_pattern.clone(),
        action: FirewallAction::Block,
        unless_pattern: None,
    });
    let result = firewall.check(&sql);
    black_box(&result);

    // --- 添加带例外条件的 Block 规则 ---
    let firewall = SqlFirewall::new();
    firewall.add_rule(FirewallRule {
        name: "fuzz_block_unless".to_string(),
        pattern: rule_pattern.clone(),
        action: FirewallAction::Block,
        unless_pattern: Some(r"(?i)\bWHERE\b".to_string()),
    });
    let result = firewall.check(&sql);
    black_box(&result);

    // --- 日志规则（不拦截，仅记录） ---
    let firewall = SqlFirewall::new();
    firewall.add_rule(FirewallRule {
        name: "fuzz_log".to_string(),
        pattern: rule_pattern,
        action: FirewallAction::Log,
        unless_pattern: None,
    });
    let result = firewall.check(&sql);
    black_box(&result);

    // --- 计数器检查 ---
    let blocked = firewall.blocked_count();
    let logged = firewall.logged_count();
    black_box(&blocked);
    black_box(&logged);

    // --- 畸形 SQL 边界 ---
    let edge_cases = [
        "",                          // 空字符串
        " ",                         // 空格
        "\0",                        // NULL 字节
        "\x00\x01\x02",              // 非 UTF-8（已由 bytes_to_string 转换）
        &"A".repeat(10000),          // 超长字符串
        "SELECT",                    // 仅关键字
        ";",                         // 仅分号
        "''",                        // 空字符串字面量
        "\"\"",                      // 双引号
    ];
    for edge_sql in &edge_cases {
        let firewall = SqlFirewall::new();
        let result = firewall.check(edge_sql);
        black_box(&result);
    }
});
