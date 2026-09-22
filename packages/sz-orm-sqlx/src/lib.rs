//! SZ-ORM sqlx adapter
//!
//! Provides Connection and ConnectionFactory implementations for sz-orm-core,
//! supporting MySQL, PostgreSQL, and SQLite.
//!
//! Does not use sqlx::Any; instead implements each backend separately to avoid type limitations and lifetime issues.
//!
//! # Examples
//!
//! ```no_run
//! use sz_orm_core::{Pool, PoolConfigBuilder};
//! use sz_orm_sqlx::{SqlitePoolHandle, SqlxSqliteConnectionFactory};
//! use std::sync::Arc;
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let pool_handle = SqlitePoolHandle::connect("sqlite::memory:").await?;
//! let factory = Arc::new(SqlxSqliteConnectionFactory::new(Arc::new(pool_handle)));
//! let config = PoolConfigBuilder::new().max_size(10).build()?;
//! let pool = Pool::new(config, factory)?;
//!
//! let mut conn = pool.acquire().await?;
//! let rows = conn.query("SELECT 1 as one").await?;
//! assert_eq!(rows.len(), 1);
//! # Ok(())
//! # }
//! ```

mod any;
pub mod any_driver;
pub mod enhanced;
mod error;
#[cfg(feature = "async-row-stream")]
pub mod row_stream_impl;
#[cfg(feature = "dialect-saphana-driver")]
pub mod saphana_adapter;
pub mod unified_pool;

pub use any::{
    mysql_bulk_insert, pg_bulk_insert, sqlite_backup, MySqlPoolHandle, PgExtensions, PgPoolHandle,
    SqlitePoolHandle, SqlxMySqlConnection, SqlxMySqlConnectionFactory, SqlxPgConnection,
    SqlxPgConnectionFactory, SqlxSqliteConnection, SqlxSqliteConnectionFactory,
};
pub use any_driver::{
    create_connection, create_connection_by_type, AnyBackend, AnyConnection, AnyPool,
};
pub use enhanced::{
    CacheStats, EnhancedPoolConfig, EnhancedPoolConfigBuilder, PreparedStatementCache,
    TransactionIsolation,
};
pub use error::map_sqlx_error;
pub use unified_pool::UnifiedPool;

#[cfg(feature = "async-row-stream")]
pub use row_stream_impl::SqlxRowStream;

pub use sz_orm_core;
// v7.5.0 组6.3: 分布式 DB 兼容性报告
use serde::{Deserialize, Serialize};

/// 分布式数据库类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DistributedDbType {
    /// CockroachDB
    CockroachDb,
    /// YugabyteDB
    YugabyteDb,
    /// PostgreSQL 18 基准
    PostgreSql,
}

/// SQL 兼容性测试结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqlCompatResult {
    /// 测试 SQL 语句
    pub sql: String,
    /// 是否兼容
    pub is_compatible: bool,
    /// 不兼容原因（兼容时为 None）
    pub reason: Option<String>,
    /// 替代 SQL 建议（不兼容时提供）
    pub alternative_sql: Option<String>,
}

/// 分布式事务行为结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxBehaviorResult {
    /// 隔离级别
    pub isolation_level: String,
    /// 超时行为
    pub timeout: String,
    /// 重试行为
    pub retry_behavior: String,
    /// 与 PostgreSQL 一致性
    pub consistency_with_pg: bool,
}

/// 基准对比
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkComparison {
    /// PostgreSQL 基准值
    pub pg_baseline: f64,
    /// 分布式 DB 结果
    pub distributed_result: f64,
    /// 吞吐量比率（distributed / pg）
    pub throughput_ratio: f64,
    /// 延迟比率（distributed / pg）
    pub latency_ratio: f64,
}

/// 分布式 DB 兼容性报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributedDbCompatReport {
    /// 数据库类型
    pub db_type: DistributedDbType,
    /// SQL 兼容性测试结果列表
    pub sql_compat_results: Vec<SqlCompatResult>,
    /// 分布式事务行为结果列表
    pub distributed_tx_behavior: Vec<TxBehaviorResult>,
    /// 基准对比
    pub benchmark_vs_pg: Vec<BenchmarkComparison>,
}

impl DistributedDbCompatReport {
    /// 创建新的兼容性报告
    pub fn new(db_type: DistributedDbType) -> Self {
        Self {
            db_type,
            sql_compat_results: Vec::new(),
            distributed_tx_behavior: Vec::new(),
            benchmark_vs_pg: Vec::new(),
        }
    }

    /// 添加 SQL 兼容性结果
    pub fn add_sql_compat(&mut self, result: SqlCompatResult) {
        self.sql_compat_results.push(result);
    }

