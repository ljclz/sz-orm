//! Oracle EXPLAIN PLAN 解析器
//!
//! 输入示例（`EXPLAIN PLAN FOR ...` 后查询 `PLAN_TABLE`/`DBMS_XPLAN.DISPLAY` 输出）：
//!
//! ```text
//! Plan hash value: 1234567890
//!
//! ---------------------------------------------------------------------------
//! | Id  | Operation                    | Name      | Rows  | Bytes | Cost   |
//! ---------------------------------------------------------------------------
//! |   0 | SELECT STATEMENT             |           |     1 |    38 |     3  |
//! |   1 |  TABLE ACCESS BY INDEX ROWID | USERS     |     1 |    38 |     2  |
//! |*  2 |   INDEX UNIQUE SCAN          | USERS_PK  |     1 |       |     1  |
//! ---------------------------------------------------------------------------
//! ```

use crate::{ExplainError, ExplainParser, ExplainPlan, ScanType};

/// Oracle EXPLAIN 解析器
#[derive(Debug)]
pub struct OracleParser;

impl ExplainParser for OracleParser {
    fn parse(&self, raw: &str) -> Result<ExplainPlan, ExplainError> {
        let mut best: Option<(ScanType, String, Option<String>, u64)> = None;
        // 优先级：FullTable > IndexRange > UniqueLookup > IndexLookup > Other
        let rank = |s: ScanType| match s {
            ScanType::FullTable => 5,
            ScanType::IndexRange => 4,
            ScanType::UniqueLookup => 3,
            ScanType::IndexLookup => 2,
            ScanType::Other => 1,
        };

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
            if cells.len() < 2 {
                continue;
            }
            if cells.iter().any(|c| c.eq_ignore_ascii_case("Operation")) {
                continue; // 表头
            }
            let operation = cells
                .get(1)
                .map(|s| s.to_ascii_uppercase())
                .unwrap_or_default();
            let name = cells.get(2).cloned().unwrap_or_default();
            let rows = cells
                .get(3)
                .and_then(|s| crate::parsers::parse_u64(s))
                .unwrap_or(0);

            let candidate: Option<(ScanType, String, Option<String>, u64)> =
                if operation.contains("TABLE ACCESS FULL") {
                    Some((ScanType::FullTable, name.clone(), None, rows))
                } else if operation.contains("INDEX RANGE SCAN") {
                    Some((
                        ScanType::IndexRange,
                        "<unknown>".into(),
                        Some(name.clone()),
                        rows,
                    ))
                } else if operation.contains("INDEX UNIQUE SCAN") {
                    Some((
                        ScanType::UniqueLookup,
                        "<unknown>".into(),
                        Some(name.clone()),
                        rows,
                    ))
                } else if operation.contains("INDEX") && operation.contains("SCAN") {
                    Some((
                        ScanType::IndexLookup,
                        "<unknown>".into(),
                        Some(name.clone()),
                        rows,
                    ))
                } else if operation.contains("TABLE ACCESS") {
                    Some((ScanType::Other, name.clone(), None, rows))
                } else {
                    None
                };
            if let Some(c) = candidate {
                if best.is_none() || rank(c.0) > rank(best.as_ref().unwrap().0) {
                    best = Some(c);
                }
            }
        }
        let (scan_type, table, index, rows) = best.ok_or_else(|| ExplainError::Unparseable {
            reason: "no recognizable operation row".into(),
        })?;
        Ok(ExplainPlan {
            scan_type,
            table,
            index,
            rows,
            extra: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL_SCAN: &str = "\
---------------------------------------------------------------------------
| Id  | Operation          | Name  | Rows  | Bytes | Cost (%CPU)| Time     |
---------------------------------------------------------------------------
|   0 | SELECT STATEMENT   |       |  1150 | 40250 |    22  (0)| 00:00:01 |
|   1 |  TABLE ACCESS FULL | USERS |  1150 | 40250 |    21  (0)| 00:00:01 |
---------------------------------------------------------------------------
";

    const PK_LOOKUP: &str = "\
---------------------------------------------------------------------------
| Id  | Operation                    | Name      | Rows  | Bytes | Cost   |
---------------------------------------------------------------------------
|   0 | SELECT STATEMENT             |           |     1 |    38 |     3  |
|   1 |  TABLE ACCESS BY INDEX ROWID | USERS     |     1 |    38 |     2  |
|*  2 |   INDEX UNIQUE SCAN          | USERS_PK  |     1 |       |     1  |
---------------------------------------------------------------------------
";

    const RANGE_SCAN: &str = "\
---------------------------------------------------------------------------
| Id  | Operation          | Name          | Rows  | Bytes | Cost   |
---------------------------------------------------------------------------
|   0 | SELECT STATEMENT   |               |    42 |  1512 |     4  |
|   1 |  TABLE ACCESS FULL | ORDERS        |    42 |  1512 |     3  |
|*  2 |   INDEX RANGE SCAN | IDX_ORDERS_UA |    42 |       |     1  |
---------------------------------------------------------------------------
";

    #[test]
    fn parses_full_table_access() {
        let plan = OracleParser.parse(FULL_SCAN).expect("parse ok");
        assert_eq!(plan.scan_type, ScanType::FullTable);
        assert_eq!(plan.table, "USERS");
        assert!(plan.index.is_none());
        assert_eq!(plan.rows, 1150);
        assert!(plan.missing_index(1000));
    }

    #[test]
    fn unique_scan_beats_rowid_access() {
        // 同一计划中含 TABLE ACCESS + INDEX UNIQUE SCAN，应选中 UniqueLookup
        let plan = OracleParser.parse(PK_LOOKUP).expect("parse ok");
        assert_eq!(plan.scan_type, ScanType::UniqueLookup);
        assert_eq!(plan.index.as_deref(), Some("USERS_PK"));
        assert_eq!(plan.table, "<unknown>");
    }

    #[test]
    fn full_scan_beats_range_scan() {
        // 同时出现 TABLE ACCESS FULL 与 INDEX RANGE SCAN 时（复杂计划），
        // 全表扫描是更严重的信号，优先报告
        let plan = OracleParser.parse(RANGE_SCAN).expect("parse ok");
        assert_eq!(plan.scan_type, ScanType::FullTable);
        assert_eq!(plan.table, "ORDERS");
    }

    #[test]
    fn rejects_unknown_input() {
        let err = OracleParser.parse("no plan here").unwrap_err();
        assert!(matches!(err, ExplainError::Unparseable { .. }));
    }
}
