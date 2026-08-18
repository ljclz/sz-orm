//! 冲突检测与解决（Conflict Resolver）
//!
//! 在多数据源融合场景中，检测和解决数据冲突。
//! 支持多种冲突解决策略：最后写入胜、源优先级、合并、手动解决。

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// 冲突类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ConflictType {
    /// 同一记录在不同源有不同值
    ValueMismatch,
    /// 同一记录在一个源存在，另一个源不存在
    ExistenceMismatch,
    /// 版本号冲突
    VersionConflict,
    /// 时间戳冲突
    TimestampConflict,
}

impl ConflictType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ConflictType::ValueMismatch => "value_mismatch",
            ConflictType::ExistenceMismatch => "existence_mismatch",
            ConflictType::VersionConflict => "version_conflict",
            ConflictType::TimestampConflict => "timestamp_conflict",
        }
    }
}

/// 冲突解决策略
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ResolutionStrategy {
    /// 最后写入胜
    LastWriteWins,
    /// 源优先级（高优先级源胜）
    SourcePriority,
    /// 合并字段
    MergeFields,
    /// 保留冲突（需手动解决）
    KeepConflict,
    /// 主源胜
    PrimaryWins,
}

impl ResolutionStrategy {
    pub fn as_str(&self) -> &'static str {
        match self {
            ResolutionStrategy::LastWriteWins => "last_write_wins",
            ResolutionStrategy::SourcePriority => "source_priority",
            ResolutionStrategy::MergeFields => "merge_fields",
            ResolutionStrategy::KeepConflict => "keep_conflict",
            ResolutionStrategy::PrimaryWins => "primary_wins",
        }
    }
}

/// 数据记录版本
#[derive(Debug, Clone, serde::Serialize)]
pub struct DataVersion {
    pub source: String,
    pub value: serde_json::Value,
    pub version: u64,
    pub timestamp_ms: i64,
}

impl DataVersion {
    pub fn new(source: &str, value: serde_json::Value, version: u64) -> Self {
        Self {
            source: source.to_string(),
            value,
            version,
            timestamp_ms: now_ms(),
        }
    }

    pub fn with_timestamp(mut self, ts: i64) -> Self {
        self.timestamp_ms = ts;
        self
    }
}

/// 冲突
#[derive(Debug, Clone, serde::Serialize)]
pub struct Conflict {
    pub key: String,
    pub conflict_type: ConflictType,
    pub versions: Vec<DataVersion>,
    pub detected_at_ms: i64,
}

impl Conflict {
    pub fn new(key: &str, conflict_type: ConflictType, versions: Vec<DataVersion>) -> Self {
        Self {
            key: key.to_string(),
            conflict_type,
            versions,
            detected_at_ms: now_ms(),
        }
    }

    pub fn version_count(&self) -> usize {
        self.versions.len()
    }
}

/// 冲突解决结果
#[derive(Debug, Clone, serde::Serialize)]
pub struct Resolution {
    pub key: String,
    pub strategy: ResolutionStrategy,
    pub resolved_value: serde_json::Value,
    pub winning_source: String,
    pub conflict: Conflict,
}

/// 冲突解决器
pub struct ConflictResolver {
    strategy: ResolutionStrategy,
    source_priorities: RwLock<HashMap<String, u32>>,
    primary_source: String,
    resolved_count: AtomicU64,
    unresolved_count: AtomicU64,
}

impl ConflictResolver {
    pub fn new(strategy: ResolutionStrategy, primary_source: &str) -> Self {
        Self {
            strategy,
            source_priorities: RwLock::new(HashMap::new()),
            primary_source: primary_source.to_string(),
            resolved_count: AtomicU64::new(0),
            unresolved_count: AtomicU64::new(0),
        }
    }

    pub fn set_priority(&self, source: &str, priority: u32) {
        if let Ok(mut priorities) = self.source_priorities.write() {
            priorities.insert(source.to_string(), priority);
        }
    }

    pub fn strategy(&self) -> ResolutionStrategy {
        self.strategy
    }

