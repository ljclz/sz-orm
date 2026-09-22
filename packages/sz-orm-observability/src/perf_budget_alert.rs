//! 性能预算告警引擎（v8.1.0 任务 2.5，feature gate: `perf-budget-alert`）
//!
//! 指标采集 → 预算对比 → 超预算 ≤ 30s 触发告警 `PERF_BUDGET_EXCEEDED` → 附根因分析。
//! 复用 v8.0.0 `perf_hit_metrics.rs` 既有指标采集。

use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// 可观测性错误（v8.1.0 新增）
#[derive(Debug, Clone, thiserror::Error)]
pub enum ObsError {
    #[error("指标采集失败: {0}")]
    MetricCollectionFailed(String),
    #[error("根因分析失败: {0}")]
    RootCauseAnalysisFailed(String),
    #[error("预算配置无效: {0}")]
    InvalidBudgetConfig(String),
}

/// 性能预算（单接口）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerfBudget {
    /// P99 延迟预算（ms）
    pub latency_p99_ms: f64,
    /// 吞吐量预算下限（QPS）
    pub throughput_qps: f64,
    /// 内存预算（MB）
    pub memory_mb: f64,
}

impl PerfBudget {
    /// 创建性能预算
    pub fn new(latency_p99_ms: f64, throughput_qps: f64, memory_mb: f64) -> Self {
        Self {
            latency_p99_ms,
            throughput_qps,
            memory_mb,
        }
    }
}

/// 性能指标（单接口实测）
#[derive(Debug, Clone, Serialize)]
pub struct PerfMetric {
    /// 接口名
    pub interface_name: String,
    /// P99 延迟实测（ms）
    pub latency_p99_ms: f64,
    /// 吞吐量实测（QPS）
    pub throughput_qps: f64,
    /// 内存实测（MB）
    pub memory_mb: f64,
    /// 采集时间戳
    #[serde(skip)]
    pub collected_at: Instant,
}

impl PerfMetric {
    /// 创建性能指标
    pub fn new(
        interface_name: impl Into<String>,
        latency_p99_ms: f64,
        throughput_qps: f64,
        memory_mb: f64,
    ) -> Self {
        Self {
            interface_name: interface_name.into(),
            latency_p99_ms,
            throughput_qps,
            memory_mb,
            collected_at: Instant::now(),
        }
    }
}

/// 根因分析
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RootCauseAnalysis {
    /// 超预算指标名
    pub exceeded_metric: String,
    /// 预算值
    pub budget_value: f64,
    /// 实际值
    pub actual_value: f64,
    /// 超出百分比
    pub exceed_pct: f64,
    /// 根因定位（已采集时附定位，待人工分析时为 `ROOT_CAUSE_PENDING`）
    pub root_cause: String,
}

/// 性能预算告警
#[derive(Debug, Clone, Serialize)]
pub struct PerfBudgetAlert {
    /// 告警码 `PERF_BUDGET_EXCEEDED`
    pub alert_code: String,
    /// 接口名
    pub interface_name: String,
    /// 超预算指标名
    pub metric_name: String,
    /// 预算值
    pub budget_value: f64,
    /// 实际值
    pub actual_value: f64,
    /// 根因分析
    pub root_cause: RootCauseAnalysis,
    /// 告警触发时间
    #[serde(skip)]
    pub triggered_at: Instant,
}

/// 性能预算告警引擎
pub struct PerfBudgetAlertEngine {
    /// 接口名 → 预算
    budgets: HashMap<String, PerfBudget>,
    /// 告警延迟阈值（默认 30s，超预算 ≤ 30s 触发告警）
    alert_latency_threshold: Duration,
}

impl PerfBudgetAlertEngine {
    /// 创建告警引擎：指定预算映射和告警延迟阈值
    pub fn new(budgets: HashMap<String, PerfBudget>, alert_latency_threshold: Duration) -> Self {
        Self {
            budgets,
            alert_latency_threshold,
        }
    }

    /// 使用默认告警延迟阈值（30s）
    pub fn with_defaults(budgets: HashMap<String, PerfBudget>) -> Self {
        Self::new(budgets, Duration::from_secs(30))
    }

