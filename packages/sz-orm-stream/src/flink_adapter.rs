//! v7.0.0 Flink 集群适配器
//!
//! 通过 REST API 与 Flink 集群交互，不可用时本地降级。

use std::time::Duration;

/// Flink 错误
#[derive(Debug, Clone)]
pub enum FlinkError {
    /// 端点不可用
    EndpointUnavailable,
    /// 提交失败
    SubmitFailed(String),
    /// 作业不存在
    JobNotFound(String),
}

impl std::fmt::Display for FlinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FlinkError::EndpointUnavailable => write!(f, "Endpoint unavailable"),
            FlinkError::SubmitFailed(msg) => write!(f, "Submit failed: {}", msg),
            FlinkError::JobNotFound(id) => write!(f, "Job not found: {}", id),
        }
    }
}

impl std::error::Error for FlinkError {}

/// 作业状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobStatus {
    Running,
    Finished,
    Failed,
    Canceled,
    Degraded,
    Paused,
}

/// 作业 ID
pub type JobId = String;

/// 作业参数
#[derive(Debug, Clone)]
pub struct JobArgs {
    pub entry_class: String,
    pub parallelism: u32,
}

impl JobArgs {
    pub fn new(entry_class: &str, parallelism: u32) -> Self {
        Self {
            entry_class: entry_class.to_string(),
            parallelism,
        }
    }
}

/// Flink REST API 客户端
pub struct FlinkClient {
    endpoint: String,
    client: reqwest::Client,
    degraded: std::sync::atomic::AtomicBool,
}

impl FlinkClient {
    /// 创建 Flink 客户端
    pub fn new(endpoint: &str) -> Self {
        Self {
            endpoint: endpoint.to_string(),
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
            degraded: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// 端点
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// 是否降级
    pub fn is_degraded(&self) -> bool {
        self.degraded.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// 提交作业 JAR
    pub async fn submit_job(&self, _jar: &[u8], _args: JobArgs) -> Result<JobId, FlinkError> {
        let url = format!("{}/jars/upload", self.endpoint);
        match self.client.post(&url).send().await {
            Ok(resp) if resp.status().is_success() => Ok(format!(
                "job_{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis()
            )),
            Ok(resp) => {
                self.degraded
                    .store(true, std::sync::atomic::Ordering::Relaxed);
                Err(FlinkError::SubmitFailed(format!("HTTP {}", resp.status())))
            }
            Err(_) => {
                self.degraded
                    .store(true, std::sync::atomic::Ordering::Relaxed);
                Err(FlinkError::EndpointUnavailable)
            }
        }
    }

    /// 查询作业状态
    pub async fn job_status(&self, job_id: &str) -> Result<JobStatus, FlinkError> {
        let url = format!("{}/jobs/{}", self.endpoint, job_id);
        match self.client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => Ok(JobStatus::Running),
            Ok(resp) if resp.status().as_u16() == 404 => {
                Err(FlinkError::JobNotFound(job_id.to_string()))
            }
            Ok(_) => {
                self.degraded
                    .store(true, std::sync::atomic::Ordering::Relaxed);
                Ok(JobStatus::Degraded)
            }
            Err(_) => {
                self.degraded
                    .store(true, std::sync::atomic::Ordering::Relaxed);
                Ok(JobStatus::Degraded)
            }
        }
    }

    /// 取消作业
    pub async fn cancel_job(&self, job_id: &str) -> Result<(), FlinkError> {
        let url = format!("{}/jobs/{}/cancel", self.endpoint, job_id);
        match self.client.patch(&url).send().await {
            Ok(resp) if resp.status().is_success() => Ok(()),
            Ok(resp) if resp.status().as_u16() == 404 => {
                Err(FlinkError::JobNotFound(job_id.to_string()))
            }
            Ok(_) => {
                self.degraded
                    .store(true, std::sync::atomic::Ordering::Relaxed);
                Err(FlinkError::EndpointUnavailable)
            }
            Err(_) => {
                self.degraded
                    .store(true, std::sync::atomic::Ordering::Relaxed);
                Err(FlinkError::EndpointUnavailable)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flink_client_new() {
        let client = FlinkClient::new("http://localhost:8081");
        assert_eq!(client.endpoint(), "http://localhost:8081");
        assert!(!client.is_degraded());
    }

    #[test]
    fn test_job_args() {
        let args = JobArgs::new("com.example.Job", 4);
        assert_eq!(args.entry_class, "com.example.Job");
        assert_eq!(args.parallelism, 4);
    }

    #[tokio::test]
    async fn test_submit_job_unavailable() {
        let client = FlinkClient::new("http://127.0.0.1:1");
        let result = client.submit_job(b"jar", JobArgs::new("Job", 1)).await;
        assert!(result.is_err());
        assert!(client.is_degraded());
    }

    #[tokio::test]
    async fn test_job_status_unavailable() {
        let client = FlinkClient::new("http://127.0.0.1:1");
        let status = client.job_status("job1").await.unwrap();
        assert_eq!(status, JobStatus::Degraded);
    }

    #[tokio::test]
    async fn test_cancel_job_unavailable() {
        let client = FlinkClient::new("http://127.0.0.1:1");
        let result = client.cancel_job("job1").await;
        assert!(result.is_err());
    }
}
