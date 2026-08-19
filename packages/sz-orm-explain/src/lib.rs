//! # sz-orm-explain — Cross-Dialect EXPLAIN Result Parser
//!
//! Parses `EXPLAIN` output from MySQL / PostgreSQL / SQLite / Oracle / MSSQL into
//! a unified [`ExplainPlan`], used for:
//!
//! - Full table scan detection (`ScanType::FullTable`)
//! - Missing index detection (`index == None` and scanned rows exceed threshold)
//! - Execution plan regression detection ([`crate::regression`], requires `explain-analyzer` feature)
//!
//! Typical usage (with `sz-orm-macros`'s `query!` macro db-verify mode):
//!
//! ```rust,ignore
//! let plan = sz_orm_explain::parser_for(ExplainDialect::Postgres)?.parse(explain_raw)?;
//! if plan.scan_type == ScanType::FullTable {
//!     // Compile-time/CI warning: full table scan
//! }
//! ```
//!
//! Parse failure returns [`ExplainError::Unparseable`], does not panic, can degrade to "no warnings".
//!
//! Note: This package intentionally has zero dependency on `sz-orm-core`, uses its own [`ExplainDialect`] enum
//! to describe dialects, avoiding core → macros → explain → core dependency cycle.

#[cfg(feature = "explain-analyzer")]
pub mod analyzer;
pub mod dialect;
pub mod parsers;
#[cfg(feature = "perf-baseline")]
pub mod perf_baseline;
#[cfg(feature = "explain-analyzer")]
pub mod regression;

/// Supported EXPLAIN parse dialects (corresponding to database engines, format compatible within family)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplainDialect {
    /// MySQL (and MariaDB/TiDB/PolarDB/OceanBase)
    MySql,
    /// PostgreSQL (and CockroachDB/YugabyteDB)
    Postgres,
    /// SQLite (and DuckDB)
    Sqlite,
    /// Oracle (and Dameng)
    Oracle,
    /// SQL Server (and Sybase)
    Mssql,
}

impl ExplainDialect {
    pub fn as_str(&self) -> &'static str {
        match self {
            ExplainDialect::MySql => "mysql",
            ExplainDialect::Postgres => "postgres",
            ExplainDialect::Sqlite => "sqlite",
            ExplainDialect::Oracle => "oracle",
            ExplainDialect::Mssql => "mssql",
        }
    }

    pub fn all() -> &'static [ExplainDialect] {
        &[
            ExplainDialect::MySql,
            ExplainDialect::Postgres,
            ExplainDialect::Sqlite,
            ExplainDialect::Oracle,
            ExplainDialect::Mssql,
        ]
    }

    pub fn parse_name(s: &str) -> Option<ExplainDialect> {
        match s.to_lowercase().as_str() {
            "mysql" => Some(ExplainDialect::MySql),
            "postgres" | "postgresql" => Some(ExplainDialect::Postgres),
            "sqlite" => Some(ExplainDialect::Sqlite),
            "oracle" => Some(ExplainDialect::Oracle),
            "mssql" | "sqlserver" => Some(ExplainDialect::Mssql),
            _ => None,
        }
    }
}

/// Scan type classification (unified abstraction of scan methods across dialects)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "explain-analyzer",
    derive(serde::Serialize, serde::Deserialize)
)]
pub enum ScanType {
    /// Full table scan (MySQL `ALL` / PG `Seq Scan` / SQLite `SCAN` / Oracle `TABLE ACCESS FULL` / MSSQL `Table Scan`)
    FullTable,
    /// Index range scan (MySQL `range` / PG `Index Scan` / SQLite `SEARCH ... USING INDEX` / Oracle `INDEX RANGE SCAN` / MSSQL `Index Scan`)
    IndexRange,
    /// Index equality/point lookup (MySQL `ref`/`eq_ref` / Oracle `INDEX UNIQUE SCAN`)
    IndexLookup,
    /// Primary key/unique index point lookup (MySQL `const` / PG `Index Scan ... pkey` / MSSQL `Index Seek`)
    UniqueLookup,
    /// Other (materialization, sorting, aggregation, etc.)
    Other,
}

