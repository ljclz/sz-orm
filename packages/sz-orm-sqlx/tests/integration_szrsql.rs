//! szrsql 集成测试（v10.13.2 新增）
//!
//! szrsql（`D:\www\test\鲜视达\rust\szrsql`，v1.0.0-rc.2）是自研 Rust 数据库，
//! 对外暴露 pgwire v3.0 线协议（默认端口 6543），语义层为 PG 语义（README「协议」节、
//! AGENTS.md「架构速览」：六前端全部经 `ExecutorService` 执行同一套 PG 语义 SQL）。
//! sz-orm 侧以 `DbType::Szrsql`（PG 方言族委托）+ `szrsql://` scheme（复用
//! sqlx Postgres 驱动）接入。
//!
//! 需要本机启动 szrsql 服务（未启动自动跳过）：
//! ```text
//! cargo run -p szrsql-bin -- --port 6543 --auth-mode trust --data-dir <dir>
//! ```
//! 运行：cargo test -p sz-orm-sqlx --test integration_szrsql -- --ignored
//!
//! 凭据经环境变量 `SZ_ORM_SZRSQL_URL` 注入，不入库（审计合规铁律）。

use sz_orm_core::{Connection, DbType, Pool, PoolConfigBuilder, Value};
use sz_orm_sqlx::{AnyPool, PgPoolHandle, SqlxPgConnectionFactory};

/// 默认 szrsql 连接 URL（本机 trust 模式，密码占位）；`SZ_ORM_SZRSQL_URL` 覆盖。
const SZRSQL_URL_DEFAULT: &str = "szrsql://postgres:trust@127.0.0.1:6543/postgres";

fn szrsql_url() -> String {
    std::env::var("SZ_ORM_SZRSQL_URL").unwrap_or_else(|_| SZRSQL_URL_DEFAULT.to_string())
}

/// szrsql 服务未启动时跳过（connect 失败即视为未启动）
async fn try_pool() -> Option<AnyPool> {
    AnyPool::connect(&szrsql_url()).await.ok()
}

fn unique_table(prefix: &str) -> String {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{}_{}_{}", prefix, pid, nanos % 1_000_000)
}

#[tokio::test]
#[ignore = "需要 szrsql 服务在 127.0.0.1:6543"]
async fn szrsql_connect_reports_szrsql_backend() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: szrsql 未启动（127.0.0.1:6543）");
        return;
    };
    // szrsql:// 走 Postgres wire 后端（sqlx Postgres 驱动复用）
    assert_eq!(pool.backend(), sz_orm_sqlx::AnyBackend::Postgres);
    // PG 方言族：is_postgres_family 为真（方言层 Szrsql 委托 PostgreSqlDialect）
    assert!(pool.dialect().db_type().is_postgres_family());
    assert_eq!(
        sz_orm_core::get_dialect(DbType::Szrsql).unwrap().db_type(),
        DbType::Szrsql
    );
}

