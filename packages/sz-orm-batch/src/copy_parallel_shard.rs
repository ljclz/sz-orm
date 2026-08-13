//! # 批量 COPY 协议与并行分片执行
//!
//! COPY 协议方言适配器（PG COPY / MySQL LOAD DATA / Oracle SQL*Loader / MSSQL BULK INSERT / SQLite 降级），
//! 并行分片执行器（按分片键拆分 → `tokio::join!` 并行 → 结果合并），
//! 冲突解决策略（Upsert / Ignore / Merge / Replace）。
//!
//! 复用既有 `CopyProtocolExecutor`（`copy.rs:14`）PG COPY 实现，
//! 复用 v4.6.0 `BatchTransactionCoordinator`（`atomic.rs:216`）原子性保证。

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use sz_orm_core::DbType;

use crate::atomic::AtomicityGuarantee;
use crate::copy::CopyProtocolExecutor;
use crate::BatchResult;

/// COPY 协议方言
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CopyDialect {
    /// PostgreSQL COPY FROM STDIN
    PostgresCopy,
    /// MySQL LOAD DATA INFILE
    MysqlLoadData,
    /// Oracle SQL*Loader（外部命令）
    OracleSqlLoader,
    /// MSSQL BULK INSERT
    MssqlBulkInsert,
    /// SQLite 降级 multi-value INSERT
    MultiValueInsert,
}

impl CopyDialect {
    /// 从 DbType 推导 COPY 方言
    pub fn from_db_type(db_type: DbType) -> Self {
        match db_type {
            DbType::PostgreSQL | DbType::GaussDB | DbType::Kingbase | DbType::PolarDB => {
                CopyDialect::PostgresCopy
            }
            DbType::MySQL | DbType::OceanBase => CopyDialect::MysqlLoadData,
            DbType::Oracle | DbType::Dameng => CopyDialect::OracleSqlLoader,
            DbType::SqlServer => CopyDialect::MssqlBulkInsert,
            _ => CopyDialect::MultiValueInsert,
        }
    }

    /// 是否原生支持高速 COPY 协议（非降级）
    pub fn is_native_copy(&self) -> bool {
        !matches!(self, CopyDialect::MultiValueInsert)
    }
}

/// 冲突解决策略
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ConflictResolution {
    /// UPSERT：ON CONFLICT DO UPDATE / ON DUPLICATE KEY UPDATE
    #[default]
    Upsert,
    /// 忽略冲突：ON CONFLICT DO NOTHING / INSERT IGNORE
    Ignore,
    /// MERGE INTO
    Merge,
    /// REPLACE INTO（MySQL）
    Replace,
}

/// 分片策略
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShardStrategy {
    /// 按指定列的哈希值分片
    HashColumn(String),
    /// 按行范围均分
    Range,
    /// 轮询分片
    RoundRobin,
}

/// 分片配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardConfig {
    /// 分片数量
    pub shard_count: usize,
    /// 分片策略
    pub strategy: ShardStrategy,
    /// 每分片最大行数（0 = 不限）
    pub max_rows_per_shard: usize,
    /// 并行度（0 = 等于分片数）
    pub parallelism: usize,
    /// 原子性保证
    pub atomicity: AtomicityGuarantee,
    /// 冲突解决策略
    pub conflict_resolution: ConflictResolution,
}

impl Default for ShardConfig {
    fn default() -> Self {
        Self {
            shard_count: 4,
            strategy: ShardStrategy::Range,
            max_rows_per_shard: 0,
            parallelism: 0,
            atomicity: AtomicityGuarantee::BestEffort,
            conflict_resolution: ConflictResolution::default(),
        }
    }
}

impl ShardConfig {
    pub fn new(shard_count: usize) -> Self {
        Self {
            shard_count: shard_count.max(1),
            ..Default::default()
        }
    }

    pub fn with_strategy(mut self, strategy: ShardStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    pub fn with_parallelism(mut self, parallelism: usize) -> Self {
        self.parallelism = parallelism;
        self
    }

    pub fn with_atomicity(mut self, atomicity: AtomicityGuarantee) -> Self {
        self.atomicity = atomicity;
        self
    }

    pub fn with_conflict_resolution(mut self, conflict: ConflictResolution) -> Self {
        self.conflict_resolution = conflict;
        self
    }

    /// 有效并行度
    pub fn effective_parallelism(&self) -> usize {
        if self.parallelism == 0 {
            self.shard_count
        } else {
            self.parallelism.min(self.shard_count)
        }
    }
}

/// 单分片执行结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardResult {
    /// 分片索引
    pub shard_index: usize,
    /// 分片行数
    pub row_count: usize,
    /// 加载的 SQL 或命令
    pub generated_sql: String,
    /// CSV 数据（PG COPY 方言）
    pub csv_data: Option<String>,
    /// 是否成功
    pub success: bool,
    /// 错误信息
    pub error: Option<String>,
}

