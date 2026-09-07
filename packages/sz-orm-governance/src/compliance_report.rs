//! 合规报告生成（v6.7.0 TASK-6.6）
//!
//! 聚合审计摘要 / 权限变更 / 加密字段清单 / 脱敏策略清单，
//! 生成 Markdown 或 JSON 格式的统一合规报告。
//!
//! 设计原则：自包含，不跨包依赖。调用方从 `sz-orm-audit`/
//! `sz-orm-auth`/`sz-orm-masking`/`sz-orm-core::field_cipher`
//! 提取数据后传入聚合结构，本模块负责格式化输出。

use serde::{Deserialize, Serialize};

/// 报告格式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReportFormat {
    /// Markdown 格式
    Markdown,
    /// JSON 格式
    Json,
}

/// 审计摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditSummary {
    /// 审计日志总条数
    pub total_entries: usize,
    /// 失败操作数
    pub failed_operations: usize,
    /// 链式哈希校验结果（true=通过）
    pub chain_intact: bool,
    /// 涉及的表列表
    pub tables: Vec<String>,
    /// 时间范围起始（ISO 8601）
    pub period_start: String,
    /// 时间范围结束（ISO 8601）
    pub period_end: String,
}

/// 权限变更记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionChange {
    /// 角色 ID
    pub role_id: String,
    /// 变更类型（grant/revoke）
    pub change_type: String,
    /// 权限目标（表名/操作）
    pub target: String,
    /// 操作主体（用户 ID）
    pub actor: String,
    /// 变更时间（ISO 8601）
    pub timestamp: String,
}

/// 加密字段清单条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedFieldEntry {
    /// 表名
    pub table: String,
    /// 字段名
    pub field: String,
    /// 加密算法
    pub algorithm: String,
    /// 密钥 ID
    pub key_id: String,
}

/// 脱敏策略清单条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaskingPolicyEntry {
    /// 表名
    pub table: String,
    /// 字段名
    pub field: String,
    /// 脱敏规则（phone/email/idcard/name/address/custom）
    pub rule: String,
    /// 适用角色
    pub applicable_roles: Vec<String>,
}

/// 合规报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceReport {
    /// 审计摘要
    pub audit_summary: AuditSummary,
    /// 权限变更记录
    pub permission_changes: Vec<PermissionChange>,
    /// 加密字段清单
    pub encrypted_fields: Vec<EncryptedFieldEntry>,
    /// 脱敏策略清单
    pub masking_policies: Vec<MaskingPolicyEntry>,
}

/// 报告生成错误
#[derive(Debug, thiserror::Error)]
pub enum ReportError {
    #[error("序列化失败: {0}")]
    SerializeFailed(String),
    #[error("审计摘要缺失")]
    AuditSummaryMissing,
}

/// 合规报告生成器
///
/// 聚合审计摘要、权限变更、加密字段清单、脱敏策略清单，
/// 生成 Markdown 或 JSON 格式的统一合规报告。
///
/// # 示例
///
/// ```
/// use sz_orm_governance::compliance_report::*;
///
/// let generator = ComplianceReportGenerator::new();
/// let summary = AuditSummary {
///     total_entries: 100,
///     failed_operations: 2,
///     chain_intact: true,
///     tables: vec!["users".into()],
///     period_start: "2026-09-01T00:00:00Z".into(),
///     period_end: "2026-09-07T00:00:00Z".into(),
/// };
/// let report = generator.generate(&summary, &[], &[], &[], ReportFormat::Json).unwrap();
/// assert!(report.contains("\"total_entries\":100"));
/// ```
pub struct ComplianceReportGenerator;

impl ComplianceReportGenerator {
    pub fn new() -> Self {
        Self
    }

    /// 生成合规报告
    ///
    /// 将审计摘要、权限变更、加密字段清单、脱敏策略清单
    /// 聚合为统一报告，按指定格式输出。
    pub fn generate(
        &self,
        audit_summary: &AuditSummary,
        permission_changes: &[PermissionChange],
        encrypted_fields: &[EncryptedFieldEntry],
        masking_policies: &[MaskingPolicyEntry],
        format: ReportFormat,
    ) -> Result<String, ReportError> {
        let report = ComplianceReport {
            audit_summary: audit_summary.clone(),
            permission_changes: permission_changes.to_vec(),
            encrypted_fields: encrypted_fields.to_vec(),
            masking_policies: masking_policies.to_vec(),
        };
        match format {
            ReportFormat::Json => Self::to_json(&report),
            ReportFormat::Markdown => Ok(Self::to_markdown(&report)),
        }
    }

    fn to_json(report: &ComplianceReport) -> Result<String, ReportError> {
        serde_json::to_string(report).map_err(|e| ReportError::SerializeFailed(e.to_string()))
    }

