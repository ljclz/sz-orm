//! 真实 DB 查询执行器模块（v7.2.0）
//!
//! 提供 sz-orm / sqlx / sea-orm 三框架的真实 DB 基准查询执行器，
//! 替代 `run_workload()` 的延迟模型模拟。Diesel 因依赖不在本地缓存，
//! 暂以 `real-bench-diesel` feature 门控（未启用）。

#![cfg(feature = "real-bench")]

use std::sync::Arc;
use std::time::Instant;

use sea_orm::TransactionTrait;
use sqlx::Row;
use tokio::time::sleep;

use crate::{BenchConfig, BenchError, BenchResult, DbBackend, FrameworkType, WorkloadType};

// ── DatasetInitializer ──────────────────────────────────────────────

/// 数据集初始化器：建表 + 批量插入测试数据
pub struct DatasetInitializer;

impl DatasetInitializer {
    /// 初始化数据集：建 `bench_users` 表并插入 `dataset_size` 行
    pub async fn init(
        backend: DbBackend,
        connection: &str,
        dataset_size: usize,
    ) -> Result<(), BenchError> {
        match backend {
            DbBackend::Sqlite => Self::init_sqlite(connection, dataset_size).await,
            DbBackend::Mysql => Self::init_mysql(connection, dataset_size).await,
        }
    }

    async fn init_sqlite(connection: &str, dataset_size: usize) -> Result<(), BenchError> {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(5)
            .connect(connection)
            .await
            .map_err(|e| BenchError::DbConnectFailed(e.to_string()))?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS bench_users (\
             id INTEGER PRIMARY KEY AUTOINCREMENT,\
             name TEXT NOT NULL,\
             email TEXT NOT NULL,\
             created_at INTEGER NOT NULL)",
        )
        .execute(&pool)
        .await
        .map_err(|e| BenchError::QueryFailed(e.to_string()))?;

        sqlx::query("DELETE FROM bench_users")
            .execute(&pool)
            .await
            .map_err(|e| BenchError::QueryFailed(e.to_string()))?;

        for i in 1..=dataset_size {
            let name = format!("user_{i}");
            let email = format!("user_{i}@bench.test");
            sqlx::query(
                "INSERT INTO bench_users (id, name, email, created_at) VALUES (?, ?, ?, ?)",
            )
            .bind(i as i64)
            .bind(&name)
            .bind(&email)
            .bind(i as i64)
            .execute(&pool)
            .await
            .map_err(|e| BenchError::QueryFailed(e.to_string()))?;
        }

        pool.close().await;
        Ok(())
    }

    async fn init_mysql(connection: &str, dataset_size: usize) -> Result<(), BenchError> {
        let pool = sqlx::mysql::MySqlPoolOptions::new()
            .max_connections(5)
            .connect(connection)
            .await
            .map_err(|e| BenchError::DbConnectFailed(e.to_string()))?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS bench_users (\
             id INTEGER PRIMARY KEY AUTO_INCREMENT,\
             name VARCHAR(255) NOT NULL,\
             email VARCHAR(255) NOT NULL,\
             created_at BIGINT NOT NULL)",
        )
        .execute(&pool)
        .await
        .map_err(|e| BenchError::QueryFailed(e.to_string()))?;

        sqlx::query("TRUNCATE TABLE bench_users")
            .execute(&pool)
            .await
            .map_err(|e| BenchError::QueryFailed(e.to_string()))?;

        for i in 1..=dataset_size {
            let name = format!("user_{i}");
            let email = format!("user_{i}@bench.test");
            sqlx::query(
                "INSERT INTO bench_users (id, name, email, created_at) VALUES (?, ?, ?, ?)",
            )
            .bind(i as i64)
            .bind(&name)
            .bind(&email)
            .bind(i as i64)
            .execute(&pool)
            .await
            .map_err(|e| BenchError::QueryFailed(e.to_string()))?;
        }

        pool.close().await;
        Ok(())
    }
}

// ── SzOrmWorkload ───────────────────────────────────────────────────

/// sz-orm 真实查询执行器（通过 sz-orm-sqlx 适配器 → sz-orm-core Pool）
pub struct SzOrmWorkload {
    pool: sz_orm_core::Pool,
}

