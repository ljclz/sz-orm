#![cfg(feature = "owasp-pentest-suite")]

//! OWASP v10.10.0 边界场景攻击向量渗透测试（T4.5）
//!
//! 新增 5 个边界场景变体：Unicode 全角/RTL/组合字符、空字节变体、
//! 超长输入变体、空白字符变体、注释嵌套变体。
//! 复用既有 owasp-pentest-suite feature gate。

use sz_orm_audit::{HashChainAuditor, SqlAuditContext, SqlAuditor};

fn ctx(sql: &str, user: &str, ts: i64) -> SqlAuditContext {
    SqlAuditContext {
        sql: sql.to_string(),
        user: user.to_string(),
        timestamp: ts,
    }
}

/// 边界-1：Unicode 全角字符 + RTL 覆盖注入变体
///
/// 攻击模型：攻击者使用全角字符（＇）和 RTL 覆盖字符（U+202E）
/// 试图绕过基于 ASCII 的注入检测。审计系统应完整记录不崩溃。
#[test]
fn test_owasp_v10100_a03_injection_boundary_unicode_variant() {
    let auditor = SqlAuditor::new();
    let malicious = "SELECT * FROM users WHERE name = ＇admin＇ OR 1=1\u{202E}";
    auditor.log(&ctx(malicious, "attacker", 1000));
    let logs = auditor.get_logs();
    assert_eq!(logs.len(), 1);
    assert!(logs[0].sql.contains("OR 1=1"), "注入向量应被记录");
    assert!(logs[0].sql.contains('\u{202E}'), "RTL 字符应被保留");
}

/// 边界-2：多空字节注入变体（开头/中间/结尾）
///
/// 攻击模型：攻击者在不同位置注入多个空字节试图截断处理。
/// Rust 字符串是 UTF-8 安全的，空字节不截断。
#[test]
fn test_owasp_v10100_a03_injection_boundary_null_byte_variant() {
    let auditor = SqlAuditor::new();
    let variants = [
        "\0SELECT * FROM users WHERE 1=1",
        "SELECT\0 * FROM users WHERE 1=1",
        "SELECT * FROM users WHERE 1=1\0",
        "SELECT * FROM users\0WHERE\0 1=1",
    ];
    for (i, malicious) in variants.iter().enumerate() {
        auditor.log(&ctx(malicious, "attacker", 1000 + i as i64));
    }
    let logs = auditor.get_logs();
    assert_eq!(logs.len(), 4);
    for log in &logs {
        assert!(log.sql.contains("1=1"), "空字节不应截断注入向量");
    }
}

/// 边界-3：超长输入 + 注入组合变体（1MB payload）
///
/// 攻击模型：攻击者发送 1MB 超长 payload 混合注入向量，
/// 试图触发缓冲区溢出或处理超时。
#[test]
fn test_owasp_v10100_a03_injection_boundary_super_long_variant() {
    let auditor = SqlAuditor::new();
    let padding = "A".repeat(1_000_000);
    let malicious = format!(
        "SELECT * FROM users WHERE id = '{}' UNION SELECT col FROM secrets--",
        padding
    );
    auditor.log(&ctx(&malicious, "attacker", 1000));
    let logs = auditor.get_logs();
    assert_eq!(logs.len(), 1);
    assert!(logs[0].sql.len() >= 1_000_000, "超长输入应完整记录");
    assert!(logs[0].sql.contains("UNION SELECT"), "注入向量应保留");
}

/// 边界-4：空白字符变体（Tab/换行/垂直空白/全角空格）
///
/// 攻击模型：攻击者使用各种空白字符变体试图绕过 SQL 关键字检测。
#[test]
fn test_owasp_v10100_a03_injection_boundary_whitespace_variant() {
    let auditor = HashChainAuditor::new();
    let variants = [
        "SELECT\t*\tFROM\tusers\tWHERE\t1=1",
        "SELECT\n*\nFROM\nusers\nWHERE\n1=1",
        "SELECT\u{000B}*\u{000B}FROM\u{000B}users",
        "SELECT\u{3000}*\u{3000}FROM\u{3000}users",
    ];
    for (i, malicious) in variants.iter().enumerate() {
        auditor.log(&ctx(malicious, "attacker", 1000 + i as i64));
    }
    let entries = auditor.get_entries();
    assert_eq!(entries.len(), 4);
    assert!(auditor.verify().is_ok(), "哈希链完整性应保持");
}

/// 边界-5：注释嵌套变体（条件注释 + 多行注释）
///
/// 攻击模型：攻击者使用条件注释和多行注释试图绕过 WAF。
#[test]
fn test_owasp_v10100_a03_injection_boundary_comment_variant() {
    let auditor = SqlAuditor::new();
    let variants = [
        "SELECT * FROM users WHERE 1=1 /* admin */ --",
        "SELECT * FROM /* comment */ users WHERE 1=1",
        "SELECT * FROM users WHERE 1=1#",
        "SELECT * FROM users; /* multi\nline\ncomment */ DROP TABLE logs--",
    ];
    for (i, malicious) in variants.iter().enumerate() {
        auditor.log(&ctx(malicious, "attacker", 1000 + i as i64));
    }
    let logs = auditor.get_logs();
    assert_eq!(logs.len(), 4);
    assert!(logs[3].sql.contains("DROP TABLE"), "注释后的注入应被记录");
}