    fn to_markdown(report: &ComplianceReport) -> String {
        let mut md = String::with_capacity(2048);
        md.push_str("# 合规审计报告\n\n");

        md.push_str("## 1. 审计摘要\n\n");
        md.push_str(&format!(
            "- 审计日志总条数: {}\n",
            report.audit_summary.total_entries
        ));
        md.push_str(&format!(
            "- 失败操作数: {}\n",
            report.audit_summary.failed_operations
        ));
        md.push_str(&format!(
            "- 链式哈希校验: {}\n",
            if report.audit_summary.chain_intact {
                "✅ 通过"
            } else {
                "❌ 失败"
            }
        ));
        md.push_str(&format!(
            "- 涉及表: {}\n",
            report.audit_summary.tables.join(", ")
        ));
        md.push_str(&format!(
            "- 时间范围: {} ~ {}\n\n",
            report.audit_summary.period_start, report.audit_summary.period_end
        ));

        md.push_str("## 2. 权限变更记录\n\n");
        if report.permission_changes.is_empty() {
            md.push_str("（无权限变更记录）\n\n");
        } else {
            md.push_str("| 角色 | 变更类型 | 目标 | 操作主体 | 时间 |\n");
            md.push_str("|------|----------|------|----------|------|\n");
            for c in &report.permission_changes {
                md.push_str(&format!(
                    "| {} | {} | {} | {} | {} |\n",
                    c.role_id, c.change_type, c.target, c.actor, c.timestamp
                ));
            }
            md.push('\n');
        }

        md.push_str("## 3. 加密字段清单\n\n");
        if report.encrypted_fields.is_empty() {
            md.push_str("（无加密字段）\n\n");
        } else {
            md.push_str("| 表 | 字段 | 算法 | 密钥 ID |\n");
            md.push_str("|----|------|------|---------|\n");
            for e in &report.encrypted_fields {
                md.push_str(&format!(
                    "| {} | {} | {} | {} |\n",
                    e.table, e.field, e.algorithm, e.key_id
                ));
            }
            md.push('\n');
        }

        md.push_str("## 4. 脱敏策略清单\n\n");
        if report.masking_policies.is_empty() {
            md.push_str("（无脱敏策略）\n\n");
        } else {
            md.push_str("| 表 | 字段 | 规则 | 适用角色 |\n");
            md.push_str("|----|------|------|----------|\n");
            for m in &report.masking_policies {
                md.push_str(&format!(
                    "| {} | {} | {} | {} |\n",
                    m.table,
                    m.field,
                    m.rule,
                    m.applicable_roles.join(", ")
                ));
            }
            md.push('\n');
        }

        md
    }
}

