//! 端到端测试：ESG 月度报告导出

use std::collections::HashMap;
use sz_orm_observability::green::{
    CarbonFootprint, EnergyMetrics, EsgReportGenerator, ExportFormat, ReportPeriod,
};

#[tokio::test]
async fn e2e_esg_report_export() {
    let gen = EsgReportGenerator::new();
    let metrics = vec![EnergyMetrics {
        timestamp: 1,
        cpu_energy_kwh: 1.0,
        memory_energy_kwh: 0.5,
        io_energy_kwh: 0.3,
        network_energy_kwh: 0.2,
    }];
    let footprint = CarbonFootprint {
        period: ReportPeriod::Monthly,
        total_kgco2e: 1.0,
        by_component: HashMap::new(),
    };
    let report = gen.generate(&metrics, &footprint, ReportPeriod::Monthly);
    let json = gen.export(&report, ExportFormat::Json).unwrap();
    assert!(json.contains("total_energy_kwh"));
}
