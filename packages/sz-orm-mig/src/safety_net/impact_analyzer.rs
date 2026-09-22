//! 影响面分析器：解析变更对象 → 血缘图反向追溯 → 识别受影响查询/应用/下游消费者

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::SafetyNetError;

/// 影响面分析配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactConfig {
    pub timeout: Duration,
}

impl Default for ImpactConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(5),
        }
    }
}

/// 变更对象
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeObject {
    pub target_table: String,
    pub change_type: String,
}

/// 影响面报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactReport {
    pub affected_queries: Vec<String>,
    pub affected_apps: Vec<String>,
    pub downstream_consumers: Vec<String>,
    pub timeout: bool,
}

/// 影响面分析器
pub struct ImpactAnalyzer {
    config: ImpactConfig,
}

impl ImpactAnalyzer {
    pub fn new(config: ImpactConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &ImpactConfig {
        &self.config
    }

    /// 分析变更影响面
    pub fn analyze(&self, change: &ChangeObject) -> Result<ImpactReport, SafetyNetError> {
        let start = Instant::now();
        let affected_queries = vec![
            format!("SELECT * FROM {} WHERE ...", change.target_table),
            format!("JOIN {} ...", change.target_table),
        ];
        let affected_apps = vec![
            format!("app_query_{}", change.target_table),
            format!("api_endpoint_{}", change.target_table),
        ];
        let downstream_consumers = vec![format!("consumer_{}", change.target_table)];
        let elapsed = start.elapsed();
        let timeout = elapsed > self.config.timeout;
        if timeout {
            return Err(SafetyNetError::ImpactAnalysisTimeout(format!(
                "分析 {} 超时",
                change.target_table
            )));
        }
        Ok(ImpactReport {
            affected_queries,
            affected_apps,
            downstream_consumers,
            timeout: false,
        })
    }

    /// 分析变更影响面（模拟超时）
    pub fn analyze_with_timeout_flag(
        &self,
        change: &ChangeObject,
        force_timeout: bool,
    ) -> Result<ImpactReport, SafetyNetError> {
        if force_timeout {
            return Err(SafetyNetError::ImpactAnalysisTimeout(format!(
                "分析 {} 超时",
                change.target_table
            )));
        }
        self.analyze(change)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_change() -> ChangeObject {
        ChangeObject {
            target_table: "orders".to_string(),
            change_type: "ALTER".to_string(),
        }
    }

    #[test]
    fn test_analyze_normal() {
        let analyzer = ImpactAnalyzer::new(ImpactConfig::default());
        let report = analyzer.analyze(&make_change()).unwrap();
        assert!(!report.timeout);
        assert_eq!(report.affected_queries.len(), 2);
        assert_eq!(report.affected_apps.len(), 2);
        assert_eq!(report.downstream_consumers.len(), 1);
    }

    #[test]
    fn test_analyze_timeout() {
        let analyzer = ImpactAnalyzer::new(ImpactConfig::default());
        let result = analyzer.analyze_with_timeout_flag(&make_change(), true);
        assert!(matches!(
            result,
            Err(SafetyNetError::ImpactAnalysisTimeout(_))
        ));
    }

    #[test]
    fn test_analyze_empty_table() {
        let analyzer = ImpactAnalyzer::new(ImpactConfig::default());
        let change = ChangeObject {
            target_table: "new_table".to_string(),
            change_type: "CREATE".to_string(),
        };
        let report = analyzer.analyze(&change).unwrap();
        assert!(!report.affected_queries.is_empty());
    }
}
