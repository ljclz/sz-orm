//! Oracle 23ai 真实数据库性能基准测试
//!
//! 使用 sz-orm-oracle 适配器（基于 `oracle` crate / ODPI-C）连接本机 Oracle 23ai，
//! 测量 INSERT / SELECT BY ID / SELECT ALL / UPDATE / DELETE 全场景性能。
//!
//! 环境要求：
//! - Oracle 23ai Free 运行于 127.0.0.1:1521（FREEPDB1 服务名）
//! - Oracle Client 库（oci.dll 等）位于 PATH 中
//! - 可通过环境变量 `SZ_ORM_ORACLE_*` 覆盖默认连接参数
//!
//! 运行方式（需 --ignored 显式启动）：
//! ```bash
//! cargo test -p sz-orm-oracle --test benchmark -- --ignored --nocapture
//! ```

#![cfg(test)]

use std::sync::Arc;
use std::time::Instant;
use sz_orm_core::{Pool, PoolConfigBuilder, Value};
use sz_orm_oracle::{OracleConnectionFactory, OraclePoolHandle};

/// 默认 Oracle 连接参数（本机 23ai Free）；可通过环境变量覆盖。
/// 使用专用测试用户 sz_orm_test（已授予 DBA 权限），避免 sys/SYSDBA 特权。
const ORACLE_USER_DEFAULT: &str = "sz_orm_test";
const ORACLE_PASSWORD_DEFAULT: &str = "SzOrmTest2026";
const ORACLE_CONNECT_STRING_DEFAULT: &str = "127.0.0.1:1521/freepdb1.FALSE";

fn oracle_user() -> String {
    std::env::var("SZ_ORM_ORACLE_USER").unwrap_or_else(|_| ORACLE_USER_DEFAULT.to_string())
}

fn oracle_password() -> String {
    std::env::var("SZ_ORM_ORACLE_PASSWORD").unwrap_or_else(|_| ORACLE_PASSWORD_DEFAULT.to_string())
}

fn oracle_connect_string() -> String {
    std::env::var("SZ_ORM_ORACLE_CONNECT_STRING")
        .unwrap_or_else(|_| ORACLE_CONNECT_STRING_DEFAULT.to_string())
}

/// 测试数据行数
const ROW_COUNT: usize = 1000;

/// 创建连接池
fn create_pool() -> Pool {
    let handle = Arc::new(
        OraclePoolHandle::connect(&oracle_user(), &oracle_password(), &oracle_connect_string())
            .expect("Oracle connect failed - is Oracle 23ai running on 127.0.0.1:1521?"),
    );
    let factory = Arc::new(OracleConnectionFactory::new(handle));
    let config = PoolConfigBuilder::new()
        .max_size(10)
        .min_idle(2)
        .acquire_timeout(10)
        .build()
        .expect("PoolConfig invalid");
    Pool::new(config, factory).expect("Pool::new failed")
}

/// 初始化测试表并插入数据
async fn setup(pool: &Pool) -> String {
    let table = format!("bench_sz_orm_{}", std::process::id() % 1000);
    let mut conn = pool.acquire().await.expect("acquire");

    // 删除已存在的表（忽略错误）
    let _ = conn.execute(&format!("DROP TABLE {}", table)).await;

    // 创建表（Oracle 23ai 支持 IDENTITY 列）
    conn.execute(&format!(
        "CREATE TABLE {} (id NUMBER GENERATED ALWAYS AS IDENTITY PRIMARY KEY, name VARCHAR2(255) NOT NULL, email VARCHAR2(255) NOT NULL, age NUMBER(10) NOT NULL)",
        table
    ))
    .await
    .expect("CREATE TABLE");

    // 批量插入数据（使用参数绑定）
    let sql = format!(
        "INSERT INTO {} (name, email, age) VALUES (:1, :2, :3)",
        table
    );
    for i in 0..ROW_COUNT {
        let name = format!("user_{}", i);
        let email = format!("user_{}@test.com", i);
        let age = (i % 100) as i64;
        conn.execute_with_params(
            &sql,
            &[Value::String(name), Value::String(email), Value::I64(age)],
        )
        .await
        .expect("INSERT");
    }

    table
}

/// 清理测试表
async fn teardown(pool: &Pool, table: &str) {
    let mut conn = pool.acquire().await.expect("acquire");
    let _ = conn.execute(&format!("DROP TABLE {}", table)).await;
    pool.close_all().await;
}

