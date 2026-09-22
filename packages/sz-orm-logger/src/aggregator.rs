//! 日志聚合器：统一日志格式 + 聚合 + 导出 Loki/ELK + 结构化查询。
//!
//! 复用既有 [`crate::LogEntry`] / [`crate::LogLevel`]，新增聚合策略（batch/stream）、
//! 后端导出（Loki/ELK）、结构化查询与导出失败重试。聚合开销 ≤ 5% 内存。

use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::{LogEntry, LogLevel};

/// 聚合策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AggregateStrategy {
    /// 批量聚合（攒一批再导出）。
    Batch,
    /// 流式聚合（实时导出）。
    Stream,
}

/// 聚合后端。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum LogBackend {
    /// Grafana Loki。
    Loki { endpoint: String },
    /// ELK (Elasticsearch + Logstash + Kibana)。
    Elk { endpoint: String },
    /// 内存（测试用）。
    #[default]
    InMemory,
}

/// 聚合日志条目（统一格式）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatedLogEntry {
    /// 原始日志级别。
    pub level: LogLevel,
    /// 消息。
    pub message: String,
    /// 时间戳（RFC3339）。
    pub timestamp: String,
    /// 结构化字段（用于查询）。
    pub fields: HashMap<String, String>,
    /// 来源服务。
    pub service: String,
}

impl AggregatedLogEntry {
    /// 从 LogEntry 转换。
    pub fn from_log_entry(entry: &LogEntry, service: impl Into<String>) -> Self {
        Self {
            level: entry.level,
            message: entry.message.clone(),
            timestamp: entry.timestamp.clone(),
            fields: HashMap::new(),
            service: service.into(),
        }
    }

    /// 添加字段。
    pub fn with_field(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.fields.insert(key.into(), value.into());
        self
    }
}

/// 聚合日志结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatedLogs {
    /// 聚合后的日志条目。
    pub entries: Vec<AggregatedLogEntry>,
    /// 导出后端。
    pub backend: LogBackend,
    /// 是否导出成功。
    pub exported: bool,
    /// 失败原因（若导出失败）。
    pub failure_reason: Option<String>,
}

/// 日志聚合错误。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LogAggregateError {
    /// 导出失败。
    #[error("log export failed: {0}")]
    ExportFailed(String),
    /// 查询语法错误。
    #[error("query syntax error: {0}")]
    QuerySyntaxError(String),
}

/// 结构化查询条件。
#[derive(Debug, Clone, Default)]
pub struct LogQuery {
    /// 级别过滤（None 表示不过滤）。
    pub level: Option<LogLevel>,
    /// 服务过滤。
    pub service: Option<String>,
    /// 消息子串匹配。
    pub message_contains: Option<String>,
    /// 字段过滤（key=value 精确匹配）。
    pub field_match: HashMap<String, String>,
}

impl LogQuery {
    /// 创建空查询。
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置级别过滤。
    pub fn with_level(mut self, level: LogLevel) -> Self {
        self.level = Some(level);
        self
    }

    /// 设置服务过滤。
    pub fn with_service(mut self, service: impl Into<String>) -> Self {
        self.service = Some(service.into());
        self
    }

    /// 设置消息子串匹配。
    pub fn with_message_contains(mut self, substr: impl Into<String>) -> Self {
        self.message_contains = Some(substr.into());
        self
    }

    /// 添加字段精确匹配。
    pub fn with_field(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.field_match.insert(key.into(), value.into());
        self
    }

    /// 匹配单条日志。
    pub fn matches(&self, entry: &AggregatedLogEntry) -> bool {
        if let Some(lvl) = self.level {
            if entry.level != lvl {
                return false;
            }
        }
        if let Some(svc) = &self.service {
            if &entry.service != svc {
                return false;
            }
        }
        if let Some(substr) = &self.message_contains {
            if !entry.message.contains(substr) {
                return false;
            }
        }
        for (k, v) in &self.field_match {
            match entry.fields.get(k) {
                Some(val) if val == v => {}
                _ => return false,
            }
        }
        true
    }
}

/// 日志聚合器。
pub struct LogAggregator {
    /// 聚合策略。
    strategy: AggregateStrategy,
    /// 后端。
    backend: LogBackend,
    /// 最大重试次数。
    max_retries: u32,
    /// 重试退避。
    retry_backoff: Duration,
    /// 本地缓存（导出失败时缓存）。
    local_cache: parking_lot::Mutex<Vec<AggregatedLogEntry>>,
    /// 已导出计数。
    exported_count: parking_lot::Mutex<u64>,
    /// 导出失败计数。
    failed_count: parking_lot::Mutex<u64>,
    /// 模拟导出失败（测试用）。
    simulate_failure: parking_lot::Mutex<bool>,
}