impl SzOrmWorkload {
    /// 创建 sz-orm 工作负载执行器（SQLite 后端）
    pub async fn new_sqlite(connection: &str, pool_size: usize) -> Result<Self, BenchError> {
        let handle = sz_orm_sqlx::SqlitePoolHandle::connect(connection)
            .await
            .map_err(|e| BenchError::DbConnectFailed(e.to_string()))?;
        let arc_handle = Arc::new(handle);
        let factory = sz_orm_sqlx::SqlxSqliteConnectionFactory::new(arc_handle);
        let config = sz_orm_core::PoolConfigBuilder::new()
            .max_size(pool_size as u32)
            .build()
            .map_err(|e| BenchError::DbConnectFailed(e.to_string()))?;
        let pool = sz_orm_core::Pool::new(config, Arc::new(factory))
            .map_err(|e| BenchError::DbConnectFailed(e.to_string()))?;
        Ok(Self { pool })
    }

    /// 执行指定工作负载 `rounds` 轮，返回每轮延迟（微秒）
    pub async fn execute(
        &self,
        workload: WorkloadType,
        rounds: u32,
    ) -> Result<Vec<u64>, BenchError> {
        let mut latencies = Vec::with_capacity(rounds as usize);
        for _ in 0..rounds {
            let start = Instant::now();
            self.run_single(workload).await?;
            latencies.push(start.elapsed().as_micros() as u64);
        }
        Ok(latencies)
    }

    async fn run_single(&self, workload: WorkloadType) -> Result<(), BenchError> {
        let mut conn = self.pool.acquire().await.map_err(|e| {
            BenchError::DbConnectFailed(format!("sz-orm pool acquire failed: {e:?}"))
        })?;

        use sz_orm_core::Value;

        match workload {
            WorkloadType::SingleRowQuery => {
                let sql = "SELECT * FROM bench_users WHERE id = ?";
                let params = [Value::I32(1)];
                let rows = conn
                    .query_with_params(sql, &params)
                    .await
                    .map_err(|e| BenchError::QueryFailed(format!("sz-orm query failed: {e:?}")))?;
                if rows.is_empty() {
                    return Err(BenchError::QueryFailed(
                        "SingleRowQuery: expected >= 1 row".into(),
                    ));
                }
            }
            WorkloadType::BatchQuery => {
                let sql = "SELECT * FROM bench_users WHERE id >= ? AND id < ?";
                let params = [Value::I32(1), Value::I32(101)];
                let rows = conn
                    .query_with_params(sql, &params)
                    .await
                    .map_err(|e| BenchError::QueryFailed(format!("sz-orm query failed: {e:?}")))?;
                if rows.len() < 100 {
                    return Err(BenchError::QueryFailed(format!(
                        "BatchQuery: expected >= 100 rows, got {}",
                        rows.len()
                    )));
                }
            }
            WorkloadType::ComplexJoin => {
                let sql = "SELECT a.id, b.name FROM bench_users a \
                           JOIN bench_users b ON a.id = b.id WHERE a.id = ?";
                let params = [Value::I32(1)];
                let rows = conn
                    .query_with_params(sql, &params)
                    .await
                    .map_err(|e| BenchError::QueryFailed(format!("sz-orm query failed: {e:?}")))?;
                if rows.is_empty() {
                    return Err(BenchError::QueryFailed(
                        "ComplexJoin: expected >= 1 row".into(),
                    ));
                }
            }
            WorkloadType::Transaction => {
                conn.begin_transaction()
                    .await
                    .map_err(|e| BenchError::QueryFailed(format!("begin failed: {e:?}")))?;
                for i in 1..=10 {
                    let sql = "SELECT * FROM bench_users WHERE id = ?";
                    let params = [Value::I32(i)];
                    let _ = conn.query_with_params(sql, &params).await.map_err(|e| {
                        BenchError::QueryFailed(format!("sz-orm query failed: {e:?}"))
                    })?;
                }
                conn.commit()
                    .await
                    .map_err(|e| BenchError::QueryFailed(format!("commit failed: {e:?}")))?;
            }
            WorkloadType::PoolConcurrency => {
                let sql = "SELECT * FROM bench_users WHERE id = ?";
                let params = [Value::I32(1)];
                let _ = conn
                    .query_with_params(sql, &params)
                    .await
                    .map_err(|e| BenchError::QueryFailed(format!("sz-orm query failed: {e:?}")))?;
            }
        }
        Ok(())
    }
}

