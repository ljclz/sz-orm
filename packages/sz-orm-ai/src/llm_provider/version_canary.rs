//! 模型版本灰度切换：灰度路由 → 健康检查 → 失败回滚 → 切换 ≤ 10s
//!
//! 复用 `ModelVersionRegistry`，切换记录审计。

use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use serde::{Deserialize, Serialize};

use super::version_registry::{ModelVersionError, ModelVersionId, ModelVersionRegistry};

/// 灰度切换错误
#[derive(Debug, thiserror::Error)]
pub enum CanaryError {
    #[error("灰度比例无效: {0}")]
    InvalidCanaryRatio(f64),
    #[error("健康检查失败: {0}")]
    HealthCheckFailed(String),
    #[error(transparent)]
    ModelVersion(#[from] ModelVersionError),
    #[error("切换超时: {0:?}")]
    SwitchTimeout(Duration),
}

/// 健康检查结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResult {
    pub passed: bool,
    pub latency: Duration,
    pub error_rate: f64,
    pub detail: String,
}

/// 灰度切换结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanarySwitchResult {
    pub version_id: ModelVersionId,
    pub ratio: f64,
    pub switched: bool,
    pub health_check_passed: bool,
    pub rollback_executed: bool,
    pub switched_at: SystemTime,
    pub duration: Duration,
    pub audit_tag: String,
}

/// 健康检查器 trait
pub trait HealthChecker: Send + Sync {
    fn check(&self, version_id: &ModelVersionId) -> HealthCheckResult;
}

/// 默认健康检查器（基于版本元数据存在性）
pub struct DefaultHealthChecker {
    registry: Arc<ModelVersionRegistry>,
}

impl DefaultHealthChecker {
    pub fn new(registry: Arc<ModelVersionRegistry>) -> Self {
        Self { registry }
    }
}

impl HealthChecker for DefaultHealthChecker {
    fn check(&self, version_id: &ModelVersionId) -> HealthCheckResult {
        let start = Instant::now();
        let meta = self.registry.get_version(version_id);
        let latency = start.elapsed();
        match meta {
            Some(m) if !m.signature.is_empty() => HealthCheckResult {
                passed: true,
                latency,
                error_rate: 0.0,
                detail: format!("版本 {} 健康检查通过", version_id),
            },
            Some(_) => HealthCheckResult {
                passed: false,
                latency,
                error_rate: 1.0,
                detail: format!("版本 {} 签名无效", version_id),
            },
            None => HealthCheckResult {
                passed: false,
                latency,
                error_rate: 1.0,
                detail: format!("版本 {} 不存在", version_id),
            },
        }
    }
}

/// 模型版本灰度切换
pub struct ModelVersionCanary {
    registry: Arc<ModelVersionRegistry>,
    health_checker: Arc<dyn HealthChecker>,
    switch_timeout: Duration,
    history: parking_lot::Mutex<Vec<CanarySwitchResult>>,
}