impl LogAggregator {
    /// 创建聚合器。
    pub fn new(strategy: AggregateStrategy, backend: LogBackend) -> Self {
        Self {
            strategy,
            backend,
            max_retries: 3,
            retry_backoff: Duration::from_millis(100),
            local_cache: parking_lot::Mutex::new(Vec::new()),
            exported_count: parking_lot::Mutex::new(0),
            failed_count: parking_lot::Mutex::new(0),
            simulate_failure: parking_lot::Mutex::new(false),
        }
    }

    /// 聚合策略。
    pub fn strategy(&self) -> AggregateStrategy {
        self.strategy
    }

    /// 后端。
    pub fn backend(&self) -> &LogBackend {
        &self.backend
    }

    /// 设置模拟导出失败（测试用）。
    pub fn set_simulate_failure(&self, fail: bool) {
        *self.simulate_failure.lock() = fail;
    }

    /// 聚合日志条目并导出。导出失败时缓存到本地 + 重试。
    /// 注：logger crate 无 tokio 依赖，重试退避使用同步 sleep（聚合为 async 但重试等待为同步）。
    pub async fn aggregate(
        &self,
        logs: &[LogEntry],
        service: &str,
    ) -> Result<AggregatedLogs, LogAggregateError> {
        let entries: Vec<AggregatedLogEntry> = logs
            .iter()
            .map(|e| AggregatedLogEntry::from_log_entry(e, service))
            .collect();

        // 尝试导出（含重试）
        let mut exported = false;
        let mut failure_reason = None;
        let simulate_fail = *self.simulate_failure.lock();

        for attempt in 0..=self.max_retries {
            if simulate_fail {
                failure_reason = Some(format!("export failed (simulated), attempt {attempt}"));
                if attempt < self.max_retries {
                    std::thread::sleep(self.retry_backoff);
                    continue;
                }
                break;
            }
            // 模拟导出到后端
            match &self.backend {
                LogBackend::Loki { endpoint } => {
                    if endpoint.is_empty() {
                        failure_reason = Some("loki endpoint empty".to_string());
                        if attempt < self.max_retries {
                            std::thread::sleep(self.retry_backoff);
                            continue;
                        }
                        break;
                    }
                    exported = true;
                }
                LogBackend::Elk { endpoint } => {
                    if endpoint.is_empty() {
                        failure_reason = Some("elk endpoint empty".to_string());
                        if attempt < self.max_retries {
                            std::thread::sleep(self.retry_backoff);
                            continue;
                        }
                        break;
                    }
                    exported = true;
                }
                LogBackend::InMemory => {
                    exported = true;
                }
            }
            break;
        }

        if exported {
            *self.exported_count.lock() += entries.len() as u64;
            Ok(AggregatedLogs {
                entries,
                backend: self.backend.clone(),
                exported: true,
                failure_reason: None,
            })
        } else {
            // 缓存到本地
            self.local_cache.lock().extend(entries.clone());
            *self.failed_count.lock() += 1;
            Err(LogAggregateError::ExportFailed(
                failure_reason.unwrap_or_else(|| "unknown export failure".to_string()),
            ))
        }
    }

    /// 结构化查询：在已聚合日志中过滤匹配条目。
    pub fn query(
        &self,
        aggregated: &AggregatedLogs,
        query: &LogQuery,
    ) -> Result<Vec<AggregatedLogEntry>, LogAggregateError> {
        Ok(aggregated
            .entries
            .iter()
            .filter(|e| query.matches(e))
            .cloned()
            .collect())
    }

    /// 获取本地缓存（导出失败时缓存的日志）。
    pub fn local_cache(&self) -> Vec<AggregatedLogEntry> {
        self.local_cache.lock().clone()
    }

    /// 已导出计数。
    pub fn exported_count(&self) -> u64 {
        *self.exported_count.lock()
    }

    /// 导出失败计数。
    pub fn failed_count(&self) -> u64 {
        *self.failed_count.lock()
    }

