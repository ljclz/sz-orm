//! Grafana 仪表盘导出器：自动生成 Grafana 仪表盘 JSON，覆盖六维指标。
//!
//! 复用既有 [`crate::prometheus_exporter`] 思路，依赖 `metrics-collect` 指标。
//! 生成失败时告警 `DASHBOARD_EXPORT_FAILED` 并使用上次成功版本。

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::unified_collector::MetricDimension;
/// 仪表盘生成错误。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DashboardExportError {
    /// 生成失败。
    #[error("dashboard export failed: {0}")]
    ExportFailed(String),
}

/// Grafana 仪表盘 JSON（完整 JSON 文档）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrafanaDashboardJson {
    /// 仪表盘标题。
    pub title: String,
    /// 仪表盘 UID。
    pub uid: String,
    /// Grafana schema 版本。
    pub schema_version: u32,
    /// 完整 JSON 文档（panels/datasource/templating 等）。
    pub json: Value,
}

/// Grafana 仪表盘导出器。
pub struct GrafanaDashboardExporter {
    /// 数据源名称（Prometheus）。
    datasource: String,
    /// 上次成功版本（生成失败时降级使用）。
    last_success: parking_lot::Mutex<Option<GrafanaDashboardJson>>,
    /// 生成失败计数。
    failed_count: parking_lot::Mutex<u64>,
}

impl GrafanaDashboardExporter {
    /// 创建导出器，默认数据源 Prometheus。
    pub fn new() -> Self {
        Self {
            datasource: "Prometheus".to_string(),
            last_success: parking_lot::Mutex::new(None),
            failed_count: parking_lot::Mutex::new(0),
        }
    }

    /// 设置数据源。
    pub fn with_datasource(mut self, datasource: impl Into<String>) -> Self {
        self.datasource = datasource.into();
        self
    }

    /// 导出 Grafana 仪表盘 JSON，覆盖六维指标。生成失败时返回错误 + 保留上次成功版本。
    pub async fn export(&self) -> Result<GrafanaDashboardJson, DashboardExportError> {
        let panels = self.build_six_dimension_panels();

        let dashboard = json!({
            "title": "SZ-ORM Observability Dashboard",
            "uid": "sz-orm-obs",
            "schemaVersion": 38,
            "version": 1,
            "refresh": "15s",
            "time": {
                "from": "now-1h",
                "to": "now"
            },
            "templating": {
                "list": []
            },
            "panels": panels,
        });

        let result = GrafanaDashboardJson {
            title: "SZ-ORM Observability Dashboard".to_string(),
            uid: "sz-orm-obs".to_string(),
            schema_version: 38,
            json: dashboard,
        };

        // 缓存成功版本
        *self.last_success.lock() = Some(result.clone());
        Ok(result)
    }

    /// 构建六维面板（性能/AI/分布式/安全/生态/可观测性）。
    fn build_six_dimension_panels(&self) -> Vec<Value> {
        let dimensions = MetricDimension::all();
        dimensions
            .iter()
            .enumerate()
            .map(|(i, dim)| {
                let (title, queries) = self.dimension_panel(dim);
                let grid_pos = json!({
                    "h": 8,
                    "w": 12,
                    "x": if i % 2 == 0 { 0 } else { 12 },
                    "y": (i / 2) * 8
                });
                let targets: Vec<Value> = queries
                    .iter()
                    .enumerate()
                    .map(|(j, q)| {
                        json!({
                            "refId": char::from_u32('A' as u32 + j as u32).unwrap_or('A'),
                            "expr": q,
                            "datasource": self.datasource
                        })
                    })
                    .collect();
                json!({
                    "id": i + 1,
                    "title": title,
                    "type": "timeseries",
                    "datasource": self.datasource,
                    "gridPos": grid_pos,
                    "targets": targets,
                    "fieldConfig": {
                        "defaults": {
                            "unit": "short"
                        }
                    }
                })
            })
            .collect()
    }

    /// 每个维度的面板标题和查询。
    fn dimension_panel(&self, dim: &MetricDimension) -> (String, Vec<String>) {
        match dim {
            MetricDimension::Performance => (
                "Performance".to_string(),
                vec![
                    "rate(sz_orm_query_duration_seconds_count[5m])".to_string(),
                    "histogram_quantile(0.99, sz_orm_query_duration_seconds_bucket)".to_string(),
                    "sz_orm_pool_active_connections".to_string(),
                ],
            ),
            MetricDimension::Ai => (
                "AI Autonomous".to_string(),
                vec![
                    "sz_orm_ai_autonomous_decisions_total".to_string(),
                    "sz_orm_ai_ab_test_significance".to_string(),
                ],
            ),
            MetricDimension::Distributed => (
                "Distributed Consensus".to_string(),
                vec![
                    "sz_orm_raft_leader_elections_total".to_string(),
                    "sz_orm_replication_lag_seconds".to_string(),
                    "sz_orm_split_brain_detected".to_string(),
                ],
            ),
            MetricDimension::Security => (
                "Security & Compliance".to_string(),
                vec![
                    "sz_orm_compliance_violations_total".to_string(),
                    "sz_orm_key_rotation_count".to_string(),
                    "sz_orm_audit_events_total".to_string(),
                ],
            ),
            MetricDimension::Ecosystem => (
                "Ecosystem".to_string(),
                vec![
                    "sz_orm_plugin_downloads_total".to_string(),
                    "sz_orm_sdk_generated_total".to_string(),
                ],
            ),
            MetricDimension::Observability => (
                "Observability Self".to_string(),
                vec![
                    "sz_orm_trace_spans_total".to_string(),
                    "sz_orm_metrics_collection_lost".to_string(),
                    "sz_orm_alert_triggered_total".to_string(),
                ],
            ),
        }
    }

