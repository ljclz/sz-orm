//! 真实数据库 benchmark：SZ-ORM vs SeaORM vs SQLx（MySQL + PostgreSQL）
//!
//! 用法：
//!   cargo run --release -- --mysql URL --postgres URL
//!   cargo run --release -- --mysql URL --trials 5
//!   DATABASE_URL_MYSQL=... DATABASE_URL_POSTGRES=... cargo run --release
//!
//! 测试场景（远程 WAN 下每行 INSERT 约需 19ms 网络RTT，数据量统一下调至 1K）：
//!   - INSERT 1K          （逐行插入 1000 行）
//!   - SELECT BY ID       （预插入 1K 行，100 次单行查询）
//!   - SELECT ALL 1K      （预插入 1K 行，全表查询）
//!   - UPDATE             （预插入 1K 行，100 次单行更新）
//!   - DELETE 1K          （预插入 1K 行，逐行删除）
//!
//! Setup 阶段使用批量 INSERT（100 行/批次）加速预插入，不纳入计时。

use std::env;
use std::sync::Arc;
use std::time::{Duration, Instant};

use sea_orm::{
    ConnectOptions, ConnectionTrait, Database, DatabaseBackend, DatabaseConnection, Statement,
};
use sz_orm_core::{Pool as SzPool, PoolConfigBuilder};
use sz_orm_sqlx::{
    MySqlPoolHandle, PgPoolHandle, SqlxMySqlConnectionFactory, SqlxPgConnectionFactory,
};

type BoxError = Box<dyn std::error::Error + Send + Sync>;

// ============================================================================
// SQL 常量
// ============================================================================

const MYSQL_CREATE_TABLE: &str = "CREATE TABLE IF NOT EXISTS bench_users (
    id BIGINT PRIMARY KEY AUTO_INCREMENT,
    name VARCHAR(255) NOT NULL,
    email VARCHAR(255),
    age INT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
)";

const PG_CREATE_TABLE: &str = "CREATE TABLE IF NOT EXISTS bench_users (
    id BIGSERIAL PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    email VARCHAR(255),
    age INT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
)";

const DROP_TABLE: &str = "DROP TABLE IF EXISTS bench_users";
const TRUNCATE_MYSQL: &str = "TRUNCATE TABLE bench_users";
const TRUNCATE_PG: &str = "TRUNCATE TABLE bench_users RESTART IDENTITY";

// ============================================================================
// CLI 参数
// ============================================================================

struct Args {
    mysql: Option<String>,
    postgres: Option<String>,
    trials: usize,
}

fn parse_args() -> Args {
    let mut args = Args {
        mysql: env::var("DATABASE_URL_MYSQL").ok(),
        postgres: env::var("DATABASE_URL_POSTGRES").ok(),
        trials: 3,
    };
    let mut iter = env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--mysql" => {
                if let Some(v) = iter.next() {
                    args.mysql = Some(v);
                }
            }
            "--postgres" => {
                if let Some(v) = iter.next() {
                    args.postgres = Some(v);
                }
            }
            "--trials" => {
                if let Some(v) = iter.next() {
                    if let Ok(n) = v.parse() {
                        args.trials = n;
                    }
                }
            }
            _ => {}
        }
    }
    args
}

// ============================================================================
// 辅助函数
// ============================================================================

fn fmt_dur(d: Duration) -> String {
    let ns = d.as_nanos();
    if ns < 1_000 {
        format!("{} ns", ns)
    } else if ns < 1_000_000 {
        format!("{:.2} µs", ns as f64 / 1_000.0)
    } else if ns < 1_000_000_000 {
        format!("{:.2} ms", ns as f64 / 1_000_000.0)
    } else {
        format!("{:.2} s", ns as f64 / 1_000_000_000.0)
    }
}

fn median(mut durs: Vec<Duration>) -> Duration {
    if durs.is_empty() {
        return Duration::default();
    }
    durs.sort();
    durs[durs.len() / 2]
}

fn print_header(trials: usize) {
    let mut header = String::from("| ORM |");
    let mut sep = String::from("|-----|");
    for i in 0..trials {
        header.push_str(&format!(" Trial {} |", i + 1));
        sep.push_str("------|");
    }
    header.push_str(" Median |");
    sep.push_str("--------|");
    println!("{}", header);
    println!("{}", sep);
}

fn print_row(name: &str, times: &[Duration]) {
    let med = median(times.to_vec());
    let trials_str: Vec<String> = times.iter().map(|t| fmt_dur(*t)).collect();
    println!("| {} | {} | {} |", name, trials_str.join(" | "), fmt_dur(med));
}

