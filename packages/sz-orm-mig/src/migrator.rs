use crate::error::MigError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigConfig {
    pub source_db: DatabaseConfig,
    pub target_db: DatabaseConfig,
    pub tables: Vec<String>,
    pub batch_size: usize,
    pub skip_errors: bool,
    pub dry_run: bool,
}

impl Default for MigConfig {
    fn default() -> Self {
        Self {
            source_db: DatabaseConfig::default(),
            target_db: DatabaseConfig::default(),
            tables: Vec::new(),
            batch_size: 1000,
            skip_errors: false,
            dry_run: false,
        }
    }
}

impl MigConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_source(mut self, config: DatabaseConfig) -> Self {
        self.source_db = config;
        self
    }

    pub fn with_target(mut self, config: DatabaseConfig) -> Self {
        self.target_db = config;
        self
    }

    pub fn with_tables(mut self, tables: Vec<String>) -> Self {
        self.tables = tables;
        self
    }

    pub fn with_batch_size(mut self, size: usize) -> Self {
        self.batch_size = size;
        self
    }

    pub fn with_skip_errors(mut self, skip: bool) -> Self {
        self.skip_errors = skip;
        self
    }

    pub fn with_dry_run(mut self, dry: bool) -> Self {
        self.dry_run = dry;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub db_type: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub database: String,
    pub charset: String,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            db_type: "mysql".to_string(),
            host: "localhost".to_string(),
            port: 3306,
            username: "root".to_string(),
            password: "".to_string(),
            database: "test".to_string(),
            charset: "utf8mb4".to_string(),
        }
    }
}

impl DatabaseConfig {
    pub fn mysql() -> Self {
        Self {
            db_type: "mysql".to_string(),
            port: 3306,
            charset: "utf8mb4".to_string(),
            ..Default::default()
        }
    }

    pub fn postgresql() -> Self {
        Self {
            db_type: "postgresql".to_string(),
            port: 5432,
            charset: "utf8".to_string(),
            ..Default::default()
        }
    }

    pub fn sqlite(path: &str) -> Self {
        Self {
            db_type: "sqlite".to_string(),
            port: 0,
            database: path.to_string(),
            ..Default::default()
        }
    }

    pub fn with_host(mut self, host: impl Into<String>) -> Self {
        self.host = host.into();
        self
    }

    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    pub fn with_username(mut self, username: impl Into<String>) -> Self {
        self.username = username.into();
        self
    }

    pub fn with_password(mut self, password: impl Into<String>) -> Self {
        self.password = password.into();
        self
    }

    pub fn with_database(mut self, database: impl Into<String>) -> Self {
        self.database = database.into();
        self
    }

