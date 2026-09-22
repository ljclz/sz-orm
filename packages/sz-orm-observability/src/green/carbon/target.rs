//! 碳减排目标管理：目标设定 + 进度跟踪 + 多格式导出

use super::super::types::{CarbonFootprint, ExportFormat};
use super::CarbonError;

/// 碳减排目标进度
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TargetProgress {
    /// 当前减排量（kgCO2e，正值表示减排）
    pub current_reduction: f64,
    /// 达成率（0.0 ~ 1.0）
    pub achievement_rate: f64,
    /// 数据空洞标记（缺失组件列表）
    pub data_gaps: Vec<String>,
    /// 目标风险预测（true = 有风险，进度落后于预期）
    pub at_risk: bool,
}

/// 碳减排目标管理器
pub struct CarbonReductionTarget {
    /// 基线排放（kgCO2e）
    baseline_kgco2e: f64,
    /// 目标排放（kgCO2e）
    target_kgco2e: f64,
    /// 截止时间戳（unix 秒）
    deadline_ts: i64,
    /// 起始时间戳（unix 秒）
    start_ts: i64,
    /// 历史足迹记录（时间戳 -> 排放量）
    history: Vec<(i64, f64)>,
    /// 当前足迹排放（kgCO2e）
    current_footprint_kgco2e: f64,
}

impl CarbonReductionTarget {
    /// 创建新的碳减排目标管理器
    ///
    /// `baseline_kgco2e` 为基线排放量，`start_ts` 为起始时间戳（unix 秒）
    pub fn new(baseline_kgco2e: f64, start_ts: i64) -> Self {
        Self {
            baseline_kgco2e,
            target_kgco2e: baseline_kgco2e,
            deadline_ts: start_ts,
            start_ts,
            history: Vec::new(),
            current_footprint_kgco2e: baseline_kgco2e,
        }
    }

    /// 设定减排目标
    ///
    /// `target_kgco2e` 为目标排放量，`deadline_ts` 为截止时间戳
    pub fn set_target(&mut self, target_kgco2e: f64, deadline_ts: i64) {
        self.target_kgco2e = target_kgco2e;
        self.deadline_ts = deadline_ts;
    }

    /// 更新进度，返回当前进度状态
    ///
    /// 内部记录当前时间戳和排放量，检测数据空洞和目标风险
    pub fn update_progress(&mut self, footprint: &CarbonFootprint) -> TargetProgress {
        let now_ts = chrono::Utc::now().timestamp();
        self.current_footprint_kgco2e = footprint.total_kgco2e;
        self.history.push((now_ts, footprint.total_kgco2e));

        let current_reduction = self.baseline_kgco2e - self.current_footprint_kgco2e;
        let total_reduction_needed = self.baseline_kgco2e - self.target_kgco2e;
        let achievement_rate = if total_reduction_needed > 0.0 {
            (current_reduction / total_reduction_needed).clamp(0.0, 1.0)
        } else {
            1.0
        };

        let data_gaps = Self::detect_data_gaps(footprint);
        let at_risk = self.predict_risk(now_ts, achievement_rate);

        TargetProgress {
            current_reduction,
            achievement_rate,
            data_gaps,
            at_risk,
        }
    }