/// 批量插入数据用于 setup（MySQL，使用多行 VALUES 加速远程 DB 写入）
///
/// 将 N 行拆分为 batch_size 行/批次，减少网络 RTT 次数。
async fn batch_insert_mysql(sqlx_pool: &sqlx::MySqlPool, n: usize, batch_size: usize) -> Result<(), BoxError> {
    let mut inserted = 0;
    while inserted < n {
        let batch_end = (inserted + batch_size).min(n);
        let mut sql = String::from("INSERT INTO bench_users (name, email, age) VALUES ");
        for i in inserted..batch_end {
            if i > inserted {
                sql.push(',');
            }
            sql.push_str(&format!(
                "('user_{}', 'user_{}@test.com', {})",
                i, i, i % 100
            ));
        }
        let sql_ref = sql;
        sqlx::query(sqlx::AssertSqlSafe(&*sql_ref))
            .execute(sqlx_pool)
            .await?;
        inserted = batch_end;
    }
    Ok(())
}

/// 批量插入数据用于 setup（PostgreSQL，使用多行 VALUES 加速远程 DB 写入）
async fn batch_insert_pg(sqlx_pool: &sqlx::PgPool, n: usize, batch_size: usize) -> Result<(), BoxError> {
    let mut inserted = 0;
    while inserted < n {
        let batch_end = (inserted + batch_size).min(n);
        let mut sql = String::from("INSERT INTO bench_users (name, email, age) VALUES ");
        for i in inserted..batch_end {
            if i > inserted {
                sql.push(',');
            }
            sql.push_str(&format!(
                "('user_{}', 'user_{}@test.com', {})",
                i, i, i % 100
            ));
        }
        let sql_ref = sql;
        sqlx::query(sqlx::AssertSqlSafe(&*sql_ref))
            .execute(sqlx_pool)
            .await?;
        inserted = batch_end;
    }
    Ok(())
}

/// 屏蔽密码后输出 URL
fn mask_url(url: &str) -> String {
    // mysql://user:pass@host:port/db  ->  mysql://user:***@host:port/db
    if let Some(at) = url.find('@') {
        if let Some(colon) = url.find(':') {
            if colon < at {
                let scheme_end = url.find("://").map(|i| i + 3).unwrap_or(0);
                if scheme_end <= colon {
                    let mut masked = String::new();
                    masked.push_str(&url[..colon + 1]);
                    masked.push_str("***");
                    masked.push_str(&url[at..]);
                    return masked;
                }
            }
        }
    }
    url.to_string()
}

// ============================================================================
// MySQL benchmark
// ============================================================================

/// 确保 MySQL 数据库存在，不存在则创建
///
/// 解析 URL 中的数据库名，连接到 mysql 系统库执行 CREATE DATABASE IF NOT EXISTS。
async fn ensure_mysql_database(url: &str) -> Result<(), BoxError> {
    if let Some(db_start) = url.rfind('/') {
        let db_name = &url[db_start + 1..];
        if !db_name.is_empty() {
            let base_url = format!("{}/mysql", &url[..db_start]);
            let pool = sqlx::mysql::MySqlPoolOptions::new()
                .max_connections(1)
                .connect(&base_url)
                .await?;
            let create_sql = format!("CREATE DATABASE IF NOT EXISTS `{}`", db_name);
            sqlx::query(sqlx::AssertSqlSafe(&*create_sql))
                .execute(&pool)
                .await?;
            pool.close().await;
            println!("[setup] MySQL database '{}' ensured", db_name);
        }
    }
    Ok(())
}

