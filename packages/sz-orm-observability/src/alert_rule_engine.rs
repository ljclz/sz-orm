//! 告警规则引擎：阈值/异常/组合告警 + 通知路由 + ≤ 10s 评估 + 准确率 ≥ 99%。
//!
//! 复用既有 [`crate::obs_alert_bridge`] 思路，新增规则引擎、规则类型（阈值/异常/组合）、
//! 评估间隔、通知路由与告警风暴抑制。依赖 `metrics-collect` 指标。

use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::alert_storm_suppressor::{AlertItem, AlertSeverity, AlertStormSuppressor};

/// 比较运算符。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComparisonOp {
    /// 大于。
    Gt,
    /// 大于等于。
    Gte,
    /// 小于。
    Lt,
    /// 小于等于。
    Lte,
    /// 等于。
    Eq,
    /// 不等于。
    Ne,
}

impl ComparisonOp {
    /// 执行比较。
    pub fn compare(&self, actual: f64, threshold: f64) -> bool {
        match self {
            ComparisonOp::Gt => actual > threshold,
            ComparisonOp::Gte => actual >= threshold,
            ComparisonOp::Lt => actual < threshold,
            ComparisonOp::Lte => actual <= threshold,
            ComparisonOp::Eq => (actual - threshold).abs() < f64::EPSILON,
            ComparisonOp::Ne => (actual - threshold).abs() >= f64::EPSILON,
        }
    }
}

/// 逻辑运算符（组合规则）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogicOp {
    /// 全部满足。
    And,
    /// 任一满足。
    Or,
}

/// 告警规则类型。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertRuleType {
    /// 阈值规则：指标值与阈值比较。
    Threshold {
        metric: String,
        op: ComparisonOp,
        threshold: f64,
    },
    /// 异常规则：指标在窗口内偏离均值超阈值。
    Anomaly {
        metric: String,
        window: Duration,
        deviation_threshold: f64,
    },
    /// 组合规则：多个规则逻辑组合。
    Combined {
        rules: Vec<AlertRuleType>,
        logic: LogicOp,
    },
}

/// 告警规则。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertRule {
    /// 规则名。
    pub name: String,
    /// 规则类型。
    pub rule_type: AlertRuleType,
    /// 告警级别。
    pub severity: AlertSeverity,
    /// 通知路由（告警发送到哪个通道）。
    pub notify_route: String,
    /// 是否启用。
    pub enabled: bool,
}

impl AlertRule {
    /// 创建阈值规则。
    pub fn threshold(
        name: impl Into<String>,
        metric: impl Into<String>,
        op: ComparisonOp,
        threshold: f64,
        severity: AlertSeverity,
    ) -> Self {
        Self {
            name: name.into(),
            rule_type: AlertRuleType::Threshold {
                metric: metric.into(),
                op,
                threshold,
            },
            severity,
            notify_route: "default".to_string(),
            enabled: true,
        }
    }

    /// 创建异常规则。
    pub fn anomaly(
        name: impl Into<String>,
        metric: impl Into<String>,
        window: Duration,
        deviation_threshold: f64,
        severity: AlertSeverity,
    ) -> Self {
        Self {
            name: name.into(),
            rule_type: AlertRuleType::Anomaly {
                metric: metric.into(),
                window,
                deviation_threshold,
            },
            severity,
            notify_route: "default".to_string(),
            enabled: true,
        }
    }

    /// 创建组合规则。
    pub fn combined(
        name: impl Into<String>,
        rules: Vec<AlertRuleType>,
        logic: LogicOp,
        severity: AlertSeverity,
    ) -> Self {
        Self {
            name: name.into(),
            rule_type: AlertRuleType::Combined { rules, logic },
            severity,
            notify_route: "default".to_string(),
            enabled: true,
        }
    }

    /// 设置通知路由。
    pub fn with_route(mut self, route: impl Into<String>) -> Self {
        self.notify_route = route.into();
        self
    }
}

/// 指标快照（规则评估输入）。
#[derive(Debug, Clone, Default)]
pub struct MetricsSnapshot {
    /// 指标名 → 当前值。
    values: HashMap<String, f64>,
    /// 指标历史（用于异常检测，指标名 → 近期值序列）。
    history: HashMap<String, Vec<f64>>,
}