    pub fn connection_string(&self) -> String {
        match self.db_type.as_str() {
            "mysql" => format!(
                "mysql://{}:{}@{}:{}/{}?charset={}",
                self.username, self.password, self.host, self.port, self.database, self.charset
            ),
            "postgresql" => format!(
                "postgres://{}:{}@{}:{}/{}",
                self.username, self.password, self.host, self.port, self.database
            ),
            "sqlite" => format!("sqlite:{}", self.database),
            _ => format!(
                "{}://{}:{}@{}:{}/{}",
                self.db_type, self.username, self.password, self.host, self.port, self.database
            ),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigReport {
    pub transaction_id: String,
    pub source_table: String,
    pub target_table: String,
    pub total_rows: u64,
    pub migrated_rows: u64,
    pub failed_rows: u64,
    pub duration_ms: u64,
    pub errors: Vec<String>,
}

impl MigReport {
    pub fn new(source_table: impl Into<String>, target_table: impl Into<String>) -> Self {
        Self {
            transaction_id: uuid_simple(),
            source_table: source_table.into(),
            target_table: target_table.into(),
            total_rows: 0,
            migrated_rows: 0,
            failed_rows: 0,
            duration_ms: 0,
            errors: Vec::new(),
        }
    }

    pub fn add_error(&mut self, error: impl Into<String>) {
        self.errors.push(error.into());
        self.failed_rows += 1;
    }

    pub fn success_rate(&self) -> f64 {
        if self.total_rows == 0 {
            return 0.0;
        }
        (self.migrated_rows as f64 / self.total_rows as f64) * 100.0
    }
}

fn uuid_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{:x}-{:x}", now, rand_simple())
}

fn rand_simple() -> u128 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    seed.wrapping_mul(1103515245).wrapping_add(12345) & 0x7fffffff
}

pub trait TableReader: Send + Sync {
    fn read_table(
        &self,
        table: &str,
        offset: u64,
        limit: u64,
    ) -> impl std::future::Future<Output = Result<Vec<RowData>, MigError>> + Send;
    fn count(&self, table: &str)
        -> impl std::future::Future<Output = Result<u64, MigError>> + Send;
    fn table_columns(
        &self,
        table: &str,
    ) -> impl std::future::Future<Output = Result<Vec<ColumnInfo>, MigError>> + Send;
}

pub trait TableWriter: Send + Sync {
    fn write_table(
        &self,
        table: &str,
        rows: Vec<RowData>,
    ) -> impl std::future::Future<Output = Result<u64, MigError>> + Send;
    fn create_table(
        &self,
        table: &str,
        columns: &[ColumnInfo],
    ) -> impl std::future::Future<Output = Result<(), MigError>> + Send;
}

/// An in-memory table store that implements both [`TableReader`] and
/// [`TableWriter`].
///
/// Intended for unit tests and dry-run migrations where a real database
/// connection is not available. Internally data is kept in
/// `Arc<RwLock<HashMap<table_name, InMemoryTable>>>` so that the returned
/// futures can own a cloned handle to the state (the traits use
/// `impl Future` in return position, which forbids borrowing `&self`).
#[derive(Debug, Clone, Default)]
pub struct InMemoryTableStore {
    tables: Arc<RwLock<HashMap<String, InMemoryTable>>>,
}

#[derive(Debug, Clone, Default)]
struct InMemoryTable {
    columns: Vec<ColumnInfo>,
    rows: Vec<RowData>,
}

impl InMemoryTableStore {
    /// Creates an empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Pre-populates the store with a table containing the given columns and
    /// rows. Overwrites any existing table with the same name.
    pub fn with_table(
        self,
        name: impl Into<String>,
        columns: Vec<ColumnInfo>,
        rows: Vec<RowData>,
    ) -> Self {
        {
            let mut tables = self
                .tables
                .write()
                .map_err(|e| MigError::Migration(e.to_string()))
                .expect("lock poisoned");
            tables.insert(name.into(), InMemoryTable { columns, rows });
        }
        self
    }

    /// Synchronously inserts a table (creating it if absent) and appends the
    /// provided rows. Returns the number of rows appended.
    pub fn append_rows(&self, table: &str, rows: Vec<RowData>) -> Result<u64, MigError> {
        let mut tables = self
            .tables
            .write()
            .map_err(|e| MigError::Migration(e.to_string()))?;
        let entry = tables.entry(table.to_string()).or_default();
        let written = rows.len() as u64;
        entry.rows.extend(rows);
        Ok(written)
    }

    /// Synchronously creates a table, replacing any existing schema for that
    /// name. Existing rows are preserved (their schema is updated).
    pub fn define_table(&self, table: &str, columns: Vec<ColumnInfo>) -> Result<(), MigError> {
        let mut tables = self
            .tables
            .write()
            .map_err(|e| MigError::Migration(e.to_string()))?;
        let entry = tables.entry(table.to_string()).or_default();
        entry.columns = columns;
        Ok(())
    }

    /// Returns the total number of rows currently stored in `table`, or an
    /// error if the table does not exist.
    pub fn row_count(&self, table: &str) -> Result<u64, MigError> {
        let tables = self
            .tables
            .read()
            .map_err(|e| MigError::Migration(e.to_string()))?;
        let t = tables
            .get(table)
            .ok_or_else(|| MigError::TableNotFound(table.to_string()))?;
        Ok(t.rows.len() as u64)
    }