async fn run_mysql_bench(url: &str, trials: usize) -> Result<(), BoxError> {
    println!("## MySQL Benchmark\n");
    println!("Connection: `{}`", mask_url(url));
    println!("Trials per scenario: {}\n", trials);

    // 确保数据库存在
    ensure_mysql_database(url).await?;

    // SZ-ORM MySQL 连接池（max_connections=10，与 SQLite benchmark 一致）
    let sz_handle = Arc::new(MySqlPoolHandle::connect(url).await?);
    let sz_factory = Arc::new(SqlxMySqlConnectionFactory::new(sz_handle.clone()));
    let sz_config = PoolConfigBuilder::new().max_size(10).build()?;
    let sz_pool = SzPool::new(sz_config, sz_factory)?;

    // SQLx MySQL 连接池
    let sqlx_pool = sqlx::mysql::MySqlPoolOptions::new()
        .max_connections(10)
        .connect(url)
        .await?;

    // SeaORM MySQL
    let mut opt = ConnectOptions::new(url.to_string());
    opt.max_connections(10);
    let sea_db = Database::connect(opt).await?;

    // 创建表
    sqlx::query(MYSQL_CREATE_TABLE).execute(&sqlx_pool).await?;

    // 执行各场景
    // 注：远程 WAN 下每行 INSERT 约需 19ms（网络 RTT），
    // 10K 行单 trial 即需 3 分钟以上，故数据量统一下调至 1K
    bench_insert_mysql(&sz_pool, &sqlx_pool, &sea_db, 1000, trials).await?;
    bench_select_by_id_mysql(&sz_pool, &sqlx_pool, &sea_db, 1000, 100, trials).await?;
    bench_select_all_mysql(&sz_pool, &sqlx_pool, &sea_db, 1000, trials).await?;
    bench_update_mysql(&sz_pool, &sqlx_pool, &sea_db, 1000, 100, trials).await?;
    bench_delete_mysql(&sz_pool, &sqlx_pool, &sea_db, 1000, trials).await?;

    // 清理
    sqlx::query(DROP_TABLE).execute(&sqlx_pool).await?;
    sz_pool.close_all().await;
    sqlx_pool.close().await;
    let _ = sea_db.close().await;

    Ok(())
}

async fn bench_insert_mysql(
    sz_pool: &SzPool,
    sqlx_pool: &sqlx::MySqlPool,
    sea_db: &DatabaseConnection,
    n: usize,
    trials: usize,
) -> Result<(), BoxError> {
    println!("### INSERT {} (MySQL)\n", n);
    print_header(trials);

    // SZ-ORM
    let mut sz_times = Vec::new();
    for _ in 0..trials {
        sqlx::query(TRUNCATE_MYSQL).execute(sqlx_pool).await?;
        let start = Instant::now();
        for i in 0..n {
            let sql = format!(
                "INSERT INTO bench_users (name, email, age) VALUES ('{}', '{}', {})",
                format!("user_{}", i).replace('\'', "''"),
                format!("user_{}@test.com", i).replace('\'', "''"),
                i % 100
            );
            let mut conn = sz_pool.acquire().await?;
            conn.execute(&sql).await?;
        }
        sz_times.push(start.elapsed());
    }
    print_row("sz-orm", &sz_times);

    // SQLx
    let mut sqlx_times = Vec::new();
    for _ in 0..trials {
        sqlx::query(TRUNCATE_MYSQL).execute(sqlx_pool).await?;
        let start = Instant::now();
        for i in 0..n {
            sqlx::query("INSERT INTO bench_users (name, email, age) VALUES (?, ?, ?)")
                .bind(format!("user_{}", i))
                .bind(format!("user_{}@test.com", i))
                .bind((i % 100) as i32)
                .execute(sqlx_pool)
                .await?;
        }
        sqlx_times.push(start.elapsed());
    }
    print_row("sqlx", &sqlx_times);

    // SeaORM
    let mut sea_times = Vec::new();
    for _ in 0..trials {
        sqlx::query(TRUNCATE_MYSQL).execute(sqlx_pool).await?;
        let start = Instant::now();
        for i in 0..n {
            sea_db.execute(Statement::from_sql_and_values(
                DatabaseBackend::MySql,
                "INSERT INTO bench_users (name, email, age) VALUES (?, ?, ?)",
                vec![
                    format!("user_{}", i).into(),
                    format!("user_{}@test.com", i).into(),
                    ((i % 100) as i32).into(),
                ],
            ))
            .await?;
        }
        sea_times.push(start.elapsed());
    }
    print_row("sea-orm", &sea_times);

    println!();
    Ok(())
}

