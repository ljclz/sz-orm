//! CDC 快照模式 — 全表快照标记
//!
//! 在启动 CDC 增量同步前，可选执行全表快照以同步存量数据。

use std::collections::HashSet;

/// 快照模式
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotMode {
    /// 不执行快照，仅增量同步
    IncrementalOnly,
    /// 先快照后增量
    SnapshotThenIncremental,
    /// 仅快照
    SnapshotOnly,
}

/// 快照状态跟踪器
#[derive(Debug)]
pub struct SnapshotTracker {
    mode: SnapshotMode,
    completed_tables: HashSet<String>,
    total_tables: HashSet<String>,
}

impl SnapshotTracker {
    /// 创建快照跟踪器
    pub fn new(mode: SnapshotMode) -> Self {
        Self {
            mode,
            completed_tables: HashSet::new(),
            total_tables: HashSet::new(),
        }
    }

    /// 注册需要快照的表
    pub fn register_table(&mut self, table: &str) {
        self.total_tables.insert(table.to_string());
    }

    /// 标记表快照完成
    pub fn mark_completed(&mut self, table: &str) {
        self.completed_tables.insert(table.to_string());
    }

    /// 检查快照是否全部完成
    pub fn is_snapshot_complete(&self) -> bool {
        self.total_tables.is_empty() || self.total_tables == self.completed_tables
    }

    /// 检查表是否需要快照
    pub fn needs_snapshot(&self, table: &str) -> bool {
        match self.mode {
            SnapshotMode::IncrementalOnly => false,
            _ => !self.completed_tables.contains(table),
        }
    }

    /// 获取快照模式
    pub fn mode(&self) -> &SnapshotMode {
        &self.mode
    }

    /// 已完成表数
    pub fn completed_count(&self) -> usize {
        self.completed_tables.len()
    }

    /// 总表数
    pub fn total_count(&self) -> usize {
        self.total_tables.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snapshot_mode_incremental_only() {
        let tracker = SnapshotTracker::new(SnapshotMode::IncrementalOnly);
        assert!(!tracker.needs_snapshot("users"));
    }

    #[test]
    fn test_snapshot_completion_tracking() {
        let mut tracker = SnapshotTracker::new(SnapshotMode::SnapshotThenIncremental);
        tracker.register_table("users");
        tracker.register_table("posts");
        assert!(!tracker.is_snapshot_complete());
        tracker.mark_completed("users");
        assert!(!tracker.is_snapshot_complete());
        tracker.mark_completed("posts");
        assert!(tracker.is_snapshot_complete());
    }

    #[test]
    fn test_snapshot_only_mode() {
        let mut tracker = SnapshotTracker::new(SnapshotMode::SnapshotOnly);
        tracker.register_table("users");
        assert!(tracker.needs_snapshot("users"));
        tracker.mark_completed("users");
        assert!(!tracker.needs_snapshot("users"));
    }
}
