#![cfg(feature = "owasp-pentest-suite")]

//! OWASP v10.10.0 组合攻击向量渗透测试（T4.5）
//!
//! 新增 4 个组合攻击变体：UNION + 堆叠组合、编码 + 注释组合、
//! 多阶段攻击组合、盲注 + 时间延迟组合。
//! 复用既有 owasp-pentest-suite feature gate。

use sz_orm_audit::{AuditRules, HashChainAuditor, HashChainEntry, SqlAuditContext, SqlAuditor};

fn ctx(sql: &str, user: &str, ts: i64) -> SqlAuditContext {
    SqlAuditContext {
        sql: sql.to_string(),
        user: user.to_string(),
        timestamp: ts,
    }
}

/// 组合-1：UNION + 堆叠注入组合变体（多语句 + 数据窃取）
///
/// 攻击模型：攻击者组合 UNION SELECT 窃取数据 + 堆叠注入破坏表，
/// 在单次请求中同时窃取和破坏。deny 规则应拦截。
#[test]
fn test_owasp_v10100_a03_injection_combination_union_stacked_variant() {
    let rules = AuditRules::new().deny("union select").deny("drop table");
    let attacks = [
        "' UNION SELECT password FROM users; DROP TABLE logs--",
        "1 UNION SELECT * FROM admin; DROP TABLE audit--",
        "' UNION ALL SELECT credit_card FROM payments; DROP TABLE orders--",
    ];
    for attack in &attacks {
        assert!(
            !rules.should_audit(attack),
            "UNION + 堆叠组合攻击应被 deny 规则拦截: {}", attack
        );
    }
    assert!(rules.should_audit("SELECT * FROM products"), "正常查询应通过");
}

/// 组合-2：编码 + 注释组合变体（双重编码 + 行注释）
///
/// 攻击模型：攻击者使用双重 URL 编码绕过 WAF，再用行注释截断。
#[test]
fn test_owasp_v10100_a03_injection_combination_encoded_comment_variant() {
    let auditor = SqlAuditor::new();
    let attacks = [
        "%2527%2520OR%25201%253D1%2520--",
        "0x27%20OR%201=1%20--",
        "%27%20OR%201%3D1%23",
    ];
    for (i, attack) in attacks.iter().enumerate() {
        auditor.log(&ctx(attack, "attacker", 1000 + i as i64));
    }
    let logs = auditor.get_logs();
    assert_eq!(logs.len(), 3);
    for log in &logs {
        assert!(!log.sql.is_empty(), "编码组合攻击应被完整记录");
    }
}

/// 组合-3：多阶段攻击组合（侦察 + 窃取 + 清理）
///
/// 攻击模型：攻击者分三阶段：先探测表结构，再窃取数据，最后删除审计痕迹。
/// 哈希链应保证审计记录不可篡改。
#[test]
fn test_owasp_v10100_a03_injection_combination_multistage_variant() {
    let auditor = HashChainAuditor::new();
    let stages = [
        "SELECT table_name FROM information_schema.tables",
        "' UNION SELECT column_name FROM information_schema.columns--",
        "DROP TABLE audit_logs; DELETE FROM audit WHERE 1=1--",
    ];
    for (i, stage) in stages.iter().enumerate() {
        auditor.log(&ctx(stage, "attacker", 1000 + i as i64));
    }
    let entries = auditor.get_entries();
    assert_eq!(entries.len(), 3);
    assert!(auditor.verify().is_ok(), "多阶段攻击后哈希链应保持完整");
    assert!(
        entries[2].entry.sql.contains("DROP TABLE audit_logs"),
        "清理阶段应被记录"
    );
}

/// 组合-4：盲注 + 时间延迟组合变体
///
/// 攻击模型：攻击者使用条件时间延迟（SLEEP/WAITFOR）进行盲注，
/// 通过响应时间差异推断数据。审计应记录完整向量。
#[test]
fn test_owasp_v10100_a03_injection_combination_blind_time_variant() {
    let auditor = HashChainAuditor::new();
    let attacks = [
        "' OR IF(1=1, SLEEP(5), 0)--",
        "'; WAITFOR DELAY '00:00:05'--",
        "' AND (SELECT CASE WHEN (1=1) THEN SLEEP(5) ELSE 0 END)--",
        "' OR (SELECT SUBSTRING(password,1,1) FROM users WHERE id=1)='a' AND SLEEP(5)--",
    ];
    for (i, attack) in attacks.iter().enumerate() {
        auditor.log(&ctx(attack, "attacker", 1000 + i as i64));
    }
    let entries = auditor.get_entries();
    assert_eq!(entries.len(), 4);
    let recomputed = HashChainEntry::genesis(ctx(attacks[0], "attacker", 1000));
    assert_eq!(
        entries[0].current_hash, recomputed.current_hash,
        "盲注向量哈希应可重现"
    );
}