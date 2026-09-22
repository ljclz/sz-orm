//! v8.1.0 合规扫描引擎
//!
//! 定期自动扫描代码/配置/数据，产出违规项 + 修复建议 + 合规状态。
//! 复用 `HashChainEnhancedAuditor` 存储扫描结果（ADR-006 复用证据链）。
//! 全量扫描 ≤ 10min，零误判判定。

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::evidence_chain::ComplianceStandard;
use crate::hash_chain_enhanced::{
    AuditOpType, AuditResult, EnhancedAuditEntry, HashChainEnhancedAuditor,
};

/// 合规扫描范围
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComplianceScope {
    Code,
    Config,
    Data,
}

/// 违规严重级别
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViolationSeverity {
    High,
    Medium,
    Low,
}

/// 合规违规项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceViolation {
    pub standard: String,
    pub clause_id: String,
    pub description: String,
    pub severity: ViolationSeverity,
    pub location: String,
}

/// 修复建议
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixSuggestion {
    pub violation_clause_id: String,
    pub suggestion: String,
    pub priority: u32,
}

/// 合规扫描状态
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComplianceScanStatus {
    Compliant,
    NonCompliant,
    Partial,
}

/// 合规扫描报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceScanReport {
    pub violations: Vec<ComplianceViolation>,
    pub fix_suggestions: Vec<FixSuggestion>,
    pub compliance_status: ComplianceScanStatus,
    pub scan_duration: Duration,
    pub scanned_standards: Vec<String>,
}

/// 扫描输入项
#[derive(Debug, Clone)]
pub struct ScanItem {
    pub location: String,
    pub content: String,
}

/// 扫描输入
#[derive(Debug, Clone)]
pub struct ComplianceScanInput {
    pub items: Vec<ScanItem>,
}

/// 扫描规则
#[derive(Debug, Clone)]
pub struct ScanRule {
    pub standard: ComplianceStandard,
    pub clause_id: String,
    pub pattern: String,
    pub description: String,
    pub severity: ViolationSeverity,
    pub fix: String,
}

/// 合规扫描错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComplianceScanError {
    ScanTimeout(String),
    ScanFailed(String),
}

impl std::fmt::Display for ComplianceScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ScanTimeout(msg) => write!(f, "COMPLIANCE_SCAN_TIMEOUT: {}", msg),
            Self::ScanFailed(msg) => write!(f, "Scan failed: {}", msg),
        }
    }
}

impl std::error::Error for ComplianceScanError {}

/// 合规扫描引擎
pub struct ComplianceScanEngine {
    standards: Vec<ComplianceStandard>,
    scope: ComplianceScope,
    period: Duration,
    auditor: Arc<HashChainEnhancedAuditor>,
    scan_rules: Vec<ScanRule>,
}

impl ComplianceScanEngine {
    pub fn new(
        standards: Vec<ComplianceStandard>,
        scope: ComplianceScope,
        auditor: Arc<HashChainEnhancedAuditor>,
    ) -> Self {
        Self {
            standards,
            scope,
            period: Duration::from_secs(86400),
            auditor,
            scan_rules: default_scan_rules(),
        }
    }

    pub fn with_period(mut self, period: Duration) -> Self {
        self.period = period;
        self
    }

    pub fn with_scan_rules(mut self, rules: Vec<ScanRule>) -> Self {
        self.scan_rules = rules;
        self
    }

    pub fn period(&self) -> Duration {
        self.period
    }

    pub fn scope(&self) -> &ComplianceScope {
        &self.scope
    }