impl ScanType {
    /// Whether it is a full table scan (performance warning trigger condition)
    pub fn is_full_table_scan(&self) -> bool {
        matches!(self, ScanType::FullTable)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ScanType::FullTable => "full-table",
            ScanType::IndexRange => "index-range",
            ScanType::IndexLookup => "index-lookup",
            ScanType::UniqueLookup => "unique-lookup",
            ScanType::Other => "other",
        }
    }

    pub fn all() -> &'static [ScanType] {
        &[
            ScanType::FullTable,
            ScanType::IndexRange,
            ScanType::IndexLookup,
            ScanType::UniqueLookup,
            ScanType::Other,
        ]
    }

    pub fn is_index_scan(&self) -> bool {
        matches!(
            self,
            ScanType::IndexRange | ScanType::IndexLookup | ScanType::UniqueLookup
        )
    }

    pub fn is_index_lookup(&self) -> bool {
        matches!(self, ScanType::IndexLookup | ScanType::UniqueLookup)
    }

    pub fn parse_name(s: &str) -> Option<ScanType> {
        match s.to_lowercase().as_str() {
            "full-table" | "full" | "seq" => Some(ScanType::FullTable),
            "index-range" | "range" => Some(ScanType::IndexRange),
            "index-lookup" | "ref" => Some(ScanType::IndexLookup),
            "unique-lookup" | "const" | "unique" => Some(ScanType::UniqueLookup),
            "other" => Some(ScanType::Other),
            _ => None,
        }
    }
}

/// Parsed execution plan summary
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    feature = "explain-analyzer",
    derive(serde::Serialize, serde::Deserialize)
)]
pub struct ExplainPlan {
    /// Scan type
    pub scan_type: ScanType,
    /// Involved table name (`"<unknown>"` when unknown)
    pub table: String,
    /// Index name used (`None` for full table scan/unknown)
    pub index: Option<String>,
    /// Estimated scanned row count
    pub rows: u64,
    /// Extra info (filter/using where, etc.)
    pub extra: Vec<String>,
}

impl ExplainPlan {
    pub fn new(
        scan_type: ScanType,
        table: impl Into<String>,
        index: Option<String>,
        rows: u64,
        extra: Vec<String>,
    ) -> Self {
        Self {
            scan_type,
            table: table.into(),
            index,
            rows,
            extra,
        }
    }

    pub fn is_full_table_scan(&self) -> bool {
        self.scan_type.is_full_table_scan()
    }

    pub fn has_index(&self) -> bool {
        self.index.is_some()
    }

    pub fn is_large_result(&self, threshold: u64) -> bool {
        self.rows >= threshold
    }

    pub fn table_name(&self) -> &str {
        &self.table
    }

    pub fn index_name(&self) -> Option<&str> {
        self.index.as_deref()
    }

    pub fn estimated_rows(&self) -> u64 {
        self.rows
    }

    pub fn extra_info(&self) -> &[String] {
        &self.extra
    }

    pub fn has_extra(&self, keyword: &str) -> bool {
        self.extra.iter().any(|e| e.contains(keyword))
    }

    pub fn summary(&self) -> String {
        let idx = self.index.as_deref().unwrap_or("none");
        format!(
            "scan={} table={} index={} rows={}",
            self.scan_type.as_str(),
            self.table,
            idx,
            self.rows
        )
    }

    /// Missing index check: full table scan or row count exceeds threshold and no index used
    pub fn missing_index(&self, row_threshold: u64) -> bool {
        self.index.is_none() && (self.scan_type.is_full_table_scan() || self.rows >= row_threshold)
    }
}

/// EXPLAIN parse error
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExplainError {
    /// Unparseable (not EXPLAIN output / format not recognized)
    Unparseable { reason: String },
    /// Dialect not supported (no corresponding parser)
    UnsupportedDialect,
}

impl std::fmt::Display for ExplainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExplainError::Unparseable { reason } => {
                write!(f, "unparseable explain output: {reason}")
            }
            ExplainError::UnsupportedDialect => {
                write!(f, "unsupported dialect for explain parsing")
            }
        }
    }
}

impl std::error::Error for ExplainError {}

