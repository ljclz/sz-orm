//! 推理优化配置与优化器（TASK-027）

use crate::types::{ModelOpsError, Quantization};
use serde::{Deserialize, Serialize};

/// 推理优化配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceOptConfig {
    pub quantization: Quantization,
    pub batch_size: usize,
    pub enable_kv_cache: bool,
    pub enable_flash_attention: bool,
    pub max_context_length: usize,
    pub temperature: f64,
    pub top_p: f64,
}

impl Default for InferenceOptConfig {
    fn default() -> Self {
        Self {
            quantization: Quantization::Int8,
            batch_size: 1,
            enable_kv_cache: true,
            enable_flash_attention: true,
            max_context_length: 4096,
            temperature: 0.7,
            top_p: 0.9,
        }
    }
}

/// 推理优化结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceOptResult {
    pub estimated_speedup: f64,
    pub estimated_memory_mb: f64,
    pub config: InferenceOptConfig,
    pub optimizations: Vec<String>,
}

/// 推理优化器
pub struct InferenceOptimizer {
    baseline_memory_mb: f64,
    baseline_latency_ms: f64,
}

impl InferenceOptimizer {
    pub fn new(baseline_memory_mb: f64, baseline_latency_ms: f64) -> Self {
        Self {
            baseline_memory_mb,
            baseline_latency_ms,
        }
    }

    /// 优化推理配置并返回预期收益
    pub fn optimize(
        &self,
        config: &InferenceOptConfig,
    ) -> Result<InferenceOptResult, ModelOpsError> {
        if config.batch_size == 0 {
            return Err(ModelOpsError::InferenceFailed("批大小不能为 0".to_string()));
        }
        if config.temperature < 0.0 || config.temperature > 2.0 {
            return Err(ModelOpsError::InferenceFailed(
                "温度应在 [0, 2] 范围".to_string(),
            ));
        }
        if config.top_p < 0.0 || config.top_p > 1.0 {
            return Err(ModelOpsError::InferenceFailed(
                "top_p 应在 [0, 1] 范围".to_string(),
            ));
        }

        let mut speedup = 1.0;
        let mut memory = self.baseline_memory_mb;
        let mut optimizations = Vec::new();

        match config.quantization {
            Quantization::Int4 => {
                speedup *= 2.5;
                memory *= 0.25;
                optimizations.push("INT4 量化：内存减少 75%，速度提升 2.5x".to_string());
            }
            Quantization::Int8 => {
                speedup *= 1.5;
                memory *= 0.5;
                optimizations.push("INT8 量化：内存减少 50%，速度提升 1.5x".to_string());
            }
            Quantization::None => {
                optimizations.push("无量化：全精度推理".to_string());
            }
        }

        if config.batch_size > 1 {
            let batch_speedup = 1.0 + (config.batch_size as f64 - 1.0) * 0.3;
            speedup *= batch_speedup;
            optimizations.push(format!(
                "批处理 {}：吞吐量提升 {:.1}x",
                config.batch_size, batch_speedup
            ));
        }

        if config.enable_kv_cache {
            speedup *= 1.8;
            memory *= 1.15;
            optimizations.push("KV Cache：重复推理速度提升 1.8x".to_string());
        }

        if config.enable_flash_attention {
            speedup *= 1.3;
            memory *= 0.9;
            optimizations.push("Flash Attention：长序列速度提升 1.3x".to_string());
        }

        let _estimated_latency = self.baseline_latency_ms / speedup;

        Ok(InferenceOptResult {
            estimated_speedup: speedup,
            estimated_memory_mb: memory,
            config: config.clone(),
            optimizations,
        })
    }

    /// 比较两个配置的预期性能
    pub fn compare_configs(
        &self,
        config_a: &InferenceOptConfig,
        config_b: &InferenceOptConfig,
    ) -> Result<ConfigComparison, ModelOpsError> {
        let result_a = self.optimize(config_a)?;
        let result_b = self.optimize(config_b)?;

        Ok(ConfigComparison {
            speedup_ratio: result_a.estimated_speedup / result_b.estimated_speedup,
            memory_ratio: result_a.estimated_memory_mb / result_b.estimated_memory_mb,
            better_config: if result_a.estimated_speedup > result_b.estimated_speedup {
                ConfigChoice::A
            } else {
                ConfigChoice::B
            },
        })
    }

    /// 估算推理延迟（毫秒）
    pub fn estimate_latency(&self, config: &InferenceOptConfig) -> Result<f64, ModelOpsError> {
        let result = self.optimize(config)?;
        Ok(self.baseline_latency_ms / result.estimated_speedup)
    }
}

/// 配置比较结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigComparison {
    pub speedup_ratio: f64,
    pub memory_ratio: f64,
    pub better_config: ConfigChoice,
}

/// 配置选择
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConfigChoice {
    A,
    B,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_optimize_int4() {
        let optimizer = InferenceOptimizer::new(4000.0, 100.0);
        let config = InferenceOptConfig {
            quantization: Quantization::Int4,
            ..Default::default()
        };
        let result = optimizer.optimize(&config).unwrap();
        assert!(result.estimated_speedup > 2.0, "INT4 应显著提速");
        assert!(result.estimated_memory_mb < 2000.0, "INT4 应减少内存");
    }

    #[test]
    fn test_optimize_batch_processing() {
        let optimizer = InferenceOptimizer::new(4000.0, 100.0);
        let config = InferenceOptConfig {
            batch_size: 8,
            ..Default::default()
        };
        let result = optimizer.optimize(&config).unwrap();
        assert!(result.estimated_speedup > 1.0, "批处理应提速");
    }

    #[test]
    fn test_optimize_invalid_batch() {
        let optimizer = InferenceOptimizer::new(4000.0, 100.0);
        let config = InferenceOptConfig {
            batch_size: 0,
            ..Default::default()
        };
        assert!(optimizer.optimize(&config).is_err());
    }

    #[test]
    fn test_compare_configs() {
        let optimizer = InferenceOptimizer::new(4000.0, 100.0);
        let config_a = InferenceOptConfig {
            quantization: Quantization::Int4,
            ..Default::default()
        };
        let config_b = InferenceOptConfig {
            quantization: Quantization::None,
            ..Default::default()
        };
        let comparison = optimizer.compare_configs(&config_a, &config_b).unwrap();
        assert_eq!(comparison.better_config, ConfigChoice::A);
    }

    #[test]
    fn test_estimate_latency() {
        let optimizer = InferenceOptimizer::new(4000.0, 100.0);
        let config = InferenceOptConfig {
            quantization: Quantization::Int4,
            enable_kv_cache: true,
            ..Default::default()
        };
        let latency = optimizer.estimate_latency(&config).unwrap();
        assert!(latency < 100.0, "优化后延迟应降低");
    }
}
