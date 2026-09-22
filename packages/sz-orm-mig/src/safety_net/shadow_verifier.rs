//! 影子流量验证器：复制生产流量到影子实例 → 执行变更 → 对比结果 → 检测缺陷

use serde::{Deserialize, Serialize};

use crate::gray_traffic_router::GrayTrafficRouter;

use super::SafetyNetError;

/// 影子验证配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShadowConfig {
    pub shadow_ratio: u32,
    pub max_defects: u32,
}

impl Default for ShadowConfig {
    fn default() -> Self {
        Self {
            shadow_ratio: 10,
            max_defects: 0,
        }
    }
}

/// 影子验证结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShadowVerifyResult {
    pub defect_detected: bool,
    pub defects: Vec<String>,
    pub pass_rate: f64,
    pub total_requests: u32,
    pub shadow_requests: u32,
}

/// 影子流量验证器
pub struct ShadowTrafficVerifier {
    router: GrayTrafficRouter,
    config: ShadowConfig,
}

impl ShadowTrafficVerifier {
    pub fn new(router: GrayTrafficRouter, config: ShadowConfig) -> Self {
        Self { router, config }
    }

    pub fn config(&self) -> &ShadowConfig {
        &self.config
    }

    /// 验证变更
    pub fn verify(
        &self,
        defects: Vec<String>,
        total_requests: u32,
    ) -> Result<ShadowVerifyResult, SafetyNetError> {
        let mut shadow_count = 0;
        for i in 0..total_requests {
            if self.router.is_gray_traffic(i as u64, &[], &[]) {
                shadow_count += 1;
            }
        }
        let defect_detected = !defects.is_empty();
        let pass_rate = if shadow_count > 0 {
            (shadow_count - defects.len() as u32) as f64 / shadow_count as f64
        } else {
            1.0
        };
        let result = ShadowVerifyResult {
            defect_detected,
            defects: defects.clone(),
            pass_rate,
            total_requests,
            shadow_requests: shadow_count,
        };
        if defect_detected && defects.len() as u32 > self.config.max_defects {
            return Err(SafetyNetError::ShadowTrafficDefectDetected(format!(
                "发现 {} 个缺陷",
                defects.len()
            )));
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gray_release::TrafficSwitchMethod;

    fn make_verifier(ratio: u32) -> ShadowTrafficVerifier {
        let router = GrayTrafficRouter::new(TrafficSwitchMethod::Percentage, ratio);
        ShadowTrafficVerifier::new(router, ShadowConfig::default())
    }

    #[test]
    fn test_verify_no_defects() {
        let verifier = make_verifier(10);
        let result = verifier.verify(vec![], 100).unwrap();
        assert!(!result.defect_detected);
        assert_eq!(result.shadow_requests, 10);
        assert_eq!(result.pass_rate, 1.0);
    }

    #[test]
    fn test_verify_with_defects() {
        let verifier = make_verifier(50);
        let defects = vec!["defect_1".to_string(), "defect_2".to_string()];
        let result = verifier.verify(defects, 100);
        assert!(matches!(
            result,
            Err(SafetyNetError::ShadowTrafficDefectDetected(_))
        ));
    }

    #[test]
    fn test_verify_shadow_routing() {
        let verifier = make_verifier(30);
        let result = verifier.verify(vec![], 100).unwrap();
        assert_eq!(result.shadow_requests, 30);
    }

    #[test]
    fn test_verify_zero_ratio() {
        let verifier = make_verifier(0);
        let result = verifier.verify(vec![], 100).unwrap();
        assert_eq!(result.shadow_requests, 0);
        assert_eq!(result.pass_rate, 1.0);
    }
}