impl ExplainError {
    pub fn is_unparseable(&self) -> bool {
        matches!(self, ExplainError::Unparseable { .. })
    }

    pub fn is_unsupported_dialect(&self) -> bool {
        matches!(self, ExplainError::UnsupportedDialect)
    }
}

/// Dialect EXPLAIN parser trait
pub trait ExplainParser: Send + Sync + std::fmt::Debug {
    /// Parse raw EXPLAIN output into unified [`ExplainPlan`]
    fn parse(&self, raw: &str) -> Result<ExplainPlan, ExplainError>;

    fn parse_or_default(&self, raw: &str) -> ExplainPlan {
        self.parse(raw).unwrap_or(ExplainPlan {
            scan_type: ScanType::Other,
            table: "<unknown>".into(),
            index: None,
            rows: 0,
            extra: Vec::new(),
        })
    }
}

/// Dispatch parser by dialect
pub fn parser_for(dialect: ExplainDialect) -> Result<Box<dyn ExplainParser>, ExplainError> {
    match dialect {
        ExplainDialect::MySql => Ok(Box::new(parsers::mysql::MySqlParser)),
        ExplainDialect::Postgres => Ok(Box::new(parsers::postgres::PostgresParser)),
        ExplainDialect::Sqlite => Ok(Box::new(parsers::sqlite::SqliteParser)),
        ExplainDialect::Oracle => Ok(Box::new(parsers::oracle::OracleParser)),
        ExplainDialect::Mssql => Ok(Box::new(parsers::mssql::MssqlParser)),
    }
}

/// Convenience parse function: parse EXPLAIN output by dialect
pub fn parse_explain(dialect: ExplainDialect, raw: &str) -> Result<ExplainPlan, ExplainError> {
    let parser = parser_for(dialect)?;
    parser.parse(raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dialect_and_scan_type_parse_name() {
        assert_eq!(
            ExplainDialect::parse_name("mysql"),
            Some(ExplainDialect::MySql)
        );
        assert_eq!(
            ExplainDialect::parse_name("postgresql"),
            Some(ExplainDialect::Postgres)
        );
        assert_eq!(ExplainDialect::parse_name("unknown"), None);
        assert_eq!(ExplainDialect::all().len(), 5);

        assert_eq!(ScanType::parse_name("full"), Some(ScanType::FullTable));
        assert_eq!(ScanType::parse_name("range"), Some(ScanType::IndexRange));
        assert_eq!(ScanType::parse_name("invalid"), None);
        assert_eq!(ScanType::all().len(), 5);
        assert!(ScanType::IndexRange.is_index_scan());
        assert!(ScanType::UniqueLookup.is_index_lookup());
        assert!(!ScanType::FullTable.is_index_scan());
    }

    #[test]
    fn test_explain_plan_helpers() {
        let plan = ExplainPlan::new(
            ScanType::FullTable,
            "users",
            None,
            50000,
            vec!["Using where".to_string()],
        );
        assert!(plan.is_full_table_scan());
        assert!(!plan.has_index());
        assert!(plan.is_large_result(1000));
        assert_eq!(plan.table_name(), "users");
        assert!(plan.index_name().is_none());
        assert_eq!(plan.estimated_rows(), 50000);
        assert!(plan.has_extra("Using"));
        assert!(!plan.has_extra("Filesort"));
        assert!(plan.summary().contains("full-table"));
        assert!(plan.missing_index(1000));

        let indexed = ExplainPlan::new(
            ScanType::IndexLookup,
            "orders",
            Some("idx_user".to_string()),
            10,
            vec![],
        );
        assert!(!indexed.is_full_table_scan());
        assert!(indexed.has_index());
        assert_eq!(indexed.index_name(), Some("idx_user"));
        assert!(!indexed.missing_index(1000));
    }

    #[test]
    fn test_explain_error_helpers() {
        let err = ExplainError::Unparseable {
            reason: "bad format".into(),
        };
        assert!(err.is_unparseable());
        assert!(!err.is_unsupported_dialect());

        let err2 = ExplainError::UnsupportedDialect;
        assert!(!err2.is_unparseable());
        assert!(err2.is_unsupported_dialect());
    }
}