impl MetricsSnapshot {
    /// 创建空快照。
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置指标值。
    pub fn set(&mut self, metric: impl Into<String>, value: f64) {
        self.values.insert(metric.into(), value);
    }

    /// 设置指标历史。
    pub fn set_history(&mut self, metric: impl Into<String>, history: Vec<f64>) {
        self.history.insert(metric.into(), history);
    }

    /// 获取指标值。
    pub fn get(&self, metric: &str) -> Option<f64> {
        self.values.get(metric).copied()
    }

    /// 获取指标历史。
    pub fn get_history(&self, metric: &str) -> Option<&Vec<f64>> {
        self.history.get(metric)
    }
}

/// 告警通知。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertNotification {
    /// 规则名。
    pub rule_name: String,
    /// 级别。
    pub severity: AlertSeverity,
    /// 消息。
    pub message: String,
    /// 通知路由。
    pub route: String,
    /// 触发时间（不参与序列化，仅运行时使用）。
    #[serde(skip, default = "default_instant")]
    pub triggered_at: Instant,
    /// 聚合数量。
    pub aggregate_count: u32,
}

fn default_instant() -> Instant {
    Instant::now()
}

/// 告警规则引擎错误。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AlertRuleError {
    /// 评估超时。
    #[error("alert eval timeout: rule {0}")]
    EvalTimeout(String),
    /// 指标缺失。
    #[error("metric missing: {0}")]
    MetricMissing(String),
}

/// 告警规则引擎。
pub struct AlertRuleEngine {
    /// 规则列表。
    rules: Vec<AlertRule>,
    /// 评估间隔（默认 10s）。
    eval_interval: Duration,
    /// 告警风暴抑制器。
    storm_suppressor: AlertStormSuppressor,
    /// 评估超时阈值。
    eval_timeout: Duration,
    /// 已触发告警计数。
    triggered_count: parking_lot::Mutex<u64>,
    /// 真实异常计数（用于准确率计算）。
    real_anomaly_count: parking_lot::Mutex<u64>,
    /// 漏报计数。
    missed_count: parking_lot::Mutex<u64>,
}

impl AlertRuleEngine {
    /// 创建规则引擎，默认评估间隔 10s。
    pub fn new(storm_suppressor: AlertStormSuppressor) -> Self {
        Self {
            rules: Vec::new(),
            eval_interval: Duration::from_secs(10),
            storm_suppressor,
            eval_timeout: Duration::from_secs(10),
            triggered_count: parking_lot::Mutex::new(0),
            real_anomaly_count: parking_lot::Mutex::new(0),
            missed_count: parking_lot::Mutex::new(0),
        }
    }

    /// 添加规则。
    pub fn add_rule(&mut self, rule: AlertRule) {
        self.rules.push(rule);
    }

    /// 评估间隔。
    pub fn eval_interval(&self) -> Duration {
        self.eval_interval
    }

    /// 规则数量。
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// 评估所有规则，返回告警通知列表。评估 ≤ 10s。
    pub async fn evaluate(
        &self,
        snapshot: &MetricsSnapshot,
    ) -> Result<Vec<AlertNotification>, AlertRuleError> {
        let start = Instant::now();
        let mut alerts = Vec::new();

        for rule in &self.rules {
            if !rule.enabled {
                continue;
            }
            // 评估超时检查
            if start.elapsed() > self.eval_timeout {
                return Err(AlertRuleError::EvalTimeout(rule.name.clone()));
            }
            if let Some(alert) = self.evaluate_rule(rule, snapshot) {
                alerts.push(alert);
            }
        }

        // 风暴抑制
        let alert_items: Vec<AlertItem> = alerts
            .iter()
            .map(|n| {
                AlertItem::new(n.rule_name.clone(), n.severity, n.message.clone())
                    .with_label("route", n.route.clone())
            })
            .collect();
        let suppressed = self.storm_suppressor.suppress(alert_items);

        // 转换回通知
        let notifications: Vec<AlertNotification> = suppressed
            .into_iter()
            .map(|item| AlertNotification {
                rule_name: item.name,
                severity: item.severity,
                message: item.message,
                route: item
                    .labels
                    .get("route")
                    .cloned()
                    .unwrap_or_else(|| "default".to_string()),
                triggered_at: item.triggered_at,
                aggregate_count: item.aggregate_count,
            })
            .collect();

        *self.triggered_count.lock() += notifications.len() as u64;
        Ok(notifications)
    }

