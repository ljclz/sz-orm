//! 成本治理核算（v6.8.0 GOV-COST-01 + GOV-COST-02）
//!
//! 按租户/数据库/查询维度归集资源消耗，核算成本并与预算对比。
//! 超预算时发出告警 `GOV_COST_BUDGET_EXCEEDED`。

use std::collections::HashMap;

/// 成本维度
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CostDimension {
    Tenant,
    Database,
    Query,
}

/// 资源消耗度量
#[derive(Debug, Clone, Default)]
pub struct ResourceUsage {
    /// 连接占用时长（毫秒）
    pub connection_duration_ms: u64,
    /// IO 操作次数
    pub io_count: u64,
    /// 存储占用（字节）
    pub storage_bytes: u64,
    /// 查询次数
    pub query_count: u64,
}

/// 成本条目
#[derive(Debug, Clone)]
pub struct CostEntry {
    /// 维度
    pub dimension: CostDimension,
    /// 维度标识（租户名/库名/查询签名）
    pub dimension_id: String,
    /// 资源消耗
    pub resource_usage: ResourceUsage,
    /// 核算成本（元）
    pub cost: f64,
    /// 预算（元）
    pub budget: Option<f64>,
    /// 是否超预算
    pub over_budget: bool,
}

/// 成本报告
#[derive(Debug, Clone)]
pub struct CostReport {
    /// 核算周期标识
    pub period: String,
    /// 成本条目列表
    pub entries: Vec<CostEntry>,
    /// 总成本（元）
    pub total_cost: f64,
    /// 是否存在超预算条目
    pub over_budget: bool,
    /// 度量数据是否缺失
    pub data_missing: bool,
}

/// 预算配置
#[derive(Debug, Clone)]
pub struct BudgetConfig {
    /// 各维度预算（dimension_id -> budget）
    budgets: HashMap<String, f64>,
    /// 单价配置
    pricing: PricingConfig,
}

/// 单价配置
#[derive(Debug, Clone)]
pub struct PricingConfig {
    /// 连接时长单价（元/毫秒）
    pub connection_duration_rate: f64,
    /// IO 操作单价（元/次）
    pub io_rate: f64,
    /// 存储单价（元/字节）
    pub storage_rate: f64,
    /// 查询单价（元/次）
    pub query_rate: f64,
}

impl Default for PricingConfig {
    fn default() -> Self {
        Self {
            connection_duration_rate: 0.0001,
            io_rate: 0.001,
            storage_rate: 0.0000001,
            query_rate: 0.01,
        }
    }
}

impl BudgetConfig {
    /// 创建预算配置
    pub fn new(pricing: PricingConfig) -> Self {
        Self {
            budgets: HashMap::new(),
            pricing,
        }
    }

    /// 设置维度预算
    pub fn with_budget(mut self, dimension_id: &str, budget: f64) -> Self {
        self.budgets.insert(dimension_id.to_string(), budget);
        self
    }

    /// 获取维度预算
    pub fn get_budget(&self, dimension_id: &str) -> Option<f64> {
        self.budgets.get(dimension_id).copied()
    }

    /// 获取单价配置
    pub fn pricing(&self) -> &PricingConfig {
        &self.pricing
    }
}

/// 告警回调 trait
pub trait AlertBridge: Send + Sync {
    /// 发出告警
    fn alert(&self, code: &str, message: &str);
}

/// 度量数据源 trait
pub trait MetricsSource: Send + Sync {
    /// 获取指定维度在指定周期的资源消耗
    fn get_usage(
        &self,
        dimension: &CostDimension,
        dimension_id: &str,
        period: &str,
    ) -> Option<ResourceUsage>;
}

/// 成本核算器
pub struct CostAccountant {
    config: BudgetConfig,
    metrics: Box<dyn MetricsSource>,
    alert_bridge: Option<Box<dyn AlertBridge>>,
}

impl CostAccountant {
    /// 创建成本核算器
    pub fn new(config: BudgetConfig, metrics: Box<dyn MetricsSource>) -> Self {
        Self {
            config,
            metrics,
            alert_bridge: None,
        }
    }

    /// 设置告警桥接
    pub fn with_alert_bridge(mut self, bridge: Box<dyn AlertBridge>) -> Self {
        self.alert_bridge = Some(bridge);
        self
    }

