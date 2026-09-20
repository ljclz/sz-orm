//! 冷热迁移调度器

use std::sync::Arc;

use parking_lot::RwLock;

use super::types::{ColdHotClassification, DataRange, LifecycleError, MigrationWindow};

/// 冷热迁移调度器：在配置的迁移窗口内执行冷热分离迁移
pub struct ColdHotMigrationScheduler {
    in_progress: Arc<RwLock<Vec<String>>>,
    cold_storage_available: Arc<RwLock<bool>>,
}

impl ColdHotMigrationScheduler {
    pub fn new() -> Self {
        Self {
            in_progress: Arc::new(RwLock::new(Vec::new())),
            cold_storage_available: Arc::new(RwLock::new(true)),
        }
    }

    pub fn set_cold_storage_available(&self, available: bool) {
        *self.cold_storage_available.write() = available;
    }

    pub fn is_cold_storage_available(&self) -> bool {
        *self.cold_storage_available.read()
    }

    pub fn in_window(&self, current_hour: u32, window: &MigrationWindow) -> bool {
        current_hour >= window.start_hour && current_hour < window.end_hour
    }

    pub fn schedule_migration(
        &self,
        classification: &ColdHotClassification,
        window: &MigrationWindow,
        current_hour: u32,
    ) -> Result<Vec<DataRange>, LifecycleError> {
        if !self.in_window(current_hour, window) {
            return Err(LifecycleError::MigrationWindowExceeded(format!(
                "当前时间 {} 不在迁移窗口 [{}, {}) 内",
                current_hour, window.start_hour, window.end_hour
            )));
        }
        if !self.is_cold_storage_available() {
            return Err(LifecycleError::ColdStorageUnavailable(
                "COLD_STORAGE_UNAVAILABLE".to_string(),
            ));
        }
        {
            let mut progress = self.in_progress.write();
            if progress.iter().any(|t| t == &classification.table) {
                return Err(LifecycleError::MigrationWindowExceeded(format!(
                    "表 {} 已有迁移在进行中",
                    classification.table
                )));
            }
            progress.push(classification.table.clone());
        }
        Ok(classification.cold_data.clone())
    }

    pub fn complete_migration(&self, table: &str) {
        let mut progress = self.in_progress.write();
        progress.retain(|t| t != table);
    }

    pub fn is_migration_in_progress(&self, table: &str) -> bool {
        self.in_progress.read().iter().any(|t| t == table)
    }

    pub fn pending_migrations(&self) -> Vec<String> {
        self.in_progress.read().clone()
    }
}

impl Default for ColdHotMigrationScheduler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::super::types::DataTemperature;
    use super::*;

    fn make_classification(table: &str, cold_count: usize) -> ColdHotClassification {
        ColdHotClassification {
            table: table.to_string(),
            hot_data: vec![],
            warm_data: vec![],
            cold_data: (0..cold_count)
                .map(|_| DataRange {
                    min_id: 0,
                    max_id: 100,
                    row_count: 100,
                    temperature: DataTemperature::Cold,
                    last_accessed_days_ago: 90,
                    access_frequency_per_day: 0.1,
                })
                .collect(),
        }
    }

    #[test]
    fn test_in_window() {
        let scheduler = ColdHotMigrationScheduler::new();
        let window = MigrationWindow::default();
        assert!(scheduler.in_window(3, &window));
        assert!(!scheduler.in_window(7, &window));
    }

    #[test]
    fn test_schedule_migration_success() {
        let scheduler = ColdHotMigrationScheduler::new();
        let classification = make_classification("orders", 3);
        let window = MigrationWindow::default();

        let result = scheduler.schedule_migration(&classification, &window, 3);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 3);
        assert!(scheduler.is_migration_in_progress("orders"));
    }

    #[test]
    fn test_schedule_migration_out_of_window() {
        let scheduler = ColdHotMigrationScheduler::new();
        let classification = make_classification("orders", 1);
        let window = MigrationWindow::default();

        let result = scheduler.schedule_migration(&classification, &window, 10);
        assert!(matches!(
            result,
            Err(LifecycleError::MigrationWindowExceeded(_))
        ));
    }

    #[test]
    fn test_schedule_migration_cold_storage_unavailable() {
        let scheduler = ColdHotMigrationScheduler::new();
        scheduler.set_cold_storage_available(false);
        let classification = make_classification("orders", 1);
        let window = MigrationWindow::default();

        let result = scheduler.schedule_migration(&classification, &window, 3);
        assert!(matches!(
            result,
            Err(LifecycleError::ColdStorageUnavailable(_))
        ));
    }

    #[test]
    fn test_schedule_migration_already_in_progress() {
        let scheduler = ColdHotMigrationScheduler::new();
        let classification = make_classification("orders", 1);
        let window = MigrationWindow::default();

        scheduler
            .schedule_migration(&classification, &window, 3)
            .unwrap();
        let result = scheduler.schedule_migration(&classification, &window, 3);
        assert!(result.is_err());
    }

    #[test]
    fn test_complete_migration() {
        let scheduler = ColdHotMigrationScheduler::new();
        let classification = make_classification("orders", 1);
        let window = MigrationWindow::default();

        scheduler
            .schedule_migration(&classification, &window, 3)
            .unwrap();
        assert!(scheduler.is_migration_in_progress("orders"));

        scheduler.complete_migration("orders");
        assert!(!scheduler.is_migration_in_progress("orders"));
    }
}