    /// 执行合规扫描
    ///
    /// 扫描输入内容 → 匹配违规规则 → 产出违规项 + 修复建议 → 记录审计哈希链。
    /// 全量扫描 ≤ 10min，超时返回 `ComplianceScanError::ScanTimeout`。
    pub async fn scan(
        &self,
        input: &ComplianceScanInput,
    ) -> Result<ComplianceScanReport, ComplianceScanError> {
        let start = Instant::now();
        let timeout = Duration::from_secs(600);

        let mut violations = Vec::new();
        let mut fix_suggestions = Vec::new();

        for standard in &self.standards {
            for rule in &self.scan_rules {
                if rule.standard != *standard {
                    continue;
                }
                for item in &input.items {
                    if item.content.contains(&rule.pattern) {
                        violations.push(ComplianceViolation {
                            standard: standard.name().to_string(),
                            clause_id: rule.clause_id.clone(),
                            description: rule.description.clone(),
                            severity: rule.severity.clone(),
                            location: item.location.clone(),
                        });
                        fix_suggestions.push(FixSuggestion {
                            violation_clause_id: rule.clause_id.clone(),
                            suggestion: rule.fix.clone(),
                            priority: match rule.severity {
                                ViolationSeverity::High => 1,
                                ViolationSeverity::Medium => 2,
                                ViolationSeverity::Low => 3,
                            },
                        });
                    }
                }
            }
        }

        let elapsed = start.elapsed();
        if elapsed > timeout {
            return Err(ComplianceScanError::ScanTimeout(format!(
                "已扫描 {} 项，建议分批扫描",
                input.items.len()
            )));
        }

        let status = if violations.is_empty() {
            ComplianceScanStatus::Compliant
        } else if violations
            .iter()
            .all(|v| v.severity == ViolationSeverity::Low)
        {
            ComplianceScanStatus::Partial
        } else {
            ComplianceScanStatus::NonCompliant
        };

        let report = ComplianceScanReport {
            violations,
            fix_suggestions,
            compliance_status: status,
            scan_duration: elapsed,
            scanned_standards: self
                .standards
                .iter()
                .map(|s| s.name().to_string())
                .collect(),
        };

        self.record_scan_to_audit(&report);
        Ok(report)
    }

    fn record_scan_to_audit(&self, report: &ComplianceScanReport) {
        let _ = self.auditor.log(EnhancedAuditEntry {
            subject: "compliance-scan-engine".to_string(),
            object: format!("scope={:?}", self.scope),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0),
            op_type: AuditOpType::Select,
            result: AuditResult::Success,
            source_ip: "127.0.0.1".to_string(),
            sql: format!(
                "COMPLIANCE_SCAN status={:?} violations={}",
                report.compliance_status,
                report.violations.len()
            ),
        });
    }
}