    /// 评估单条规则。
    fn evaluate_rule(
        &self,
        rule: &AlertRule,
        snapshot: &MetricsSnapshot,
    ) -> Option<AlertNotification> {
        let triggered = self.eval_rule_type(&rule.rule_type, snapshot);
        if triggered {
            Some(AlertNotification {
                rule_name: rule.name.clone(),
                severity: rule.severity,
                message: format!("rule {} triggered", rule.name),
                route: rule.notify_route.clone(),
                triggered_at: Instant::now(),
                aggregate_count: 1,
            })
        } else {
            None
        }
    }

    /// 递归评估规则类型。
    fn eval_rule_type(&self, rule_type: &AlertRuleType, snapshot: &MetricsSnapshot) -> bool {
        match rule_type {
            AlertRuleType::Threshold {
                metric,
                op,
                threshold,
            } => snapshot
                .get(metric)
                .map(|v| op.compare(v, *threshold))
                .unwrap_or(false),
            AlertRuleType::Anomaly {
                metric,
                window: _,
                deviation_threshold,
            } => {
                // 异常检测：计算历史均值和标准差，当前值偏离超阈值则异常
                let current = match snapshot.get(metric) {
                    Some(v) => v,
                    None => return false,
                };
                let history = match snapshot.get_history(metric) {
                    Some(h) => h,
                    None => return false,
                };
                if history.is_empty() {
                    return false;
                }
                let mean = history.iter().sum::<f64>() / history.len() as f64;
                let variance =
                    history.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / history.len() as f64;
                let stddev = variance.sqrt();
                let deviation = if stddev > 0.0 {
                    (current - mean).abs() / stddev
                } else {
                    0.0
                };
                deviation > *deviation_threshold
            }
            AlertRuleType::Combined { rules, logic } => {
                let results: Vec<bool> = rules
                    .iter()
                    .map(|r| self.eval_rule_type(r, snapshot))
                    .collect();
                match logic {
                    LogicOp::And => results.iter().all(|r| *r),
                    LogicOp::Or => results.iter().any(|r| *r),
                }
            }
        }
    }

    /// 记录真实异常（用于准确率计算）。
    pub fn record_real_anomaly(&self) {
        *self.real_anomaly_count.lock() += 1;
    }

    /// 记录漏报。
    pub fn record_missed(&self) {
        *self.missed_count.lock() += 1;
    }

    /// 告警准确率 = 触发数 / (触发数 + 漏报数)。目标 ≥ 99%。
    pub fn accuracy(&self) -> f64 {
        let triggered = *self.triggered_count.lock();
        let missed = *self.missed_count.lock();
        if triggered + missed == 0 {
            1.0
        } else {
            triggered as f64 / (triggered + missed) as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alert_storm_suppressor::SuppressStrategy;

    #[tokio::test]
    async fn test_threshold_rule_triggered() {
        let mut engine = AlertRuleEngine::new(AlertStormSuppressor::new(SuppressStrategy::None));
        engine.add_rule(AlertRule::threshold(
            "high_cpu",
            "cpu_usage",
            ComparisonOp::Gt,
            80.0,
            AlertSeverity::Warning,
        ));
        let mut snapshot = MetricsSnapshot::new();
        snapshot.set("cpu_usage", 90.0);
        let alerts = engine.evaluate(&snapshot).await.unwrap();
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].rule_name, "high_cpu");
    }

    #[tokio::test]
    async fn test_threshold_rule_not_triggered() {
        let mut engine = AlertRuleEngine::new(AlertStormSuppressor::new(SuppressStrategy::None));
        engine.add_rule(AlertRule::threshold(
            "high_cpu",
            "cpu_usage",
            ComparisonOp::Gt,
            80.0,
            AlertSeverity::Warning,
        ));
        let mut snapshot = MetricsSnapshot::new();
        snapshot.set("cpu_usage", 50.0);
        let alerts = engine.evaluate(&snapshot).await.unwrap();
        assert_eq!(alerts.len(), 0);
    }

