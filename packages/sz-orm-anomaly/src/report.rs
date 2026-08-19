//! 报告导出模块：JSON + Markdown 格式
//!
//! 报告含检测时间段、异常列表、统计摘要（总异常数/各类型计数/各严重级别计数）。

use serde::{Deserialize, Serialize};

use crate::alert::Alert;

/// 时间范围
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeRange {
    /// 起始时间戳（毫秒）
    pub start_ms: u64,
    /// 结束时间戳（毫秒）
    pub end_ms: u64,
}

impl TimeRange {
    /// 创建时间范围
    pub fn new(start_ms: u64, end_ms: u64) -> Self {
        Self { start_ms, end_ms }
    }

    /// 检查时间戳是否在范围内
    pub fn contains(&self, timestamp: u64) -> bool {
        timestamp >= self.start_ms && timestamp <= self.end_ms
    }

    /// 持续时间（毫秒）
    pub fn duration_ms(&self) -> u64 {
        self.end_ms.saturating_sub(self.start_ms)
    }
}

/// 报告统计摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportSummary {
    /// 总异常数
    pub total_alerts: usize,
    /// 各类型计数
    pub by_type: std::collections::HashMap<String, usize>,
    /// 各严重级别计数
    pub by_severity: std::collections::HashMap<String, usize>,
    /// 检测时间段
    pub time_range: TimeRange,
}

/// 报告导出器
pub struct ReportExporter;

impl ReportExporter {
    /// 导出 JSON 报告
    ///
    /// JSON 含：检测时间段/异常列表/统计摘要（总异常数/各类型计数/各严重级别计数）
    pub fn export_report_json(alerts: &[Alert], period: TimeRange) -> String {
        let summary = Self::compute_summary(alerts, period);
        let report = serde_json::json!({
            "time_range": {
                "start_ms": period.start_ms,
                "end_ms": period.end_ms,
                "duration_ms": period.duration_ms(),
            },
            "summary": summary,
            "alerts": alerts,
        });
        serde_json::to_string_pretty(&report).unwrap_or_else(|_| "{}".to_string())
    }

    /// 导出 Markdown 报告
    ///
    /// Markdown 含表格 + 异常列表
    pub fn export_report_markdown(alerts: &[Alert], period: TimeRange) -> String {
        let summary = Self::compute_summary(alerts, period);
        let mut md = String::new();

        md.push_str("# 异常检测报告\n\n");
        md.push_str("## 检测时间段\n\n");
        md.push_str(&format!(
            "- 起始时间：{}ms\n- 结束时间：{}ms\n- 持续时间：{}ms\n\n",
            period.start_ms,
            period.end_ms,
            period.duration_ms()
        ));

        md.push_str("## 统计摘要\n\n");
        md.push_str(&format!("- 总异常数：{}\n\n", summary.total_alerts));

        md.push_str("### 按类型统计\n\n");
        md.push_str("| 异常类型 | 数量 |\n|----------|------|\n");
        for (atype, count) in &summary.by_type {
            md.push_str(&format!("| {} | {} |\n", atype, count));
        }

        md.push_str("\n### 按严重级别统计\n\n");
        md.push_str("| 严重级别 | 数量 |\n|----------|------|\n");
        for (sev, count) in &summary.by_severity {
            md.push_str(&format!("| {} | {} |\n", sev, count));
        }

        md.push_str("\n## 异常列表\n\n");
        if alerts.is_empty() {
            md.push_str("无异常告警。\n");
        } else {
            md.push_str("| 时间戳 | 类型 | 严重级别 | 指标值 | 阈值 | 建议操作 |\n");
            md.push_str("|--------|------|----------|--------|------|----------|\n");
            for alert in alerts {
                md.push_str(&format!(
                    "| {} | {} | {} | {:.2} | {:.2} | {} |\n",
                    alert.timestamp,
                    alert.anomaly_type.as_str(),
                    alert.severity.as_str(),
                    alert.metric_value,
                    alert.threshold,
                    alert.suggestion
                ));
            }
        }

        md
    }

    /// 计算统计摘要
    fn compute_summary(alerts: &[Alert], period: TimeRange) -> ReportSummary {
        let mut by_type = std::collections::HashMap::new();
        let mut by_severity = std::collections::HashMap::new();
        for alert in alerts {
            *by_type
                .entry(alert.anomaly_type.as_str().to_string())
                .or_insert(0) += 1;
            *by_severity
                .entry(alert.severity.as_str().to_string())
                .or_insert(0) += 1;
        }
        ReportSummary {
            total_alerts: alerts.len(),
            by_type,
            by_severity,
            time_range: period,
        }
    }

