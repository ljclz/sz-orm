//! 模式版本管理
//!
//! 提供 [`SchemaVersioning`] 用于管理模式版本历史、生成版本迁移、
//! 检测版本冲突等。

use std::collections::HashMap;
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

/// 模式版本
#[derive(Debug, Clone)]
pub struct SchemaVersion {
    /// 版本号
    pub version: u64,
    /// 版本名
    pub name: String,
    /// 描述
    pub description: String,
    /// 创建时间戳（Unix 秒）
    pub created_at: u64,
    /// 校验和（用于检测漂移）
    pub checksum: String,
    /// 是否已应用
    pub applied: bool,
    /// 应用时间戳
    pub applied_at: Option<u64>,
}

impl SchemaVersion {
    /// 创建新版本
    #[must_use]
    pub fn new(version: u64, name: &str) -> Self {
        Self {
            version,
            name: name.to_string(),
            description: String::new(),
            created_at: current_timestamp(),
            checksum: String::new(),
            applied: false,
            applied_at: None,
        }
    }

    /// 设置描述
    #[must_use]
    pub fn with_description(mut self, desc: &str) -> Self {
        self.description = desc.to_string();
        self
    }

    /// 设置校验和
    #[must_use]
    pub fn with_checksum(mut self, checksum: &str) -> Self {
        self.checksum = checksum.to_string();
        self
    }

    /// 标记为已应用
    pub fn mark_applied(&mut self) {
        self.applied = true;
        self.applied_at = Some(current_timestamp());
    }

    /// 是否已应用
    #[must_use]
    pub fn is_applied(&self) -> bool {
        self.applied
    }
}

/// 获取当前时间戳
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 版本状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionStatus {
    /// 待应用
    Pending,
    /// 已应用
    Applied,
    /// 已回滚
    RolledBack,
    /// 冲突
    Conflict,
}

impl VersionStatus {
    /// 返回描述
    #[must_use]
    pub fn description(&self) -> &'static str {
        match self {
            VersionStatus::Pending => "pending",
            VersionStatus::Applied => "applied",
            VersionStatus::RolledBack => "rolled back",
            VersionStatus::Conflict => "conflict",
        }
    }
}

/// 模式版本管理器
#[derive(Debug, Default)]
pub struct SchemaVersioning {
    /// 版本历史
    versions: Vec<SchemaVersion>,
    /// 当前版本号
    current_version: u64,
}

impl SchemaVersioning {
    /// 创建新的版本管理器
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 添加版本
    pub fn add_version(&mut self, version: SchemaVersion) {
        self.versions.push(version);
    }

    /// 注册新版本（便捷方法）
    pub fn register(&mut self, version: u64, name: &str) {
        self.add_version(SchemaVersion::new(version, name));
    }

    /// 获取所有版本
    #[must_use]
    pub fn versions(&self) -> &[SchemaVersion] {
        &self.versions
    }

    /// 获取最新版本
    #[must_use]
    pub fn latest_version(&self) -> Option<&SchemaVersion> {
        self.versions.last()
    }

    /// 获取已应用的版本
    #[must_use]
    pub fn applied_versions(&self) -> Vec<&SchemaVersion> {
        self.versions.iter().filter(|v| v.applied).collect()
    }

    /// 获取待应用的版本
    #[must_use]
    pub fn pending_versions(&self) -> Vec<&SchemaVersion> {
        self.versions.iter().filter(|v| !v.applied).collect()
    }

    /// 标记版本为已应用
    ///
    /// # Errors
    ///
    /// 若版本不存在返回 `Err`。
    pub fn mark_applied(&mut self, version: u64) -> Result<(), String> {
        let v = self
            .versions
            .iter_mut()
            .find(|v| v.version == version)
            .ok_or_else(|| format!("version {version} not found"))?;
        v.mark_applied();
        self.current_version = version;
        Ok(())
    }

    /// 回滚到指定版本
    ///
    /// # Errors
    ///
    /// 若版本不存在返回 `Err`。
    pub fn rollback_to(&mut self, version: u64) -> Result<(), String> {
        if !self.versions.iter().any(|v| v.version == version) {
            return Err(format!("version {version} not found"));
        }
        for v in &mut self.versions {
            if v.version > version {
                v.applied = false;
                v.applied_at = None;
            }
        }
        self.current_version = version;
        Ok(())
    }

    /// 获取当前版本号
    #[must_use]
    pub fn current_version(&self) -> u64 {
        self.current_version
    }

    /// 检测校验和冲突
    #[must_use]
    pub fn detect_conflicts(&self) -> Vec<(u64, String, String)> {
        let mut seen = HashMap::new();
        let mut conflicts = Vec::new();
        for v in &self.versions {
            if !v.checksum.is_empty() {
                if let Some(existing) = seen.insert(v.version, v.checksum.clone()) {
                    if existing != v.checksum {
                        conflicts.push((v.version, existing, v.checksum.clone()));
                    }
                }
            }
        }
        conflicts
    }

    /// 版本数量
    #[must_use]
    pub fn count(&self) -> usize {
        self.versions.len()
    }

    /// 已应用版本数
    #[must_use]
    pub fn applied_count(&self) -> usize {
        self.versions.iter().filter(|v| v.applied).count()
    }

    /// 待应用版本数
    #[must_use]
    pub fn pending_count(&self) -> usize {
        self.versions.iter().filter(|v| !v.applied).count()
    }

    /// 生成版本历史报告
    #[must_use]
    pub fn history_report(&self) -> String {
        let mut report = String::from("Schema Version History:\n");
        for v in &self.versions {
            let status = if v.applied { "APPLIED" } else { "PENDING" };
            report.push_str(&format!(
                "  V{} {} [{}] - {}\n",
                v.version, v.name, status, v.description
            ));
        }
        report
    }

