//! 冷热分类器

use super::types::{
    AccessPatternStats, ColdHotClassification, ColdHotThreshold, DataRange, DataTemperature,
    LifecycleError,
};

/// 冷热分类器：基于访问频率和时间维度将数据分类为热/温/冷三级
pub struct ColdHotClassifier {
    threshold: ColdHotThreshold,
}

impl ColdHotClassifier {
    pub fn new(threshold: ColdHotThreshold) -> Self {
        Self { threshold }
    }

    pub fn with_default_threshold() -> Self {
        Self::new(ColdHotThreshold::default())
    }

    pub fn classify(
        &self,
        table: &str,
        stats: &AccessPatternStats,
        ranges: &[DataRange],
    ) -> Result<ColdHotClassification, LifecycleError> {
        if stats.table != table {
            return Err(LifecycleError::TableNotFound(format!(
                "统计信息表名 {} 与目标表 {} 不匹配",
                stats.table, table
            )));
        }

        let mut hot_data = Vec::new();
        let mut warm_data = Vec::new();
        let mut cold_data = Vec::new();

        for mut range in ranges.iter().cloned() {
            range.temperature = self.classify_range(&range);
            match range.temperature {
                DataTemperature::Hot => hot_data.push(range),
                DataTemperature::Warm => warm_data.push(range),
                DataTemperature::Cold => cold_data.push(range),
                _ => {}
            }
        }

        Ok(ColdHotClassification {
            table: table.to_string(),
            hot_data,
            warm_data,
            cold_data,
        })
    }

    pub fn classify_range(&self, range: &DataRange) -> DataTemperature {
        if range.access_frequency_per_day >= self.threshold.access_frequency_per_day
            && range.last_accessed_days_ago <= self.threshold.age_days
        {
            DataTemperature::Hot
        } else if range.last_accessed_days_ago <= self.threshold.age_days * 2 {
            DataTemperature::Warm
        } else {
            DataTemperature::Cold
        }
    }

    pub fn promote_to_hot(&self, range: &DataRange) -> DataRange {
        DataRange {
            temperature: DataTemperature::Hot,
            ..range.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_range(freq: f64, days_ago: u32) -> DataRange {
        DataRange {
            min_id: 0,
            max_id: 100,
            row_count: 100,
            temperature: DataTemperature::Hot,
            last_accessed_days_ago: days_ago,
            access_frequency_per_day: freq,
        }
    }

    fn make_stats(table: &str) -> AccessPatternStats {
        AccessPatternStats {
            table: table.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn test_classify_hot() {
        let classifier = ColdHotClassifier::with_default_threshold();
        let range = make_range(5.0, 5);
        assert_eq!(classifier.classify_range(&range), DataTemperature::Hot);
    }

    #[test]
    fn test_classify_warm() {
        let classifier = ColdHotClassifier::with_default_threshold();
        let range = make_range(0.5, 45);
        assert_eq!(classifier.classify_range(&range), DataTemperature::Warm);
    }

    #[test]
    fn test_classify_cold() {
        let classifier = ColdHotClassifier::with_default_threshold();
        let range = make_range(0.1, 90);
        assert_eq!(classifier.classify_range(&range), DataTemperature::Cold);
    }

    #[test]
    fn test_classify_full() {
        let classifier = ColdHotClassifier::with_default_threshold();
        let stats = make_stats("orders");
        let ranges = vec![make_range(5.0, 5), make_range(0.5, 45), make_range(0.1, 90)];

        let result = classifier.classify("orders", &stats, &ranges).unwrap();
        assert_eq!(result.hot_data.len(), 1);
        assert_eq!(result.warm_data.len(), 1);
        assert_eq!(result.cold_data.len(), 1);
    }

    #[test]
    fn test_classify_table_mismatch() {
        let classifier = ColdHotClassifier::with_default_threshold();
        let stats = make_stats("other");
        let result = classifier.classify("orders", &stats, &[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_promote_to_hot() {
        let classifier = ColdHotClassifier::with_default_threshold();
        let range = make_range(0.1, 90);
        let promoted = classifier.promote_to_hot(&range);
        assert_eq!(promoted.temperature, DataTemperature::Hot);
    }
}
