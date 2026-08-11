//! 方言分派辅助
//!
//! 提供 [`ExplainDialect`] → 解析器的映射与便捷入口，
//! 核心分派逻辑在 [`crate::parser_for`]。

use crate::{ExplainDialect, ExplainError, ExplainParser, ExplainPlan};

/// 便捷入口：按 [`ExplainDialect`] 解析 EXPLAIN 输出
///
/// ```rust
/// use sz_orm_explain::dialect::parse_explain;
/// use sz_orm_explain::ExplainDialect;
///
/// let raw = "Seq Scan on users  (cost=0.00..21.50 rows=1150 width=36)";
/// let plan = parse_explain(ExplainDialect::Postgres, raw).unwrap();
/// assert_eq!(plan.scan_type, sz_orm_explain::ScanType::FullTable);
/// ```
pub fn parse_explain(dialect: ExplainDialect, raw: &str) -> Result<ExplainPlan, ExplainError> {
    crate::parser_for(dialect)?.parse(raw)
}

/// 获取解析器并返回 trait 对象（供宏/CI 复用）
pub fn get_parser(dialect: ExplainDialect) -> Result<Box<dyn ExplainParser>, ExplainError> {
    crate::parser_for(dialect)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ExplainDialect, ScanType};

    #[test]
    fn mysql_parses() {
        let plan = parse_explain(
            ExplainDialect::MySql,
            "| 1 | SIMPLE | users | ALL | NULL | NULL | NULL | NULL | 1150 | Using where |",
        )
        .unwrap();
        assert_eq!(plan.scan_type, ScanType::FullTable);
        assert_eq!(plan.table, "users");
    }

    #[test]
    fn postgres_parses() {
        let plan = parse_explain(
            ExplainDialect::Postgres,
            "Index Scan using t_pkey on t  (cost=0.29..8.31 rows=1 width=36)",
        )
        .unwrap();
        assert_eq!(plan.scan_type, ScanType::IndexRange);
        assert_eq!(plan.index.as_deref(), Some("t_pkey"));
    }

    #[test]
    fn sqlite_parses() {
        let plan = parse_explain(ExplainDialect::Sqlite, "QUERY PLAN\n|--SCAN users").unwrap();
        assert_eq!(plan.scan_type, ScanType::FullTable);
    }

    #[test]
    fn oracle_parses() {
        let plan = parse_explain(
            ExplainDialect::Oracle,
            "| 1 |  TABLE ACCESS FULL | USERS | 1150 | 40250 | 21 |",
        )
        .unwrap();
        assert_eq!(plan.scan_type, ScanType::FullTable);
    }

    #[test]
    fn mssql_parses() {
        let plan = parse_explain(
            ExplainDialect::Mssql,
            "|--Table Scan(OBJECT:([db].[dbo].[users]))",
        )
        .unwrap();
        assert_eq!(plan.scan_type, ScanType::FullTable);
    }

    #[test]
    fn all_dialects_have_parser() {
        for d in [
            ExplainDialect::MySql,
            ExplainDialect::Postgres,
            ExplainDialect::Sqlite,
            ExplainDialect::Oracle,
            ExplainDialect::Mssql,
        ] {
            assert!(crate::parser_for(d).is_ok(), "missing parser for {d:?}");
        }
    }
}