    /// 导出 CSV 报告
    pub fn export_report_csv(alerts: &[Alert]) -> String {
        let mut csv = String::new();
        csv.push_str("timestamp,anomaly_type,severity,metric_value,threshold,suggestion\n");
        for alert in alerts {
            let suggestion_escaped = alert.suggestion.replace(',', ";");
            csv.push_str(&format!(
                "{},{},{},{:.4},{:.4},{}\n",
                alert.timestamp,
                alert.anomaly_type.as_str(),
                alert.severity.as_str(),
                alert.metric_value,
                alert.threshold,
                suggestion_escaped
            ));
        }
        csv
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alert::{AnomalyType, Severity};

    fn sample_alerts() -> Vec<Alert> {
        vec![
            Alert {
                anomaly_type: AnomalyType::SlowQuerySpike,
                severity: Severity::Warn,
                timestamp: 1000,
                metric_value: 20.0,
                threshold: 10.0,
                baseline: None,
                suggestion: "check index".to_string(),
                sql_summary: None,
            },
            Alert {
                anomaly_type: AnomalyType::ErrorRateSpike,
                severity: Severity::Critical,
                timestamp: 2000,
                metric_value: 0.1,
                threshold: 0.05,
                baseline: None,
                suggestion: "check connection".to_string(),
                sql_summary: None,
            },
            Alert {
                anomaly_type: AnomalyType::PoolExhaustion,
                severity: Severity::Critical,
                timestamp: 3000,
                metric_value: 15.0,
                threshold: 10.0,
                baseline: None,
                suggestion: "increase pool size".to_string(),
                sql_summary: None,
            },
        ]
    }

    #[test]
    fn test_time_range_contains() {
        let range = TimeRange::new(1000, 2000);
        assert!(range.contains(1500));
        assert!(range.contains(1000));
        assert!(range.contains(2000));
        assert!(!range.contains(500));
        assert!(!range.contains(2500));
    }

    #[test]
    fn test_time_range_duration() {
        let range = TimeRange::new(1000, 3000);
        assert_eq!(range.duration_ms(), 2000);
    }

    #[test]
    fn test_export_json() {
        let alerts = sample_alerts();
        let period = TimeRange::new(1000, 3000);
        let json = ReportExporter::export_report_json(&alerts, period);
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("JSON should be valid");
        assert_eq!(parsed["summary"]["total_alerts"], 3);
        assert_eq!(parsed["alerts"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn test_export_json_empty() {
        let period = TimeRange::new(1000, 2000);
        let json = ReportExporter::export_report_json(&[], period);
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("JSON should be valid");
        assert_eq!(parsed["summary"]["total_alerts"], 0);
    }

    #[test]
    fn test_export_markdown() {
        let alerts = sample_alerts();
        let period = TimeRange::new(1000, 3000);
        let md = ReportExporter::export_report_markdown(&alerts, period);
        assert!(md.contains("# 异常检测报告"));
        assert!(md.contains("## 检测时间段"));
        assert!(md.contains("## 统计摘要"));
        assert!(md.contains("## 异常列表"));
        assert!(md.contains("slow_query_spike"));
        assert!(md.contains("error_rate_spike"));
        assert!(md.contains("pool_exhaustion"));
    }

    #[test]
    fn test_export_markdown_empty() {
        let period = TimeRange::new(1000, 2000);
        let md = ReportExporter::export_report_markdown(&[], period);
        assert!(md.contains("无异常告警"));
    }

    #[test]
    fn test_export_csv() {
        let alerts = sample_alerts();
        let csv = ReportExporter::export_report_csv(&alerts);
        assert!(csv.contains("timestamp,anomaly_type,severity"));
        assert!(csv.contains("slow_query_spike"));
        assert!(csv.contains("error_rate_spike"));
    }

    #[test]
    fn test_summary_by_type() {
        let alerts = sample_alerts();
        let summary = ReportExporter::compute_summary(&alerts, TimeRange::new(0, 0));
        assert_eq!(summary.total_alerts, 3);
        assert_eq!(summary.by_type.get("slow_query_spike"), Some(&1));
        assert_eq!(summary.by_type.get("error_rate_spike"), Some(&1));
        assert_eq!(summary.by_type.get("pool_exhaustion"), Some(&1));
    }

    #[test]
    fn test_summary_by_severity() {
        let alerts = sample_alerts();
        let summary = ReportExporter::compute_summary(&alerts, TimeRange::new(0, 0));
        assert_eq!(summary.by_severity.get("warn"), Some(&1));
        assert_eq!(summary.by_severity.get("critical"), Some(&2));
    }
}
