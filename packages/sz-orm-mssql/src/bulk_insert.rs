//! SQL Server 批量插入
//!
//! 提供 [`BulkInsert`] 用于构建高效的批量 INSERT 语句，支持
//! BULK INSERT、INSERT INTO ... VALUES 多行、表值参数等方式。

use std::fmt;

/// 批量插入策略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BulkInsertStrategy {
    /// 多行 VALUES（INSERT INTO t VALUES (...), (...), ...）
    MultiRowValues,
    /// BULK INSERT（从文件加载）
    BulkInsertFromFile,
    /// 表值参数（TVP）
    TableValuedParameter,
    /// MERGE 语句
    Merge,
}

impl BulkInsertStrategy {
    /// 返回描述
    #[must_use]
    pub fn description(&self) -> &'static str {
        match self {
            BulkInsertStrategy::MultiRowValues => "multi-row VALUES",
            BulkInsertStrategy::BulkInsertFromFile => "BULK INSERT from file",
            BulkInsertStrategy::TableValuedParameter => "table-valued parameter",
            BulkInsertStrategy::Merge => "MERGE statement",
        }
    }
}

impl fmt::Display for BulkInsertStrategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.description())
    }
}

/// BULK INSERT 选项
#[derive(Debug, Clone)]
pub struct BulkInsertOptions {
    /// 字段终止符（默认 ","）
    pub field_terminator: String,
    /// 行终止符（默认 "\n"）
    pub row_terminator: String,
    /// 是否第一行是标题
    pub first_row_is_header: bool,
    /// 数据文件代码页
    pub codepage: Option<String>,
    /// 错误文件路径
    pub error_file: Option<String>,
    /// 最大错误数
    pub max_errors: Option<u32>,
    /// 批次大小
    pub batch_size: Option<usize>,
    /// 行数限制
    pub rows_per_batch: Option<usize>,
    /// 是否按顺序排列的聚集索引
    pub order: Option<String>,
    /// 是否检查约束
    pub check_constraints: bool,
    /// 是否触发触发器
    pub fire_triggers: bool,
    /// 是否保持 NULL 值
    pub keep_nulls: bool,
    /// 是否使用 tablock
    pub tablock: bool,
}

impl Default for BulkInsertOptions {
    fn default() -> Self {
        Self {
            field_terminator: ",".to_string(),
            row_terminator: "\\n".to_string(),
            first_row_is_header: false,
            codepage: None,
            error_file: None,
            max_errors: None,
            batch_size: None,
            rows_per_batch: None,
            order: None,
            check_constraints: false,
            fire_triggers: false,
            keep_nulls: false,
            tablock: false,
        }
    }
}

impl BulkInsertOptions {
    /// 创建默认选项
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置字段终止符
    #[must_use]
    pub fn with_field_terminator(mut self, term: &str) -> Self {
        self.field_terminator = term.to_string();
        self
    }

    /// 设置行终止符
    #[must_use]
    pub fn with_row_terminator(mut self, term: &str) -> Self {
        self.row_terminator = term.to_string();
        self
    }

    /// 启用首行标题
    #[must_use]
    pub fn with_header(mut self) -> Self {
        self.first_row_is_header = true;
        self
    }

    /// 设置代码页
    #[must_use]
    pub fn with_codepage(mut self, cp: &str) -> Self {
        self.codepage = Some(cp.to_string());
        self
    }

    /// 设置错误文件
    #[must_use]
    pub fn with_error_file(mut self, path: &str) -> Self {
        self.error_file = Some(path.to_string());
        self
    }

    /// 设置最大错误数
    #[must_use]
    pub fn with_max_errors(mut self, n: u32) -> Self {
        self.max_errors = Some(n);
        self
    }

    /// 设置批次大小
    #[must_use]
    pub fn with_batch_size(mut self, n: usize) -> Self {
        self.batch_size = Some(n);
        self
    }

    /// 启用检查约束
    #[must_use]
    pub fn with_check_constraints(mut self) -> Self {
        self.check_constraints = true;
        self
    }

    /// 启用触发器
    #[must_use]
    pub fn with_fire_triggers(mut self) -> Self {
        self.fire_triggers = true;
        self
    }

    /// 启用保持 NULL
    #[must_use]
    pub fn with_keep_nulls(mut self) -> Self {
        self.keep_nulls = true;
        self
    }

    /// 启用 tablock
    #[must_use]
    pub fn with_tablock(mut self) -> Self {
        self.tablock = true;
        self
    }

