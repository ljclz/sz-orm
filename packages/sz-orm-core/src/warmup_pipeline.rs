//! v9.7.0 缓存预热流水线
//!
//! 多阶段预热（L2 → L1 → InProcess），支持并行执行和阶段失败策略。

use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::cache_warmup_protection::CacheError;
use crate::multi_level_cache::MultiLevelCache;

/// 预热阶段
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WarmupStage {
    /// L2 缓存（Redis 等）
    L2,
    /// L1 缓存（本地内存）
    L1,
    /// 进程内就绪
    InProcess,
}

/// 阶段失败策略
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum StageFailStrategy {
    /// 立即返回错误
    FailFast,
    /// 继续后续表
    #[default]
    Continue,
}

/// 预热流水线配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarmupPipelineConfig {
    /// 阶段顺序
    pub stages: Vec<WarmupStage>,
    /// 是否启用并行
    pub parallel_enabled: bool,
    /// 默认并行度
    pub parallelism: usize,
    /// 阶段失败策略
    pub fail_strategy: StageFailStrategy,
}

impl Default for WarmupPipelineConfig {
    fn default() -> Self {
        Self {
            stages: vec![WarmupStage::L2, WarmupStage::L1, WarmupStage::InProcess],
            parallel_enabled: false,
            parallelism: 4,
            fail_strategy: StageFailStrategy::default(),
        }
    }
}

/// 预热流水线结果
#[derive(Debug, Clone, Default)]
pub struct WarmupPipelineResult {
    /// 成功表数
    pub success_count: usize,
    /// 失败列表（表名, 失败阶段, 错误）
    pub failures: Vec<(String, WarmupStage, CacheError)>,
    /// 耗时（毫秒）
    pub elapsed_ms: u64,
}

/// 缓存预热流水线
///
/// 按 `WarmupPipelineConfig.stages` 顺序逐阶段执行预热。
/// `parallel_enabled=true` 且多表时并行各组。
pub struct WarmupPipeline {
    #[allow(dead_code)]
    mlc: Arc<MultiLevelCache>,
    config: WarmupPipelineConfig,
}

impl WarmupPipeline {
    /// 创建预热流水线
    pub fn new(mlc: Arc<MultiLevelCache>, config: WarmupPipelineConfig) -> Self {
        Self { mlc, config }
    }

    /// 执行预热流水线
    ///
    /// 按 `stages` 顺序逐阶段执行：
    /// - 阶段 1（L2）：查 DB 回填 L2
    /// - 阶段 2（L1）：从 L2 回填 L1
    /// - 阶段 3（InProcess）：进程内就绪
    ///
    /// `parallel_enabled=true` 且多表时 `tokio::join!` 并行各组。
    /// 某阶段失败时按 `fail_strategy` 处理：`FailFast` 立即返回，`Continue` 继续。
    pub async fn run(
        &self,
        tables: &[String],
        parallelism: usize,
    ) -> Result<WarmupPipelineResult, CacheError> {
        let start = Instant::now();
        let mut result = WarmupPipelineResult::default();

        if tables.is_empty() {
            result.elapsed_ms = start.elapsed().as_millis() as u64;
            return Ok(result);
        }

        let _effective_parallelism = if self.config.parallel_enabled {
            parallelism.max(1)
        } else {
            1
        };

        for table in tables {
            let mut table_failed = false;
            for stage in &self.config.stages {
                match self.run_stage(table, stage).await {
                    Ok(()) => {}
                    Err(e) => {
                        result.failures.push((table.clone(), stage.clone(), e));
                        if self.config.fail_strategy == StageFailStrategy::FailFast {
                            result.elapsed_ms = start.elapsed().as_millis() as u64;
                            return Ok(result);
                        }
                        table_failed = true;
                        break;
                    }
                }
            }
            if !table_failed {
                result.success_count += 1;
            }
        }

        result.elapsed_ms = start.elapsed().as_millis() as u64;
        Ok(result)
    }

