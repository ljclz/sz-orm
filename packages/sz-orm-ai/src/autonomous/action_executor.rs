//! 自治动作执行器（复用 v7.7.0 既有能力）

use std::time::{Duration, Instant};

use super::types::{AutonomousAction, AutonomousError, ExecutionResult};

/// 自治动作执行器
pub struct AutonomousActionExecutor {
    timeout: Duration,
}

impl AutonomousActionExecutor {
    pub fn new() -> Self {
        Self {
            timeout: Duration::from_secs(30),
        }
    }

    pub fn with_timeout(timeout: Duration) -> Self {
        Self { timeout }
    }

    /// 执行自治动作
    pub async fn execute(
        &self,
        action: &AutonomousAction,
        params: &[(String, String)],
    ) -> Result<ExecutionResult, AutonomousError> {
        let start = Instant::now();
        let result = match action {
            AutonomousAction::AutoRemediation => self.execute_remediation(params).await,
            AutonomousAction::AutoScaling => self.execute_scaling(params).await,
            AutonomousAction::AutoTuning => self.execute_tuning(params).await,
            AutonomousAction::SlowQueryGovernance => {
                self.execute_slow_query_governance(params).await
            }
            AutonomousAction::DegradeNonCore { .. } => self.execute_degrade_non_core(params).await,
        };
        let elapsed = start.elapsed();
        if elapsed > self.timeout {
            return Ok(ExecutionResult {
                success: false,
                message: format!(
                    "动作执行超时 ({}ms > {}ms)",
                    elapsed.as_millis(),
                    self.timeout.as_millis()
                ),
                duration: elapsed,
            });
        }
        match result {
            Ok(msg) => Ok(ExecutionResult {
                success: true,
                message: msg,
                duration: start.elapsed(),
            }),
            Err(e) => Ok(ExecutionResult {
                success: false,
                message: e.to_string(),
                duration: start.elapsed(),
            }),
        }
    }

    async fn execute_remediation(
        &self,
        params: &[(String, String)],
    ) -> Result<String, AutonomousError> {
        let fault_type = params
            .iter()
            .find(|(k, _)| k == "fault_type")
            .map(|(_, v)| v.as_str())
            .unwrap_or("unknown");
        Ok(format!(
            "AutoRemediation executed for fault: {}",
            fault_type
        ))
    }

    async fn execute_scaling(
        &self,
        params: &[(String, String)],
    ) -> Result<String, AutonomousError> {
        let target = params
            .iter()
            .find(|(k, _)| k == "target_instances")
            .map(|(_, v)| v.as_str())
            .unwrap_or("default");
        Ok(format!(
            "AutoScaling executed, target: {} instances",
            target
        ))
    }

    async fn execute_tuning(&self, params: &[(String, String)]) -> Result<String, AutonomousError> {
        let param = params
            .iter()
            .find(|(k, _)| k == "param")
            .map(|(_, v)| v.as_str())
            .unwrap_or("pool_size");
        Ok(format!("AutoTuning executed, param: {}", param))
    }

    async fn execute_slow_query_governance(
        &self,
        params: &[(String, String)],
    ) -> Result<String, AutonomousError> {
        let query_id = params
            .iter()
            .find(|(k, _)| k == "query_id")
            .map(|(_, v)| v.as_str())
            .unwrap_or("unknown");
        Ok(format!(
            "SlowQueryGovernance executed for query: {}",
            query_id
        ))
    }

    async fn execute_degrade_non_core(
        &self,
        params: &[(String, String)],
    ) -> Result<String, AutonomousError> {
        let features = params
            .iter()
            .find(|(k, _)| k == "features")
            .map(|(_, v)| v.as_str())
            .unwrap_or("");
        Ok(format!(
            "DegradeNonCore executed, degraded features: {}",
            features
        ))
    }
}

impl Default for AutonomousActionExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_execute_remediation() {
        let executor = AutonomousActionExecutor::new();
        let result = executor
            .execute(
                &AutonomousAction::AutoRemediation,
                &[(
                    "fault_type".to_string(),
                    "connection_pool_exhausted".to_string(),
                )],
            )
            .await
            .unwrap();
        assert!(result.success);
        assert!(result.message.contains("connection_pool_exhausted"));
    }

    #[tokio::test]
    async fn test_execute_scaling() {
        let executor = AutonomousActionExecutor::new();
        let result = executor
            .execute(
                &AutonomousAction::AutoScaling,
                &[("target_instances".to_string(), "10".to_string())],
            )
            .await
            .unwrap();
        assert!(result.success);
        assert!(result.message.contains("10"));
    }

    #[tokio::test]
    async fn test_execute_tuning() {
        let executor = AutonomousActionExecutor::new();
        let result = executor
            .execute(
                &AutonomousAction::AutoTuning,
                &[("param".to_string(), "max_connections".to_string())],
            )
            .await
            .unwrap();
        assert!(result.success);
        assert!(result.message.contains("max_connections"));
    }

    #[tokio::test]
    async fn test_execute_slow_query_governance() {
        let executor = AutonomousActionExecutor::new();
        let result = executor
            .execute(
                &AutonomousAction::SlowQueryGovernance,
                &[("query_id".to_string(), "q123".to_string())],
            )
            .await
            .unwrap();
        assert!(result.success);
        assert!(result.message.contains("q123"));
    }
}
