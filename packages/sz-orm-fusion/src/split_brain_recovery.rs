//! 脑裂恢复器（v8.1.0，`split-brain-detect` feature gate）
//!
//! 分区恢复 → 冲突检测 → 按策略解决 → 自动合并 ≤ 30s → 数据最终一致。
//! 复用 v8.0.0 `conflict.rs`（ConflictResolver）。

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::conflict::{ConflictResolver, DataVersion, Resolution, ResolutionStrategy};

/// 脑裂恢复错误
#[derive(Debug, Clone)]
pub enum DistError {
    /// 冲突无法解决（告警 `SPLIT_BRAIN_CONFLICT_UNRESOLVED`）
    ConflictUnresolved { key: String, detail: String },
    /// 恢复超时
    RecoveryTimeout { elapsed: Duration, limit: Duration },
}

impl std::fmt::Display for DistError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ConflictUnresolved { key, detail } => {
                write!(
                    f,
                    "SPLIT_BRAIN_CONFLICT_UNRESOLVED: key={key}, detail={detail}"
                )
            }
            Self::RecoveryTimeout { elapsed, limit } => {
                write!(f, "recovery timeout: {elapsed:?} > {limit:?}")
            }
        }
    }
}

impl std::error::Error for DistError {}

/// 分区数据（脑裂两侧的数据版本，key → versions）
#[derive(Debug, Clone, Default)]
pub struct PartitionData {
    /// key → 两侧的数据版本列表
    pub versions_by_key: HashMap<String, Vec<DataVersion>>,
}

impl PartitionData {
    pub fn new() -> Self {
        Self::default()
    }

    /// 添加分区 A 的数据
    pub fn add_partition_a(&mut self, key: &str, version: DataVersion) {
        self.versions_by_key
            .entry(key.to_string())
            .or_default()
            .push(version);
    }

    /// 添加分区 B 的数据
    pub fn add_partition_b(&mut self, key: &str, version: DataVersion) {
        self.versions_by_key
            .entry(key.to_string())
            .or_default()
            .push(version);
    }

    /// 键数量
    pub fn key_count(&self) -> usize {
        self.versions_by_key.len()
    }
}

/// 恢复结果
#[derive(Debug, Clone)]
pub struct RecoveryResult {
    /// 合并后的解决结果
    pub merged: Vec<Resolution>,
    /// 恢复耗时
    pub elapsed: Duration,
    /// 是否成功（无未解决冲突）
    pub success: bool,
}

/// 脑裂恢复器
///
/// 分区恢复 → 冲突检测 → 按策略解决 → 自动合并 ≤ 30s → 数据最终一致。
pub struct SplitBrainRecovery {
    resolver: ConflictResolver,
    /// 恢复超时上限（默认 30s）
    pub recovery_timeout: Duration,
}

impl SplitBrainRecovery {
    /// 创建恢复器
    pub fn new(strategy: ResolutionStrategy, primary_source: &str) -> Self {
        Self {
            resolver: ConflictResolver::new(strategy, primary_source),
            recovery_timeout: Duration::from_secs(30),
        }
    }

    /// 恢复分区数据
    ///
    /// 分区恢复 → 冲突检测 → 按策略解决 → 自动合并 ≤ 30s → 数据最终一致。
    /// 无冲突直接合并；冲突无法解决标记 `SPLIT_BRAIN_CONFLICT_UNRESOLVED`。
    pub fn recover(&self, data: &PartitionData) -> Result<RecoveryResult, DistError> {
        let start = Instant::now();
        let mut merged = Vec::new();

        for (key, versions) in &data.versions_by_key {
            if versions.len() < 2 {
                // 单侧数据，无冲突，直接合并
                continue;
            }
            if let Some(conflict) = self.resolver.detect_conflict(key, versions) {
                let resolution = self.resolver.resolve(&conflict);
                if resolution.strategy == ResolutionStrategy::KeepConflict {
                    return Err(DistError::ConflictUnresolved {
                        key: key.clone(),
                        detail: format!(
                            "conflict type={:?} cannot be auto-resolved",
                            conflict.conflict_type
                        ),
                    });
                }
                merged.push(resolution);
            }
        }

        let elapsed = start.elapsed();
        if elapsed > self.recovery_timeout {
            return Err(DistError::RecoveryTimeout {
                elapsed,
                limit: self.recovery_timeout,
            });
        }

        Ok(RecoveryResult {
            merged,
            elapsed,
            success: true,
        })
    }