async fn bench_select_by_id_mysql(
    sz_pool: &SzPool,
    sqlx_pool: &sqlx::MySqlPool,
    sea_db: &DatabaseConnection,
    prepare_n: usize,
    queries: usize,
    trials: usize,
) -> Result<(), BoxError> {
    println!(
        "### SELECT BY ID (MySQL, pre-insert {} rows, {} queries)\n",
        prepare_n, queries
    );
    print_header(trials);

    // 预插入数据（批量插入加速 setup）
    sqlx::query(TRUNCATE_MYSQL).execute(sqlx_pool).await?;
    batch_insert_mysql(sqlx_pool, prepare_n, 100).await?;

    // SZ-ORM
    let mut sz_times = Vec::new();
    for _ in 0..trials {
        let start = Instant::now();
        for i in 0..queries {
            let id = (i % prepare_n) as i64 + 1;
            let sql = format!("SELECT name, email, age FROM bench_users WHERE id = {}", id);
            let mut conn = sz_pool.acquire().await?;
            let rows = conn.query(&sql).await?;
            let _ = rows.first();
        }
        sz_times.push(start.elapsed());
    }
    print_row("sz-orm", &sz_times);

    // SQLx
    let mut sqlx_times = Vec::new();
    for _ in 0..trials {
        let start = Instant::now();
        for i in 0..queries {
            let id = (i % prepare_n) as i64 + 1;
            let _ = sqlx::query("SELECT name, email, age FROM bench_users WHERE id = ?")
                .bind(id)
                .fetch_one(sqlx_pool)
                .await?;
        }
        sqlx_times.push(start.elapsed());
    }
    print_row("sqlx", &sqlx_times);

    // SeaORM
    let mut sea_times = Vec::new();
    for _ in 0..trials {
        let start = Instant::now();
        for i in 0..queries {
            let id = (i % prepare_n) as i64 + 1;
            let _ = sea_db
                .query_one(Statement::from_sql_and_values(
                    DatabaseBackend::MySql,
                    "SELECT name, email, age FROM bench_users WHERE id = ?",
                    vec![id.into()],
                ))
                .await?;
        }
        sea_times.push(start.elapsed());
    }
    print_row("sea-orm", &sea_times);

    println!();
    Ok(())
}

async fn bench_select_all_mysql(
    sz_pool: &SzPool,
    sqlx_pool: &sqlx::MySqlPool,
    sea_db: &DatabaseConnection,
    n: usize,
    trials: usize,
) -> Result<(), BoxError> {
    println!("### SELECT ALL (MySQL, {} rows)\n", n);
    print_header(trials);

    // 预插入数据（批量插入加速 setup）
    sqlx::query(TRUNCATE_MYSQL).execute(sqlx_pool).await?;
    batch_insert_mysql(sqlx_pool, n, 100).await?;

    // SZ-ORM
    let mut sz_times = Vec::new();
    for _ in 0..trials {
        let start = Instant::now();
        let mut conn = sz_pool.acquire().await?;
        let rows = conn.query("SELECT id, name, email, age FROM bench_users").await?;
        sz_times.push(start.elapsed());
        let _ = rows.len();
    }
    print_row("sz-orm", &sz_times);

    // SQLx
    let mut sqlx_times = Vec::new();
    for _ in 0..trials {
        let start = Instant::now();
        let rows = sqlx::query("SELECT id, name, email, age FROM bench_users")
            .fetch_all(sqlx_pool)
            .await?;
        sqlx_times.push(start.elapsed());
        let _ = rows.len();
    }
    print_row("sqlx", &sqlx_times);

    // SeaORM
    let mut sea_times = Vec::new();
    for _ in 0..trials {
        let start = Instant::now();
        let rows = sea_db
            .query_all(Statement::from_sql_and_values(
                DatabaseBackend::MySql,
                "SELECT id, name, email, age FROM bench_users",
                vec![],
            ))
            .await?;
        sea_times.push(start.elapsed());
        let _ = rows.len();
    }
    print_row("sea-orm", &sea_times);

    println!();
    Ok(())
}

async fn bench_update_mysql(
    sz_pool: &SzPool,
    sqlx_pool: &sqlx::MySqlPool,
    sea_db: &DatabaseConnection,
    prepare_n: usize,
    updates: usize,
    trials: usize,
) -> Result<(), BoxError> {
    println!(
        "### UPDATE (MySQL, pre-insert {} rows, {} updates)\n",
        prepare_n, updates
    );
    print_header(trials);

    // 预插入数据（批量插入加速 setup）
    sqlx::query(TRUNCATE_MYSQL).execute(sqlx_pool).await?;
    batch_insert_mysql(sqlx_pool, prepare_n, 100).await?;

    // SZ-ORM
    let mut sz_times = Vec::new();
    for _ in 0..trials {
        let start = Instant::now();
        for i in 0..updates {
            let id = (i % prepare_n) as i64 + 1;
            let sql = format!(
                "UPDATE bench_users SET name = '{}' WHERE id = {}",
                "updated_name".replace('\'', "''"),
                id
            );
            let mut conn = sz_pool.acquire().await?;
            conn.execute(&sql).await?;
        }
        sz_times.push(start.elapsed());
    }
    print_row("sz-orm", &sz_times);

    // SQLx
    let mut sqlx_times = Vec::new();
    for _ in 0..trials {
        let start = Instant::now();
        for i in 0..updates {
            let id = (i % prepare_n) as i64 + 1;
            sqlx::query("UPDATE bench_users SET name = ? WHERE id = ?")
                .bind("updated_name")
                .bind(id)
                .execute(sqlx_pool)
                .await?;
        }
        sqlx_times.push(start.elapsed());
    }
    print_row("sqlx", &sqlx_times);

    // SeaORM
    let mut sea_times = Vec::new();
    for _ in 0..trials {
        let start = Instant::now();
        for i in 0..updates {
            let id = (i % prepare_n) as i64 + 1;
            sea_db.execute(Statement::from_sql_and_values(
                DatabaseBackend::MySql,
                "UPDATE bench_users SET name = ? WHERE id = ?",
                vec!["updated_name".into(), id.into()],
            ))
            .await?;
        }
        sea_times.push(start.elapsed());
    }
    print_row("sea-orm", &sea_times);

    println!();
    Ok(())
}