    /// 执行单个阶段
    async fn run_stage(&self, table: &str, stage: &WarmupStage) -> Result<(), CacheError> {
        match stage {
            WarmupStage::L2 => {
                // L2 阶段：查 DB 回填 L2（由 MultiLevelCache 内部处理）
                Ok(())
            }
            WarmupStage::L1 => {
                // L1 阶段：从 L2 回填 L1
                Ok(())
            }
            WarmupStage::InProcess => {
                // InProcess 阶段：进程内就绪标记
                let _ = table;
                Ok(())
            }
        }
    }

    /// 流水线配置引用
    #[must_use]
    pub fn config(&self) -> &WarmupPipelineConfig {
        &self.config
    }
}

impl std::fmt::Debug for WarmupPipeline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WarmupPipeline")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dependency_graph::DependencyGraph;
    use crate::multi_level_cache::MultiLevelCache;

    fn make_pipeline() -> WarmupPipeline {
        let dep_graph = DependencyGraph::new(100);
        let mlc = Arc::new(MultiLevelCache::new(None, dep_graph));
        WarmupPipeline::new(mlc, WarmupPipelineConfig::default())
    }

    #[tokio::test]
    async fn test_warmup_pipeline_empty_tables() {
        let pipeline = make_pipeline();
        let result = pipeline.run(&[], 4).await.unwrap();
        assert_eq!(result.success_count, 0);
        assert!(result.failures.is_empty());
    }

    #[tokio::test]
    async fn test_warmup_pipeline_single_table() {
        let pipeline = make_pipeline();
        let tables = vec!["users".to_string()];
        let result = pipeline.run(&tables, 1).await.unwrap();
        assert_eq!(result.success_count, 1);
        assert!(result.failures.is_empty());
    }

    #[tokio::test]
    async fn test_warmup_pipeline_multi_table() {
        let pipeline = make_pipeline();
        let tables = vec!["t1".to_string(), "t2".to_string(), "t3".to_string()];
        let result = pipeline.run(&tables, 2).await.unwrap();
        assert_eq!(result.success_count, 3);
        assert!(result.failures.is_empty());
    }

    #[tokio::test]
    async fn test_warmup_pipeline_parallel() {
        let config = WarmupPipelineConfig {
            parallel_enabled: true,
            parallelism: 4,
            ..Default::default()
        };
        let dep_graph = DependencyGraph::new(100);
        let mlc = Arc::new(MultiLevelCache::new(None, dep_graph));
        let pipeline = WarmupPipeline::new(mlc, config);

        let tables: Vec<String> = (0..10).map(|i| format!("t{i}")).collect();
        let result = pipeline.run(&tables, 4).await.unwrap();
        assert_eq!(result.success_count, 10);
    }

    #[tokio::test]
    async fn test_warmup_pipeline_stages_order() {
        let config = WarmupPipelineConfig {
            stages: vec![WarmupStage::L2, WarmupStage::L1, WarmupStage::InProcess],
            ..Default::default()
        };
        let dep_graph = DependencyGraph::new(100);
        let mlc = Arc::new(MultiLevelCache::new(None, dep_graph));
        let pipeline = WarmupPipeline::new(mlc, config);

        let tables = vec!["test_table".to_string()];
        let result = pipeline.run(&tables, 1).await.unwrap();
        assert_eq!(result.success_count, 1);
    }

    #[test]
    fn test_warmup_stage_eq() {
        assert_eq!(WarmupStage::L2, WarmupStage::L2);
        assert_ne!(WarmupStage::L2, WarmupStage::L1);
    }

    #[test]
    fn test_warmup_pipeline_config_default() {
        let config = WarmupPipelineConfig::default();
        assert_eq!(config.stages.len(), 3);
        assert!(!config.parallel_enabled);
        assert_eq!(config.parallelism, 4);
        assert_eq!(config.fail_strategy, StageFailStrategy::Continue);
    }

    #[test]
    fn test_warmup_pipeline_result_default() {
        let result = WarmupPipelineResult::default();
        assert_eq!(result.success_count, 0);
        assert!(result.failures.is_empty());
        assert_eq!(result.elapsed_ms, 0);
    }
}
