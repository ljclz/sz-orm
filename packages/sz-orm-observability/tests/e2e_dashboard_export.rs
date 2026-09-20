//! 端到端测试：Grafana 仪表盘 JSON 导出

use sz_orm_observability::slo_automation::{CalcWindow, SloAchievement, SloDashboardExporter};

#[tokio::test]
async fn e2e_dashboard_export_grafana() {
    let exporter = SloDashboardExporter::new();
    let achievement = SloAchievement {
        window: CalcWindow::TwentyFourHours,
        achievement_rate: 0.995,
        sufficient_data: true,
    };
    let json = exporter.export_grafana_json(&achievement);
    assert!(json.contains("SLO Dashboard"));
}
