//! MySQL EXPLAIN 解析器（同时支持两种输出格式）
//!
//! 格式一：树形文本（MySQL 8.0.22+ / 9.x 默认输出，类似 PostgreSQL）：
//!
//! ```text
//! -> Filter: (sz_orm_explain_users.`name` = 'alice')  (cost=0.45 rows=1)
//!     -> Table scan on sz_orm_explain_users  (cost=0.45 rows=2)
//! -> Covering index lookup on sz_orm_explain_users using idx_email (email = 'a@x.com')  (cost=0.35 rows=1)
//! ```
//!
//! 格式二：经典表格（`EXPLAIN FORMAT=TRADITIONAL` / 旧版本）：
//!
//! ```text
//! +----+-------------+-------+-------+---------------+------+---------+------+------+-------------+
//! | id | select_type | table | type  | possible_keys | key  | key_len | ref  | rows | Extra       |
//! +----+-------------+-------+-------+---------------+------+---------+------+------+-------------+
//! |  1 | SIMPLE      | users | ALL   | NULL          | NULL | NULL    | NULL | 1150 | Using where |
//! +----+-------------+-------+-------+---------------+------+---------+------+------+-------------+
//! ```

use crate::{ExplainError, ExplainParser, ExplainPlan, ScanType};

/// MySQL EXPLAIN 解析器
#[derive(Debug)]
pub struct MySqlParser;

impl ExplainParser for MySqlParser {
    fn parse(&self, raw: &str) -> Result<ExplainPlan, ExplainError> {
        // 树形文本格式（以 `->` 开头）优先识别
        if raw.lines().any(|l| l.trim_start().starts_with("->")) {
            return parse_tree_format(raw);
        }
        parse_table_format(raw)
    }
}

/// 树形文本格式解析（MySQL 8.0.22+ / 9.x）
fn parse_tree_format(raw: &str) -> Result<ExplainPlan, ExplainError> {
    let mut best: Option<(ScanType, String, Option<String>, u64)> = None;
    let rank = |s: ScanType| match s {
        ScanType::FullTable => 5,
        ScanType::IndexRange => 4,
        ScanType::UniqueLookup => 3,
        ScanType::IndexLookup => 2,
        ScanType::Other => 1,
    };

    for line in raw.lines() {
        let line = line.trim();
        // 去掉 `->` 前缀
        let Some(body) = line.strip_prefix("->") else {
            continue;
        };
        let body = body.trim();
        let upper = body.to_ascii_uppercase();

        let candidate: Option<(ScanType, String, Option<String>, u64)> =
            if upper.starts_with("TABLE SCAN ON") {
                Some((
                    ScanType::FullTable,
                    extract_table_from_tree(&body["TABLE SCAN ON".len()..]),
                    None,
                    0,
                ))
            } else if upper.starts_with("PRIMARY KEY LOOKUP ON") {
                let rest = &body["PRIMARY KEY LOOKUP ON".len()..];
                Some((
                    ScanType::UniqueLookup,
                    extract_table_from_tree(rest),
                    None,
                    0,
                ))
            } else if upper.starts_with("COVERING INDEX LOOKUP ON") {
                let (table, index) = split_tree_index(body, "COVERING INDEX LOOKUP ON");
                Some((ScanType::IndexRange, table, Some(index), 0))
            } else if upper.starts_with("INDEX RANGE SCAN ON") {
                let (table, index) = split_tree_index(body, "INDEX RANGE SCAN ON");
                Some((ScanType::IndexRange, table, Some(index), 0))
            } else if upper.starts_with("INDEX LOOKUP ON") {
                let (table, index) = split_tree_index(body, "INDEX LOOKUP ON");
                Some((ScanType::IndexRange, table, Some(index), 0))
            } else {
                None
            };
        if let Some(c) = candidate {
            if best.is_none() || rank(c.0) > rank(best.as_ref().unwrap().0) {
                best = Some(c);
            }
        }
    }
    let (scan_type, table, index, _) = best.ok_or_else(|| ExplainError::Unparseable {
        reason: "no recognizable tree operator".into(),
    })?;
    // rows= 从任意行提取
    let rows = crate::parsers::extract_rows_from_kv(raw).unwrap_or(0);
    // Filter: 等收集为 extra
    let extra: Vec<String> = raw
        .lines()
        .filter(|l| l.trim_start().starts_with("-> Filter:"))
        .map(|l| l.trim().to_string())
        .collect();
    Ok(ExplainPlan {
        scan_type,
        table,
        index,
        rows,
        extra,
    })
}

