//! 异常检测错误类型

use thiserror::Error;

/// 异常检测错误
#[derive(Debug, Clone, PartialEq, Error)]
pub enum AnomalyErrorKind {
    /// 配置非法
    #[error("invalid config: {0}")]
    InvalidConfig(String),
    /// 指标采集失败
    #[error("metric collection failed: {0}")]
    CollectionFailed(String),
    /// 检测失败
    #[error("detection failed: {0}")]
    DetectionFailed(String),
    /// 报告导出失败
    #[error("report export failed: {0}")]
    ExportFailed(String),
    /// 订阅失败
    #[error("subscription failed: {0}")]
    SubscriptionFailed(String),
}

#[cfg(feature = "anomaly-detection")]
#[derive(Debug, thiserror::Error)]
pub enum AnomalyError {
    #[error(transparent)]
    Kind(#[from] AnomalyErrorKind),

    #[error("json serialization failed: {0}")]
    Json(#[from] serde_json::Error),
}