    /// 查找版本
    #[must_use]
    pub fn find_version(&self, version: u64) -> Option<&SchemaVersion> {
        self.versions.iter().find(|v| v.version == version)
    }

    /// 获取版本范围
    #[must_use]
    pub fn versions_in_range(&self, from: u64, to: u64) -> Vec<&SchemaVersion> {
        self.versions
            .iter()
            .filter(|v| (from..=to).contains(&v.version))
            .collect()
    }
}

impl fmt::Display for SchemaVersioning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SchemaVersioning(total={}, applied={}, pending={})",
            self.count(),
            self.applied_count(),
            self.pending_count()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_version_new() {
        let v = SchemaVersion::new(1, "init");
        assert_eq!(v.version, 1);
        assert_eq!(v.name, "init");
        assert!(!v.applied);
    }

    #[test]
    fn test_schema_version_with_description() {
        let v = SchemaVersion::new(1, "init").with_description("initial schema");
        assert_eq!(v.description, "initial schema");
    }

    #[test]
    fn test_schema_version_mark_applied() {
        let mut v = SchemaVersion::new(1, "init");
        v.mark_applied();
        assert!(v.is_applied());
        assert!(v.applied_at.is_some());
    }

    #[test]
    fn test_version_status_description() {
        assert_eq!(VersionStatus::Pending.description(), "pending");
        assert_eq!(VersionStatus::Applied.description(), "applied");
    }

    #[test]
    fn test_schema_versioning_new() {
        let sv = SchemaVersioning::new();
        assert_eq!(sv.count(), 0);
    }

    #[test]
    fn test_schema_versioning_add() {
        let mut sv = SchemaVersioning::new();
        sv.add_version(SchemaVersion::new(1, "init"));
        assert_eq!(sv.count(), 1);
    }

    #[test]
    fn test_schema_versioning_mark_applied() {
        let mut sv = SchemaVersioning::new();
        sv.add_version(SchemaVersion::new(1, "init"));
        sv.mark_applied(1).unwrap();
        assert_eq!(sv.applied_count(), 1);
        assert_eq!(sv.current_version(), 1);
    }

    #[test]
    fn test_schema_versioning_mark_applied_not_found() {
        let mut sv = SchemaVersioning::new();
        assert!(sv.mark_applied(99).is_err());
    }

    #[test]
    fn test_schema_versioning_rollback() {
        let mut sv = SchemaVersioning::new();
        sv.add_version(SchemaVersion::new(1, "a"));
        sv.add_version(SchemaVersion::new(2, "b"));
        sv.add_version(SchemaVersion::new(3, "c"));
        sv.mark_applied(1).unwrap();
        sv.mark_applied(2).unwrap();
        sv.mark_applied(3).unwrap();
        sv.rollback_to(1).unwrap();
        assert_eq!(sv.current_version(), 1);
        assert_eq!(sv.applied_count(), 1);
    }

    #[test]
    fn test_schema_versioning_pending() {
        let mut sv = SchemaVersioning::new();
        sv.add_version(SchemaVersion::new(1, "a"));
        sv.add_version(SchemaVersion::new(2, "b"));
        sv.mark_applied(1).unwrap();
        assert_eq!(sv.pending_count(), 1);
        assert_eq!(sv.applied_count(), 1);
    }

    #[test]
    fn test_schema_versioning_latest() {
        let mut sv = SchemaVersioning::new();
        sv.add_version(SchemaVersion::new(1, "a"));
        sv.add_version(SchemaVersion::new(2, "b"));
        assert_eq!(sv.latest_version().unwrap().version, 2);
    }

    #[test]
    fn test_schema_versioning_find() {
        let mut sv = SchemaVersioning::new();
        sv.add_version(SchemaVersion::new(1, "a"));
        assert!(sv.find_version(1).is_some());
        assert!(sv.find_version(2).is_none());
    }

    #[test]
    fn test_schema_versioning_versions_in_range() {
        let mut sv = SchemaVersioning::new();
        sv.add_version(SchemaVersion::new(1, "a"));
        sv.add_version(SchemaVersion::new(2, "b"));
        sv.add_version(SchemaVersion::new(3, "c"));
        let range = sv.versions_in_range(1, 2);
        assert_eq!(range.len(), 2);
    }

    #[test]
    fn test_schema_versioning_history_report() {
        let mut sv = SchemaVersioning::new();
        sv.add_version(SchemaVersion::new(1, "init").with_description("initial"));
        let report = sv.history_report();
        assert!(report.contains("V1 init"));
        assert!(report.contains("PENDING"));
    }

    #[test]
    fn test_schema_versioning_display() {
        let sv = SchemaVersioning::new();
        let s = format!("{}", sv);
        assert!(s.contains("SchemaVersioning"));
    }

    #[test]
    fn test_schema_versioning_detect_conflicts() {
        let mut sv = SchemaVersioning::new();
        sv.add_version(SchemaVersion::new(1, "a").with_checksum("abc"));
        sv.add_version(SchemaVersion::new(1, "a").with_checksum("xyz"));
        let conflicts = sv.detect_conflicts();
        assert_eq!(conflicts.len(), 1);
    }

    #[test]
    fn test_schema_versioning_no_conflicts() {
        let mut sv = SchemaVersioning::new();
        sv.add_version(SchemaVersion::new(1, "a").with_checksum("abc"));
        sv.add_version(SchemaVersion::new(2, "b").with_checksum("xyz"));
        let conflicts = sv.detect_conflicts();
        assert!(conflicts.is_empty());
    }
}
