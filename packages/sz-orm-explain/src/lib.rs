//! # sz-orm-explain — 跨方言 EXPLAIN 结果解析器
//!
//! 将 MySQL / PostgreSQL / SQLite / Oracle / MSSQL 的 `EXPLAIN` 输出解析为
//! 统一的 [`ExplainPlan`]，用于：
//!
//! - 全表扫描检测（`ScanType::FullTable`）
//! - 缺失索引检测（`index == None` 且扫描行数超阈值）
//! - 执行计划回归检测（[`crate::regression`]，需 `explain-analyzer` feature）
//!
//! 典型用法（配合 `sz-orm-macros` 的 `query!` 宏 db-verify 模式）：
//!
//! ```rust,ignore
//! let plan = sz_orm_explain::parser_for(ExplainDialect::Postgres)?.parse(explain_raw)?;
//! if plan.scan_type == ScanType::FullTable {
//!     // 编译期/CI 警告：全表扫描
//! }
//! ```
//!
//! 解析失败返回 [`ExplainError::Unparseable`]，不 panic，可降级为"无警告"。
//!
//! 注意：本包刻意零依赖 `sz-orm-core`，通过自带的 [`ExplainDialect`] 枚举
//! 描述方言，避免 core → macros → explain → core 依赖循环。

#[cfg(feature = "explain-analyzer")]
pub mod analyzer;
pub mod dialect;
pub mod parsers;
#[cfg(feature = "perf-baseline")]
pub mod perf_baseline;
#[cfg(feature = "explain-analyzer")]
pub mod regression;

/// 支持的 EXPLAIN 解析方言（与数据库引擎对应，家族内格式兼容）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplainDialect {
    /// MySQL（及 MariaDB/TiDB/PolarDB/OceanBase）
    MySql,
    /// PostgreSQL（及 CockroachDB/YugabyteDB）
    Postgres,
    /// SQLite（及 DuckDB）
    Sqlite,
    /// Oracle（及 Dameng）
    Oracle,
    /// SQL Server（及 Sybase）
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

/// 扫描类型分类（各方言扫描方式的统一抽象）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "explain-analyzer",
    derive(serde::Serialize, serde::Deserialize)
)]
pub enum ScanType {
    /// 全表扫描（MySQL `ALL` / PG `Seq Scan` / SQLite `SCAN` / Oracle `TABLE ACCESS FULL` / MSSQL `Table Scan`）
    FullTable,
    /// 索引范围扫描（MySQL `range` / PG `Index Scan` / SQLite `SEARCH ... USING INDEX` / Oracle `INDEX RANGE SCAN` / MSSQL `Index Scan`）
    IndexRange,
    /// 索引等值/点查（MySQL `ref`/`eq_ref` / Oracle `INDEX UNIQUE SCAN`）
    IndexLookup,
    /// 主键/唯一索引点查（MySQL `const` / PG `Index Scan ... pkey` / MSSQL `Index Seek`）
    UniqueLookup,
    /// 其他（物化、排序、聚合等）
    Other,
}

impl ScanType {
    /// 是否为全表扫描（性能警告触发条件）
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

/// 解析后的执行计划摘要
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    feature = "explain-analyzer",
    derive(serde::Serialize, serde::Deserialize)
)]
pub struct ExplainPlan {
    /// 扫描类型
    pub scan_type: ScanType,
    /// 涉及的表名（未知时为 `"<unknown>"`）
    pub table: String,
    /// 使用的索引名（全表扫描/未知时为 `None`）
    pub index: Option<String>,
    /// 预估扫描行数
    pub rows: u64,
    /// 额外信息（filter/using where 等）
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

    /// 缺失索引判断：全表扫描或行数超阈值且未使用索引
    pub fn missing_index(&self, row_threshold: u64) -> bool {
        self.index.is_none() && (self.scan_type.is_full_table_scan() || self.rows >= row_threshold)
    }
}

/// EXPLAIN 解析错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExplainError {
    /// 无法解析（非 EXPLAIN 输出 / 格式不识别）
    Unparseable { reason: String },
    /// 方言不受支持（无对应解析器）
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

/// 方言 EXPLAIN 解析器 trait
pub trait ExplainParser: Send + Sync + std::fmt::Debug {
    /// 解析原始 EXPLAIN 输出为统一 [`ExplainPlan`]
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

/// 按方言分派解析器
pub fn parser_for(dialect: ExplainDialect) -> Result<Box<dyn ExplainParser>, ExplainError> {
    match dialect {
        ExplainDialect::MySql => Ok(Box::new(parsers::mysql::MySqlParser)),
        ExplainDialect::Postgres => Ok(Box::new(parsers::postgres::PostgresParser)),
        ExplainDialect::Sqlite => Ok(Box::new(parsers::sqlite::SqliteParser)),
        ExplainDialect::Oracle => Ok(Box::new(parsers::oracle::OracleParser)),
        ExplainDialect::Mssql => Ok(Box::new(parsers::mssql::MssqlParser)),
    }
}

/// 便捷解析函数：按方言解析 EXPLAIN 输出
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
