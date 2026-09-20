//! 能耗指标采集器

use std::sync::Arc;

use parking_lot::RwLock;

use super::types::EnergyMetrics;

/// 能耗指标采集器
pub struct EnergyMetricsCollector {
    metrics: Arc<RwLock<Vec<EnergyMetrics>>>,
}

impl EnergyMetricsCollector {
    pub fn new() -> Self {
        Self {
            metrics: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn collect(&self, metrics: EnergyMetrics) {
        self.metrics.write().push(metrics);
    }

    pub fn get_all(&self) -> Vec<EnergyMetrics> {
        self.metrics.read().clone()
    }

    pub fn count(&self) -> usize {
        self.metrics.read().len()
    }

    pub fn export_prometheus(&self) -> String {
        let metrics = self.metrics.read();
        let mut output = String::new();

        let total_cpu: f64 = metrics.iter().map(|m| m.cpu_energy_kwh).sum();
        let total_mem: f64 = metrics.iter().map(|m| m.memory_energy_kwh).sum();
        let total_io: f64 = metrics.iter().map(|m| m.io_energy_kwh).sum();
        let total_net: f64 = metrics.iter().map(|m| m.network_energy_kwh).sum();

        output.push_str("# HELP sz_orm_green_cpu_energy_kwh Total CPU energy\n");
        output.push_str("# TYPE sz_orm_green_cpu_energy_kwh gauge\n");
        output.push_str(&format!("sz_orm_green_cpu_energy_kwh {}\n", total_cpu));
        output.push_str(&format!("sz_orm_green_memory_energy_kwh {}\n", total_mem));
        output.push_str(&format!("sz_orm_green_io_energy_kwh {}\n", total_io));
        output.push_str(&format!("sz_orm_green_network_energy_kwh {}\n", total_net));

        output
    }

    pub fn total_energy_kwh(&self) -> f64 {
        self.metrics.read().iter().map(|m| m.total_kwh()).sum()
    }
}

impl Default for EnergyMetricsCollector {
    fn default() -> Self {
        Self::new()
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
    fn test_collect() {
        let collector = EnergyMetricsCollector::new();
        collector.collect(make_metrics(1.0));
        collector.collect(make_metrics(2.0));
        assert_eq!(collector.count(), 2);
    }

    #[test]
    fn test_total_energy() {
        let collector = EnergyMetricsCollector::new();
        collector.collect(make_metrics(1.0));
        collector.collect(make_metrics(2.0));
        assert_eq!(collector.total_energy_kwh(), 5.0);
    }

    #[test]
    fn test_export_prometheus() {
        let collector = EnergyMetricsCollector::new();
        collector.collect(make_metrics(1.0));
        let output = collector.export_prometheus();
        assert!(output.contains("sz_orm_green_cpu_energy_kwh"));
        assert!(output.contains("sz_orm_green_memory_energy_kwh"));
    }
}
