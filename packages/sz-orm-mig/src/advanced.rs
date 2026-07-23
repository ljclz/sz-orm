//! # 高级迁移功能
//!
//! 提供迁移回滚（Down Migration）、迁移状态跟踪、迁移冲突检测和
//! ETL 数据管道等高级迁移能力。

use crate::error::MigError;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

// ====================================================================
// 迁移定义：支持 Up/Down 双向迁移
// ====================================================================

/// 单个迁移定义，包含正向（Up）和反向（Down）SQL 脚本
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Migration {
    /// 迁移版本号（唯一标识，通常为时间戳或递增整数）
    pub version: u64,
    /// 迁移名称/描述
    pub name: String,
    /// 正向迁移 SQL（应用迁移时执行）
    pub up_sql: String,
    /// 反向迁移 SQL（回滚迁移时执行），为空表示不可回滚
    pub down_sql: String,
    /// 此迁移涉及的表名列表（用于冲突检测）
    pub affected_tables: Vec<String>,
}

impl Migration {
    /// 创建新的迁移定义
    pub fn new(
        version: u64,
        name: impl Into<String>,
        up_sql: impl Into<String>,
        down_sql: impl Into<String>,
    ) -> Self {
        Self {
            version,
            name: name.into(),
            up_sql: up_sql.into(),
            down_sql: down_sql.into(),
            affected_tables: Vec::new(),
        }
    }

    /// 设置此迁移涉及的表名列表
    pub fn with_tables(mut self, tables: Vec<String>) -> Self {
        self.affected_tables = tables;
        self
    }

    /// 判断此迁移是否可回滚（down_sql 非空）
    pub fn is_reversible(&self) -> bool {
        !self.down_sql.trim().is_empty()
    }
}

// ====================================================================
// 迁移状态跟踪
// ====================================================================

/// 迁移的执行状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MigrationStatus {
    /// 尚未执行
    Pending,
    /// 已成功应用
    Applied,
    /// 执行失败
    Failed,
    /// 已回滚
    RolledBack,
}

impl MigrationStatus {
    /// 判断迁移是否已应用
    pub fn is_applied(&self) -> bool {
        matches!(self, MigrationStatus::Applied)
    }

    /// 判断迁移是否可回滚（仅已应用的迁移可回滚）
    pub fn can_rollback(&self) -> bool {
        matches!(self, MigrationStatus::Applied)
    }
}

/// 单条迁移状态记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationRecord {
    /// 迁移版本号
    pub version: u64,
    /// 迁移名称
    pub name: String,
    /// 当前状态
    pub status: MigrationStatus,
    /// 应用时间戳（Unix 毫秒），None 表示未应用
    pub applied_at: Option<i64>,
    /// 回滚时间戳（Unix 毫秒），None 表示未回滚
    pub rolled_back_at: Option<i64>,
    /// 执行耗时（毫秒）
    pub duration_ms: u64,
    /// 错误信息（执行失败时记录）
    pub error: Option<String>,
}

impl MigrationRecord {
    /// 创建一条 Pending 状态的记录
    pub fn pending(version: u64, name: impl Into<String>) -> Self {
        Self {
            version,
            name: name.into(),
            status: MigrationStatus::Pending,
            applied_at: None,
            rolled_back_at: None,
            duration_ms: 0,
            error: None,
        }
    }

    /// 标记为已应用
    pub fn mark_applied(&mut self, duration_ms: u64) {
        self.status = MigrationStatus::Applied;
        self.applied_at = Some(current_timestamp_millis());
        self.duration_ms = duration_ms;
        self.error = None;
    }

    /// 标记为已回滚
    pub fn mark_rolled_back(&mut self, duration_ms: u64) {
        self.status = MigrationStatus::RolledBack;
        self.rolled_back_at = Some(current_timestamp_millis());
        self.duration_ms = duration_ms;
    }

    /// 标记为失败
    pub fn mark_failed(&mut self, error: impl Into<String>, duration_ms: u64) {
        self.status = MigrationStatus::Failed;
        self.error = Some(error.into());
        self.duration_ms = duration_ms;
    }
}

/// 迁移状态跟踪器：记录所有迁移的执行状态
pub struct MigrationTracker {
    /// 已注册的迁移记录（version -> MigrationRecord）
    records: Mutex<HashMap<u64, MigrationRecord>>,
}

impl MigrationTracker {
    pub fn new() -> Self {
        Self {
            records: Mutex::new(HashMap::new()),
        }
    }

