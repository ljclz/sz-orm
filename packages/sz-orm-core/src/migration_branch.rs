//! 迁移版本分支模块（v4.1.0，`migration-branch` feature gate）
//!
//! 提供多分支并行开发迁移管理：分支创建、合并冲突检测、版本拓扑。

use std::collections::{HashMap, HashSet};

/// 迁移分支
#[derive(Debug, Clone)]
pub struct MigrationBranch {
    /// 分支名
    pub name: String,
    /// 基线版本（从哪个版本分叉）
    pub base_version: String,
    /// 分支上的迁移版本列表
    pub versions: Vec<String>,
    /// 是否已合并
    pub merged: bool,
}

impl MigrationBranch {
    /// 创建新分支
    pub fn new(name: String, base_version: String) -> Self {
        Self {
            name,
            base_version,
            versions: Vec::new(),
            merged: false,
        }
    }

    /// 添加迁移版本
    pub fn add_version(&mut self, version: String) {
        self.versions.push(version);
    }

    /// 标记为已合并
    pub fn mark_merged(&mut self) {
        self.merged = true;
    }
}

/// 合并冲突
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeConflict {
    /// 冲突类型
    pub conflict_type: ConflictType,
    /// 冲突版本
    pub version: String,
    /// 源分支
    pub source_branch: String,
    /// 描述
    pub description: String,
}

/// 冲突类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictType {
    /// 版本号冲突
    VersionConflict,
    /// 依赖冲突
    DependencyConflict,
    /// Schema 冲突
    SchemaConflict,
}

/// 分支管理器
pub struct MigrationBranchManager {
    /// 主干版本
    main_versions: Vec<String>,
    /// 分支表
    branches: HashMap<String, MigrationBranch>,
}

impl MigrationBranchManager {
    /// 创建分支管理器
    pub fn new() -> Self {
        Self {
            main_versions: Vec::new(),
            branches: HashMap::new(),
        }
    }

    /// 添加主干版本
    pub fn add_main_version(&mut self, version: String) {
        self.main_versions.push(version);
    }

    /// 创建分支
    pub fn create_branch(&mut self, name: &str, base_version: &str) -> Result<(), String> {
        if self.branches.contains_key(name) {
            return Err(format!("分支 {} 已存在", name));
        }
        if !self.main_versions.contains(&base_version.to_string()) {
            return Err(format!("基线版本 {} 不存在于主干", base_version));
        }
        self.branches.insert(
            name.to_string(),
            MigrationBranch::new(name.to_string(), base_version.to_string()),
        );
        Ok(())
    }

    /// 在分支上添加迁移版本
    pub fn add_branch_version(&mut self, branch: &str, version: String) -> Result<(), String> {
        let b = self
            .branches
            .get_mut(branch)
            .ok_or_else(|| format!("分支 {} 不存在", branch))?;
        if b.merged {
            return Err(format!("分支 {} 已合并，不能添加版本", branch));
        }
        b.add_version(version);
        Ok(())
    }

    /// 检测合并冲突
    pub fn detect_conflicts(&self, branch: &str) -> Result<Vec<MergeConflict>, String> {
        let b = self
            .branches
            .get(branch)
            .ok_or_else(|| format!("分支 {} 不存在", branch))?;

        let mut conflicts = Vec::new();
        let main_set: HashSet<&String> = self.main_versions.iter().collect();

        for version in &b.versions {
            if main_set.contains(version) {
                conflicts.push(MergeConflict {
                    conflict_type: ConflictType::VersionConflict,
                    version: version.clone(),
                    source_branch: branch.to_string(),
                    description: format!("版本 {} 与主干版本冲突", version),
                });
            }
        }

        for (other_name, other_branch) in &self.branches {
            if other_name == branch {
                continue;
            }
            let other_set: HashSet<&String> = other_branch.versions.iter().collect();
            for version in &b.versions {
                if other_set.contains(version) {
                    conflicts.push(MergeConflict {
                        conflict_type: ConflictType::VersionConflict,
                        version: version.clone(),
                        source_branch: branch.to_string(),
                        description: format!("版本 {} 与分支 {} 冲突", version, other_name),
                    });
                }
            }
        }

        Ok(conflicts)
    }

    /// 合并分支到主干
    pub fn merge_branch(&mut self, branch: &str) -> Result<Vec<String>, String> {
        let conflicts = self.detect_conflicts(branch)?;
        if !conflicts.is_empty() {
            return Err(format!("合并失败：检测到 {} 个冲突", conflicts.len()));
        }

        let b = self
            .branches
            .get_mut(branch)
            .ok_or_else(|| format!("分支 {} 不存在", branch))?;
        if b.merged {
            return Err(format!("分支 {} 已合并", branch));
        }

        let merged_versions = b.versions.clone();
        for version in &merged_versions {
            self.main_versions.push(version.clone());
        }
        b.mark_merged();
        Ok(merged_versions)
    }

