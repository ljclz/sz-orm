//! 端到端测试：能耗采集中断断点续采

use sz_orm_observability::green::{EnergyBreakpointResumer, EnergyMetrics};

#[tokio::test]
async fn e2e_breakpoint_resume() {
    let mut resumer = EnergyBreakpointResumer::new();
    resumer.record(&EnergyMetrics {
        timestamp: 100,
        cpu_energy_kwh: 1.0,
        memory_energy_kwh: 0.5,
        io_energy_kwh: 0.3,
        network_energy_kwh: 0.2,
    });
    resumer.record(&EnergyMetrics {
        timestamp: 200,
        cpu_energy_kwh: 1.0,
        memory_energy_kwh: 0.5,
        io_energy_kwh: 0.3,
        network_energy_kwh: 0.2,
    });
    assert!(resumer.has_gaps());
}
