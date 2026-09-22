//! 告警风暴抑制器：短时大量告警 → 聚合/去重/抑制 → 减少通知噪音。
//!
//! 聚合策略：none（不抑制）/ group（按标签分组）/ deduplicate（去重）/ inhibit（抑制）。
//! 聚合失败时返回原始告警列表（不丢失告警）。

use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// 告警级别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum AlertSeverity {
    /// 信息。
    Info,
    /// 警告。
    Warning,
    /// 严重。
    Critical,
}

impl AlertSeverity {
    /// 级别名。
    pub fn as_str(&self) -> &'static str {
        match self {
            AlertSeverity::Info => "info",
            AlertSeverity::Warning => "warning",
            AlertSeverity::Critical => "critical",
        }
    }
}

/// 告警条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertItem {
    /// 告警名/规则名。
    pub name: String,
    /// 级别。
    pub severity: AlertSeverity,
    /// 消息。
    pub message: String,
    /// 标签（用于分组/去重）。
    pub labels: HashMap<String, String>,
    /// 触发时间（不参与序列化，仅运行时使用）。
    #[serde(skip, default = "default_instant")]
    pub triggered_at: Instant,
    /// 聚合数量（去重/分组后附原始数量）。
    pub aggregate_count: u32,
}

fn default_instant() -> Instant {
    Instant::now()
}

impl AlertItem {
    /// 创建新告警。
    pub fn new(
        name: impl Into<String>,
        severity: AlertSeverity,
        message: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            severity,
            message: message.into(),
            labels: HashMap::new(),
            triggered_at: Instant::now(),
            aggregate_count: 1,
        }
    }

    /// 添加标签。
    pub fn with_label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.insert(key.into(), value.into());
        self
    }
}

/// 聚合策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SuppressStrategy {
    /// 不抑制。
    None,
    /// 按标签分组聚合。
    Group,
    /// 去重（相同 name + labels 只保留一个）。
    Deduplicate,
    /// 抑制（高级别告警抑制低级别同源告警）。
    Inhibit,
}

/// 告警风暴抑制器。
pub struct AlertStormSuppressor {
    /// 聚合策略。
    strategy: SuppressStrategy,
    /// 聚合窗口（仅聚合窗口内的告警）。
    window: Duration,
}

impl AlertStormSuppressor {
    /// 创建抑制器，默认窗口 60s。
    pub fn new(strategy: SuppressStrategy) -> Self {
        Self {
            strategy,
            window: Duration::from_secs(60),
        }
    }

    /// 设置聚合窗口。
    pub fn with_window(mut self, window: Duration) -> Self {
        self.window = window;
        self
    }

    /// 聚合策略。
    pub fn strategy(&self) -> SuppressStrategy {
        self.strategy
    }

    /// 聚合窗口。
    pub fn window(&self) -> Duration {
        self.window
    }

    /// 抑制告警风暴。返回抑制后的告警列表。
    /// 聚合失败时返回原始告警列表（不丢失告警）。
    pub fn suppress(&self, alerts: Vec<AlertItem>) -> Vec<AlertItem> {
        match self.strategy {
            SuppressStrategy::None => alerts,
            SuppressStrategy::Group => self.suppress_group(alerts),
            SuppressStrategy::Deduplicate => self.suppress_deduplicate(alerts),
            SuppressStrategy::Inhibit => self.suppress_inhibit(alerts),
        }
    }

    /// 按标签分组聚合：相同 labels 的告警合并为一个，aggregate_count 为原始数量。
    fn suppress_group(&self, alerts: Vec<AlertItem>) -> Vec<AlertItem> {
        let now = Instant::now();
        let mut groups: HashMap<String, AlertItem> = HashMap::new();
        let mut ungrouped = Vec::new();

        for alert in alerts {
            // 仅聚合窗口内的告警
            if now.duration_since(alert.triggered_at) > self.window {
                ungrouped.push(alert);
                continue;
            }
            // 分组键：name + labels 排序拼接
            let mut label_pairs: Vec<(String, String)> = alert
                .labels
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            label_pairs.sort();
            let group_key = format!("{}|{:?}", alert.name, label_pairs);
            groups
                .entry(group_key)
                .and_modify(|existing: &mut AlertItem| {
                    existing.aggregate_count += 1;
                    // 保留最高级别
                    if alert.severity > existing.severity {
                        existing.severity = alert.severity;
                    }
                })
                .or_insert(alert);
        }

        let mut result: Vec<AlertItem> = groups.into_values().collect();
        result.extend(ungrouped);
        result
    }

    /// 去重：相同 name + labels 只保留一个（aggregate_count 为重复数）。
    fn suppress_deduplicate(&self, alerts: Vec<AlertItem>) -> Vec<AlertItem> {
        let mut seen: HashMap<String, AlertItem> = HashMap::new();
        for alert in alerts {
            let mut label_pairs: Vec<(String, String)> = alert
                .labels
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            label_pairs.sort();
            let dedup_key = format!("{}|{:?}", alert.name, label_pairs);
            seen.entry(dedup_key)
                .and_modify(|existing: &mut AlertItem| {
                    existing.aggregate_count += 1;
                })
                .or_insert(alert);
        }
        seen.into_values().collect()
    }