    /// 生成 WITH 子句
    #[must_use]
    pub fn to_with_clause(&self) -> String {
        let mut opts = Vec::new();
        opts.push(format!("FIELDTERMINATOR = '{}'", self.field_terminator));
        opts.push(format!("ROWTERMINATOR = '{}'", self.row_terminator));
        if self.first_row_is_header {
            opts.push("FIRSTROW = 2".to_string());
        }
        if let Some(ref cp) = self.codepage {
            opts.push(format!("CODEPAGE = '{cp}'"));
        }
        if let Some(ref ef) = self.error_file {
            opts.push(format!("ERRORFILE = '{ef}'"));
        }
        if let Some(me) = self.max_errors {
            opts.push(format!("MAXERRORS = {me}"));
        }
        if let Some(bs) = self.batch_size {
            opts.push(format!("BATCHSIZE = {bs}"));
        }
        if let Some(rpb) = self.rows_per_batch {
            opts.push(format!("ROWS_PER_BATCH = {rpb}"));
        }
        if let Some(ref order) = self.order {
            opts.push(format!("ORDER ({order})"));
        }
        if self.check_constraints {
            opts.push("CHECK_CONSTRAINTS".to_string());
        }
        if self.fire_triggers {
            opts.push("FIRE_TRIGGERS".to_string());
        }
        if self.keep_nulls {
            opts.push("KEEPNULLS".to_string());
        }
        if self.tablock {
            opts.push("TABLOCK".to_string());
        }
        format!("WITH ({})", opts.join(", "))
    }
}

/// 批量插入构建器
#[derive(Debug, Clone)]
pub struct BulkInsert {
    /// 目标表名
    table: String,
    /// 列名列表
    columns: Vec<String>,
    /// 策略
    strategy: BulkInsertStrategy,
    /// 批量选项
    options: BulkInsertOptions,
    /// 数据行（仅 MultiRowValues 策略）
    rows: Vec<Vec<String>>,
    /// 数据文件路径（仅 BulkInsertFromFile 策略）
    data_file: Option<String>,
}

impl BulkInsert {
    /// 创建多行 VALUES 批量插入
    #[must_use]
    pub fn multi_row(table: &str) -> Self {
        Self {
            table: table.to_string(),
            columns: Vec::new(),
            strategy: BulkInsertStrategy::MultiRowValues,
            options: BulkInsertOptions::default(),
            rows: Vec::new(),
            data_file: None,
        }
    }

    /// 创建 BULK INSERT 构建器
    #[must_use]
    pub fn from_file(table: &str, file_path: &str) -> Self {
        Self {
            table: table.to_string(),
            columns: Vec::new(),
            strategy: BulkInsertStrategy::BulkInsertFromFile,
            options: BulkInsertOptions::default(),
            rows: Vec::new(),
            data_file: Some(file_path.to_string()),
        }
    }

    /// 设置列名
    #[must_use]
    pub fn columns(mut self, cols: &[&str]) -> Self {
        self.columns = cols.iter().map(|s| s.to_string()).collect();
        self
    }

    /// 设置批量选项
    #[must_use]
    pub fn with_options(mut self, options: BulkInsertOptions) -> Self {
        self.options = options;
        self
    }

    /// 添加一行数据（仅 MultiRowValues）
    #[must_use]
    pub fn add_row(mut self, values: Vec<String>) -> Self {
        self.rows.push(values);
        self
    }

    /// 生成 SQL 语句
    #[must_use]
    pub fn build(&self) -> String {
        match self.strategy {
            BulkInsertStrategy::MultiRowValues => self.build_multi_row(),
            BulkInsertStrategy::BulkInsertFromFile => self.build_bulk_insert(),
            BulkInsertStrategy::TableValuedParameter => self.build_tvp(),
            BulkInsertStrategy::Merge => self.build_merge(),
        }
    }

    /// 生成多行 VALUES 语句
    fn build_multi_row(&self) -> String {
        if self.columns.is_empty() || self.rows.is_empty() {
            return String::new();
        }
        let cols = self.columns.join(", ");
        let values: Vec<String> = self
            .rows
            .iter()
            .map(|row| {
                let vals: Vec<String> = row.to_vec();
                format!("({})", vals.join(", "))
            })
            .collect();
        format!(
            "INSERT INTO {} ({}) VALUES {}",
            self.table,
            cols,
            values.join(", ")
        )
    }

    /// 生成 BULK INSERT 语句
    fn build_bulk_insert(&self) -> String {
        let file = self.data_file.as_deref().unwrap_or("");
        let with_clause = self.options.to_with_clause();
        format!("BULK INSERT {} FROM '{}' {}", self.table, file, with_clause)
    }

    /// 生成 TVP INSERT 语句
    fn build_tvp(&self) -> String {
        if self.columns.is_empty() {
            return String::new();
        }
        let cols = self.columns.join(", ");
        let placeholders: Vec<String> = self.columns.iter().map(|c| format!("@{c}")).collect();
        format!(
            "INSERT INTO {} ({}) SELECT {} FROM @tvp",
            self.table,
            cols,
            placeholders.join(", ")
        )
    }