    /// Returns a snapshot of the columns defined for `table`, or an error
    /// if the table does not exist.
    pub fn columns(&self, table: &str) -> Result<Vec<ColumnInfo>, MigError> {
        let tables = self
            .tables
            .read()
            .map_err(|e| MigError::Migration(e.to_string()))?;
        let t = tables
            .get(table)
            .ok_or_else(|| MigError::TableNotFound(table.to_string()))?;
        Ok(t.columns.clone())
    }
}

impl TableReader for InMemoryTableStore {
    fn read_table(
        &self,
        table: &str,
        offset: u64,
        limit: u64,
    ) -> impl std::future::Future<Output = Result<Vec<RowData>, MigError>> + Send {
        let tables = self.tables.clone();
        let table_name = table.to_string();
        async move {
            let tables = tables
                .read()
                .map_err(|e| MigError::Migration(e.to_string()))?;
            let t = tables
                .get(&table_name)
                .ok_or_else(|| MigError::TableNotFound(table_name.clone()))?;
            let rows = t
                .rows
                .iter()
                .skip(offset as usize)
                .take(limit as usize)
                .cloned()
                .collect();
            Ok(rows)
        }
    }

    fn count(
        &self,
        table: &str,
    ) -> impl std::future::Future<Output = Result<u64, MigError>> + Send {
        let tables = self.tables.clone();
        let table_name = table.to_string();
        async move {
            let tables = tables
                .read()
                .map_err(|e| MigError::Migration(e.to_string()))?;
            let t = tables
                .get(&table_name)
                .ok_or_else(|| MigError::TableNotFound(table_name.clone()))?;
            Ok(t.rows.len() as u64)
        }
    }

    fn table_columns(
        &self,
        table: &str,
    ) -> impl std::future::Future<Output = Result<Vec<ColumnInfo>, MigError>> + Send {
        let tables = self.tables.clone();
        let table_name = table.to_string();
        async move {
            let tables = tables
                .read()
                .map_err(|e| MigError::Migration(e.to_string()))?;
            let t = tables
                .get(&table_name)
                .ok_or_else(|| MigError::TableNotFound(table_name.clone()))?;
            Ok(t.columns.clone())
        }
    }
}

impl TableWriter for InMemoryTableStore {
    fn write_table(
        &self,
        table: &str,
        rows: Vec<RowData>,
    ) -> impl std::future::Future<Output = Result<u64, MigError>> + Send {
        let tables = self.tables.clone();
        let table_name = table.to_string();
        async move {
            let mut tables = tables
                .write()
                .map_err(|e| MigError::Migration(e.to_string()))?;
            let entry = tables.entry(table_name).or_default();
            let written = rows.len() as u64;
            entry.rows.extend(rows);
            Ok(written)
        }
    }

    fn create_table(
        &self,
        table: &str,
        columns: &[ColumnInfo],
    ) -> impl std::future::Future<Output = Result<(), MigError>> + Send {
        let tables = self.tables.clone();
        let table_name = table.to_string();
        let columns = columns.to_vec();
        async move {
            let mut tables = tables
                .write()
                .map_err(|e| MigError::Migration(e.to_string()))?;
            let entry = tables.entry(table_name).or_default();
            entry.columns = columns;
            Ok(())
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RowData {
    pub data: HashMap<String, serde_json::Value>,
}

impl RowData {
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
        }
    }

    pub fn with(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.data.insert(key.into(), value);
        self
    }

    pub fn get(&self, key: &str) -> Option<&serde_json::Value> {
        self.data.get(key)
    }
}

impl Default for RowData {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub default_value: Option<String>,
    pub is_primary_key: bool,
}

impl ColumnInfo {
    pub fn new(name: impl Into<String>, data_type: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            data_type: data_type.into(),
            nullable: true,
            default_value: None,
            is_primary_key: false,
        }
    }

    pub fn not_null(mut self) -> Self {
        self.nullable = false;
        self
    }

    pub fn nullable(mut self, nullable: bool) -> Self {
        self.nullable = nullable;
        self
    }

    pub fn primary_key(mut self) -> Self {
        self.is_primary_key = true;
        self
    }

    pub fn is_primary_key(mut self, is_pk: bool) -> Self {
        self.is_primary_key = is_pk;
        self
    }

