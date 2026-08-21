//! # Logger Adapter — sz-orm-core 结构化日志适配层
//!
//! v5.0.0 M4：将 sz-orm-logger 的 StructuredLogger 接入 sz-orm-core，
//! 提供 `log_query` / `log_count` 入口。

use std::sync::OnceLock;

use parking_lot::RwLock;
use sz_orm_logger::{LogLevel, Logger, StructuredLogger};

static LOGGER: OnceLock<RwLock<StructuredLogger>> = OnceLock::new();

fn logger() -> &'static RwLock<StructuredLogger> {
    LOGGER.get_or_init(|| RwLock::new(StructuredLogger::new()))
}

/// 记录一条日志
pub fn log_query(level: LogLevel, msg: &str) {
    let logger = logger().read();
    logger.log(level, msg);
}

/// 获取已记录的日志条数
pub fn log_count() -> usize {
    let logger = logger().read();
    logger.entries().len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_query_writes_entry() {
        let before = log_count();
        log_query(LogLevel::Info, "test log message");
        let after = log_count();
        assert!(after > before, "log count should increment");
    }

    #[test]
    fn test_log_multiple_entries() {
        log_query(LogLevel::Warn, "warning message");
        log_query(LogLevel::Error, "error message");
        assert!(log_count() > 0);
    }
}
