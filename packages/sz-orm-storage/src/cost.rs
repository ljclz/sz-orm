//! 存储成本分析模块（Cost Analysis）
//!
//! 对应 v4.6.0 REQ-V46-005，tasks.md M7。
//!
//! # 核心概念
//!
//! - **CostAnalyzer**：成本分析器，按 provider/bucket/tier 统计存储成本
//! - **CostOptimizationSuggestion**：四种优化建议（TierDowngrade/LifecycleOptimize/DeleteExpired/CompressCold）
//! - **CostReport**：成本报表（JSON/CSV 格式）
//!
//! # 使用示例
//!
//! ```
//! use sz_orm_storage::cost::{CostAnalyzer, CostConfig, BucketCost};
//!
//! let config = CostConfig::new();
//! let analyzer = CostAnalyzer::new(config);
//! let bucket_costs = vec![
//!     BucketCost {
//!         provider: "s3".to_string(),
//!         bucket: "my-bucket".to_string(),
//!         tier: "Standard".to_string(),
//!         capacity_cost: 100.0,
//!         request_cost: 10.0,
//!         traffic_cost: 5.0,
//!         total_cost: 115.0,
//!         size_gb: 500.0,
//!     },
//! ];
//! ```

use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

// ============================================================================
// ReportFormat — 报表格式
// ============================================================================

/// 成本报表格式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReportFormat {
    /// JSON 格式
    Json,
    /// CSV 格式
    Csv,
}

// ============================================================================
// CostOptimizationSuggestion — 成本优化建议
// ============================================================================

/// 成本优化建议
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CostOptimizationSuggestion {
    /// 存储层降级：冷数据从高成本 tier 降级到低成本 tier
    TierDowngrade {
        /// bucket 名称
        bucket: String,
        /// 原 tier
        from_tier: String,
        /// 目标 tier
        to_tier: String,
        /// 预期节省百分比
        expected_saving_percent: f64,
    },
    /// 生命周期规则优化
    LifecycleOptimize {
        /// bucket 名称
        bucket: String,
        /// 优化描述
        description: String,
    },
    /// 删除过期数据
    DeleteExpired {
        /// bucket 名称
        bucket: String,
        /// 过期数据量
        expired_count: u64,
    },
    /// 压缩冷数据
    CompressCold {
        /// bucket 名称
        bucket: String,
        /// 冷数据大小（GB）
        cold_data_size_gb: f64,
    },
}

// ============================================================================
// CostConfig — 成本分析配置
// ============================================================================

/// 成本分析配置
#[derive(Debug, Clone)]
pub struct CostConfig {
    /// 分析间隔（毫秒），默认每日
    pub analysis_interval_ms: u64,
    /// 报表格式
    pub report_format: ReportFormat,
    /// 要分析的 provider 列表
    pub providers: Vec<String>,
}

impl Default for CostConfig {
    fn default() -> Self {
        Self {
            analysis_interval_ms: 86_400_000,
            report_format: ReportFormat::Json,
            providers: vec![
                "local".to_string(),
                "s3".to_string(),
                "aliyun".to_string(),
                "tencent".to_string(),
                "qiniu".to_string(),
                "huawei".to_string(),
                "upyun".to_string(),
            ],
        }
    }
}

impl CostConfig {
    /// 创建默认配置
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置分析间隔
    pub fn with_analysis_interval_ms(mut self, interval_ms: u64) -> Self {
        self.analysis_interval_ms = interval_ms;
        self
    }

    /// 设置报表格式
    pub fn with_report_format(mut self, format: ReportFormat) -> Self {
        self.report_format = format;
        self
    }

    /// 设置 provider 列表
    pub fn with_providers(mut self, providers: Vec<String>) -> Self {
        self.providers = providers;
        self
    }
}

// ============================================================================
// CostError — 成本分析错误
// ============================================================================

/// 成本分析错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CostError {
    /// provider 不可用
    ProviderUnavailable(String),
    /// 成本数据异常
    AbnormalData(String),
    /// 序列化错误
    SerializationError(String),
}

impl std::fmt::Display for CostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CostError::ProviderUnavailable(msg) => write!(f, "provider unavailable: {}", msg),
            CostError::AbnormalData(msg) => write!(f, "abnormal cost data: {}", msg),
            CostError::SerializationError(msg) => write!(f, "serialization error: {}", msg),
        }
    }
}