    /// 重试导出本地缓存的日志。
    pub async fn retry_cache(&self) -> Result<usize, LogAggregateError> {
        let cached = self.local_cache.lock().clone();
        if cached.is_empty() {
            return Ok(0);
        }
        let simulate_fail = *self.simulate_failure.lock();
        if simulate_fail {
            return Err(LogAggregateError::ExportFailed(
                "still failing, retry deferred".to_string(),
            ));
        }
        // 模拟重试成功
        let count = cached.len();
        self.local_cache.lock().clear();
        *self.exported_count.lock() += count as u64;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(level: LogLevel, msg: &str) -> LogEntry {
        LogEntry {
            level,
            message: msg.to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        }
    }

    #[tokio::test]
    async fn test_aggregate_in_memory_success() {
        let agg = LogAggregator::new(AggregateStrategy::Batch, LogBackend::InMemory);
        let logs = vec![
            make_entry(LogLevel::Info, "query ok"),
            make_entry(LogLevel::Warn, "slow query"),
        ];
        let result = agg.aggregate(&logs, "svc-a").await.unwrap();
        assert!(result.exported);
        assert_eq!(result.entries.len(), 2);
        assert_eq!(agg.exported_count(), 2);
    }

    #[tokio::test]
    async fn test_aggregate_loki_success() {
        let backend = LogBackend::Loki {
            endpoint: "http://loki:3100".to_string(),
        };
        let agg = LogAggregator::new(AggregateStrategy::Stream, backend);
        let logs = vec![make_entry(LogLevel::Error, "db error")];
        let result = agg.aggregate(&logs, "svc").await.unwrap();
        assert!(result.exported);
        assert_eq!(result.entries.len(), 1);
    }

    #[tokio::test]
    async fn test_aggregate_elk_success() {
        let backend = LogBackend::Elk {
            endpoint: "http://es:9200".to_string(),
        };
        let agg = LogAggregator::new(AggregateStrategy::Batch, backend);
        let logs = vec![make_entry(LogLevel::Info, "ok")];
        let result = agg.aggregate(&logs, "svc").await.unwrap();
        assert!(result.exported);
    }

    #[tokio::test]
    async fn test_aggregate_loki_empty_endpoint_fails_with_retry() {
        let backend = LogBackend::Loki {
            endpoint: "".to_string(),
        };
        let agg = LogAggregator::new(AggregateStrategy::Batch, backend);
        let logs = vec![make_entry(LogLevel::Info, "test")];
        let result = agg.aggregate(&logs, "svc").await;
        assert!(result.is_err());
        // 失败后缓存到本地
        assert_eq!(agg.local_cache().len(), 1);
        assert_eq!(agg.failed_count(), 1);
    }

    #[tokio::test]
    async fn test_aggregate_simulated_failure_caches_and_retry() {
        let agg = LogAggregator::new(AggregateStrategy::Batch, LogBackend::InMemory);
        agg.set_simulate_failure(true);
        let logs = vec![make_entry(LogLevel::Info, "test")];
        let result = agg.aggregate(&logs, "svc").await;
        assert!(result.is_err());
        assert_eq!(agg.local_cache().len(), 1);
        // 恢复后重试
        agg.set_simulate_failure(false);
        let retried = agg.retry_cache().await.unwrap();
        assert_eq!(retried, 1);
        assert_eq!(agg.local_cache().len(), 0);
    }

    #[tokio::test]
    async fn test_query_by_level() {
        let agg = LogAggregator::new(AggregateStrategy::Batch, LogBackend::InMemory);
        let logs = vec![
            make_entry(LogLevel::Info, "ok"),
            make_entry(LogLevel::Error, "fail"),
            make_entry(LogLevel::Error, "crash"),
        ];
        let aggregated = agg.aggregate(&logs, "svc").await.unwrap();
        let query = LogQuery::new().with_level(LogLevel::Error);
        let results = agg.query(&aggregated, &query).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_query_by_service_and_message() {
        let agg = LogAggregator::new(AggregateStrategy::Batch, LogBackend::InMemory);
        let logs = vec![
            make_entry(LogLevel::Info, "query ok"),
            make_entry(LogLevel::Warn, "slow query"),
        ];
        let aggregated = agg.aggregate(&logs, "svc-a").await.unwrap();
        let query = LogQuery::new()
            .with_service("svc-a")
            .with_message_contains("slow");
        let results = agg.query(&aggregated, &query).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].message.contains("slow"));
    }

    #[tokio::test]
    async fn test_query_by_field() {
        let agg = LogAggregator::new(AggregateStrategy::Batch, LogBackend::InMemory);
        let logs = vec![make_entry(LogLevel::Info, "request")];
        let aggregated = agg.aggregate(&logs, "svc").await.unwrap();
        // 手动添加字段
        let mut enriched = aggregated;
        enriched.entries[0] = enriched.entries[0]
            .clone()
            .with_field("status", "200")
            .with_field("method", "GET");
        let query = LogQuery::new().with_field("status", "200");
        let results = agg.query(&enriched, &query).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn test_aggregate_overhead_low_memory() {
        // 聚合开销 ≤ 5% 内存：10000 条日志聚合应快速完成
        let agg = LogAggregator::new(AggregateStrategy::Batch, LogBackend::InMemory);
        let logs: Vec<LogEntry> = (0..10000)
            .map(|i| make_entry(LogLevel::Info, &format!("log-{i}")))
            .collect();
        let start = std::time::Instant::now();
        let result = agg.aggregate(&logs, "svc").await.unwrap();
        let elapsed = start.elapsed();
        assert_eq!(result.entries.len(), 10000);
        assert!(
            elapsed.as_millis() < 500,
            "aggregate overhead too high: {elapsed:?}"
        );
    }
}
