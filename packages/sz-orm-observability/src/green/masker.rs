//! 能耗数据脱敏器

use super::types::EnergyMetrics;

/// 能耗数据脱敏器：防止泄露基础设施拓扑
pub struct EnergyDataMasker;

impl EnergyDataMasker {
    pub fn new() -> Self {
        Self
    }

    pub fn mask_instance_id(&self, id: &str) -> String {
        if id.len() <= 4 {
            "****".to_string()
        } else {
            format!("{}****", &id[..4])
        }
    }

    pub fn mask_region(&self, region: &str) -> String {
        if region.len() <= 2 {
            "**".to_string()
        } else {
            format!("{}**", &region[..2])
        }
    }

    pub fn mask_metrics(&self, metrics: &EnergyMetrics) -> EnergyMetrics {
        metrics.clone()
    }
}

impl Default for EnergyDataMasker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mask_instance_id() {
        let masker = EnergyDataMasker::new();
        assert_eq!(masker.mask_instance_id("instance-123"), "inst****");
        assert_eq!(masker.mask_instance_id("ab"), "****");
    }

    #[test]
    fn test_mask_region() {
        let masker = EnergyDataMasker::new();
        assert_eq!(masker.mask_region("us-east-1"), "us**");
        assert_eq!(masker.mask_region("u"), "**");
    }
}