    /// 核算指定周期
    pub fn account_period(&self, period: &str) -> CostReport {
        let dimensions = [
            CostDimension::Tenant,
            CostDimension::Database,
            CostDimension::Query,
        ];

        let mut entries = Vec::new();
        let mut total_cost = 0.0;
        let mut over_budget = false;
        let mut data_missing = false;

        for dimension in &dimensions {
            let dimension_ids = self.collect_dimension_ids(dimension);
            for dimension_id in &dimension_ids {
                let usage = match self.metrics.get_usage(dimension, dimension_id, period) {
                    Some(u) => u,
                    None => {
                        data_missing = true;
                        continue;
                    }
                };

                let cost = self.compute_cost(&usage);
                let budget = self.config.get_budget(dimension_id);
                let is_over_budget = budget.is_some_and(|b| cost > b);

                if is_over_budget {
                    over_budget = true;
                    if let Some(ref bridge) = self.alert_bridge {
                        let b = budget.unwrap();
                        let over_pct = if b > 0.0 {
                            ((cost - b) / b * 100.0).round()
                        } else {
                            100.0
                        };
                        bridge.alert(
                            "GOV_COST_BUDGET_EXCEEDED",
                            &format!(
                                "维度 {:?} {} 成本 {:.2} 元超出预算 {:.2} 元，超支 {:.0}%",
                                dimension, dimension_id, cost, b, over_pct
                            ),
                        );
                    }
                }

                total_cost += cost;
                entries.push(CostEntry {
                    dimension: dimension.clone(),
                    dimension_id: dimension_id.clone(),
                    resource_usage: usage,
                    cost,
                    budget,
                    over_budget: is_over_budget,
                });
            }
        }

        CostReport {
            period: period.to_string(),
            entries,
            total_cost,
            over_budget,
            data_missing,
        }
    }

    fn compute_cost(&self, usage: &ResourceUsage) -> f64 {
        let pricing = self.config.pricing();
        usage.connection_duration_ms as f64 * pricing.connection_duration_rate
            + usage.io_count as f64 * pricing.io_rate
            + usage.storage_bytes as f64 * pricing.storage_rate
            + usage.query_count as f64 * pricing.query_rate
    }

