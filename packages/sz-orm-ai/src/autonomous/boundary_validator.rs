//! 边界校验器

use super::types::{ActionBoundary, AutonomousAction, AutonomousError};

/// 边界校验器：校验决策动作是否在策略配置的边界范围内
#[derive(Debug, Clone)]
pub struct BoundaryValidator;

impl BoundaryValidator {
    pub fn new() -> Self {
        Self
    }

    /// 校验动作值是否在边界范围内
    pub fn validate(
        &self,
        action: &AutonomousAction,
        value: f64,
        boundary: &ActionBoundary,
    ) -> Result<(), AutonomousError> {
        if value < boundary.min || value > boundary.max {
            return Err(AutonomousError::BoundaryExceeded {
                action: format!("{:?}", action),
                value,
                min: boundary.min,
                max: boundary.max,
            });
        }
        Ok(())
    }

    /// 校验多个参数
    pub fn validate_params(
        &self,
        action: &AutonomousAction,
        params: &[(f64, ActionBoundary)],
    ) -> Result<(), AutonomousError> {
        for (value, boundary) in params {
            self.validate(action, *value, boundary)?;
        }
        Ok(())
    }
}

impl Default for BoundaryValidator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_within_bounds() {
        let validator = BoundaryValidator::new();
        let boundary = ActionBoundary {
            min: 0.0,
            max: 100.0,
        };
        assert!(validator
            .validate(&AutonomousAction::AutoScaling, 50.0, &boundary)
            .is_ok());
    }

    #[test]
    fn test_validate_below_min() {
        let validator = BoundaryValidator::new();
        let boundary = ActionBoundary {
            min: 10.0,
            max: 100.0,
        };
        let result = validator.validate(&AutonomousAction::AutoScaling, 5.0, &boundary);
        assert!(matches!(
            result,
            Err(AutonomousError::BoundaryExceeded { .. })
        ));
    }

    #[test]
    fn test_validate_above_max() {
        let validator = BoundaryValidator::new();
        let boundary = ActionBoundary {
            min: 0.0,
            max: 100.0,
        };
        let result = validator.validate(&AutonomousAction::AutoScaling, 150.0, &boundary);
        assert!(matches!(
            result,
            Err(AutonomousError::BoundaryExceeded { .. })
        ));
    }

    #[test]
    fn test_validate_at_boundaries() {
        let validator = BoundaryValidator::new();
        let boundary = ActionBoundary {
            min: 0.0,
            max: 100.0,
        };
        assert!(validator
            .validate(&AutonomousAction::AutoTuning, 0.0, &boundary)
            .is_ok());
        assert!(validator
            .validate(&AutonomousAction::AutoTuning, 100.0, &boundary)
            .is_ok());
    }

    #[test]
    fn test_validate_multiple_params() {
        let validator = BoundaryValidator::new();
        let b1 = ActionBoundary {
            min: 0.0,
            max: 10.0,
        };
        let b2 = ActionBoundary {
            min: 0.0,
            max: 100.0,
        };
        assert!(validator
            .validate_params(
                &AutonomousAction::AutoTuning,
                &[(5.0, b1.clone()), (50.0, b2.clone())]
            )
            .is_ok());
        assert!(validator
            .validate_params(&AutonomousAction::AutoTuning, &[(15.0, b1), (50.0, b2)])
            .is_err());
    }
}