#[tokio::test]
#[ignore = "需要 szrsql 服务在 127.0.0.1:6543"]
async fn szrsql_crud_roundtrip() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: szrsql 未启动");
        return;
    };
    let mut conn = pool.create().await.unwrap();
    let table = unique_table("szrsql_crud");
    conn.execute(&format!("DROP TABLE IF EXISTS {table}"))
        .await
        .unwrap();
    conn.execute(&format!(
        "CREATE TABLE {table} (id BIGSERIAL PRIMARY KEY, name TEXT NOT NULL, bal BIGINT)"
    ))
    .await
    .unwrap();

    // INSERT + RETURNING（szrsql 支持 RETURNING，阶段一实测）
    let rows = conn
        .query(&format!(
            "INSERT INTO {table} (name, bal) VALUES ('alice', 100) RETURNING id"
        ))
        .await
        .unwrap();
    assert_eq!(rows.len(), 1, "RETURNING 应返回 1 行");

    // SELECT
    let rows = conn
        .query(&format!(
            "SELECT id, name, bal FROM {table} WHERE name = 'alice'"
        ))
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);

    // UPDATE
    let n = conn
        .execute(&format!(
            "UPDATE {table} SET bal = 200 WHERE name = 'alice'"
        ))
        .await
        .unwrap();
    assert_eq!(n, 1);

    // DELETE
    let n = conn
        .execute(&format!("DELETE FROM {table} WHERE name = 'alice'"))
        .await
        .unwrap();
    assert_eq!(n, 1);

    conn.execute(&format!("DROP TABLE IF EXISTS {table}"))
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "需要 szrsql 服务在 127.0.0.1:6543"]
async fn szrsql_parameterized_queries() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: szrsql 未启动");
        return;
    };
    let mut conn = pool.create().await.unwrap();
    let table = unique_table("szrsql_param");
    conn.execute(&format!("DROP TABLE IF EXISTS {table}"))
        .await
        .unwrap();
    conn.execute(&format!(
        "CREATE TABLE {table} (id BIGSERIAL PRIMARY KEY, name TEXT, bal BIGINT, ok BOOLEAN)"
    ))
    .await
    .unwrap();

    // 参数化 INSERT（$n 占位符 + prepared statement，门禁 9 参数化铁律）
    let n = conn
        .execute_with_params(
            &format!("INSERT INTO {table} (name, bal, ok) VALUES ($1, $2, $3)"),
            &[
                Value::String("alice".into()),
                Value::I64(100),
                Value::Bool(true),
            ],
        )
        .await
        .unwrap();
    assert_eq!(n, 1);

    // 参数化 SELECT
    let rows = conn
        .query_with_params(
            &format!("SELECT id, name, bal, ok FROM {table} WHERE name = $1"),
            &[Value::String("alice".into())],
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);

    // 参数化 UPDATE / DELETE
    let n = conn
        .execute_with_params(
            &format!("UPDATE {table} SET bal = $1 WHERE name = $2"),
            &[Value::I64(999), Value::String("alice".into())],
        )
        .await
        .unwrap();
    assert_eq!(n, 1);
    let n = conn
        .execute_with_params(
            &format!("DELETE FROM {table} WHERE name = $1"),
            &[Value::String("alice".into())],
        )
        .await
        .unwrap();
    assert_eq!(n, 1);

    conn.execute(&format!("DROP TABLE IF EXISTS {table}"))
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "需要 szrsql 服务在 127.0.0.1:6543"]
async fn szrsql_transaction_rollback() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: szrsql 未启动");
        return;
    };
    let mut conn = pool.create().await.unwrap();
    let table = unique_table("szrsql_tx");
    conn.execute(&format!("DROP TABLE IF EXISTS {table}"))
        .await
        .unwrap();
    conn.execute(&format!("CREATE TABLE {table} (id INT PRIMARY KEY)"))
        .await
        .unwrap();

    // 提交路径
    conn.begin_transaction().await.unwrap();
    conn.execute(&format!("INSERT INTO {table} (id) VALUES (1)"))
        .await
        .unwrap();
    conn.commit().await.unwrap();
    let rows = conn
        .query(&format!("SELECT id FROM {table}"))
        .await
        .unwrap();
    assert_eq!(rows.len(), 1, "COMMIT 后应可见 1 行");

    // 回滚路径
    conn.begin_transaction().await.unwrap();
    conn.execute(&format!("INSERT INTO {table} (id) VALUES (2)"))
        .await
        .unwrap();
    conn.rollback().await.unwrap();
    let rows = conn
        .query(&format!("SELECT id FROM {table}"))
        .await
        .unwrap();
    assert_eq!(rows.len(), 1, "ROLLBACK 后仍应只有 1 行");

    conn.execute(&format!("DROP TABLE IF EXISTS {table}"))
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "需要 szrsql 服务在 127.0.0.1:6543"]
async fn szrsql_type_roundtrip() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: szrsql 未启动");
        return;
    };
    let mut conn = pool.create().await.unwrap();
    let table = unique_table("szrsql_types");
    conn.execute(&format!("DROP TABLE IF EXISTS {table}"))
        .await
        .unwrap();
    // BIGINT/TEXT/BOOLEAN/TIMESTAMP 类型映射（阶段一实测：原生支持）
    conn.execute(&format!(
        "CREATE TABLE {table} (id BIGINT PRIMARY KEY, name TEXT, ok BOOLEAN, ts TIMESTAMP)"
    ))
    .await
    .unwrap();
    let n = conn
        .execute_with_params(
            &format!("INSERT INTO {table} (id, name, ok, ts) VALUES ($1, $2, $3, $4)"),
            &[
                Value::I64(1),
                Value::String("t".into()),
                Value::Bool(true),
                Value::String("2026-11-08 12:00:00".into()),
            ],
        )
        .await
        .unwrap();
    assert_eq!(n, 1);
    let rows = conn
        .query(&format!("SELECT id, name, ok FROM {table} WHERE id = 1"))
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);

    // Option/NULL 往返
    let n = conn
        .execute_with_params(
            &format!("INSERT INTO {table} (id, name, ok, ts) VALUES ($1, $2, $3, $4)"),
            &[Value::I64(2), Value::Null, Value::Bool(false), Value::Null],
        )
        .await
        .unwrap();
    assert_eq!(n, 1);
    let rows = conn
        .query(&format!("SELECT name FROM {table} WHERE id = 2"))
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);

    conn.execute(&format!("DROP TABLE IF EXISTS {table}"))
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "需要 szrsql 服务在 127.0.0.1:6543"]
async fn szrsql_pagination_limit_offset() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: szrsql 未启动");
        return;
    };
    let mut conn = pool.create().await.unwrap();
    let table = unique_table("szrsql_page");
    conn.execute(&format!("DROP TABLE IF EXISTS {table}"))
        .await
        .unwrap();
    conn.execute(&format!(
        "CREATE TABLE {table} (id BIGINT PRIMARY KEY, name TEXT)"
    ))
    .await
    .unwrap();
    for i in 1..=5i64 {
        conn.execute_with_params(
            &format!("INSERT INTO {table} (id, name) VALUES ($1, $2)"),
            &[Value::I64(i), Value::String(format!("n{i}"))],
        )
        .await
        .unwrap();
    }
    // LIMIT/OFFSET 分页（PG 语法）
    let rows = conn
        .query(&format!(
            "SELECT id FROM {table} ORDER BY id LIMIT 2 OFFSET 1"
        ))
        .await
        .unwrap();
    assert_eq!(rows.len(), 2, "LIMIT 2 OFFSET 1 应返回 2 行");

    conn.execute(&format!("DROP TABLE IF EXISTS {table}"))
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "需要 szrsql 服务在 127.0.0.1:6543"]
async fn szrsql_information_schema_probe() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: szrsql 未启动");
        return;
    };
    let mut conn = pool.create().await.unwrap();
    let table = unique_table("szrsql_meta");
    conn.execute(&format!("DROP TABLE IF EXISTS {table}"))
        .await
        .unwrap();
    conn.execute(&format!(
        "CREATE TABLE {table} (id BIGINT PRIMARY KEY, name TEXT)"
    ))
    .await
    .unwrap();
    // information_schema.columns（schema 探测依赖，阶段一实测 5 列返回）
    let rows = conn
        .query(&format!(
            "SELECT column_name, data_type FROM information_schema.columns WHERE table_name = '{table}'"
        ))
        .await
        .unwrap();
    assert_eq!(
        rows.len(),
        2,
        "information_schema.columns 应返回 2 列元数据"
    );

    conn.execute(&format!("DROP TABLE IF EXISTS {table}"))
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "需要 szrsql 服务在 127.0.0.1:6543"]
async fn szrsql_pool_acquire_release_roundtrip() {
    let Some(_probe) = try_pool().await else {
        eprintln!("skip: szrsql 未启动");
        return;
    };
    // 自研连接池 + PgPoolHandle（szrsql:// 改写 postgres:// 后直连）
    let url = szrsql_url();
    let pg_url = url.replacen("szrsql://", "postgres://", 1);
    let handle = Arc::new(PgPoolHandle::connect(&pg_url).await.unwrap());
    let factory = Arc::new(SqlxPgConnectionFactory::new(handle));
    let config = PoolConfigBuilder::new()
        .max_size(4)
        .min_idle(0)
        .acquire_timeout(10)
        .build()
        .unwrap();
    let pool = Pool::new(config, factory).unwrap();
    for i in 0..10u32 {
        let mut conn = pool.acquire().await.unwrap();
        let sql = format!("SELECT {i} as n");
        let rows = conn.query(&sql).await.unwrap();
        assert_eq!(rows.len(), 1);
        pool.release(conn).await;
    }
    let status = pool.status().await;
    assert!(
        status.active <= 4 && status.idle <= 4,
        "池 active/idle 均不应超过 max_size，实际 active={} idle={}",
        status.active,
        status.idle
    );
}

use std::sync::Arc;