// ── SqlxWorkload ────────────────────────────────────────────────────

/// sqlx 真实查询执行器
pub struct SqlxWorkload {
    sqlite_pool: Option<sqlx::SqlitePool>,
    mysql_pool: Option<sqlx::MySqlPool>,
}

impl SqlxWorkload {
    /// 创建 sqlx SQLite 工作负载执行器
    pub async fn new_sqlite(connection: &str, pool_size: usize) -> Result<Self, BenchError> {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(pool_size as u32)
            .connect(connection)
            .await
            .map_err(|e| BenchError::DbConnectFailed(e.to_string()))?;
        Ok(Self {
            sqlite_pool: Some(pool),
            mysql_pool: None,
        })
    }

    /// 创建 sqlx MySQL 工作负载执行器
    pub async fn new_mysql(connection: &str, pool_size: usize) -> Result<Self, BenchError> {
        let pool = sqlx::mysql::MySqlPoolOptions::new()
            .max_connections(pool_size as u32)
            .connect(connection)
            .await
            .map_err(|e| BenchError::DbConnectFailed(e.to_string()))?;
        Ok(Self {
            sqlite_pool: None,
            mysql_pool: Some(pool),
        })
    }

    /// 执行指定工作负载 `rounds` 轮
    pub async fn execute(
        &self,
        workload: WorkloadType,
        rounds: u32,
    ) -> Result<Vec<u64>, BenchError> {
        let mut latencies = Vec::with_capacity(rounds as usize);
        for _ in 0..rounds {
            let start = Instant::now();
            self.run_single(workload).await?;
            latencies.push(start.elapsed().as_micros() as u64);
        }
        Ok(latencies)
    }

    async fn run_single(&self, workload: WorkloadType) -> Result<(), BenchError> {
        if let Some(pool) = &self.sqlite_pool {
            self.run_sqlite(pool, workload).await
        } else if let Some(pool) = &self.mysql_pool {
            self.run_mysql(pool, workload).await
        } else {
            Err(BenchError::DbConnectFailed("no pool initialized".into()))
        }
    }

    async fn run_sqlite(
        &self,
        pool: &sqlx::SqlitePool,
        workload: WorkloadType,
    ) -> Result<(), BenchError> {
        match workload {
            WorkloadType::SingleRowQuery => {
                let row =
                    sqlx::query("SELECT id, name, email, created_at FROM bench_users WHERE id = ?")
                        .bind(1_i64)
                        .fetch_one(pool)
                        .await
                        .map_err(|e| BenchError::QueryFailed(e.to_string()))?;
                let _id: i64 = row
                    .try_get("id")
                    .map_err(|e| BenchError::QueryFailed(e.to_string()))?;
            }
            WorkloadType::BatchQuery => {
                let rows = sqlx::query(
                    "SELECT id, name, email, created_at FROM bench_users WHERE id >= ? AND id < ?",
                )
                .bind(1_i64)
                .bind(101_i64)
                .fetch_all(pool)
                .await
                .map_err(|e| BenchError::QueryFailed(e.to_string()))?;
                if rows.len() < 100 {
                    return Err(BenchError::QueryFailed(format!(
                        "BatchQuery: expected >= 100 rows, got {}",
                        rows.len()
                    )));
                }
            }
            WorkloadType::ComplexJoin => {
                let rows = sqlx::query(
                    "SELECT a.id, b.name FROM bench_users a \
                     JOIN bench_users b ON a.id = b.id WHERE a.id = ?",
                )
                .bind(1_i64)
                .fetch_all(pool)
                .await
                .map_err(|e| BenchError::QueryFailed(e.to_string()))?;
                if rows.is_empty() {
                    return Err(BenchError::QueryFailed(
                        "ComplexJoin: expected >= 1 row".into(),
                    ));
                }
            }
            WorkloadType::Transaction => {
                let mut tx = pool
                    .begin()
                    .await
                    .map_err(|e| BenchError::QueryFailed(e.to_string()))?;
                for i in 1..=10_i64 {
                    let _ = sqlx::query(
                        "SELECT id, name, email, created_at FROM bench_users WHERE id = ?",
                    )
                    .bind(i)
                    .fetch_one(&mut *tx)
                    .await
                    .map_err(|e| BenchError::QueryFailed(e.to_string()))?;
                }
                tx.commit()
                    .await
                    .map_err(|e| BenchError::QueryFailed(e.to_string()))?;
            }
            WorkloadType::PoolConcurrency => {
                let _ =
                    sqlx::query("SELECT id, name, email, created_at FROM bench_users WHERE id = ?")
                        .bind(1_i64)
                        .fetch_one(pool)
                        .await
                        .map_err(|e| BenchError::QueryFailed(e.to_string()))?;
            }
        }
        Ok(())
    }