    /// 注册一个迁移（初始状态为 Pending）
    pub fn register(&self, migration: &Migration) -> Result<(), MigError> {
        let mut records = self.records.lock().map_err(|e| {
            MigError::Migration(format!("failed to lock migration records: {}", e))
        })?;
        if records.contains_key(&migration.version) {
            return Err(MigError::Validation(format!(
                "migration version {} already registered",
                migration.version
            )));
        }
        records.insert(
            migration.version,
            MigrationRecord::pending(migration.version, &migration.name),
        );
        Ok(())
    }

    /// 记录迁移已应用
    pub fn record_applied(&self, version: u64, duration_ms: u64) -> Result<(), MigError> {
        let mut records = self.records.lock().map_err(|e| {
            MigError::Migration(format!("failed to lock migration records: {}", e))
        })?;
        let record = records
            .get_mut(&version)
            .ok_or_else(|| MigError::Validation(format!("migration {} not found", version)))?;
        record.mark_applied(duration_ms);
        Ok(())
    }

    /// 记录迁移已回滚
    pub fn record_rolled_back(&self, version: u64, duration_ms: u64) -> Result<(), MigError> {
        let mut records = self.records.lock().map_err(|e| {
            MigError::Migration(format!("failed to lock migration records: {}", e))
        })?;
        let record = records
            .get_mut(&version)
            .ok_or_else(|| MigError::Validation(format!("migration {} not found", version)))?;
        if !record.status.can_rollback() {
            return Err(MigError::Validation(format!(
                "migration {} cannot be rolled back (current status: {:?})",
                version, record.status
            )));
        }
        record.mark_rolled_back(duration_ms);
        Ok(())
    }

    /// 记录迁移执行失败
    pub fn record_failed(
        &self,
        version: u64,
        error: impl Into<String>,
        duration_ms: u64,
    ) -> Result<(), MigError> {
        let mut records = self.records.lock().map_err(|e| {
            MigError::Migration(format!("failed to lock migration records: {}", e))
        })?;
        let record = records
            .get_mut(&version)
            .ok_or_else(|| MigError::Validation(format!("migration {} not found", version)))?;
        record.mark_failed(error, duration_ms);
        Ok(())
    }

    /// 获取指定迁移的状态
    pub fn get_status(&self, version: u64) -> Option<MigrationStatus> {
        self.records
            .lock()
            .ok()
            .and_then(|r| r.get(&version).map(|rec| rec.status))
    }

    /// 获取指定迁移的完整记录
    pub fn get_record(&self, version: u64) -> Option<MigrationRecord> {
        self.records
            .lock()
            .ok()
            .and_then(|r| r.get(&version).cloned())
    }

    /// 返回所有已应用的迁移版本号（按版本排序）
    pub fn applied_versions(&self) -> Vec<u64> {
        let mut versions: Vec<u64> = self
            .records
            .lock()
            .map(|r| {
                r.values()
                    .filter(|rec| rec.status.is_applied())
                    .map(|rec| rec.version)
                    .collect()
            })
            .unwrap_or_default();
        versions.sort();
        versions
    }

    /// 返回所有待执行的迁移版本号（按版本排序）
    pub fn pending_versions(&self) -> Vec<u64> {
        let mut versions: Vec<u64> = self
            .records
            .lock()
            .map(|r| {
                r.values()
                    .filter(|rec| rec.status == MigrationStatus::Pending)
                    .map(|rec| rec.version)
                    .collect()
            })
            .unwrap_or_default();
        versions.sort();
        versions
    }

    /// 返回已注册的迁移总数
    pub fn count(&self) -> usize {
        self.records
            .lock()
            .map(|r| r.len())
            .unwrap_or(0)
    }

    /// 返回所有迁移记录的快照（按版本排序）
    pub fn all_records(&self) -> Vec<MigrationRecord> {
        let mut records: Vec<MigrationRecord> = self
            .records
            .lock()
            .map(|r| r.values().cloned().collect())
            .unwrap_or_default();
        records.sort_by_key(|r| r.version);
        records
    }

    /// 返回最后一个已应用的迁移版本号
    pub fn last_applied_version(&self) -> Option<u64> {
        self.applied_versions().last().copied()
    }
}

impl Default for MigrationTracker {
    fn default() -> Self {
        Self::new()
    }
}

// ====================================================================
// 迁移冲突检测
// ====================================================================

/// 迁移冲突类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConflictType {
    /// 版本号重复
    DuplicateVersion(u64),
    /// 两个迁移修改了同一张表且版本号相邻
    TableConflict {
        table: String,
        version_a: u64,
        version_b: u64,
    },
    /// 迁移引用了不存在的依赖
    MissingDependency {
        version: u64,
        dependency: u64,
    },
}

