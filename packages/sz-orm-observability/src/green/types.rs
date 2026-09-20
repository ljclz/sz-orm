//! 绿色计算核心数据结构

use std::collections::HashMap;

/// 碳排放范围
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CarbonScope {
    Scope2Market,
    Scope2Location,
}

/// 报告周期
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ReportPeriod {
    Daily,
    Weekly,
    Monthly,
}

/// 导出格式
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ExportFormat {
    Csv,
    Json,
    Prometheus,
}

/// 绿色计算配置
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GreenComputingConfig {
    pub collection_interval_secs: u64,
    pub carbon_factor: f64,
    pub scope: CarbonScope,
    pub schedule_weight: f64,
    pub latency_constraint_ms: u64,
    pub report_period: ReportPeriod,
    pub export_formats: Vec<ExportFormat>,
}

impl Default for GreenComputingConfig {
    fn default() -> Self {
        Self {
            collection_interval_secs: 60,
            carbon_factor: 0.5,
            scope: CarbonScope::Scope2Market,
            schedule_weight: 0.7,
            latency_constraint_ms: 100,
            report_period: ReportPeriod::Daily,
            export_formats: vec![ExportFormat::Json],
        }
    }
}

/// 能耗指标
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EnergyMetrics {
    pub timestamp: i64,
    pub cpu_energy_kwh: f64,
    pub memory_energy_kwh: f64,
    pub io_energy_kwh: f64,
    pub network_energy_kwh: f64,
}

impl EnergyMetrics {
    pub fn total_kwh(&self) -> f64 {
        self.cpu_energy_kwh + self.memory_energy_kwh + self.io_energy_kwh + self.network_energy_kwh
    }
}

/// 碳足迹
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CarbonFootprint {
    pub period: ReportPeriod,
    pub total_kgco2e: f64,
    pub by_component: HashMap<String, f64>,
}

/// 实例碳强度
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InstanceCarbonIntensity {
    pub instance_id: String,
    pub carbon_intensity: f64,
    pub region: String,
}

/// 调度决策
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ScheduleDecision {
    pub selected_instance: String,
    pub reason: String,
    pub fallback: bool,
}

/// ESG 报告
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EsgReport {
    pub period: ReportPeriod,
    pub total_energy_kwh: f64,
    pub total_carbon_kgco2e: f64,
    pub energy_trend: Vec<f64>,
    pub carbon_by_component: HashMap<String, f64>,
    pub scheduling_effectiveness: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_energy_metrics_total() {
        let m = EnergyMetrics {
            timestamp: 0,
            cpu_energy_kwh: 1.0,
            memory_energy_kwh: 0.5,
            io_energy_kwh: 0.3,
            network_energy_kwh: 0.2,
        };
        assert_eq!(m.total_kwh(), 2.0);
    }

    #[test]
    fn test_default_config() {
        let config = GreenComputingConfig::default();
        assert_eq!(config.collection_interval_secs, 60);
        assert_eq!(config.carbon_factor, 0.5);
    }
}
