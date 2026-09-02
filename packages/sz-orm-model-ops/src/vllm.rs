//! vLLM 推理后端（TASK-010）

use crate::types::ModelOpsError;

pub struct VllmProvider {
    pub endpoint: String,
}

impl VllmProvider {
    pub fn new(endpoint: &str) -> Self {
        Self {
            endpoint: endpoint.to_string(),
        }
    }

    pub async fn complete(&self, _prompt: &str) -> Result<String, ModelOpsError> {
        Err(ModelOpsError::InferenceFailed("未连接".into()))
    }
}