/// 迁移冲突检测结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictReport {
    /// 检测到的所有冲突
    pub conflicts: Vec<ConflictType>,
}

impl ConflictReport {
    /// 判断是否存在冲突
    pub fn has_conflicts(&self) -> bool {
        !self.conflicts.is_empty()
    }

    /// 返回冲突数量
    pub fn conflict_count(&self) -> usize {
        self.conflicts.len()
    }
}

/// 迁移冲突检测器：检测迁移集合中的潜在冲突
pub struct ConflictDetector;

impl ConflictDetector {
    pub fn new() -> Self {
        Self
    }

    /// 检测给定迁移列表中的所有冲突
    pub fn detect(migrations: &[Migration]) -> ConflictReport {
        let mut conflicts = Vec::new();

        // 1. 检测版本号重复
        let mut seen_versions: HashSet<u64> = HashSet::new();
        for mig in migrations {
            if !seen_versions.insert(mig.version) {
                conflicts.push(ConflictType::DuplicateVersion(mig.version));
            }
        }

        // 2. 检测表冲突：同一张表被多个迁移修改
        let mut table_owners: HashMap<String, Vec<u64>> = HashMap::new();
        for mig in migrations {
            for table in &mig.affected_tables {
                table_owners
                    .entry(table.clone())
                    .or_default()
                    .push(mig.version);
            }
        }
        for (table, versions) in &table_owners {
            if versions.len() > 1 {
                // 同一张表被多个版本修改，报告第一对冲突
                let mut sorted = versions.clone();
                sorted.sort();
                for i in 0..sorted.len().saturating_sub(1) {
                    conflicts.push(ConflictType::TableConflict {
                        table: table.clone(),
                        version_a: sorted[i],
                        version_b: sorted[i + 1],
                    });
                }
            }
        }

        ConflictReport { conflicts }
    }

    /// 检测迁移是否引用了不存在的依赖版本
    /// dependencies: (migration_version, depends_on_version) 列表
    pub fn detect_dependencies(
        migrations: &[Migration],
        dependencies: &[(u64, u64)],
    ) -> ConflictReport {
        let mut conflicts = Vec::new();
        let existing_versions: HashSet<u64> = migrations.iter().map(|m| m.version).collect();

        for (version, dependency) in dependencies {
            if !existing_versions.contains(dependency) {
                conflicts.push(ConflictType::MissingDependency {
                    version: *version,
                    dependency: *dependency,
                });
            }
        }

        ConflictReport { conflicts }
    }
}

impl Default for ConflictDetector {
    fn default() -> Self {
        Self::new()
    }
}

// ====================================================================
// 迁移执行器：支持 Up/Down 双向执行
// ====================================================================

/// 迁移执行结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationExecutionResult {
    /// 执行的迁移版本号
    pub version: u64,
    /// 执行方向（"up" 或 "down"）
    pub direction: String,
    /// 是否成功
    pub success: bool,
    /// 执行耗时（毫秒）
    pub duration_ms: u64,
    /// 错误信息（失败时记录）
    pub error: Option<String>,
}

/// 迁移执行器：协调迁移定义、状态跟踪和冲突检测
pub struct MigrationExecutor {
    /// 已注册的迁移定义（version -> Migration）
    migrations: Mutex<HashMap<u64, Migration>>,
    /// 状态跟踪器
    tracker: MigrationTracker,
}

impl MigrationExecutor {
    pub fn new() -> Self {
        Self {
            migrations: Mutex::new(HashMap::new()),
            tracker: MigrationTracker::new(),
        }
    }

    /// 添加迁移定义
    pub fn add_migration(&self, migration: Migration) -> Result<(), MigError> {
        let mut migrations = self.migrations.lock().map_err(|e| {
            MigError::Migration(format!("failed to lock migrations: {}", e))
        })?;
        if migrations.contains_key(&migration.version) {
            return Err(MigError::Validation(format!(
                "migration version {} already exists",
                migration.version
            )));
        }
        self.tracker.register(&migration)?;
        migrations.insert(migration.version, migration);
        Ok(())
    }