    /// 生成 MERGE 语句
    fn build_merge(&self) -> String {
        if self.columns.is_empty() {
            return String::new();
        }
        let set_clause: Vec<String> = self
            .columns
            .iter()
            .map(|c| format!("t.{c} = s.{c}"))
            .collect();
        let insert_cols = self.columns.join(", ");
        let insert_vals: Vec<String> = self.columns.iter().map(|c| format!("s.{c}")).collect();
        format!(
            "MERGE {} t USING @tvp s ON (t.id = s.id) WHEN MATCHED THEN UPDATE SET {} WHEN NOT MATCHED THEN INSERT ({}) VALUES ({})",
            self.table,
            set_clause.join(", "),
            insert_cols,
            insert_vals.join(", ")
        )
    }

    /// 获取策略
    #[must_use]
    pub fn strategy(&self) -> BulkInsertStrategy {
        self.strategy
    }

    /// 获取表名
    #[must_use]
    pub fn table(&self) -> &str {
        &self.table
    }

    /// 获取列数
    #[must_use]
    pub fn column_count(&self) -> usize {
        self.columns.len()
    }

    /// 获取行数
    #[must_use]
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// 估算内存占用（字节）
    #[must_use]
    pub fn estimated_memory_bytes(&self) -> u64 {
        const PER_CELL_BYTES: u64 = 32;
        self.rows.len() as u64 * self.columns.len() as u64 * PER_CELL_BYTES
    }
}

impl fmt::Display for BulkInsert {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.build())
    }
}

/// 批量插入结果
#[derive(Debug, Clone, Default)]
pub struct BulkInsertResult {
    /// 插入行数
    pub rows_inserted: u64,
    /// 复制行数
    pub rows_copied: u64,
    /// 错误行数
    pub error_rows: u64,
    /// 耗时（毫秒）
    pub elapsed_ms: u64,
    /// 批次数
    pub batches: usize,
}

impl BulkInsertResult {
    /// 创建新结果
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 是否有错误
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.error_rows > 0
    }

    /// 错误率
    #[must_use]
    pub fn error_rate(&self) -> f64 {
        let total = self.rows_inserted + self.error_rows;
        if total == 0 {
            0.0
        } else {
            self.error_rows as f64 / total as f64
        }
    }

    /// 吞吐量（行/秒）
    #[must_use]
    pub fn throughput(&self) -> f64 {
        if self.elapsed_ms == 0 {
            0.0
        } else {
            self.rows_copied as f64 / (self.elapsed_ms as f64 / 1000.0)
        }
    }

    /// 平均每批行数
    #[must_use]
    pub fn avg_batch_size(&self) -> f64 {
        if self.batches == 0 {
            0.0
        } else {
            self.rows_copied as f64 / self.batches as f64
        }
    }
}

