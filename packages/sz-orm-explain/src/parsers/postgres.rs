//! PostgreSQL EXPLAIN 文本格式解析器
//!
//! 输入示例（`EXPLAIN [VERBOSE] SELECT ...` 输出）：
//!
//! ```text
//! Seq Scan on users  (cost=0.00..21.50 rows=1150 width=36)
//!   Filter: (age > 18)
//! Index Scan using users_pkey on users  (cost=0.29..8.31 rows=1 width=36)
//!   Index Cond: (id = 1)
//! ```

use crate::{ExplainError, ExplainParser, ExplainPlan, ScanType};

/// PostgreSQL EXPLAIN 解析器
#[derive(Debug)]
pub struct PostgresParser;

impl ExplainParser for PostgresParser {
    fn parse(&self, raw: &str) -> Result<ExplainPlan, ExplainError> {
        // 顶层节点为第一个非空行
        let top = raw
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .ok_or_else(|| ExplainError::Unparseable {
                reason: "empty explain output".into(),
            })?;

        let (scan_type, table, index) = classify_operation(top);
        let rows = crate::parsers::extract_rows_from_kv(raw).unwrap_or(0);
        let extra = collect_extra(raw);

        Ok(ExplainPlan {
            scan_type,
            table,
            index,
            rows,
            extra,
        })
    }
}

/// 根据顶层操作符文本分类扫描类型，并提取表名/索引名
fn classify_operation(top: &str) -> (ScanType, String, Option<String>) {
    // 模式一：`Index Scan using <index> on <table>`
    if let Some(rest) = strip_prefix_ci(top, "Index Scan using") {
        let (index, table) = split_index_table(rest);
        return (ScanType::IndexRange, table, Some(index));
    }
    // 模式二：`Index Only Scan using <index> on <table>`
    if let Some(rest) = strip_prefix_ci(top, "Index Only Scan using") {
        let (index, table) = split_index_table(rest);
        return (ScanType::IndexRange, table, Some(index));
    }
    // 模式三：`Seq Scan on <table>`
    if let Some(rest) = strip_prefix_ci(top, "Seq Scan on") {
        return (ScanType::FullTable, extract_table(rest), None);
    }
    // 模式四：`Bitmap Heap Scan on <table>`（索引驱动，无法直接判断索引）
    if let Some(rest) = strip_prefix_ci(top, "Bitmap Heap Scan on") {
        return (ScanType::Other, extract_table(rest), None);
    }
    // 模式五：`Bitmap Index Scan on <index>`
    if let Some(rest) = strip_prefix_ci(top, "Bitmap Index Scan on") {
        return (
            ScanType::IndexRange,
            "<unknown>".into(),
            Some(extract_name(rest)),
        );
    }
    // 其他（Sort/Hash Join/Nested Loop/Aggregate 等）
    (
        ScanType::Other,
        top.split_whitespace()
            .nth(2)
            .unwrap_or("<unknown>")
            .to_string(),
        None,
    )
}

/// 提取 `using <index> on <table>` 中的索引名与表名
fn split_index_table(rest: &str) -> (String, String) {
    let lower = rest.to_ascii_lowercase();
    if let Some(pos) = lower.find(" on ") {
        let index = rest[..pos].trim().to_string();
        let table = extract_table(&rest[pos + 4..]);
        (index, table)
    } else {
        (rest.trim().to_string(), "<unknown>".into())
    }
}

/// 提取 `on <table> (cost=...)` 中的表名
fn extract_table(rest: &str) -> String {
    let name = rest.split_whitespace().next().unwrap_or("<unknown>");
    if name.starts_with('(') {
        "<unknown>".into()
    } else {
        name.to_string()
    }
}

/// 提取 `on <index>` 中的索引名
fn extract_name(rest: &str) -> String {
    rest.split_whitespace()
        .next()
        .unwrap_or("<unknown>")
        .to_string()
}

fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    if s.len() >= prefix.len() && s[..prefix.len()].eq_ignore_ascii_case(prefix) {
        Some(&s[prefix.len()..])
    } else {
        None
    }
}

/// 收集子节点信息（Filter/Index Cond 等）为 extra
fn collect_extra(raw: &str) -> Vec<String> {
    let mut extra = Vec::new();
    for line in raw.lines() {
        let t = line.trim();
        for key in ["Filter:", "Index Cond:", "Heap Blocks:", "Buffers:"] {
            if let Some(pos) = t.find(key) {
                extra.push(t[pos..].to_string());
            }
        }
    }
    extra
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEQ_SCAN: &str = "\
Seq Scan on users  (cost=0.00..21.50 rows=1150 width=36)
  Filter: (age > 18)
";

    const INDEX_SCAN: &str = "\
Index Scan using users_pkey on users  (cost=0.29..8.31 rows=1 width=36)
  Index Cond: (id = 1)
";

    const INDEX_ONLY: &str = "\
Index Only Scan using orders_user_idx on orders  (cost=0.42..8.44 rows=1 width=8)
  Index Cond: (user_id = 42)
";

    const JOIN_PLAN: &str = "\
Hash Join  (cost=25.31..49.02 rows=1150 width=72)
  Hash Cond: (o.user_id = u.id)
  ->  Seq Scan on orders  (cost=0.00..16.50 rows=1150 width=36)
  ->  Hash  (cost=21.50..21.50 rows=1150 width=36)
        ->  Seq Scan on users  (cost=0.00..21.50 rows=1150 width=36)
";

    #[test]
    fn parses_seq_scan() {
        let plan = PostgresParser.parse(SEQ_SCAN).expect("parse ok");
        assert_eq!(plan.scan_type, ScanType::FullTable);
        assert_eq!(plan.table, "users");
        assert!(plan.index.is_none());
        assert_eq!(plan.rows, 1150);
        assert!(plan.extra.iter().any(|e| e.contains("Filter")));
        assert!(plan.missing_index(1000));
    }

    #[test]
    fn parses_index_scan() {
        let plan = PostgresParser.parse(INDEX_SCAN).expect("parse ok");
        assert_eq!(plan.scan_type, ScanType::IndexRange);
        assert_eq!(plan.index.as_deref(), Some("users_pkey"));
        assert_eq!(plan.table, "users");
        assert_eq!(plan.rows, 1);
    }

    #[test]
    fn parses_index_only_scan() {
        let plan = PostgresParser.parse(INDEX_ONLY).expect("parse ok");
        assert_eq!(plan.scan_type, ScanType::IndexRange);
        assert_eq!(plan.index.as_deref(), Some("orders_user_idx"));
        assert_eq!(plan.table, "orders");
    }

    #[test]
    fn top_level_join_is_other() {
        let plan = PostgresParser.parse(JOIN_PLAN).expect("parse ok");
        assert_eq!(plan.scan_type, ScanType::Other);
        assert_eq!(plan.rows, 1150);
    }

    #[test]
    fn rejects_empty_input() {
        let err = PostgresParser.parse("   \n  \n").unwrap_err();
        assert!(matches!(err, ExplainError::Unparseable { .. }));
    }
}
