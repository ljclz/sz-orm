//! 综合诊断报告（v10.1.0）
//!
//! 聚合连接池/缓存/慢查询/告警/合规指标，生成综合诊断报告。默认 disabled。

use serde::{Deserialize, Serialize};

/// 诊断报告配置
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiagnosticReportConfig {
    /// 是否启用诊断报告，默认 false
    pub enabled: bool,
    /// 健康度评分权重
    pub health_score_weights: HealthScoreWeights,
}

/// 健康度评分权重（各项之和应为 1.0）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthScoreWeights {
    pub pool: f64,
    pub cache: f64,
    pub query_latency: f64,
    pub alert_frequency: f64,
}

impl Default for HealthScoreWeights {
    fn default() -> Self {
        Self {
            pool: 0.25,
            cache: 0.25,
            query_latency: 0.25,
            alert_frequency: 0.25,
        }
    }
}

/// 综合诊断报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticReport {
    pub timestamp: String,
    pub health_score: f64,
    pub pool_stats: PoolStats,
    pub cache_stats: CacheStats,
    pub slow_queries_topn: Vec<String>,
    pub alert_history: Vec<String>,
    pub compliance_status: String,
    pub performance_metrics: PerformanceMetrics,
}

/// 连接池统计（诊断报告子项）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PoolStats {
    pub max_size: u32,
    pub in_use: u32,
    pub idle: u32,
    pub total_acquired: u64,
    pub total_released: u64,
}

/// 缓存统计（诊断报告子项）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CacheStats {
    pub hit_count: u64,
    pub miss_count: u64,
    pub eviction_count: u64,
    pub hit_rate: f64,
}

/// 性能指标（诊断报告子项）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PerformanceMetrics {
    pub avg_acquire_latency_ms: f64,
    pub avg_query_latency_ms: f64,
    pub p99_query_latency_ms: f64,
}

/// 诊断报告生成器
#[derive(Clone)]
pub struct DiagnosticReportGenerator {
    config: DiagnosticReportConfig,
}

impl DiagnosticReportGenerator {
    pub fn new(config: DiagnosticReportConfig) -> Self {
        Self { config }
    }

    /// 计算健康度评分 ∈ [0.0, 1.0]
    pub fn compute_health_score(
        &self,
        pool_score: f64,
        cache_score: f64,
        latency_score: f64,
        alert_score: f64,
    ) -> f64 {
        let w = &self.config.health_score_weights;
        let score = pool_score * w.pool
            + cache_score * w.cache
            + latency_score * w.query_latency
            + alert_score * w.alert_frequency;
        score.clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diagnostic_report_config_default_disabled() {
        let config = DiagnosticReportConfig::default();
        assert!(!config.enabled);

        let gen = DiagnosticReportGenerator::new(DiagnosticReportConfig::default());
        let score = gen.compute_health_score(0.5, 0.5, 0.5, 0.5);
        assert!(
            (score - 0.5).abs() < 1e-9,
            "compute_health_score should return weighted average"
        );
    }

    #[test]
    fn test_health_score_weights_sum() {
        let w = HealthScoreWeights::default();
        let sum = w.pool + w.cache + w.query_latency + w.alert_frequency;
        assert!((sum - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_compute_health_score_bounds() {
        let gen = DiagnosticReportGenerator::new(DiagnosticReportConfig::default());
        let high = gen.compute_health_score(1.0, 1.0, 1.0, 1.0);
        let low = gen.compute_health_score(0.0, 0.0, 0.0, 0.0);
        assert!((high - 1.0).abs() < 1e-9);
        assert!((low - 0.0).abs() < 1e-9);
        assert!(high > low);
    }
}