    /// 获取分支列表
    pub fn branches(&self) -> Vec<&MigrationBranch> {
        self.branches.values().collect()
    }

    /// 获取主干版本数
    pub fn main_version_count(&self) -> usize {
        self.main_versions.len()
    }
}

impl Default for MigrationBranchManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_branch() {
        let mut manager = MigrationBranchManager::new();
        manager.add_main_version("v1".to_string());
        manager.add_main_version("v2".to_string());

        assert!(manager.create_branch("feature-a", "v1").is_ok());
        assert!(manager.create_branch("feature-a", "v2").is_err());
        assert!(manager.create_branch("feature-b", "v3").is_err());
    }

    #[test]
    fn test_add_branch_version() {
        let mut manager = MigrationBranchManager::new();
        manager.add_main_version("v1".to_string());
        manager.create_branch("feature-a", "v1").unwrap();

        assert!(manager
            .add_branch_version("feature-a", "v1.1".to_string())
            .is_ok());
        assert!(manager
            .add_branch_version("nonexistent", "v1.2".to_string())
            .is_err());
    }

    #[test]
    fn test_detect_no_conflicts() {
        let mut manager = MigrationBranchManager::new();
        manager.add_main_version("v1".to_string());
        manager.add_main_version("v2".to_string());
        manager.create_branch("feature-a", "v1").unwrap();
        manager
            .add_branch_version("feature-a", "v1.1".to_string())
            .unwrap();

        let conflicts = manager.detect_conflicts("feature-a").unwrap();
        assert!(conflicts.is_empty());
    }

    #[test]
    fn test_detect_version_conflict_with_main() {
        let mut manager = MigrationBranchManager::new();
        manager.add_main_version("v1".to_string());
        manager.add_main_version("v2".to_string());
        manager.create_branch("feature-a", "v1").unwrap();
        manager
            .add_branch_version("feature-a", "v2".to_string())
            .unwrap();

        let conflicts = manager.detect_conflicts("feature-a").unwrap();
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].conflict_type, ConflictType::VersionConflict);
    }

    #[test]
    fn test_detect_conflict_between_branches() {
        let mut manager = MigrationBranchManager::new();
        manager.add_main_version("v1".to_string());
        manager.create_branch("feature-a", "v1").unwrap();
        manager.create_branch("feature-b", "v1").unwrap();
        manager
            .add_branch_version("feature-a", "v1.1".to_string())
            .unwrap();
        manager
            .add_branch_version("feature-b", "v1.1".to_string())
            .unwrap();

        let conflicts = manager.detect_conflicts("feature-a").unwrap();
        assert_eq!(conflicts.len(), 1);
        assert!(conflicts[0].description.contains("feature-b"));
    }

    #[test]
    fn test_merge_branch_success() {
        let mut manager = MigrationBranchManager::new();
        manager.add_main_version("v1".to_string());
        manager.create_branch("feature-a", "v1").unwrap();
        manager
            .add_branch_version("feature-a", "v1.1".to_string())
            .unwrap();
        manager
            .add_branch_version("feature-a", "v1.2".to_string())
            .unwrap();

        let merged = manager.merge_branch("feature-a").unwrap();
        assert_eq!(merged.len(), 2);
        assert_eq!(manager.main_version_count(), 3);
    }

    #[test]
    fn test_merge_branch_with_conflicts() {
        let mut manager = MigrationBranchManager::new();
        manager.add_main_version("v1".to_string());
        manager.add_main_version("v2".to_string());
        manager.create_branch("feature-a", "v1").unwrap();
        manager
            .add_branch_version("feature-a", "v2".to_string())
            .unwrap();

        let result = manager.merge_branch("feature-a");
        assert!(result.is_err());
    }

    #[test]
    fn test_merge_already_merged() {
        let mut manager = MigrationBranchManager::new();
        manager.add_main_version("v1".to_string());
        manager.create_branch("feature-a", "v1").unwrap();
        manager
            .add_branch_version("feature-a", "v1.1".to_string())
            .unwrap();

        manager.merge_branch("feature-a").unwrap();
        let result = manager.merge_branch("feature-a");
        assert!(result.is_err());
    }

    #[test]
    fn test_add_version_after_merge() {
        let mut manager = MigrationBranchManager::new();
        manager.add_main_version("v1".to_string());
        manager.create_branch("feature-a", "v1").unwrap();
        manager
            .add_branch_version("feature-a", "v1.1".to_string())
            .unwrap();
        manager.merge_branch("feature-a").unwrap();

        let result = manager.add_branch_version("feature-a", "v1.2".to_string());
        assert!(result.is_err());
    }
}