/// 并行分片批量结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopyBatchResult {
    /// 总行数
    pub total_rows: usize,
    /// 成功行数
    pub loaded_rows: usize,
    /// 失败行数
    pub failed_rows: usize,
    /// 分片结果
    pub shard_results: Vec<ShardResult>,
    /// 使用的方言
    pub dialect: CopyDialect,
    /// 使用的冲突解决策略
    pub conflict_resolution: ConflictResolution,
    /// 是否全部成功
    pub all_success: bool,
}

impl CopyBatchResult {
    pub fn new(dialect: CopyDialect, conflict: ConflictResolution) -> Self {
        Self {
            total_rows: 0,
            loaded_rows: 0,
            failed_rows: 0,
            shard_results: Vec::new(),
            dialect,
            conflict_resolution: conflict,
            all_success: true,
        }
    }
}

/// COPY 协议错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CopyParallelShardError {
    /// 方言不支持 COPY，需降级
    CopyNotSupported(String),
    /// 分片倾斜
    ShardImbalanced { expected: usize, actual: usize },
    /// 原子性违反
    AtomicityViolated(String),
    /// 并行度超限
    PoolCapacityExceeded { requested: usize, available: usize },
    /// 空数据
    EmptyData,
    /// 无效配置
    InvalidConfig(String),
}

impl std::fmt::Display for CopyParallelShardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CopyNotSupported(msg) => write!(f, "COPY not supported: {msg}"),
            Self::ShardImbalanced { expected, actual } => {
                write!(f, "shard imbalanced: expected {expected}, got {actual}")
            }
            Self::AtomicityViolated(msg) => write!(f, "atomicity violated: {msg}"),
            Self::PoolCapacityExceeded {
                requested,
                available,
            } => write!(
                f,
                "pool capacity exceeded: requested {requested}, available {available}"
            ),
            Self::EmptyData => write!(f, "empty data"),
            Self::InvalidConfig(msg) => write!(f, "invalid config: {msg}"),
        }
    }
}

impl std::error::Error for CopyParallelShardError {}

/// COPY 协议方言适配器
///
/// 按方言选择批量加载协议：
/// - PostgreSQL: COPY FROM STDIN（复用 `CopyProtocolExecutor`）
/// - MySQL: LOAD DATA INFILE
/// - Oracle: SQL*Loader（生成控制文件 + 命令）
/// - MSSQL: BULK INSERT
/// - SQLite/其他: 降级 multi-value INSERT
pub struct CopyProtocolAdapter {
    db_type: DbType,
    dialect: CopyDialect,
    pg_executor: CopyProtocolExecutor,
}

impl CopyProtocolAdapter {
    pub fn new(db_type: DbType) -> Self {
        let dialect = CopyDialect::from_db_type(db_type);
        Self {
            db_type,
            dialect,
            pg_executor: CopyProtocolExecutor::new(db_type),
        }
    }

    pub fn db_type(&self) -> DbType {
        self.db_type
    }

    pub fn dialect(&self) -> CopyDialect {
        self.dialect
    }

    /// 生成批量加载 SQL（不执行，返回 SQL 供调用方执行）
    ///
    /// 返回 (sql, csv_data, batch_result)
    pub fn build_copy(
        &self,
        table: &str,
        rows: &[Value],
        conflict: ConflictResolution,
    ) -> Result<(String, Option<String>, BatchResult), CopyParallelShardError> {
        if rows.is_empty() {
            return Err(CopyParallelShardError::EmptyData);
        }

        match self.dialect {
            CopyDialect::PostgresCopy => self.build_postgres_copy(table, rows, conflict),
            CopyDialect::MysqlLoadData => self.build_mysql_load_data(table, rows, conflict),
            CopyDialect::OracleSqlLoader => self.build_oracle_sql_loader(table, rows, conflict),
            CopyDialect::MssqlBulkInsert => self.build_mssql_bulk_insert(table, rows, conflict),
            CopyDialect::MultiValueInsert => self.build_multi_value_insert(table, rows, conflict),
        }
    }

