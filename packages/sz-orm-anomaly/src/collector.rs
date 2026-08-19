//! 指标采集模块：慢查询/错误率/连接池指标 + SQL 摘要脱敏
//!
//! 异步/旁路采集（不阻塞主路径）：`record_*` 方法仅写入滑动窗口，O(1) 操作。
//! SQL 摘要脱敏复用 `sz-orm-masking::DataMasker`，将参数值替换为占位符。

use serde::{Deserialize, Serialize};

use crate::window::SlidingWindow;

/// 指标类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MetricType {
    /// 慢查询指标
    SlowQuery,
    /// 错误率指标
    Error,
    /// 连接池指标
    PoolUsage,
}

/// 错误类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ErrorType {
    /// 连接错误
    ConnectionError,
    /// SQL 错误
    SqlError,
    /// 超时错误
    TimeoutError,
}

impl ErrorType {
    /// 转为字符串标识
    pub fn as_str(&self) -> &'static str {
        match self {
            ErrorType::ConnectionError => "connection_error",
            ErrorType::SqlError => "sql_error",
            ErrorType::TimeoutError => "timeout_error",
        }
    }
}

/// 慢查询指标
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlowQueryMetric {
    /// 时间戳（毫秒，UNIX_EPOCH 起）
    pub timestamp: u64,
    /// 查询耗时（毫秒）
    pub elapsed_ms: u64,
    /// SQL 摘要（已脱敏）
    pub sql_summary: String,
}

impl SlowQueryMetric {
    /// 估算内存占用（字节）
    pub fn estimated_size(&self) -> usize {
        std::mem::size_of::<u64>() * 2 + self.sql_summary.len() + std::mem::size_of::<String>()
    }
}

/// 错误指标
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorMetric {
    /// 时间戳（毫秒）
    pub timestamp: u64,
    /// 错误类型
    pub error_type: ErrorType,
}

impl ErrorMetric {
    /// 估算内存占用（字节）
    pub fn estimated_size(&self) -> usize {
        std::mem::size_of::<u64>() + std::mem::size_of::<ErrorType>()
    }
}

/// 连接池指标
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolMetric {
    /// 时间戳（毫秒）
    pub timestamp: u64,
    /// 活跃连接数
    pub active: u32,
    /// 空闲连接数
    pub idle: u32,
    /// 等待线程数
    pub waiting: u32,
    /// 获取连接耗时（毫秒）
    pub acquire_ms: u64,
}

impl PoolMetric {
    /// 估算内存占用（字节）
    pub fn estimated_size(&self) -> usize {
        std::mem::size_of::<u64>() * 2 + std::mem::size_of::<u32>() * 3
    }

    /// 连接池使用率（0.0 ~ 1.0）
    pub fn usage_rate(&self) -> f64 {
        let total = self.active + self.idle;
        if total == 0 {
            0.0
        } else {
            self.active as f64 / total as f64
        }
    }
}

