use sz_orm_logger::*;

#[test]
fn test_log_level_ordering() {
    assert!(LogLevel::Error > LogLevel::Warn);
    assert!(LogLevel::Warn > LogLevel::Info);
    assert!(LogLevel::Info > LogLevel::Debug);
}

#[test]
fn test_log_level_as_str() {
    assert_eq!(LogLevel::Debug.as_str(), "DEBUG");
    assert_eq!(LogLevel::Info.as_str(), "INFO");
    assert_eq!(LogLevel::Warn.as_str(), "WARN");
    assert_eq!(LogLevel::Error.as_str(), "ERROR");
}

#[test]
fn test_structured_logger_new() {
    let logger = StructuredLogger::new();
    logger.output("test message");
    let entries = logger.entries();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].message, "test message");
    assert_eq!(entries[0].level, LogLevel::Info);
}

#[test]
fn test_structured_logger_level_filter() {
    let logger = StructuredLogger::with_level(LogLevel::Warn);
    logger.log(LogLevel::Debug, "debug msg");
    logger.log(LogLevel::Info, "info msg");
    logger.log(LogLevel::Warn, "warn msg");
    logger.log(LogLevel::Error, "error msg");
    let entries = logger.entries();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].level, LogLevel::Warn);
    assert_eq!(entries[1].level, LogLevel::Error);
}

#[test]
fn test_structured_logger_empty() {
    let logger = StructuredLogger::new();
    assert!(logger.entries().is_empty());
}

#[test]
fn test_structured_logger_output_method() {
    let logger = StructuredLogger::new();
    logger.output("hello");
    logger.output("world");
    let entries = logger.entries();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].message, "hello");
    assert_eq!(entries[1].message, "world");
}

#[test]
fn test_structured_logger_all_levels() {
    let logger = StructuredLogger::with_level(LogLevel::Debug);
    logger.log(LogLevel::Debug, "d");
    logger.log(LogLevel::Info, "i");
    logger.log(LogLevel::Warn, "w");
    logger.log(LogLevel::Error, "e");
    assert_eq!(logger.entries().len(), 4);
}

#[test]
fn test_log_entry_serialization() {
    let entry = LogEntry {
        level: LogLevel::Info,
        message: "test".to_string(),
        timestamp: "2026-08-08T00:00:00".to_string(),
    };
    let json = serde_json::to_string(&entry).unwrap();
    let deserialized: LogEntry = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.message, "test");
    assert_eq!(deserialized.level, LogLevel::Info);
}

#[test]
fn test_log_level_serialization() {
    let json = serde_json::to_string(&LogLevel::Warn).unwrap();
    let deserialized: LogLevel = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, LogLevel::Warn);
}
