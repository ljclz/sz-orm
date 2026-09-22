//! v8.1.0 合规扫描脱敏器
//!
//! 对扫描结果中的敏感字段脱敏，禁止泄露明文（ADR-008 统一脱敏，admin 排除）。
//! 复用 `mask_sensitive` 既有实现。

use serde::{Deserialize, Serialize};

use crate::compliance_scan_engine::ComplianceScanReport;
use crate::mask_sensitive;

/// 脱敏后的违规项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesensitizedViolation {
    pub standard: String,
    pub clause_id: String,
    pub description: String,
    pub severity: String,
    pub location: String,
}

/// 脱敏后的扫描报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesensitizedReport {
    pub violations: Vec<DesensitizedViolation>,
    pub fix_suggestions: Vec<DesensitizedFixSuggestion>,
    pub compliance_status: String,
    pub scan_duration_ms: u64,
    pub scanned_standards: Vec<String>,
}

/// 脱敏后的修复建议
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesensitizedFixSuggestion {
    pub violation_clause_id: String,
    pub suggestion: String,
    pub priority: u32,
}

/// 脱敏错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DesensitizeError {
    DesensitizeFailed(String),
}

impl std::fmt::Display for DesensitizeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DesensitizeFailed(msg) => write!(f, "Desensitize failed: {}", msg),
        }
    }
}

impl std::error::Error for DesensitizeError {}

/// 合规扫描脱敏器
///
/// 对扫描报告中的敏感字段（password/token/credit_card 等）脱敏。
/// admin 角色可跳过脱敏查看原始数据（ADR-008）。
pub struct ComplianceScanDesensitizer {
    /// 是否为 admin（admin 跳过脱敏）
    is_admin: bool,
}

impl ComplianceScanDesensitizer {
    pub fn new(is_admin: bool) -> Self {
        Self { is_admin }
    }

    /// 对扫描报告脱敏
    ///
    /// admin 直接返回原始数据（位置/描述不脱敏）；
    /// 非 admin 对所有文本字段执行敏感关键词脱敏。
    pub fn desensitize(
        &self,
        report: &ComplianceScanReport,
    ) -> Result<DesensitizedReport, DesensitizeError> {
        if self.is_admin {
            return Ok(self.pass_through(report));
        }

        let violations = report
            .violations
            .iter()
            .map(|v| DesensitizedViolation {
                standard: mask_sensitive(&v.standard),
                clause_id: mask_sensitive(&v.clause_id),
                description: mask_sensitive(&v.description),
                severity: format!("{:?}", v.severity),
                location: mask_sensitive(&v.location),
            })
            .collect();

        let fix_suggestions = report
            .fix_suggestions
            .iter()
            .map(|f| DesensitizedFixSuggestion {
                violation_clause_id: mask_sensitive(&f.violation_clause_id),
                suggestion: mask_sensitive(&f.suggestion),
                priority: f.priority,
            })
            .collect();

        Ok(DesensitizedReport {
            violations,
            fix_suggestions,
            compliance_status: format!("{:?}", report.compliance_status),
            scan_duration_ms: report.scan_duration.as_millis() as u64,
            scanned_standards: report.scanned_standards.clone(),
        })
    }

    fn pass_through(&self, report: &ComplianceScanReport) -> DesensitizedReport {
        DesensitizedReport {
            violations: report
                .violations
                .iter()
                .map(|v| DesensitizedViolation {
                    standard: v.standard.clone(),
                    clause_id: v.clause_id.clone(),
                    description: v.description.clone(),
                    severity: format!("{:?}", v.severity),
                    location: v.location.clone(),
                })
                .collect(),
            fix_suggestions: report
                .fix_suggestions
                .iter()
                .map(|f| DesensitizedFixSuggestion {
                    violation_clause_id: f.violation_clause_id.clone(),
                    suggestion: f.suggestion.clone(),
                    priority: f.priority,
                })
                .collect(),
            compliance_status: format!("{:?}", report.compliance_status),
            scan_duration_ms: report.scan_duration.as_millis() as u64,
            scanned_standards: report.scanned_standards.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compliance_scan_engine::{
        ComplianceScanEngine, ComplianceScanInput, ComplianceScanStatus, ComplianceScope,
        ComplianceViolation, ScanItem, ViolationSeverity,
    };
    use crate::evidence_chain::ComplianceStandard;
    use crate::hash_chain_enhanced::HashChainEnhancedAuditor;
    use std::sync::Arc;

    fn make_report() -> ComplianceScanReport {
        let auditor = Arc::new(HashChainEnhancedAuditor::new());
        let engine = ComplianceScanEngine::new(
            vec![ComplianceStandard::Gdpr],
            ComplianceScope::Code,
            auditor,
        );
        let input = ComplianceScanInput {
            items: vec![ScanItem {
                location: "src/auth.rs:42".to_string(),
                content: "password_plain".to_string(),
            }],
        };
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(engine.scan(&input))
            .unwrap()
    }

    #[test]
    fn desensitize_non_admin_masks_sensitive_fields() {
        let report = make_report();
        let desensitizer = ComplianceScanDesensitizer::new(false);
        let desensitized = desensitizer.desensitize(&report).unwrap();
        assert!(!desensitized.violations.is_empty());
        // 确保不包含明文 password
        let json = serde_json::to_string(&desensitized).unwrap();
        let lower = json.to_ascii_lowercase();
        assert!(
            !lower.contains("password_plain"),
            "脱敏后不应包含明文 password_plain"
        );
    }

    #[test]
    fn desensitize_admin_keeps_original() {
        let report = make_report();
        let original_desc = report.violations[0].description.clone();
        let desensitizer = ComplianceScanDesensitizer::new(true);
        let desensitized = desensitizer.desensitize(&report).unwrap();
        assert_eq!(desensitized.violations[0].description, original_desc);
    }

    #[test]
    fn desensitize_preserves_structure() {
        let report = make_report();
        let desensitizer = ComplianceScanDesensitizer::new(false);
        let desensitized = desensitizer.desensitize(&report).unwrap();
        assert_eq!(desensitized.violations.len(), report.violations.len());
        assert_eq!(
            desensitized.fix_suggestions.len(),
            report.fix_suggestions.len()
        );
    }

    #[test]
    fn desensitize_empty_report() {
        let auditor = Arc::new(HashChainEnhancedAuditor::new());
        let engine = ComplianceScanEngine::new(
            vec![ComplianceStandard::Gdpr],
            ComplianceScope::Code,
            auditor,
        );
        let input = ComplianceScanInput { items: vec![] };
        let report = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(engine.scan(&input))
            .unwrap();
        let desensitizer = ComplianceScanDesensitizer::new(false);
        let desensitized = desensitizer.desensitize(&report).unwrap();
        assert!(desensitized.violations.is_empty());
    }

    #[test]
    fn desensitize_token_keyword_masked() {
        let violation = ComplianceViolation {
            standard: "SOC2".to_string(),
            clause_id: "SOC2-CC7.2".to_string(),
            description: "token leaked in log".to_string(),
            severity: ViolationSeverity::High,
            location: "src/api.rs:10".to_string(),
        };
        let report = ComplianceScanReport {
            violations: vec![violation],
            fix_suggestions: vec![],
            compliance_status: ComplianceScanStatus::NonCompliant,
            scan_duration: std::time::Duration::from_millis(10),
            scanned_standards: vec!["SOC2".to_string()],
        };
        let desensitizer = ComplianceScanDesensitizer::new(false);
        let desensitized = desensitizer.desensitize(&report).unwrap();
        assert!(desensitized.violations[0].description.contains("******"));
    }
}
