//! 碳足迹计算器

use std::collections::HashMap;

use super::types::{CarbonFootprint, CarbonScope, EnergyMetrics, ReportPeriod};
use super::GreenError;

/// 碳足迹计算器
pub struct CarbonFootprintCalculator {
    carbon_factor: f64,
    scope: CarbonScope,
}

impl CarbonFootprintCalculator {
    pub fn new(carbon_factor: f64, scope: CarbonScope) -> Self {
        Self {
            carbon_factor,
            scope,
        }
    }

    pub fn calculate(
        &self,
        metrics: &[EnergyMetrics],
        period: ReportPeriod,
    ) -> Result<CarbonFootprint, GreenError> {
        if self.carbon_factor <= 0.0 {
            return Err(GreenError::CarbonFactorMissing(
                "碳排放因子未配置".to_string(),
            ));
        }

        let mut by_component = HashMap::new();
        let total_cpu: f64 = metrics.iter().map(|m| m.cpu_energy_kwh).sum();
        let total_mem: f64 = metrics.iter().map(|m| m.memory_energy_kwh).sum();
        let total_io: f64 = metrics.iter().map(|m| m.io_energy_kwh).sum();
        let total_net: f64 = metrics.iter().map(|m| m.network_energy_kwh).sum();

        by_component.insert("cpu".to_string(), total_cpu * self.carbon_factor);
        by_component.insert("memory".to_string(), total_mem * self.carbon_factor);
        by_component.insert("io".to_string(), total_io * self.carbon_factor);
        by_component.insert("network".to_string(), total_net * self.carbon_factor);

        let total_kgco2e: f64 = by_component.values().sum();

        Ok(CarbonFootprint {
            period,
            total_kgco2e,
            by_component,
        })
    }

    pub fn scope(&self) -> CarbonScope {
        self.scope
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_metrics(cpu: f64) -> EnergyMetrics {
        EnergyMetrics {
            timestamp: 0,
            cpu_energy_kwh: cpu,
            memory_energy_kwh: 0.5,
            io_energy_kwh: 0.3,
            network_energy_kwh: 0.2,
        }
    }

    #[test]
    fn test_calculate() {
        let calc = CarbonFootprintCalculator::new(0.5, CarbonScope::Scope2Market);
        let metrics = vec![make_metrics(1.0)];
        let footprint = calc.calculate(&metrics, ReportPeriod::Daily).unwrap();
        assert!(footprint.total_kgco2e > 0.0);
        assert!(footprint.by_component.contains_key("cpu"));
    }

    #[test]
    fn test_zero_carbon_factor() {
        let calc = CarbonFootprintCalculator::new(0.0, CarbonScope::Scope2Market);
        let metrics = vec![make_metrics(1.0)];
        assert!(calc.calculate(&metrics, ReportPeriod::Daily).is_err());
    }

    #[test]
    fn test_scope2() {
        let calc = CarbonFootprintCalculator::new(0.5, CarbonScope::Scope2Location);
        assert_eq!(calc.scope(), CarbonScope::Scope2Location);
    }
}
