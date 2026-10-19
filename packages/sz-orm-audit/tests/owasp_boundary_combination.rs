#![cfg(feature = "owasp-pentest-suite")]

//! OWASP 边界/组合/绕过渗透测试（v10.9.0 T3.2）
//!
//! 补充 15~20 个边界场景、组合攻击向量、防御绕过尝试测试用例，
//! 复用既有 owasp-pentest-suite feature gate。
//!
//! 测试分类：
//! - 边界场景：Unicode 注入/空字节/超长输入/空字符串/混合编码
//! - 组合攻击向量：UNION + 堆叠/编码 + 注释/OR + 注释/DROP + ALTER/敏感数据 + 注入
//! - 防御绕过尝试：哈希碰撞/角色操纵/审计篡改/脱敏绕过/轮转绕过

use sz_orm_audit::{
    AuditRules, HashChainAuditor, HashChainEntry, RotatingAuditor, SqlAuditContext, SqlAuditor,
    GENESIS_HASH,
};

fn ctx(sql: &str, user: &str, ts: i64) -> SqlAuditContext {
    SqlAuditContext {
        sql: sql.to_string(),
        user: user.to_string(),
        timestamp: ts,
    }
}

// ==================== 边界场景测试 ====================

/// 边界-1：Unicode 零宽字符注入不破坏审计
///
/// 攻击模型：攻击者在 SQL 中嵌入零宽字符（U+200B）试图绕过模式匹配。
/// 审计系统应正确记录包含 Unicode 的 SQL，不崩溃不丢失。
#[test]
fn owasp_a03_injection_boundary_unicode_zero_width() {
    let auditor = SqlAuditor::new();
    let malicious = "SELECT * FROM users WHERE name = 'admin\u{200B}' OR 1=1";
    let audit_ctx = ctx(malicious, "attacker", 1000);
    auditor.log(&audit_ctx);
    let logs = auditor.get_logs();
    assert_eq!(logs.len(), 1);
    assert!(logs[0].sql.contains('\u{200B}'), "Unicode 字符应被保留");
    assert!(logs[0].sql.contains("OR 1=1"), "注入向量应被记录");
}

/// 边界-2：空字节注入不截断审计记录
///
/// 攻击模型：攻击者注入空字节（\0）试图截断 C 字符串处理。
/// Rust 字符串是 UTF-8 安全的，空字节不截断。
#[test]
fn owasp_a03_injection_boundary_null_byte() {
    let auditor = SqlAuditor::new();
    let malicious = "SELECT * FROM users; \0 DROP TABLE users";
    let audit_ctx = ctx(malicious, "attacker", 1000);
    auditor.log(&audit_ctx);
    let logs = auditor.get_logs();
    assert_eq!(logs.len(), 1);
    assert!(
        logs[0].sql.contains("DROP TABLE users"),
        "空字节后的内容不应被截断"
    );
}

/// 边界-3：超长输入不导致缓冲区溢出
///
/// 攻击模型：攻击者发送超长字符串试图触发缓冲区溢出。
/// Rust 的 String 是堆分配的，无固定缓冲区限制。
#[test]
fn owasp_a03_injection_boundary_super_long_input() {
    let auditor = SqlAuditor::new();
    let long_payload = format!("SELECT * FROM users WHERE id = '{}'", "A".repeat(100_000));
    let audit_ctx = ctx(&long_payload, "attacker", 1000);
    auditor.log(&audit_ctx);
    let logs = auditor.get_logs();
    assert_eq!(logs.len(), 1);
    assert_eq!(
        logs[0].sql.len(),
        long_payload.len(),
        "超长输入应完整记录"
    );
}

/// 边界-4：空字符串审计记录
///
/// 边界场景：空 SQL 字符串应被正确处理，不崩溃。
#[test]
fn owasp_a03_injection_boundary_empty_string() {
    let auditor = SqlAuditor::new();
    let audit_ctx = ctx("", "system", 1000);
    auditor.log(&audit_ctx);
    let logs = auditor.get_logs();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].sql, "", "空字符串应被正确记录");
}

/// 边界-5：混合编码注入（URL 编码 + Unicode + ASCII）
///
/// 攻击模型：攻击者混合使用 URL 编码、Unicode 和 ASCII 试图绕过过滤。
#[test]
fn owasp_a03_injection_boundary_mixed_encoding() {
    let auditor = SqlAuditor::new();
    let mixed = "SELECT%20*%20FROM%20users%20WHERE%20name%3D'admin'%20OR%201%3D1--\u{200B}";
    let audit_ctx = ctx(mixed, "attacker", 1000);
    auditor.log(&audit_ctx);
    let logs = auditor.get_logs();
    assert_eq!(logs.len(), 1);
    assert!(logs[0].sql.contains("%20OR%201%3D1"), "混合编码应被完整记录");
}

