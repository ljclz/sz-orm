use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use sz_orm_governance::cost_accountant::*;

struct AlertMetrics {
    data: HashMap<(String, String), ResourceUsage>,
}

impl AlertMetrics {
    fn new() -> Self {
        Self {
            data: HashMap::new(),
        }
    }

    fn with(mut self, dim: &str, id: &str, usage: ResourceUsage) -> Self {
        self.data.insert((dim.to_string(), id.to_string()), usage);
        self
    }
}

impl MetricsSource for AlertMetrics {
    fn get_usage(
        &self,
        dimension: &CostDimension,
        dimension_id: &str,
        _period: &str,
    ) -> Option<ResourceUsage> {
        let dim_str = format!("{:?}", dimension).to_lowercase();
        self.data.get(&(dim_str, dimension_id.to_string())).cloned()
    }
}

struct CapturingAlertBridge {
    alerts: Mutex<Vec<(String, String)>>,
}

impl CapturingAlertBridge {
    fn new() -> Self {
        Self {
            alerts: Mutex::new(Vec::new()),
        }
    }

    fn captured(&self) -> Vec<(String, String)> {
        self.alerts.lock().unwrap().clone()
    }

    fn count(&self) -> usize {
        self.alerts.lock().unwrap().len()
    }
}

struct SharedAlertBridge(Arc<CapturingAlertBridge>);

impl AlertBridge for SharedAlertBridge {
    fn alert(&self, code: &str, message: &str) {
        self.0
            .alerts
            .lock()
            .unwrap()
            .push((code.to_string(), message.to_string()));
    }
}

fn budget_config() -> BudgetConfig {
    BudgetConfig::new(PricingConfig {
        query_rate: 0.01,
        ..Default::default()
    })
    .with_budget("tenant_a", 10.0)
    .with_budget("tenant_b", 5.0)
    .with_budget("db_orders", 20.0)
}

#[test]
fn wiring_alert_fired_when_over_budget() {
    let metrics = AlertMetrics::new().with(
        "tenant",
        "tenant_a",
        ResourceUsage {
            query_count: 2000,
            ..Default::default()
        },
    );
    let bridge = Arc::new(CapturingAlertBridge::new());
    let accountant = CostAccountant::new(budget_config(), Box::new(metrics))
        .with_alert_bridge(Box::new(SharedAlertBridge(bridge.clone())));
    let report = accountant.account_period("2026-09");
    assert!(report.over_budget);
    assert!(bridge.count() >= 1);
}

#[test]
fn wiring_alert_code_is_gov_cost_budget_exceeded() {
    let metrics = AlertMetrics::new().with(
        "tenant",
        "tenant_a",
        ResourceUsage {
            query_count: 2000,
            ..Default::default()
        },
    );
    let bridge = Arc::new(CapturingAlertBridge::new());
    let accountant = CostAccountant::new(budget_config(), Box::new(metrics))
        .with_alert_bridge(Box::new(SharedAlertBridge(bridge.clone())));
    let _ = accountant.account_period("2026-09");
    let alerts = bridge.captured();
    assert!(alerts
        .iter()
        .any(|(code, _)| code == "GOV_COST_BUDGET_EXCEEDED"));
}

#[test]
fn wiring_alert_message_contains_dimension_id() {
    let metrics = AlertMetrics::new().with(
        "tenant",
        "tenant_a",
        ResourceUsage {
            query_count: 2000,
            ..Default::default()
        },
    );
    let bridge = Arc::new(CapturingAlertBridge::new());
    let accountant = CostAccountant::new(budget_config(), Box::new(metrics))
        .with_alert_bridge(Box::new(SharedAlertBridge(bridge.clone())));
    let _ = accountant.account_period("2026-09");
    let alerts = bridge.captured();
    let msg = alerts
        .iter()
        .find(|(c, _)| c == "GOV_COST_BUDGET_EXCEEDED")
        .map(|(_, m)| m.as_str())
        .unwrap();
    assert!(msg.contains("tenant_a"));
}

#[test]
fn wiring_alert_message_contains_cost_and_budget_values() {
    let metrics = AlertMetrics::new().with(
        "tenant",
        "tenant_a",
        ResourceUsage {
            query_count: 2000,
            ..Default::default()
        },
    );
    let bridge = Arc::new(CapturingAlertBridge::new());
    let accountant = CostAccountant::new(budget_config(), Box::new(metrics))
        .with_alert_bridge(Box::new(SharedAlertBridge(bridge.clone())));
    let _ = accountant.account_period("2026-09");
    let alerts = bridge.captured();
    let msg = alerts
        .iter()
        .find(|(c, _)| c == "GOV_COST_BUDGET_EXCEEDED")
        .map(|(_, m)| m.as_str())
        .unwrap();
    assert!(msg.contains("20.00") || msg.contains("20"));
    assert!(msg.contains("10.00") || msg.contains("10"));
}

#[test]
fn wiring_alert_not_fired_when_within_budget() {
    let metrics = AlertMetrics::new().with(
        "tenant",
        "tenant_a",
        ResourceUsage {
            query_count: 100,
            ..Default::default()
        },
    );
    let bridge = Arc::new(CapturingAlertBridge::new());
    let accountant = CostAccountant::new(budget_config(), Box::new(metrics))
        .with_alert_bridge(Box::new(SharedAlertBridge(bridge.clone())));
    let report = accountant.account_period("2026-09");
    assert!(!report.over_budget);
    assert_eq!(bridge.count(), 0);
}

#[test]
fn wiring_multiple_alerts_for_multiple_over_budget_dimensions() {
    let metrics = AlertMetrics::new()
        .with(
            "tenant",
            "tenant_a",
            ResourceUsage {
                query_count: 2000,
                ..Default::default()
            },
        )
        .with(
            "tenant",
            "tenant_b",
            ResourceUsage {
                query_count: 1000,
                ..Default::default()
            },
        )
        .with(
            "database",
            "db_orders",
            ResourceUsage {
                query_count: 5000,
                ..Default::default()
            },
        );
    let bridge = Arc::new(CapturingAlertBridge::new());
    let accountant = CostAccountant::new(budget_config(), Box::new(metrics))
        .with_alert_bridge(Box::new(SharedAlertBridge(bridge.clone())));
    let report = accountant.account_period("2026-09");
    assert!(report.over_budget);
    assert!(bridge.count() >= 2);
}

#[test]
fn wiring_alert_message_contains_over_pct() {
    let metrics = AlertMetrics::new().with(
        "tenant",
        "tenant_a",
        ResourceUsage {
            query_count: 2000,
            ..Default::default()
        },
    );
    let bridge = Arc::new(CapturingAlertBridge::new());
    let accountant = CostAccountant::new(budget_config(), Box::new(metrics))
        .with_alert_bridge(Box::new(SharedAlertBridge(bridge.clone())));
    let _ = accountant.account_period("2026-09");
    let alerts = bridge.captured();
    let msg = alerts
        .iter()
        .find(|(c, _)| c == "GOV_COST_BUDGET_EXCEEDED")
        .map(|(_, m)| m.as_str())
        .unwrap();
    assert!(msg.contains("%"));
}
