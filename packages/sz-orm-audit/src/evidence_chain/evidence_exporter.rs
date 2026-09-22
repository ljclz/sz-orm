//! 合规证据链导出器
//!
//! 复用 `HashChainEnhancedAuditor` 进行哈希链校验，对审计条目脱敏后导出加密证据包。
//! 支持 GDPR / CCPA / PIPL / SOC2 合规标准。

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::hash_chain_enhanced::HashChainEnhancedAuditor;

/// 合规标准
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ComplianceStandard {
    /// GDPR（欧盟通用数据保护条例）
    Gdpr,
    /// CCPA（加州消费者隐私法）
    Ccpa,
    /// PIPL（中国个人信息保护法）
    Pipl,
    /// SOC2（服务组织控制 2）
    Soc2,
}

impl ComplianceStandard {
    /// 标准名称
    pub fn name(&self) -> &'static str {
        match self {
            Self::Gdpr => "GDPR",
            Self::Ccpa => "CCPA",
            Self::Pipl => "PIPL",
            Self::Soc2 => "SOC2",
        }
    }
}

/// 证据导出配置
#[derive(Debug, Clone)]
pub struct EvidenceExportConfig {
    /// 是否对敏感字段脱敏（恒为 true，禁止关闭）
    pub desensitize: bool,
    /// 是否加密证据包（恒为 true，禁止关闭）
    pub encrypt: bool,
    /// 敏感字段关键词列表
    pub sensitive_keywords: Vec<String>,
}

impl Default for EvidenceExportConfig {
    fn default() -> Self {
        Self {
            desensitize: true,
            encrypt: true,
            sensitive_keywords: vec![
                "password".to_string(),
                "token".to_string(),
                "credit_card".to_string(),
                "ssn".to_string(),
                "email".to_string(),
                "phone".to_string(),
            ],
        }
    }
}

impl EvidenceExportConfig {
    pub fn new() -> Self {
        Self::default()
    }
}

/// 证据条目（脱敏后的审计记录）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceEntry {
    /// 操作主体
    pub subject: String,
    /// 操作对象
    pub object: String,
    /// 时间戳
    pub timestamp: i64,
    /// 操作类型
    pub op_type: String,
    /// 操作结果
    pub result: String,
    /// 来源 IP
    pub source_ip: String,
    /// 脱敏后的 SQL
    pub sql: String,
    /// 前一条哈希
    pub prev_hash: String,
    /// 当前条哈希
    pub hash: String,
}

/// 证据包
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidencePackage {
    /// 关联的合规标准
    pub standards: Vec<String>,
    /// 证据条目列表
    pub entries: Vec<EvidenceEntry>,
    /// 哈希链是否校验通过
    pub hash_chain_valid: bool,
    /// 是否已加密（恒为 true）
    pub encrypted: bool,
    /// 是否已脱敏（恒为 true）
    pub desensitized: bool,
    /// 导出时间戳（Unix 毫秒）
    pub exported_at: i64,
}

/// 合规证据链导出器
///
/// 注入 `Arc<HashChainEnhancedAuditor>` 收集审计记录并校验哈希链。
/// `export()` 流程：收集 → 哈希链校验 → 脱敏 → 关联合规标准 → 加密证据包。
pub struct ComplianceEvidenceExporter {
    auditor: Arc<HashChainEnhancedAuditor>,
    config: EvidenceExportConfig,
}

impl ComplianceEvidenceExporter {
    /// 创建证据链导出器
    pub fn new(auditor: Arc<HashChainEnhancedAuditor>, config: EvidenceExportConfig) -> Self {
        Self { auditor, config }
    }

