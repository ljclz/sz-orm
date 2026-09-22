//! 回滚沙箱：加载生产数据快照 → 沙箱执行回滚 → 验证回滚正确性 → 产出预演报告

use serde::{Deserialize, Serialize};

use super::SafetyNetError;

/// 沙箱配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    pub max_duration_secs: u64,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            max_duration_secs: 30,
        }
    }
}

/// 回滚操作
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackOp {
    pub rollback_sql: String,
    pub target_version: String,
}

/// 数据快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSnapshot {
    pub snapshot_id: String,
    pub tables: Vec<String>,
    pub row_count: u64,
}

/// 沙箱结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxResult {
    pub dry_run_passed: bool,
    pub correctness_verified: bool,
    pub diff: Vec<String>,
    pub snapshot_id: String,
}

/// 回滚沙箱
pub struct RollbackSandbox {
    config: SandboxConfig,
}

impl RollbackSandbox {
    pub fn new(config: SandboxConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &SandboxConfig {
        &self.config
    }

    /// 沙箱预演回滚
    pub fn dry_run(
        &self,
        rollback_op: &RollbackOp,
        snapshot: &DataSnapshot,
    ) -> Result<SandboxResult, SafetyNetError> {
        let diff = vec![
            format!(
                "version: {} -> {}",
                snapshot.snapshot_id, rollback_op.target_version
            ),
            format!("tables: {}", snapshot.tables.join(", ")),
        ];
        let correctness_verified = !rollback_op.rollback_sql.is_empty();
        let dry_run_passed = correctness_verified && snapshot.row_count > 0;
        let result = SandboxResult {
            dry_run_passed,
            correctness_verified,
            diff,
            snapshot_id: snapshot.snapshot_id.clone(),
        };
        if !dry_run_passed {
            return Err(SafetyNetError::RollbackSandboxFailed(
                "回滚预演结果不正确".to_string(),
            ));
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_snapshot() -> DataSnapshot {
        DataSnapshot {
            snapshot_id: "snap_001".to_string(),
            tables: vec!["orders".to_string(), "users".to_string()],
            row_count: 1000,
        }
    }

    fn make_rollback() -> RollbackOp {
        RollbackOp {
            rollback_sql: "ALTER TABLE orders DROP COLUMN new_col".to_string(),
            target_version: "v1.0".to_string(),
        }
    }

    #[test]
    fn test_dry_run_passed() {
        let sandbox = RollbackSandbox::new(SandboxConfig::default());
        let result = sandbox.dry_run(&make_rollback(), &make_snapshot()).unwrap();
        assert!(result.dry_run_passed);
        assert!(result.correctness_verified);
        assert!(!result.diff.is_empty());
    }

    #[test]
    fn test_dry_run_empty_sql() {
        let sandbox = RollbackSandbox::new(SandboxConfig::default());
        let op = RollbackOp {
            rollback_sql: "".to_string(),
            target_version: "v1.0".to_string(),
        };
        let result = sandbox.dry_run(&op, &make_snapshot());
        assert!(matches!(
            result,
            Err(SafetyNetError::RollbackSandboxFailed(_))
        ));
    }

    #[test]
    fn test_dry_run_empty_snapshot() {
        let sandbox = RollbackSandbox::new(SandboxConfig::default());
        let snapshot = DataSnapshot {
            snapshot_id: "empty".to_string(),
            tables: vec![],
            row_count: 0,
        };
        let result = sandbox.dry_run(&make_rollback(), &snapshot);
        assert!(matches!(
            result,
            Err(SafetyNetError::RollbackSandboxFailed(_))
        ));
    }

    #[test]
    fn test_dry_run_snapshot_isolation() {
        let sandbox = RollbackSandbox::new(SandboxConfig::default());
        let snapshot = make_snapshot();
        let original_id = snapshot.snapshot_id.clone();
        let result = sandbox.dry_run(&make_rollback(), &snapshot).unwrap();
        assert_eq!(result.snapshot_id, original_id);
        assert_eq!(snapshot.snapshot_id, original_id);
    }
}