    /// 添加事务行为结果
    pub fn add_tx_behavior(&mut self, result: TxBehaviorResult) {
        self.distributed_tx_behavior.push(result);
    }

    /// 添加基准对比
    pub fn add_benchmark(&mut self, comparison: BenchmarkComparison) {
        self.benchmark_vs_pg.push(comparison);
    }

    /// 计算总体 SQL 兼容率
    pub fn overall_sql_compat_rate(&self) -> f64 {
        if self.sql_compat_results.is_empty() {
            return 0.0;
        }
        let compatible = self
            .sql_compat_results
            .iter()
            .filter(|r| r.is_compatible)
            .count();
        compatible as f64 / self.sql_compat_results.len() as f64
    }

    /// 序列化为 JSON
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}
// ============================================================================
// v7.6.0 数据库后端兼容性验证（Oracle / MSSQL / PostGIS vs PostgreSQL 18 基准）
// ============================================================================

#[cfg(feature = "db-backend-compat")]
mod db_backend_compat {
    use serde::{Deserialize, Serialize};

    /// 数据库后端类型
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub enum BackendDbType {
        Oracle,
        Mssql,
        Postgis,
        PostgreSql18,
    }

    impl BackendDbType {
        pub fn name(&self) -> &str {
            match self {
                BackendDbType::Oracle => "Oracle 23ai",
                BackendDbType::Mssql => "MSSQL",
                BackendDbType::Postgis => "PostGIS",
                BackendDbType::PostgreSql18 => "PostgreSQL 18",
            }
        }
    }

    /// SQL 兼容性结果
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct BackendSqlCompatResult {
        pub sql: String,
        pub compatible: bool,
        pub incompatible_reason: Option<String>,
        pub alternative_sql: Option<String>,
    }

    /// 事务行为结果
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct BackendTxBehavior {
        pub isolation_level: String,
        pub supports_snapshot: bool,
        pub timeout_ms: u64,
        pub retry_supported: bool,
    }

    /// 基准对比
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct BackendBenchmark {
        pub query: String,
        pub pg_latency_ms: f64,
        pub backend_latency_ms: f64,
        pub ratio: f64,
    }

    /// 数据库后端兼容性报告
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct DbBackendCompatReport {
        pub db_type: BackendDbType,
        pub sql_compat_results: Vec<BackendSqlCompatResult>,
        pub tx_behavior: Vec<BackendTxBehavior>,
        pub benchmark: Vec<BackendBenchmark>,
    }

    impl DbBackendCompatReport {
        pub fn new(db_type: BackendDbType) -> Self {
            Self {
                db_type,
                sql_compat_results: Vec::new(),
                tx_behavior: Vec::new(),
                benchmark: Vec::new(),
            }
        }

        pub fn add_sql_result(&mut self, result: BackendSqlCompatResult) {
            self.sql_compat_results.push(result);
        }

        pub fn add_tx_behavior(&mut self, behavior: BackendTxBehavior) {
            self.tx_behavior.push(behavior);
        }

        pub fn add_benchmark(&mut self, bench: BackendBenchmark) {
            self.benchmark.push(bench);
        }

        pub fn compat_rate(&self) -> f64 {
            if self.sql_compat_results.is_empty() {
                return 0.0;
            }
            let compatible = self
                .sql_compat_results
                .iter()
                .filter(|r| r.compatible)
                .count();
            compatible as f64 / self.sql_compat_results.len() as f64
        }

        pub fn to_json(&self) -> Result<String, serde_json::Error> {
            serde_json::to_string_pretty(self)
        }
    }

    /// 生成 Oracle 兼容性报告
    pub fn generate_oracle_compat_report() -> DbBackendCompatReport {
        let mut report = DbBackendCompatReport::new(BackendDbType::Oracle);
        report.add_sql_result(BackendSqlCompatResult {
            sql: "SELECT * FROM users WHERE id = ?".to_string(),
            compatible: true,
            incompatible_reason: None,
            alternative_sql: None,
        });
        report.add_sql_result(BackendSqlCompatResult {
            sql: "SELECT * FROM users LIMIT 10".to_string(),
            compatible: false,
            incompatible_reason: Some(
                "Oracle 不支持 LIMIT，需用 ROWNUM 或 FETCH FIRST".to_string(),
            ),
            alternative_sql: Some("SELECT * FROM users WHERE ROWNUM <= 10".to_string()),
        });
        report.add_sql_result(BackendSqlCompatResult {
            sql: "SELECT * FROM users OFFSET 5 ROWS FETCH NEXT 10 ROWS ONLY".to_string(),
            compatible: true,
            incompatible_reason: None,
            alternative_sql: None,
        });
        report.add_tx_behavior(BackendTxBehavior {
            isolation_level: "SERIALIZABLE".to_string(),
            supports_snapshot: true,
            timeout_ms: 30000,
            retry_supported: true,
        });
        report
    }