    async fn run_mysql(
        &self,
        pool: &sqlx::MySqlPool,
        workload: WorkloadType,
    ) -> Result<(), BenchError> {
        match workload {
            WorkloadType::SingleRowQuery => {
                let row =
                    sqlx::query("SELECT id, name, email, created_at FROM bench_users WHERE id = ?")
                        .bind(1_i64)
                        .fetch_one(pool)
                        .await
                        .map_err(|e| BenchError::QueryFailed(e.to_string()))?;
                let _id: i64 = row
                    .try_get("id")
                    .map_err(|e| BenchError::QueryFailed(e.to_string()))?;
            }
            WorkloadType::BatchQuery => {
                let rows = sqlx::query(
                    "SELECT id, name, email, created_at FROM bench_users WHERE id >= ? AND id < ?",
                )
                .bind(1_i64)
                .bind(101_i64)
                .fetch_all(pool)
                .await
                .map_err(|e| BenchError::QueryFailed(e.to_string()))?;
                if rows.len() < 100 {
                    return Err(BenchError::QueryFailed(format!(
                        "BatchQuery: expected >= 100 rows, got {}",
                        rows.len()
                    )));
                }
            }
            WorkloadType::ComplexJoin => {
                let rows = sqlx::query(
                    "SELECT a.id, b.name FROM bench_users a \
                     JOIN bench_users b ON a.id = b.id WHERE a.id = ?",
                )
                .bind(1_i64)
                .fetch_all(pool)
                .await
                .map_err(|e| BenchError::QueryFailed(e.to_string()))?;
                if rows.is_empty() {
                    return Err(BenchError::QueryFailed(
                        "ComplexJoin: expected >= 1 row".into(),
                    ));
                }
            }
            WorkloadType::Transaction => {
                let mut tx = pool
                    .begin()
                    .await
                    .map_err(|e| BenchError::QueryFailed(e.to_string()))?;
                for i in 1..=10_i64 {
                    let _ = sqlx::query(
                        "SELECT id, name, email, created_at FROM bench_users WHERE id = ?",
                    )
                    .bind(i)
                    .fetch_one(&mut *tx)
                    .await
                    .map_err(|e| BenchError::QueryFailed(e.to_string()))?;
                }
                tx.commit()
                    .await
                    .map_err(|e| BenchError::QueryFailed(e.to_string()))?;
            }
            WorkloadType::PoolConcurrency => {
                let _ =
                    sqlx::query("SELECT id, name, email, created_at FROM bench_users WHERE id = ?")
                        .bind(1_i64)
                        .fetch_one(pool)
                        .await
                        .map_err(|e| BenchError::QueryFailed(e.to_string()))?;
            }
        }
        Ok(())
    }
}

// ── SeaOrmWorkload ──────────────────────────────────────────────────

/// sea-orm 真实查询执行器
pub struct SeaOrmWorkload {
    conn: sea_orm::DatabaseConnection,
}

impl SeaOrmWorkload {
    /// 创建 sea-orm 工作负载执行器
    pub async fn new(connection: &str) -> Result<Self, BenchError> {
        let conn = sea_orm::Database::connect(connection)
            .await
            .map_err(|e| BenchError::DbConnectFailed(e.to_string()))?;
        Ok(Self { conn })
    }

    /// 执行指定工作负载 `rounds` 轮
    pub async fn execute(
        &self,
        workload: WorkloadType,
        rounds: u32,
    ) -> Result<Vec<u64>, BenchError> {
        let mut latencies = Vec::with_capacity(rounds as usize);
        for _ in 0..rounds {
            let start = Instant::now();
            self.run_single(workload).await?;
            latencies.push(start.elapsed().as_micros() as u64);
        }
        Ok(latencies)
    }

