//! 端到端测试：能耗周期采集 + Prometheus 导出

use sz_orm_observability::green::{EnergyMetrics, EnergyMetricsCollector};

#[tokio::test]
async fn e2e_energy_collect_and_export() {
    let collector = EnergyMetricsCollector::new();
    collector.collect(EnergyMetrics {
        timestamp: 1,
        cpu_energy_kwh: 1.0,
        memory_energy_kwh: 0.5,
        io_energy_kwh: 0.3,
        network_energy_kwh: 0.2,
    });
    collector.collect(EnergyMetrics {
        timestamp: 2,
        cpu_energy_kwh: 2.0,
        memory_energy_kwh: 0.5,
        io_energy_kwh: 0.3,
        network_energy_kwh: 0.2,
    });
    let output = collector.export_prometheus();
    assert!(output.contains("sz_orm_green_cpu_energy_kwh"));
}
