//! SQLite in-memory 性能基准测试
//!
//! 使用 sz-orm-sqlx 适配器（基于 sqlx 0.9）连接 SQLite in-memory（cache=shared），
//! 测量 INSERT / SELECT BY ID / SELECT ALL / UPDATE / DELETE 全场景性能。
//!
//! 环境要求：无（SQLite in-memory，零部署）
//!
//! 运行方式：
//! ```bash
//! cargo test -p sz-orm-sqlx --test benchmark -- --nocapture
//! ```

#![cfg(test)]

use std::sync::Arc;
use std::time::Instant;
use sz_orm_core::{Pool, PoolConfigBuilder, Value};
use sz_orm_sqlx::SqlitePoolHandle;

/// SQLite 连接字符串：in-memory + cache=shared，多连接共享同一数据库
const SQLITE_URL: &str = "sqlite::memory:?cache=shared";

/// 测试数据行数
const ROW_COUNT: usize = 1000;

/// 创建连接池
async fn create_pool() -> Pool {
    let handle = Arc::new(
        SqlitePoolHandle::connect(SQLITE_URL)
            .await
            .expect("SQLite connect failed"),
    );
    let factory = Arc::new(sz_orm_sqlx::SqlxSqliteConnectionFactory::new(handle));
    let config = PoolConfigBuilder::new()
        .max_size(10)
        .min_idle(2)
        .acquire_timeout(10)
        .build()
        .expect("PoolConfig invalid");
    Pool::new(config, factory).expect("Pool::new failed")
}

/// 初始化测试表并插入数据
async fn setup(pool: &Pool) {
    let mut conn = pool.acquire().await.expect("acquire");

    // 删除已存在的表
    let _ = conn.execute("DROP TABLE IF EXISTS bench_sz_orm").await;

    // 创建表
    conn.execute(
        "CREATE TABLE bench_sz_orm (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, email TEXT NOT NULL, age INTEGER NOT NULL)",
    )
    .await
    .expect("CREATE TABLE");

    // 批量插入数据（使用参数绑定）
    let sql = "INSERT INTO bench_sz_orm (name, email, age) VALUES (?, ?, ?)";
    for i in 0..ROW_COUNT {
        let name = format!("user_{}", i);
        let email = format!("user_{}@test.com", i);
        let age = (i % 100) as i64;
        conn.execute_with_params(
            sql,
            &[Value::String(name), Value::String(email), Value::I64(age)],
        )
        .await
        .expect("INSERT");
    }
}

/// 清理测试表
async fn teardown(pool: &Pool) {
    let mut conn = pool.acquire().await.expect("acquire");
    let _ = conn.execute("DROP TABLE IF EXISTS bench_sz_orm").await;
    pool.close_all().await;
}