    /// 导出进度报告
    ///
    /// 支持 CSV / JSON / Prometheus 三种格式，Prometheus 指标名前缀 `sz_orm_carbon_`
    pub fn export(&self, format: ExportFormat) -> Result<String, CarbonError> {
        let current_reduction = self.baseline_kgco2e - self.current_footprint_kgco2e;
        let total_reduction_needed = self.baseline_kgco2e - self.target_kgco2e;
        let achievement_rate = if total_reduction_needed > 0.0 {
            (current_reduction / total_reduction_needed).clamp(0.0, 1.0)
        } else {
            1.0
        };

        match format {
            ExportFormat::Csv => {
                let mut out = String::new();
                out.push_str("metric,value\n");
                out.push_str(&format!("baseline_kgco2e,{}\n", self.baseline_kgco2e));
                out.push_str(&format!("target_kgco2e,{}\n", self.target_kgco2e));
                out.push_str(&format!(
                    "current_footprint_kgco2e,{}\n",
                    self.current_footprint_kgco2e
                ));
                out.push_str(&format!("current_reduction,{}\n", current_reduction));
                out.push_str(&format!("achievement_rate,{}\n", achievement_rate));
                Ok(out)
            }
            ExportFormat::Json => {
                let map = serde_json::json!({
                    "baseline_kgco2e": self.baseline_kgco2e,
                    "target_kgco2e": self.target_kgco2e,
                    "current_footprint_kgco2e": self.current_footprint_kgco2e,
                    "current_reduction": current_reduction,
                    "achievement_rate": achievement_rate,
                });
                serde_json::to_string_pretty(&map)
                    .map_err(|e| CarbonError::ExportFailed(e.to_string()))
            }
            ExportFormat::Prometheus => {
                let mut out = String::new();
                out.push_str("# HELP sz_orm_carbon_baseline_kgco2e Baseline carbon emission\n");
                out.push_str("# TYPE sz_orm_carbon_baseline_kgco2e gauge\n");
                out.push_str(&format!(
                    "sz_orm_carbon_baseline_kgco2e {}\n",
                    self.baseline_kgco2e
                ));
                out.push_str("# HELP sz_orm_carbon_target_kgco2e Target carbon emission\n");
                out.push_str("# TYPE sz_orm_carbon_target_kgco2e gauge\n");
                out.push_str(&format!(
                    "sz_orm_carbon_target_kgco2e {}\n",
                    self.target_kgco2e
                ));
                out.push_str(
                    "# HELP sz_orm_carbon_current_footprint_kgco2e Current carbon footprint\n",
                );
                out.push_str("# TYPE sz_orm_carbon_current_footprint_kgco2e gauge\n");
                out.push_str(&format!(
                    "sz_orm_carbon_current_footprint_kgco2e {}\n",
                    self.current_footprint_kgco2e
                ));
                out.push_str("# HELP sz_orm_carbon_current_reduction Current carbon reduction\n");
                out.push_str("# TYPE sz_orm_carbon_current_reduction gauge\n");
                out.push_str(&format!(
                    "sz_orm_carbon_current_reduction {}\n",
                    current_reduction
                ));
                out.push_str(
                    "# HELP sz_orm_carbon_achievement_rate Carbon target achievement rate\n",
                );
                out.push_str("# TYPE sz_orm_carbon_achievement_rate gauge\n");
                out.push_str(&format!(
                    "sz_orm_carbon_achievement_rate {}\n",
                    achievement_rate
                ));
                Ok(out)
            }
        }
    }

    /// 返回基线排放
    pub fn baseline(&self) -> f64 {
        self.baseline_kgco2e
    }

    /// 返回目标排放
    pub fn target(&self) -> f64 {
        self.target_kgco2e
    }

    /// 返回当前足迹排放
    pub fn current_footprint(&self) -> f64 {
        self.current_footprint_kgco2e
    }

    /// 数据空洞检测：检查 by_component 是否包含所有预期组件
    fn detect_data_gaps(footprint: &CarbonFootprint) -> Vec<String> {
        let expected = ["cpu", "memory", "io", "network"];
        let mut gaps = Vec::new();
        for comp in expected {
            if !footprint.by_component.contains_key(comp) {
                gaps.push(format!("missing component: {}", comp));
            }
        }
        gaps
    }