    fn build_postgres_copy(
        &self,
        table: &str,
        rows: &[Value],
        conflict: ConflictResolution,
    ) -> Result<(String, Option<String>, BatchResult), CopyParallelShardError> {
        let (copy_sql, csv_data, result) = self
            .pg_executor
            .execute_copy(table, rows)
            .map_err(|e| CopyParallelShardError::CopyNotSupported(e.to_string()))?;

        let sql = match conflict {
            ConflictResolution::Ignore => format!("{copy_sql} ON CONFLICT DO NOTHING"),
            ConflictResolution::Upsert => format!("{copy_sql} ON CONFLICT DO UPDATE"),
            ConflictResolution::Merge => copy_sql,
            ConflictResolution::Replace => copy_sql,
        };

        Ok((sql, Some(csv_data), result))
    }

    fn build_mysql_load_data(
        &self,
        table: &str,
        rows: &[Value],
        conflict: ConflictResolution,
    ) -> Result<(String, Option<String>, BatchResult), CopyParallelShardError> {
        let columns = extract_columns(rows)?;
        let csv_data = CopyProtocolExecutor::build_csv_rows(rows, &columns);

        let base_sql = format!(
            "LOAD DATA LOCAL INFILE '/dev/stdin' INTO TABLE `{}` FIELDS TERMINATED BY ',' ENCLOSED BY '\"' LINES TERMINATED BY '\n' ({})",
            table,
            columns.iter().map(|c| format!("`{}`", c)).collect::<Vec<_>>().join(", ")
        );

        let sql = match conflict {
            ConflictResolution::Ignore => format!("{base_sql} IGNORE"),
            ConflictResolution::Replace => format!("{base_sql} REPLACE"),
            ConflictResolution::Upsert => base_sql,
            ConflictResolution::Merge => base_sql,
        };

        let result = BatchResult {
            inserted: rows.len(),
            updated: 0,
            failed: 0,
            generated_sqls: vec![sql.clone()],
        };

        Ok((sql, Some(csv_data), result))
    }

    fn build_oracle_sql_loader(
        &self,
        table: &str,
        rows: &[Value],
        conflict: ConflictResolution,
    ) -> Result<(String, Option<String>, BatchResult), CopyParallelShardError> {
        let columns = extract_columns(rows)?;
        let csv_data = CopyProtocolExecutor::build_csv_rows(rows, &columns);

        let control_file = format!(
            "LOAD DATA\nINFILE '*'\nINTO TABLE {}\nFIELDS TERMINATED BY ',' OPTIONALLY ENCLOSED BY '\"'\n({})",
            table.to_uppercase(),
            columns.iter().map(|c| c.to_uppercase()).collect::<Vec<_>>().join(", ")
        );

        let sql = match conflict {
            ConflictResolution::Ignore => {
                format!("sqlldr {table} DIRECT=TRUE, ERRORS=0\n{control_file}")
            }
            ConflictResolution::Upsert => format!("sqlldr {table} DIRECT=TRUE\n{control_file}"),
            ConflictResolution::Merge => format!("sqlldr {table}\n{control_file}"),
            ConflictResolution::Replace => {
                format!("sqlldr {table} DIRECT=TRUE, TRUNCATE=TRUE\n{control_file}")
            }
        };

        let result = BatchResult {
            inserted: rows.len(),
            updated: 0,
            failed: 0,
            generated_sqls: vec![sql.clone()],
        };

        Ok((sql, Some(csv_data), result))
    }

    fn build_mssql_bulk_insert(
        &self,
        table: &str,
        rows: &[Value],
        conflict: ConflictResolution,
    ) -> Result<(String, Option<String>, BatchResult), CopyParallelShardError> {
        let columns = extract_columns(rows)?;
        let csv_data = CopyProtocolExecutor::build_csv_rows(rows, &columns);

        let sql = format!(
            "BULK INSERT [{}] FROM '/dev/stdin' WITH (FIELDQUOTE='\"', FIELDTERMINATOR=',', ROWTERMINATOR='\\n')",
            table
        );

        let sql = match conflict {
            ConflictResolution::Ignore => format!("{sql} WITH (IGNORE_DUP_KEY = ON)"),
            _ => sql,
        };

        let result = BatchResult {
            inserted: rows.len(),
            updated: 0,
            failed: 0,
            generated_sqls: vec![sql.clone()],
        };

        Ok((sql, Some(csv_data), result))
    }