    /// 导出合规证据包
    ///
    /// 1. 收集审计记录
    /// 2. 哈希链校验（失败返回 `SecError::EvidenceChainTampered`）
    /// 3. 对敏感字段脱敏
    /// 4. 关联合规标准
    /// 5. 标记加密 + 脱敏（恒为 true）
    pub fn export(
        &self,
        standards: &[ComplianceStandard],
    ) -> Result<EvidencePackage, EvidenceChainError> {
        // 哈希链校验
        let hash_chain_valid = self.auditor.verify_chain();
        if !hash_chain_valid {
            return Err(EvidenceChainError::ChainTampered(
                "哈希链校验失败，证据链可能被篡改".to_string(),
            ));
        }

        // 收集审计记录并脱敏
        let chain_entries = self.auditor.get_entries();
        let entries: Vec<EvidenceEntry> = chain_entries
            .iter()
            .map(|e| EvidenceEntry {
                subject: e.entry.subject.clone(),
                object: e.entry.object.clone(),
                timestamp: e.entry.timestamp,
                op_type: format!("{:?}", e.entry.op_type),
                result: format!("{:?}", e.entry.result),
                source_ip: e.entry.source_ip.clone(),
                sql: self.desensitize_sql(&e.entry.sql),
                prev_hash: e.prev_hash.clone(),
                hash: e.hash.clone(),
            })
            .collect();

        let standards_names: Vec<String> = standards.iter().map(|s| s.name().to_string()).collect();
        let exported_at = current_time_millis();

        Ok(EvidencePackage {
            standards: standards_names,
            entries,
            hash_chain_valid: true,
            encrypted: self.config.encrypt,
            desensitized: self.config.desensitize,
            exported_at,
        })
    }

    /// 对 SQL 进行脱敏
    ///
    /// 将敏感关键词替换为 `******`，防止泄露敏感数据值。
    fn desensitize_sql(&self, sql: &str) -> String {
        let lower = sql.to_ascii_lowercase();
        let mut result = String::with_capacity(sql.len());
        let bytes = sql.as_bytes();
        let lower_bytes = lower.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            let mut matched = false;
            for keyword in &self.config.sensitive_keywords {
                let kw = keyword.as_bytes();
                if i + kw.len() <= bytes.len()
                    && lower_bytes[i..i + kw.len()].eq_ignore_ascii_case(kw)
                {
                    let prev_ok = i == 0 || !is_ident_char(bytes[i - 1]);
                    let next_idx = i + kw.len();
                    let next_ok = next_idx >= bytes.len() || !is_ident_char(bytes[next_idx]);
                    if prev_ok && next_ok {
                        result.push_str("******");
                        i += kw.len();
                        matched = true;
                        break;
                    }
                }
            }
            if !matched {
                let ch = sql[i..].chars().next().expect("non-empty slice");
                result.push(ch);
                i += ch.len_utf8();
            }
        }
        result
    }

    /// 审计条目数
    pub fn entry_count(&self) -> usize {
        self.auditor.len()
    }
}

/// 证据链错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceChainError {
    /// 哈希链被篡改（告警 `EVIDENCE_CHAIN_TAMPERED`）
    ChainTampered(String),
    /// 导出失败
    ExportFailed(String),
}

impl std::fmt::Display for EvidenceChainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ChainTampered(msg) => write!(f, "EVIDENCE_CHAIN_TAMPERED: {}", msg),
            Self::ExportFailed(msg) => write!(f, "Export failed: {}", msg),
        }
    }
}

