//! NL 查询闭环主管线（TASK-008 占位，后续实现）
#![allow(dead_code)]

use crate::types::{NlQueryError, NlQueryResponse};

/// NL 查询管线
pub struct NlQueryPipeline;

impl NlQueryPipeline {
    pub fn new() -> Self {
        Self
    }

    pub async fn query(&self, _nl: &str) -> Result<NlQueryResponse, NlQueryError> {
        Err(NlQueryError::Nl2SqlFailed("未实现".into()))
    }
}

impl Default for NlQueryPipeline {
    fn default() -> Self {
        Self::new()
    }
}