impl std::error::Error for CostError {}

// ============================================================================
// BucketCost / ProviderCost / CostReport — 成本报表结构
// ============================================================================

/// bucket 成本
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BucketCost {
    /// provider 名称
    pub provider: String,
    /// bucket 名称
    pub bucket: String,
    /// 存储层
    pub tier: String,
    /// 容量成本
    pub capacity_cost: f64,
    /// 请求成本
    pub request_cost: f64,
    /// 流量成本
    pub traffic_cost: f64,
    /// 总成本
    pub total_cost: f64,
    /// 存储大小（GB）
    pub size_gb: f64,
}

/// provider 成本
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderCost {
    /// provider 名称
    pub provider: String,
    /// bucket 成本列表
    pub bucket_costs: Vec<BucketCost>,
    /// provider 总成本
    pub total_cost: f64,
}

/// 成本报表
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CostReport {
    /// 生成时间（Unix 毫秒）
    pub generated_at: u64,
    /// provider 成本列表
    pub provider_costs: Vec<ProviderCost>,
    /// 总成本
    pub total_cost: f64,
    /// 优化建议
    pub suggestions: Vec<CostOptimizationSuggestion>,
}

// ============================================================================
// CostAnalyzer — 成本分析器
// ============================================================================

/// 成本分析器
///
/// 按 provider/bucket/tier 统计存储成本，生成优化建议和成本报表。
pub struct CostAnalyzer {
    /// 分析配置
    config: CostConfig,
}

impl CostAnalyzer {
    /// 创建成本分析器
    pub fn new(config: CostConfig) -> Self {
        Self { config }
    }