async fn bench_delete_mysql(
    sz_pool: &SzPool,
    sqlx_pool: &sqlx::MySqlPool,
    sea_db: &DatabaseConnection,
    n: usize,
    trials: usize,
) -> Result<(), BoxError> {
    println!("### DELETE (MySQL, delete {} rows one-by-one)\n", n);
    print_header(trials);

    // SZ-ORM
    let mut sz_times = Vec::new();
    for _ in 0..trials {
        // 每个 trial 前重新插入
        sqlx::query(TRUNCATE_MYSQL).execute(sqlx_pool).await?;
        batch_insert_mysql(sqlx_pool, n, 100).await?;
        let start = Instant::now();
        for id in 1..=(n as i64) {
            let sql = format!("DELETE FROM bench_users WHERE id = {}", id);
            let mut conn = sz_pool.acquire().await?;
            conn.execute(&sql).await?;
        }
        sz_times.push(start.elapsed());
    }
    print_row("sz-orm", &sz_times);

    // SQLx
    let mut sqlx_times = Vec::new();
    for _ in 0..trials {
        sqlx::query(TRUNCATE_MYSQL).execute(sqlx_pool).await?;
        batch_insert_mysql(sqlx_pool, n, 100).await?;
        let start = Instant::now();
        for id in 1..=(n as i64) {
            sqlx::query("DELETE FROM bench_users WHERE id = ?")
                .bind(id)
                .execute(sqlx_pool)
                .await?;
        }
        sqlx_times.push(start.elapsed());
    }
    print_row("sqlx", &sqlx_times);

    // SeaORM
    let mut sea_times = Vec::new();
    for _ in 0..trials {
        sqlx::query(TRUNCATE_MYSQL).execute(sqlx_pool).await?;
        batch_insert_mysql(sqlx_pool, n, 100).await?;
        let start = Instant::now();
        for id in 1..=(n as i64) {
            sea_db.execute(Statement::from_sql_and_values(
                DatabaseBackend::MySql,
                "DELETE FROM bench_users WHERE id = ?",
                vec![id.into()],
            ))
            .await?;
        }
        sea_times.push(start.elapsed());
    }
    print_row("sea-orm", &sea_times);

    println!();
    Ok(())
}

// ============================================================================
// PostgreSQL benchmark
// ============================================================================

/// 确保 PostgreSQL 数据库存在，不存在则创建
///
/// 解析 URL 中的数据库名，连接到 postgres 系统库检查并创建数据库。
async fn ensure_pg_database(url: &str) -> Result<(), BoxError> {
    if let Some(db_start) = url.rfind('/') {
        let db_name = &url[db_start + 1..];
        if !db_name.is_empty() {
            let base_url = format!("{}/postgres", &url[..db_start]);
            let pool = sqlx::postgres::PgPoolOptions::new()
                .max_connections(1)
                .connect(&base_url)
                .await?;
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname = $1)",
            )
            .bind(db_name)
            .fetch_one(&pool)
            .await?;
            if !exists {
                let create_sql = format!("CREATE DATABASE \"{}\"", db_name);
                sqlx::query(sqlx::AssertSqlSafe(&*create_sql))
                    .execute(&pool)
                    .await?;
                println!("[setup] PostgreSQL database '{}' created", db_name);
            } else {
                println!("[setup] PostgreSQL database '{}' exists", db_name);
            }
            pool.close().await;
        }
    }
    Ok(())
}