    /// 检查单条指标是否超预算，超预算返回告警
    pub async fn check_budget(
        &self,
        metric: &PerfMetric,
    ) -> Result<Option<PerfBudgetAlert>, ObsError> {
        let budget = match self.budgets.get(&metric.interface_name) {
            Some(b) => b,
            None => return Ok(None),
        };

        // 检查 P99 延迟超预算
        if metric.latency_p99_ms > budget.latency_p99_ms {
            let exceed_pct =
                ((metric.latency_p99_ms - budget.latency_p99_ms) / budget.latency_p99_ms) * 100.0;
            return Ok(Some(self.build_alert(
                metric,
                "latency_p99_ms",
                budget.latency_p99_ms,
                metric.latency_p99_ms,
                exceed_pct,
            )));
        }

        // 检查吞吐量低于预算下限
        if metric.throughput_qps < budget.throughput_qps {
            let exceed_pct = if budget.throughput_qps > 0.0 {
                ((budget.throughput_qps - metric.throughput_qps) / budget.throughput_qps) * 100.0
            } else {
                0.0
            };
            return Ok(Some(self.build_alert(
                metric,
                "throughput_qps",
                budget.throughput_qps,
                metric.throughput_qps,
                exceed_pct,
            )));
        }

        // 检查内存超预算
        if metric.memory_mb > budget.memory_mb {
            let exceed_pct = ((metric.memory_mb - budget.memory_mb) / budget.memory_mb) * 100.0;
            return Ok(Some(self.build_alert(
                metric,
                "memory_mb",
                budget.memory_mb,
                metric.memory_mb,
                exceed_pct,
            )));
        }

        Ok(None)
    }

    /// 批量检查多条指标，返回所有触发的告警
    pub async fn check_budgets(
        &self,
        metrics: &[PerfMetric],
    ) -> Result<Vec<PerfBudgetAlert>, ObsError> {
        let mut alerts = Vec::new();
        for metric in metrics {
            if let Some(alert) = self.check_budget(metric).await? {
                alerts.push(alert);
            }
        }
        Ok(alerts)
    }

    /// 告警延迟阈值（用于外部判定告警是否在阈值内）
    pub fn alert_latency_threshold(&self) -> Duration {
        self.alert_latency_threshold
    }

    /// 预算是否已配置（用于判定返回 None 是"未配置"还是"未超预算"）
    pub fn has_budget(&self, interface_name: &str) -> bool {
        self.budgets.contains_key(interface_name)
    }

    /// 渲染预算达标率 Prometheus 指标（任务 2.6）
    ///
    /// 暴露指标：`sz_orm_perf_budget_compliance_rate`（gauge，预算达标率 0.0~1.0）
    pub fn render_budget_compliance_rate(&self, metrics: &[PerfMetric]) -> String {
        let mut output = String::new();
        output.push_str(
            "# HELP sz_orm_perf_budget_compliance_rate Performance budget compliance rate\n",
        );
        output.push_str("# TYPE sz_orm_perf_budget_compliance_rate gauge\n");
        if metrics.is_empty() {
            output.push_str("sz_orm_perf_budget_compliance_rate 1.0\n");
            return output;
        }
        let mut compliant = 0usize;
        for metric in metrics {
            if let Some(budget) = self.budgets.get(&metric.interface_name) {
                if metric.latency_p99_ms <= budget.latency_p99_ms
                    && metric.throughput_qps >= budget.throughput_qps
                    && metric.memory_mb <= budget.memory_mb
                {
                    compliant += 1;
                }
            } else {
                compliant += 1;
            }
        }
        let rate = compliant as f64 / metrics.len() as f64;
        output.push_str(&format!("sz_orm_perf_budget_compliance_rate {rate}\n"));
        output
    }

    fn build_alert(
        &self,
        metric: &PerfMetric,
        metric_name: &str,
        budget_value: f64,
        actual_value: f64,
        exceed_pct: f64,
    ) -> PerfBudgetAlert {
        let root_cause = self.analyze_root_cause(metric, metric_name, budget_value, actual_value);
        PerfBudgetAlert {
            alert_code: "PERF_BUDGET_EXCEEDED".to_string(),
            interface_name: metric.interface_name.clone(),
            metric_name: metric_name.to_string(),
            budget_value,
            actual_value,
            root_cause: RootCauseAnalysis {
                exceeded_metric: metric_name.to_string(),
                budget_value,
                actual_value,
                exceed_pct,
                root_cause,
            },
            triggered_at: Instant::now(),
        }
    }

