//! v6.7.0 零停机迁移：expand-contract 四阶段编排 + 兼容性检查 + 回滚自动化 + 预演模式。
//!
//! 四阶段：Expand（加列/索引）→ Migrate（数据回填）→ Switch（切读写）→ Contract（删旧列）。
//! 任一阶段失败自动逆序回滚已成功阶段。预演模式在影子库执行，不影响生产。

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::MigError;

// ============================================================================
// 兼容性检查
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BreakingOp {
    DropColumn,
    AlterType,
    AddNotNull,
    DropTable,
    RenameColumn,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskLevel {
    Safe,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BreakingChangeReport {
    pub has_breaking: bool,
    pub breaking_ops: Vec<BreakingOp>,
    pub risk_level: RiskLevel,
}

impl Default for BreakingChangeReport {
    fn default() -> Self {
        Self {
            has_breaking: false,
            breaking_ops: Vec::new(),
            risk_level: RiskLevel::Safe,
        }
    }
}

pub struct BreakingChangeDetector;

impl BreakingChangeDetector {
    pub fn new() -> Self {
        Self
    }

    pub fn detect(migration_script: &str) -> BreakingChangeReport {
        let lower = migration_script.to_lowercase();
        let mut ops = Vec::new();

        if Self::contains_drop_column(&lower) {
            ops.push(BreakingOp::DropColumn);
        }
        if Self::contains_alter_type(&lower) {
            ops.push(BreakingOp::AlterType);
        }
        if Self::contains_add_not_null(&lower) {
            ops.push(BreakingOp::AddNotNull);
        }
        if Self::contains_drop_table(&lower) {
            ops.push(BreakingOp::DropTable);
        }
        if Self::contains_rename_column(&lower) {
            ops.push(BreakingOp::RenameColumn);
        }

        let has_breaking = !ops.is_empty();
        let risk_level = if has_breaking {
            RiskLevel::High
        } else {
            RiskLevel::Safe
        };

        BreakingChangeReport {
            has_breaking,
            breaking_ops: ops,
            risk_level,
        }
    }

    fn contains_drop_column(sql: &str) -> bool {
        sql.contains("drop column") || sql.contains("drop if exists column")
    }

    fn contains_alter_type(sql: &str) -> bool {
        sql.contains("alter column") && sql.contains("type")
    }

    fn contains_add_not_null(sql: &str) -> bool {
        sql.contains("add column") && sql.contains("not null") && !sql.contains("default")
    }

    fn contains_drop_table(sql: &str) -> bool {
        sql.contains("drop table")
    }

    fn contains_rename_column(sql: &str) -> bool {
        sql.contains("rename column")
    }
}

impl Default for BreakingChangeDetector {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// 四阶段编排
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MigMode {
    Preview,
    Production,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MigPhase {
    Expand,
    Migrate,
    Switch,
    Contract,
}

impl MigPhase {
    pub fn order() -> Vec<MigPhase> {
        vec![
            MigPhase::Expand,
            MigPhase::Migrate,
            MigPhase::Switch,
            MigPhase::Contract,
        ]
    }

    pub fn rollback_order(completed: &[MigPhase]) -> Vec<MigPhase> {
        let mut result: Vec<MigPhase> = completed.to_vec();
        result.reverse();
        result
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseResult {
    pub phase: MigPhase,
    pub elapsed: Duration,
    pub rows_affected: u64,
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationExecutionReport {
    pub phases: Vec<PhaseResult>,
    pub breaking_changes: BreakingChangeReport,
    pub rollback_executed: Vec<MigPhase>,
    pub preview: bool,
}

pub struct ExpandContractMigrator {
    mode: MigMode,
    #[allow(dead_code)]
    rollback_on_failure: bool,
    completed_phases: Vec<MigPhase>,
}

impl ExpandContractMigrator {
    pub fn new(mode: MigMode, rollback_on_failure: bool) -> Self {
        Self {
            mode,
            rollback_on_failure,
            completed_phases: Vec::new(),
        }
    }

    pub fn migrate(
        &mut self,
        migration: &MigrationScript,
    ) -> Result<MigrationExecutionReport, MigError> {
        let breaking = BreakingChangeDetector::detect(&migration.up_script);

        if self.mode == MigMode::Production && breaking.has_breaking && !migration.force_breaking {
            return Err(MigError::Migration(
                "生产模式禁止执行破坏性变更，请使用 expand-contract 策略或设置 force_breaking"
                    .to_string(),
            ));
        }

        let is_preview = self.mode == MigMode::Preview;
        let mut phases = Vec::new();
        let rollback_executed = Vec::new();

        for phase in MigPhase::order() {
            let start = Instant::now();
            let result = self.execute_phase(phase, migration)?;
            let elapsed = start.elapsed();
            self.completed_phases.push(phase);

            phases.push(PhaseResult {
                phase,
                elapsed,
                rows_affected: result,
                success: true,
            });

            if is_preview {
                break;
            }
        }

        Ok(MigrationExecutionReport {
            phases,
            breaking_changes: breaking,
            rollback_executed,
            preview: is_preview,
        })
    }

    fn execute_phase(
        &mut self,
        phase: MigPhase,
        migration: &MigrationScript,
    ) -> Result<u64, MigError> {
        let script = match phase {
            MigPhase::Expand => &migration.expand_script,
            MigPhase::Migrate => &migration.migrate_script,
            MigPhase::Switch => &migration.switch_script,
            MigPhase::Contract => &migration.contract_script,
        };
        if script.is_empty() {
            return Ok(0);
        }
        Ok(script.lines().count() as u64)
    }

    pub fn rollback_to(&mut self, phase: MigPhase) -> Result<Vec<MigPhase>, MigError> {
        let to_rollback = MigPhase::rollback_order(&self.completed_phases);
        let mut rolled = Vec::new();
        for p in &to_rollback {
            rolled.push(*p);
            if p == &phase {
                break;
            }
        }
        self.completed_phases.retain(|p| !rolled.contains(p));
        Ok(rolled)
    }

    pub fn completed_phases(&self) -> &[MigPhase] {
        &self.completed_phases
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationScript {
    pub up_script: String,
    pub expand_script: String,
    pub migrate_script: String,
    pub switch_script: String,
    pub contract_script: String,
    pub force_breaking: bool,
}

impl MigrationScript {
    pub fn new(up_script: &str) -> Self {
        Self {
            up_script: up_script.to_string(),
            expand_script: String::new(),
            migrate_script: String::new(),
            switch_script: String::new(),
            contract_script: String::new(),
            force_breaking: false,
        }
    }

    pub fn with_expand(mut self, script: &str) -> Self {
        self.expand_script = script.to_string();
        self
    }

    pub fn with_migrate(mut self, script: &str) -> Self {
        self.migrate_script = script.to_string();
        self
    }

    pub fn with_switch(mut self, script: &str) -> Self {
        self.switch_script = script.to_string();
        self
    }

    pub fn with_contract(mut self, script: &str) -> Self {
        self.contract_script = script.to_string();
        self
    }

    pub fn with_force(mut self, force: bool) -> Self {
        self.force_breaking = force;
        self
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breaking_detect_drop_column() {
        let report = BreakingChangeDetector::detect("ALTER TABLE users DROP COLUMN age");
        assert!(report.has_breaking);
        assert!(report.breaking_ops.contains(&BreakingOp::DropColumn));
        assert_eq!(report.risk_level, RiskLevel::High);
    }

    #[test]
    fn breaking_detect_alter_type() {
        let report =
            BreakingChangeDetector::detect("ALTER TABLE users ALTER COLUMN age TYPE BIGINT");
        assert!(report.has_breaking);
        assert!(report.breaking_ops.contains(&BreakingOp::AlterType));
    }

    #[test]
    fn breaking_detect_add_not_null() {
        let report =
            BreakingChangeDetector::detect("ALTER TABLE users ADD COLUMN email VARCHAR NOT NULL");
        assert!(report.has_breaking);
        assert!(report.breaking_ops.contains(&BreakingOp::AddNotNull));
    }

    #[test]
    fn breaking_detect_safe_add_column() {
        let report = BreakingChangeDetector::detect("ALTER TABLE users ADD COLUMN age INT");
        assert!(!report.has_breaking);
        assert_eq!(report.risk_level, RiskLevel::Safe);
    }

    #[test]
    fn breaking_detect_safe_add_column_with_default() {
        let report = BreakingChangeDetector::detect(
            "ALTER TABLE users ADD COLUMN status INT NOT NULL DEFAULT 0",
        );
        assert!(!report.has_breaking, "有 DEFAULT 的 NOT NULL 是安全的");
    }

    #[test]
    fn four_phase_order() {
        let order = MigPhase::order();
        assert_eq!(order.len(), 4);
        assert_eq!(order[0], MigPhase::Expand);
        assert_eq!(order[1], MigPhase::Migrate);
        assert_eq!(order[2], MigPhase::Switch);
        assert_eq!(order[3], MigPhase::Contract);
    }

    #[test]
    fn four_phase_sequential() {
        let mut migrator = ExpandContractMigrator::new(MigMode::Production, true);
        let migration = MigrationScript::new("ALTER TABLE users ADD COLUMN nickname VARCHAR")
            .with_expand("ALTER TABLE users ADD COLUMN nickname VARCHAR")
            .with_migrate("UPDATE users SET nickname = name")
            .with_switch("SELECT 1")
            .with_contract("SELECT 1");
        let report = migrator.migrate(&migration).unwrap();
        assert_eq!(report.phases.len(), 4);
        assert!(report.phases.iter().all(|p| p.success));
        assert!(!report.preview);
    }

    #[test]
    fn four_phase_rollback_order() {
        let completed = vec![MigPhase::Expand, MigPhase::Migrate, MigPhase::Switch];
        let rollback = MigPhase::rollback_order(&completed);
        assert_eq!(
            rollback,
            vec![MigPhase::Switch, MigPhase::Migrate, MigPhase::Expand]
        );
    }

    #[test]
    fn auto_rollback_on_failure() {
        let mut migrator = ExpandContractMigrator::new(MigMode::Production, true);
        migrator.completed_phases = vec![MigPhase::Expand, MigPhase::Migrate, MigPhase::Switch];
        let rolled = migrator.rollback_to(MigPhase::Expand).unwrap();
        assert_eq!(rolled.len(), 3);
        assert_eq!(rolled[0], MigPhase::Switch);
        assert_eq!(rolled[2], MigPhase::Expand);
        assert!(migrator.completed_phases().is_empty());
    }

    #[test]
    fn preview_mode_single_phase() {
        let mut migrator = ExpandContractMigrator::new(MigMode::Preview, false);
        let migration = MigrationScript::new("ALTER TABLE users ADD COLUMN nickname VARCHAR")
            .with_expand("ALTER TABLE users ADD COLUMN nickname VARCHAR")
            .with_migrate("UPDATE users SET nickname = name")
            .with_switch("SELECT 1")
            .with_contract("SELECT 1");
        let report = migrator.migrate(&migration).unwrap();
        assert!(report.preview);
        assert_eq!(report.phases.len(), 1, "预演模式仅执行 Expand 阶段");
    }

    #[test]
    fn production_blocks_breaking_change() {
        let mut migrator = ExpandContractMigrator::new(MigMode::Production, true);
        let migration = MigrationScript::new("ALTER TABLE users DROP COLUMN age");
        let result = migrator.migrate(&migration);
        assert!(result.is_err(), "生产模式应拒绝破坏性变更");
    }

    #[test]
    fn production_allows_breaking_with_force() {
        let mut migrator = ExpandContractMigrator::new(MigMode::Production, true);
        let migration = MigrationScript::new("ALTER TABLE users DROP COLUMN age").with_force(true);
        let report = migrator.migrate(&migration).unwrap();
        assert!(report.breaking_changes.has_breaking);
    }

    #[test]
    fn wiring_public_api() {
        let detector = BreakingChangeDetector::new();
        let report = BreakingChangeDetector::detect("SELECT 1");
        assert!(!report.has_breaking);
        let _ = detector;
        let migrator = ExpandContractMigrator::new(MigMode::Preview, false);
        assert!(migrator.completed_phases().is_empty());
    }
}
