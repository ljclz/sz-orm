//! SQLite EXPLAIN QUERY PLAN 解析器
//!
//! 输入示例（`EXPLAIN QUERY PLAN SELECT ...` 输出，两种格式均支持）：
//!
//! ```text
//! QUERY PLAN
//! |--SCAN users
//! `--SEARCH users USING INDEX users_email_idx (email=?)
//! ```
//!
//! ```text
//! 0 0 0 SCAN TABLE users
//! 0 1 0 SEARCH TABLE users USING INDEX users_email_idx (email=?)
//! ```
//!
//! 注意：SQLite 的 EXPLAIN QUERY PLAN 不输出预估行数，`rows` 固定为 0。

use crate::{ExplainError, ExplainParser, ExplainPlan, ScanType};

/// SQLite EXPLAIN 解析器
#[derive(Debug)]
pub struct SqliteParser;

impl ExplainParser for SqliteParser {
    fn parse(&self, raw: &str) -> Result<ExplainPlan, ExplainError> {
        let mut plans: Vec<(ScanType, String, Option<String>)> = Vec::new();
        for line in raw.lines() {
            let line = line.trim();
            if line.is_empty() || line.eq_ignore_ascii_case("QUERY PLAN") {
                continue;
            }
            // 去掉 `|--` / `` `-- `` 前缀
            let body = strip_prefix(line);
            // 数字前缀格式：`0 0 0 SCAN TABLE users` → 跳过前 3 个数字列
            let body = if body
                .split_whitespace()
                .next()
                .unwrap_or("")
                .chars()
                .all(|c| c.is_ascii_digit())
            {
                body.split_whitespace()
                    .skip(3)
                    .collect::<Vec<_>>()
                    .join(" ")
            } else {
                body
            };
            if body.is_empty() {
                continue;
            }
            if let Some(rest) = strip_prefix_ci(&body, "SCAN") {
                plans.push((ScanType::FullTable, extract_table(rest), None));
            } else if let Some(rest) = strip_prefix_ci(&body, "SEARCH") {
                let (scan_type, table, index) = classify_search(rest);
                plans.push((scan_type, table, index));
            } else if strip_prefix_ci(&body, "USE TEMP B-TREE").is_some() {
                plans.push((ScanType::Other, "<temp>".into(), None));
            }
            // 其他（LIST SUBQUERY/CO-ROUTINE 等）忽略
        }
        let (scan_type, table, index) =
            plans
                .first()
                .cloned()
                .ok_or_else(|| ExplainError::Unparseable {
                    reason: "no SCAN/SEARCH row found".into(),
                })?;
        Ok(ExplainPlan {
            scan_type,
            table,
            index,
            rows: 0,
            extra: Vec::new(),
        })
    }
}

/// 去掉树形前缀（`|--`/`` `-- ``）与 `TABLE` 关键字
fn strip_prefix(line: &str) -> String {
    let mut s = line.to_string();
    for p in ["|--", "`--", "--"] {
        if let Some(rest) = s.strip_prefix(p) {
            s = rest.trim().to_string();
        }
    }
    s
}

/// 提取 `SCAN TABLE <t>` 或 `SCAN <t>` 中的表名
fn extract_table(rest: &str) -> String {
    rest.split_whitespace()
        .find(|w| !w.eq_ignore_ascii_case("TABLE") && !w.starts_with('('))
        .unwrap_or("<unknown>")
        .to_string()
}

/// 分类 `SEARCH ... USING INDEX <idx>` / `SEARCH ... USING PRIMARY KEY` / `SEARCH ... USING COVERING INDEX <idx>`
fn classify_search(rest: &str) -> (ScanType, String, Option<String>) {
    let lower = rest.to_ascii_lowercase();
    let table = extract_table(rest);
    let Some(pos) = lower.find("using") else {
        return (ScanType::Other, table, None);
    };
    let using = lower[pos + 5..].trim_start();
    if using.starts_with("primary key") {
        return (ScanType::UniqueLookup, table, None);
    }
    if let Some(idx) = using.strip_prefix("covering index ") {
        let idx = idx.split_whitespace().next().unwrap_or("<unknown>");
        return (ScanType::IndexRange, table, Some(idx.to_string()));
    }
    if let Some(idx) = using.strip_prefix("index ") {
        let idx = idx.split_whitespace().next().unwrap_or("<unknown>");
        return (ScanType::IndexRange, table, Some(idx.to_string()));
    }
    (ScanType::Other, table, None)
}

fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    if s.len() >= prefix.len() && s[..prefix.len()].eq_ignore_ascii_case(prefix) {
        Some(&s[prefix.len()..])
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TREE_FORMAT: &str = "\
QUERY PLAN
|--SCAN users
`--SEARCH orders USING INDEX idx_orders_user (user_id=?)
";

    const TABLE_FORMAT: &str = "\
0 0 0 SCAN TABLE users
0 1 0 SEARCH TABLE orders USING INDEX idx_orders_user (user_id=?)
";

    const PK_LOOKUP: &str = "\
QUERY PLAN
|--SEARCH users USING PRIMARY KEY (rowid=?)
";

    const COVERING: &str = "\
QUERY PLAN
`--SEARCH orders USING COVERING INDEX idx_orders_user (user_id=?)
";

    #[test]
    fn parses_tree_format() {
        let plan = SqliteParser.parse(TREE_FORMAT).expect("parse ok");
        assert_eq!(plan.scan_type, ScanType::FullTable);
        assert_eq!(plan.table, "users");
        assert!(plan.index.is_none());
        assert_eq!(plan.rows, 0);
    }

    #[test]
    fn parses_table_format() {
        let plan = SqliteParser.parse(TABLE_FORMAT).expect("parse ok");
        assert_eq!(plan.scan_type, ScanType::FullTable);
        assert_eq!(plan.table, "users");
    }

    #[test]
    fn parses_primary_key_search() {
        let plan = SqliteParser.parse(PK_LOOKUP).expect("parse ok");
        assert_eq!(plan.scan_type, ScanType::UniqueLookup);
        assert_eq!(plan.table, "users");
    }

    #[test]
    fn parses_covering_index() {
        let plan = SqliteParser.parse(COVERING).expect("parse ok");
        assert_eq!(plan.scan_type, ScanType::IndexRange);
        assert_eq!(plan.index.as_deref(), Some("idx_orders_user"));
    }

    #[test]
    fn rejects_unknown_input() {
        let err = SqliteParser.parse("hello world").unwrap_err();
        assert!(matches!(err, ExplainError::Unparseable { .. }));
    }
}