    pub fn detect_conflict(&self, key: &str, versions: &[DataVersion]) -> Option<Conflict> {
        if versions.len() < 2 {
            return None;
        }
        let values: Vec<&serde_json::Value> = versions.iter().map(|v| &v.value).collect();
        let all_same = values.windows(2).all(|w| w[0] == w[1]);
        if all_same {
            return None;
        }
        let conflict_type = if versions.iter().any(|v| v.value.is_null()) {
            ConflictType::ExistenceMismatch
        } else {
            let max_version = versions.iter().map(|v| v.version).max().unwrap_or(0);
            let min_version = versions.iter().map(|v| v.version).min().unwrap_or(0);
            if max_version != min_version {
                ConflictType::VersionConflict
            } else {
                ConflictType::ValueMismatch
            }
        };
        Some(Conflict::new(key, conflict_type, versions.to_vec()))
    }

    pub fn resolve(&self, conflict: &Conflict) -> Resolution {
        match self.strategy {
            ResolutionStrategy::LastWriteWins => self.resolve_last_write_wins(conflict),
            ResolutionStrategy::SourcePriority => self.resolve_source_priority(conflict),
            ResolutionStrategy::MergeFields => self.resolve_merge_fields(conflict),
            ResolutionStrategy::KeepConflict => self.resolve_keep_conflict(conflict),
            ResolutionStrategy::PrimaryWins => self.resolve_primary_wins(conflict),
        }
    }

    fn resolve_last_write_wins(&self, conflict: &Conflict) -> Resolution {
        self.resolved_count.fetch_add(1, Ordering::Relaxed);
        let winner = conflict
            .versions
            .iter()
            .max_by_key(|v| v.timestamp_ms)
            .unwrap_or(&conflict.versions[0]);
        Resolution {
            key: conflict.key.clone(),
            strategy: ResolutionStrategy::LastWriteWins,
            resolved_value: winner.value.clone(),
            winning_source: winner.source.clone(),
            conflict: conflict.clone(),
        }
    }

    fn resolve_source_priority(&self, conflict: &Conflict) -> Resolution {
        self.resolved_count.fetch_add(1, Ordering::Relaxed);
        let priorities = self
            .source_priorities
            .read()
            .map(|p| p.clone())
            .unwrap_or_default();
        let winner = conflict
            .versions
            .iter()
            .max_by_key(|v| priorities.get(&v.source).copied().unwrap_or(0))
            .unwrap_or(&conflict.versions[0]);
        Resolution {
            key: conflict.key.clone(),
            strategy: ResolutionStrategy::SourcePriority,
            resolved_value: winner.value.clone(),
            winning_source: winner.source.clone(),
            conflict: conflict.clone(),
        }
    }

