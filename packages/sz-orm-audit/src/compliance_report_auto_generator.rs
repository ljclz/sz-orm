//! v8.1.0 合规报告自动生成器
//!
//! 到周期 → 汇总合规状态 → 脱敏处理 → 生成报告 ≤ 30s。
//! 复用 v8.0.0 `compliance_report.rs` 的 `ComplianceReportGenerator`。
//! 依赖 `compliance-auto-scan` 扫描结果。

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::compliance_report::{ComplianceFramework, ComplianceReport, ComplianceReportGenerator};
use crate::compliance_scan_desensitizer::{ComplianceScanDesensitizer, DesensitizedReport};
use crate::compliance_scan_engine::ComplianceScanReport;

/// 报告格式
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReportFormat {
    Json,
    Html,
    Markdown,
}

/// 自动生成的合规报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoComplianceReport {
    pub framework: String,
    pub generated_at: i64,
    pub overall_status: String,
    pub violation_count: usize,
    pub fix_suggestion_count: usize,
    pub evidence_refs: Vec<String>,
    pub report_body: String,
    pub data_sufficient: bool,
}

/// 报告生成错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReportGenError {
    ReportDataInsufficient(String),
    ReportGenFailed(String),
}

impl std::fmt::Display for ReportGenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReportDataInsufficient(msg) => {
                write!(f, "REPORT_DATA_INSUFFICIENT: {}", msg)
            }
            Self::ReportGenFailed(msg) => write!(f, "Report generation failed: {}", msg),
        }
    }
}

impl std::error::Error for ReportGenError {}

/// 合规报告自动生成器
///
/// 到周期 → 汇总合规状态 → 脱敏 → 生成报告 ≤ 30s。
/// 数据不足标记 `REPORT_DATA_INSUFFICIENT`。
pub struct ComplianceReportAutoGenerator {
    framework: ComplianceFramework,
    period: Duration,
    format: ReportFormat,
    desensitizer: ComplianceScanDesensitizer,
}

impl ComplianceReportAutoGenerator {
    pub fn new(framework: ComplianceFramework, format: ReportFormat) -> Self {
        Self {
            framework,
            period: Duration::from_secs(86400),
            format,
            desensitizer: ComplianceScanDesensitizer::new(false),
        }
    }

    pub fn with_period(mut self, period: Duration) -> Self {
        self.period = period;
        self
    }

    pub fn with_admin(mut self, is_admin: bool) -> Self {
        self.desensitizer = ComplianceScanDesensitizer::new(is_admin);
        self
    }

    pub fn period(&self) -> Duration {
        self.period
    }

    /// 生成合规报告
    ///
    /// 汇总扫描结果 → 脱敏 → 生成报告 ≤ 30s。
    /// 扫描结果为空时标记 `REPORT_DATA_INSUFFICIENT`。
    pub async fn generate(
        &self,
        scan_report: &ComplianceScanReport,
    ) -> Result<AutoComplianceReport, ReportGenError> {
        let start = Instant::now();
        let timeout = Duration::from_secs(30);

        if scan_report.scanned_standards.is_empty() {
            return Err(ReportGenError::ReportDataInsufficient(
                "无扫描结果，无法生成报告".to_string(),
            ));
        }

        let desensitized = self
            .desensitizer
            .desensitize(scan_report)
            .map_err(|e| ReportGenError::ReportGenFailed(e.to_string()))?;

        let base_report = ComplianceReportGenerator::new().generate(self.framework.clone());

        let report_body = match self.format {
            ReportFormat::Json => self.render_json(&base_report, &desensitized),
            ReportFormat::Html => self.render_html(&base_report, &desensitized),
            ReportFormat::Markdown => self.render_markdown(&base_report, &desensitized),
        };

        let elapsed = start.elapsed();
        if elapsed > timeout {
            return Err(ReportGenError::ReportGenFailed(
                "报告生成超时 (>30s)".to_string(),
            ));
        }

        let evidence_refs = base_report
            .clauses
            .iter()
            .flat_map(|c| c.evidence.iter().map(|e| e.reference()))
            .collect();

        Ok(AutoComplianceReport {
            framework: self.framework.name().to_string(),
            generated_at: base_report.generated_at,
            overall_status: desensitized.compliance_status,
            violation_count: desensitized.violations.len(),
            fix_suggestion_count: desensitized.fix_suggestions.len(),
            evidence_refs,
            report_body,
            data_sufficient: true,
        })
    }

    fn render_json(&self, base: &ComplianceReport, desensitized: &DesensitizedReport) -> String {
        serde_json::json!({
            "framework": base.framework.name(),
            "generated_at": base.generated_at,
            "overall_status": desensitized.compliance_status,
            "violations": desensitized.violations,
            "fix_suggestions": desensitized.fix_suggestions,
            "evidence_verified": base.evidence_verified,
        })
        .to_string()
    }