    /// 执行指定迁移的正向（Up）脚本
    /// 返回执行结果，actual_execution 为 true 时实际调用 executor_fn
    pub fn execute_up<F>(&self, version: u64, executor_fn: F) -> MigrationExecutionResult
    where
        F: FnOnce(&str) -> Result<(), String>,
    {
        let start = std::time::Instant::now();
        let migrations = match self.migrations.lock() {
            Ok(m) => m,
            Err(_) => {
                return MigrationExecutionResult {
                    version,
                    direction: "up".to_string(),
                    success: false,
                    duration_ms: 0,
                    error: Some("failed to lock migrations".to_string()),
                }
            }
        };
        let migration = match migrations.get(&version) {
            Some(m) => m,
            None => {
                return MigrationExecutionResult {
                    version,
                    direction: "up".to_string(),
                    success: false,
                    duration_ms: 0,
                    error: Some(format!("migration {} not found", version)),
                }
            }
        };

        let result = executor_fn(&migration.up_sql);
        let duration_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(()) => {
                let _ = self.tracker.record_applied(version, duration_ms);
                MigrationExecutionResult {
                    version,
                    direction: "up".to_string(),
                    success: true,
                    duration_ms,
                    error: None,
                }
            }
            Err(e) => {
                let _ = self.tracker.record_failed(version, &e, duration_ms);
                MigrationExecutionResult {
                    version,
                    direction: "up".to_string(),
                    success: false,
                    duration_ms,
                    error: Some(e),
                }
            }
        }
    }

    /// 执行指定迁移的反向（Down）脚本
    pub fn execute_down<F>(&self, version: u64, executor_fn: F) -> MigrationExecutionResult
    where
        F: FnOnce(&str) -> Result<(), String>,
    {
        let start = std::time::Instant::now();
        let migrations = match self.migrations.lock() {
            Ok(m) => m,
            Err(_) => {
                return MigrationExecutionResult {
                    version,
                    direction: "down".to_string(),
                    success: false,
                    duration_ms: 0,
                    error: Some("failed to lock migrations".to_string()),
                }
            }
        };
        let migration = match migrations.get(&version) {
            Some(m) => m,
            None => {
                return MigrationExecutionResult {
                    version,
                    direction: "down".to_string(),
                    success: false,
                    duration_ms: 0,
                    error: Some(format!("migration {} not found", version)),
                }
            }
        };

        if !migration.is_reversible() {
            return MigrationExecutionResult {
                version,
                direction: "down".to_string(),
                success: false,
                duration_ms: 0,
                error: Some(format!("migration {} is not reversible", version)),
            };
        }

        // 检查迁移是否已应用
        let status = self.tracker.get_status(version);
        if !matches!(status, Some(MigrationStatus::Applied)) {
            return MigrationExecutionResult {
                version,
                direction: "down".to_string(),
                success: false,
                duration_ms: 0,
                error: Some(format!(
                    "migration {} cannot be rolled back (status: {:?})",
                    version, status
                )),
            };
        }

        let result = executor_fn(&migration.down_sql);
        let duration_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(()) => {
                let _ = self.tracker.record_rolled_back(version, duration_ms);
                MigrationExecutionResult {
                    version,
                    direction: "down".to_string(),
                    success: true,
                    duration_ms,
                    error: None,
                }
            }
            Err(e) => {
                MigrationExecutionResult {
                    version,
                    direction: "down".to_string(),
                    success: false,
                    duration_ms,
                    error: Some(e),
                }
            }
        }
    }

    /// 返回所有已注册的迁移版本号（排序）
    pub fn registered_versions(&self) -> Vec<u64> {
        let mut versions: Vec<u64> = self
            .migrations
            .lock()
            .map(|m| m.keys().copied().collect())
            .unwrap_or_default();
        versions.sort();
        versions
    }

    /// 返回状态跟踪器的引用
    pub fn tracker(&self) -> &MigrationTracker {
        &self.tracker
    }

    /// 检测所有已注册迁移中的冲突
    pub fn detect_conflicts(&self) -> ConflictReport {
        let migrations: Vec<Migration> = self
            .migrations
            .lock()
            .map(|m| m.values().cloned().collect())
            .unwrap_or_default();
        ConflictDetector::detect(&migrations)
    }
}

impl Default for MigrationExecutor {
    fn default() -> Self {
        Self::new()
    }
}

// ====================================================================
// ETL 数据管道
// ====================================================================

/// ETL 管道阶段
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EtlStage {
    /// 提取阶段：从源读取数据
    Extract,
    /// 转换阶段：对数据进行清洗/转换
    Transform,
    /// 加载阶段：写入目标
    Load,
}

/// ETL 管道执行统计
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EtlStats {
    /// 提取的记录数
    pub extracted: u64,
    /// 转换后的记录数（可能与 extracted 不同，如过滤后）
    pub transformed: u64,
    /// 成功加载的记录数
    pub loaded: u64,
    /// 加载失败的记录数
    pub failed: u64,
    /// 总耗时（毫秒）
    pub total_duration_ms: u64,
    /// 各阶段耗时（毫秒）
    pub stage_durations_ms: HashMap<EtlStage, u64>,
}

