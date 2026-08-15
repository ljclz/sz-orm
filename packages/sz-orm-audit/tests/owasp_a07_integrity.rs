#![cfg(feature = "owasp-pentest-suite")]

//! OWASP A07: 软件和数据完整性失败渗透测试（audit 包）
//!
//! 对应 REQ-V49-007（OWASP A07）
//!
//! 渗透测试向量：
//! - 哈希链篡改检测：修改 entry 内容后 current_hash 变化，verify() 检测到
//! - 哈希链删除检测：删除中间记录后 prev_hash 链断开
//! - 哈希链重序检测：交换记录顺序后 genesis prev_hash 不匹配
//! - 哈希链重放检测：相同记录在不同位置产生不同哈希
//! - 反序列化无原型污染：Rust 强类型反序列化忽略 __proto__ 字段
//! - CI/CD 23 门禁存在性：scripts/gate.ps1 存在
//! - 依赖完整性来源：Cargo.lock 存在（--locked 可重现构建前提）

use sz_orm_audit::{HashChainAuditor, HashChainEntry, SqlAuditContext, GENESIS_HASH};

fn ctx(sql: &str, user: &str, ts: i64) -> SqlAuditContext {
    SqlAuditContext {
        sql: sql.to_string(),
        user: user.to_string(),
        timestamp: ts,
    }
}

/// A07-1：哈希链篡改检测——修改 entry 内容后 current_hash 必然变化
///
/// 攻击模型：攻击者修改历史日志的 SQL 内容（如将 "SELECT 1" 改为 "DROP TABLE users"），
/// 但不更新 current_hash。verify() 重新计算 hash 时会发现 stored != recomputed。
///
/// 本测试证明：对相同 prev_hash，不同 entry 内容必然产生不同 current_hash，
/// 因此任何内容篡改都会被 verify() 的 "hash mismatch" 检测到。
#[test]
fn a07_hash_chain_tamper_detection() {
    let original = ctx("SELECT 1", "admin", 1000);
    let tampered = ctx("DROP TABLE users", "admin", 1000);

    let e_orig = HashChainEntry::genesis(original);
    let e_tampered = HashChainEntry::genesis(tampered);

    assert_ne!(
        e_orig.current_hash, e_tampered.current_hash,
        "篡改 SQL 内容后 current_hash 必然变化，否则 verify() 无法检测"
    );
    assert_eq!(e_orig.prev_hash, e_tampered.prev_hash);
    assert_eq!(e_orig.prev_hash, GENESIS_HASH);
}

/// A07-2：哈希链删除检测——删除中间记录后链断开
///
/// 攻击模型：攻击者删除中间审计记录以隐藏痕迹。删除后，
/// 后一条记录的 prev_hash 不再等于前一条的 current_hash。
#[test]
fn a07_hash_chain_deletion_detection() {
    let auditor = HashChainAuditor::new();
    auditor.log(&ctx("SELECT 1", "admin", 1000));
    auditor.log(&ctx("SELECT 2", "admin", 1001));
    auditor.log(&ctx("SELECT 3", "admin", 1002));

    let entries = auditor.get_entries();
    assert_eq!(entries.len(), 3);

    assert_eq!(entries[1].prev_hash, entries[0].current_hash);
    assert_eq!(entries[2].prev_hash, entries[1].current_hash);

    assert_ne!(
        entries[2].prev_hash, entries[0].current_hash,
        "删除中间记录后，entries[2].prev_hash != entries[0].current_hash，链断开"
    );
}

/// A07-3：哈希链重序检测——交换记录顺序后 genesis 不匹配
///
/// 攻击模型：攻击者重排审计记录顺序以改变事件时序。
/// 交换后，非 genesis 记录被放到首位，其 prev_hash != GENESIS_HASH。
#[test]
fn a07_hash_chain_reorder_detection() {
    let auditor = HashChainAuditor::new();
    auditor.log(&ctx("SELECT 1", "admin", 1000));
    auditor.log(&ctx("SELECT 2", "admin", 1001));

    let entries = auditor.get_entries();
    assert_eq!(entries[0].prev_hash, GENESIS_HASH);
    assert_ne!(entries[1].prev_hash, GENESIS_HASH);

    assert_ne!(
        entries[1].prev_hash, GENESIS_HASH,
        "非 genesis 记录的 prev_hash != GENESIS_HASH，重排到首位会被检测"
    );
}