    /// 获取当前时间（Unix 毫秒）
    fn now_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }

    /// 分析成本
    ///
    /// 接收外部提供的 bucket 成本数据，按 provider 汇总，生成成本报表。
    pub fn analyze(&self, bucket_costs: Vec<BucketCost>) -> Result<CostReport, CostError> {
        for bc in &bucket_costs {
            if bc.capacity_cost < 0.0 || bc.request_cost < 0.0 || bc.traffic_cost < 0.0 {
                return Err(CostError::AbnormalData(format!(
                    "negative cost from provider {} bucket {}",
                    bc.provider, bc.bucket
                )));
            }
        }
        let mut provider_map: std::collections::HashMap<String, Vec<BucketCost>> =
            std::collections::HashMap::new();
        for bc in bucket_costs {
            provider_map
                .entry(bc.provider.clone())
                .or_default()
                .push(bc);
        }
        let mut provider_costs = Vec::new();
        let mut total_cost = 0.0;
        for provider_name in &self.config.providers {
            if let Some(buckets) = provider_map.get(provider_name) {
                let provider_total: f64 = buckets.iter().map(|b| b.total_cost).sum();
                total_cost += provider_total;
                provider_costs.push(ProviderCost {
                    provider: provider_name.clone(),
                    bucket_costs: buckets.clone(),
                    total_cost: provider_total,
                });
            }
        }
        let report = CostReport {
            generated_at: Self::now_ms(),
            provider_costs,
            total_cost,
            suggestions: Vec::new(),
        };
        Ok(report)
    }

    /// 生成优化建议
    pub fn suggest_optimization(&self, report: &CostReport) -> Vec<CostOptimizationSuggestion> {
        let mut suggestions = Vec::new();
        for pc in &report.provider_costs {
            for bc in &pc.bucket_costs {
                if bc.tier == "Standard" && bc.size_gb > 10.0 {
                    suggestions.push(CostOptimizationSuggestion::TierDowngrade {
                        bucket: bc.bucket.clone(),
                        from_tier: "Standard".to_string(),
                        to_tier: "InfrequentAccess".to_string(),
                        expected_saving_percent: 60.0,
                    });
                }
                if bc.tier == "Standard" && bc.size_gb > 100.0 {
                    suggestions.push(CostOptimizationSuggestion::CompressCold {
                        bucket: bc.bucket.clone(),
                        cold_data_size_gb: bc.size_gb * 0.5,
                    });
                }
                if bc.size_gb > 0.0 && bc.tier == "Archive" {
                    suggestions.push(CostOptimizationSuggestion::DeleteExpired {
                        bucket: bc.bucket.clone(),
                        expired_count: (bc.size_gb / 10.0) as u64,
                    });
                }
                if bc.tier == "Standard" {
                    suggestions.push(CostOptimizationSuggestion::LifecycleOptimize {
                        bucket: bc.bucket.clone(),
                        description: "consider lifecycle rule to auto-transition to InfrequentAccess after 30 days".to_string(),
                    });
                }
            }
        }
        suggestions
    }

    /// 生成报表
    pub fn generate_report(&self, report: &CostReport) -> Result<String, CostError> {
        match self.config.report_format {
            ReportFormat::Json => serde_json::to_string_pretty(report)
                .map_err(|e| CostError::SerializationError(e.to_string())),
            ReportFormat::Csv => Ok(self.generate_csv(report)),
        }
    }

    /// 生成 CSV 报表
    fn generate_csv(&self, report: &CostReport) -> String {
        let mut csv = String::from(
            "provider,bucket,tier,capacity_cost,request_cost,traffic_cost,total_cost,size_gb\n",
        );
        for pc in &report.provider_costs {
            for bc in &pc.bucket_costs {
                csv.push_str(&format!(
                    "{},{},{},{},{},{},{},{}\n",
                    bc.provider,
                    bc.bucket,
                    bc.tier,
                    bc.capacity_cost,
                    bc.request_cost,
                    bc.traffic_cost,
                    bc.total_cost,
                    bc.size_gb
                ));
            }
        }
        csv
    }

    /// 分析并生成完整报表（含建议）
    pub fn analyze_full(&self, bucket_costs: Vec<BucketCost>) -> Result<CostReport, CostError> {
        let mut report = self.analyze(bucket_costs)?;
        report.suggestions = self.suggest_optimization(&report);
        Ok(report)
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_bucket_cost(
        provider: &str,
        bucket: &str,
        tier: &str,
        total: f64,
        size_gb: f64,
    ) -> BucketCost {
        BucketCost {
            provider: provider.to_string(),
            bucket: bucket.to_string(),
            tier: tier.to_string(),
            capacity_cost: total * 0.8,
            request_cost: total * 0.1,
            traffic_cost: total * 0.1,
            total_cost: total,
            size_gb,
        }
    }

    #[test]
    fn test_config_default() {
        let config = CostConfig::new();
        assert_eq!(config.analysis_interval_ms, 86_400_000);
        assert_eq!(config.report_format, ReportFormat::Json);
        assert_eq!(config.providers.len(), 7);
    }

    #[test]
    fn test_config_builder() {
        let config = CostConfig::new()
            .with_analysis_interval_ms(3_600_000)
            .with_report_format(ReportFormat::Csv)
            .with_providers(vec!["s3".to_string()]);
        assert_eq!(config.analysis_interval_ms, 3_600_000);
        assert_eq!(config.report_format, ReportFormat::Csv);
        assert_eq!(config.providers.len(), 1);
    }

    #[test]
    fn test_analyze() {
        let analyzer = CostAnalyzer::new(CostConfig::new());
        let costs = vec![
            make_bucket_cost("s3", "bucket1", "Standard", 100.0, 500.0),
            make_bucket_cost("s3", "bucket2", "Archive", 50.0, 200.0),
        ];
        let report = analyzer.analyze(costs).unwrap();
        assert_eq!(report.provider_costs.len(), 1);
        assert_eq!(report.provider_costs[0].provider, "s3");
        assert!((report.total_cost - 150.0).abs() < 0.001);
    }

    #[test]
    fn test_analyze_multiple_providers() {
        let analyzer = CostAnalyzer::new(CostConfig::new());
        let costs = vec![
            make_bucket_cost("s3", "bucket1", "Standard", 100.0, 500.0),
            make_bucket_cost("aliyun", "bucket2", "Standard", 80.0, 300.0),
        ];
        let report = analyzer.analyze(costs).unwrap();
        assert_eq!(report.provider_costs.len(), 2);
        assert!((report.total_cost - 180.0).abs() < 0.001);
    }

    #[test]
    fn test_analyze_negative_cost() {
        let analyzer = CostAnalyzer::new(CostConfig::new());
        let costs = vec![BucketCost {
            provider: "s3".to_string(),
            bucket: "bucket1".to_string(),
            tier: "Standard".to_string(),
            capacity_cost: -10.0,
            request_cost: 5.0,
            traffic_cost: 5.0,
            total_cost: 0.0,
            size_gb: 100.0,
        }];
        let result = analyzer.analyze(costs);
        assert!(matches!(result, Err(CostError::AbnormalData(_))));
    }

    #[test]
    fn test_suggest_tier_downgrade() {
        let analyzer = CostAnalyzer::new(CostConfig::new());
        let costs = vec![make_bucket_cost("s3", "bucket1", "Standard", 100.0, 500.0)];
        let report = analyzer.analyze(costs).unwrap();
        let suggestions = analyzer.suggest_optimization(&report);
        let has_downgrade = suggestions
            .iter()
            .any(|s| matches!(s, CostOptimizationSuggestion::TierDowngrade { .. }));
        assert!(has_downgrade);
    }

    #[test]
    fn test_suggest_compress_cold() {
        let analyzer = CostAnalyzer::new(CostConfig::new());
        let costs = vec![make_bucket_cost("s3", "bucket1", "Standard", 100.0, 200.0)];
        let report = analyzer.analyze(costs).unwrap();
        let suggestions = analyzer.suggest_optimization(&report);
        let has_compress = suggestions
            .iter()
            .any(|s| matches!(s, CostOptimizationSuggestion::CompressCold { .. }));
        assert!(has_compress);
    }

    #[test]
    fn test_suggest_delete_expired() {
        let analyzer = CostAnalyzer::new(CostConfig::new());
        let costs = vec![make_bucket_cost("s3", "bucket1", "Archive", 50.0, 200.0)];
        let report = analyzer.analyze(costs).unwrap();
        let suggestions = analyzer.suggest_optimization(&report);
        let has_delete = suggestions
            .iter()
            .any(|s| matches!(s, CostOptimizationSuggestion::DeleteExpired { .. }));
        assert!(has_delete);
    }

    #[test]
    fn test_suggest_lifecycle_optimize() {
        let analyzer = CostAnalyzer::new(CostConfig::new());
        let costs = vec![make_bucket_cost("s3", "bucket1", "Standard", 100.0, 50.0)];
        let report = analyzer.analyze(costs).unwrap();
        let suggestions = analyzer.suggest_optimization(&report);
        let has_lifecycle = suggestions
            .iter()
            .any(|s| matches!(s, CostOptimizationSuggestion::LifecycleOptimize { .. }));
        assert!(has_lifecycle);
    }

    #[test]
    fn test_generate_json_report() {
        let config = CostConfig::new().with_report_format(ReportFormat::Json);
        let analyzer = CostAnalyzer::new(config);
        let costs = vec![make_bucket_cost("s3", "bucket1", "Standard", 100.0, 500.0)];
        let report = analyzer.analyze(costs).unwrap();
        let json = analyzer.generate_report(&report).unwrap();
        assert!(json.contains("provider"));
        assert!(json.contains("bucket1"));
    }

    #[test]
    fn test_generate_csv_report() {
        let config = CostConfig::new().with_report_format(ReportFormat::Csv);
        let analyzer = CostAnalyzer::new(config);
        let costs = vec![make_bucket_cost("s3", "bucket1", "Standard", 100.0, 500.0)];
        let report = analyzer.analyze(costs).unwrap();
        let csv = analyzer.generate_report(&report).unwrap();
        assert!(csv.contains("provider,bucket,tier"));
        assert!(csv.contains("s3,bucket1,Standard"));
    }

    #[test]
    fn test_empty_providers() {
        let config = CostConfig::new().with_providers(vec![]);
        let analyzer = CostAnalyzer::new(config);
        let costs = vec![make_bucket_cost("s3", "bucket1", "Standard", 100.0, 500.0)];
        let report = analyzer.analyze(costs).unwrap();
        assert!(report.provider_costs.is_empty());
        assert!((report.total_cost - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_analyze_full() {
        let analyzer = CostAnalyzer::new(CostConfig::new());
        let costs = vec![
            make_bucket_cost("s3", "bucket1", "Standard", 100.0, 500.0),
            make_bucket_cost("s3", "bucket2", "Archive", 50.0, 200.0),
        ];
        let report = analyzer.analyze_full(costs).unwrap();
        assert!(!report.suggestions.is_empty());
    }

    #[test]
    fn test_cost_error_display() {
        assert_eq!(
            CostError::ProviderUnavailable("s3".to_string()).to_string(),
            "provider unavailable: s3"
        );
        assert_eq!(
            CostError::AbnormalData("negative".to_string()).to_string(),
            "abnormal cost data: negative"
        );
    }
}