impl fmt::Display for BulkInsertResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "BulkInsertResult(inserted={}, copied={}, errors={}, elapsed={}ms)",
            self.rows_inserted, self.rows_copied, self.error_rows, self.elapsed_ms
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bulk_insert_strategy_description() {
        assert_eq!(
            BulkInsertStrategy::MultiRowValues.description(),
            "multi-row VALUES"
        );
        assert_eq!(
            BulkInsertStrategy::BulkInsertFromFile.description(),
            "BULK INSERT from file"
        );
    }

    #[test]
    fn test_bulk_insert_options_default() {
        let opts = BulkInsertOptions::default();
        assert_eq!(opts.field_terminator, ",");
        assert!(!opts.check_constraints);
    }

    #[test]
    fn test_bulk_insert_options_builder() {
        let opts = BulkInsertOptions::new()
            .with_field_terminator("|")
            .with_row_terminator("\\r\\n")
            .with_header()
            .with_check_constraints()
            .with_tablock();
        assert_eq!(opts.field_terminator, "|");
        assert!(opts.first_row_is_header);
        assert!(opts.check_constraints);
        assert!(opts.tablock);
    }

    #[test]
    fn test_bulk_insert_options_to_with_clause() {
        let opts = BulkInsertOptions::new()
            .with_field_terminator(",")
            .with_row_terminator("\\n");
        let clause = opts.to_with_clause();
        assert!(clause.contains("FIELDTERMINATOR = ','"));
        assert!(clause.contains("ROWTERMINATOR = '\\n'"));
    }

    #[test]
    fn test_bulk_insert_options_with_header_clause() {
        let opts = BulkInsertOptions::new().with_header();
        let clause = opts.to_with_clause();
        assert!(clause.contains("FIRSTROW = 2"));
    }

    #[test]
    fn test_bulk_insert_options_with_codepage() {
        let opts = BulkInsertOptions::new().with_codepage("65001");
        let clause = opts.to_with_clause();
        assert!(clause.contains("CODEPAGE = '65001'"));
    }

    #[test]
    fn test_bulk_insert_options_with_batch_size() {
        let opts = BulkInsertOptions::new().with_batch_size(1000);
        let clause = opts.to_with_clause();
        assert!(clause.contains("BATCHSIZE = 1000"));
    }

    #[test]
    fn test_bulk_insert_multi_row() {
        let bi = BulkInsert::multi_row("users")
            .columns(&["id", "name"])
            .add_row(vec!["1".to_string(), "'Alice'".to_string()])
            .add_row(vec!["2".to_string(), "'Bob'".to_string()]);
        let sql = bi.build();
        assert!(sql.contains("INSERT INTO users (id, name) VALUES"));
        assert!(sql.contains("(1, 'Alice')"));
        assert!(sql.contains("(2, 'Bob')"));
    }

    #[test]
    fn test_bulk_insert_multi_row_empty() {
        let bi = BulkInsert::multi_row("users").columns(&["id"]);
        let sql = bi.build();
        assert!(sql.is_empty());
    }

    #[test]
    fn test_bulk_insert_from_file() {
        let bi = BulkInsert::from_file("users", "C:/data/users.csv")
            .with_options(BulkInsertOptions::new().with_field_terminator(","));
        let sql = bi.build();
        assert!(sql.contains("BULK INSERT users"));
        assert!(sql.contains("C:/data/users.csv"));
        assert!(sql.contains("FIELDTERMINATOR = ','"));
    }

    #[test]
    fn test_bulk_insert_strategy() {
        let bi = BulkInsert::multi_row("users");
        assert_eq!(bi.strategy(), BulkInsertStrategy::MultiRowValues);
    }

    #[test]
    fn test_bulk_insert_table() {
        let bi = BulkInsert::multi_row("users");
        assert_eq!(bi.table(), "users");
    }

    #[test]
    fn test_bulk_insert_column_count() {
        let bi = BulkInsert::multi_row("users").columns(&["a", "b", "c"]);
        assert_eq!(bi.column_count(), 3);
    }

    #[test]
    fn test_bulk_insert_row_count() {
        let bi = BulkInsert::multi_row("users")
            .columns(&["id"])
            .add_row(vec!["1".to_string()])
            .add_row(vec!["2".to_string()]);
        assert_eq!(bi.row_count(), 2);
    }

    #[test]
    fn test_bulk_insert_estimated_memory() {
        let bi = BulkInsert::multi_row("users")
            .columns(&["a", "b"])
            .add_row(vec!["1".to_string(), "2".to_string()]);
        assert_eq!(bi.estimated_memory_bytes(), 2 * 32);
    }

    #[test]
    fn test_bulk_insert_display() {
        let bi = BulkInsert::multi_row("users")
            .columns(&["id"])
            .add_row(vec!["1".to_string()]);
        let s = format!("{}", bi);
        assert!(s.contains("INSERT INTO users"));
    }

    #[test]
    fn test_bulk_insert_result_default() {
        let r = BulkInsertResult::default();
        assert_eq!(r.rows_inserted, 0);
        assert!(!r.has_errors());
    }

    #[test]
    fn test_bulk_insert_result_has_errors() {
        let r = BulkInsertResult {
            error_rows: 5,
            ..BulkInsertResult::default()
        };
        assert!(r.has_errors());
    }

    #[test]
    fn test_bulk_insert_result_error_rate() {
        let r = BulkInsertResult {
            rows_inserted: 95,
            error_rows: 5,
            ..BulkInsertResult::default()
        };
        let rate = r.error_rate();
        assert!((rate - 0.05).abs() < 1e-10);
    }

    #[test]
    fn test_bulk_insert_result_throughput() {
        let r = BulkInsertResult {
            rows_copied: 1000,
            elapsed_ms: 500,
            ..BulkInsertResult::default()
        };
        let t = r.throughput();
        assert!((t - 2000.0).abs() < 1e-10);
    }

    #[test]
    fn test_bulk_insert_result_avg_batch_size() {
        let r = BulkInsertResult {
            rows_copied: 500,
            batches: 5,
            ..BulkInsertResult::default()
        };
        assert!((r.avg_batch_size() - 100.0).abs() < 1e-10);
    }

    #[test]
    fn test_bulk_insert_result_display() {
        let r = BulkInsertResult {
            rows_inserted: 100,
            rows_copied: 100,
            error_rows: 0,
            elapsed_ms: 50,
            ..BulkInsertResult::default()
        };
        let s = format!("{}", r);
        assert!(s.contains("inserted=100"));
        assert!(s.contains("elapsed=50ms"));
    }
}
