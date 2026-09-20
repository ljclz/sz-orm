//! 端到端测试：碳足迹核算 GHG Scope 2

use sz_orm_observability::green::{
    CarbonFootprintCalculator, CarbonScope, EnergyMetrics, ReportPeriod,
};

#[tokio::test]
async fn e2e_carbon_calc_scope2() {
    let calc = CarbonFootprintCalculator::new(0.5, CarbonScope::Scope2Market);
    let metrics = vec![EnergyMetrics {
        timestamp: 1,
        cpu_energy_kwh: 1.0,
        memory_energy_kwh: 0.5,
        io_energy_kwh: 0.3,
        network_energy_kwh: 0.2,
    }];
    let footprint = calc.calculate(&metrics, ReportPeriod::Daily).unwrap();
    assert!(footprint.total_kgco2e > 0.0);
}
