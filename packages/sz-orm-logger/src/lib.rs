//! # SZ-ORM Logger — 结构化日志
//!
//! 提供多级别（Debug/Info/Warn/Error）、多输出目标的日志记录，支持异步写入与
//! 结构化字段，可组合多个 Logger 实现输出到不同后端。
//!
//! ## 主要类型
//!
//! - [`Logger`] trait — 日志器接口
//! - [`LogLevel`] — 日志级别
//! - [`LogEntry`] — 日志条目
//!
//! ## 高级日志功能（`advanced` 模块）
//!
//! - [`advanced::LogRotator`] — 日志轮转（按大小/时间）
//! - [`advanced::MultiOutputLogger`] / [`advanced::LogSink`] — 多输出扇出
//! - [`advanced::LevelFilter`] — 按 target 细粒度级别过滤
//! - [`advanced::StructuredLogEntry`] / [`advanced::StructuredLogWriter`] — 结构化字段

pub mod advanced;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Debug => "DEBUG",
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
        }
    }
}

pub trait Logger: Send + Sync {
    fn log(&self, level: LogLevel, msg: &str);
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub level: LogLevel,
    pub message: String,
    pub timestamp: String,
}

pub struct StructuredLogger {
    level: LogLevel,
    entries: Arc<Mutex<Vec<LogEntry>>>,
}

impl StructuredLogger {
    pub fn new() -> Self {
        Self::with_level(LogLevel::Info)
    }

    pub fn with_level(level: LogLevel) -> Self {
        Self {
            level,
            entries: Arc::new(Mutex::new(vec![])),
        }
    }

    /// Convenience method equivalent to `log(LogLevel::Info, msg)`.
    pub fn output(&self, msg: &str) {
        self.log(LogLevel::Info, msg);
    }

    /// Return a snapshot of all log entries that passed the level filter.
    pub fn entries(&self) -> Vec<LogEntry> {
        let entries = self.entries.lock().unwrap();
        entries.iter().cloned().collect()
    }

    pub fn level(&self) -> LogLevel {
        self.level
    }

    /// Internal shared handle so multiple loggers can write to the same sink.
    pub fn shared_handle(&self) -> Arc<Mutex<Vec<LogEntry>>> {
        Arc::clone(&self.entries)
    }
}

impl Default for StructuredLogger {
    fn default() -> Self {
        Self::new()
    }
}

impl Logger for StructuredLogger {
    fn log(&self, level: LogLevel, msg: &str) {
        // Filter: anything strictly below the configured level is dropped.
        if level < self.level {
            return;
        }
        let timestamp = chrono::Utc::now().to_rfc3339();
        let entry = LogEntry {
            level,
            message: msg.to_string(),
            timestamp: timestamp.clone(),
        };
        {
            let mut entries = self.entries.lock().unwrap();
            entries.push(entry);
        }
        // Also emit to stdout for runtime observability, with level + timestamp.
        println!("[{}] {} - {}", level.as_str(), timestamp, msg);
    }
}

/// Factory that creates loggers with different configurations.
pub struct LoggerFactory;

impl LoggerFactory {
    pub fn new() -> Self {
        Self
    }

    pub fn create(&self, level: LogLevel) -> StructuredLogger {
        StructuredLogger::with_level(level)
    }

    pub fn debug(&self) -> StructuredLogger {
        self.create(LogLevel::Debug)
    }

    pub fn info(&self) -> StructuredLogger {
        self.create(LogLevel::Info)
    }

    pub fn warn(&self) -> StructuredLogger {
        self.create(LogLevel::Warn)
    }

    pub fn error(&self) -> StructuredLogger {
        self.create(LogLevel::Error)
    }
}

impl Default for LoggerFactory {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    pub counters: HashMap<String, u64>,
    pub gauges: HashMap<String, f64>,
}