impl EtlStats {
    pub fn new() -> Self {
        Self::default()
    }

    /// 返回成功率（0.0 - 100.0）
    pub fn success_rate(&self) -> f64 {
        let total = self.loaded + self.failed;
        if total == 0 {
            return 0.0;
        }
        (self.loaded as f64 / total as f64) * 100.0
    }

    /// 返回提取到加载的记录保留率（0.0 - 100.0）
    pub fn retention_rate(&self) -> f64 {
        if self.extracted == 0 {
            return 0.0;
        }
        (self.loaded as f64 / self.extracted as f64) * 100.0
    }
}

/// ETL 管道配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EtlConfig {
    /// 批处理大小
    pub batch_size: usize,
    /// 是否在转换阶段跳过错误记录
    pub skip_errors: bool,
    /// 是否只进行试运行（不实际写入）
    pub dry_run: bool,
}

impl Default for EtlConfig {
    fn default() -> Self {
        Self {
            batch_size: 1000,
            skip_errors: false,
            dry_run: false,
        }
    }
}

impl EtlConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_batch_size(mut self, size: usize) -> Self {
        self.batch_size = size;
        self
    }

    pub fn with_skip_errors(mut self, skip: bool) -> Self {
        self.skip_errors = skip;
        self
    }

    pub fn with_dry_run(mut self, dry: bool) -> Self {
        self.dry_run = dry;
        self
    }
}