    async fn run_single(&self, workload: WorkloadType) -> Result<(), BenchError> {
        use sea_orm::ConnectionTrait;

        match workload {
            WorkloadType::SingleRowQuery => {
                let stmt = sea_orm::Statement::from_sql_and_values(
                    self.conn.get_database_backend(),
                    "SELECT id, name, email, created_at FROM bench_users WHERE id = ?",
                    [1_i32.into()],
                );
                let rows = self
                    .conn
                    .query_all(stmt)
                    .await
                    .map_err(|e| BenchError::QueryFailed(e.to_string()))?;
                if rows.is_empty() {
                    return Err(BenchError::QueryFailed(
                        "SingleRowQuery: expected >= 1 row".into(),
                    ));
                }
            }
            WorkloadType::BatchQuery => {
                let stmt = sea_orm::Statement::from_sql_and_values(
                    self.conn.get_database_backend(),
                    "SELECT id, name, email, created_at FROM bench_users WHERE id >= ? AND id < ?",
                    [1_i32.into(), 101_i32.into()],
                );
                let rows = self
                    .conn
                    .query_all(stmt)
                    .await
                    .map_err(|e| BenchError::QueryFailed(e.to_string()))?;
                if rows.len() < 100 {
                    return Err(BenchError::QueryFailed(format!(
                        "BatchQuery: expected >= 100 rows, got {}",
                        rows.len()
                    )));
                }
            }
            WorkloadType::ComplexJoin => {
                let stmt = sea_orm::Statement::from_sql_and_values(
                    self.conn.get_database_backend(),
                    "SELECT a.id, b.name FROM bench_users a \
                     JOIN bench_users b ON a.id = b.id WHERE a.id = ?",
                    [1_i32.into()],
                );
                let rows = self
                    .conn
                    .query_all(stmt)
                    .await
                    .map_err(|e| BenchError::QueryFailed(e.to_string()))?;
                if rows.is_empty() {
                    return Err(BenchError::QueryFailed(
                        "ComplexJoin: expected >= 1 row".into(),
                    ));
                }
            }
            WorkloadType::Transaction => {
                let txn = self
                    .conn
                    .begin()
                    .await
                    .map_err(|e| BenchError::QueryFailed(e.to_string()))?;
                for i in 1..=10_i32 {
                    let stmt = sea_orm::Statement::from_sql_and_values(
                        txn.get_database_backend(),
                        "SELECT id, name, email, created_at FROM bench_users WHERE id = ?",
                        [i.into()],
                    );
                    let _ = txn
                        .query_all(stmt)
                        .await
                        .map_err(|e| BenchError::QueryFailed(e.to_string()))?;
                }
                txn.commit()
                    .await
                    .map_err(|e| BenchError::QueryFailed(e.to_string()))?;
            }
            WorkloadType::PoolConcurrency => {
                let stmt = sea_orm::Statement::from_sql_and_values(
                    self.conn.get_database_backend(),
                    "SELECT id, name, email, created_at FROM bench_users WHERE id = ?",
                    [1_i32.into()],
                );
                let _ = self
                    .conn
                    .query_all(stmt)
                    .await
                    .map_err(|e| BenchError::QueryFailed(e.to_string()))?;
            }
        }
        Ok(())
    }
}

// ── DieselWorkload（real-bench-diesel feature gate，diesel 暂不可用）─

/// diesel 真实查询执行器（需要 `real-bench-diesel` feature + diesel 依赖）
#[cfg(feature = "real-bench-diesel")]
pub struct DieselWorkload;

#[cfg(feature = "real-bench-diesel")]
impl DieselWorkload {
    /// 创建 diesel 工作负载执行器
    ///
    /// 注意：diesel 依赖不在本地缓存中，此方法在 `real-bench-diesel` feature 下编译。
    /// 启用方式：在 Cargo.toml 添加 `diesel = { version = "2", optional = true }`，
    /// 并在 `real-bench-diesel` feature 中引入 `dep:diesel`。
    pub async fn new(_connection: &str) -> Result<Self, BenchError> {
        Err(BenchError::DbConnectFailed(
            "diesel 依赖未配置：请在 Cargo.toml 添加 diesel 依赖后启用 real-bench-diesel feature"
                .into(),
        ))
    }

    pub async fn execute(
        &self,
        _workload: WorkloadType,
        _rounds: u32,
    ) -> Result<Vec<u64>, BenchError> {
        Err(BenchError::QueryFailed("diesel 依赖未配置".into()))
    }
}

