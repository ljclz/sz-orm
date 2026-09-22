//! 成本模拟器：模拟迁移后存储成本下降 vs 查询延迟上升，计算净收益

use serde::{Deserialize, Serialize};

use super::super::types::DataTemperature;
use super::pyramid::TierMigrationStep;

/// 置信度
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Confidence {
    High,
    Medium,
    Low,
}

/// 成本模拟配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostSimulateConfig {
    /// 各层级每 GB 每月成本（元）
    pub tier_cost_per_gb: TierCostTable,
    /// 各层级查询延迟基线（ms）
    pub tier_latency_ms: TierLatencyTable,
    /// 低置信度数据阈值（行数）
    pub low_confidence_threshold: u64,
}

/// 层级成本表
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierCostTable {
    pub hot: f64,
    pub warm: f64,
    pub cold: f64,
    pub archived: f64,
}

impl Default for TierCostTable {
    fn default() -> Self {
        Self {
            hot: 2.0,
            warm: 1.0,
            cold: 0.5,
            archived: 0.1,
        }
    }
}

/// 层级延迟表
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierLatencyTable {
    pub hot: f64,
    pub warm: f64,
    pub cold: f64,
    pub archived: f64,
}

impl Default for TierLatencyTable {
    fn default() -> Self {
        Self {
            hot: 1.0,
            warm: 5.0,
            cold: 50.0,
            archived: 500.0,
        }
    }
}

impl Default for CostSimulateConfig {
    fn default() -> Self {
        Self {
            tier_cost_per_gb: TierCostTable::default(),
            tier_latency_ms: TierLatencyTable::default(),
            low_confidence_threshold: 100,
        }
    }
}

/// 成本报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostReport {
    pub storage_cost_delta: f64,
    pub latency_delta: f64,
    pub net_benefit: f64,
    pub confidence: Confidence,
    pub tag: String,
}

/// 存储统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageStats {
    pub total_gb: f64,
    pub avg_query_per_day: f64,
}

/// 成本模拟器
pub struct CostSimulator {
    config: CostSimulateConfig,
}

impl CostSimulator {
    pub fn new(config: CostSimulateConfig) -> Self {
        Self { config }
    }

    /// 模拟迁移后成本变化
    pub fn simulate(&self, plan: &[TierMigrationStep], stats: &StorageStats) -> CostReport {
        let total_rows: u64 = plan.iter().map(|s| s.data_range.row_count).sum();
        let confidence = if total_rows < self.config.low_confidence_threshold {
            Confidence::Low
        } else if total_rows < self.config.low_confidence_threshold * 10 {
            Confidence::Medium
        } else {
            Confidence::High
        };

        let mut storage_delta = 0.0;
        let mut latency_delta = 0.0;

        for step in plan {
            let from_cost = self.tier_cost(step.from_tier);
            let to_cost = self.tier_cost(step.to_tier);
            let from_latency = self.tier_latency(step.from_tier);
            let to_latency = self.tier_latency(step.to_tier);
            let fraction = if stats.total_gb > 0.0 {
                step.data_range.row_count as f64 / stats.total_gb
            } else {
                0.0
            };
            storage_delta += (to_cost - from_cost) * fraction;
            latency_delta += (to_latency - from_latency) * fraction;
        }

        let net_benefit =
            -storage_delta * stats.total_gb - latency_delta * stats.avg_query_per_day * 0.001;
        let tag = if confidence == Confidence::Low {
            "COST_SIMULATION_LOW_CONFIDENCE".to_string()
        } else {
            String::new()
        };

        CostReport {
            storage_cost_delta: storage_delta,
            latency_delta,
            net_benefit,
            confidence,
            tag,
        }
    }

    fn tier_cost(&self, tier: DataTemperature) -> f64 {
        match tier {
            DataTemperature::Hot => self.config.tier_cost_per_gb.hot,
            DataTemperature::Warm => self.config.tier_cost_per_gb.warm,
            DataTemperature::Cold => self.config.tier_cost_per_gb.cold,
            DataTemperature::Archived => self.config.tier_cost_per_gb.archived,
            DataTemperature::Destroy => 0.0,
        }
    }

    fn tier_latency(&self, tier: DataTemperature) -> f64 {
        match tier {
            DataTemperature::Hot => self.config.tier_latency_ms.hot,
            DataTemperature::Warm => self.config.tier_latency_ms.warm,
            DataTemperature::Cold => self.config.tier_latency_ms.cold,
            DataTemperature::Archived => self.config.tier_latency_ms.archived,
            DataTemperature::Destroy => 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::types::DataRange;
    use super::*;

    fn make_step(from: DataTemperature, to: DataTemperature, rows: u64) -> TierMigrationStep {
        TierMigrationStep {
            data_range: DataRange {
                min_id: 1,
                max_id: 100,
                row_count: rows,
                temperature: from,
                last_accessed_days_ago: 30,
                access_frequency_per_day: 0.1,
            },
            from_tier: from,
            to_tier: to,
        }
    }

    #[test]
    fn test_simulate_normal() {
        let sim = CostSimulator::new(CostSimulateConfig::default());
        let plan = vec![
            make_step(DataTemperature::Hot, DataTemperature::Warm, 500),
            make_step(DataTemperature::Warm, DataTemperature::Cold, 500),
        ];
        let stats = StorageStats {
            total_gb: 1000.0,
            avg_query_per_day: 100.0,
        };
        let report = sim.simulate(&plan, &stats);
        assert!(report.storage_cost_delta < 0.0);
        assert!(report.latency_delta > 0.0);
        assert_eq!(report.confidence, Confidence::High);
        assert!(report.tag.is_empty());
    }

    #[test]
    fn test_simulate_low_confidence() {
        let sim = CostSimulator::new(CostSimulateConfig::default());
        let plan = vec![make_step(DataTemperature::Hot, DataTemperature::Warm, 50)];
        let stats = StorageStats {
            total_gb: 1000.0,
            avg_query_per_day: 100.0,
        };
        let report = sim.simulate(&plan, &stats);
        assert_eq!(report.confidence, Confidence::Low);
        assert_eq!(report.tag, "COST_SIMULATION_LOW_CONFIDENCE");
    }

    #[test]
    fn test_simulate_medium_confidence() {
        let sim = CostSimulator::new(CostSimulateConfig::default());
        let plan = vec![make_step(DataTemperature::Hot, DataTemperature::Warm, 500)];
        let stats = StorageStats {
            total_gb: 1000.0,
            avg_query_per_day: 100.0,
        };
        let report = sim.simulate(&plan, &stats);
        assert_eq!(report.confidence, Confidence::Medium);
    }

    #[test]
    fn test_simulate_empty_plan() {
        let sim = CostSimulator::new(CostSimulateConfig::default());
        let stats = StorageStats {
            total_gb: 1000.0,
            avg_query_per_day: 100.0,
        };
        let report = sim.simulate(&[], &stats);
        assert_eq!(report.storage_cost_delta, 0.0);
        assert_eq!(report.latency_delta, 0.0);
        assert_eq!(report.confidence, Confidence::Low);
    }
}