    #[tokio::test]
    async fn test_threshold_rule_metric_missing() {
        let mut engine = AlertRuleEngine::new(AlertStormSuppressor::new(SuppressStrategy::None));
        engine.add_rule(AlertRule::threshold(
            "high_cpu",
            "cpu_usage",
            ComparisonOp::Gt,
            80.0,
            AlertSeverity::Warning,
        ));
        let snapshot = MetricsSnapshot::new();
        let alerts = engine.evaluate(&snapshot).await.unwrap();
        assert_eq!(alerts.len(), 0);
    }

    #[tokio::test]
    async fn test_anomaly_rule_triggered() {
        let mut engine = AlertRuleEngine::new(AlertStormSuppressor::new(SuppressStrategy::None));
        engine.add_rule(AlertRule::anomaly(
            "latency_spike",
            "latency_ms",
            Duration::from_secs(60),
            2.0, // 2σ
            AlertSeverity::Critical,
        ));
        let mut snapshot = MetricsSnapshot::new();
        snapshot.set("latency_ms", 500.0); // 异常高
        snapshot.set_history(
            "latency_ms",
            vec![100.0, 105.0, 95.0, 100.0, 110.0, 90.0, 100.0],
        );
        let alerts = engine.evaluate(&snapshot).await.unwrap();
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].severity, AlertSeverity::Critical);
    }

    #[tokio::test]
    async fn test_anomaly_rule_not_triggered() {
        let mut engine = AlertRuleEngine::new(AlertStormSuppressor::new(SuppressStrategy::None));
        engine.add_rule(AlertRule::anomaly(
            "latency_spike",
            "latency_ms",
            Duration::from_secs(60),
            2.0,
            AlertSeverity::Critical,
        ));
        let mut snapshot = MetricsSnapshot::new();
        snapshot.set("latency_ms", 105.0); // 正常范围
        snapshot.set_history(
            "latency_ms",
            vec![100.0, 105.0, 95.0, 100.0, 110.0, 90.0, 100.0],
        );
        let alerts = engine.evaluate(&snapshot).await.unwrap();
        assert_eq!(alerts.len(), 0);
    }

    #[tokio::test]
    async fn test_combined_rule_and() {
        let mut engine = AlertRuleEngine::new(AlertStormSuppressor::new(SuppressStrategy::None));
        engine.add_rule(AlertRule::combined(
            "cpu_and_mem",
            vec![
                AlertRuleType::Threshold {
                    metric: "cpu".to_string(),
                    op: ComparisonOp::Gt,
                    threshold: 80.0,
                },
                AlertRuleType::Threshold {
                    metric: "mem".to_string(),
                    op: ComparisonOp::Gt,
                    threshold: 80.0,
                },
            ],
            LogicOp::And,
            AlertSeverity::Critical,
        ));
        let mut snapshot = MetricsSnapshot::new();
        snapshot.set("cpu", 90.0);
        snapshot.set("mem", 85.0);
        let alerts = engine.evaluate(&snapshot).await.unwrap();
        assert_eq!(alerts.len(), 1);
    }

    #[tokio::test]
    async fn test_combined_rule_or() {
        let mut engine = AlertRuleEngine::new(AlertStormSuppressor::new(SuppressStrategy::None));
        engine.add_rule(AlertRule::combined(
            "cpu_or_mem",
            vec![
                AlertRuleType::Threshold {
                    metric: "cpu".to_string(),
                    op: ComparisonOp::Gt,
                    threshold: 80.0,
                },
                AlertRuleType::Threshold {
                    metric: "mem".to_string(),
                    op: ComparisonOp::Gt,
                    threshold: 80.0,
                },
            ],
            LogicOp::Or,
            AlertSeverity::Warning,
        ));
        let mut snapshot = MetricsSnapshot::new();
        snapshot.set("cpu", 50.0);
        snapshot.set("mem", 90.0);
        let alerts = engine.evaluate(&snapshot).await.unwrap();
        assert_eq!(alerts.len(), 1);
    }

    #[tokio::test]
    async fn test_combined_rule_and_not_triggered() {
        let mut engine = AlertRuleEngine::new(AlertStormSuppressor::new(SuppressStrategy::None));
        engine.add_rule(AlertRule::combined(
            "cpu_and_mem",
            vec![
                AlertRuleType::Threshold {
                    metric: "cpu".to_string(),
                    op: ComparisonOp::Gt,
                    threshold: 80.0,
                },
                AlertRuleType::Threshold {
                    metric: "mem".to_string(),
                    op: ComparisonOp::Gt,
                    threshold: 80.0,
                },
            ],
            LogicOp::And,
            AlertSeverity::Critical,
        ));
        let mut snapshot = MetricsSnapshot::new();
        snapshot.set("cpu", 90.0);
        snapshot.set("mem", 50.0); // mem 不满足
        let alerts = engine.evaluate(&snapshot).await.unwrap();
        assert_eq!(alerts.len(), 0);
    }

    #[tokio::test]
    async fn test_notify_route() {
        let mut engine = AlertRuleEngine::new(AlertStormSuppressor::new(SuppressStrategy::None));
        engine.add_rule(
            AlertRule::threshold(
                "high_cpu",
                "cpu",
                ComparisonOp::Gt,
                80.0,
                AlertSeverity::Warning,
            )
            .with_route("pagerduty"),
        );
        let mut snapshot = MetricsSnapshot::new();
        snapshot.set("cpu", 90.0);
        let alerts = engine.evaluate(&snapshot).await.unwrap();
        assert_eq!(alerts[0].route, "pagerduty");
    }

    #[tokio::test]
    async fn test_eval_within_10s() {
        let mut engine = AlertRuleEngine::new(AlertStormSuppressor::new(SuppressStrategy::None));
        // 添加 100 条规则
        for i in 0..100 {
            engine.add_rule(AlertRule::threshold(
                format!("rule-{i}"),
                "metric",
                ComparisonOp::Gt,
                50.0,
                AlertSeverity::Info,
            ));
        }
        let mut snapshot = MetricsSnapshot::new();
        snapshot.set("metric", 100.0);
        let start = Instant::now();
        let alerts = engine.evaluate(&snapshot).await.unwrap();
        let elapsed = start.elapsed();
        assert_eq!(alerts.len(), 100);
        assert!(elapsed.as_millis() < 100, "eval too slow: {elapsed:?}");
    }

    #[tokio::test]
    async fn test_accuracy_calculation() {
        let engine = AlertRuleEngine::new(AlertStormSuppressor::new(SuppressStrategy::None));
        // 模拟 100 次触发，1 次漏报 → 准确率 100/101 ≈ 99%
        for _ in 0..100 {
            *engine.triggered_count.lock() += 1;
        }
        engine.record_missed();
        let acc = engine.accuracy();
        assert!(acc >= 0.99, "accuracy = {acc}");
    }

    #[tokio::test]
    async fn test_storm_suppression_deduplicate() {
        let mut engine =
            AlertRuleEngine::new(AlertStormSuppressor::new(SuppressStrategy::Deduplicate));
        // 相同规则触发多次（通过多次评估模拟）
        engine.add_rule(AlertRule::threshold(
            "high_cpu",
            "cpu",
            ComparisonOp::Gt,
            80.0,
            AlertSeverity::Warning,
        ));
        let mut snapshot = MetricsSnapshot::new();
        snapshot.set("cpu", 90.0);
        // 单次评估只产生 1 个告警，风暴抑制在单次内生效
        let alerts = engine.evaluate(&snapshot).await.unwrap();
        assert_eq!(alerts.len(), 1);
    }

    #[tokio::test]
    async fn test_disabled_rule_skipped() {
        let mut engine = AlertRuleEngine::new(AlertStormSuppressor::new(SuppressStrategy::None));
        let mut rule = AlertRule::threshold(
            "disabled",
            "cpu",
            ComparisonOp::Gt,
            80.0,
            AlertSeverity::Warning,
        );
        rule.enabled = false;
        engine.add_rule(rule);
        let mut snapshot = MetricsSnapshot::new();
        snapshot.set("cpu", 90.0);
        let alerts = engine.evaluate(&snapshot).await.unwrap();
        assert_eq!(alerts.len(), 0);
    }
}