    fn render_html(&self, base: &ComplianceReport, desensitized: &DesensitizedReport) -> String {
        let mut html = String::new();
        html.push_str("<!DOCTYPE html><html><head><title>Compliance Report</title></head><body>");
        html.push_str(&format!(
            "<h1>{} Compliance Report</h1>",
            base.framework.name()
        ));
        html.push_str(&format!(
            "<p>Status: {}</p>",
            desensitized.compliance_status
        ));
        html.push_str(&format!(
            "<p>Violations: {}</p>",
            desensitized.violations.len()
        ));
        html.push_str("<ul>");
        for v in &desensitized.violations {
            html.push_str(&format!(
                "<li>{}: {} ({})</li>",
                v.clause_id, v.description, v.severity
            ));
        }
        html.push_str("</ul>");
        html.push_str("</body></html>");
        html
    }

    fn render_markdown(
        &self,
        base: &ComplianceReport,
        desensitized: &DesensitizedReport,
    ) -> String {
        let mut md = String::new();
        md.push_str(&format!(
            "# {} Compliance Report\n\n",
            base.framework.name()
        ));
        md.push_str(&format!(
            "Status: {}\nViolations: {}\n\n",
            desensitized.compliance_status,
            desensitized.violations.len()
        ));
        for v in &desensitized.violations {
            md.push_str(&format!(
                "- **{}**: {} ({})\n",
                v.clause_id, v.description, v.severity
            ));
        }
        md
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compliance_scan_engine::{
        ComplianceScanEngine, ComplianceScanInput, ComplianceScanStatus, ComplianceScope, ScanItem,
    };
    use crate::evidence_chain::ComplianceStandard;
    use crate::hash_chain_enhanced::HashChainEnhancedAuditor;
    use std::sync::Arc;

    async fn make_scan_report() -> ComplianceScanReport {
        let auditor = Arc::new(HashChainEnhancedAuditor::new());
        let engine = ComplianceScanEngine::new(
            vec![ComplianceStandard::Gdpr],
            ComplianceScope::Code,
            auditor,
        );
        let input = ComplianceScanInput {
            items: vec![ScanItem {
                location: "src/auth.rs".to_string(),
                content: "password_plain".to_string(),
            }],
        };
        engine.scan(&input).await.unwrap()
    }

    fn make_empty_scan_report() -> ComplianceScanReport {
        ComplianceScanReport {
            violations: vec![],
            fix_suggestions: vec![],
            compliance_status: ComplianceScanStatus::Compliant,
            scan_duration: Duration::from_millis(1),
            scanned_standards: vec![],
        }
    }

    #[tokio::test]
    async fn generate_json_report() {
        let scan = make_scan_report().await;
        let gen = ComplianceReportAutoGenerator::new(ComplianceFramework::Gdpr, ReportFormat::Json);
        let report = gen.generate(&scan).await.unwrap();
        assert_eq!(report.framework, "GDPR");
        assert!(report.data_sufficient);
        assert!(report.report_body.contains("GDPR"));
    }

    #[tokio::test]
    async fn generate_html_report() {
        let scan = make_scan_report().await;
        let gen = ComplianceReportAutoGenerator::new(ComplianceFramework::Gdpr, ReportFormat::Html);
        let report = gen.generate(&scan).await.unwrap();
        assert!(report.report_body.contains("<html>"));
        assert!(report.report_body.contains("GDPR"));
    }

    #[tokio::test]
    async fn generate_markdown_report() {
        let scan = make_scan_report().await;
        let gen =
            ComplianceReportAutoGenerator::new(ComplianceFramework::Gdpr, ReportFormat::Markdown);
        let report = gen.generate(&scan).await.unwrap();
        assert!(report.report_body.contains("# GDPR"));
    }

    #[tokio::test]
    async fn generate_data_insufficient() {
        let scan = make_empty_scan_report();
        let gen = ComplianceReportAutoGenerator::new(ComplianceFramework::Gdpr, ReportFormat::Json);
        let err = gen.generate(&scan).await.unwrap_err();
        assert_eq!(
            err,
            ReportGenError::ReportDataInsufficient("无扫描结果，无法生成报告".to_string())
        );
    }

    #[tokio::test]
    async fn generate_admin_keeps_original() {
        let scan = make_scan_report().await;
        let gen = ComplianceReportAutoGenerator::new(ComplianceFramework::Gdpr, ReportFormat::Json)
            .with_admin(true);
        let report = gen.generate(&scan).await.unwrap();
        assert!(report.data_sufficient);
    }

    #[tokio::test]
    async fn generate_within_30s() {
        let scan = make_scan_report().await;
        let gen = ComplianceReportAutoGenerator::new(ComplianceFramework::Gdpr, ReportFormat::Json);
        let start = Instant::now();
        gen.generate(&scan).await.unwrap();
        assert!(start.elapsed().as_secs() < 30);
    }

    #[tokio::test]
    async fn generate_evidence_refs() {
        let scan = make_scan_report().await;
        let gen = ComplianceReportAutoGenerator::new(ComplianceFramework::Gdpr, ReportFormat::Json);
        let report = gen.generate(&scan).await.unwrap();
        // GDPR 报告应包含证据引用
        assert!(!report.evidence_refs.is_empty());
    }

    #[tokio::test]
    async fn generate_violation_count() {
        let scan = make_scan_report().await;
        let gen = ComplianceReportAutoGenerator::new(ComplianceFramework::Gdpr, ReportFormat::Json);
        let report = gen.generate(&scan).await.unwrap();
        assert_eq!(report.violation_count, scan.violations.len());
    }
}