    pub fn with_default(mut self, value: impl Into<String>) -> Self {
        self.default_value = Some(value.into());
        self
    }
}

pub struct DataMigrator<R, W>
where
    R: TableReader,
    W: TableWriter,
{
    reader: R,
    writer: W,
    config: MigConfig,
}

impl<R, W> DataMigrator<R, W>
where
    R: TableReader,
    W: TableWriter,
{
    pub fn new(reader: R, writer: W, config: MigConfig) -> Self {
        Self {
            reader,
            writer,
            config,
        }
    }

    pub async fn migrate(&self, table: &str) -> Result<MigReport, MigError> {
        let mut report = MigReport::new(table, table);
        let start_time = std::time::Instant::now();

        let _columns = self.reader.table_columns(table).await?;
        let total = self.reader.count(table).await?;
        report.total_rows = total;

        if self.config.dry_run {
            report.migrated_rows = total;
            report.duration_ms = start_time.elapsed().as_millis() as u64;
            return Ok(report);
        }

        let batch_size = self.config.batch_size;
        let mut offset = 0u64;

        while offset < total {
            let rows = self
                .reader
                .read_table(table, offset, batch_size as u64)
                .await?;

            if rows.is_empty() {
                break;
            }

            match self.writer.write_table(table, rows).await {
                Ok(written) => {
                    report.migrated_rows += written;
                }
                Err(e) => {
                    if self.config.skip_errors {
                        report.add_error(e.to_string());
                    } else {
                        return Err(e);
                    }
                }
            }

            offset += batch_size as u64;
        }

        report.duration_ms = start_time.elapsed().as_millis() as u64;
        Ok(report)
    }

    pub async fn migrate_all(&self) -> Result<Vec<MigReport>, MigError> {
        if self.config.tables.is_empty() {
            return Err(MigError::Validation(
                "No tables configured for migration. Call MigConfig::with_tables(...) before invoking migrate_all()."
                    .to_string(),
            ));
        }

        let mut reports = Vec::with_capacity(self.config.tables.len());
        for table in &self.config.tables {
            let report = self.migrate(table).await?;
            reports.push(report);
        }
        Ok(reports)
    }

    pub async fn mysql_to_pg(&self, table: &str) -> Result<MigReport, MigError> {
        let columns = self.reader.table_columns(table).await?;

        let pg_columns: Vec<ColumnInfo> = columns
            .into_iter()
            .map(|col| {
                let pg_type = match col.data_type.to_lowercase().as_str() {
                    "varchar" | "char" | "text" | "longtext" | "mediumtext" | "tinytext" => {
                        "varchar"
                    }
                    "int" | "tinyint" | "smallint" | "mediumint" | "bigint" => "integer",
                    "float" | "double" | "decimal" => "numeric",
                    "datetime" | "timestamp" => "timestamp",
                    "date" => "date",
                    "time" => "time",
                    "json" => "jsonb",
                    "blob" | "mediumblob" | "longblob" | "tinyblob" => "bytea",
                    "bool" | "boolean" => "boolean",
                    _ => "text",
                };
                ColumnInfo::new(col.name, pg_type)
                    .with_default(col.default_value.unwrap_or_default())
                    .nullable(col.nullable)
                    .is_primary_key(col.is_primary_key)
            })
            .collect();

        self.writer.create_table(table, &pg_columns).await?;

        self.migrate(table).await
    }

    pub async fn pg_to_mysql(&self, table: &str) -> Result<MigReport, MigError> {
        let columns = self.reader.table_columns(table).await?;

        let mysql_columns: Vec<ColumnInfo> = columns
            .into_iter()
            .map(|col| {
                let mysql_type = match col.data_type.to_lowercase().as_str() {
                    "varchar" => "varchar(255)",
                    "text" => "text",
                    "integer" | "int4" => "int",
                    "bigint" | "int8" => "bigint",
                    "smallint" | "int2" => "smallint",
                    "numeric" | "decimal" | "float8" => "decimal(10,2)",
                    "float4" => "float",
                    "timestamp" | "timestamptz" => "datetime",
                    "date" => "date",
                    "time" | "timetz" => "time",
                    "jsonb" => "json",
                    "bytea" => "blob",
                    "boolean" | "bool" => "tinyint(1)",
                    _ => "varchar(255)",
                };
                ColumnInfo::new(col.name, mysql_type.to_string())
                    .with_default(col.default_value.unwrap_or_default())
                    .nullable(col.nullable)
                    .is_primary_key(col.is_primary_key)
            })
            .collect();

        self.writer.create_table(table, &mysql_columns).await?;

        self.migrate(table).await
    }
}
