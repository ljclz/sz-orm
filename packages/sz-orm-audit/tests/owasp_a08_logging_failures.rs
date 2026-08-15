#![cfg(feature = "owasp-pentest-suite")]

//! OWASP A08: 安全日志和监控失败渗透测试（audit 包）
//!
//! 对应 REQ-V49-008（OWASP A08 深化）
//!
//! 渗透测试向量：
//! - 敏感数据脱敏：password/token/credit_card/ssn 等关键词被脱敏为 ******
//! - 审计日志哈希链完整：verify() 通过
//! - 安全事件被记录：审计记录包含 user/timestamp/sql
//! - 日志注入防护：SQL 中的换行符不破坏日志格式
//! - 审计日志包含足够上下文：user/timestamp/sql 三要素都存在
//! - 多种敏感信息脱敏：token/api_key/session/cvv 等全部覆盖
//! - 审计日志时间戳可追踪：timestamp 字段保留原始值

use sz_orm_audit::{HashChainAuditor, SqlAuditContext, SqlAuditor};

fn ctx(sql: &str, user: &str, ts: i64) -> SqlAuditContext {
    SqlAuditContext {
        sql: sql.to_string(),
        user: user.to_string(),
        timestamp: ts,
    }
}

/// A08-1：敏感数据脱敏——password 关键词被脱敏
///
/// 攻击模型：攻击者获取审计日志文件，从中提取敏感字段名。
/// 防护：SqlAuditor::log 自动将 password 等敏感关键词替换为 ******，
/// 使攻击者无法从日志中识别敏感字段位置。
#[test]
fn a08_password_masked_in_audit_log() {
    let auditor = SqlAuditor::new();
    auditor.log(&ctx(
        "SELECT * FROM users WHERE password='secret123'",
        "admin",
        1000,
    ));

    let logs = auditor.get_logs();
    assert_eq!(logs.len(), 1);
    assert!(
        !logs[0].sql.contains("password"),
        "password 关键词应被脱敏，实际: {}",
        logs[0].sql
    );
    assert!(logs[0].sql.contains("******"), "脱敏标记应存在");
}

/// A08-2：审计日志哈希链完整——verify() 通过
///
/// 攻击模型：攻击者篡改审计日志文件以消除痕迹。
/// 防护：HashChainAuditor 使用 SHA-256 哈希链，任何篡改都会被 verify() 检测。
#[test]
fn a08_audit_log_hash_chain_integrity() {
    let auditor = HashChainAuditor::new();
    auditor.log(&ctx("SELECT * FROM orders", "admin", 1000));
    auditor.log(&ctx("UPDATE orders SET status='paid'", "system", 1001));
    auditor.log(&ctx("DELETE FROM temp_logs", "cleanup", 1002));

    assert_eq!(auditor.len(), 3);
    assert!(auditor.verify().is_ok(), "未篡改的哈希链应通过验证");

    let entries = auditor.get_entries();
    assert_eq!(entries[0].prev_hash, sz_orm_audit::GENESIS_HASH);
    assert_eq!(entries[1].prev_hash, entries[0].current_hash);
    assert_eq!(entries[2].prev_hash, entries[1].current_hash);
}

/// A08-3：安全事件被记录——审计记录包含 user/timestamp/sql
///
/// 攻击模型：攻击者执行恶意操作但不被日志记录。
/// 防护：SqlAuditor 强制记录 user/timestamp/sql 三要素。
#[test]
fn a08_security_event_recorded_with_context() {
    let auditor = SqlAuditor::new();
    auditor.log(&ctx("DROP TABLE users", "attacker", 1700000000));

    let logs = auditor.get_logs();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].user, "attacker", "必须记录操作者");
    assert_eq!(logs[0].timestamp, 1700000000, "必须记录时间戳");
    assert!(logs[0].sql.contains("DROP"), "必须记录 SQL 操作类型");
}

/// A08-4：日志注入防护——SQL 中的换行符不破坏日志格式
///
/// 攻击模型：攻击者在 SQL 中注入换行符，试图伪造审计日志条目。
/// 防护：审计日志存储为结构化数据（SqlAuditContext），换行符不影响 JSON 序列化。
#[test]
fn a08_log_injection_newline_protection() {
    let auditor = SqlAuditor::new();
    let malicious_sql = "SELECT 1\n-- INSERT INTO admin VALUES('hacker')\n";
    auditor.log(&ctx(malicious_sql, "user1", 1000));

    let logs = auditor.get_logs();
    assert_eq!(logs.len(), 1, "换行符不应产生多条日志");

    let json = serde_json::to_string(&logs[0]).unwrap();
    assert!(json.contains("\\n"), "换行符在 JSON 中被转义，不破坏格式");
    assert!(!json.contains("\n\n"), "不存在未转义的连续换行");
}

/// A08-5：审计日志包含足够上下文——三要素都存在
///
/// 攻击模型：日志缺少关键上下文，无法追溯安全事件。
/// 防护：SqlAuditContext 强制包含 sql/user/timestamp 三个字段。
#[test]
fn a08_audit_log_sufficient_context() {
    let auditor = SqlAuditor::new();
    auditor.log(&ctx(
        "SELECT * FROM financial_records",
        "auditor",
        1700000001,
    ));

    let logs = auditor.get_logs();
    let entry = &logs[0];
    assert!(!entry.sql.is_empty(), "sql 字段非空");
    assert!(!entry.user.is_empty(), "user 字段非空");
    assert!(entry.timestamp > 0, "timestamp 为有效时间戳");
}

/// A08-6：多种敏感信息脱敏——token/api_key/session/cvv 全部覆盖
///
/// 攻击模型：审计日志泄露多种敏感信息。
/// 防护：mask_sensitive 覆盖 13 类敏感关键词。
#[test]
fn a08_multiple_sensitive_keywords_masked() {
    let test_cases = [
        ("WHERE token='abc'", "token"),
        ("WHERE api_key='xyz'", "api_key"),
        ("WHERE session='s123'", "session"),
        ("WHERE credit_card='4111'", "credit_card"),
        ("WHERE cvv='123'", "cvv"),
        ("WHERE ssn='123-45'", "ssn"),
        ("WHERE secret='data'", "secret"),
        ("WHERE access_key='ak'", "access_key"),
    ];

    for (sql, keyword) in &test_cases {
        let auditor = SqlAuditor::new();
        auditor.log(&ctx(sql, "user", 1000));
        let logs = auditor.get_logs();
        assert!(
            !logs[0].sql.contains(keyword),
            "关键词 '{}' 应被脱敏，实际: {}",
            keyword,
            logs[0].sql
        );
        assert!(logs[0].sql.contains("******"), "应包含脱敏标记");
    }
}

/// A08-7：审计日志时间戳可追踪——timestamp 保留原始值
///
/// 攻击模型：时间戳丢失或被篡改，无法重建事件时序。
/// 防护：SqlAuditor 保留原始 timestamp，不修改。
#[test]
fn a08_audit_timestamp_preserved() {
    let timestamps = [1700000000, 1700000001, 1700000002, 1700000100];
    let auditor = SqlAuditor::new();

    for &ts in &timestamps {
        auditor.log(&ctx("SELECT 1", "user", ts));
    }

    let logs = auditor.get_logs();
    assert_eq!(logs.len(), timestamps.len());
    for (i, &expected_ts) in timestamps.iter().enumerate() {
        assert_eq!(
            logs[i].timestamp, expected_ts,
            "timestamp 应保留原始值以支持时序重建"
        );
    }
}