    /// 抑制：高级别告警抑制低级别同源告警（相同 name）。
    fn suppress_inhibit(&self, alerts: Vec<AlertItem>) -> Vec<AlertItem> {
        // 按告警名分组，找出每个 name 的最高级别
        let mut max_severity: HashMap<String, AlertSeverity> = HashMap::new();
        for alert in &alerts {
            max_severity
                .entry(alert.name.clone())
                .and_modify(|existing: &mut AlertSeverity| {
                    if alert.severity > *existing {
                        *existing = alert.severity;
                    }
                })
                .or_insert(alert.severity);
        }
        // 保留最高级别的告警，抑制低级别同源告警
        alerts
            .into_iter()
            .filter(|a| {
                max_severity
                    .get(&a.name)
                    .map(|max| a.severity == *max)
                    .unwrap_or(true)
            })
            .collect()
    }
}

impl Default for AlertStormSuppressor {
    fn default() -> Self {
        Self::new(SuppressStrategy::Deduplicate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_suppress_none() {
        let suppressor = AlertStormSuppressor::new(SuppressStrategy::None);
        let alerts = vec![
            AlertItem::new("a", AlertSeverity::Warning, "msg1"),
            AlertItem::new("a", AlertSeverity::Warning, "msg2"),
        ];
        let result = suppressor.suppress(alerts);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_suppress_deduplicate() {
        let suppressor = AlertStormSuppressor::new(SuppressStrategy::Deduplicate);
        let alerts = vec![
            AlertItem::new("high_cpu", AlertSeverity::Warning, "cpu 90%").with_label("host", "h1"),
            AlertItem::new("high_cpu", AlertSeverity::Warning, "cpu 91%").with_label("host", "h1"),
            AlertItem::new("high_cpu", AlertSeverity::Critical, "cpu 99%").with_label("host", "h2"),
        ];
        let result = suppressor.suppress(alerts);
        // h1 去重为 1 个（aggregate_count=2），h1 和 h2 标签不同不合并
        assert_eq!(result.len(), 2);
        let total_count: u32 = result.iter().map(|a| a.aggregate_count).sum();
        assert_eq!(total_count, 3);
    }

    #[test]
    fn test_suppress_group() {
        let suppressor = AlertStormSuppressor::new(SuppressStrategy::Group);
        let alerts = vec![
            AlertItem::new("db_down", AlertSeverity::Critical, "db unavailable")
                .with_label("db", "primary"),
            AlertItem::new("db_down", AlertSeverity::Warning, "db slow")
                .with_label("db", "primary"),
            AlertItem::new("db_down", AlertSeverity::Critical, "db unavailable")
                .with_label("db", "replica"),
        ];
        let result = suppressor.suppress(alerts);
        // primary 分组聚合为 1 个，replica 分组为 1 个
        assert_eq!(result.len(), 2);
        let primary = result
            .iter()
            .find(|a| a.labels.get("db") == Some(&"primary".to_string()))
            .unwrap();
        assert_eq!(primary.aggregate_count, 2);
        assert_eq!(primary.severity, AlertSeverity::Critical);
    }

    #[test]
    fn test_suppress_inhibit() {
        let suppressor = AlertStormSuppressor::new(SuppressStrategy::Inhibit);
        let alerts = vec![
            AlertItem::new("disk_full", AlertSeverity::Warning, "disk 85%"),
            AlertItem::new("disk_full", AlertSeverity::Critical, "disk 99%"),
            AlertItem::new("disk_full", AlertSeverity::Info, "disk 50%"),
        ];
        let result = suppressor.suppress(alerts);
        // 只保留 Critical 级别
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].severity, AlertSeverity::Critical);
    }

    #[test]
    fn test_suppress_empty() {
        let suppressor = AlertStormSuppressor::new(SuppressStrategy::Deduplicate);
        let result = suppressor.suppress(Vec::new());
        assert!(result.is_empty());
    }

    #[test]
    fn test_suppress_aggregate_count_detail() {
        let suppressor = AlertStormSuppressor::new(SuppressStrategy::Deduplicate);
        let alerts = vec![
            AlertItem::new("a", AlertSeverity::Warning, "m1"),
            AlertItem::new("a", AlertSeverity::Warning, "m2"),
            AlertItem::new("a", AlertSeverity::Warning, "m3"),
            AlertItem::new("a", AlertSeverity::Warning, "m4"),
        ];
        let result = suppressor.suppress(alerts);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].aggregate_count, 4);
    }

    #[test]
    fn test_suppress_group_window_expired() {
        let suppressor = AlertStormSuppressor::new(SuppressStrategy::Group)
            .with_window(Duration::from_millis(1));
        let mut old_alert = AlertItem::new("a", AlertSeverity::Warning, "old");
        old_alert.triggered_at = Instant::now() - Duration::from_secs(10);
        let new_alert = AlertItem::new("a", AlertSeverity::Warning, "new");
        let result = suppressor.suppress(vec![old_alert, new_alert]);
        // 窗口外的告警不聚合，保留原始
        assert_eq!(result.len(), 2);
    }
}