/// 从 `on <table> using <idx> (...)` 提取表名与索引名（树格式）
fn split_tree_index(body: &str, keyword: &str) -> (String, String) {
    let rest = &body[keyword.len()..];
    let lower = rest.to_ascii_lowercase();
    let table = if let Some(pos) = lower.find(" using ") {
        rest[..pos].trim().to_string()
    } else {
        rest.split_whitespace()
            .next()
            .unwrap_or("<unknown>")
            .to_string()
    };
    let index = if let Some(pos) = lower.find(" using ") {
        let after = &rest[pos + 7..];
        after
            .split_whitespace()
            .next()
            .unwrap_or("<unknown>")
            .to_string()
    } else {
        "<unknown>".to_string()
    };
    (table, index)
}

/// 提取树格式中的表名（取第一个词，忽略 `(cost=...)` 括号）
fn extract_table_from_tree(rest: &str) -> String {
    let name = rest.split_whitespace().next().unwrap_or("<unknown>");
    if name.starts_with('(') {
        "<unknown>".into()
    } else {
        name.to_string()
    }
}

/// 经典表格格式解析（`EXPLAIN FORMAT=TRADITIONAL` / 旧版本）
fn parse_table_format(raw: &str) -> Result<ExplainPlan, ExplainError> {
    let mut columns: Vec<Vec<String>> = Vec::new();
    for line in raw.lines() {
        let line = line.trim();
        if !line.starts_with('|') {
            continue;
        }
        let cells: Vec<String> = line
            .split('|')
            .map(|c| c.trim().to_string())
            .filter(|c| !c.is_empty())
            .collect();
        if cells.is_empty() {
            continue;
        }
        // 跳过表头（包含列名）与分隔行
        if cells.iter().any(|c| c.eq_ignore_ascii_case("select_type"))
            || cells
                .iter()
                .all(|c| c.chars().all(|ch| ch == '-' || ch == '+'))
        {
            continue;
        }
        columns.push(cells);
    }
    let cells = columns.first().ok_or_else(|| ExplainError::Unparseable {
        reason: "no data row found".into(),
    })?;
    // 列布局（MySQL 8.0 前 10 列；8.0+ 含 partitions/filtered 共 12 列）：
    // id | select_type | table | type | possible_keys | key | key_len | ref | rows | Extra
    // 用列名表头识别定位，避免硬编码索引偏移
    let header = detect_table_header(raw);
    let (table_idx, type_idx, key_idx, rows_idx, extra_idx) = match header {
        Some(cols) => (
            cols.iter().position(|c| c == "table").unwrap_or(2),
            cols.iter().position(|c| c == "type").unwrap_or(3),
            cols.iter().position(|c| c == "key").unwrap_or(5),
            cols.iter().position(|c| c == "rows").unwrap_or(8),
            cols.iter().position(|c| c == "Extra").unwrap_or(9),
        ),
        None => (2, 3, 5, 8, 9),
    };
    let table = cells
        .get(table_idx)
        .cloned()
        .unwrap_or_else(|| "<unknown>".into());
    let scan_type = match cells
        .get(type_idx)
        .map(|s| s.to_ascii_uppercase())
        .as_deref()
    {
        Some("ALL") => ScanType::FullTable,
        Some("RANGE") | Some("INDEX") => ScanType::IndexRange,
        Some("REF") | Some("EQ_REF") => ScanType::IndexLookup,
        Some("CONST") | Some("SYSTEM") => ScanType::UniqueLookup,
        _ => ScanType::Other,
    };
    let index = match cells.get(key_idx).map(|s| s.as_str()) {
        Some(s) if !s.is_empty() && !s.eq_ignore_ascii_case("null") => Some(s.to_string()),
        _ => None,
    };
    let rows = cells
        .get(rows_idx)
        .and_then(|s| crate::parsers::parse_u64(s))
        .unwrap_or(0);
    let extra = cells
        .get(extra_idx)
        .map(|s| {
            s.split(',')
                .map(|e| e.trim().to_string())
                .filter(|e| !e.is_empty())
                .collect()
        })
        .unwrap_or_default();
    Ok(ExplainPlan {
        scan_type,
        table,
        index,
        rows,
        extra,
    })
}