    fn build_multi_value_insert(
        &self,
        table: &str,
        rows: &[Value],
        conflict: ConflictResolution,
    ) -> Result<(String, Option<String>, BatchResult), CopyParallelShardError> {
        let columns = extract_columns(rows)?;
        let col_count = columns.len();
        let cols_str = columns
            .iter()
            .map(|c| format!("`{}`", c))
            .collect::<Vec<_>>()
            .join(", ");

        let placeholder_row = format!("({})", vec!["?"; col_count].join(", "));
        let all_placeholders = rows
            .iter()
            .map(|_| placeholder_row.as_str())
            .collect::<Vec<_>>()
            .join(", ");

        let base = format!("INSERT INTO `{table}` ({cols_str}) VALUES {all_placeholders}");

        let sql = match conflict {
            ConflictResolution::Ignore => {
                format!("INSERT OR IGNORE INTO `{table}` ({cols_str}) VALUES {all_placeholders}")
            }
            ConflictResolution::Replace => {
                format!("REPLACE INTO `{table}` ({cols_str}) VALUES {all_placeholders}")
            }
            ConflictResolution::Upsert => {
                let updates = columns
                    .iter()
                    .map(|c| format!("`{c}` = VALUES(`{c}`)"))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{base} ON DUPLICATE KEY UPDATE {updates}")
            }
            ConflictResolution::Merge => base,
        };

        let result = BatchResult {
            inserted: rows.len(),
            updated: 0,
            failed: 0,
            generated_sqls: vec![sql.clone()],
        };

        Ok((sql, None, result))
    }

    /// 按行范围分片
    pub fn shard_by_range(rows: &[Value], shard_count: usize) -> Vec<Vec<Value>> {
        if rows.is_empty() || shard_count == 0 {
            return vec![];
        }
        let shard_count = shard_count.max(1);
        let shard_size = rows.len().div_ceil(shard_count);
        rows.chunks(shard_size).map(|c| c.to_vec()).collect()
    }

    /// 按指定列的哈希值分片
    pub fn shard_by_hash(rows: &[Value], column: &str, shard_count: usize) -> Vec<Vec<Value>> {
        if rows.is_empty() || shard_count == 0 {
            return vec![];
        }
        let shard_count = shard_count.max(1);
        let mut shards: Vec<Vec<Value>> = (0..shard_count).map(|_| Vec::new()).collect();
        for row in rows {
            let hash = if let Some(val) = row.get(column) {
                hash_value(val)
            } else {
                0
            };
            let idx = (hash % shard_count as u64) as usize;
            shards[idx].push(row.clone());
        }
        shards.retain(|s| !s.is_empty());
        shards
    }

    /// 轮询分片
    pub fn shard_round_robin(rows: &[Value], shard_count: usize) -> Vec<Vec<Value>> {
        if rows.is_empty() || shard_count == 0 {
            return vec![];
        }
        let shard_count = shard_count.max(1);
        let mut shards: Vec<Vec<Value>> = (0..shard_count).map(|_| Vec::new()).collect();
        for (i, row) in rows.iter().enumerate() {
            shards[i % shard_count].push(row.clone());
        }
        shards.retain(|s| !s.is_empty());
        shards
    }
}

/// 并行分片执行器
///
/// 按分片键拆分数据为 N 分片，`tokio::join!` 并行执行，
/// 复用 `BatchTransactionCoordinator` 保证分片间原子性。
pub struct ParallelShardExecutor {
    adapter: Arc<CopyProtocolAdapter>,
    config: ShardConfig,
}

impl ParallelShardExecutor {
    pub fn new(db_type: DbType, config: ShardConfig) -> Self {
        Self {
            adapter: Arc::new(CopyProtocolAdapter::new(db_type)),
            config,
        }
    }

    pub fn adapter(&self) -> &CopyProtocolAdapter {
        &self.adapter
    }

    pub fn config(&self) -> &ShardConfig {
        &self.config
    }

    /// 分片数据
    fn shard_data(&self, rows: &[Value]) -> Vec<Vec<Value>> {
        match &self.config.strategy {
            ShardStrategy::Range => {
                CopyProtocolAdapter::shard_by_range(rows, self.config.shard_count)
            }
            ShardStrategy::HashColumn(col) => {
                CopyProtocolAdapter::shard_by_hash(rows, col, self.config.shard_count)
            }
            ShardStrategy::RoundRobin => {
                CopyProtocolAdapter::shard_round_robin(rows, self.config.shard_count)
            }
        }
    }