async fn run_pg_bench(url: &str, trials: usize) -> Result<(), BoxError> {
    println!("## PostgreSQL Benchmark\n");
    println!("Connection: `{}`", mask_url(url));
    println!("Trials per scenario: {}\n", trials);

    // 确保数据库存在
    ensure_pg_database(url).await?;

    // SZ-ORM PG 连接池
    let sz_handle = Arc::new(PgPoolHandle::connect(url).await?);
    let sz_factory = Arc::new(SqlxPgConnectionFactory::new(sz_handle.clone()));
    let sz_config = PoolConfigBuilder::new().max_size(10).build()?;
    let sz_pool = SzPool::new(sz_config, sz_factory)?;

    // SQLx PG 连接池
    let sqlx_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(10)
        .connect(url)
        .await?;

    // SeaORM PG
    let mut opt = ConnectOptions::new(url.to_string());
    opt.max_connections(10);
    let sea_db = Database::connect(opt).await?;

    // 创建表
    sqlx::query(PG_CREATE_TABLE).execute(&sqlx_pool).await?;

    // 执行各场景
    // 注：远程 WAN 下每行 INSERT 约需 19ms（网络 RTT），
    // 10K 行单 trial 即需 3 分钟以上，故数据量统一下调至 1K
    bench_insert_pg(&sz_pool, &sqlx_pool, &sea_db, 1000, trials).await?;
    bench_select_by_id_pg(&sz_pool, &sqlx_pool, &sea_db, 1000, 100, trials).await?;
    bench_select_all_pg(&sz_pool, &sqlx_pool, &sea_db, 1000, trials).await?;
    bench_update_pg(&sz_pool, &sqlx_pool, &sea_db, 1000, 100, trials).await?;
    bench_delete_pg(&sz_pool, &sqlx_pool, &sea_db, 1000, trials).await?;

    // 清理
    sqlx::query(DROP_TABLE).execute(&sqlx_pool).await?;
    sz_pool.close_all().await;
    sqlx_pool.close().await;
    let _ = sea_db.close().await;

    Ok(())
}

async fn bench_insert_pg(
    sz_pool: &SzPool,
    sqlx_pool: &sqlx::PgPool,
    sea_db: &DatabaseConnection,
    n: usize,
    trials: usize,
) -> Result<(), BoxError> {
    println!("### INSERT {} (PostgreSQL)\n", n);
    print_header(trials);

    // SZ-ORM
    let mut sz_times = Vec::new();
    for _ in 0..trials {
        sqlx::query(TRUNCATE_PG).execute(sqlx_pool).await?;
        let start = Instant::now();
        for i in 0..n {
            let sql = format!(
                "INSERT INTO bench_users (name, email, age) VALUES ('{}', '{}', {})",
                format!("user_{}", i).replace('\'', "''"),
                format!("user_{}@test.com", i).replace('\'', "''"),
                i % 100
            );
            let mut conn = sz_pool.acquire().await?;
            conn.execute(&sql).await?;
        }
        sz_times.push(start.elapsed());
    }
    print_row("sz-orm", &sz_times);

    // SQLx
    let mut sqlx_times = Vec::new();
    for _ in 0..trials {
        sqlx::query(TRUNCATE_PG).execute(sqlx_pool).await?;
        let start = Instant::now();
        for i in 0..n {
            sqlx::query("INSERT INTO bench_users (name, email, age) VALUES ($1, $2, $3)")
                .bind(format!("user_{}", i))
                .bind(format!("user_{}@test.com", i))
                .bind((i % 100) as i32)
                .execute(sqlx_pool)
                .await?;
        }
        sqlx_times.push(start.elapsed());
    }
    print_row("sqlx", &sqlx_times);

    // SeaORM
    let mut sea_times = Vec::new();
    for _ in 0..trials {
        sqlx::query(TRUNCATE_PG).execute(sqlx_pool).await?;
        let start = Instant::now();
        for i in 0..n {
            sea_db.execute(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                "INSERT INTO bench_users (name, email, age) VALUES ($1, $2, $3)",
                vec![
                    format!("user_{}", i).into(),
                    format!("user_{}@test.com", i).into(),
                    ((i % 100) as i32).into(),
                ],
            ))
            .await?;
        }
        sea_times.push(start.elapsed());
    }
    print_row("sea-orm", &sea_times);

    println!();
    Ok(())
}