pub struct Metrics {
    counters: HashMap<String, u64>,
    gauges: HashMap<String, f64>,
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            counters: HashMap::new(),
            gauges: HashMap::new(),
        }
    }

    pub fn increment_counter(&mut self, name: &str) {
        *self.counters.entry(name.to_string()).or_insert(0) += 1;
    }

    pub fn add_counter(&mut self, name: &str, value: u64) {
        *self.counters.entry(name.to_string()).or_insert(0) += value;
    }

    pub fn set_gauge(&mut self, name: &str, value: f64) {
        self.gauges.insert(name.to_string(), value);
    }

    pub fn get_counter(&self, name: &str) -> Option<u64> {
        self.counters.get(name).copied()
    }

    pub fn get_gauge(&self, name: &str) -> Option<f64> {
        self.gauges.get(name).copied()
    }

    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            counters: self.counters.clone(),
            gauges: self.gauges.clone(),
        }
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_level_ordering() {
        // Verifies the PartialOrd derivation: Debug < Info < Warn < Error
        assert!(LogLevel::Debug < LogLevel::Info);
        assert!(LogLevel::Info < LogLevel::Warn);
        assert!(LogLevel::Warn < LogLevel::Error);
    }

    #[test]
    fn test_logger_default_level_info_filters_debug() {
        let l = StructuredLogger::new(); // default Info
        l.log(LogLevel::Debug, "debug msg");
        l.log(LogLevel::Info, "info msg");
        let entries = l.entries();
        assert_eq!(entries.len(), 1, "debug should be filtered out");
        assert_eq!(entries[0].message, "info msg");
        assert_eq!(entries[0].level, LogLevel::Info);
        // Timestamp should be a non-empty RFC3339 string
        assert!(!entries[0].timestamp.is_empty());
        assert!(entries[0].timestamp.contains('T'));
    }

    #[test]
    fn test_logger_with_level_warn_filters_info() {
        let l = StructuredLogger::with_level(LogLevel::Warn);
        l.log(LogLevel::Debug, "debug msg");
        l.log(LogLevel::Info, "info msg");
        l.log(LogLevel::Warn, "warn msg");
        l.log(LogLevel::Error, "error msg");
        let entries = l.entries();
        assert_eq!(entries.len(), 2, "only Warn and Error should pass");
        assert!(entries.iter().all(|e| e.level >= LogLevel::Warn));
        assert!(entries.iter().any(|e| e.message == "warn msg"));
        assert!(entries.iter().any(|e| e.message == "error msg"));
    }

    #[test]
    fn test_logger_with_level_error_only_error() {
        let l = StructuredLogger::with_level(LogLevel::Error);
        l.log(LogLevel::Warn, "warn msg");
        l.log(LogLevel::Error, "error msg");
        let entries = l.entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].level, LogLevel::Error);
    }

    #[test]
    fn test_logger_with_level_debug_passes_everything() {
        let l = StructuredLogger::with_level(LogLevel::Debug);
        l.log(LogLevel::Debug, "d");
        l.log(LogLevel::Info, "i");
        l.log(LogLevel::Warn, "w");
        l.log(LogLevel::Error, "e");
        assert_eq!(l.entries().len(), 4);
    }

    #[test]
    fn test_logger_factory_creates_loggers_with_different_levels() {
        let factory = LoggerFactory::new();
        let debug_logger = factory.debug();
        let error_logger = factory.error();
        debug_logger.log(LogLevel::Debug, "debug");
        debug_logger.log(LogLevel::Info, "info");
        error_logger.log(LogLevel::Info, "should be filtered");
        error_logger.log(LogLevel::Error, "error");
        assert_eq!(debug_logger.entries().len(), 2);
        assert_eq!(error_logger.entries().len(), 1);
        assert_eq!(error_logger.entries()[0].message, "error");
    }

    #[test]
    fn test_logger_factory_default_creates_info_logger() {
        let factory = LoggerFactory;
        let logger = factory.info();
        logger.log(LogLevel::Debug, "should be filtered");
        logger.log(LogLevel::Info, "should pass");
        assert_eq!(logger.entries().len(), 1);
    }

    #[test]
    fn test_logger_output_method_logs_at_info() {
        let l = StructuredLogger::with_level(LogLevel::Debug);
        l.output("hello");
        let entries = l.entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].level, LogLevel::Info);
        assert_eq!(entries[0].message, "hello");
    }

    #[test]
    fn test_metrics_increment_and_read() {
        let mut m = Metrics::new();
        m.increment_counter("req");
        m.increment_counter("req");
        m.increment_counter("err");
        assert_eq!(m.get_counter("req"), Some(2));
        assert_eq!(m.get_counter("err"), Some(1));
        assert_eq!(m.get_counter("missing"), None);
    }

    #[test]
    fn test_metrics_add_counter() {
        let mut m = Metrics::new();
        m.add_counter("bytes", 100);
        m.add_counter("bytes", 50);
        assert_eq!(m.get_counter("bytes"), Some(150));
    }

    #[test]
    fn test_metrics_gauge_overwrites() {
        let mut m = Metrics::new();
        m.set_gauge("cpu", 0.5);
        assert_eq!(m.get_gauge("cpu"), Some(0.5));
        m.set_gauge("cpu", 0.8); // overwrite
        assert_eq!(m.get_gauge("cpu"), Some(0.8));
        assert_eq!(m.get_gauge("missing"), None);
    }

    #[test]
    fn test_metrics_snapshot_captures_state() {
        let mut m = Metrics::new();
        m.increment_counter("a");
        m.increment_counter("a");
        m.increment_counter("b");
        m.set_gauge("g1", 1.0);
        m.set_gauge("g2", 2.5);
        let snap = m.snapshot();
        assert_eq!(snap.counters.get("a"), Some(&2));
        assert_eq!(snap.counters.get("b"), Some(&1));
        assert_eq!(snap.gauges.get("g1"), Some(&1.0));
        assert_eq!(snap.gauges.get("g2"), Some(&2.5));
        // Snapshot is independent of subsequent changes
        m.increment_counter("a");
        assert_eq!(snap.counters.get("a"), Some(&2));
        assert_eq!(m.get_counter("a"), Some(3));
    }

    #[test]
    fn test_log_entry_has_timestamp_and_level() {
        let l = StructuredLogger::with_level(LogLevel::Debug);
        l.log(LogLevel::Warn, "warning");
        let e = &l.entries()[0];
        assert_eq!(e.level, LogLevel::Warn);
        assert_eq!(e.message, "warning");
        // RFC3339 timestamps contain 'T' separator and 'Z' for UTC
        assert!(e.timestamp.contains('T'));
        assert!(e.timestamp.ends_with('Z') || e.timestamp.contains('+'));
    }
}