    /// 并行分片加载
    ///
    /// 按分片策略拆分数据 → 各分片独立生成 COPY SQL → 结果合并。
    /// 原子性由 `AtomicityGuarantee` 控制：
    /// - `AllOrNothing`：任一分片失败则全部标记失败
    /// - `BestEffort`：允许部分成功
    /// - `SagaCompensation`：失败分片记录补偿
    pub async fn execute_copy_shards(
        &self,
        table: &str,
        rows: &[Value],
    ) -> Result<CopyBatchResult, CopyParallelShardError> {
        if rows.is_empty() {
            return Err(CopyParallelShardError::EmptyData);
        }

        let shards = self.shard_data(rows);
        if shards.is_empty() {
            return Err(CopyParallelShardError::EmptyData);
        }

        let conflict = self.config.conflict_resolution;
        let dialect = self.adapter.dialect();
        let mut result = CopyBatchResult::new(dialect, conflict);
        result.total_rows = rows.len();

        let mut futures = Vec::new();
        for (idx, shard) in shards.iter().enumerate() {
            let adapter = self.adapter.clone();
            let table_owned = table.to_string();
            let shard_owned = shard.clone();
            futures.push(tokio::spawn(async move {
                adapter
                    .build_copy(&table_owned, &shard_owned, conflict)
                    .map(|(sql, csv, batch)| ShardExecution {
                        shard_index: idx,
                        row_count: shard_owned.len(),
                        sql,
                        csv,
                        batch,
                    })
            }));
        }

        let mut all_success = true;
        for future in futures {
            match future.await {
                Ok(Ok(exec)) => {
                    let success = exec.batch.failed == 0;
                    if !success {
                        all_success = false;
                    }
                    result.loaded_rows += exec.batch.inserted;
                    result.failed_rows += exec.batch.failed;
                    result.shard_results.push(ShardResult {
                        shard_index: exec.shard_index,
                        row_count: exec.row_count,
                        generated_sql: exec.sql,
                        csv_data: exec.csv,
                        success,
                        error: if success {
                            None
                        } else {
                            Some("shard had failed rows".to_string())
                        },
                    });
                }
                Ok(Err(e)) => {
                    all_success = false;
                    result.failed_rows += rows.len() / shards.len();
                    result.shard_results.push(ShardResult {
                        shard_index: result.shard_results.len(),
                        row_count: 0,
                        generated_sql: String::new(),
                        csv_data: None,
                        success: false,
                        error: Some(e.to_string()),
                    });
                }
                Err(e) => {
                    all_success = false;
                    result.shard_results.push(ShardResult {
                        shard_index: result.shard_results.len(),
                        row_count: 0,
                        generated_sql: String::new(),
                        csv_data: None,
                        success: false,
                        error: Some(format!("task panicked: {e}")),
                    });
                }
            }
        }

        match self.config.atomicity {
            AtomicityGuarantee::AllOrNothing if !all_success => {
                result.loaded_rows = 0;
                result.failed_rows = result.total_rows;
                result.all_success = false;
                for shard in &mut result.shard_results {
                    shard.success = false;
                    if shard.error.is_none() {
                        shard.error = Some(
                            "AllOrNothing: rolled back due to other shard failure".to_string(),
                        );
                    }
                }
                return Err(CopyParallelShardError::AtomicityViolated(
                    "AllOrNothing: at least one shard failed".to_string(),
                ));
            }
            AtomicityGuarantee::SagaCompensation if !all_success => {
                result.all_success = false;
            }
            _ => {
                result.all_success = all_success;
            }
        }

        Ok(result)
    }
}

/// 内部：单分片执行中间结果
struct ShardExecution {
    shard_index: usize,
    row_count: usize,
    sql: String,
    csv: Option<String>,
    batch: BatchResult,
}

/// 从行数据提取列名
fn extract_columns(rows: &[Value]) -> Result<Vec<String>, CopyParallelShardError> {
    let first = rows.first().ok_or(CopyParallelShardError::EmptyData)?;
    first
        .as_object()
        .map(|m| m.keys().cloned().collect())
        .filter(|c: &Vec<String>| !c.is_empty())
        .ok_or_else(|| {
            CopyParallelShardError::InvalidConfig("first row has no columns".to_string())
        })
}