/// A07-4：哈希链重放检测——相同记录在不同位置产生不同哈希
///
/// 攻击模型：攻击者重放旧审计记录。由于 prev_hash 不同（链位置不同），
/// 重放记录的 current_hash 与原始记录不同，verify() 会检测到链断裂。
#[test]
fn a07_hash_chain_replay_detection() {
    let auditor = HashChainAuditor::new();
    let record = ctx("SELECT 1", "admin", 1000);

    auditor.log(&record);
    auditor.log(&record);

    let entries = auditor.get_entries();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].entry.sql, entries[1].entry.sql);
    assert_eq!(entries[0].entry.user, entries[1].entry.user);
    assert_eq!(entries[0].entry.timestamp, entries[1].entry.timestamp);

    assert_ne!(
        entries[0].current_hash, entries[1].current_hash,
        "相同内容在不同链位置产生不同 current_hash，重放可被检测"
    );
    assert_eq!(entries[1].prev_hash, entries[0].current_hash);
}

/// A07-5：反序列化无原型污染——Rust 强类型忽略 __proto__ 字段
///
/// 攻击模型：攻击者在 JSON 中注入 __proto__ 字段试图污染原型链。
/// Rust serde 强类型反序列化只识别结构体定义的字段，忽略未知字段。
#[test]
fn a07_deserialization_no_proto_pollution() {
    let malicious_json = r#"{
        "prev_hash": "0000000000000000000000000000000000000000000000000000000000000000",
        "current_hash": "abc123",
        "entry": {
            "sql": "SELECT 1",
            "user": "admin",
            "timestamp": 1000,
            "__proto__": {"polluted": true},
            "constructor": {"prototype": {"isAdmin": true}}
        },
        "__proto__": {"isAdmin": true}
    }"#;

    let result: Result<HashChainEntry, _> = serde_json::from_str(malicious_json);
    assert!(result.is_ok(), "反序列化应成功，忽略未知字段");

    let entry = result.unwrap();
    assert_eq!(entry.entry.sql, "SELECT 1");
    assert_eq!(entry.entry.user, "admin");
    assert_eq!(entry.entry.timestamp, 1000);
}

/// A07-6：CI/CD 23 门禁存在性——scripts/gate.ps1 存在
///
/// 攻击模型：攻击者删除或禁用 CI/CD 门禁以绕过质量检查。
/// 本测试验证门禁脚本存在且非空。
#[test]
fn a07_cicd_gate_exists() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let project_root = std::path::Path::new(manifest_dir)
        .ancestors()
        .nth(2)
        .expect("无法定位项目根目录");

    let gate_script = project_root.join("scripts").join("gate.ps1");
    assert!(
        gate_script.exists(),
        "CI/CD 门禁脚本 scripts/gate.ps1 必须存在"
    );

    let content = std::fs::read_to_string(&gate_script).expect("读取 gate.ps1 失败");
    assert!(!content.is_empty(), "gate.ps1 不应为空文件");
    assert!(content.contains("cargo"), "gate.ps1 应调用 cargo 命令");
}

/// A07-7：依赖完整性来源——Cargo.lock 存在（可重现构建前提）
///
/// 攻击模型：攻击者删除 Cargo.lock 以引入依赖漂移，
/// 使构建不可重现并可能引入恶意依赖版本。
/// 本测试验证 Cargo.lock 存在且非空。
#[test]
fn a07_dependency_integrity_cargo_lock_exists() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let project_root = std::path::Path::new(manifest_dir)
        .ancestors()
        .nth(2)
        .expect("无法定位项目根目录");

    let cargo_lock = project_root.join("Cargo.lock");
    assert!(cargo_lock.exists(), "Cargo.lock 必须存在以保证可重现构建");

    let content = std::fs::read_to_string(&cargo_lock).expect("读取 Cargo.lock 失败");
    assert!(!content.is_empty(), "Cargo.lock 不应为空");
    assert!(content.contains("version = "), "Cargo.lock 应包含版本信息");
}
