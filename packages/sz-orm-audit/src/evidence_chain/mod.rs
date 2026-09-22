//! v8.0.0 合规证据链导出
//!
//! 收集审计记录 → 哈希链校验 → 脱敏 → 关联合规标准（GDPR/CCPA/PIPL/SOC2）→ 导出加密证据包。

pub mod evidence_exporter;

pub use evidence_exporter::{
    ComplianceEvidenceExporter, ComplianceStandard, EvidenceEntry, EvidenceExportConfig,
    EvidencePackage,
};