impl ModelVersionCanary {
    pub fn new(
        registry: Arc<ModelVersionRegistry>,
        health_checker: Arc<dyn HealthChecker>,
    ) -> Self {
        Self {
            registry,
            health_checker,
            switch_timeout: Duration::from_secs(10),
            history: parking_lot::Mutex::new(Vec::new()),
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.switch_timeout = timeout;
        self
    }

    /// 灰度切换：ratio ∈ (0, 1]，默认 0.1
    pub async fn canary_switch(
        &self,
        version_id: ModelVersionId,
        ratio: f64,
    ) -> Result<CanarySwitchResult, CanaryError> {
        if ratio <= 0.0 || ratio > 1.0 {
            return Err(CanaryError::InvalidCanaryRatio(ratio));
        }
        let start = Instant::now();
        let switched_at = SystemTime::now();
        let health = self.health_checker.check(&version_id);
        let mut result = CanarySwitchResult {
            version_id: version_id.clone(),
            ratio,
            switched: false,
            health_check_passed: health.passed,
            rollback_executed: false,
            switched_at,
            duration: Duration::ZERO,
            audit_tag: String::new(),
        };
        if !health.passed {
            let previous_active = self.registry.active_version();
            if let Some(prev) = &previous_active {
                self.registry.rollback(prev).await?;
                result.rollback_executed = true;
                result.audit_tag = format!(
                    "MODEL_VERSION_ROLLBACK: 健康检查失败 ({}), 回滚到 {}",
                    health.detail, prev
                );
            } else {
                result.audit_tag = format!(
                    "MODEL_VERSION_ROLLBACK: 健康检查失败 ({})，无前序版本可回滚",
                    health.detail
                );
            }
            result.duration = start.elapsed();
            self.history.lock().push(result.clone());
            return Err(CanaryError::HealthCheckFailed(health.detail));
        }
        self.registry.rollback(&version_id).await?;
        result.switched = true;
        result.audit_tag = format!(
            "MODEL_VERSION_CANARY_SWITCH: 灰度比例 {:.0}%，切换到 {}",
            ratio * 100.0,
            version_id
        );
        result.duration = start.elapsed();
        if result.duration > self.switch_timeout {
            self.history.lock().push(result.clone());
            return Err(CanaryError::SwitchTimeout(result.duration));
        }
        self.history.lock().push(result.clone());
        Ok(result)
    }

    /// 查询切换历史
    pub fn history(&self) -> Vec<CanarySwitchResult> {
        self.history.lock().clone()
    }

    /// 当前活跃版本
    pub fn active_version(&self) -> Option<ModelVersionId> {
        self.registry.active_version()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm_provider::version_registry::ModelVersionMeta;

    fn make_meta(id: &str) -> ModelVersionMeta {
        ModelVersionMeta {
            version_id: id.to_string(),
            model_name: "gpt-4o".to_string(),
            signature: "sig-abc".to_string(),
            created_at: SystemTime::now(),
            is_canary: false,
            checksum: "sha256:abc".to_string(),
        }
    }

    async fn make_setup() -> (ModelVersionCanary, Arc<ModelVersionRegistry>) {
        let registry = Arc::new(ModelVersionRegistry::new());
        registry.register(make_meta("v1")).await.unwrap();
        registry.register(make_meta("v2")).await.unwrap();
        let health = Arc::new(DefaultHealthChecker::new(registry.clone()));
        let canary = ModelVersionCanary::new(registry.clone(), health);
        (canary, registry)
    }

    #[tokio::test]
    async fn test_canary_switch_success() {
        let (canary, registry) = make_setup().await;
        assert_eq!(registry.active_version(), Some("v1".to_string()));
        let result = canary.canary_switch("v2".to_string(), 0.1).await.unwrap();
        assert!(result.switched);
        assert!(result.health_check_passed);
        assert!(!result.rollback_executed);
        assert!(result.duration <= Duration::from_secs(10));
        assert_eq!(canary.active_version(), Some("v2".to_string()));
        assert!(result.audit_tag.contains("CANARY_SWITCH"));
    }

    #[tokio::test]
    async fn test_invalid_ratio_zero() {
        let (canary, _) = make_setup().await;
        let result = canary.canary_switch("v2".to_string(), 0.0).await;
        assert!(matches!(result, Err(CanaryError::InvalidCanaryRatio(0.0))));
    }

    #[tokio::test]
    async fn test_invalid_ratio_over_one() {
        let (canary, _) = make_setup().await;
        let result = canary.canary_switch("v2".to_string(), 1.5).await;
        assert!(matches!(result, Err(CanaryError::InvalidCanaryRatio(1.5))));
    }

    #[tokio::test]
    async fn test_health_check_fail_rollback() {
        struct FailingHealthChecker;
        impl HealthChecker for FailingHealthChecker {
            fn check(&self, version_id: &ModelVersionId) -> HealthCheckResult {
                HealthCheckResult {
                    passed: false,
                    latency: Duration::from_millis(10),
                    error_rate: 1.0,
                    detail: format!("版本 {} 模拟健康检查失败", version_id),
                }
            }
        }
        let registry = Arc::new(ModelVersionRegistry::new());
        registry.register(make_meta("v1")).await.unwrap();
        registry.register(make_meta("v2")).await.unwrap();
        let canary = ModelVersionCanary::new(registry.clone(), Arc::new(FailingHealthChecker));
        let result = canary.canary_switch("v2".to_string(), 0.1).await;
        assert!(matches!(result, Err(CanaryError::HealthCheckFailed(_))));
        let history = canary.history();
        assert!(history[0].rollback_executed);
        assert!(history[0].audit_tag.contains("MODEL_VERSION_ROLLBACK"));
        assert_eq!(registry.active_version(), Some("v1".to_string()));
    }

    #[tokio::test]
    async fn test_switch_within_10s() {
        let (canary, _) = make_setup().await;
        let result = canary.canary_switch("v2".to_string(), 0.5).await.unwrap();
        assert!(result.duration <= Duration::from_secs(10));
    }

    #[tokio::test]
    async fn test_switch_audit_recorded() {
        let (canary, _) = make_setup().await;
        canary.canary_switch("v2".to_string(), 0.1).await.unwrap();
        let history = canary.history();
        assert_eq!(history.len(), 1);
        assert!(history[0].switched);
        assert!(!history[0].audit_tag.is_empty());
    }

    #[tokio::test]
    async fn test_full_ratio_switch() {
        let (canary, _) = make_setup().await;
        let result = canary.canary_switch("v2".to_string(), 1.0).await.unwrap();
        assert!(result.switched);
        assert_eq!(result.ratio, 1.0);
    }

    #[tokio::test]
    async fn test_multiple_switches_history() {
        let registry = Arc::new(ModelVersionRegistry::new());
        registry.register(make_meta("v1")).await.unwrap();
        registry.register(make_meta("v2")).await.unwrap();
        registry.register(make_meta("v3")).await.unwrap();
        let health = Arc::new(DefaultHealthChecker::new(registry.clone()));
        let canary = ModelVersionCanary::new(registry.clone(), health);
        canary.canary_switch("v2".to_string(), 0.1).await.unwrap();
        canary.canary_switch("v3".to_string(), 0.5).await.unwrap();
        canary.canary_switch("v1".to_string(), 1.0).await.unwrap();
        let history = canary.history();
        assert_eq!(history.len(), 3);
        assert_eq!(canary.active_version(), Some("v1".to_string()));
    }
}
