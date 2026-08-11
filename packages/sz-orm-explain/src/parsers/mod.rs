//! 各方言 EXPLAIN 解析器
//!
//! 每个解析器实现 [`crate::ExplainParser`]，将对应方言的 EXPLAIN 输出
//! 解析为统一的 [`crate::ExplainPlan`]。解析策略保持宽容：
//! 关键信息缺失时填默认值（`table = "<unknown>"`、`rows = 0`），不 panic。

pub mod mssql;
pub mod mysql;
pub mod oracle;
pub mod postgres;
pub mod sqlite;

/// 从 EXPLAIN 文本中提取第一个 `rows=N` 形式的行数（PG/MySQL 均有此格式）
pub(crate) fn extract_rows_from_kv(raw: &str) -> Option<u64> {
    for line in raw.lines() {
        if let Some(pos) = line.find("rows=") {
            let rest = &line[pos + 5..];
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if !digits.is_empty() {
                return digits.parse().ok();
            }
        }
    }
    None
}

/// 提取数字（过滤非数字字符），供表格型输出使用
pub(crate) fn parse_u64(s: &str) -> Option<u64> {
    let digits: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        None
    } else {
        digits.parse().ok()
    }
}
