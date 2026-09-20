//! SLI/SLO 核心数据结构

/// SLI 类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SliType {
    Availability,
    Latency,
    Throughput,
    Correctness,
}

/// 计算窗口
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CalcWindow {
    OneHour,
    TwentyFourHours,
    SevenDays,
    TwentyDays,
}

/// 预算周期
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum BudgetPeriod {
    Monthly,
    Quarterly,
}

/// 仪表盘格式
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DashboardFormat {
    GrafanaJson,
}

/// SLO 自动化配置
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SloAutomationConfig {
    pub sli_type: SliType,
    pub slo_target: f64,
    pub calc_window: CalcWindow,
    pub budget_period: BudgetPeriod,
    pub retention_days: u32,
    pub dashboard_format: DashboardFormat,
}

impl Default for SloAutomationConfig {
    fn default() -> Self {
        Self {
            sli_type: SliType::Availability,
            slo_target: 0.999,
            calc_window: CalcWindow::TwentyFourHours,
            budget_period: BudgetPeriod::Monthly,
            retention_days: 30,
            dashboard_format: DashboardFormat::GrafanaJson,
        }
    }
}

/// SLI 指标
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SliMetrics {
    pub timestamp: i64,
    pub availability: f64,
    pub latency_ms: f64,
    pub throughput: f64,
    pub correctness: f64,
}

/// 请求结果
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RequestResult {
    pub success: bool,
    pub latency_ms: f64,
    pub timestamp: i64,
}

/// SLO 达成率
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SloAchievement {
    pub window: CalcWindow,
    pub achievement_rate: f64,
    pub sufficient_data: bool,
}

/// 错误预算
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ErrorBudget {
    pub total_budget: f64,
    pub consumed: f64,
    pub remaining: f64,
    pub exhausted: bool,
}

impl ErrorBudget {
    pub fn new(total_budget: f64) -> Self {
        Self {
            total_budget,
            consumed: 0.0,
            remaining: total_budget,
            exhausted: false,
        }
    }

    pub fn update(&mut self, consumed: f64) {
        self.consumed += consumed;
        self.remaining = (self.total_budget - self.consumed).max(0.0);
        self.exhausted = self.remaining <= 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = SloAutomationConfig::default();
        assert_eq!(config.slo_target, 0.999);
        assert_eq!(config.retention_days, 30);
    }

    #[test]
    fn test_error_budget() {
        let mut budget = ErrorBudget::new(100.0);
        budget.update(30.0);
        assert_eq!(budget.remaining, 70.0);
        assert!(!budget.exhausted);

        budget.update(70.0);
        assert!(budget.exhausted);
    }
}