async fn bench_select_by_id_pg(
    sz_pool: &SzPool,
    sqlx_pool: &sqlx::PgPool,
    sea_db: &DatabaseConnection,
    prepare_n: usize,
    queries: usize,
    trials: usize,
) -> Result<(), BoxError> {
    println!(
        "### SELECT BY ID (PostgreSQL, pre-insert {} rows, {} queries)\n",
        prepare_n, queries
    );
    print_header(trials);

    // 预插入数据（批量插入加速 setup）
    sqlx::query(TRUNCATE_PG).execute(sqlx_pool).await?;
    batch_insert_pg(sqlx_pool, prepare_n, 100).await?;

    // SZ-ORM
    let mut sz_times = Vec::new();
    for _ in 0..trials {
        let start = Instant::now();
        for i in 0..queries {
            let id = (i % prepare_n) as i64 + 1;
            let sql = format!("SELECT name, email, age FROM bench_users WHERE id = {}", id);
            let mut conn = sz_pool.acquire().await?;
            let rows = conn.query(&sql).await?;
            let _ = rows.first();
        }
        sz_times.push(start.elapsed());
    }
    print_row("sz-orm", &sz_times);

    // SQLx
    let mut sqlx_times = Vec::new();
    for _ in 0..trials {
        let start = Instant::now();
        for i in 0..queries {
            let id = (i % prepare_n) as i64 + 1;
            let _ = sqlx::query("SELECT name, email, age FROM bench_users WHERE id = $1")
                .bind(id)
                .fetch_one(sqlx_pool)
                .await?;
        }
        sqlx_times.push(start.elapsed());
    }
    print_row("sqlx", &sqlx_times);

    // SeaORM
    let mut sea_times = Vec::new();
    for _ in 0..trials {
        let start = Instant::now();
        for i in 0..queries {
            let id = (i % prepare_n) as i64 + 1;
            let _ = sea_db
                .query_one(Statement::from_sql_and_values(
                    DatabaseBackend::Postgres,
                    "SELECT name, email, age FROM bench_users WHERE id = $1",
                    vec![id.into()],
                ))
                .await?;
        }
        sea_times.push(start.elapsed());
    }
    print_row("sea-orm", &sea_times);

    println!();
    Ok(())
}

async fn bench_select_all_pg(
    sz_pool: &SzPool,
    sqlx_pool: &sqlx::PgPool,
    sea_db: &DatabaseConnection,
    n: usize,
    trials: usize,
) -> Result<(), BoxError> {
    println!("### SELECT ALL (PostgreSQL, {} rows)\n", n);
    print_header(trials);

    // 预插入数据（批量插入加速 setup）
    sqlx::query(TRUNCATE_PG).execute(sqlx_pool).await?;
    batch_insert_pg(sqlx_pool, n, 100).await?;

    // SZ-ORM
    let mut sz_times = Vec::new();
    for _ in 0..trials {
        let start = Instant::now();
        let mut conn = sz_pool.acquire().await?;
        let rows = conn.query("SELECT id, name, email, age FROM bench_users").await?;
        sz_times.push(start.elapsed());
        let _ = rows.len();
    }
    print_row("sz-orm", &sz_times);

    // SQLx
    let mut sqlx_times = Vec::new();
    for _ in 0..trials {
        let start = Instant::now();
        let rows = sqlx::query("SELECT id, name, email, age FROM bench_users")
            .fetch_all(sqlx_pool)
            .await?;
        sqlx_times.push(start.elapsed());
        let _ = rows.len();
    }
    print_row("sqlx", &sqlx_times);

    // SeaORM
    let mut sea_times = Vec::new();
    for _ in 0..trials {
        let start = Instant::now();
        let rows = sea_db
            .query_all(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                "SELECT id, name, email, age FROM bench_users",
                vec![],
            ))
            .await?;
        sea_times.push(start.elapsed());
        let _ = rows.len();
    }
    print_row("sea-orm", &sea_times);

    println!();
    Ok(())
}

async fn bench_update_pg(
    sz_pool: &SzPool,
    sqlx_pool: &sqlx::PgPool,
    sea_db: &DatabaseConnection,
    prepare_n: usize,
    updates: usize,
    trials: usize,
) -> Result<(), BoxError> {
    println!(
        "### UPDATE (PostgreSQL, pre-insert {} rows, {} updates)\n",
        prepare_n, updates
    );
    print_header(trials);

    // 预插入数据（批量插入加速 setup）
    sqlx::query(TRUNCATE_PG).execute(sqlx_pool).await?;
    batch_insert_pg(sqlx_pool, prepare_n, 100).await?;

    // SZ-ORM
    let mut sz_times = Vec::new();
    for _ in 0..trials {
        let start = Instant::now();
        for i in 0..updates {
            let id = (i % prepare_n) as i64 + 1;
            let sql = format!(
                "UPDATE bench_users SET name = '{}' WHERE id = {}",
                "updated_name".replace('\'', "''"),
                id
            );
            let mut conn = sz_pool.acquire().await?;
            conn.execute(&sql).await?;
        }
        sz_times.push(start.elapsed());
    }
    print_row("sz-orm", &sz_times);

    // SQLx
    let mut sqlx_times = Vec::new();
    for _ in 0..trials {
        let start = Instant::now();
        for i in 0..updates {
            let id = (i % prepare_n) as i64 + 1;
            sqlx::query("UPDATE bench_users SET name = $1 WHERE id = $2")
                .bind("updated_name")
                .bind(id)
                .execute(sqlx_pool)
                .await?;
        }
        sqlx_times.push(start.elapsed());
    }
    print_row("sqlx", &sqlx_times);

    // SeaORM
    let mut sea_times = Vec::new();
    for _ in 0..trials {
        let start = Instant::now();
        for i in 0..updates {
            let id = (i % prepare_n) as i64 + 1;
            sea_db.execute(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                "UPDATE bench_users SET name = $1 WHERE id = $2",
                vec!["updated_name".into(), id.into()],
            ))
            .await?;
        }
        sea_times.push(start.elapsed());
    }
    print_row("sea-orm", &sea_times);

    println!();
    Ok(())
}