    fn resolve_merge_fields(&self, conflict: &Conflict) -> Resolution {
        self.resolved_count.fetch_add(1, Ordering::Relaxed);
        let mut merged = serde_json::Map::new();
        for version in &conflict.versions {
            if let Some(obj) = version.value.as_object() {
                for (k, v) in obj {
                    if !merged.contains_key(k) {
                        merged.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        let winner = conflict
            .versions
            .iter()
            .max_by_key(|v| v.timestamp_ms)
            .unwrap_or(&conflict.versions[0]);
        Resolution {
            key: conflict.key.clone(),
            strategy: ResolutionStrategy::MergeFields,
            resolved_value: serde_json::Value::Object(merged),
            winning_source: winner.source.clone(),
            conflict: conflict.clone(),
        }
    }

    fn resolve_keep_conflict(&self, conflict: &Conflict) -> Resolution {
        self.unresolved_count.fetch_add(1, Ordering::Relaxed);
        let winner = &conflict.versions[0];
        Resolution {
            key: conflict.key.clone(),
            strategy: ResolutionStrategy::KeepConflict,
            resolved_value: winner.value.clone(),
            winning_source: winner.source.clone(),
            conflict: conflict.clone(),
        }
    }

    fn resolve_primary_wins(&self, conflict: &Conflict) -> Resolution {
        self.resolved_count.fetch_add(1, Ordering::Relaxed);
        let winner = conflict
            .versions
            .iter()
            .find(|v| v.source == self.primary_source)
            .unwrap_or(&conflict.versions[0]);
        Resolution {
            key: conflict.key.clone(),
            strategy: ResolutionStrategy::PrimaryWins,
            resolved_value: winner.value.clone(),
            winning_source: winner.source.clone(),
            conflict: conflict.clone(),
        }
    }

    pub fn resolved_count(&self) -> u64 {
        self.resolved_count.load(Ordering::Relaxed)
    }

    pub fn unresolved_count(&self) -> u64 {
        self.unresolved_count.load(Ordering::Relaxed)
    }

    pub fn primary_source(&self) -> &str {
        &self.primary_source
    }
}

/// 冲突日志
///
/// 记录所有检测到的冲突和解决结果。
pub struct ConflictLog {
    entries: RwLock<Vec<LogEntry>>,
    max_entries: usize,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct LogEntry {
    pub conflict: Conflict,
    pub resolution: Option<Resolution>,
    pub logged_at_ms: i64,
}

impl ConflictLog {
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: RwLock::new(Vec::new()),
            max_entries,
        }
    }

    pub fn log_conflict(&self, conflict: Conflict) {
        if let Ok(mut entries) = self.entries.write() {
            entries.push(LogEntry {
                conflict,
                resolution: None,
                logged_at_ms: now_ms(),
            });
            if entries.len() > self.max_entries {
                entries.remove(0);
            }
        }
    }

    pub fn log_resolution(&self, resolution: Resolution) {
        if let Ok(mut entries) = self.entries.write() {
            entries.push(LogEntry {
                conflict: resolution.conflict.clone(),
                resolution: Some(resolution),
                logged_at_ms: now_ms(),
            });
            if entries.len() > self.max_entries {
                entries.remove(0);
            }
        }
    }

    pub fn entries(&self) -> Vec<LogEntry> {
        self.entries
            .read()
            .ok()
            .map(|e| e.clone())
            .unwrap_or_default()
    }

    pub fn entry_count(&self) -> usize {
        self.entries.read().map(|e| e.len()).unwrap_or(0)
    }

    pub fn unresolved_entries(&self) -> Vec<LogEntry> {
        self.entries
            .read()
            .ok()
            .map(|e| {
                e.iter()
                    .filter(|entry| entry.resolution.is_none())
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn clear(&self) {
        if let Ok(mut entries) = self.entries.write() {
            entries.clear();
        }
    }
}

impl Default for ConflictLog {
    fn default() -> Self {
        Self::new(1000)
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conflict_type_as_str() {
        assert_eq!(ConflictType::ValueMismatch.as_str(), "value_mismatch");
        assert_eq!(ConflictType::VersionConflict.as_str(), "version_conflict");
    }

    #[test]
    fn test_resolution_strategy_as_str() {
        assert_eq!(
            ResolutionStrategy::LastWriteWins.as_str(),
            "last_write_wins"
        );
        assert_eq!(ResolutionStrategy::PrimaryWins.as_str(), "primary_wins");
    }

    #[test]
    fn test_data_version_new() {
        let v = DataVersion::new("db1", serde_json::json!({"a": 1}), 1);
        assert_eq!(v.source, "db1");
        assert_eq!(v.version, 1);
    }

    #[test]
    fn test_conflict_new() {
        let versions = vec![
            DataVersion::new("db1", serde_json::json!(1), 1),
            DataVersion::new("db2", serde_json::json!(2), 1),
        ];
        let c = Conflict::new("key1", ConflictType::ValueMismatch, versions);
        assert_eq!(c.key, "key1");
        assert_eq!(c.version_count(), 2);
    }

    #[test]
    fn test_detect_conflict_no_conflict() {
        let resolver = ConflictResolver::new(ResolutionStrategy::LastWriteWins, "primary");
        let versions = vec![
            DataVersion::new("db1", serde_json::json!(1), 1),
            DataVersion::new("db2", serde_json::json!(1), 1),
        ];
        assert!(resolver.detect_conflict("k", &versions).is_none());
    }

    #[test]
    fn test_detect_conflict_value_mismatch() {
        let resolver = ConflictResolver::new(ResolutionStrategy::LastWriteWins, "primary");
        let versions = vec![
            DataVersion::new("db1", serde_json::json!(1), 1),
            DataVersion::new("db2", serde_json::json!(2), 1),
        ];
        let c = resolver.detect_conflict("k", &versions).unwrap();
        assert_eq!(c.conflict_type, ConflictType::ValueMismatch);
    }

    #[test]
    fn test_detect_conflict_version_conflict() {
        let resolver = ConflictResolver::new(ResolutionStrategy::LastWriteWins, "primary");
        let versions = vec![
            DataVersion::new("db1", serde_json::json!(1), 1),
            DataVersion::new("db2", serde_json::json!(2), 2),
        ];
        let c = resolver.detect_conflict("k", &versions).unwrap();
        assert_eq!(c.conflict_type, ConflictType::VersionConflict);
    }

    #[test]
    fn test_detect_conflict_single_version() {
        let resolver = ConflictResolver::new(ResolutionStrategy::LastWriteWins, "primary");
        let versions = vec![DataVersion::new("db1", serde_json::json!(1), 1)];
        assert!(resolver.detect_conflict("k", &versions).is_none());
    }

    #[test]
    fn test_resolve_last_write_wins() {
        let resolver = ConflictResolver::new(ResolutionStrategy::LastWriteWins, "primary");
        let versions = vec![
            DataVersion::new("db1", serde_json::json!(1), 1).with_timestamp(100),
            DataVersion::new("db2", serde_json::json!(2), 1).with_timestamp(200),
        ];
        let conflict = Conflict::new("k", ConflictType::ValueMismatch, versions);
        let resolution = resolver.resolve(&conflict);
        assert_eq!(resolution.winning_source, "db2");
        assert_eq!(resolution.resolved_value, serde_json::json!(2));
    }

    #[test]
    fn test_resolve_source_priority() {
        let resolver = ConflictResolver::new(ResolutionStrategy::SourcePriority, "primary");
        resolver.set_priority("db1", 10);
        resolver.set_priority("db2", 5);
        let versions = vec![
            DataVersion::new("db1", serde_json::json!(1), 1),
            DataVersion::new("db2", serde_json::json!(2), 1),
        ];
        let conflict = Conflict::new("k", ConflictType::ValueMismatch, versions);
        let resolution = resolver.resolve(&conflict);
        assert_eq!(resolution.winning_source, "db1");
    }

    #[test]
    fn test_resolve_primary_wins() {
        let resolver = ConflictResolver::new(ResolutionStrategy::PrimaryWins, "primary");
        let versions = vec![
            DataVersion::new("replica", serde_json::json!(1), 1),
            DataVersion::new("primary", serde_json::json!(2), 1),
        ];
        let conflict = Conflict::new("k", ConflictType::ValueMismatch, versions);
        let resolution = resolver.resolve(&conflict);
        assert_eq!(resolution.winning_source, "primary");
    }

    #[test]
    fn test_resolve_merge_fields() {
        let resolver = ConflictResolver::new(ResolutionStrategy::MergeFields, "primary");
        let versions = vec![
            DataVersion::new("db1", serde_json::json!({"a": 1, "b": 2}), 1),
            DataVersion::new("db2", serde_json::json!({"b": 3, "c": 4}), 1),
        ];
        let conflict = Conflict::new("k", ConflictType::ValueMismatch, versions);
        let resolution = resolver.resolve(&conflict);
        let obj = resolution.resolved_value.as_object().unwrap();
        assert_eq!(obj["a"], 1);
        assert_eq!(obj["b"], 2);
        assert_eq!(obj["c"], 4);
    }

    #[test]
    fn test_resolve_keep_conflict() {
        let resolver = ConflictResolver::new(ResolutionStrategy::KeepConflict, "primary");
        let versions = vec![
            DataVersion::new("db1", serde_json::json!(1), 1),
            DataVersion::new("db2", serde_json::json!(2), 1),
        ];
        let conflict = Conflict::new("k", ConflictType::ValueMismatch, versions);
        let resolution = resolver.resolve(&conflict);
        assert_eq!(resolution.strategy, ResolutionStrategy::KeepConflict);
        assert_eq!(resolver.unresolved_count(), 1);
    }

    #[test]
    fn test_resolved_count() {
        let resolver = ConflictResolver::new(ResolutionStrategy::LastWriteWins, "primary");
        let versions = vec![
            DataVersion::new("db1", serde_json::json!(1), 1),
            DataVersion::new("db2", serde_json::json!(2), 1),
        ];
        let conflict = Conflict::new("k", ConflictType::ValueMismatch, versions);
        resolver.resolve(&conflict);
        resolver.resolve(&conflict);
        assert_eq!(resolver.resolved_count(), 2);
    }

    #[test]
    fn test_conflict_log_basic() {
        let log = ConflictLog::new(100);
        assert_eq!(log.entry_count(), 0);
    }

    #[test]
    fn test_conflict_log_log_conflict() {
        let log = ConflictLog::new(100);
        let versions = vec![
            DataVersion::new("db1", serde_json::json!(1), 1),
            DataVersion::new("db2", serde_json::json!(2), 1),
        ];
        let conflict = Conflict::new("k", ConflictType::ValueMismatch, versions);
        log.log_conflict(conflict);
        assert_eq!(log.entry_count(), 1);
    }

    #[test]
    fn test_conflict_log_log_resolution() {
        let log = ConflictLog::new(100);
        let versions = vec![
            DataVersion::new("db1", serde_json::json!(1), 1),
            DataVersion::new("db2", serde_json::json!(2), 1),
        ];
        let conflict = Conflict::new("k", ConflictType::ValueMismatch, versions);
        let resolver = ConflictResolver::new(ResolutionStrategy::LastWriteWins, "primary");
        let resolution = resolver.resolve(&conflict);
        log.log_resolution(resolution);
        assert_eq!(log.entry_count(), 1);
    }

    #[test]
    fn test_conflict_log_max_entries() {
        let log = ConflictLog::new(2);
        for i in 0..5 {
            let versions = vec![
                DataVersion::new("db1", serde_json::json!(i), 1),
                DataVersion::new("db2", serde_json::json!(i + 1), 1),
            ];
            log.log_conflict(Conflict::new("k", ConflictType::ValueMismatch, versions));
        }
        assert_eq!(log.entry_count(), 2);
    }

    #[test]
    fn test_conflict_log_unresolved() {
        let log = ConflictLog::new(100);
        let versions = vec![
            DataVersion::new("db1", serde_json::json!(1), 1),
            DataVersion::new("db2", serde_json::json!(2), 1),
        ];
        log.log_conflict(Conflict::new("k", ConflictType::ValueMismatch, versions));
        assert_eq!(log.unresolved_entries().len(), 1);
    }

    #[test]
    fn test_conflict_log_clear() {
        let log = ConflictLog::new(100);
        let versions = vec![
            DataVersion::new("db1", serde_json::json!(1), 1),
            DataVersion::new("db2", serde_json::json!(2), 1),
        ];
        log.log_conflict(Conflict::new("k", ConflictType::ValueMismatch, versions));
        log.clear();
        assert_eq!(log.entry_count(), 0);
    }

    #[test]
    fn test_resolver_primary_source() {
        let resolver = ConflictResolver::new(ResolutionStrategy::PrimaryWins, "primary");
        assert_eq!(resolver.primary_source(), "primary");
    }

    #[test]
    fn test_resolver_strategy() {
        let resolver = ConflictResolver::new(ResolutionStrategy::MergeFields, "primary");
        assert_eq!(resolver.strategy(), ResolutionStrategy::MergeFields);
    }
}
