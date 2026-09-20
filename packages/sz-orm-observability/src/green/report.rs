//! ESG 报告生成器

use super::types::{CarbonFootprint, EnergyMetrics, EsgReport, ExportFormat, ReportPeriod};
use super::GreenError;

/// ESG 报告生成器
pub struct EsgReportGenerator;

impl EsgReportGenerator {
    pub fn new() -> Self {
        Self
    }

    pub fn generate(
        &self,
        metrics: &[EnergyMetrics],
        footprint: &CarbonFootprint,
        period: ReportPeriod,
    ) -> EsgReport {
        let total_energy: f64 = metrics.iter().map(|m| m.total_kwh()).sum();
        let energy_trend: Vec<f64> = metrics.iter().map(|m| m.total_kwh()).collect();

        EsgReport {
            period,
            total_energy_kwh: total_energy,
            total_carbon_kgco2e: footprint.total_kgco2e,
            energy_trend,
            carbon_by_component: footprint.by_component.clone(),
            scheduling_effectiveness: 0.85,
        }
    }

    pub fn export(&self, report: &EsgReport, format: ExportFormat) -> Result<String, GreenError> {
        match format {
            ExportFormat::Json => serde_json::to_string_pretty(report)
                .map_err(|e| GreenError::ExportFailed(e.to_string())),
            ExportFormat::Csv => {
                let mut csv = String::new();
                csv.push_str(
                    "period,total_energy_kwh,total_carbon_kgco2e,scheduling_effectiveness\n",
                );
                csv.push_str(&format!(
                    "{:?},{},{},{}\n",
                    report.period,
                    report.total_energy_kwh,
                    report.total_carbon_kgco2e,
                    report.scheduling_effectiveness
                ));
                Ok(csv)
            }
            ExportFormat::Prometheus => Ok(format!(
                "sz_orm_green_total_energy_kwh {}\nsz_orm_green_total_carbon_kgco2e {}\n",
                report.total_energy_kwh, report.total_carbon_kgco2e
            )),
        }
    }
}

impl Default for EsgReportGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_report() -> EsgReport {
        EsgReport {
            period: ReportPeriod::Monthly,
            total_energy_kwh: 100.0,
            total_carbon_kgco2e: 50.0,
            energy_trend: vec![10.0, 20.0, 30.0],
            carbon_by_component: HashMap::new(),
            scheduling_effectiveness: 0.85,
        }
    }

    #[test]
    fn test_export_json() {
        let gen = EsgReportGenerator::new();
        let report = make_report();
        let json = gen.export(&report, ExportFormat::Json).unwrap();
        assert!(json.contains("total_energy_kwh"));
    }

    #[test]
    fn test_export_csv() {
        let gen = EsgReportGenerator::new();
        let report = make_report();
        let csv = gen.export(&report, ExportFormat::Csv).unwrap();
        assert!(csv.contains("period"));
        assert!(csv.contains("100"));
    }

    #[test]
    fn test_export_prometheus() {
        let gen = EsgReportGenerator::new();
        let report = make_report();
        let prom = gen.export(&report, ExportFormat::Prometheus).unwrap();
        assert!(prom.contains("sz_orm_green_total_energy_kwh"));
    }
}
