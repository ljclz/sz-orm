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

/// 方言 EXPLAIN 解析器 trait
pub trait ExplainParser: Send + Sync + std::fmt::Debug {
    /// 解析原始 EXPLAIN 输出为统一 [`ExplainPlan`]
    fn parse(&self, raw: &str) -> Result<ExplainPlan, ExplainError>;
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
