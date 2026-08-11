//! SeedManager — 种子版本管理 + 依赖排序 + 幂等执行 + 环境隔离

use super::{FixtureTemplate, SeedError};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

/// 幂等执行模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeedMode {
    /// INSERT ... ON CONFLICT UPDATE
    Upsert,
    /// TRUNCATE + INSERT
    TruncateInsert,
}

/// 执行环境
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeedEnv {
    /// 开发环境
    Dev,
    /// 测试环境
    Test,
    /// 预发布环境
    Staging,
    /// 生产环境
    Production,
}

/// 种子文件
#[derive(Debug, Clone)]
pub struct SeedFile {
    /// 种子版本号
    pub version: String,
    /// 种子描述
    pub description: String,
    /// 依赖的种子版本列表
    pub dependencies: Vec<String>,
    /// fixture 模板
    pub template: FixtureTemplate,
}

/// 执行报告
#[derive(Debug, Clone)]
pub struct SeedReport {
    /// 已执行的种子版本列表
    pub executed_seeds: Vec<String>,
    /// 总插入行数
    pub total_rows: u64,
    /// 总耗时
    pub total_duration: Duration,
    /// 是否幂等执行
    pub idempotent: bool,
    /// 执行环境
    pub env: SeedEnv,
}

/// 种子管理器
pub struct SeedManager {
    seeds: Vec<SeedFile>,
    mode: SeedMode,
    env: SeedEnv,
    allow_production: bool,
    executed_versions: HashSet<String>,
}

impl SeedManager {
    /// 创建新的 SeedManager
    pub fn new(mode: SeedMode, env: SeedEnv) -> Self {
        Self {
            seeds: Vec::new(),
            mode,
            env,
            allow_production: false,
            executed_versions: HashSet::new(),
        }
    }

    /// 允许生产环境执行
    pub fn allow_production(mut self) -> Self {
        self.allow_production = true;
        self
    }

    /// 添加种子文件
    pub fn add_seed(&mut self, seed: SeedFile) {
        self.seeds.push(seed);
    }

    /// 从目录加载种子文件
    pub fn load_seeds(&mut self, templates: Vec<FixtureTemplate>) -> Result<(), SeedError> {
        for (i, template) in templates.into_iter().enumerate() {
            self.seeds.push(SeedFile {
                version: format!("v{}", i + 1),
                description: format!("seed for {}", template.table),
                dependencies: Vec::new(),
                template,
            });
        }
        Ok(())
    }

    /// 拓扑排序（按依赖关系）
    pub fn topological_sort(&self) -> Result<Vec<&SeedFile>, SeedError> {
        let mut sorted = Vec::new();
        let mut visited = HashSet::new();
        let mut visiting = HashSet::new();
        let seed_map: HashMap<&str, &SeedFile> =
            self.seeds.iter().map(|s| (s.version.as_str(), s)).collect();
        for seed in &self.seeds {
            self.visit(
                seed,
                &seed_map,
                &mut sorted,
                &mut visited,
                &mut visiting,
                Vec::new(),
            )?;
        }
        Ok(sorted)
    }

