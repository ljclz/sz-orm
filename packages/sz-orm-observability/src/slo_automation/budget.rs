//! 错误预算追踪器

use super::types::ErrorBudget;

/// 错误预算追踪器
pub struct ErrorBudgetTracker {
    budget: ErrorBudget,
}

impl ErrorBudgetTracker {
    pub fn new(total_budget: f64) -> Self {
        Self {
            budget: ErrorBudget::new(total_budget),
        }
    }

    pub fn remaining_budget(&self) -> f64 {
        self.budget.remaining
    }

    pub fn allow_non_critical_change(&self) -> bool {
        !self.budget.exhausted
    }

    pub fn update(&mut self, consumed: f64) -> bool {
        self.budget.update(consumed);
        self.budget.exhausted
    }

    pub fn is_exhausted(&self) -> bool {
        self.budget.exhausted
    }

    pub fn budget(&self) -> &ErrorBudget {
        &self.budget
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_budget_not_exhausted() {
        let tracker = ErrorBudgetTracker::new(100.0);
        assert!(tracker.allow_non_critical_change());
        assert!(!tracker.is_exhausted());
    }

    #[test]
    fn test_budget_exhausted() {
        let mut tracker = ErrorBudgetTracker::new(100.0);
        tracker.update(100.0);
        assert!(!tracker.allow_non_critical_change());
        assert!(tracker.is_exhausted());
    }

    #[test]
    fn test_partial_consumption() {
        let mut tracker = ErrorBudgetTracker::new(100.0);
        tracker.update(30.0);
        assert_eq!(tracker.remaining_budget(), 70.0);
        assert!(tracker.allow_non_critical_change());
    }
}
