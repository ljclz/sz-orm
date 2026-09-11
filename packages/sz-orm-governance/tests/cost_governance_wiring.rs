use std::collections::HashMap;
use sz_orm_governance::cost_accountant::*;

struct WiringMetrics {
    data: HashMap<(String, String), ResourceUsage>,
}

impl WiringMetrics {
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

impl MetricsSource for WiringMetrics {
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

fn default_config() -> BudgetConfig {
    BudgetConfig::new(PricingConfig::default())
        .with_budget("tenant_a", 100.0)
        .with_budget("tenant_b", 50.0)
        .with_budget("db_orders", 200.0)
        .with_budget("db_users", 30.0)
        .with_budget("query_select_users", 10.0)
}

#[test]
fn wiring_full_pipeline_config_to_report() {
    let metrics = WiringMetrics::new()
        .with(
            "tenant",
            "tenant_a",
            ResourceUsage {
                query_count: 100,
                ..Default::default()
            },
        )
        .with(
            "tenant",
            "tenant_b",
            ResourceUsage {
                query_count: 50,
                ..Default::default()
            },
        )
        .with(
            "database",
            "db_orders",
            ResourceUsage {
                io_count: 200,
                ..Default::default()
            },
        )
        .with(
            "database",
            "db_users",
            ResourceUsage {
                io_count: 100,
                ..Default::default()
            },
        )
        .with(
            "query",
            "query_select_users",
            ResourceUsage {
                query_count: 5,
                ..Default::default()
            },
        );
    let accountant = CostAccountant::new(default_config(), Box::new(metrics));
    let report = accountant.account_period("2026-09");
    assert!(!report.entries.is_empty());
    assert!(report.total_cost > 0.0);
    assert!(!report.data_missing);
}

#[test]
fn wiring_all_three_dimensions_present() {
    let metrics = WiringMetrics::new()
        .with(
            "tenant",
            "tenant_a",
            ResourceUsage {
                query_count: 10,
                ..Default::default()
            },
        )
        .with(
            "database",
            "db_orders",
            ResourceUsage {
                io_count: 10,
                ..Default::default()
            },
        )
        .with(
            "query",
            "query_select_users",
            ResourceUsage {
                query_count: 1,
                ..Default::default()
            },
        );
    let accountant = CostAccountant::new(default_config(), Box::new(metrics));
    let report = accountant.account_period("2026-09");
    let dims: Vec<_> = report.entries.iter().map(|e| e.dimension.clone()).collect();
    assert!(dims.contains(&CostDimension::Tenant));
    assert!(dims.contains(&CostDimension::Database));
    assert!(dims.contains(&CostDimension::Query));
}

#[test]
fn wiring_cost_formula_connection_duration() {
    let metrics = WiringMetrics::new().with(
        "tenant",
        "tenant_a",
        ResourceUsage {
            connection_duration_ms: 10000,
            ..Default::default()
        },
    );
    let pricing = PricingConfig {
        connection_duration_rate: 0.001,
        ..Default::default()
    };
    let config = BudgetConfig::new(pricing).with_budget("tenant_a", 100.0);
    let accountant = CostAccountant::new(config, Box::new(metrics));
    let report = accountant.account_period("2026-09");
    let entry = report
        .entries
        .iter()
        .find(|e| e.dimension_id == "tenant_a")
        .unwrap();
    assert!((entry.cost - 10.0).abs() < 0.001);
}

#[test]
fn wiring_cost_formula_io_count() {
    let metrics = WiringMetrics::new().with(
        "tenant",
        "tenant_a",
        ResourceUsage {
            io_count: 500,
            ..Default::default()
        },
    );
    let pricing = PricingConfig {
        io_rate: 0.01,
        ..Default::default()
    };
    let config = BudgetConfig::new(pricing).with_budget("tenant_a", 100.0);
    let accountant = CostAccountant::new(config, Box::new(metrics));
    let report = accountant.account_period("2026-09");
    let entry = report
        .entries
        .iter()
        .find(|e| e.dimension_id == "tenant_a")
        .unwrap();
    assert!((entry.cost - 5.0).abs() < 0.001);
}

#[test]
fn wiring_cost_formula_storage_bytes() {
    let metrics = WiringMetrics::new().with(
        "tenant",
        "tenant_a",
        ResourceUsage {
            storage_bytes: 100_000_000,
            ..Default::default()
        },
    );
    let pricing = PricingConfig {
        storage_rate: 0.0000001,
        ..Default::default()
    };
    let config = BudgetConfig::new(pricing).with_budget("tenant_a", 100.0);
    let accountant = CostAccountant::new(config, Box::new(metrics));
    let report = accountant.account_period("2026-09");
    let entry = report
        .entries
        .iter()
        .find(|e| e.dimension_id == "tenant_a")
        .unwrap();
    assert!((entry.cost - 10.0).abs() < 0.001);
}

#[test]
fn wiring_cost_formula_query_count() {
    let metrics = WiringMetrics::new().with(
        "tenant",
        "tenant_a",
        ResourceUsage {
            query_count: 200,
            ..Default::default()
        },
    );
    let pricing = PricingConfig {
        query_rate: 0.05,
        ..Default::default()
    };
    let config = BudgetConfig::new(pricing).with_budget("tenant_a", 100.0);
    let accountant = CostAccountant::new(config, Box::new(metrics));
    let report = accountant.account_period("2026-09");
    let entry = report
        .entries
        .iter()
        .find(|e| e.dimension_id == "tenant_a")
        .unwrap();
    assert!((entry.cost - 10.0).abs() < 0.001);
}

#[test]
fn wiring_no_budget_dimension_not_over_budget() {
    let metrics = WiringMetrics::new().with(
        "query",
        "query_select_users",
        ResourceUsage {
            query_count: 99999,
            ..Default::default()
        },
    );
    let config = BudgetConfig::new(PricingConfig::default());
    let accountant = CostAccountant::new(config, Box::new(metrics));
    let report = accountant.account_period("2026-09");
    let entry = report
        .entries
        .iter()
        .find(|e| e.dimension_id == "query_select_users")
        .unwrap();
    assert!(!entry.over_budget);
    assert!(entry.budget.is_none());
}

#[test]
fn wiring_data_missing_marks_report() {
    let metrics = WiringMetrics::new().with(
        "tenant",
        "tenant_a",
        ResourceUsage {
            query_count: 10,
            ..Default::default()
        },
    );
    let accountant = CostAccountant::new(default_config(), Box::new(metrics));
    let report = accountant.account_period("2026-09");
    assert!(report.data_missing);
}

#[test]
fn wiring_total_cost_equals_sum_of_entries() {
    let metrics = WiringMetrics::new()
        .with(
            "tenant",
            "tenant_a",
            ResourceUsage {
                query_count: 100,
                ..Default::default()
            },
        )
        .with(
            "tenant",
            "tenant_b",
            ResourceUsage {
                query_count: 50,
                ..Default::default()
            },
        )
        .with(
            "database",
            "db_orders",
            ResourceUsage {
                io_count: 200,
                ..Default::default()
            },
        );
    let accountant = CostAccountant::new(default_config(), Box::new(metrics));
    let report = accountant.account_period("2026-09");
    let sum: f64 = report.entries.iter().map(|e| e.cost).sum();
    assert!((report.total_cost - sum).abs() < 0.0001);
}

#[test]
fn wiring_empty_metrics_produces_empty_report() {
    let metrics = WiringMetrics::new();
    let accountant = CostAccountant::new(default_config(), Box::new(metrics));
    let report = accountant.account_period("2026-09");
    assert!(report.entries.is_empty());
    assert_eq!(report.total_cost, 0.0);
    assert!(report.data_missing);
    assert!(!report.over_budget);
}

#[test]
fn wiring_period_label_in_report() {
    let metrics = WiringMetrics::new().with(
        "tenant",
        "tenant_a",
        ResourceUsage {
            query_count: 1,
            ..Default::default()
        },
    );
    let accountant = CostAccountant::new(default_config(), Box::new(metrics));
    let report = accountant.account_period("2026-Q3");
    assert_eq!(report.period, "2026-Q3");
}

#[test]
fn wiring_entry_contains_all_fields() {
    let metrics = WiringMetrics::new().with(
        "tenant",
        "tenant_a",
        ResourceUsage {
            connection_duration_ms: 100,
            io_count: 10,
            storage_bytes: 1000,
            query_count: 5,
        },
    );
    let accountant = CostAccountant::new(default_config(), Box::new(metrics));
    let report = accountant.account_period("2026-09");
    let entry = report
        .entries
        .iter()
        .find(|e| e.dimension_id == "tenant_a")
        .unwrap();
    assert_eq!(entry.dimension, CostDimension::Tenant);
    assert_eq!(entry.dimension_id, "tenant_a");
    assert_eq!(entry.resource_usage.connection_duration_ms, 100);
    assert_eq!(entry.resource_usage.io_count, 10);
    assert_eq!(entry.resource_usage.storage_bytes, 1000);
    assert_eq!(entry.resource_usage.query_count, 5);
    assert!(entry.budget.is_some());
    assert!(!entry.over_budget);
}