    /// 无冲突快速合并
    pub fn merge_no_conflict(&self, data: &PartitionData) -> Vec<DataVersion> {
        let mut result = Vec::new();
        for versions in data.versions_by_key.values() {
            result.extend(versions.iter().cloned());
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_version(source: &str, value: serde_json::Value, version: u64) -> DataVersion {
        DataVersion::new(source, value, version)
    }

    #[test]
    fn test_no_conflict_merge() {
        let mut data = PartitionData::new();
        data.add_partition_a("k1", make_version("A", json!("v1"), 1));
        data.add_partition_b("k1", make_version("B", json!("v1"), 1));
        let recovery = SplitBrainRecovery::new(ResolutionStrategy::LastWriteWins, "A");
        let result = recovery.recover(&data).unwrap();
        assert!(result.success);
        // 相同值无冲突，merged 为空
        assert!(result.merged.is_empty());
    }

    #[test]
    fn test_conflict_resolved_last_write_wins() {
        let mut data = PartitionData::new();
        data.add_partition_a("k1", make_version("A", json!("v1"), 1).with_timestamp(100));
        data.add_partition_b("k1", make_version("B", json!("v2"), 2).with_timestamp(200));
        let recovery = SplitBrainRecovery::new(ResolutionStrategy::LastWriteWins, "A");
        let result = recovery.recover(&data).unwrap();
        assert!(result.success);
        assert_eq!(result.merged.len(), 1);
        assert_eq!(result.merged[0].winning_source, "B");
    }

    #[test]
    fn test_conflict_unresolved_keep_conflict() {
        let mut data = PartitionData::new();
        data.add_partition_a("k1", make_version("A", json!("v1"), 1));
        data.add_partition_b("k1", make_version("B", json!("v2"), 2));
        let recovery = SplitBrainRecovery::new(ResolutionStrategy::KeepConflict, "A");
        let err = recovery.recover(&data).unwrap_err();
        assert!(matches!(err, DistError::ConflictUnresolved { .. }));
    }

    #[test]
    fn test_single_partition_no_conflict() {
        let mut data = PartitionData::new();
        data.add_partition_a("k1", make_version("A", json!("v1"), 1));
        let recovery = SplitBrainRecovery::new(ResolutionStrategy::LastWriteWins, "A");
        let result = recovery.recover(&data).unwrap();
        assert!(result.success);
        assert!(result.merged.is_empty());
    }

    #[test]
    fn test_multiple_keys_mixed() {
        let mut data = PartitionData::new();
        // k1: 无冲突
        data.add_partition_a("k1", make_version("A", json!("same"), 1));
        data.add_partition_b("k1", make_version("B", json!("same"), 1));
        // k2: 有冲突可解决
        data.add_partition_a("k2", make_version("A", json!("v1"), 1).with_timestamp(100));
        data.add_partition_b("k2", make_version("B", json!("v2"), 2).with_timestamp(200));
        let recovery = SplitBrainRecovery::new(ResolutionStrategy::LastWriteWins, "A");
        let result = recovery.recover(&data).unwrap();
        assert!(result.success);
        assert_eq!(result.merged.len(), 1);
    }

    #[test]
    fn test_merge_no_conflict_fast_path() {
        let mut data = PartitionData::new();
        data.add_partition_a("k1", make_version("A", json!("v1"), 1));
        data.add_partition_b("k2", make_version("B", json!("v2"), 2));
        let recovery = SplitBrainRecovery::new(ResolutionStrategy::LastWriteWins, "A");
        let merged = recovery.merge_no_conflict(&data);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn test_recovery_within_30s() {
        let mut data = PartitionData::new();
        for i in 0..100 {
            let key = format!("k{i}");
            data.add_partition_a(&key, make_version("A", json!("v1"), 1).with_timestamp(100));
            data.add_partition_b(&key, make_version("B", json!("v2"), 2).with_timestamp(200));
        }
        let recovery = SplitBrainRecovery::new(ResolutionStrategy::LastWriteWins, "A");
        let result = recovery.recover(&data).unwrap();
        assert!(result.elapsed < Duration::from_secs(30));
        assert!(result.success);
    }

    #[test]
    fn test_empty_partition() {
        let data = PartitionData::new();
        let recovery = SplitBrainRecovery::new(ResolutionStrategy::LastWriteWins, "A");
        let result = recovery.recover(&data).unwrap();
        assert!(result.success);
        assert!(result.merged.is_empty());
    }
}
