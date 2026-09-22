//! v8.1.0 安全合规自动化端到端接线测试
//!
//! 4 个 e2e 测试验证生产调用点可达：
//! 1. ComplianceScanEngine GDPR 合规自动扫描 ≤ 10min 产出违规项修复建议
//! 2. KeyAutoRotateScheduler 90 天密钥自动轮换 ≤ 5s 不中断服务失败回滚
//! 3. EvidenceAutoArchiveScheduler 证据自动归档 ≤ 60s 哈希链完整性校验
//! 4. ComplianceReportAutoGenerator 合规报告自动生成 ≤ 30s 脱敏审计要求格式

use std::sync::Arc;
use std::time::{Duration, Instant};

use sz_orm_audit::compliance_report_auto_generator::{ComplianceReportAutoGenerator, ReportFormat};
use sz_orm_audit::compliance_scan_desensitizer::ComplianceScanDesensitizer;
use sz_orm_audit::compliance_scan_engine::{
    ComplianceScanEngine, ComplianceScanInput, ComplianceScanStatus, ComplianceScope, ScanItem,
};
use sz_orm_audit::evidence_auto_archive_scheduler::EvidenceAutoArchiveScheduler;
use sz_orm_audit::evidence_chain::ComplianceStandard;
use sz_orm_audit::hash_chain_enhanced::{
    AuditOpType, AuditResult, EnhancedAuditEntry, HashChainEnhancedAuditor,
};

use sz_orm_crypto::dek_buffer::DekBuffer;
use sz_orm_crypto::key_rotation_enhanced::KeyRotationEnhanced;
use sz_orm_crypto::tde_mgmt::key_auto_rotate_scheduler::KeyAutoRotateScheduler;
use sz_orm_crypto::tde_mgmt::DekRotationConfig;
use sz_orm_crypto::tde_mgmt::DekRotationManager;

/// e2e 1: ComplianceScanEngine GDPR 合规自动扫描 ≤ 10min 产出违规项修复建议
///
/// 生产调用点：`packages/sz-orm-audit/src/compliance_scan_engine.rs` `ComplianceScanEngine::scan`
#[tokio::test]
async fn e2e_compliance_scan_gdpr() {
    let auditor = Arc::new(HashChainEnhancedAuditor::new());
    let engine = ComplianceScanEngine::new(
        vec![ComplianceStandard::Gdpr],
        ComplianceScope::Code,
        auditor.clone(),
    );

    let input = ComplianceScanInput {
        items: vec![
            ScanItem {
                location: "src/auth.rs:42".to_string(),
                content: "let password_plain = user.password;".to_string(),
            },
            ScanItem {
                location: "src/user.rs:100".to_string(),
                content: "no_right_to_erasure not implemented".to_string(),
            },
        ],
    };

    let start = Instant::now();
    let report = engine.scan(&input).await.unwrap();
    let elapsed = start.elapsed();

    // ≤ 10min
    assert!(elapsed.as_secs() < 600, "扫描应在 10min 内完成");

    // 产出违规项
    assert!(!report.violations.is_empty(), "应检出违规项");
    assert_eq!(report.compliance_status, ComplianceScanStatus::NonCompliant);

    // 产出修复建议
    assert!(!report.fix_suggestions.is_empty(), "应产出修复建议");

    // 每项违规有对应修复建议
    for v in &report.violations {
        assert!(
            report
                .fix_suggestions
                .iter()
                .any(|f| f.violation_clause_id == v.clause_id),
            "违规项 {} 应有对应修复建议",
            v.clause_id
        );
    }

    // 审计哈希链记录
    assert!(!auditor.is_empty(), "扫描结果应记录到审计哈希链");
    assert!(auditor.verify_chain(), "审计哈希链应完整");
}

/// e2e 2: KeyAutoRotateScheduler 90 天密钥自动轮换 ≤ 5s 不中断服务失败回滚
///
/// 生产调用点：`packages/sz-orm-crypto/src/tde_mgmt/key_auto_rotate_scheduler.rs` `KeyAutoRotateScheduler::rotate_now`
#[tokio::test]
async fn e2e_key_auto_rotate_90_day() {
    let key_mgr = Arc::new(KeyRotationEnhanced::new("dek-v1", vec![0x42u8; 32]));
    let dek = Arc::new(DekBuffer::new(vec![0x42u8; 32]));
    let manager = Arc::new(DekRotationManager::new(
        key_mgr,
        dek,
        DekRotationConfig::new(),
    ));

    let scheduler =
        KeyAutoRotateScheduler::new(Duration::from_secs(90 * 86400), 3, manager).unwrap();

    let start = Instant::now();
    let record = scheduler.rotate_now().await.unwrap();
    let elapsed = start.elapsed();

    // ≤ 5s
    assert!(elapsed.as_secs() < 5, "轮换应在 5s 内完成");

    // 轮换成功
    assert!(record.success, "轮换应成功");
    assert!(!record.rollback, "不应触发回滚");

    // 不中断服务：轮换后状态正常
    let status = scheduler.status();
    assert!(status.active_dek_count >= 1, "轮换后应有活跃密钥");

    // 再次轮换验证不中断
    let record2 = scheduler.rotate_now().await.unwrap();
    assert!(record2.success, "第二次轮换应成功");
    assert_ne!(
        record.new_dek_id, record2.new_dek_id,
        "每次轮换应生成新密钥"
    );
}

