//! 一致性级别配置（v8.1.0，`consistency-tunable` feature gate）
//!
//! 级别配置（strong/sequential/eventual）→ 按操作粒度应用（全局或按操作）。
//! 复用既有 `cache_coherence.rs`（v8.0.0 缓存一致性可配扩展为共识级别可配）。

use std::collections::HashMap;

/// 一致性级别配置错误
#[derive(Debug, Clone)]
pub enum DistError {
    /// 级别无效
    InvalidConsistencyLevel(String),
}

impl std::fmt::Display for DistError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidConsistencyLevel(msg) => write!(f, "invalid consistency level: {msg}"),
        }
    }
}

impl std::error::Error for DistError {}

/// 一致性级别
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConsistencyLevel {
    /// 强一致（线性一致性）
    Strong,
    /// 顺序一致
    Sequential,
    /// 最终一致
    Eventual,
}

impl ConsistencyLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            ConsistencyLevel::Strong => "strong",
            ConsistencyLevel::Sequential => "sequential",
            ConsistencyLevel::Eventual => "eventual",
        }
    }

    pub fn parse(s: &str) -> Result<Self, DistError> {
        match s {
            "strong" => Ok(ConsistencyLevel::Strong),
            "sequential" => Ok(ConsistencyLevel::Sequential),
            "eventual" => Ok(ConsistencyLevel::Eventual),
            _ => Err(DistError::InvalidConsistencyLevel(s.to_string())),
        }
    }
}

/// 配置粒度
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigGranularity {
    /// 全局配置
    Global,
    /// 按操作配置
    PerOperation,
}

/// 一致性级别配置
///
/// 级别配置 → 按操作粒度应用（全局或按操作）→ 操作按配置级别执行。
pub struct ConsistencyLevelConfig {
    /// 默认级别
    pub level: ConsistencyLevel,
    /// 配置粒度
    pub granularity: ConfigGranularity,
    /// 操作 → 级别映射（PerOperation 粒度时使用）
    pub operation_mapping: HashMap<String, ConsistencyLevel>,
}

impl ConsistencyLevelConfig {
    /// 创建全局配置
    pub fn global(level: ConsistencyLevel) -> Self {
        Self {
            level,
            granularity: ConfigGranularity::Global,
            operation_mapping: HashMap::new(),
        }
    }

    /// 创建按操作配置
    pub fn per_operation(default: ConsistencyLevel) -> Self {
        Self {
            level: default,
            granularity: ConfigGranularity::PerOperation,
            operation_mapping: HashMap::new(),
        }
    }

    /// 为操作设置一致性级别
    pub fn set_operation_level(&mut self, op: &str, level: ConsistencyLevel) {
        self.granularity = ConfigGranularity::PerOperation;
        self.operation_mapping.insert(op.to_string(), level);
    }

    /// 获取操作的一致性级别
    ///
    /// 按操作粒度：优先返回操作映射的级别，未映射返回全局级别。
    pub fn for_operation(&self, op: &str) -> ConsistencyLevel {
        match self.granularity {
            ConfigGranularity::Global => self.level,
            ConfigGranularity::PerOperation => self
                .operation_mapping
                .get(op)
                .copied()
                .unwrap_or(self.level),
        }
    }

    /// 当前默认级别
    pub fn default_level(&self) -> ConsistencyLevel {
        self.level
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_global_config() {
        let config = ConsistencyLevelConfig::global(ConsistencyLevel::Strong);
        assert_eq!(config.for_operation("read"), ConsistencyLevel::Strong);
        assert_eq!(config.for_operation("write"), ConsistencyLevel::Strong);
        assert_eq!(config.default_level(), ConsistencyLevel::Strong);
    }

    #[test]
    fn test_per_operation_config() {
        let mut config = ConsistencyLevelConfig::per_operation(ConsistencyLevel::Eventual);
        config.set_operation_level("write", ConsistencyLevel::Strong);
        config.set_operation_level("read", ConsistencyLevel::Sequential);
        assert_eq!(config.for_operation("write"), ConsistencyLevel::Strong);
        assert_eq!(config.for_operation("read"), ConsistencyLevel::Sequential);
        // 未映射操作返回默认
        assert_eq!(config.for_operation("delete"), ConsistencyLevel::Eventual);
    }

    #[test]
    fn test_consistency_level_from_str() {
        assert_eq!(
            ConsistencyLevel::parse("strong").unwrap(),
            ConsistencyLevel::Strong
        );
        assert_eq!(
            ConsistencyLevel::parse("sequential").unwrap(),
            ConsistencyLevel::Sequential
        );
        assert_eq!(
            ConsistencyLevel::parse("eventual").unwrap(),
            ConsistencyLevel::Eventual
        );
        assert!(ConsistencyLevel::parse("invalid").is_err());
    }

    #[test]
    fn test_consistency_level_as_str() {
        assert_eq!(ConsistencyLevel::Strong.as_str(), "strong");
        assert_eq!(ConsistencyLevel::Sequential.as_str(), "sequential");
        assert_eq!(ConsistencyLevel::Eventual.as_str(), "eventual");
    }

    #[test]
    fn test_global_to_per_operation_switch() {
        let mut config = ConsistencyLevelConfig::global(ConsistencyLevel::Strong);
        config.set_operation_level("analytics", ConsistencyLevel::Eventual);
        assert_eq!(config.granularity, ConfigGranularity::PerOperation);
        assert_eq!(
            config.for_operation("analytics"),
            ConsistencyLevel::Eventual
        );
        assert_eq!(config.for_operation("write"), ConsistencyLevel::Strong);
    }

    #[test]
    fn test_all_three_levels() {
        let mut config = ConsistencyLevelConfig::per_operation(ConsistencyLevel::Eventual);
        config.set_operation_level("critical_write", ConsistencyLevel::Strong);
        config.set_operation_level("ordered_read", ConsistencyLevel::Sequential);
        config.set_operation_level("best_effort", ConsistencyLevel::Eventual);
        assert_eq!(
            config.for_operation("critical_write"),
            ConsistencyLevel::Strong
        );
        assert_eq!(
            config.for_operation("ordered_read"),
            ConsistencyLevel::Sequential
        );
        assert_eq!(
            config.for_operation("best_effort"),
            ConsistencyLevel::Eventual
        );
    }
}