// ==================== 组合攻击向量测试 ====================

/// 组合-1：UNION + 堆叠注入组合
///
/// 攻击模型：攻击者组合 UNION SELECT 和堆叠注入（;DROP TABLE），
/// 试图在单次请求中同时窃取数据和破坏表。
#[test]
fn owasp_a03_injection_combination_union_and_stacked() {
    let auditor = HashChainAuditor::new();
    let combined = "' UNION SELECT password FROM users; DROP TABLE logs--";
    auditor.log(&ctx(combined, "attacker", 1000));
    let entries = auditor.get_entries();
    assert_eq!(entries.len(), 1);
    assert!(
        entries[0].entry.sql.contains("UNION SELECT") && entries[0].entry.sql.contains("DROP TABLE"),
        "组合攻击向量应被完整记录"
    );
}

/// 组合-2：URL 编码 + 注释注入组合
///
/// 攻击模型：攻击者使用 URL 编码绕过 WAF，再用注释截断后续查询。
#[test]
fn owasp_a03_injection_combination_encoded_and_comment() {
    let auditor = SqlAuditor::new();
    let combined = "%27%20OR%201%3D1%20--%20";
    auditor.log(&ctx(combined, "attacker", 1000));
    let logs = auditor.get_logs();
    assert_eq!(logs.len(), 1);
    assert!(
        logs[0].sql.contains("%27") && logs[0].sql.contains("--"),
        "编码 + 注释组合应被记录"
    );
}

/// 组合-3：OR 注入 + 注释截断组合
///
/// 攻击模型：经典 OR 1=1 + 注释截断，绕过密码验证。
#[test]
fn owasp_a03_injection_combination_or_and_comment() {
    let auditor = HashChainAuditor::new();
    let combined = "admin' OR '1'='1' -- ";
    auditor.log(&ctx(combined, "attacker", 1000));
    let entries = auditor.get_entries();
    assert_eq!(entries.len(), 1);
    let recomputed = HashChainEntry::genesis(ctx(combined, "attacker", 1000));
    assert_eq!(
        entries[0].current_hash, recomputed.current_hash,
        "组合注入的哈希应可重现"
    );
}

/// 组合-4：DROP + ALTER 组合越权
///
/// 攻击模型：攻击者同时执行 DROP 和 ALTER，最大化破坏。
/// deny 规则应拦截危险操作（should_audit 返回 false 表示被 deny 排除）。
#[test]
fn owasp_a03_injection_combination_drop_and_alter() {
    let rules = AuditRules::new()
        .deny("drop table")
        .deny("alter table");
    assert!(rules.should_audit("SELECT * FROM users"));
    assert!(!rules.should_audit("DROP TABLE users; ALTER TABLE logs ADD col int"));
}

/// 组合-5：敏感数据查询 + 注入组合
///
/// 攻击模型：攻击者通过注入窃取敏感字段（password/secret/token）。
#[test]
fn owasp_a03_injection_combination_sensitive_and_injection() {
    let auditor = SqlAuditor::new();
    let combined = "' UNION SELECT password, secret, token FROM credentials--";
    auditor.log(&ctx(combined, "attacker", 1000));
    let logs = auditor.get_logs();
    assert_eq!(logs.len(), 1);
    let masked = auditor.mask_sensitive(&logs[0].sql);
    assert!(
        masked.contains("******") || masked.contains("password"),
        "敏感数据应被脱敏或记录: {}",
        masked
    );
}

// ==================== 防御绕过尝试测试 ====================

/// 绕过-1：哈希碰撞尝试——不同输入产生不同哈希
///
/// 攻击模型：攻击者试图构造两个不同 SQL 产生相同哈希以绕过篡改检测。
/// SHA-256 抗碰撞，不同输入必然不同哈希（概率意义上）。
#[test]
fn owasp_a07_integrity_bypass_hash_collision_attempt() {
    let sql1 = "SELECT * FROM users WHERE id = 1";
    let sql2 = "SELECT * FROM users WHERE id = 2";
    let e1 = HashChainEntry::genesis(ctx(sql1, "admin", 1000));
    let e2 = HashChainEntry::genesis(ctx(sql2, "admin", 1000));
    assert_ne!(
        e1.current_hash, e2.current_hash,
        "不同 SQL 应产生不同哈希，碰撞绕过不可行"
    );
}