/// SQL 摘要脱敏：将参数值替换为占位符 `?`
///
/// 例：`SELECT * FROM users WHERE password='secret' AND id=123` → `SELECT * FROM users WHERE password=? AND id=?`
pub fn mask_sql_summary(sql: &str) -> String {
    let mut result = String::with_capacity(sql.len());
    let chars: Vec<char> = sql.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\'' || c == '"' {
            // 字符串字面值：跳到匹配的引号，替换为 ?
            let quote = c;
            i += 1;
            while i < chars.len() {
                if chars[i] == quote {
                    // 检查是否是转义引号（连续两个引号）
                    if i + 1 < chars.len() && chars[i + 1] == quote {
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                i += 1;
            }
            result.push('?');
        } else if c.is_ascii_digit()
            && (i == 0
                || (!chars[i - 1].is_ascii_alphanumeric()
                    && chars[i - 1] != '_'
                    && chars[i - 1] != '.'))
        {
            // 数字字面值：跳过连续数字
            let mut j = i;
            while j < chars.len() && (chars[j].is_ascii_digit() || chars[j] == '.') {
                j += 1;
            }
            let prev_non_space = if i > 0 { chars[i - 1] } else { ' ' };
            if prev_non_space == '.' {
                // 表名限定（如 table_123 或 t.1），不替换
                result.extend(chars[i..j].iter());
            } else {
                result.push('?');
            }
            i = j;
        } else {
            result.push(c);
            i += 1;
        }
    }
    result
}

/// 指标采集器
///
/// 采集三类指标（慢查询/错误率/连接池），写入滑动窗口。
/// 线程安全：内部使用 `Arc<SlidingWindow>`，可跨线程共享。
#[derive(Clone)]
pub struct MetricCollector {
    window: std::sync::Arc<SlidingWindow>,
}

impl MetricCollector {
    /// 创建指标采集器
    pub fn new(window: std::sync::Arc<SlidingWindow>) -> Self {
        Self { window }
    }

    /// 记录慢查询指标（REQ-ANM-001）
    ///
    /// - `elapsed_ms`：查询耗时（毫秒）
    /// - `sql_summary`：SQL 摘要（将自动脱敏）
    /// - `timestamp`：时间戳（毫秒，0 表示当前时间）
    pub fn record_slow_query(&self, elapsed_ms: u64, sql_summary: &str, timestamp: u64) {
        let ts = if timestamp == 0 {
            crate::window::current_timestamp_ms()
        } else {
            timestamp
        };
        let masked = mask_sql_summary(sql_summary);
        self.window.push_slow_query(SlowQueryMetric {
            timestamp: ts,
            elapsed_ms,
            sql_summary: masked,
        });
    }

    /// 记录查询错误指标（REQ-ANM-002）
    ///
    /// - `error_type`：错误类型
    /// - `timestamp`：时间戳（毫秒，0 表示当前时间）
    pub fn record_error(&self, error_type: ErrorType, timestamp: u64) {
        let ts = if timestamp == 0 {
            crate::window::current_timestamp_ms()
        } else {
            timestamp
        };
        self.window.push_error(ErrorMetric {
            timestamp: ts,
            error_type,
        });
    }

    /// 记录连接池指标（REQ-ANM-003）
    ///
    /// - `active`：活跃连接数
    /// - `idle`：空闲连接数
    /// - `waiting`：等待线程数
    /// - `acquire_ms`：获取连接耗时（毫秒）
    /// - `timestamp`：时间戳（毫秒，0 表示当前时间）
    pub fn record_pool_usage(
        &self,
        active: u32,
        idle: u32,
        waiting: u32,
        acquire_ms: u64,
        timestamp: u64,
    ) {
        let ts = if timestamp == 0 {
            crate::window::current_timestamp_ms()
        } else {
            timestamp
        };
        self.window.push_pool(PoolMetric {
            timestamp: ts,
            active,
            idle,
            waiting,
            acquire_ms,
        });
    }

    /// 获取慢查询指标快照
    pub fn slow_query_metrics(&self) -> Vec<SlowQueryMetric> {
        self.window.slow_query_metrics()
    }

    /// 获取错误指标快照
    pub fn error_metrics(&self) -> Vec<ErrorMetric> {
        self.window.error_metrics()
    }

    /// 获取连接池指标快照
    pub fn pool_metrics(&self) -> Vec<PoolMetric> {
        self.window.pool_metrics()
    }

    /// 慢查询指标数量
    pub fn slow_query_count(&self) -> usize {
        self.window.slow_query_count()
    }

    /// 错误指标数量
    pub fn error_count(&self) -> usize {
        self.window.error_count()
    }

    /// 连接池指标数量
    pub fn pool_count(&self) -> usize {
        self.window.pool_count()
    }

    /// 累计丢弃数
    pub fn dropped_count(&self) -> u64 {
        self.window.dropped_count()
    }

    /// 累计写入数
    pub fn written_count(&self) -> u64 {
        self.window.written_count()
    }

    /// 估算内存占用（字节）
    pub fn estimated_memory_bytes(&self) -> usize {
        self.window.estimated_memory_bytes()
    }

    /// 清空所有指标
    pub fn clear(&self) {
        self.window.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn new_collector() -> MetricCollector {
        MetricCollector::new(std::sync::Arc::new(SlidingWindow::new(
            Duration::from_secs(60),
        )))
    }

    #[test]
    fn test_record_slow_query() {
        let collector = new_collector();
        let ts = crate::window::current_timestamp_ms();
        collector.record_slow_query(150, "SELECT * FROM users WHERE id = 1", ts);
        let metrics = collector.slow_query_metrics();
        assert_eq!(metrics.len(), 1);
        assert_eq!(metrics[0].elapsed_ms, 150);
        assert_eq!(metrics[0].timestamp, ts);
        // SQL 摘要应被脱敏
        assert!(metrics[0].sql_summary.contains('?'));
        assert!(!metrics[0].sql_summary.contains("= 1"));
    }

    #[test]
    fn test_record_error() {
        let collector = new_collector();
        let ts = crate::window::current_timestamp_ms();
        collector.record_error(ErrorType::SqlError, ts);
        collector.record_error(ErrorType::TimeoutError, ts);
        collector.record_error(ErrorType::ConnectionError, ts);
        let metrics = collector.error_metrics();
        assert_eq!(metrics.len(), 3);
        assert_eq!(metrics[0].error_type, ErrorType::SqlError);
        assert_eq!(metrics[1].error_type, ErrorType::TimeoutError);
        assert_eq!(metrics[2].error_type, ErrorType::ConnectionError);
    }

    #[test]
    fn test_record_pool_usage() {
        let collector = new_collector();
        let ts = crate::window::current_timestamp_ms();
        collector.record_pool_usage(48, 2, 15, 1200, ts);
        let metrics = collector.pool_metrics();
        assert_eq!(metrics.len(), 1);
        assert_eq!(metrics[0].active, 48);
        assert_eq!(metrics[0].idle, 2);
        assert_eq!(metrics[0].waiting, 15);
        assert_eq!(metrics[0].acquire_ms, 1200);
    }

    #[test]
    fn test_sql_masking_string_literal() {
        let masked = mask_sql_summary("SELECT * FROM users WHERE password='secret'");
        assert!(masked.contains("password=?"));
        assert!(!masked.contains("secret"));
    }

    #[test]
    fn test_sql_masking_number() {
        let masked = mask_sql_summary("SELECT * FROM users WHERE id = 123");
        assert!(masked.contains("id = ?"));
        assert!(!masked.contains("123"));
    }

    #[test]
    fn test_sql_masking_multiple_params() {
        let masked =
            mask_sql_summary("SELECT * FROM users WHERE name='admin' AND password='pwd' AND id=42");
        assert_eq!(
            masked,
            "SELECT * FROM users WHERE name=? AND password=? AND id=?"
        );
    }

    #[test]
    fn test_sql_masking_preserves_structure() {
        let masked = mask_sql_summary("SELECT id, name FROM users WHERE id = 1");
        assert!(masked.starts_with("SELECT id, name FROM users WHERE id = ?"));
    }

    #[test]
    fn test_sql_masking_table_qualified() {
        let masked = mask_sql_summary("SELECT * FROM users.table_123 WHERE id = 1");
        assert!(masked.contains("users.table_123"));
        assert!(masked.contains("id = ?"));
    }

    #[test]
    fn test_sql_masking_no_params() {
        let masked = mask_sql_summary("SELECT * FROM users");
        assert_eq!(masked, "SELECT * FROM users");
    }

    #[test]
    fn test_sql_masking_empty() {
        assert_eq!(mask_sql_summary(""), "");
    }

    #[test]
    fn test_error_type_as_str() {
        assert_eq!(ErrorType::ConnectionError.as_str(), "connection_error");
        assert_eq!(ErrorType::SqlError.as_str(), "sql_error");
        assert_eq!(ErrorType::TimeoutError.as_str(), "timeout_error");
    }

    #[test]
    fn test_pool_usage_rate() {
        let metric = PoolMetric {
            timestamp: 0,
            active: 8,
            idle: 2,
            waiting: 0,
            acquire_ms: 50,
        };
        assert!((metric.usage_rate() - 0.8).abs() < 1e-9);
    }

    #[test]
    fn test_pool_usage_rate_zero() {
        let metric = PoolMetric {
            timestamp: 0,
            active: 0,
            idle: 0,
            waiting: 0,
            acquire_ms: 0,
        };
        assert_eq!(metric.usage_rate(), 0.0);
    }

    #[test]
    fn test_collector_clear() {
        let collector = new_collector();
        let ts = crate::window::current_timestamp_ms();
        collector.record_slow_query(100, "SELECT 1", ts);
        assert_eq!(collector.slow_query_count(), 1);
        collector.clear();
        assert_eq!(collector.slow_query_count(), 0);
    }
}