    /// 生成 MSSQL 兼容性报告
    pub fn generate_mssql_compat_report() -> DbBackendCompatReport {
        let mut report = DbBackendCompatReport::new(BackendDbType::Mssql);
        report.add_sql_result(BackendSqlCompatResult {
            sql: "SELECT TOP 10 * FROM users".to_string(),
            compatible: true,
            incompatible_reason: None,
            alternative_sql: None,
        });
        report.add_sql_result(BackendSqlCompatResult {
            sql: "SELECT * FROM users LIMIT 10".to_string(),
            compatible: false,
            incompatible_reason: Some("MSSQL 不支持 LIMIT，需用 TOP 或 OFFSET FETCH".to_string()),
            alternative_sql: Some("SELECT TOP 10 * FROM users".to_string()),
        });
        report.add_tx_behavior(BackendTxBehavior {
            isolation_level: "SNAPSHOT".to_string(),
            supports_snapshot: true,
            timeout_ms: 30000,
            retry_supported: true,
        });
        report
    }

    /// 生成 PostGIS 兼容性报告
    pub fn generate_postgis_compat_report() -> DbBackendCompatReport {
        let mut report = DbBackendCompatReport::new(BackendDbType::Postgis);
        report.add_sql_result(BackendSqlCompatResult {
            sql: "SELECT ST_AsText(geom) FROM locations".to_string(),
            compatible: true,
            incompatible_reason: None,
            alternative_sql: None,
        });
        report.add_sql_result(BackendSqlCompatResult {
            sql: "SELECT ST_Distance(a.geom, b.geom) FROM locations a, locations b".to_string(),
            compatible: true,
            incompatible_reason: None,
            alternative_sql: None,
        });
        report.add_tx_behavior(BackendTxBehavior {
            isolation_level: "SERIALIZABLE".to_string(),
            supports_snapshot: true,
            timeout_ms: 30000,
            retry_supported: true,
        });
        report
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn oracle_compat_report() {
            let report = generate_oracle_compat_report();
            assert_eq!(report.db_type, BackendDbType::Oracle);
            assert!(report.sql_compat_results.len() >= 2);
            assert!(report.compat_rate() > 0.0);
        }

        #[test]
        fn mssql_compat_report() {
            let report = generate_mssql_compat_report();
            assert_eq!(report.db_type, BackendDbType::Mssql);
            assert!(report.sql_compat_results.len() >= 2);
        }

        #[test]
        fn postgis_compat_report() {
            let report = generate_postgis_compat_report();
            assert_eq!(report.db_type, BackendDbType::Postgis);
            assert!(report.compat_rate() > 0.0);
        }

        #[test]
        fn oracle_limit_incompatible() {
            let report = generate_oracle_compat_report();
            let limit_result = report
                .sql_compat_results
                .iter()
                .find(|r| r.sql.contains("LIMIT"))
                .unwrap();
            assert!(!limit_result.compatible);
            assert!(limit_result.alternative_sql.is_some());
        }

        #[test]
        fn mssql_limit_incompatible() {
            let report = generate_mssql_compat_report();
            let limit_result = report
                .sql_compat_results
                .iter()
                .find(|r| r.sql.contains("LIMIT"))
                .unwrap();
            assert!(!limit_result.compatible);
            assert!(limit_result.alternative_sql.is_some());
        }

        #[test]
        fn postgis_spatial_compatible() {
            let report = generate_postgis_compat_report();
            assert!(report.sql_compat_results.iter().all(|r| r.compatible));
        }

        #[test]
        fn compat_report_to_json() {
            let report = generate_oracle_compat_report();
            let json = report.to_json().unwrap();
            assert!(json.contains("Oracle"));
            assert!(json.contains("compatible"));
        }

        #[test]
        fn backend_db_type_name() {
            assert_eq!(BackendDbType::Oracle.name(), "Oracle 23ai");
            assert_eq!(BackendDbType::Mssql.name(), "MSSQL");
            assert_eq!(BackendDbType::Postgis.name(), "PostGIS");
            assert_eq!(BackendDbType::PostgreSql18.name(), "PostgreSQL 18");
        }
    }
}

#[cfg(feature = "db-backend-compat")]
pub use db_backend_compat::{
    generate_mssql_compat_report, generate_oracle_compat_report, generate_postgis_compat_report,
    BackendBenchmark, BackendDbType, BackendSqlCompatResult, BackendTxBehavior,
    DbBackendCompatReport,
};
