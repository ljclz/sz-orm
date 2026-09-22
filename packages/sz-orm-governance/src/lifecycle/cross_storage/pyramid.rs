//! 多级存储金字塔：数据按 Hot→Warm→Cold→Archive→Destroy 逐级流转

use std::collections::HashSet;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::super::rule_engine::LifecycleRuleEngine;
use super::super::types::{DataRange, DataTemperature};
use super::CrossStorageError;

/// 存储层级分类器（扩展自 ColdHotClassifier）
#[derive(Debug, Clone)]
pub struct StorageTierClassifier {
    available_tiers: HashSet<DataTemperature>,
}

impl StorageTierClassifier {
    pub fn new() -> Self {
        Self {
            available_tiers: [
                DataTemperature::Hot,
                DataTemperature::Warm,
                DataTemperature::Cold,
                DataTemperature::Archived,
            ]
            .into_iter()
            .collect(),
        }
    }

    pub fn with_tier_available(mut self, tier: DataTemperature, available: bool) -> Self {
        if available {
            self.available_tiers.insert(tier);
        } else {
            self.available_tiers.remove(&tier);
        }
        self
    }

    pub fn is_available(&self, tier: DataTemperature) -> bool {
        self.available_tiers.contains(&tier)
    }
}

impl Default for StorageTierClassifier {
    fn default() -> Self {
        Self::new()
    }
}

/// 金字塔配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PyramidConfig {
    pub enforce_adjacent_migration: bool,
}

impl Default for PyramidConfig {
    fn default() -> Self {
        Self {
            enforce_adjacent_migration: true,
        }
    }
}

/// 层级迁移步骤
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierMigrationStep {
    pub data_range: DataRange,
    pub from_tier: DataTemperature,
    pub to_tier: DataTemperature,
}

/// 迁移报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationReport {
    pub completed_steps: Vec<TierMigrationStep>,
    pub skipped_steps: Vec<TierMigrationStep>,
    pub total_migrated: u64,
}

/// 多级存储金字塔
pub struct StoragePyramid {
    classifier: StorageTierClassifier,
    rule_engine: Arc<LifecycleRuleEngine>,
    config: PyramidConfig,
}

impl StoragePyramid {
    pub fn new(
        classifier: StorageTierClassifier,
        rule_engine: Arc<LifecycleRuleEngine>,
        config: PyramidConfig,
    ) -> Self {
        Self {
            classifier,
            rule_engine,
            config,
        }
    }

    pub fn classifier(&self) -> &StorageTierClassifier {
        &self.classifier
    }

    pub fn rule_engine(&self) -> &Arc<LifecycleRuleEngine> {
        &self.rule_engine
    }

    /// 生成逐级迁移计划（禁止跨级跳转）
    pub fn plan_migration(
        &self,
        ranges: &[DataRange],
    ) -> Result<Vec<TierMigrationStep>, CrossStorageError> {
        let mut steps = Vec::new();
        for range in ranges {
            let from_tier = range.temperature;
            let to_tier = from_tier.next_tier().ok_or_else(|| {
                CrossStorageError::DataRangeInvalid(format!(
                    "数据范围 {:?} 已在最低层级",
                    from_tier
                ))
            })?;
            if self.config.enforce_adjacent_migration && !from_tier.can_migrate_to(to_tier) {
                return Err(CrossStorageError::SkipTierForbidden {
                    from: from_tier,
                    to: to_tier,
                });
            }
            if !self.classifier.is_available(to_tier) {
                return Err(CrossStorageError::StorageTierUnavailable(to_tier));
            }
            steps.push(TierMigrationStep {
                data_range: range.clone(),
                from_tier,
                to_tier,
            });
        }
        Ok(steps)
    }

    /// 执行迁移计划
    pub async fn execute_migration(
        &self,
        plan: &[TierMigrationStep],
    ) -> Result<MigrationReport, CrossStorageError> {
        let mut completed = Vec::new();
        let mut skipped = Vec::new();
        let mut total_migrated = 0u64;
        for step in plan {
            if !self.classifier.is_available(step.to_tier) {
                skipped.push(step.clone());
                continue;
            }
            completed.push(step.clone());
            total_migrated += step.data_range.row_count;
        }
        Ok(MigrationReport {
            completed_steps: completed,
            skipped_steps: skipped,
            total_migrated,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_range(temp: DataTemperature) -> DataRange {
        DataRange {
            min_id: 1,
            max_id: 100,
            row_count: 100,
            temperature: temp,
            last_accessed_days_ago: 30,
            access_frequency_per_day: 0.1,
        }
    }

    #[tokio::test]
    async fn test_plan_adjacent_migration() {
        let classifier = StorageTierClassifier::new();
        let engine = Arc::new(LifecycleRuleEngine::new());
        let pyramid = StoragePyramid::new(classifier, engine, PyramidConfig::default());
        let ranges = vec![make_range(DataTemperature::Hot)];
        let plan = pyramid.plan_migration(&ranges).unwrap();
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].from_tier, DataTemperature::Hot);
        assert_eq!(plan[0].to_tier, DataTemperature::Warm);
    }

    #[tokio::test]
    async fn test_plan_multi_tier_migration() {
        let classifier = StorageTierClassifier::new();
        let engine = Arc::new(LifecycleRuleEngine::new());
        let pyramid = StoragePyramid::new(classifier, engine, PyramidConfig::default());
        let ranges = vec![
            make_range(DataTemperature::Hot),
            make_range(DataTemperature::Warm),
            make_range(DataTemperature::Cold),
        ];
        let plan = pyramid.plan_migration(&ranges).unwrap();
        assert_eq!(plan.len(), 3);
        assert_eq!(plan[0].to_tier, DataTemperature::Warm);
        assert_eq!(plan[1].to_tier, DataTemperature::Cold);
        assert_eq!(plan[2].to_tier, DataTemperature::Archived);
    }

    #[tokio::test]
    async fn test_plan_destroy_tier_no_next() {
        let classifier = StorageTierClassifier::new();
        let engine = Arc::new(LifecycleRuleEngine::new());
        let pyramid = StoragePyramid::new(classifier, engine, PyramidConfig::default());
        let ranges = vec![make_range(DataTemperature::Destroy)];
        let result = pyramid.plan_migration(&ranges);
        assert!(matches!(
            result,
            Err(CrossStorageError::DataRangeInvalid(_))
        ));
    }

    #[tokio::test]
    async fn test_plan_tier_unavailable() {
        let classifier =
            StorageTierClassifier::new().with_tier_available(DataTemperature::Warm, false);
        let engine = Arc::new(LifecycleRuleEngine::new());
        let pyramid = StoragePyramid::new(classifier, engine, PyramidConfig::default());
        let ranges = vec![make_range(DataTemperature::Hot)];
        let result = pyramid.plan_migration(&ranges);
        assert!(matches!(
            result,
            Err(CrossStorageError::StorageTierUnavailable(_))
        ));
    }

    #[tokio::test]
    async fn test_execute_migration() {
        let classifier = StorageTierClassifier::new();
        let engine = Arc::new(LifecycleRuleEngine::new());
        let pyramid = StoragePyramid::new(classifier, engine, PyramidConfig::default());
        let ranges = vec![
            make_range(DataTemperature::Hot),
            make_range(DataTemperature::Warm),
        ];
        let plan = pyramid.plan_migration(&ranges).unwrap();
        let report = pyramid.execute_migration(&plan).await.unwrap();
        assert_eq!(report.completed_steps.len(), 2);
        assert!(report.skipped_steps.is_empty());
        assert_eq!(report.total_migrated, 200);
    }
}