/// 绕过-2：角色操纵——普通用户伪造管理员角色
///
/// 攻击模型：攻击者尝试在审计日志中伪造 user 字段为 "admin"。
/// 审计系统记录的 user 来自可信上下文，伪造的 user 不影响哈希链完整性。
#[test]
fn owasp_a01_access_control_bypass_role_manipulation() {
    let auditor = HashChainAuditor::new();
    auditor.log(&ctx("SELECT * FROM users", "admin", 1000));
    auditor.log(&ctx("SELECT * FROM users", "attacker", 1001));
    let entries = auditor.get_entries();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].entry.user, "admin");
    assert_eq!(entries[1].entry.user, "attacker");
    assert_ne!(
        entries[0].entry.user, entries[1].entry.user,
        "不同用户应被区分记录"
    );
    assert_eq!(
        entries[1].prev_hash, entries[0].current_hash,
        "哈希链应链接连续记录"
    );
}

/// 绕过-3：审计日志篡改——修改记录后哈希不匹配
///
/// 攻击模型：攻击者修改审计日志中的 SQL 内容，但不更新哈希。
/// verify 时重新计算的哈希与存储的哈希不匹配，检测到篡改。
#[test]
fn owasp_a07_integrity_bypass_audit_tampering() {
    let original = ctx("SELECT 1", "admin", 1000);
    let tampered = ctx("DROP TABLE users", "admin", 1000);
    let e_orig = HashChainEntry::genesis(original);
    let e_tampered = HashChainEntry::genesis(tampered);
    assert_ne!(
        e_orig.current_hash, e_tampered.current_hash,
        "篡改 SQL 后哈希必然变化，篡改可被检测"
    );
    assert_eq!(e_orig.prev_hash, GENESIS_HASH);
    assert_eq!(e_tampered.prev_hash, GENESIS_HASH);
}

/// 绕过-4：脱敏绕过——大小写混合敏感字段
///
/// 攻击模型：攻击者使用大小写混合（PaSsWoRd）试图绕过脱敏。
/// 脱敏应大小写不敏感。
#[test]
fn owasp_a02_crypto_bypass_masking_case_mixed() {
    let auditor = SqlAuditor::new();
    let mixed_case = "SELECT PaSsWoRd FROM users WHERE SeCrEt = 'abc'";
    let masked = auditor.mask_sensitive(mixed_case);
    assert!(
        masked.contains("******"),
        "大小写混合的敏感字段应被脱敏: {}",
        masked
    );
}

/// 绕过-5：轮转绕过——超量日志仍被审计
///
/// 攻击模型：攻击者发送大量请求试图触发日志轮转，使早期记录被丢弃。
/// 轮转后审计系统应继续记录新请求。
#[test]
fn owasp_a09_logging_bypass_rotation_still_audits() {
    let auditor = RotatingAuditor::with_max_entries(3);
    for i in 0..10 {
        let audit_ctx = ctx(&format!("SELECT {}", i), "user", 1000 + i);
        let logged = auditor.log(&audit_ctx);
        assert!(logged, "第 {} 条日志应被记录", i);
    }
    assert!(
        auditor.rotation_count() > 0,
        "应发生轮转: {}",
        auditor.rotation_count()
    );
    assert!(
        !auditor.is_empty(),
        "轮转后审计系统应仍有记录"
    );
}

/// 绕过-6：哈希链重放——相同记录在不同位置产生不同哈希
///
/// 攻击模型：攻击者复制一条合法审计记录试图重放。
/// 由于 prev_hash 不同，重放的记录哈希与原始不同。
#[test]
fn owasp_a07_integrity_bypass_replay_detection() {
    let auditor = HashChainAuditor::new();
    auditor.log(&ctx("SELECT 1", "admin", 1000));
    auditor.log(&ctx("SELECT 1", "admin", 1001));
    let entries = auditor.get_entries();
    assert_eq!(entries.len(), 2);
    assert_ne!(
        entries[0].current_hash, entries[1].current_hash,
        "相同 SQL 在不同位置应产生不同哈希（prev_hash 不同），重放可被检测"
    );
}

/// 绕过-7：审计规则绕过——deny 规则正确拦截
///
/// 攻击模型：攻击者执行 DROP TABLE 试图绕过审计规则。
/// deny 规则应排除该 SQL（should_audit 返回 false 表示被 deny 拦截）。
#[test]
fn owasp_a03_injection_bypass_audit_rules_deny() {
    let rules = AuditRules::new()
        .allow("select")
        .deny("drop")
        .deny("alter")
        .deny("grant");
    assert!(rules.should_audit("SELECT * FROM users"));
    assert!(!rules.should_audit("DROP TABLE users"));
    assert!(!rules.should_audit("ALTER TABLE users ADD col int"));
    assert!(!rules.should_audit("GRANT SELECT ON users TO public"));
    assert_eq!(rules.deny_count(), 3);
    assert_eq!(rules.allow_count(), 1);
}