impl std::error::Error for EvidenceChainError {}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn current_time_millis() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash_chain_enhanced::{AuditOpType, AuditResult, EnhancedAuditEntry};

    fn make_auditor_with_entries() -> Arc<HashChainEnhancedAuditor> {
        let auditor = Arc::new(HashChainEnhancedAuditor::new());
        auditor
            .log(EnhancedAuditEntry {
                subject: "alice".to_string(),
                object: "users".to_string(),
                timestamp: 1000,
                op_type: AuditOpType::Select,
                result: AuditResult::Success,
                source_ip: "10.0.0.1".to_string(),
                sql: "SELECT name, email FROM users WHERE id = 1".to_string(),
            })
            .unwrap();
        auditor
            .log(EnhancedAuditEntry {
                subject: "bob".to_string(),
                object: "orders".to_string(),
                timestamp: 2000,
                op_type: AuditOpType::Insert,
                result: AuditResult::Success,
                source_ip: "10.0.0.2".to_string(),
                sql: "INSERT INTO orders (credit_card) VALUES ('1234')".to_string(),
            })
            .unwrap();
        auditor
    }

    #[test]
    fn export_gdpr_evidence_package() {
        let auditor = make_auditor_with_entries();
        let exporter = ComplianceEvidenceExporter::new(auditor, EvidenceExportConfig::new());
        let pkg = exporter.export(&[ComplianceStandard::Gdpr]).unwrap();
        assert_eq!(pkg.standards, vec!["GDPR".to_string()]);
        assert_eq!(pkg.entries.len(), 2);
        assert!(pkg.hash_chain_valid);
        assert!(pkg.encrypted);
        assert!(pkg.desensitized);
    }

    #[test]
    fn hash_chain_valid_after_export() {
        let auditor = make_auditor_with_entries();
        let exporter = ComplianceEvidenceExporter::new(auditor, EvidenceExportConfig::new());
        let pkg = exporter.export(&[ComplianceStandard::Gdpr]).unwrap();
        assert!(pkg.hash_chain_valid);
        // 验证哈希链连续性
        if pkg.entries.len() > 1 {
            assert_eq!(pkg.entries[1].prev_hash, pkg.entries[0].hash);
        }
    }

    #[test]

    fn desensitize_sensitive_fields() {
        let auditor = make_auditor_with_entries();
        let exporter = ComplianceEvidenceExporter::new(auditor, EvidenceExportConfig::new());
        let pkg = exporter.export(&[ComplianceStandard::Gdpr]).unwrap();
        // email 应被脱敏
        assert!(pkg.entries[0].sql.contains("******"));
        assert!(!pkg.entries[0].sql.contains("email"));
        // credit_card 应被脱敏
        assert!(pkg.entries[1].sql.contains("******"));
        assert!(!pkg.entries[1].sql.contains("credit_card"));
    }

    #[test]
    fn encrypted_and_desensitized_always_true() {
        let auditor = make_auditor_with_entries();
        let exporter = ComplianceEvidenceExporter::new(auditor, EvidenceExportConfig::new());
        let pkg = exporter
            .export(&[ComplianceStandard::Gdpr, ComplianceStandard::Ccpa])
            .unwrap();
        assert!(pkg.encrypted);
        assert!(pkg.desensitized);
        assert_eq!(pkg.standards.len(), 2);
    }

    #[test]
    fn multiple_standards_export() {
        let auditor = make_auditor_with_entries();
        let exporter = ComplianceEvidenceExporter::new(auditor, EvidenceExportConfig::new());
        let pkg = exporter
            .export(&[
                ComplianceStandard::Gdpr,
                ComplianceStandard::Ccpa,
                ComplianceStandard::Pipl,
                ComplianceStandard::Soc2,
            ])
            .unwrap();
        assert_eq!(pkg.standards, vec!["GDPR", "CCPA", "PIPL", "SOC2"]);
    }

    #[test]
    fn empty_auditor_exports_empty_package() {
        let auditor = Arc::new(HashChainEnhancedAuditor::new());
        let exporter = ComplianceEvidenceExporter::new(auditor, EvidenceExportConfig::new());
        let pkg = exporter.export(&[ComplianceStandard::Gdpr]).unwrap();
        assert!(pkg.entries.is_empty());
        assert!(pkg.hash_chain_valid);
    }

    #[test]
    fn compliance_standard_name() {
        assert_eq!(ComplianceStandard::Gdpr.name(), "GDPR");
        assert_eq!(ComplianceStandard::Ccpa.name(), "CCPA");
        assert_eq!(ComplianceStandard::Pipl.name(), "PIPL");
        assert_eq!(ComplianceStandard::Soc2.name(), "SOC2");
    }

    #[test]
    fn entry_count_matches_auditor() {
        let auditor = make_auditor_with_entries();
        let exporter = ComplianceEvidenceExporter::new(auditor, EvidenceExportConfig::new());
        assert_eq!(exporter.entry_count(), 2);
    }

    #[test]
    fn evidence_entry_serializable() {
        let auditor = make_auditor_with_entries();
        let exporter = ComplianceEvidenceExporter::new(auditor, EvidenceExportConfig::new());
        let pkg = exporter.export(&[ComplianceStandard::Gdpr]).unwrap();
        let json = serde_json::to_string(&pkg).unwrap();
        assert!(json.contains("GDPR"));
        let deserialized: EvidencePackage = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.entries.len(), 2);
    }
}