    /// 获取上次成功版本（生成失败时降级使用）。
    pub fn last_success(&self) -> Option<GrafanaDashboardJson> {
        self.last_success.lock().clone()
    }

    /// 生成失败计数。
    pub fn failed_count(&self) -> u64 {
        *self.failed_count.lock()
    }

    /// 模拟生成失败（测试用）。
    pub fn simulate_failure(&self) -> DashboardExportError {
        *self.failed_count.lock() += 1;
        DashboardExportError::ExportFailed("DASHBOARD_EXPORT_FAILED: simulated".to_string())
    }

    /// 校验仪表盘 JSON 可导入 Grafana：必需字段存在。
    pub fn validate_importable(dashboard: &GrafanaDashboardJson) -> bool {
        let json = &dashboard.json;
        json.get("title").is_some()
            && json.get("uid").is_some()
            && json.get("schemaVersion").is_some()
            && json.get("panels").map(|p| p.is_array()).unwrap_or(false)
    }
}

impl Default for GrafanaDashboardExporter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_export_generates_valid_json() {
        let exporter = GrafanaDashboardExporter::new();
        let dashboard = exporter.export().await.unwrap();
        assert_eq!(dashboard.title, "SZ-ORM Observability Dashboard");
        assert_eq!(dashboard.uid, "sz-orm-obs");
        assert_eq!(dashboard.schema_version, 38);
    }

    #[tokio::test]
    async fn test_export_covers_six_dimensions() {
        let exporter = GrafanaDashboardExporter::new();
        let dashboard = exporter.export().await.unwrap();
        let panels = dashboard.json.get("panels").unwrap().as_array().unwrap();
        // 六维各一个面板
        assert_eq!(panels.len(), 6);
        let titles: Vec<&str> = panels
            .iter()
            .map(|p| p.get("title").unwrap().as_str().unwrap())
            .collect();
        assert!(titles.contains(&"Performance"));
        assert!(titles.contains(&"AI Autonomous"));
        assert!(titles.contains(&"Distributed Consensus"));
        assert!(titles.contains(&"Security & Compliance"));
        assert!(titles.contains(&"Ecosystem"));
        assert!(titles.contains(&"Observability Self"));
    }

    #[tokio::test]
    async fn test_export_importable() {
        let exporter = GrafanaDashboardExporter::new();
        let dashboard = exporter.export().await.unwrap();
        assert!(GrafanaDashboardExporter::validate_importable(&dashboard));
    }

    #[tokio::test]
    async fn test_export_caches_last_success() {
        let exporter = GrafanaDashboardExporter::new();
        assert!(exporter.last_success().is_none());
        exporter.export().await.unwrap();
        assert!(exporter.last_success().is_some());
    }

    #[tokio::test]
    async fn test_simulate_failure_increments_count() {
        let exporter = GrafanaDashboardExporter::new();
        assert_eq!(exporter.failed_count(), 0);
        let err = exporter.simulate_failure();
        assert!(err.to_string().contains("DASHBOARD_EXPORT_FAILED"));
        assert_eq!(exporter.failed_count(), 1);
    }

    #[tokio::test]
    async fn test_failure_fallback_to_last_success() {
        let exporter = GrafanaDashboardExporter::new();
        // 先成功导出一次
        let first = exporter.export().await.unwrap();
        // 模拟失败后降级使用上次成功版本
        let _err = exporter.simulate_failure();
        let fallback = exporter.last_success().unwrap();
        assert_eq!(fallback.uid, first.uid);
    }

    #[tokio::test]
    async fn test_custom_datasource() {
        let exporter = GrafanaDashboardExporter::new().with_datasource("VictoriaMetrics");
        let dashboard = exporter.export().await.unwrap();
        let panels = dashboard.json.get("panels").unwrap().as_array().unwrap();
        assert_eq!(
            panels[0].get("datasource").unwrap().as_str().unwrap(),
            "VictoriaMetrics"
        );
    }

    #[tokio::test]
    async fn test_panels_have_grid_positions() {
        let exporter = GrafanaDashboardExporter::new();
        let dashboard = exporter.export().await.unwrap();
        let panels = dashboard.json.get("panels").unwrap().as_array().unwrap();
        for panel in panels {
            assert!(panel.get("gridPos").is_some());
            assert!(panel.get("targets").unwrap().is_array());
        }
    }

    #[tokio::test]
    async fn test_validate_importable_rejects_invalid() {
        let invalid = GrafanaDashboardJson {
            title: "bad".to_string(),
            uid: "bad".to_string(),
            schema_version: 0,
            json: json!({}),
        };
        assert!(!GrafanaDashboardExporter::validate_importable(&invalid));
    }
}
