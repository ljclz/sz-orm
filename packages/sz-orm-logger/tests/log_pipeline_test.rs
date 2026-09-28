use sz_orm_logger::log_pipeline::*;
use sz_orm_logger::LogLevel;
use std::collections::{HashMap, HashSet};

#[test]
fn test_log_record_new() {
    let record = LogRecord::new(LogLevel::Info, "module", "message");
    assert_eq!(record.level, LogLevel::Info);
    assert_eq!(record.target, "module");
    assert_eq!(record.message, "message");
    assert!(record.fields.is_empty());
}

#[test]
fn test_log_record_with_field() {
    let record = LogRecord::new(LogLevel::Info, "module", "message")
        .with_field("key", "value");
    assert_eq!(record.fields.get("key"), Some(&"value".to_string()));
}

#[test]
fn test_log_record_with_fields() {
    let mut fields = HashMap::new();
    fields.insert("a".to_string(), "1".to_string());
    fields.insert("b".to_string(), "2".to_string());
    let record = LogRecord::new(LogLevel::Info, "module", "message")
        .with_fields(fields);
    assert_eq!(record.fields.len(), 2);
}

#[test]
fn test_log_record_level_at_least() {
    let record = LogRecord::new(LogLevel::Warn, "module", "message");
    assert!(record.level_at_least(LogLevel::Warn));
    assert!(record.level_at_least(LogLevel::Info));
    assert!(!record.level_at_least(LogLevel::Error));
}

#[test]
fn test_log_record_display() {
    let record = LogRecord::new(LogLevel::Info, "module", "message");
    let s = format!("{}", record);
    assert!(s.contains("INFO"));
    assert!(s.contains("module"));
    assert!(s.contains("message"));
}

#[test]
fn test_level_threshold_filter() {
    let filter = LevelThresholdFilter::new(LogLevel::Warn);
    let warn_record = LogRecord::new(LogLevel::Warn, "m", "msg");
    let info_record = LogRecord::new(LogLevel::Info, "m", "msg");
    let error_record = LogRecord::new(LogLevel::Error, "m", "msg");
    assert!(filter.should_keep(&warn_record));
    assert!(!filter.should_keep(&info_record));
    assert!(filter.should_keep(&error_record));
    assert_eq!(filter.name(), "level_threshold");
}

#[test]
fn test_target_filter_allow() {
    let filter = TargetFilter::allowlist(&["module_a"]);
    let record_a = LogRecord::new(LogLevel::Info, "module_a", "msg");
    let record_b = LogRecord::new(LogLevel::Info, "module_b", "msg");
    assert!(filter.should_keep(&record_a));
    assert!(!filter.should_keep(&record_b));
}

#[test]
fn test_target_filter_deny() {
    let filter = TargetFilter::blocklist(&["module_a"]);
    let record_a = LogRecord::new(LogLevel::Info, "module_a", "msg");
    let record_b = LogRecord::new(LogLevel::Info, "module_b", "msg");
    assert!(!filter.should_keep(&record_a));
    assert!(filter.should_keep(&record_b));
}

#[test]
fn test_contains_filter() {
    let filter = ContainsFilter::include("error");
    let record_with = LogRecord::new(LogLevel::Info, "m", "an error occurred");
    let record_without = LogRecord::new(LogLevel::Info, "m", "all good");
    assert!(filter.should_keep(&record_with));
    assert!(!filter.should_keep(&record_without));
}

#[test]
fn test_log_level_as_str() {
    assert_eq!(LogLevel::Trace.as_str(), "TRACE");
    assert_eq!(LogLevel::Debug.as_str(), "DEBUG");
    assert_eq!(LogLevel::Info.as_str(), "INFO");
    assert_eq!(LogLevel::Warn.as_str(), "WARN");
    assert_eq!(LogLevel::Error.as_str(), "ERROR");
}

#[test]
fn test_log_level_ordering() {
    assert!(LogLevel::Error > LogLevel::Warn);
    assert!(LogLevel::Warn > LogLevel::Info);
    assert!(LogLevel::Info > LogLevel::Debug);
    assert!(LogLevel::Debug > LogLevel::Trace);
}

#[test]
fn test_all_filter_empty() {
    let filter = AllFilter::new(vec![]);
    let record = LogRecord::new(LogLevel::Info, "m", "msg");
    assert!(filter.should_keep(&record));
}

#[test]
fn test_all_filter_all_pass() {
    let filter = AllFilter::new(vec![
        Box::new(LevelThresholdFilter::new(LogLevel::Info)),
        Box::new(ContainsFilter::include("test")),
    ]);
    let record = LogRecord::new(LogLevel::Info, "m", "test message");
    assert!(filter.should_keep(&record));
}

#[test]
fn test_all_filter_one_fails() {
    let filter = AllFilter::new(vec![
        Box::new(LevelThresholdFilter::new(LogLevel::Warn)),
        Box::new(ContainsFilter::include("test")),
    ]);
    let record = LogRecord::new(LogLevel::Info, "m", "test message");
    assert!(!filter.should_keep(&record));
}

#[test]
fn test_any_filter_empty() {
    let filter = AnyFilter::new(vec![]);
    let record = LogRecord::new(LogLevel::Info, "m", "msg");
    assert!(!filter.should_keep(&record));
}

#[test]
fn test_any_filter_one_passes() {
    let filter = AnyFilter::new(vec![
        Box::new(LevelThresholdFilter::new(LogLevel::Error)),
        Box::new(ContainsFilter::include("test")),
    ]);
    let record = LogRecord::new(LogLevel::Info, "m", "test message");
    assert!(filter.should_keep(&record));
}

#[test]
fn test_any_filter_none_pass() {
    let filter = AnyFilter::new(vec![
        Box::new(LevelThresholdFilter::new(LogLevel::Error)),
        Box::new(ContainsFilter::include("missing")),
    ]);
    let record = LogRecord::new(LogLevel::Info, "m", "test message");
    assert!(!filter.should_keep(&record));
}