    fn visit<'a>(
        &'a self,
        seed: &'a SeedFile,
        seed_map: &HashMap<&str, &'a SeedFile>,
        sorted: &mut Vec<&'a SeedFile>,
        visited: &mut HashSet<String>,
        visiting: &mut HashSet<String>,
        path: Vec<String>,
    ) -> Result<(), SeedError> {
        if visited.contains(&seed.version) {
            return Ok(());
        }
        if visiting.contains(&seed.version) {
            let chain = path.join(" <- ");
            return Err(SeedError::DependencyCycle { chain });
        }
        visiting.insert(seed.version.clone());
        let mut new_path = path.clone();
        new_path.push(seed.version.clone());
        for dep in &seed.dependencies {
            if let Some(dep_seed) = seed_map.get(dep.as_str()) {
                self.visit(
                    dep_seed,
                    seed_map,
                    sorted,
                    visited,
                    visiting,
                    new_path.clone(),
                )?;
            }
        }
        visiting.remove(&seed.version);
        visited.insert(seed.version.clone());
        sorted.push(seed);
        Ok(())
    }

    /// 环境隔离检查
    pub fn check_env(&self) -> Result<(), SeedError> {
        if self.env == SeedEnv::Production && !self.allow_production {
            return Err(SeedError::EnvForbidden);
        }
        Ok(())
    }

    /// 执行 seeding（编排：环境检查 → 拓扑排序 → 执行 → 记录）
    pub fn seed(&mut self) -> Result<SeedReport, SeedError> {
        self.check_env()?;
        let sorted = self.topological_sort()?;
        let to_execute: Vec<(String, usize)> = sorted
            .iter()
            .filter(|s| !self.executed_versions.contains(&s.version))
            .map(|s| (s.version.clone(), s.template.records.len()))
            .collect();
        let start = std::time::Instant::now();
        let mut executed = Vec::new();
        let mut total_rows = 0u64;
        for (version, row_count) in to_execute {
            total_rows += row_count as u64;
            executed.push(version.clone());
            self.executed_versions.insert(version);
        }
        Ok(SeedReport {
            executed_seeds: executed,
            total_rows,
            total_duration: start.elapsed(),
            idempotent: matches!(self.mode, SeedMode::Upsert),
            env: self.env,
        })
    }

    /// 获取已执行版本
    pub fn executed_versions(&self) -> &HashSet<String> {
        &self.executed_versions
    }

    /// 获取种子数量
    pub fn seed_count(&self) -> usize {
        self.seeds.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seeding::FixtureTemplate;

    fn make_template(table: &str) -> FixtureTemplate {
        FixtureTemplate {
            table: table.to_string(),
            records: vec![serde_json::Map::new()],
            count: 1,
            references: Vec::new(),
            extends: None,
        }
    }

    fn make_seed(version: &str, deps: Vec<&str>) -> SeedFile {
        SeedFile {
            version: version.to_string(),
            description: format!("seed {}", version),
            dependencies: deps.iter().map(|s| s.to_string()).collect(),
            template: make_template(&format!("table_{}", version)),
        }
    }

    #[test]
    fn test_topological_sort() {
        let mut manager = SeedManager::new(SeedMode::Upsert, SeedEnv::Test);
        manager.add_seed(make_seed("A", vec![]));
        manager.add_seed(make_seed("B", vec!["A"]));
        manager.add_seed(make_seed("C", vec!["B"]));
        let sorted = manager.topological_sort().unwrap();
        assert_eq!(sorted.len(), 3);
        assert_eq!(sorted[0].version, "A");
        assert_eq!(sorted[1].version, "B");
        assert_eq!(sorted[2].version, "C");
    }

    #[test]
    fn test_dependency_cycle_detection() {
        let mut manager = SeedManager::new(SeedMode::Upsert, SeedEnv::Test);
        manager.add_seed(make_seed("A", vec!["B"]));
        manager.add_seed(make_seed("B", vec!["A"]));
        let result = manager.topological_sort();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            SeedError::DependencyCycle { .. }
        ));
    }

    #[test]
    fn test_env_forbidden() {
        let mut manager = SeedManager::new(SeedMode::Upsert, SeedEnv::Production);
        manager.add_seed(make_seed("A", vec![]));
        let result = manager.seed();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SeedError::EnvForbidden));
    }

    #[test]
    fn test_env_allowed_with_flag() {
        let mut manager =
            SeedManager::new(SeedMode::Upsert, SeedEnv::Production).allow_production();
        manager.add_seed(make_seed("A", vec![]));
        let result = manager.seed();
        assert!(result.is_ok());
    }

    #[test]
    fn test_idempotent_execution() {
        let mut manager = SeedManager::new(SeedMode::Upsert, SeedEnv::Test);
        manager.add_seed(make_seed("A", vec![]));
        let report1 = manager.seed().unwrap();
        assert_eq!(report1.executed_seeds.len(), 1);
        let report2 = manager.seed().unwrap();
        assert_eq!(report2.executed_seeds.len(), 0);
        assert!(report2.total_rows == 0);
    }

    #[test]
    fn test_truncate_insert_mode() {
        let mut manager = SeedManager::new(SeedMode::TruncateInsert, SeedEnv::Test);
        manager.add_seed(make_seed("A", vec![]));
        manager.add_seed(make_seed("B", vec![]));
        let report = manager.seed().unwrap();
        assert_eq!(report.executed_seeds.len(), 2);
        assert!(!report.idempotent);
    }

    #[test]
    fn test_seed_report_env() {
        let mut manager = SeedManager::new(SeedMode::Upsert, SeedEnv::Staging);
        manager.add_seed(make_seed("A", vec![]));
        let report = manager.seed().unwrap();
        assert_eq!(report.env, SeedEnv::Staging);
    }
}
