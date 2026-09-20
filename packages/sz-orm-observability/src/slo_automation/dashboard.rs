//! SLO 仪表盘导出器

use super::types::SloAchievement;

/// SLO 仪表盘导出器
pub struct SloDashboardExporter;

impl SloDashboardExporter {
    pub fn new() -> Self {
        Self
    }

    pub fn export_grafana_json(&self, achievement: &SloAchievement) -> String {
        let dashboard = serde_json::json!({
            "dashboard": {
                "title": "SZ-ORM SLO Dashboard",
                "panels": [
                    {
                        "title": "SLO Achievement Rate",
                        "type": "gauge",
                        "value": achievement.achievement_rate,
                        "window": format!("{:?}", achievement.window)
                    },
                    {
                        "title": "Error Budget",
                        "type": "stat",
                        "sufficient_data": achievement.sufficient_data
                    },
                    {
                        "title": "SLI Trend",
                        "type": "graph"
                    }
                ]
            }
        });
        serde_json::to_string_pretty(&dashboard).unwrap_or_default()
    }
}

impl Default for SloDashboardExporter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::super::types::CalcWindow;
    use super::*;

    #[test]
    fn test_export_grafana_json() {
        let exporter = SloDashboardExporter::new();
        let achievement = SloAchievement {
            window: CalcWindow::TwentyFourHours,
            achievement_rate: 0.995,
            sufficient_data: true,
        };
        let json = exporter.export_grafana_json(&achievement);
        assert!(json.contains("SLO Dashboard"));
        assert!(json.contains("0.995"));
    }
}