/// 完整 CRUD benchmark
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sqlite_crud_benchmark() {
    println!("=== SQLite in-memory CRUD Benchmark (sz-orm-sqlx) ===");
    println!("数据量: {} 行", ROW_COUNT);
    println!("连接: {}", SQLITE_URL);
    println!();

    let pool = create_pool().await;
    setup(&pool).await;
    println!("setup 完成: 表 bench_sz_orm 已插入 {} 行", ROW_COUNT);

    // SELECT BY ID（100 次查询）
    let select_count = 100;
    let start = Instant::now();
    for i in 1..=select_count {
        let mut conn = pool.acquire().await.expect("acquire");
        let sql = "SELECT name, email, age FROM bench_sz_orm WHERE id = ?";
        let rows = conn
            .query_with_params(sql, &[Value::I64(i as i64)])
            .await
            .expect("query");
        assert!(!rows.is_empty(), "SELECT BY ID should return 1 row");
    }
    let elapsed = start.elapsed();
    let per_op = elapsed / select_count as u32;
    println!(
        "[SQLite] SELECT BY ID: {} 次查询总耗时 {:?}, 平均 {:?}/op",
        select_count, elapsed, per_op
    );

    // SELECT ALL（全表查询 - HashMap 路径）
    let start = Instant::now();
    {
        let mut conn = pool.acquire().await.expect("acquire");
        let sql = "SELECT id, name, email, age FROM bench_sz_orm";
        let rows = conn.query(sql).await.expect("query");
        assert_eq!(rows.len(), ROW_COUNT, "SELECT ALL should return all rows");
    }
    let elapsed = start.elapsed();
    println!(
        "[SQLite] SELECT ALL HashMap ({}行): 总耗时 {:?}",
        ROW_COUNT, elapsed
    );

    // SELECT ALL（全表查询 - 位置式 query_values 路径，绕过 HashMap）
    let start = Instant::now();
    {
        let mut conn = pool.acquire().await.expect("acquire");
        let sql = "SELECT id, name, email, age FROM bench_sz_orm";
        let (columns, values_matrix) = conn.query_values(sql).await.expect("query_values");
        assert_eq!(values_matrix.len(), ROW_COUNT, "SELECT ALL should return all rows");
        assert_eq!(columns.len(), 4, "should have 4 columns");
    }
    let elapsed = start.elapsed();
    println!(
        "[SQLite] SELECT ALL Positional ({}行): 总耗时 {:?}",
        ROW_COUNT, elapsed
    );

    // SELECT BY ID（位置式 query_values_with_params 路径）
    let start = Instant::now();
    for i in 1..=select_count {
        let mut conn = pool.acquire().await.expect("acquire");
        let sql = "SELECT name, email, age FROM bench_sz_orm WHERE id = ?";
        let (_cols, values_matrix) = conn
            .query_values_with_params(sql, &[Value::I64(i as i64)])
            .await
            .expect("query_values_with_params");
        assert!(!values_matrix.is_empty(), "SELECT BY ID should return 1 row");
    }
    let elapsed = start.elapsed();
    let per_op = elapsed / select_count as u32;
    println!(
        "[SQLite] SELECT BY ID Positional: {} 次查询总耗时 {:?}, 平均 {:?}/op",
        select_count, elapsed, per_op
    );

    // UPDATE BY ID（100 次更新）
    let update_count = 100;
    let start = Instant::now();
    for i in 1..=update_count {
        let mut conn = pool.acquire().await.expect("acquire");
        let sql = "UPDATE bench_sz_orm SET name = ? WHERE id = ?";
        let new_name = format!("updated_{}", i);
        conn.execute_with_params(sql, &[Value::String(new_name), Value::I64(i as i64)])
            .await
            .expect("update");
    }
    let elapsed = start.elapsed();
    let per_op = elapsed / update_count as u32;
    println!(
        "[SQLite] UPDATE BY ID: {} 次更新总耗时 {:?}, 平均 {:?}/op",
        update_count, elapsed, per_op
    );

    // DELETE（删除前 100 行）
    let delete_count = 100;
    let start = Instant::now();
    for i in 1..=delete_count {
        let mut conn = pool.acquire().await.expect("acquire");
        let sql = "DELETE FROM bench_sz_orm WHERE id = ?";
        conn.execute_with_params(sql, &[Value::I64(i as i64)])
            .await
            .expect("delete");
    }
    let elapsed = start.elapsed();
    let per_op = elapsed / delete_count as u32;
    println!(
        "[SQLite] DELETE BY ID: {} 次删除总耗时 {:?}, 平均 {:?}/op",
        delete_count, elapsed, per_op
    );

    // INSERT（100 行插入，测量单行插入性能）
    let insert_count = 100;
    let start = Instant::now();
    for i in 0..insert_count {
        let mut conn = pool.acquire().await.expect("acquire");
        let sql = "INSERT INTO bench_sz_orm (name, email, age) VALUES (?, ?, ?)";
        let name = format!("new_user_{}", i);
        let email = format!("new_user_{}@test.com", i);
        let age = (i % 100) as i64;
        conn.execute_with_params(
            sql,
            &[Value::String(name), Value::String(email), Value::I64(age)],
        )
        .await
        .expect("insert");
    }
    let elapsed = start.elapsed();
    let per_op = elapsed / insert_count as u32;
    println!(
        "[SQLite] INSERT: {} 次插入总耗时 {:?}, 平均 {:?}/op",
        insert_count, elapsed, per_op
    );

    println!();
    println!("=== SQLite in-memory CRUD Benchmark 完成 ===");

    teardown(&pool).await;
}
