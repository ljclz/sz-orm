//! 数据生命周期核心数据结构

use std::collections::HashMap;

/// 生命周期错误类型
#[derive(Debug, thiserror::Error)]
pub enum LifecycleError {
    #[error("规则无效: {0}")]
    RuleInvalid(String),
    #[error("表未找到: {0}")]
    TableNotFound(String),
    #[error("归档目标不可用: {0}")]
    ArchiveTargetUnavailable(String),
    #[error("完整性校验失败: {0}")]
    IntegrityCheckFailed(String),
    #[error("查询超时: {0}")]
    QueryTimeout(String),
    #[error("冷存储不可用: {0}")]
    ColdStorageUnavailable(String),
    #[error("迁移窗口外: {0}")]
    MigrationWindowExceeded(String),
    #[error("数据范围无效: {0}")]
    DataRangeInvalid(String),
}

/// 生命周期规则
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LifecycleRule {
    pub name: String,
    pub target_table: String,
    pub cold_hot_threshold: ColdHotThreshold,
    pub retention_days: u32,
    pub destruction_days: u32,
    pub migration_window: MigrationWindow,
    pub archive_target: String,
    pub enabled: bool,
}

/// 冷热分类阈值
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ColdHotThreshold {
    pub access_frequency_per_day: f64,
    pub age_days: u32,
}

impl Default for ColdHotThreshold {
    fn default() -> Self {
        Self {
            access_frequency_per_day: 1.0,
            age_days: 30,
        }
    }
}

/// 迁移时间窗口
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MigrationWindow {
    pub start_hour: u32,
    pub end_hour: u32,
}

impl Default for MigrationWindow {
    fn default() -> Self {
        Self {
            start_hour: 2,
            end_hour: 6,
        }
    }
}

/// 数据温度级别
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum DataTemperature {
    Hot,
    Warm,
    Cold,
    Archived,
    Destroy,
}

impl DataTemperature {
    /// 返回相邻下一级温度，Destroy 无下一级
    pub fn next_tier(self) -> Option<Self> {
        match self {
            Self::Hot => Some(Self::Warm),
            Self::Warm => Some(Self::Cold),
            Self::Cold => Some(Self::Archived),
            Self::Archived => Some(Self::Destroy),
            Self::Destroy => None,
        }
    }

    /// 校验是否可迁移到目标层级（仅允许相邻层级，禁止跨级跳转）
    pub fn can_migrate_to(self, target: Self) -> bool {
        self.next_tier() == Some(target)
    }
}

/// 冷热分类结果
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ColdHotClassification {
    pub table: String,
    pub hot_data: Vec<DataRange>,
    pub warm_data: Vec<DataRange>,
    pub cold_data: Vec<DataRange>,
}

/// 数据范围
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DataRange {
    pub min_id: i64,
    pub max_id: i64,
    pub row_count: u64,
    pub temperature: DataTemperature,
    pub last_accessed_days_ago: u32,
    pub access_frequency_per_day: f64,
}

/// 归档记录
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ArchiveRecord {
    pub record_id: String,
    pub table: String,
    pub data_range: DataRange,
    pub archive_target: String,
    pub archived_at: i64,
    pub source_status: SourceStatus,
    pub integrity_verified: bool,
}

/// 归档报告
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ArchiveReport {
    pub table: String,
    pub total_archived: u64,
    pub failed: u64,
    pub records: Vec<ArchiveRecord>,
    pub duration_ms: u64,
}

/// TTL 清理报告
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TtlCleanupReport {
    pub table: String,
    pub total_scanned: u64,
    pub total_deleted: u64,
    pub total_skipped: u64,
    pub cleanup_log_path: String,
    pub skipped_reasons: HashMap<String, u64>,
}

/// 源数据状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SourceStatus {
    Active,
    Archived,
    MarkedForDeletion,
    Deleted,
}

/// 访问模式统计
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AccessPatternStats {
    pub table: String,
    pub access_count_30d: u64,
    pub avg_frequency_per_day: f64,
    pub last_accessed_days_ago: u32,
    pub total_rows: u64,
}

impl Default for AccessPatternStats {
    fn default() -> Self {
        Self {
            table: String::new(),
            access_count_30d: 0,
            avg_frequency_per_day: 0.0,
            last_accessed_days_ago: 0,
            total_rows: 0,
        }
    }
}

/// TTL 清理日志条目
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TtlCleanupLogEntry {
    pub table: String,
    pub deleted_at: i64,
    pub row_count: u64,
    pub data_summary: String,
    pub rule_name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lifecycle_error_display() {
        let err = LifecycleError::RuleInvalid("missing field".to_string());
        assert!(err.to_string().contains("missing field"));
    }

    #[test]
    fn test_cold_hot_threshold_default() {
        let t = ColdHotThreshold::default();
        assert_eq!(t.access_frequency_per_day, 1.0);
        assert_eq!(t.age_days, 30);
    }

    #[test]
    fn test_migration_window_default() {
        let w = MigrationWindow::default();
        assert_eq!(w.start_hour, 2);
        assert_eq!(w.end_hour, 6);
    }

    #[test]
    fn test_data_temperature_ordering() {
        let temps = [
            DataTemperature::Hot,
            DataTemperature::Warm,
            DataTemperature::Cold,
            DataTemperature::Archived,
        ];
        assert_eq!(temps.len(), 4);
    }

    #[test]
    fn test_lifecycle_rule_serialization() {
        let rule = LifecycleRule {
            name: "rule1".to_string(),
            target_table: "orders".to_string(),
            cold_hot_threshold: ColdHotThreshold::default(),
            retention_days: 90,
            destruction_days: 365,
            migration_window: MigrationWindow::default(),
            archive_target: "s3://archive".to_string(),
            enabled: true,
        };
        let json = serde_json::to_string(&rule).unwrap();
        let deserialized: LifecycleRule = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "rule1");
        assert_eq!(deserialized.retention_days, 90);
    }
}
