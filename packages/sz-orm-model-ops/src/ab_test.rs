//! A/B 测试框架（TASK-033）

use crate::types::ModelOpsError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A/B 测试配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbTestConfig {
    pub test_name: String,
    pub variant_a_name: String,
    pub variant_b_name: String,
    pub sample_ratio: f64,
    pub min_samples: usize,
}

/// A/B 测试样本
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbTestSample {
    pub variant: Variant,
    pub success: bool,
    pub latency_ms: f64,
}

/// 变体标识
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Variant {
    A,
    B,
}

/// 变体统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariantStats {
    pub total: usize,
    pub successes: usize,
    pub success_rate: f64,
    pub avg_latency_ms: f64,
}

/// A/B 测试结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbTestResult {
    pub config: AbTestConfig,
    pub stats_a: VariantStats,
    pub stats_b: VariantStats,
    pub winner: Option<Variant>,
    pub confidence: f64,
    pub conclusion: String,
}

/// A/B 测试框架
pub struct AbTestFramework {
    config: AbTestConfig,
    samples: Vec<AbTestSample>,
}

impl AbTestFramework {
    pub fn new(config: AbTestConfig) -> Self {
        Self {
            config,
            samples: Vec::new(),
        }
    }

    /// 记录样本
    pub fn record(&mut self, sample: AbTestSample) {
        self.samples.push(sample);
    }

    /// 分析结果
    pub fn analyze(&self) -> Result<AbTestResult, ModelOpsError> {
        let stats_a = self.compute_stats(&Variant::A);
        let stats_b = self.compute_stats(&Variant::B);

        if stats_a.total < self.config.min_samples || stats_b.total < self.config.min_samples {
            return Ok(AbTestResult {
                config: self.config.clone(),
                stats_a,
                stats_b,
                winner: None,
                confidence: 0.0,
                conclusion: format!(
                    "样本不足（最少需要 {} 个），无法得出结论",
                    self.config.min_samples
                ),
            });
        }

        let (winner, confidence, conclusion) = self.determine_winner(&stats_a, &stats_b);

        Ok(AbTestResult {
            config: self.config.clone(),
            stats_a,
            stats_b,
            winner,
            confidence,
            conclusion,
        })
    }

    fn compute_stats(&self, variant: &Variant) -> VariantStats {
        let variant_samples: Vec<_> = self
            .samples
            .iter()
            .filter(|s| &s.variant == variant)
            .collect();

        let total = variant_samples.len();
        let successes = variant_samples.iter().filter(|s| s.success).count();
        let success_rate = if total > 0 {
            successes as f64 / total as f64
        } else {
            0.0
        };
        let avg_latency_ms = if total > 0 {
            variant_samples.iter().map(|s| s.latency_ms).sum::<f64>() / total as f64
        } else {
            0.0
        };

        VariantStats {
            total,
            successes,
            success_rate,
            avg_latency_ms,
        }
    }

    fn determine_winner(
        &self,
        stats_a: &VariantStats,
        stats_b: &VariantStats,
    ) -> (Option<Variant>, f64, String) {
        let diff = stats_b.success_rate - stats_a.success_rate;
        let confidence = (diff.abs() * 100.0).min(100.0);

        if diff.abs() < 0.05 {
            (None, confidence, "两个变体表现接近，无显著差异".to_string())
        } else if diff > 0.0 {
            (
                Some(Variant::B),
                confidence,
                format!(
                    "变体 B 成功率更高（{:.1}% vs {:.1}%），建议采用 B",
                    stats_b.success_rate * 100.0,
                    stats_a.success_rate * 100.0
                ),
            )
        } else {
            (
                Some(Variant::A),
                confidence,
                format!(
                    "变体 A 成功率更高（{:.1}% vs {:.1}%），建议保留 A",
                    stats_a.success_rate * 100.0,
                    stats_b.success_rate * 100.0
                ),
            )
        }
    }

    /// 获取样本数
    pub fn sample_count(&self) -> usize {
        self.samples.len()
    }

    /// 按变体分组获取样本数
    pub fn sample_counts_by_variant(&self) -> HashMap<Variant, usize> {
        let mut counts = HashMap::new();
        counts.insert(Variant::A, 0);
        counts.insert(Variant::B, 0);
        for sample in &self.samples {
            *counts.entry(sample.variant.clone()).or_insert(0) += 1;
        }
        counts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config() -> AbTestConfig {
        AbTestConfig {
            test_name: "nl2sql_comparison".to_string(),
            variant_a_name: "qwen-7b".to_string(),
            variant_b_name: "qwen-14b".to_string(),
            sample_ratio: 0.5,
            min_samples: 10,
        }
    }

    #[test]
    fn test_analyze_insufficient_samples() {
        let framework = AbTestFramework::new(make_config());
        let result = framework.analyze().unwrap();
        assert!(result.winner.is_none());
        assert!(result.conclusion.contains("样本不足"));
    }

    #[test]
    fn test_analyze_b_wins() {
        let mut framework = AbTestFramework::new(make_config());

        for _ in 0..10 {
            framework.record(AbTestSample {
                variant: Variant::A,
                success: true,
                latency_ms: 100.0,
            });
            framework.record(AbTestSample {
                variant: Variant::A,
                success: false,
                latency_ms: 100.0,
            });
        }
        for _ in 0..15 {
            framework.record(AbTestSample {
                variant: Variant::B,
                success: true,
                latency_ms: 80.0,
            });
        }
        for _ in 0..5 {
            framework.record(AbTestSample {
                variant: Variant::B,
                success: false,
                latency_ms: 80.0,
            });
        }

        let result = framework.analyze().unwrap();
        assert_eq!(result.winner, Some(Variant::B));
    }

    #[test]
    fn test_analyze_no_significant_difference() {
        let mut framework = AbTestFramework::new(make_config());

        for _ in 0..10 {
            framework.record(AbTestSample {
                variant: Variant::A,
                success: true,
                latency_ms: 100.0,
            });
            framework.record(AbTestSample {
                variant: Variant::B,
                success: true,
                latency_ms: 100.0,
            });
        }

        let result = framework.analyze().unwrap();
        assert!(result.winner.is_none());
        assert!(result.conclusion.contains("无显著差异"));
    }

    #[test]
    fn test_sample_counts() {
        let mut framework = AbTestFramework::new(make_config());
        framework.record(AbTestSample {
            variant: Variant::A,
            success: true,
            latency_ms: 100.0,
        });
        framework.record(AbTestSample {
            variant: Variant::B,
            success: true,
            latency_ms: 100.0,
        });
        framework.record(AbTestSample {
            variant: Variant::A,
            success: false,
            latency_ms: 100.0,
        });

        assert_eq!(framework.sample_count(), 3);
        let counts = framework.sample_counts_by_variant();
        assert_eq!(counts[&Variant::A], 2);
        assert_eq!(counts[&Variant::B], 1);
    }
}