/// 从表格输出中检测表头列名（存在时返回列名列表）
fn detect_table_header(raw: &str) -> Option<Vec<String>> {
    for line in raw.lines() {
        let line = line.trim();
        if !line.starts_with('|') {
            continue;
        }
        let cells: Vec<String> = line
            .split('|')
            .map(|c| c.trim().to_string())
            .filter(|c| !c.is_empty())
            .collect();
        if cells.iter().any(|c| c.eq_ignore_ascii_case("select_type")) {
            return Some(cells);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_SCAN: &str = "\
+----+-------------+-------+-------+---------------+------+---------+------+------+-------------+
| id | select_type | table | type  | possible_keys | key  | key_len | ref  | rows | Extra       |
+----+-------------+-------+-------+---------------+------+---------+------+------+-------------+
|  1 | SIMPLE      | users | ALL   | NULL          | NULL | NULL    | NULL | 1150 | Using where |
+----+-------------+-------+-------+---------------+------+---------+------+------+-------------+
";

    const PK_LOOKUP: &str = "\
+----+-------------+-------+-------+---------------+---------+---------+-------+------+-------+
| id | select_type | table | type  | possible_keys | key     | key_len | ref   | rows | Extra |
+----+-------------+-------+-------+---------------+---------+---------+-------+------+-------+
|  1 | SIMPLE      | users | const | PRIMARY       | PRIMARY | 8       | const |    1 | NULL  |
+----+-------------+-------+-------+---------------+---------+---------+-------+------+-------+
";

    const RANGE_SCAN: &str = "\
+----+-------------+--------+-------+---------------+-----------------+---------+------+------+-------------+
| id | select_type | table  | type  | possible_keys | key             | key_len | ref  | rows | Extra       |
+----+-------------+--------+-------+---------------+-----------------+---------+------+------+-------------+
|  1 | SIMPLE      | orders | range | idx_user      | idx_user        | 8       | NULL |   42 | Using where |
+----+-------------+--------+-------+---------------+-----------------+---------+------+------+-------------+
";

    #[test]
    fn parses_full_table_scan() {
        let plan = MySqlParser.parse(ALL_SCAN).expect("parse ok");
        assert_eq!(plan.scan_type, ScanType::FullTable);
        assert_eq!(plan.table, "users");
        assert!(plan.index.is_none());
        assert_eq!(plan.rows, 1150);
        assert!(plan.extra.iter().any(|e| e.contains("Using where")));
        assert!(plan.missing_index(1000));
    }

    #[test]
    fn parses_pk_lookup() {
        let plan = MySqlParser.parse(PK_LOOKUP).expect("parse ok");
        assert_eq!(plan.scan_type, ScanType::UniqueLookup);
        assert_eq!(plan.index.as_deref(), Some("PRIMARY"));
        assert_eq!(plan.rows, 1);
        assert!(!plan.missing_index(1000));
    }

    #[test]
    fn parses_range_scan_with_index() {
        let plan = MySqlParser.parse(RANGE_SCAN).expect("parse ok");
        assert_eq!(plan.scan_type, ScanType::IndexRange);
        assert_eq!(plan.index.as_deref(), Some("idx_user"));
        assert_eq!(plan.rows, 42);
    }

    #[test]
    fn rejects_empty_input() {
        let err = MySqlParser.parse("no explain output here").unwrap_err();
        assert!(matches!(err, ExplainError::Unparseable { .. }));
    }

    // ---- 树形文本格式（MySQL 8.0.22+ / 9.x）----

    const TREE_TABLE_SCAN: &str = "\
-> Filter: (sz_orm_explain_users.`name` = 'alice')  (cost=0.45 rows=1)
    -> Table scan on sz_orm_explain_users  (cost=0.45 rows=2)
";

    const TREE_COVERING_LOOKUP: &str = "\
-> Covering index lookup on sz_orm_explain_users using idx_email (email = 'a@x.com')  (cost=0.35 rows=1)
";

    const TREE_PK_LOOKUP: &str = "\
-> Primary key lookup on sz_orm_explain_users (id = 1)  (cost=0.35 rows=1)
";

    #[test]
    fn parses_tree_table_scan() {
        let plan = MySqlParser.parse(TREE_TABLE_SCAN).expect("parse ok");
        assert_eq!(plan.scan_type, ScanType::FullTable);
        assert_eq!(plan.table, "sz_orm_explain_users");
        assert!(plan.index.is_none());
        assert_eq!(plan.rows, 1);
        assert!(plan.missing_index(1000));
    }

    #[test]
    fn parses_tree_covering_index_lookup() {
        let plan = MySqlParser.parse(TREE_COVERING_LOOKUP).expect("parse ok");
        assert_eq!(plan.scan_type, ScanType::IndexRange);
        assert_eq!(plan.table, "sz_orm_explain_users");
        assert_eq!(plan.index.as_deref(), Some("idx_email"));
        assert!(!plan.missing_index(1000));
    }

    #[test]
    fn parses_tree_primary_key_lookup() {
        let plan = MySqlParser.parse(TREE_PK_LOOKUP).expect("parse ok");
        assert_eq!(plan.scan_type, ScanType::UniqueLookup);
        assert_eq!(plan.table, "sz_orm_explain_users");
    }
}
