//! 模型路由策略（TASK-011 占位）
#![allow(dead_code)]

use crate::types::{ModelOpsError, ModelRouterConfig};

pub struct ModelRouter {
    pub config: ModelRouterConfig,
}

impl ModelRouter {
    pub fn new(config: ModelRouterConfig) -> Self {
        Self { config }
    }

    pub fn route(&self, complexity: f64) -> Result<&str, ModelOpsError> {
        if complexity < 0.3 {
            Ok(&self.config.small_model)
        } else if complexity < 0.7 {
            Ok(&self.config.medium_model)
        } else {
            Ok(&self.config.large_model)
        }
    }
}
