//! MSSQL SHOWPLAN 解析器
//!
//! 输入示例（`SET SHOWPLAN_TEXT ON` 后执行查询的输出）：
//!
//! ```text
//! |--Table Scan(OBJECT:([db].[dbo].[users]), WHERE:([age]>(18)))
//! |--Index Seek(OBJECT:([db].[dbo].[users].[PK__users__3214EC07]), SEEK:([id]=@1) ORDERED FORWARD)
//! |--Index Scan(OBJECT:([db].[dbo].[orders].[idx_orders_user]))
//! |--Hash Match(Inner Join, HASH:([o].[user_id])=([u].[id]), RESIDUAL:...)
//! ```
//!
//! 注意：SHOWPLAN_TEXT 不输出预估行数，`rows` 固定为 0。

use crate::{ExplainError, ExplainParser, ExplainPlan, ScanType};

/// MSSQL EXPLAIN 解析器
#[derive(Debug)]
pub struct MssqlParser;

impl ExplainParser for MssqlParser {
    fn parse(&self, raw: &str) -> Result<ExplainPlan, ExplainError> {
        let mut best: Option<(ScanType, String, Option<String>)> = None;
        // 优先级：Table Scan > Index Scan > Index Seek > 其他
        let rank = |s: ScanType| match s {
            ScanType::FullTable => 4,
            ScanType::IndexRange => 3,
            ScanType::UniqueLookup => 2,
            _ => 1,
        };

        for line in raw.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with("StmtText") || line.starts_with("----") {
                continue;
            }
            // 去掉树形前缀 `|--`
            let body = line.strip_prefix("|--").unwrap_or(line).trim();
            let upper = body.to_ascii_uppercase();

            let candidate: Option<(ScanType, String, Option<String>)> =
                if upper.starts_with("TABLE SCAN(") || upper.starts_with("CLUSTERED INDEX SCAN(") {
                    Some((ScanType::FullTable, extract_object(body), None))
                } else if upper.starts_with("INDEX SCAN(") {
                    Some((
                        ScanType::IndexRange,
                        "<unknown>".into(),
                        Some(extract_object(body)),
                    ))
                } else if upper.starts_with("INDEX SEEK(")
                    || upper.starts_with("CLUSTERED INDEX SEEK(")
                {
                    Some((
                        ScanType::UniqueLookup,
                        "<unknown>".into(),
                        Some(extract_object(body)),
                    ))
                } else {
                    None
                };
            if let Some(c) = candidate {
                if best.is_none() || rank(c.0) > rank(best.as_ref().unwrap().0) {
                    best = Some(c);
                }
            }
        }
        let (scan_type, table, index) = best.ok_or_else(|| ExplainError::Unparseable {
            reason: "no recognizable plan operator".into(),
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

/// 提取 `OBJECT:([db].[dbo].[users])` 中的对象名（取最后一段）
fn extract_object(body: &str) -> String {
    let upper = body.to_ascii_uppercase();
    if let Some(pos) = upper.find("OBJECT:(") {
        let rest = &body[pos + 8..];
        let inner = rest.split(')').next().unwrap_or("");
        let trimmed = inner.trim_start_matches('[').trim_end_matches(']');
        // `[db].[dbo].[users]` → 取最后一段
        trimmed
            .split("].[")
            .last()
            .map(|s| s.trim_matches('[').trim_matches(']').to_string())
            .unwrap_or_else(|| trimmed.to_string())
    } else {
        "<unknown>".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLE_SCAN: &str = "\
|--Table Scan(OBJECT:([szdb].[dbo].[users]), WHERE:([age]>(18)))
";

    const INDEX_SEEK: &str = "\
|--Index Seek(OBJECT:([szdb].[dbo].[users].[PK__users__3214EC07]), SEEK:([id]=@1) ORDERED FORWARD)
";

    const INDEX_SCAN: &str = "\
|--Index Scan(OBJECT:([szdb].[dbo].[orders].[idx_orders_user]))
";

    const JOIN_PLAN: &str = "\
  |--Nested Loops(Inner Join, OUTER REFERENCES:([o].[user_id]))
       |--Index Seek(OBJECT:([szdb].[dbo].[users].[PK__users__3214EC07]), SEEK:([u].[id]=[o].[user_id]) ORDERED FORWARD)
       |--Table Scan(OBJECT:([szdb].[dbo].[orders]))
";

    #[test]
    fn parses_table_scan() {
        let plan = MssqlParser.parse(TABLE_SCAN).expect("parse ok");
        assert_eq!(plan.scan_type, ScanType::FullTable);
        assert_eq!(plan.table, "users");
        assert!(plan.index.is_none());
        assert_eq!(plan.rows, 0);
        assert!(plan.missing_index(0));
    }

    #[test]
    fn parses_index_seek() {
        let plan = MssqlParser.parse(INDEX_SEEK).expect("parse ok");
        assert_eq!(plan.scan_type, ScanType::UniqueLookup);
        assert!(plan.index.as_deref().unwrap_or("").contains("PK__users"));
    }

    #[test]
    fn parses_index_scan() {
        let plan = MssqlParser.parse(INDEX_SCAN).expect("parse ok");
        assert_eq!(plan.scan_type, ScanType::IndexRange);
        assert!(plan
            .index
            .as_deref()
            .unwrap_or("")
            .contains("idx_orders_user"));
    }

    #[test]
    fn full_scan_beats_seek_in_join() {
        // 连接计划中同时出现 Table Scan 与 Index Seek，优先报告全表扫描
        let plan = MssqlParser.parse(JOIN_PLAN).expect("parse ok");
        assert_eq!(plan.scan_type, ScanType::FullTable);
        assert_eq!(plan.table, "orders");
    }

    #[test]
    fn rejects_unknown_input() {
        let err = MssqlParser.parse("nothing here").unwrap_err();
        assert!(matches!(err, ExplainError::Unparseable { .. }));
    }
}