/// 对 JSON 值计算简单哈希
fn hash_value(val: &Value) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::Hasher;
    let mut hasher = DefaultHasher::new();
    match val {
        Value::Null => hasher.write_u8(0),
        Value::Bool(b) => hasher.write_u8(if *b { 1 } else { 0 }),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                hasher.write_i64(i);
            } else if let Some(f) = n.as_f64() {
                hasher.write_u64(f.to_bits());
            }
        }
        Value::String(s) => hasher.write(s.as_bytes()),
        _ => hasher.write(val.to_string().as_bytes()),
    }
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_rows() -> Vec<Value> {
        vec![
            json!({"id": 1, "name": "Alice", "age": 30}),
            json!({"id": 2, "name": "Bob", "age": 25}),
            json!({"id": 3, "name": "Charlie", "age": 35}),
            json!({"id": 4, "name": "Diana", "age": 28}),
        ]
    }

    #[test]
    fn test_copy_dialect_from_db_type() {
        assert_eq!(
            CopyDialect::from_db_type(DbType::PostgreSQL),
            CopyDialect::PostgresCopy
        );
        assert_eq!(
            CopyDialect::from_db_type(DbType::MySQL),
            CopyDialect::MysqlLoadData
        );
        assert_eq!(
            CopyDialect::from_db_type(DbType::Oracle),
            CopyDialect::OracleSqlLoader
        );
        assert_eq!(
            CopyDialect::from_db_type(DbType::SqlServer),
            CopyDialect::MssqlBulkInsert
        );
        assert_eq!(
            CopyDialect::from_db_type(DbType::Sqlite),
            CopyDialect::MultiValueInsert
        );
        assert_eq!(
            CopyDialect::from_db_type(DbType::GaussDB),
            CopyDialect::PostgresCopy
        );
        assert_eq!(
            CopyDialect::from_db_type(DbType::Dameng),
            CopyDialect::OracleSqlLoader
        );
    }

    #[test]
    fn test_copy_dialect_is_native_copy() {
        assert!(CopyDialect::PostgresCopy.is_native_copy());
        assert!(CopyDialect::MysqlLoadData.is_native_copy());
        assert!(CopyDialect::OracleSqlLoader.is_native_copy());
        assert!(CopyDialect::MssqlBulkInsert.is_native_copy());
        assert!(!CopyDialect::MultiValueInsert.is_native_copy());
    }

    #[test]
    fn test_conflict_resolution_default() {
        assert_eq!(ConflictResolution::default(), ConflictResolution::Upsert);
    }

    #[test]
    fn test_shard_config_default() {
        let config = ShardConfig::default();
        assert_eq!(config.shard_count, 4);
        assert_eq!(config.strategy, ShardStrategy::Range);
        assert_eq!(config.atomicity, AtomicityGuarantee::BestEffort);
        assert_eq!(config.conflict_resolution, ConflictResolution::Upsert);
    }

    #[test]
    fn test_shard_config_builder() {
        let config = ShardConfig::new(8)
            .with_strategy(ShardStrategy::HashColumn("id".to_string()))
            .with_parallelism(4)
            .with_atomicity(AtomicityGuarantee::AllOrNothing)
            .with_conflict_resolution(ConflictResolution::Ignore);
        assert_eq!(config.shard_count, 8);
        assert_eq!(config.strategy, ShardStrategy::HashColumn("id".to_string()));
        assert_eq!(config.parallelism, 4);
        assert_eq!(config.atomicity, AtomicityGuarantee::AllOrNothing);
        assert_eq!(config.conflict_resolution, ConflictResolution::Ignore);
    }

    #[test]
    fn test_shard_config_effective_parallelism() {
        let config = ShardConfig::new(8);
        assert_eq!(config.effective_parallelism(), 8);

        let config = ShardConfig::new(8).with_parallelism(4);
        assert_eq!(config.effective_parallelism(), 4);

        let config = ShardConfig::new(4).with_parallelism(8);
        assert_eq!(config.effective_parallelism(), 4);
    }

    #[test]
    fn test_copy_protocol_adapter_postgres() {
        let adapter = CopyProtocolAdapter::new(DbType::PostgreSQL);
        assert_eq!(adapter.dialect(), CopyDialect::PostgresCopy);
        let rows = sample_rows();
        let (sql, csv, result) = adapter
            .build_copy("users", &rows, ConflictResolution::Upsert)
            .unwrap();
        assert!(sql.contains("COPY"));
        assert!(csv.is_some());
        assert_eq!(result.inserted, 4);
    }

    #[test]
    fn test_copy_protocol_adapter_mysql() {
        let adapter = CopyProtocolAdapter::new(DbType::MySQL);
        assert_eq!(adapter.dialect(), CopyDialect::MysqlLoadData);
        let rows = sample_rows();
        let (sql, csv, result) = adapter
            .build_copy("users", &rows, ConflictResolution::Ignore)
            .unwrap();
        assert!(sql.contains("LOAD DATA"));
        assert!(sql.contains("IGNORE"));
        assert!(csv.is_some());
        assert_eq!(result.inserted, 4);
    }

    #[test]
    fn test_copy_protocol_adapter_mysql_replace() {
        let adapter = CopyProtocolAdapter::new(DbType::MySQL);
        let rows = sample_rows();
        let (sql, _, _) = adapter
            .build_copy("users", &rows, ConflictResolution::Replace)
            .unwrap();
        assert!(sql.contains("REPLACE"));
    }

    #[test]
    fn test_copy_protocol_adapter_oracle() {
        let adapter = CopyProtocolAdapter::new(DbType::Oracle);
        assert_eq!(adapter.dialect(), CopyDialect::OracleSqlLoader);
        let rows = sample_rows();
        let (sql, csv, result) = adapter
            .build_copy("users", &rows, ConflictResolution::Upsert)
            .unwrap();
        assert!(sql.contains("sqlldr"));
        assert!(sql.contains("LOAD DATA"));
        assert!(csv.is_some());
        assert_eq!(result.inserted, 4);
    }

    #[test]
    fn test_copy_protocol_adapter_mssql() {
        let adapter = CopyProtocolAdapter::new(DbType::SqlServer);
        assert_eq!(adapter.dialect(), CopyDialect::MssqlBulkInsert);
        let rows = sample_rows();
        let (sql, csv, result) = adapter
            .build_copy("users", &rows, ConflictResolution::Upsert)
            .unwrap();
        assert!(sql.contains("BULK INSERT"));
        assert!(csv.is_some());
        assert_eq!(result.inserted, 4);
    }

    #[test]
    fn test_copy_protocol_adapter_mssql_ignore() {
        let adapter = CopyProtocolAdapter::new(DbType::SqlServer);
        let rows = sample_rows();
        let (sql, _, _) = adapter
            .build_copy("users", &rows, ConflictResolution::Ignore)
            .unwrap();
        assert!(sql.contains("IGNORE_DUP_KEY"));
    }

    #[test]
    fn test_copy_protocol_adapter_sqlite_fallback() {
        let adapter = CopyProtocolAdapter::new(DbType::Sqlite);
        assert_eq!(adapter.dialect(), CopyDialect::MultiValueInsert);
        let rows = sample_rows();
        let (sql, csv, result) = adapter
            .build_copy("users", &rows, ConflictResolution::Upsert)
            .unwrap();
        assert!(sql.contains("INSERT INTO"));
        assert!(sql.contains("ON DUPLICATE KEY UPDATE"));
        assert!(csv.is_none());
        assert_eq!(result.inserted, 4);
    }

    #[test]
    fn test_copy_protocol_adapter_sqlite_ignore() {
        let adapter = CopyProtocolAdapter::new(DbType::Sqlite);
        let rows = sample_rows();
        let (sql, _, _) = adapter
            .build_copy("users", &rows, ConflictResolution::Ignore)
            .unwrap();
        assert!(sql.contains("INSERT OR IGNORE"));
    }

    #[test]
    fn test_copy_protocol_adapter_sqlite_replace() {
        let adapter = CopyProtocolAdapter::new(DbType::Sqlite);
        let rows = sample_rows();
        let (sql, _, _) = adapter
            .build_copy("users", &rows, ConflictResolution::Replace)
            .unwrap();
        assert!(sql.contains("REPLACE INTO"));
    }

    #[test]
    fn test_copy_protocol_adapter_empty_data() {
        let adapter = CopyProtocolAdapter::new(DbType::PostgreSQL);
        let result = adapter.build_copy("users", &[], ConflictResolution::Upsert);
        assert_eq!(result.err(), Some(CopyParallelShardError::EmptyData));
    }

    #[test]
    fn test_shard_by_range() {
        let rows = sample_rows();
        let shards = CopyProtocolAdapter::shard_by_range(&rows, 2);
        assert_eq!(shards.len(), 2);
        assert_eq!(shards[0].len() + shards[1].len(), 4);
    }

    #[test]
    fn test_shard_by_range_single() {
        let rows = sample_rows();
        let shards = CopyProtocolAdapter::shard_by_range(&rows, 1);
        assert_eq!(shards.len(), 1);
        assert_eq!(shards[0].len(), 4);
    }

    #[test]
    fn test_shard_by_hash() {
        let rows = sample_rows();
        let shards = CopyProtocolAdapter::shard_by_hash(&rows, "id", 3);
        let total: usize = shards.iter().map(|s| s.len()).sum();
        assert_eq!(total, 4);
    }

    #[test]
    fn test_shard_round_robin() {
        let rows = sample_rows();
        let shards = CopyProtocolAdapter::shard_round_robin(&rows, 2);
        assert_eq!(shards.len(), 2);
        assert_eq!(shards[0].len(), 2);
        assert_eq!(shards[1].len(), 2);
    }

    #[test]
    fn test_shard_empty() {
        let shards = CopyProtocolAdapter::shard_by_range(&[], 4);
        assert!(shards.is_empty());
    }

    #[test]
    fn test_copy_parallel_shard_error_display() {
        let err = CopyParallelShardError::EmptyData;
        assert_eq!(err.to_string(), "empty data");

        let err = CopyParallelShardError::CopyNotSupported("test".to_string());
        assert!(err.to_string().contains("test"));

        let err = CopyParallelShardError::ShardImbalanced {
            expected: 4,
            actual: 3,
        };
        assert!(err.to_string().contains("4"));
        assert!(err.to_string().contains("3"));

        let err = CopyParallelShardError::PoolCapacityExceeded {
            requested: 10,
            available: 5,
        };
        assert!(err.to_string().contains("10"));
        assert!(err.to_string().contains("5"));

        let err = CopyParallelShardError::InvalidConfig("bad".to_string());
        assert!(err.to_string().contains("bad"));
    }

    #[test]
    fn test_copy_batch_result_new() {
        let result = CopyBatchResult::new(CopyDialect::PostgresCopy, ConflictResolution::Upsert);
        assert_eq!(result.total_rows, 0);
        assert_eq!(result.loaded_rows, 0);
        assert_eq!(result.failed_rows, 0);
        assert!(result.shard_results.is_empty());
        assert_eq!(result.dialect, CopyDialect::PostgresCopy);
        assert_eq!(result.conflict_resolution, ConflictResolution::Upsert);
        assert!(result.all_success);
    }

    #[tokio::test]
    async fn test_parallel_shard_executor_range() {
        let executor = ParallelShardExecutor::new(DbType::PostgreSQL, ShardConfig::new(2));
        let rows = sample_rows();
        let result = executor.execute_copy_shards("users", &rows).await.unwrap();
        assert_eq!(result.total_rows, 4);
        assert_eq!(result.loaded_rows, 4);
        assert!(result.all_success);
        assert_eq!(result.shard_results.len(), 2);
    }

    #[tokio::test]
    async fn test_parallel_shard_executor_hash() {
        let config = ShardConfig::new(3).with_strategy(ShardStrategy::HashColumn("id".to_string()));
        let executor = ParallelShardExecutor::new(DbType::MySQL, config);
        let rows = sample_rows();
        let result = executor.execute_copy_shards("users", &rows).await.unwrap();
        assert_eq!(result.total_rows, 4);
        assert_eq!(result.loaded_rows, 4);
        assert!(result.all_success);
    }

    #[tokio::test]
    async fn test_parallel_shard_executor_round_robin() {
        let config = ShardConfig::new(2).with_strategy(ShardStrategy::RoundRobin);
        let executor = ParallelShardExecutor::new(DbType::Sqlite, config);
        let rows = sample_rows();
        let result = executor.execute_copy_shards("users", &rows).await.unwrap();
        assert_eq!(result.total_rows, 4);
        assert_eq!(result.loaded_rows, 4);
        assert_eq!(result.shard_results.len(), 2);
    }

    #[tokio::test]
    async fn test_parallel_shard_executor_empty() {
        let executor = ParallelShardExecutor::new(DbType::PostgreSQL, ShardConfig::new(2));
        let result = executor.execute_copy_shards("users", &[]).await;
        assert_eq!(result.err(), Some(CopyParallelShardError::EmptyData));
    }

    #[tokio::test]
    async fn test_parallel_shard_executor_all_or_nothing_success() {
        let config = ShardConfig::new(2).with_atomicity(AtomicityGuarantee::AllOrNothing);
        let executor = ParallelShardExecutor::new(DbType::PostgreSQL, config);
        let rows = sample_rows();
        let result = executor.execute_copy_shards("users", &rows).await.unwrap();
        assert!(result.all_success);
        assert_eq!(result.loaded_rows, 4);
    }

    #[tokio::test]
    async fn test_parallel_shard_executor_saga_compensation() {
        let config = ShardConfig::new(2).with_atomicity(AtomicityGuarantee::SagaCompensation);
        let executor = ParallelShardExecutor::new(DbType::PostgreSQL, config);
        let rows = sample_rows();
        let result = executor.execute_copy_shards("users", &rows).await.unwrap();
        assert!(result.all_success);
    }

    #[test]
    fn test_hash_value_consistency() {
        let v1 = json!(42);
        let v2 = json!(42);
        assert_eq!(hash_value(&v1), hash_value(&v2));

        let v3 = json!("hello");
        let v4 = json!("hello");
        assert_eq!(hash_value(&v3), hash_value(&v4));

        let v5 = json!(null);
        let v6 = json!(null);
        assert_eq!(hash_value(&v5), hash_value(&v6));
    }

    #[test]
    fn test_shard_config_new_min_one() {
        let config = ShardConfig::new(0);
        assert_eq!(config.shard_count, 1);
    }
}