impl Default for ComplianceReportGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_summary() -> AuditSummary {
        AuditSummary {
            total_entries: 150,
            failed_operations: 3,
            chain_intact: true,
            tables: vec!["users".into(), "orders".into()],
            period_start: "2026-09-01T00:00:00Z".into(),
            period_end: "2026-09-07T00:00:00Z".into(),
        }
    }

    fn sample_permission_changes() -> Vec<PermissionChange> {
        vec![
            PermissionChange {
                role_id: "analyst".into(),
                change_type: "grant".into(),
                target: "orders:SELECT".into(),
                actor: "admin".into(),
                timestamp: "2026-09-03T10:00:00Z".into(),
            },
            PermissionChange {
                role_id: "guest".into(),
                change_type: "revoke".into(),
                target: "users:SELECT".into(),
                actor: "admin".into(),
                timestamp: "2026-09-04T12:00:00Z".into(),
            },
        ]
    }

    fn sample_encrypted_fields() -> Vec<EncryptedFieldEntry> {
        vec![
            EncryptedFieldEntry {
                table: "users".into(),
                field: "phone".into(),
                algorithm: "AES-256-GCM".into(),
                key_id: "k-2026-09".into(),
            },
            EncryptedFieldEntry {
                table: "users".into(),
                field: "id_card".into(),
                algorithm: "AES-256-GCM".into(),
                key_id: "k-2026-09".into(),
            },
        ]
    }

    fn sample_masking_policies() -> Vec<MaskingPolicyEntry> {
        vec![
            MaskingPolicyEntry {
                table: "users".into(),
                field: "phone".into(),
                rule: "phone".into(),
                applicable_roles: vec!["guest".into(), "analyst".into()],
            },
            MaskingPolicyEntry {
                table: "users".into(),
                field: "email".into(),
                rule: "email".into(),
                applicable_roles: vec!["guest".into()],
            },
        ]
    }

    #[test]
    fn generate_json_report() {
        let gen = ComplianceReportGenerator::new();
        let report = gen
            .generate(
                &sample_summary(),
                &sample_permission_changes(),
                &sample_encrypted_fields(),
                &sample_masking_policies(),
                ReportFormat::Json,
            )
            .unwrap();
        assert!(report.contains("\"total_entries\":150"));
        assert!(report.contains("\"failed_operations\":3"));
        assert!(report.contains("\"chain_intact\":true"));
        assert!(report.contains("\"role_id\":\"analyst\""));
        assert!(report.contains("\"algorithm\":\"AES-256-GCM\""));
        assert!(report.contains("\"rule\":\"phone\""));
    }

    #[test]
    fn generate_markdown_report() {
        let gen = ComplianceReportGenerator::new();
        let report = gen
            .generate(
                &sample_summary(),
                &sample_permission_changes(),
                &sample_encrypted_fields(),
                &sample_masking_policies(),
                ReportFormat::Markdown,
            )
            .unwrap();
        assert!(report.contains("# 合规审计报告"));
        assert!(report.contains("## 1. 审计摘要"));
        assert!(report.contains("审计日志总条数: 150"));
        assert!(report.contains("✅ 通过"));
        assert!(report.contains("## 2. 权限变更记录"));
        assert!(report.contains("| analyst | grant | orders:SELECT"));
        assert!(report.contains("## 3. 加密字段清单"));
        assert!(report.contains("| users | phone | AES-256-GCM | k-2026-09 |"));
        assert!(report.contains("## 4. 脱敏策略清单"));
        assert!(report.contains("| users | phone | phone | guest, analyst |"));
    }

    #[test]
    fn empty_report_markdown() {
        let gen = ComplianceReportGenerator::new();
        let summary = AuditSummary {
            total_entries: 0,
            failed_operations: 0,
            chain_intact: true,
            tables: vec![],
            period_start: "2026-09-07T00:00:00Z".into(),
            period_end: "2026-09-07T00:00:00Z".into(),
        };
        let report = gen
            .generate(&summary, &[], &[], &[], ReportFormat::Markdown)
            .unwrap();
        assert!(report.contains("（无权限变更记录）"));
        assert!(report.contains("（无加密字段）"));
        assert!(report.contains("（无脱敏策略）"));
    }

    #[test]
    fn empty_report_json() {
        let gen = ComplianceReportGenerator::new();
        let summary = AuditSummary {
            total_entries: 0,
            failed_operations: 0,
            chain_intact: true,
            tables: vec![],
            period_start: "".into(),
            period_end: "".into(),
        };
        let report = gen
            .generate(&summary, &[], &[], &[], ReportFormat::Json)
            .unwrap();
        let parsed: ComplianceReport = serde_json::from_str(&report).unwrap();
        assert_eq!(parsed.permission_changes.len(), 0);
        assert_eq!(parsed.encrypted_fields.len(), 0);
        assert_eq!(parsed.masking_policies.len(), 0);
    }

    #[test]
    fn chain_broken_flag_in_markdown() {
        let gen = ComplianceReportGenerator::new();
        let summary = AuditSummary {
            total_entries: 10,
            failed_operations: 5,
            chain_intact: false,
            tables: vec!["sensitive".into()],
            period_start: "2026-09-01T00:00:00Z".into(),
            period_end: "2026-09-07T00:00:00Z".into(),
        };
        let report = gen
            .generate(&summary, &[], &[], &[], ReportFormat::Markdown)
            .unwrap();
        assert!(report.contains("❌ 失败"));
    }

    #[test]
    fn json_roundtrip() {
        let gen = ComplianceReportGenerator::new();
        let report_json = gen
            .generate(
                &sample_summary(),
                &sample_permission_changes(),
                &sample_encrypted_fields(),
                &sample_masking_policies(),
                ReportFormat::Json,
            )
            .unwrap();
        let parsed: ComplianceReport = serde_json::from_str(&report_json).unwrap();
        assert_eq!(parsed.audit_summary.total_entries, 150);
        assert_eq!(parsed.permission_changes.len(), 2);
        assert_eq!(parsed.encrypted_fields.len(), 2);
        assert_eq!(parsed.masking_policies.len(), 2);
    }

    #[test]
    fn default_generator_equals_new() {
        let a = ComplianceReportGenerator::new();
        let b = ComplianceReportGenerator::default();
        let summary = sample_summary();
        let ra = a
            .generate(&summary, &[], &[], &[], ReportFormat::Json)
            .unwrap();
        let rb = b
            .generate(&summary, &[], &[], &[], ReportFormat::Json)
            .unwrap();
        assert_eq!(ra, rb);
    }

    #[test]
    fn report_format_serde() {
        let json = serde_json::to_string(&ReportFormat::Markdown).unwrap();
        assert_eq!(json, "\"Markdown\"");
        let parsed: ReportFormat = serde_json::from_str("\"Json\"").unwrap();
        assert_eq!(parsed, ReportFormat::Json);
    }
}