/// 获取当前时间戳（毫秒）
fn current_timestamp_millis() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    // ====================================================================
    // Migration 测试
    // ====================================================================

    #[test]
    fn test_migration_new() {
        let mig = Migration::new(1, "create_users", "CREATE TABLE users (...)", "DROP TABLE users");
        assert_eq!(mig.version, 1);
        assert_eq!(mig.name, "create_users");
        assert!(mig.is_reversible());
        assert!(mig.affected_tables.is_empty());
    }

    #[test]
    fn test_migration_with_tables() {
        let mig = Migration::new(1, "mig", "UP", "DOWN")
            .with_tables(vec!["users".to_string(), "orders".to_string()]);
        assert_eq!(mig.affected_tables.len(), 2);
    }

    #[test]
    fn test_migration_not_reversible() {
        let mig = Migration::new(1, "mig", "UP", "");
        assert!(!mig.is_reversible());
    }

    #[test]
    fn test_migration_reversible_with_whitespace() {
        let mig = Migration::new(1, "mig", "UP", "   ");
        assert!(!mig.is_reversible());
    }

    // ====================================================================
    // MigrationStatus 测试
    // ====================================================================

    #[test]
    fn test_migration_status_is_applied() {
        assert!(MigrationStatus::Applied.is_applied());
        assert!(!MigrationStatus::Pending.is_applied());
        assert!(!MigrationStatus::Failed.is_applied());
        assert!(!MigrationStatus::RolledBack.is_applied());
    }

    #[test]
    fn test_migration_status_can_rollback() {
        assert!(MigrationStatus::Applied.can_rollback());
        assert!(!MigrationStatus::Pending.can_rollback());
        assert!(!MigrationStatus::Failed.can_rollback());
        assert!(!MigrationStatus::RolledBack.can_rollback());
    }

    // ====================================================================
    // MigrationRecord 测试
    // ====================================================================

    #[test]
    fn test_migration_record_pending() {
        let rec = MigrationRecord::pending(1, "test");
        assert_eq!(rec.version, 1);
        assert_eq!(rec.status, MigrationStatus::Pending);
        assert!(rec.applied_at.is_none());
    }

    #[test]
    fn test_migration_record_mark_applied() {
        let mut rec = MigrationRecord::pending(1, "test");
        rec.mark_applied(100);
        assert_eq!(rec.status, MigrationStatus::Applied);
        assert!(rec.applied_at.is_some());
        assert_eq!(rec.duration_ms, 100);
        assert!(rec.error.is_none());
    }

    #[test]
    fn test_migration_record_mark_rolled_back() {
        let mut rec = MigrationRecord::pending(1, "test");
        rec.mark_applied(100);
        rec.mark_rolled_back(50);
        assert_eq!(rec.status, MigrationStatus::RolledBack);
        assert!(rec.rolled_back_at.is_some());
    }

    #[test]
    fn test_migration_record_mark_failed() {
        let mut rec = MigrationRecord::pending(1, "test");
        rec.mark_failed("connection error", 200);
        assert_eq!(rec.status, MigrationStatus::Failed);
        assert_eq!(rec.error, Some("connection error".to_string()));
    }

    // ====================================================================
    // MigrationTracker 测试
    // ====================================================================

    #[test]
    fn test_tracker_register() {
        let tracker = MigrationTracker::new();
        let mig = Migration::new(1, "test", "UP", "DOWN");
        assert!(tracker.register(&mig).is_ok());
        assert_eq!(tracker.count(), 1);
    }

    #[test]
    fn test_tracker_register_duplicate_fails() {
        let tracker = MigrationTracker::new();
        let mig = Migration::new(1, "test", "UP", "DOWN");
        tracker.register(&mig).unwrap();
        assert!(tracker.register(&mig).is_err());
    }

    #[test]
    fn test_tracker_record_applied() {
        let tracker = MigrationTracker::new();
        let mig = Migration::new(1, "test", "UP", "DOWN");
        tracker.register(&mig).unwrap();
        assert!(tracker.record_applied(1, 100).is_ok());
        assert_eq!(tracker.get_status(1), Some(MigrationStatus::Applied));
    }

    #[test]
    fn test_tracker_record_applied_not_found() {
        let tracker = MigrationTracker::new();
        assert!(tracker.record_applied(999, 100).is_err());
    }

    #[test]
    fn test_tracker_record_rolled_back() {
        let tracker = MigrationTracker::new();
        let mig = Migration::new(1, "test", "UP", "DOWN");
        tracker.register(&mig).unwrap();
        tracker.record_applied(1, 100).unwrap();
        assert!(tracker.record_rolled_back(1, 50).is_ok());
        assert_eq!(tracker.get_status(1), Some(MigrationStatus::RolledBack));
    }

    #[test]
    fn test_tracker_rollback_pending_fails() {
        let tracker = MigrationTracker::new();
        let mig = Migration::new(1, "test", "UP", "DOWN");
        tracker.register(&mig).unwrap();
        // 未应用就回滚应失败
        assert!(tracker.record_rolled_back(1, 50).is_err());
    }

    #[test]
    fn test_tracker_record_failed() {
        let tracker = MigrationTracker::new();
        let mig = Migration::new(1, "test", "UP", "DOWN");
        tracker.register(&mig).unwrap();
        assert!(tracker.record_failed(1, "error", 100).is_ok());
        assert_eq!(tracker.get_status(1), Some(MigrationStatus::Failed));
    }

    #[test]
    fn test_tracker_applied_versions() {
        let tracker = MigrationTracker::new();
        tracker.register(&Migration::new(1, "a", "UP", "DOWN")).unwrap();
        tracker.register(&Migration::new(2, "b", "UP", "DOWN")).unwrap();
        tracker.register(&Migration::new(3, "c", "UP", "DOWN")).unwrap();
        tracker.record_applied(1, 10).unwrap();
        tracker.record_applied(3, 10).unwrap();
        let applied = tracker.applied_versions();
        assert_eq!(applied, vec![1, 3]);
    }

    #[test]
    fn test_tracker_pending_versions() {
        let tracker = MigrationTracker::new();
        tracker.register(&Migration::new(1, "a", "UP", "DOWN")).unwrap();
        tracker.register(&Migration::new(2, "b", "UP", "DOWN")).unwrap();
        tracker.register(&Migration::new(3, "c", "UP", "DOWN")).unwrap();
        tracker.record_applied(2, 10).unwrap();
        let pending = tracker.pending_versions();
        assert_eq!(pending, vec![1, 3]);
    }

    #[test]
    fn test_tracker_last_applied_version() {
        let tracker = MigrationTracker::new();
        assert_eq!(tracker.last_applied_version(), None);
        tracker.register(&Migration::new(1, "a", "UP", "DOWN")).unwrap();
        tracker.register(&Migration::new(2, "b", "UP", "DOWN")).unwrap();
        tracker.record_applied(1, 10).unwrap();
        assert_eq!(tracker.last_applied_version(), Some(1));
        tracker.record_applied(2, 10).unwrap();
        assert_eq!(tracker.last_applied_version(), Some(2));
    }

    #[test]
    fn test_tracker_all_records_sorted() {
        let tracker = MigrationTracker::new();
        tracker.register(&Migration::new(3, "c", "UP", "DOWN")).unwrap();
        tracker.register(&Migration::new(1, "a", "UP", "DOWN")).unwrap();
        tracker.register(&Migration::new(2, "b", "UP", "DOWN")).unwrap();
        let records = tracker.all_records();
        assert_eq!(records.len(), 3);
        assert_eq!(records[0].version, 1);
        assert_eq!(records[1].version, 2);
        assert_eq!(records[2].version, 3);
    }

    #[test]
    fn test_tracker_get_record() {
        let tracker = MigrationTracker::new();
        tracker.register(&Migration::new(1, "test", "UP", "DOWN")).unwrap();
        let rec = tracker.get_record(1).unwrap();
        assert_eq!(rec.name, "test");
        assert!(tracker.get_record(999).is_none());
    }

    // ====================================================================
    // ConflictDetector 测试
    // ====================================================================

    #[test]
    fn test_conflict_no_conflicts() {
        let migrations = vec![
            Migration::new(1, "a", "UP", "DOWN").with_tables(vec!["users".to_string()]),
            Migration::new(2, "b", "UP", "DOWN").with_tables(vec!["orders".to_string()]),
        ];
        let report = ConflictDetector::detect(&migrations);
        assert!(!report.has_conflicts());
    }

    #[test]
    fn test_conflict_duplicate_version() {
        let migrations = vec![
            Migration::new(1, "a", "UP", "DOWN"),
            Migration::new(1, "b", "UP", "DOWN"),
        ];
        let report = ConflictDetector::detect(&migrations);
        assert!(report.has_conflicts());
        assert!(report
            .conflicts
            .iter()
            .any(|c| matches!(c, ConflictType::DuplicateVersion(1))));
    }

    #[test]
    fn test_conflict_table_conflict() {
        let migrations = vec![
            Migration::new(1, "a", "UP", "DOWN").with_tables(vec!["users".to_string()]),
            Migration::new(2, "b", "UP", "DOWN").with_tables(vec!["users".to_string()]),
        ];
        let report = ConflictDetector::detect(&migrations);
        assert!(report.has_conflicts());
        assert!(report.conflicts.iter().any(|c| matches!(
            c,
            ConflictType::TableConflict { table, .. } if table == "users"
        )));
    }

    #[test]
    fn test_conflict_missing_dependency() {
        let migrations = vec![Migration::new(1, "a", "UP", "DOWN")];
        let deps = vec![(1u64, 99u64)]; // 1 依赖 99，但 99 不存在
        let report = ConflictDetector::detect_dependencies(&migrations, &deps);
        assert!(report.has_conflicts());
        assert!(report.conflicts.iter().any(|c| matches!(
            c,
            ConflictType::MissingDependency { version: 1, dependency: 99 }
        )));
    }

    #[test]
    fn test_conflict_dependency_satisfied() {
        let migrations = vec![
            Migration::new(1, "a", "UP", "DOWN"),
            Migration::new(2, "b", "UP", "DOWN"),
        ];
        let deps = vec![(2u64, 1u64)]; // 2 依赖 1，1 存在
        let report = ConflictDetector::detect_dependencies(&migrations, &deps);
        assert!(!report.has_conflicts());
    }

    #[test]
    fn test_conflict_report_conflict_count() {
        let report = ConflictReport { conflicts: vec![] };
        assert_eq!(report.conflict_count(), 0);
        let report = ConflictReport {
            conflicts: vec![ConflictType::DuplicateVersion(1)],
        };
        assert_eq!(report.conflict_count(), 1);
    }

    // ====================================================================
    // MigrationExecutor 测试
    // ====================================================================

    #[test]
    fn test_executor_add_migration() {
        let executor = MigrationExecutor::new();
        let mig = Migration::new(1, "test", "CREATE TABLE t (...)", "DROP TABLE t");
        assert!(executor.add_migration(mig).is_ok());
        assert_eq!(executor.registered_versions(), vec![1]);
    }

    #[test]
    fn test_executor_add_duplicate_fails() {
        let executor = MigrationExecutor::new();
        let mig = Migration::new(1, "test", "UP", "DOWN");
        executor.add_migration(mig).unwrap();
        let mig2 = Migration::new(1, "test2", "UP", "DOWN");
        assert!(executor.add_migration(mig2).is_err());
    }

    #[test]
    fn test_executor_execute_up_success() {
        let executor = MigrationExecutor::new();
        let mig = Migration::new(1, "test", "CREATE TABLE users (...)", "DROP TABLE users");
        executor.add_migration(mig).unwrap();

        let result = executor.execute_up(1, |_sql| Ok(()));
        assert!(result.success);
        assert_eq!(result.direction, "up");
        assert_eq!(executor.tracker().get_status(1), Some(MigrationStatus::Applied));
    }

    #[test]
    fn test_executor_execute_up_failure() {
        let executor = MigrationExecutor::new();
        let mig = Migration::new(1, "test", "CREATE TABLE users (...)", "DROP TABLE users");
        executor.add_migration(mig).unwrap();

        let result = executor.execute_up(1, |_| Err("syntax error".to_string()));
        assert!(!result.success);
        assert_eq!(executor.tracker().get_status(1), Some(MigrationStatus::Failed));
    }

    #[test]
    fn test_executor_execute_up_not_found() {
        let executor = MigrationExecutor::new();
        let result = executor.execute_up(999, |_| Ok(()));
        assert!(!result.success);
    }

    #[test]
    fn test_executor_execute_down_success() {
        let executor = MigrationExecutor::new();
        let mig = Migration::new(1, "test", "CREATE TABLE users (...)", "DROP TABLE users");
        executor.add_migration(mig).unwrap();
        executor.execute_up(1, |_| Ok(()));

        let result = executor.execute_down(1, |_| Ok(()));
        assert!(result.success);
        assert_eq!(result.direction, "down");
        assert_eq!(executor.tracker().get_status(1), Some(MigrationStatus::RolledBack));
    }

    #[test]
    fn test_executor_execute_down_not_reversible() {
        let executor = MigrationExecutor::new();
        let mig = Migration::new(1, "test", "UP", ""); // 不可回滚
        executor.add_migration(mig).unwrap();
        executor.execute_up(1, |_| Ok(()));

        let result = executor.execute_down(1, |_| Ok(()));
        assert!(!result.success);
    }

    #[test]
    fn test_executor_execute_down_not_applied() {
        let executor = MigrationExecutor::new();
        let mig = Migration::new(1, "test", "UP", "DOWN");
        executor.add_migration(mig).unwrap();
        // 未执行 up 就直接 down
        let result = executor.execute_down(1, |_| Ok(()));
        assert!(!result.success);
    }

    #[test]
    fn test_executor_detect_conflicts() {
        let executor = MigrationExecutor::new();
        executor
            .add_migration(Migration::new(1, "a", "UP", "DOWN").with_tables(vec!["t".to_string()]))
            .unwrap();
        executor
            .add_migration(Migration::new(2, "b", "UP", "DOWN").with_tables(vec!["t".to_string()]))
            .unwrap();
        let report = executor.detect_conflicts();
        assert!(report.has_conflicts());
    }

    #[test]
    fn test_executor_full_up_down_cycle() {
        let executor = MigrationExecutor::new();
        let mig = Migration::new(1, "create_users", "CREATE TABLE users (...)", "DROP TABLE users");
        executor.add_migration(mig).unwrap();

        // 执行 up
        let up_result = executor.execute_up(1, |_| Ok(()));
        assert!(up_result.success);
        assert_eq!(executor.tracker().last_applied_version(), Some(1));

        // 执行 down
        let down_result = executor.execute_down(1, |_| Ok(()));
        assert!(down_result.success);
        assert_eq!(executor.tracker().last_applied_version(), None);
    }

    // ====================================================================
    // ETL 测试
    // ====================================================================

    #[test]
    fn test_etl_config_default() {
        let config = EtlConfig::default();
        assert_eq!(config.batch_size, 1000);
        assert!(!config.skip_errors);
        assert!(!config.dry_run);
    }

    #[test]
    fn test_etl_config_builder() {
        let config = EtlConfig::new()
            .with_batch_size(500)
            .with_skip_errors(true)
            .with_dry_run(true);
        assert_eq!(config.batch_size, 500);
        assert!(config.skip_errors);
        assert!(config.dry_run);
    }

    #[test]
    fn test_etl_stats_success_rate() {
        let mut stats = EtlStats::new();
        stats.loaded = 80;
        stats.failed = 20;
        assert_eq!(stats.success_rate(), 80.0);
    }

    #[test]
    fn test_etl_stats_success_rate_zero() {
        let stats = EtlStats::new();
        assert_eq!(stats.success_rate(), 0.0);
    }

    #[test]
    fn test_etl_stats_retention_rate() {
        let mut stats = EtlStats::new();
        stats.extracted = 100;
        stats.loaded = 90;
        assert_eq!(stats.retention_rate(), 90.0);
    }

    #[test]
    fn test_etl_stats_retention_rate_zero() {
        let stats = EtlStats::new();
        assert_eq!(stats.retention_rate(), 0.0);
    }

    #[test]
    fn test_etl_stage_equality() {
        assert_eq!(EtlStage::Extract, EtlStage::Extract);
        assert_ne!(EtlStage::Extract, EtlStage::Transform);
        assert_ne!(EtlStage::Transform, EtlStage::Load);
    }
}