    fn analyze_root_cause(
        &self,
        metric: &PerfMetric,
        metric_name: &str,
        budget_value: f64,
        actual_value: f64,
    ) -> String {
        // 基于已采集指标做根因定位
        match metric_name {
            "latency_p99_ms" => {
                if metric.throughput_qps < budget_value * 0.5 {
                    format!(
                        "P99 延迟 {actual_value:.2}ms 超预算 {budget_value:.2}ms。\
                         根因定位：吞吐量 {qps:.2} QPS 显著低于预期，疑似 DB 慢查询或连接池耗尽",
                        qps = metric.throughput_qps
                    )
                } else if metric.memory_mb > budget_value * 10.0 {
                    format!(
                        "P99 延迟 {actual_value:.2}ms 超预算 {budget_value:.2}ms。\
                         根因定位：内存 {mem:.2}MB 异常高，疑似 GC 压力或内存泄漏",
                        mem = metric.memory_mb
                    )
                } else {
                    format!(
                        "P99 延迟 {actual_value:.2}ms 超预算 {budget_value:.2}ms。\
                         已采集指标：QPS={qps:.2}，内存={mem:.2}MB。ROOT_CAUSE_PENDING：\
                         需人工分析 DB 负载/网络抖动/锁竞争",
                        qps = metric.throughput_qps,
                        mem = metric.memory_mb
                    )
                }
            }
            "throughput_qps" => {
                format!(
                    "吞吐量 {actual_value:.2} QPS 低于预算下限 {budget_value:.2} QPS。\
                     根因定位：P99 延迟={lat:.2}ms，内存={mem:.2}MB。\
                     疑似连接池饱和或 DB 负载过高",
                    lat = metric.latency_p99_ms,
                    mem = metric.memory_mb
                )
            }
            "memory_mb" => {
                format!(
                    "内存 {actual_value:.2}MB 超预算 {budget_value:.2}MB。\
                     根因定位：P99 延迟={lat:.2}ms，QPS={qps:.2}。\
                     疑似结果集过大或缓存未命中导致堆膨胀",
                    lat = metric.latency_p99_ms,
                    qps = metric.throughput_qps
                )
            }
            _ => "ROOT_CAUSE_PENDING：未知指标超预算".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_budgets() -> HashMap<String, PerfBudget> {
        let mut budgets = HashMap::new();
        budgets.insert(
            "query_user_by_id".to_string(),
            PerfBudget::new(50.0, 1000.0, 256.0),
        );
        budgets
    }

    #[tokio::test]
    async fn test_no_alert_when_within_budget() {
        let engine = PerfBudgetAlertEngine::with_defaults(make_budgets());
        let metric = PerfMetric::new("query_user_by_id", 40.0, 1200.0, 200.0);
        let alert = engine.check_budget(&metric).await.unwrap();
        assert!(alert.is_none());
    }

    #[tokio::test]
    async fn test_alert_when_latency_exceeds_budget() {
        let engine = PerfBudgetAlertEngine::with_defaults(make_budgets());
        let metric = PerfMetric::new("query_user_by_id", 80.0, 1200.0, 200.0);
        let alert = engine.check_budget(&metric).await.unwrap().unwrap();
        assert_eq!(alert.alert_code, "PERF_BUDGET_EXCEEDED");
        assert_eq!(alert.metric_name, "latency_p99_ms");
        assert!(alert.root_cause.exceed_pct > 0.0);
    }

    #[tokio::test]
    async fn test_alert_when_throughput_below_budget() {
        let engine = PerfBudgetAlertEngine::with_defaults(make_budgets());
        let metric = PerfMetric::new("query_user_by_id", 40.0, 500.0, 200.0);
        let alert = engine.check_budget(&metric).await.unwrap().unwrap();
        assert_eq!(alert.metric_name, "throughput_qps");
        assert!(alert.actual_value < alert.budget_value);
    }

    #[tokio::test]
    async fn test_alert_when_memory_exceeds_budget() {
        let engine = PerfBudgetAlertEngine::with_defaults(make_budgets());
        let metric = PerfMetric::new("query_user_by_id", 40.0, 1200.0, 512.0);
        let alert = engine.check_budget(&metric).await.unwrap().unwrap();
        assert_eq!(alert.metric_name, "memory_mb");
    }

    #[tokio::test]
    async fn test_no_alert_when_budget_not_configured() {
        let engine = PerfBudgetAlertEngine::with_defaults(make_budgets());
        let metric = PerfMetric::new("unknown_interface", 9999.0, 1.0, 9999.0);
        let alert = engine.check_budget(&metric).await.unwrap();
        assert!(alert.is_none());
    }

    #[tokio::test]
    async fn test_root_cause_pending_when_no_clear_signal() {
        let engine = PerfBudgetAlertEngine::with_defaults(make_budgets());
        // 延迟超预算但 QPS 和内存正常，根因待人工分析
        let metric = PerfMetric::new("query_user_by_id", 80.0, 1100.0, 240.0);
        let alert = engine.check_budget(&metric).await.unwrap().unwrap();
        assert!(alert.root_cause.root_cause.contains("ROOT_CAUSE_PENDING"));
    }

    #[tokio::test]
    async fn test_root_cause_located_when_low_throughput() {
        let engine = PerfBudgetAlertEngine::with_defaults(make_budgets());
        // 延迟超预算且 QPS 显著低，根因定位为 DB 慢查询
        let metric = PerfMetric::new("query_user_by_id", 80.0, 20.0, 200.0);
        let alert = engine.check_budget(&metric).await.unwrap().unwrap();
        assert!(alert.root_cause.root_cause.contains("DB 慢查询"));
    }

    #[tokio::test]
    async fn test_check_budgets_batch() {
        let engine = PerfBudgetAlertEngine::with_defaults(make_budgets());
        let metrics = vec![
            PerfMetric::new("query_user_by_id", 40.0, 1200.0, 200.0), // 无告警
            PerfMetric::new("query_user_by_id", 80.0, 1200.0, 200.0), // 延迟超预算
            PerfMetric::new("query_user_by_id", 40.0, 500.0, 200.0),  // 吞吐量低
        ];
        let alerts = engine.check_budgets(&metrics).await.unwrap();
        assert_eq!(alerts.len(), 2);
    }

    #[tokio::test]
    async fn test_alert_latency_threshold_default_30s() {
        let engine = PerfBudgetAlertEngine::with_defaults(make_budgets());
        assert_eq!(engine.alert_latency_threshold(), Duration::from_secs(30));
    }

    #[tokio::test]
    async fn test_custom_alert_latency_threshold() {
        let engine = PerfBudgetAlertEngine::new(make_budgets(), Duration::from_secs(10));
        assert_eq!(engine.alert_latency_threshold(), Duration::from_secs(10));
    }

    #[tokio::test]
    async fn test_has_budget() {
        let engine = PerfBudgetAlertEngine::with_defaults(make_budgets());
        assert!(engine.has_budget("query_user_by_id"));
        assert!(!engine.has_budget("unknown"));
    }

    #[tokio::test]
    async fn test_root_cause_for_throughput() {
        let engine = PerfBudgetAlertEngine::with_defaults(make_budgets());
        let metric = PerfMetric::new("query_user_by_id", 40.0, 500.0, 200.0);
        let alert = engine.check_budget(&metric).await.unwrap().unwrap();
        assert!(alert.root_cause.root_cause.contains("连接池饱和"));
    }

    #[tokio::test]
    async fn test_root_cause_for_memory() {
        let engine = PerfBudgetAlertEngine::with_defaults(make_budgets());
        let metric = PerfMetric::new("query_user_by_id", 40.0, 1200.0, 512.0);
        let alert = engine.check_budget(&metric).await.unwrap().unwrap();
        assert!(alert.root_cause.root_cause.contains("堆膨胀"));
    }

    #[tokio::test]
    async fn test_render_budget_compliance_rate_full() {
        let engine = PerfBudgetAlertEngine::with_defaults(make_budgets());
        let metrics = vec![PerfMetric::new("query_user_by_id", 40.0, 1200.0, 200.0)];
        let output = engine.render_budget_compliance_rate(&metrics);
        assert!(output.contains("sz_orm_perf_budget_compliance_rate 1"));
    }

    #[tokio::test]
    async fn test_render_budget_compliance_rate_partial() {
        let engine = PerfBudgetAlertEngine::with_defaults(make_budgets());
        let metrics = vec![
            PerfMetric::new("query_user_by_id", 40.0, 1200.0, 200.0), // 达标
            PerfMetric::new("query_user_by_id", 80.0, 1200.0, 200.0), // 超预算
        ];
        let output = engine.render_budget_compliance_rate(&metrics);
        assert!(output.contains("sz_orm_perf_budget_compliance_rate 0.5"));
    }

    #[tokio::test]
    async fn test_render_budget_compliance_rate_empty() {
        let engine = PerfBudgetAlertEngine::with_defaults(make_budgets());
        let output = engine.render_budget_compliance_rate(&[]);
        assert!(output.contains("sz_orm_perf_budget_compliance_rate 1"));
    }
}