/// 完整 CRUD benchmark
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Oracle benchmark 需显式 --ignored 启动；需本机 Oracle 23ai 运行"]
async fn oracle_crud_benchmark() {
    println!("=== Oracle 23ai CRUD Benchmark (sz-orm-oracle) ===");
    println!("数据量: {} 行", ROW_COUNT);
    println!("连接: {}", oracle_connect_string());
    println!();

    let pool = create_pool();
    let table = setup(&pool).await;
    println!("setup 完成: 表 {} 已插入 {} 行", table, ROW_COUNT);

    // SELECT BY ID（1000 次查询）
    let select_count = 100;
    let start = Instant::now();
    for i in 1..=select_count {
        let mut conn = pool.acquire().await.expect("acquire");
        let sql = format!("SELECT name, email, age FROM {} WHERE id = :1", table);
        let rows = conn
            .query_with_params(&sql, &[Value::I64(i as i64)])
            .await
            .expect("query");
        assert!(!rows.is_empty(), "SELECT BY ID should return 1 row");
    }
    let elapsed = start.elapsed();
    let per_op = elapsed / select_count as u32;
    println!(
        "[Oracle 23ai] SELECT BY ID: {} 次查询总耗时 {:?}, 平均 {:?}/op",
        select_count, elapsed, per_op
    );

    // SELECT ALL（全表查询 - HashMap 路径）
    let start = Instant::now();
    {
        let mut conn = pool.acquire().await.expect("acquire");
        let sql = format!("SELECT id, name, email, age FROM {}", table);
        let rows = conn.query(&sql).await.expect("query");
        assert_eq!(rows.len(), ROW_COUNT, "SELECT ALL should return all rows");
    }
    let elapsed = start.elapsed();
    println!(
        "[Oracle 23ai] SELECT ALL HashMap ({}行): 总耗时 {:?}",
        ROW_COUNT, elapsed
    );

    // SELECT ALL（全表查询 - 位置式 query_values 路径，绕过 HashMap）
    let start = Instant::now();
    {
        let mut conn = pool.acquire().await.expect("acquire");
        let sql = format!("SELECT id, name, email, age FROM {}", table);
        let (columns, values_matrix) = conn.query_values(&sql).await.expect("query_values");
        assert_eq!(values_matrix.len(), ROW_COUNT, "SELECT ALL should return all rows");
        assert_eq!(columns.len(), 4, "should have 4 columns");
    }
    let elapsed = start.elapsed();
    println!(
        "[Oracle 23ai] SELECT ALL Positional ({}行): 总耗时 {:?}",
        ROW_COUNT, elapsed
    );

    // SELECT BY ID（位置式 query_values_with_params 路径）
    let start = Instant::now();
    for i in 1..=select_count {
        let mut conn = pool.acquire().await.expect("acquire");
        let sql = format!("SELECT name, email, age FROM {} WHERE id = :1", table);
        let (_cols, values_matrix) = conn
            .query_values_with_params(&sql, &[Value::I64(i as i64)])
            .await
            .expect("query_values_with_params");
        assert!(!values_matrix.is_empty(), "SELECT BY ID should return 1 row");
    }
    let elapsed = start.elapsed();
    let per_op = elapsed / select_count as u32;
    println!(
        "[Oracle 23ai] SELECT BY ID Positional: {} 次查询总耗时 {:?}, 平均 {:?}/op",
        select_count, elapsed, per_op
    );

    // UPDATE BY ID（100 次更新）
    let update_count = 100;
    let start = Instant::now();
    for i in 1..=update_count {
        let mut conn = pool.acquire().await.expect("acquire");
        let sql = format!("UPDATE {} SET name = :1 WHERE id = :2", table);
        let new_name = format!("updated_{}", i);
        conn.execute_with_params(&sql, &[Value::String(new_name), Value::I64(i as i64)])
            .await
            .expect("update");
    }
    let elapsed = start.elapsed();
    let per_op = elapsed / update_count as u32;
    println!(
        "[Oracle 23ai] UPDATE BY ID: {} 次更新总耗时 {:?}, 平均 {:?}/op",
        update_count, elapsed, per_op
    );

    // DELETE（删除前 100 行）
    let delete_count = 100;
    let start = Instant::now();
    for i in 1..=delete_count {
        let mut conn = pool.acquire().await.expect("acquire");
        let sql = format!("DELETE FROM {} WHERE id = :1", table);
        conn.execute_with_params(&sql, &[Value::I64(i as i64)])
            .await
            .expect("delete");
    }
    let elapsed = start.elapsed();
    let per_op = elapsed / delete_count as u32;
    println!(
        "[Oracle 23ai] DELETE BY ID: {} 次删除总耗时 {:?}, 平均 {:?}/op",
        delete_count, elapsed, per_op
    );

    // INSERT（100 行插入，测量单行插入性能）
    let insert_count = 100;
    let start = Instant::now();
    for i in 0..insert_count {
        let mut conn = pool.acquire().await.expect("acquire");
        let sql = format!("INSERT INTO {} (name, email, age) VALUES (:1, :2, :3)", table);
        let name = format!("new_user_{}", i);
        let email = format!("new_user_{}@test.com", i);
        let age = (i % 100) as i64;
        conn.execute_with_params(
            &sql,
            &[Value::String(name), Value::String(email), Value::I64(age)],
        )
        .await
        .expect("insert");
    }
    let elapsed = start.elapsed();
    let per_op = elapsed / insert_count as u32;
    println!(
        "[Oracle 23ai] INSERT: {} 次插入总耗时 {:?}, 平均 {:?}/op",
        insert_count, elapsed, per_op
    );

    println!();
    println!("=== Oracle 23ai CRUD Benchmark 完成 ===");

    teardown(&pool, &table).await;
}