// ── RealDbExecutor ──────────────────────────────────────────────────

/// 真实 DB 执行器：聚合 sz-orm / sqlx / sea-orm / diesel 工作负载
pub struct RealDbExecutor {
    pub config: BenchConfig,
}

impl RealDbExecutor {
    pub fn new(config: BenchConfig) -> Self {
        Self { config }
    }

    /// 执行指定框架和负载，返回延迟数组
    pub async fn run(
        &self,
        framework: FrameworkType,
        workload: WorkloadType,
    ) -> Result<Vec<u64>, BenchError> {
        let backend = crate::validate_db_connection(&self.config.db_connection)?;
        let conn = &self.config.db_connection;
        let pool_size = self.config.pool_size;
        let rounds = self.config.measure_rounds;

        // 预热
        self.warmup(framework, workload).await?;

        match framework {
            FrameworkType::SzOrm => {
                let wl = SzOrmWorkload::new_sqlite(conn, pool_size).await?;
                wl.execute(workload, rounds).await
            }
            FrameworkType::Sqlx => {
                let wl = match backend {
                    DbBackend::Sqlite => SqlxWorkload::new_sqlite(conn, pool_size).await?,
                    DbBackend::Mysql => SqlxWorkload::new_mysql(conn, pool_size).await?,
                };
                wl.execute(workload, rounds).await
            }
            FrameworkType::SeaOrm => {
                let wl = SeaOrmWorkload::new(conn).await?;
                wl.execute(workload, rounds).await
            }
            FrameworkType::Diesel => {
                #[cfg(feature = "real-bench-diesel")]
                {
                    let wl = DieselWorkload::new(conn).await?;
                    wl.execute(workload, rounds).await
                }
                #[cfg(not(feature = "real-bench-diesel"))]
                {
                    Err(BenchError::QueryFailed(
                        "diesel 基准未启用：请使用 --features real-bench-diesel".into(),
                    ))
                }
            }
        }
    }

    async fn warmup(
        &self,
        framework: FrameworkType,
        workload: WorkloadType,
    ) -> Result<(), BenchError> {
        let conn = &self.config.db_connection;
        let pool_size = self.config.pool_size;
        let warmup_rounds = self.config.warmup_rounds;

        match framework {
            FrameworkType::SzOrm => {
                let wl = SzOrmWorkload::new_sqlite(conn, pool_size).await?;
                let _ = wl.execute(workload, warmup_rounds).await?;
            }
            FrameworkType::Sqlx => {
                let backend = crate::validate_db_connection(conn)?;
                let wl = match backend {
                    DbBackend::Sqlite => SqlxWorkload::new_sqlite(conn, pool_size).await?,
                    DbBackend::Mysql => SqlxWorkload::new_mysql(conn, pool_size).await?,
                };
                let _ = wl.execute(workload, warmup_rounds).await?;
            }
            FrameworkType::SeaOrm => {
                let wl = SeaOrmWorkload::new(conn).await?;
                let _ = wl.execute(workload, warmup_rounds).await?;
            }
            FrameworkType::Diesel => {
                // diesel 预热跳过（feature 未启用时）
                #[cfg(feature = "real-bench-diesel")]
                {
                    let wl = DieselWorkload::new(conn).await?;
                    let _ = wl.execute(workload, warmup_rounds).await?;
                }
            }
        }
        // 短暂让出调度器
        sleep(std::time::Duration::from_micros(100)).await;
        Ok(())
    }
}

// ── run_workload_real ───────────────────────────────────────────────

/// 使用真实 DB 执行基准测试，替代 `run_workload()` 的延迟模型模拟
pub async fn run_workload_real(
    framework: FrameworkType,
    workload: WorkloadType,
    config: &BenchConfig,
) -> Result<BenchResult, BenchError> {
    config.validate()?;

    let backend = crate::validate_db_connection(&config.db_connection)?;
    DatasetInitializer::init(backend, &config.db_connection, config.dataset_size).await?;

    let executor = RealDbExecutor::new(config.clone());
    let latencies = executor.run(framework, workload).await?;

    let mem = crate::MemoryMetrics::capture();
    Ok(BenchResult::from_latencies(
        framework,
        workload,
        latencies,
        mem.peak_rss_kb,
        mem.alloc_count,
        mem.alloc_bytes,
        true,
    ))
}
