//! 测试清理工具
//!
//! 提供独立表名生成、DROP TABLE 清理和事务回滚清理策略。

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// 生成独立表名（prefix + 进程 ID + 时间戳 + 计数器）
pub fn unique_table_name(prefix: &str) -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let pid = std::process::id() as u64;
    let counter = COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("{}_{}_{}_{}", prefix, pid, timestamp % 1_000_000, counter)
}

/// 生成带 UUID 风格的独立表名
pub fn unique_table_name_with_suffix(prefix: &str, suffix: &str) -> String {
    let base = unique_table_name(prefix);
    format!("{}_{}", base, suffix)
}