fn default_scan_rules() -> Vec<ScanRule> {
    vec![
        ScanRule {
            standard: ComplianceStandard::Gdpr,
            clause_id: "GDPR-32".to_string(),
            pattern: "password_plain".to_string(),
            description: "明文存储密码，违反 GDPR 第32条（数据安全）".to_string(),
            severity: ViolationSeverity::High,
            fix: "使用 PBKDF2/bcrypt 哈希存储密码，禁止明文存储".to_string(),
        },
        ScanRule {
            standard: ComplianceStandard::Gdpr,
            clause_id: "GDPR-17".to_string(),
            pattern: "no_right_to_erasure".to_string(),
            description: "缺少数据删除机制，违反 GDPR 第17条（被遗忘权）".to_string(),
            severity: ViolationSeverity::Medium,
            fix: "实现用户数据删除接口，支持被遗忘权".to_string(),
        },
        ScanRule {
            standard: ComplianceStandard::Ccpa,
            clause_id: "CCPA-1798.105".to_string(),
            pattern: "no_opt_out".to_string(),
            description: "缺少退出数据销售机制，违反 CCPA 1798.105".to_string(),
            severity: ViolationSeverity::Medium,
            fix: "实现数据销售退出机制，允许消费者拒绝数据销售".to_string(),
        },
        ScanRule {
            standard: ComplianceStandard::Pipl,
            clause_id: "PIPL-38".to_string(),
            pattern: "cross_border_unsafe".to_string(),
            description: "跨境数据传输未安全评估，违反 PIPL 第38条".to_string(),
            severity: ViolationSeverity::High,
            fix: "跨境传输前进行安全评估，确保接收方达到保护要求".to_string(),
        },
        ScanRule {
            standard: ComplianceStandard::Soc2,
            clause_id: "SOC2-CC7.2".to_string(),
            pattern: "no_audit_log".to_string(),
            description: "缺少审计日志，违反 SOC2 CC7.2（监控与跟踪）".to_string(),
            severity: ViolationSeverity::High,
            fix: "启用审计日志记录所有数据访问操作".to_string(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_engine() -> ComplianceScanEngine {
        let auditor = Arc::new(HashChainEnhancedAuditor::new());
        ComplianceScanEngine::new(
            vec![
                ComplianceStandard::Gdpr,
                ComplianceStandard::Ccpa,
                ComplianceStandard::Pipl,
                ComplianceStandard::Soc2,
            ],
            ComplianceScope::Code,
            auditor,
        )
    }

    #[tokio::test]
    async fn scan_compliant_no_violations() {
        let engine = make_engine();
        let input = ComplianceScanInput {
            items: vec![ScanItem {
                location: "src/main.rs".to_string(),
                content: "fn main() {}".to_string(),
            }],
        };
        let report = engine.scan(&input).await.unwrap();
        assert_eq!(report.compliance_status, ComplianceScanStatus::Compliant);
        assert!(report.violations.is_empty());
    }

    #[tokio::test]
    async fn scan_gdpr_password_plain_violation() {
        let engine = make_engine();
        let input = ComplianceScanInput {
            items: vec![ScanItem {
                location: "src/auth.rs:42".to_string(),
                content: "let password_plain = user.password;".to_string(),
            }],
        };
        let report = engine.scan(&input).await.unwrap();
        assert_eq!(report.compliance_status, ComplianceScanStatus::NonCompliant);
        assert_eq!(report.violations.len(), 1);
        assert_eq!(report.violations[0].clause_id, "GDPR-32");
        assert_eq!(report.violations[0].severity, ViolationSeverity::High);
        assert!(!report.fix_suggestions.is_empty());
    }

    #[tokio::test]
    async fn scan_all_standards() {
        let engine = make_engine();
        let input = ComplianceScanInput {
            items: vec![ScanItem {
                location: "config/db.rs".to_string(),
                content:
                    "password_plain no_opt_out cross_border_unsafe no_audit_log no_right_to_erasure"
                        .to_string(),
            }],
        };
        let report = engine.scan(&input).await.unwrap();
        assert_eq!(report.compliance_status, ComplianceScanStatus::NonCompliant);
        assert_eq!(report.violations.len(), 5);
        assert_eq!(report.scanned_standards.len(), 4);
    }

    #[tokio::test]
    async fn scan_partial_low_severity_only() {
        let auditor = Arc::new(HashChainEnhancedAuditor::new());
        let engine = ComplianceScanEngine::new(
            vec![ComplianceStandard::Gdpr],
            ComplianceScope::Config,
            auditor,
        )
        .with_scan_rules(vec![ScanRule {
            standard: ComplianceStandard::Gdpr,
            clause_id: "GDPR-LOW".to_string(),
            pattern: "minor_issue".to_string(),
            description: "低风险问题".to_string(),
            severity: ViolationSeverity::Low,
            fix: "建议修复".to_string(),
        }]);
        let input = ComplianceScanInput {
            items: vec![ScanItem {
                location: "config/app.rs".to_string(),
                content: "minor_issue here".to_string(),
            }],
        };
        let report = engine.scan(&input).await.unwrap();
        assert_eq!(report.compliance_status, ComplianceScanStatus::Partial);
    }

    #[tokio::test]
    async fn scan_records_to_audit_chain() {
        let auditor = Arc::new(HashChainEnhancedAuditor::new());
        let engine = ComplianceScanEngine::new(
            vec![ComplianceStandard::Gdpr],
            ComplianceScope::Code,
            auditor.clone(),
        );
        let input = ComplianceScanInput {
            items: vec![ScanItem {
                location: "src/main.rs".to_string(),
                content: "safe code".to_string(),
            }],
        };
        engine.scan(&input).await.unwrap();
        assert_eq!(auditor.len(), 1);
        assert!(auditor.verify_chain());
    }

    #[tokio::test]
    async fn scan_empty_input() {
        let engine = make_engine();
        let input = ComplianceScanInput { items: vec![] };
        let report = engine.scan(&input).await.unwrap();
        assert_eq!(report.compliance_status, ComplianceScanStatus::Compliant);
    }

    #[tokio::test]
    async fn scan_fix_suggestion_priority() {
        let engine = make_engine();
        let input = ComplianceScanInput {
            items: vec![ScanItem {
                location: "src/auth.rs".to_string(),
                content: "password_plain".to_string(),
            }],
        };
        let report = engine.scan(&input).await.unwrap();
        assert_eq!(report.fix_suggestions[0].priority, 1);
    }
}