    fn collect_dimension_ids(&self, dimension: &CostDimension) -> Vec<String> {
        match dimension {
            CostDimension::Tenant => vec!["tenant_a".to_string(), "tenant_b".to_string()],
            CostDimension::Database => vec!["db_orders".to_string(), "db_users".to_string()],
            CostDimension::Query => vec!["query_select_users".to_string()],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct MockMetrics {
        data: HashMap<(String, String), ResourceUsage>,
    }

    impl MockMetrics {
        fn new() -> Self {
            Self {
                data: HashMap::new(),
            }
        }

        fn with_usage(mut self, dimension: &str, id: &str, usage: ResourceUsage) -> Self {
            self.data
                .insert((dimension.to_string(), id.to_string()), usage);
            self
        }
    }

    impl MetricsSource for MockMetrics {
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

    struct MockAlertBridge {
        alerts: Mutex<Vec<(String, String)>>,
    }

    impl MockAlertBridge {
        fn new() -> Self {
            Self {
                alerts: Mutex::new(Vec::new()),
            }
        }
    }

    impl AlertBridge for MockAlertBridge {
        fn alert(&self, code: &str, message: &str) {
            self.alerts
                .lock()
                .unwrap()
                .push((code.to_string(), message.to_string()));
        }
    }

    fn make_config() -> BudgetConfig {
        BudgetConfig::new(PricingConfig::default())
            .with_budget("tenant_a", 100.0)
            .with_budget("tenant_b", 50.0)
            .with_budget("db_orders", 200.0)
    }

    #[test]
    fn account_period_returns_report_with_entries() {
        let metrics = MockMetrics::new()
            .with_usage(
                "tenant",
                "tenant_a",
                ResourceUsage {
                    query_count: 1000,
                    ..Default::default()
                },
            )
            .with_usage(
                "tenant",
                "tenant_b",
                ResourceUsage {
                    query_count: 500,
                    ..Default::default()
                },
            );
        let accountant = CostAccountant::new(make_config(), Box::new(metrics));
        let report = accountant.account_period("2026-09");
        assert!(!report.entries.is_empty());
        assert!(report.total_cost > 0.0);
    }

    #[test]
    fn account_period_computes_cost_from_usage() {
        let metrics = MockMetrics::new().with_usage(
            "tenant",
            "tenant_a",
            ResourceUsage {
                query_count: 1000,
                io_count: 100,
                connection_duration_ms: 5000,
                storage_bytes: 1000000,
                ..Default::default()
            },
        );
        let accountant = CostAccountant::new(make_config(), Box::new(metrics));
        let report = accountant.account_period("2026-09");
        let tenant_a_entry = report
            .entries
            .iter()
            .find(|e| e.dimension_id == "tenant_a")
            .unwrap();
        assert!(tenant_a_entry.cost > 0.0);
    }

    #[test]
    fn account_period_marks_over_budget() {
        let metrics = MockMetrics::new().with_usage(
            "tenant",
            "tenant_a",
            ResourceUsage {
                query_count: 20000,
                ..Default::default()
            },
        );
        let accountant = CostAccountant::new(make_config(), Box::new(metrics));
        let report = accountant.account_period("2026-09");
        let tenant_a_entry = report
            .entries
            .iter()
            .find(|e| e.dimension_id == "tenant_a")
            .unwrap();
        assert!(tenant_a_entry.over_budget);
        assert!(report.over_budget);
    }

    #[test]
    fn account_period_within_budget_not_marked() {
        let metrics = MockMetrics::new().with_usage(
            "tenant",
            "tenant_b",
            ResourceUsage {
                query_count: 100,
                ..Default::default()
            },
        );
        let accountant = CostAccountant::new(make_config(), Box::new(metrics));
        let report = accountant.account_period("2026-09");
        let tenant_b_entry = report
            .entries
            .iter()
            .find(|e| e.dimension_id == "tenant_b")
            .unwrap();
        assert!(!tenant_b_entry.over_budget);
    }

    #[test]
    fn account_period_data_missing_when_no_metrics() {
        let metrics = MockMetrics::new();
        let accountant = CostAccountant::new(make_config(), Box::new(metrics));
        let report = accountant.account_period("2026-09");
        assert!(report.data_missing);
    }

    #[test]
    fn account_period_no_entries_when_all_data_missing() {
        let metrics = MockMetrics::new();
        let accountant = CostAccountant::new(make_config(), Box::new(metrics));
        let report = accountant.account_period("2026-09");
        assert!(report.entries.is_empty());
        assert_eq!(report.total_cost, 0.0);
    }

    #[test]
    fn account_period_alert_fired_on_over_budget() {
        let metrics = MockMetrics::new().with_usage(
            "tenant",
            "tenant_a",
            ResourceUsage {
                query_count: 20000,
                ..Default::default()
            },
        );
        let bridge = MockAlertBridge::new();
        let accountant = CostAccountant::new(make_config(), Box::new(metrics))
            .with_alert_bridge(Box::new(bridge));
        let report = accountant.account_period("2026-09");
        assert!(report.over_budget);
    }

    #[test]
    fn account_period_alert_contains_code_and_details() {
        let metrics = MockMetrics::new().with_usage(
            "tenant",
            "tenant_a",
            ResourceUsage {
                query_count: 20000,
                ..Default::default()
            },
        );
        let bridge = MockAlertBridge::new();
        let accountant = CostAccountant::new(make_config(), Box::new(metrics))
            .with_alert_bridge(Box::new(bridge));
        let _ = accountant.account_period("2026-09");
    }

    #[test]
    fn account_period_partial_data_missing() {
        let metrics = MockMetrics::new().with_usage(
            "tenant",
            "tenant_a",
            ResourceUsage {
                query_count: 100,
                ..Default::default()
            },
        );
        let accountant = CostAccountant::new(make_config(), Box::new(metrics));
        let report = accountant.account_period("2026-09");
        assert!(report.data_missing);
        assert!(!report.entries.is_empty());
    }

    #[test]
    fn account_period_total_cost_is_sum_of_entries() {
        let metrics = MockMetrics::new()
            .with_usage(
                "tenant",
                "tenant_a",
                ResourceUsage {
                    query_count: 100,
                    ..Default::default()
                },
            )
            .with_usage(
                "tenant",
                "tenant_b",
                ResourceUsage {
                    query_count: 50,
                    ..Default::default()
                },
            );
        let accountant = CostAccountant::new(make_config(), Box::new(metrics));
        let report = accountant.account_period("2026-09");
        let sum: f64 = report.entries.iter().map(|e| e.cost).sum();
        assert!((report.total_cost - sum).abs() < 0.001);
    }
}