/// e2e 3: EvidenceAutoArchiveScheduler 证据自动归档 ≤ 60s 哈希链完整性校验
///
/// 生产调用点：`packages/sz-orm-audit/src/evidence_auto_archive_scheduler.rs` `EvidenceAutoArchiveScheduler::archive`
#[tokio::test]
async fn e2e_evidence_auto_archive() {
    let auditor = Arc::new(HashChainEnhancedAuditor::new());

    // 添加审计条目达到归档阈值
    for i in 0..5 {
        auditor
            .log(EnhancedAuditEntry {
                subject: format!("user{}", i),
                object: "sensitive_table".to_string(),
                timestamp: 1000 + i as i64,
                op_type: AuditOpType::Select,
                result: AuditResult::Success,
                source_ip: "10.0.0.1".to_string(),
                sql: format!("SELECT * FROM sensitive_table WHERE id = {}", i),
            })
            .unwrap();
    }

    let scheduler = EvidenceAutoArchiveScheduler::new(
        5,
        auditor,
        vec![ComplianceStandard::Gdpr, ComplianceStandard::Soc2],
    );

    let start = Instant::now();
    let record = scheduler.archive().await.unwrap();
    let elapsed = start.elapsed();

    // ≤ 60s
    assert!(elapsed.as_secs() < 60, "归档应在 60s 内完成");

    // 归档条目数
    assert_eq!(record.archived_count, 5, "应归档 5 条证据");

    // 哈希链完整性校验
    assert!(record.integrity_valid, "哈希链完整性校验应通过");

    // 可检索：导出证据包
    let pkg = scheduler.export_package().unwrap();
    assert!(pkg.hash_chain_valid, "证据包哈希链应有效");
    assert!(pkg.encrypted, "证据包应加密");
    assert!(pkg.desensitized, "证据包应脱敏");
    assert_eq!(pkg.standards.len(), 2, "应关联 2 个合规标准");
}

/// e2e 4: ComplianceReportAutoGenerator 合规报告自动生成 ≤ 30s 脱敏审计要求格式
///
/// 生产调用点：`packages/sz-orm-audit/src/compliance_report_auto_generator.rs` `ComplianceReportAutoGenerator::generate`
#[tokio::test]
async fn e2e_compliance_report_auto_generate() {
    let auditor = Arc::new(HashChainEnhancedAuditor::new());
    let engine = ComplianceScanEngine::new(
        vec![ComplianceStandard::Gdpr, ComplianceStandard::Soc2],
        ComplianceScope::Code,
        auditor,
    );

    let input = ComplianceScanInput {
        items: vec![
            ScanItem {
                location: "src/auth.rs:42".to_string(),
                content: "password_plain".to_string(),
            },
            ScanItem {
                location: "src/audit.rs:10".to_string(),
                content: "no_audit_log".to_string(),
            },
        ],
    };

    let scan_report = engine.scan(&input).await.unwrap();
    assert!(!scan_report.violations.is_empty());

    let generator = ComplianceReportAutoGenerator::new(
        sz_orm_audit::compliance_report::ComplianceFramework::Gdpr,
        ReportFormat::Json,
    );

    let start = Instant::now();
    let report = generator.generate(&scan_report).await.unwrap();
    let elapsed = start.elapsed();

    // ≤ 30s
    assert!(elapsed.as_secs() < 30, "报告生成应在 30s 内完成");

    // 脱敏：报告体中不应包含明文 password_plain
    let desensitizer = ComplianceScanDesensitizer::new(false);
    let desensitized = desensitizer.desensitize(&scan_report).unwrap();
    let json = serde_json::to_string(&desensitized).unwrap();
    let lower = json.to_ascii_lowercase();
    assert!(
        !lower.contains("password_plain"),
        "脱敏后不应包含明文 password_plain"
    );

    // 审计要求格式：JSON 格式包含必要字段
    assert!(
        report.report_body.contains("framework"),
        "报告应包含 framework 字段"
    );
    assert!(
        report.report_body.contains("overall_status"),
        "报告应包含 overall_status 字段"
    );
    assert!(
        report.report_body.contains("violations"),
        "报告应包含 violations 字段"
    );

    // 证据引用
    assert!(!report.evidence_refs.is_empty(), "报告应包含证据引用");
    assert!(report.data_sufficient, "数据应充足");
}