    /// 目标风险预测：实际进度落后于时间线性预期进度时标记风险
    fn predict_risk(&self, now_ts: i64, achievement_rate: f64) -> bool {
        let total_duration = self.deadline_ts - self.start_ts;
        if total_duration <= 0 {
            return false;
        }
        let elapsed = now_ts - self.start_ts;
        if elapsed <= 0 {
            return false;
        }
        let expected_rate = (elapsed as f64 / total_duration as f64).clamp(0.0, 1.0);
        achievement_rate < expected_rate
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_footprint(total: f64, components: &[&str]) -> CarbonFootprint {
        let mut by_component = HashMap::new();
        for comp in components {
            by_component.insert(comp.to_string(), total / components.len() as f64);
        }
        CarbonFootprint {
            period: super::super::super::types::ReportPeriod::Daily,
            total_kgco2e: total,
            by_component,
        }
    }

    #[test]
    fn test_set_target() {
        let mut target = CarbonReductionTarget::new(1000.0, 0);
        target.set_target(500.0, 365 * 86400);
        assert_eq!(target.target(), 500.0);
        assert_eq!(target.baseline(), 1000.0);
    }

    #[test]
    fn test_update_progress() {
        let mut target = CarbonReductionTarget::new(1000.0, 0);
        target.set_target(500.0, 365 * 86400);
        let footprint = make_footprint(700.0, &["cpu", "memory", "io", "network"]);
        let progress = target.update_progress(&footprint);
        assert_eq!(progress.current_reduction, 300.0);
        assert!((progress.achievement_rate - 0.6).abs() < 1e-9);
        assert!(progress.data_gaps.is_empty());
    }

    #[test]
    fn test_update_progress_data_gaps() {
        let mut target = CarbonReductionTarget::new(1000.0, 0);
        target.set_target(500.0, 365 * 86400);
        let footprint = make_footprint(700.0, &["cpu", "memory"]);
        let progress = target.update_progress(&footprint);
        assert_eq!(progress.data_gaps.len(), 2);
        assert!(progress.data_gaps.iter().any(|g| g.contains("io")));
        assert!(progress.data_gaps.iter().any(|g| g.contains("network")));
    }

    #[test]
    fn test_export_csv() {
        let mut target = CarbonReductionTarget::new(1000.0, 0);
        target.set_target(500.0, 365 * 86400);
        let footprint = make_footprint(700.0, &["cpu", "memory", "io", "network"]);
        target.update_progress(&footprint);
        let csv = target.export(ExportFormat::Csv).unwrap();
        assert!(csv.contains("baseline_kgco2e,1000"));
        assert!(csv.contains("target_kgco2e,500"));
        assert!(csv.contains("current_reduction,300"));
    }

    #[test]
    fn test_export_json() {
        let mut target = CarbonReductionTarget::new(1000.0, 0);
        target.set_target(500.0, 365 * 86400);
        let footprint = make_footprint(700.0, &["cpu", "memory", "io", "network"]);
        target.update_progress(&footprint);
        let json = target.export(ExportFormat::Json).unwrap();
        assert!(json.contains("baseline_kgco2e"));
        assert!(json.contains("achievement_rate"));
        assert!(json.contains("current_reduction"));
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["baseline_kgco2e"].as_f64().unwrap(), 1000.0);
        assert!((parsed["achievement_rate"].as_f64().unwrap() - 0.6).abs() < 1e-9);
    }

    #[test]
    fn test_export_prometheus() {
        let mut target = CarbonReductionTarget::new(1000.0, 0);
        target.set_target(500.0, 365 * 86400);
        let footprint = make_footprint(700.0, &["cpu", "memory", "io", "network"]);
        target.update_progress(&footprint);
        let prom = target.export(ExportFormat::Prometheus).unwrap();
        assert!(prom.contains("sz_orm_carbon_baseline_kgco2e 1000"));
        assert!(prom.contains("sz_orm_carbon_target_kgco2e 500"));
        assert!(prom.contains("sz_orm_carbon_current_reduction 300"));
        assert!(prom.contains("# TYPE sz_orm_carbon_achievement_rate gauge"));
    }

    #[test]
    fn test_at_risk_when_progress_lagging() {
        let now = chrono::Utc::now().timestamp();
        let mut target = CarbonReductionTarget::new(1000.0, now - 180 * 86400);
        target.set_target(500.0, now + 185 * 86400);
        let footprint = make_footprint(900.0, &["cpu", "memory", "io", "network"]);
        let progress = target.update_progress(&footprint);
        assert!(progress.at_risk, "进度落后于预期应标记风险");
    }

    #[test]
    fn test_not_at_risk_when_progress_ahead() {
        let now = chrono::Utc::now().timestamp();
        let mut target = CarbonReductionTarget::new(1000.0, now - 180 * 86400);
        target.set_target(500.0, now + 185 * 86400);
        let footprint = make_footprint(100.0, &["cpu", "memory", "io", "network"]);
        let progress = target.update_progress(&footprint);
        assert!(!progress.at_risk, "进度超前于预期不应标记风险");
    }
}
