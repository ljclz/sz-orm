//! Logger 适配层端到端测试
use sz_orm_core::logger_adapter::{log_count, log_query};
use sz_orm_logger::LogLevel;

#[test]
fn test_log_query_writes_entry() {
    let before = log_count();
    log_query(LogLevel::Info, "e2e log message");
    let after = log_count();
    assert!(after > before, "log count should increment");
}

#[test]
fn test_log_multiple_entries() {
    log_query(LogLevel::Warn, "e2e warning");
    log_query(LogLevel::Error, "e2e error");
    assert!(log_count() > 0);
}