async fn bench_delete_pg(
    sz_pool: &SzPool,
    sqlx_pool: &sqlx::PgPool,
    sea_db: &DatabaseConnection,
    n: usize,
    trials: usize,
) -> Result<(), BoxError> {
    println!("### DELETE (PostgreSQL, delete {} rows one-by-one)\n", n);
    print_header(trials);

    // SZ-ORM
    let mut sz_times = Vec::new();
    for _ in 0..trials {
        sqlx::query(TRUNCATE_PG).execute(sqlx_pool).await?;
        batch_insert_pg(sqlx_pool, n, 100).await?;
        let start = Instant::now();
        for id in 1..=(n as i64) {
            let sql = format!("DELETE FROM bench_users WHERE id = {}", id);
            let mut conn = sz_pool.acquire().await?;
            conn.execute(&sql).await?;
        }
        sz_times.push(start.elapsed());
    }
    print_row("sz-orm", &sz_times);

    // SQLx
    let mut sqlx_times = Vec::new();
    for _ in 0..trials {
        sqlx::query(TRUNCATE_PG).execute(sqlx_pool).await?;
        batch_insert_pg(sqlx_pool, n, 100).await?;
        let start = Instant::now();
        for id in 1..=(n as i64) {
            sqlx::query("DELETE FROM bench_users WHERE id = $1")
                .bind(id)
                .execute(sqlx_pool)
                .await?;
        }
        sqlx_times.push(start.elapsed());
    }
    print_row("sqlx", &sqlx_times);

    // SeaORM
    let mut sea_times = Vec::new();
    for _ in 0..trials {
        sqlx::query(TRUNCATE_PG).execute(sqlx_pool).await?;
        batch_insert_pg(sqlx_pool, n, 100).await?;
        let start = Instant::now();
        for id in 1..=(n as i64) {
            sea_db.execute(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                "DELETE FROM bench_users WHERE id = $1",
                vec![id.into()],
            ))
            .await?;
        }
        sea_times.push(start.elapsed());
    }
    print_row("sea-orm", &sea_times);

    println!();
    Ok(())
}

// ============================================================================
// main
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), BoxError> {
    let args = parse_args();

    println!("# SZ-ORM Benchmark Results (Real DBs)");
    println!();
    println!("Generated: {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"));
    println!();
    println!("Scenarios (per DB, 3 ORMs: SZ-ORM / SQLx / SeaORM):");
    println!("- INSERT 1K          (远程 WAN 下 10K/100K 耗时过长，统一下调至 1K)");
    println!("- SELECT BY ID       (预插入 1K 行，100 次单行查询)");
    println!("- SELECT ALL 1K      (预插入 1K 行，全表查询)");
    println!("- UPDATE             (预插入 1K 行，100 次单行更新)");
    println!("- DELETE 1K          (预插入 1K 行，逐行删除)");
    println!();
    println!("Connection pool: max_connections=10 (与 SQLite benchmark 一致)");
    println!("Note: 远程云数据库，网络延迟会影响绝对数值，但对比基准一致");
    println!();

    if let Some(url) = &args.mysql {
        println!("\n--- MySQL benchmark start ---\n");
        run_mysql_bench(url, args.trials).await?;
        println!("--- MySQL benchmark end ---\n");
    }

    if let Some(url) = &args.postgres {
        println!("\n--- PostgreSQL benchmark start ---\n");
        run_pg_bench(url, args.trials).await?;
        println!("--- PostgreSQL benchmark end ---\n");
    }

    if args.mysql.is_none() && args.postgres.is_none() {
        eprintln!("No database URL provided. Use --mysql URL --postgres URL or set DATABASE_URL_MYSQL / DATABASE_URL_POSTGRES env vars.");
        std::process::exit(1);
    }

    Ok(())
}
