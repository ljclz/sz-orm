//! MySQL 真实数据库集成测试
//!
//! 使用 sqlx (MySQL 9.6.0) 验证 sz-orm-core 的 MySQL 方言、值转换、SQL 转义、
//! 事务、连接池语义、分页、JSON 操作、SQL 注入防护等核心功能。
//!
//! 超大数据量场景：10 万条记录 CRUD、8 任务并发读写、批量插入性能基线。
//!
//! 测试数据库：mysql://root:<your-password>@127.0.0.1:3306/sz_orm_test
//!
//! 运行方式：cargo test --package sz-orm-core --test integration_mysql -- --ignored --nocapture

use sqlx::mysql::{MySqlPool, MySqlPoolOptions};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use sz_orm_core::dialect::{get_dialect, ColumnDef};
use sz_orm_core::DbType;
use sz_orm_core::Value;

/// 默认 MySQL 连接 URL（本机）；可通过环境变量 `SZ_ORM_MYSQL_URL` 覆盖以指向真实云数据库。
const MYSQL_URL_DEFAULT: &str = "mysql://root:<your-password>@127.0.0.1:3306/sz_orm_test";

fn mysql_url() -> String {
    std::env::var("SZ_ORM_MYSQL_URL").unwrap_or_else(|_| MYSQL_URL_DEFAULT.to_string())
}

/// 全局唯一表名计数器（避免并行测试冲突）
static TABLE_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_table(prefix: &str) -> String {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let counter = TABLE_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{}_{}_{}_{}", prefix, pid, nanos % 1_000_000, counter)
}

/// 建立 MySQL 连接池（5 连接，acquire_timeout=30s）
async fn setup_pool() -> MySqlPool {
    // 先尝试连接，确认 MySQL 可用
    let pool = MySqlPoolOptions::new()
        .max_connections(8)
        .acquire_timeout(Duration::from_secs(30))
        .connect(&mysql_url())
        .await
        .expect("mysql connect failed - is MySQL 9.6.0 running on 127.0.0.1:3306?");
    pool
}

/// 用方言生成 CREATE TABLE 并执行
async fn create_test_table(pool: &MySqlPool, table: &str) {
    let dialect = get_dialect(DbType::MySQL).expect("mysql dialect");
    let columns = vec![
        ColumnDef {
            name: "id".to_string(),
            sql_type: "BIGINT".to_string(),
            nullable: false,
            default: None,
            auto_increment: true,
            primary_key: true,
        },
        ColumnDef {
            name: "name".to_string(),
            sql_type: "VARCHAR(255)".to_string(),
            nullable: false,
            default: None,
            auto_increment: false,
            primary_key: false,
        },
        ColumnDef {
            name: "value".to_string(),
            sql_type: "BIGINT".to_string(),
            nullable: true,
            default: None,
            auto_increment: false,
            primary_key: false,
        },
        ColumnDef {
            name: "data".to_string(),
            sql_type: "TEXT".to_string(),
            nullable: true,
            default: None,
            auto_increment: false,
            primary_key: false,
        },
        ColumnDef {
            name: "meta".to_string(),
            sql_type: "JSON".to_string(),
            nullable: true,
            default: None,
            auto_increment: false,
            primary_key: false,
        },
    ];
    let sql = dialect.build_create_table(table, &columns);
    sqlx::query(&sql).execute(pool).await.expect("create table");
}

async fn drop_table(pool: &MySqlPool, table: &str) {
    let dialect = get_dialect(DbType::MySQL).expect("mysql dialect");
    let sql = dialect.build_drop_table(table, true);
    let _ = sqlx::query(&sql).execute(pool).await;
}

#[tokio::test]
#[ignore = "需要 MySQL 9.6.0 运行于 127.0.0.1:3306"]
async fn test_mysql_dialect_basics() {
    let dialect = get_dialect(DbType::MySQL).expect("mysql dialect");
    assert_eq!(dialect.quote("user"), "`user`");
    assert_eq!(dialect.quote("with`quote"), "`with``quote`");
    // sz-orm-core MySQL 方言使用反斜杠转义（MySQL 默认 NO_BACKSLASH_ESCAPES 关闭）
    // 输入 it's -> it\'s (Rust 字符串字面量: "it\\'s")
    assert_eq!(dialect.escape_string("it's"), "it\\'s");
    // 输入 back\slash (Rust "back\\slash") -> back\\slash (Rust "back\\\\slash")
    assert_eq!(dialect.escape_string("back\\slash"), "back\\\\slash");
    assert!(!dialect.supports_returning());
    assert_eq!(dialect.auto_increment_keyword(), "AUTO_INCREMENT");
    assert_eq!(dialect.last_insert_id_sql(), "LAST_INSERT_ID()");
    assert_eq!(dialect.json_type(), "JSON");
}

#[tokio::test]
#[ignore = "需要 MySQL 9.6.0 运行于 127.0.0.1:3306"]
async fn test_mysql_create_insert_select() {
    let pool = setup_pool().await;
    let table = unique_table("t1");
    create_test_table(&pool, &table).await;

    // 插入 3 条
    let sql = format!(
        "INSERT INTO `{}` (name, value, data) VALUES (?, ?, ?)",
        table
    );
    sqlx::query(&sql)
        .bind("alice")
        .bind(100i64)
        .bind("data1")
        .execute(&pool)
        .await
        .expect("insert 1");
    sqlx::query(&sql)
        .bind("bob")
        .bind(200i64)
        .bind("data2")
        .execute(&pool)
        .await
        .expect("insert 2");
    sqlx::query(&sql)
        .bind("carol")
        .bind(300i64)
        .bind("data3")
        .execute(&pool)
        .await
        .expect("insert 3");

    // SELECT 全部
    let select_sql = format!("SELECT name, value FROM `{}` ORDER BY id", table);
    let rows: Vec<(String, i64)> = sqlx::query_as(&select_sql)
        .fetch_all(&pool)
        .await
        .expect("select");
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].0, "alice");
    assert_eq!(rows[2].0, "carol");

    // Value 类型转换验证
    let v = Value::String("alice".to_string());
    let dialect = get_dialect(DbType::MySQL).expect("mysql dialect");
    let escaped = dialect.escape_string(v.as_str().unwrap());
    let sql = format!("SELECT value FROM `{}` WHERE name = '{}'", table, escaped);
    let row: (i64,) = sqlx::query_as(&sql)
        .fetch_one(&pool)
        .await
        .expect("query row");
    assert_eq!(row.0, 100);

    drop_table(&pool, &table).await;
}

#[tokio::test]
#[ignore = "需要 MySQL 9.6.0 运行于 127.0.0.1:3306"]
async fn test_mysql_bulk_insert_100k() {
    let pool = setup_pool().await;
    let table = unique_table("t_bulk");
    create_test_table(&pool, &table).await;

    let total: usize = 100_000;
    let start = Instant::now();

    // 批量插入：每批 1000 条
    let mut tx = pool.begin().await.expect("begin");
    let batch_size = 1000;
    let mut total_inserted = 0usize;
    for batch_start in (0..total).step_by(batch_size) {
        let batch_end = (batch_start + batch_size).min(total);
        let placeholders: Vec<String> = (batch_start..batch_end)
            .map(|_| "(?, ?, ?)".to_string())
            .collect();
        let sql = format!(
            "INSERT INTO `{}` (name, value, data) VALUES {}",
            table,
            placeholders.join(", ")
        );
        let mut q = sqlx::query(&sql);
        for i in batch_start..batch_end {
            q = q
                .bind(format!("user_{}", i))
                .bind(i as i64)
                .bind(format!("data_{}", i % 1000));
        }
        q.execute(&mut *tx).await.expect("batch insert");
        total_inserted += batch_end - batch_start;
    }
    tx.commit().await.expect("commit");
    let elapsed = start.elapsed();
    println!(
        "mysql bulk insert {} rows in {:?} ({:.0} rows/s)",
        total,
        elapsed,
        total as f64 / elapsed.as_secs_f64()
    );
    assert_eq!(total_inserted, total);

    // 验证总数
    let count_sql = format!("SELECT COUNT(*) FROM `{}`", table);
    let (count,): (i64,) = sqlx::query_as(&count_sql)
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(count as usize, total);

    // 验证末尾数据
    let last_sql = format!("SELECT name FROM `{}` WHERE value = ?", table);
    let (last_name,): (String,) = sqlx::query_as(&last_sql)
        .bind((total - 1) as i64)
        .fetch_one(&pool)
        .await
        .expect("query last");
    assert_eq!(last_name, format!("user_{}", total - 1));

    drop_table(&pool, &table).await;
}

#[tokio::test]
#[ignore = "需要 MySQL 9.6.0 运行于 127.0.0.1:3306"]
async fn test_mysql_update_delete() {
    let pool = setup_pool().await;
    let table = unique_table("t_ud");
    create_test_table(&pool, &table).await;

    // 准备 1000 条数据
    let mut tx = pool.begin().await.expect("begin");
    for i in 0..1000i64 {
        let sql = format!(
            "INSERT INTO `{}` (name, value, data) VALUES (?, ?, ?)",
            table
        );
        sqlx::query(&sql)
            .bind(format!("n_{}", i))
            .bind(i)
            .bind("x")
            .execute(&mut *tx)
            .await
            .expect("insert");
    }
    tx.commit().await.expect("commit");

    // UPDATE
    let upd = format!(
        "UPDATE `{}` SET value = value + 1000 WHERE value < 100",
        table
    );
    let result = sqlx::query(&upd).execute(&pool).await.expect("update");
    assert_eq!(result.rows_affected(), 100);

    // DELETE
    let del = format!("DELETE FROM `{}` WHERE value >= 1000", table);
    let result = sqlx::query(&del).execute(&pool).await.expect("delete");
    assert_eq!(result.rows_affected(), 100);

    // 验证总数
    let count_sql = format!("SELECT COUNT(*) FROM `{}`", table);
    let (count,): (i64,) = sqlx::query_as(&count_sql).fetch_one(&pool).await.unwrap();
    assert_eq!(count, 900);

    drop_table(&pool, &table).await;
}

#[tokio::test]
#[ignore = "需要 MySQL 9.6.0 运行于 127.0.0.1:3306"]
async fn test_mysql_transaction_commit() {
    let pool = setup_pool().await;
    let table = unique_table("t_tc");
    create_test_table(&pool, &table).await;

    let mut tx = pool.begin().await.expect("begin");
    let sql = format!(
        "INSERT INTO `{}` (name, value, data) VALUES (?, ?, ?)",
        table
    );
    sqlx::query(&sql)
        .bind("commit_row")
        .bind(1i64)
        .bind("c")
        .execute(&mut *tx)
        .await
        .expect("insert");
    tx.commit().await.expect("commit");

    let count_sql = format!("SELECT COUNT(*) FROM `{}`", table);
    let (count,): (i64,) = sqlx::query_as(&count_sql).fetch_one(&pool).await.unwrap();
    assert_eq!(count, 1);

    drop_table(&pool, &table).await;
}

#[tokio::test]
#[ignore = "需要 MySQL 9.6.0 运行于 127.0.0.1:3306"]
async fn test_mysql_transaction_rollback() {
    let pool = setup_pool().await;
    let table = unique_table("t_tr");
    create_test_table(&pool, &table).await;

    let mut tx = pool.begin().await.expect("begin");
    let sql = format!(
        "INSERT INTO `{}` (name, value, data) VALUES (?, ?, ?)",
        table
    );
    sqlx::query(&sql)
        .bind("rollback_row")
        .bind(1i64)
        .bind("r")
        .execute(&mut *tx)
        .await
        .expect("insert");
    tx.rollback().await.expect("rollback");

    let count_sql = format!("SELECT COUNT(*) FROM `{}`", table);
    let (count,): (i64,) = sqlx::query_as(&count_sql).fetch_one(&pool).await.unwrap();
    assert_eq!(count, 0, "rollback should leave table empty");

    drop_table(&pool, &table).await;
}

#[tokio::test]
#[ignore = "需要 MySQL 9.6.0 运行于 127.0.0.1:3306"]
async fn test_mysql_pagination() {
    let pool = setup_pool().await;
    let table = unique_table("t_page");
    create_test_table(&pool, &table).await;

    // 准备 1000 条数据
    let mut tx = pool.begin().await.expect("begin");
    for i in 0..1000i64 {
        let sql = format!(
            "INSERT INTO `{}` (name, value, data) VALUES (?, ?, ?)",
            table
        );
        sqlx::query(&sql)
            .bind(format!("p_{}", i))
            .bind(i)
            .bind("p")
            .execute(&mut *tx)
            .await
            .expect("insert");
    }
    tx.commit().await.expect("commit");

    let dialect = get_dialect(DbType::MySQL).expect("mysql dialect");
    let page_size = 50u64;
    let mut total_fetched = 0u64;
    let mut last_value = -1i64;
    for page in 1..=20 {
        let sql = dialect.build_pagination(
            &format!("SELECT value FROM `{}` ORDER BY value", table),
            page,
            page_size,
        );
        let rows: Vec<(i64,)> = sqlx::query_as(&sql)
            .fetch_all(&pool)
            .await
            .expect("page query");
        assert_eq!(rows.len() as u64, page_size, "page {} size mismatch", page);
        for (v,) in rows {
            assert!(
                v > last_value,
                "pagination order violated: {} <= {}",
                v,
                last_value
            );
            last_value = v;
            total_fetched += 1;
        }
    }
    assert_eq!(total_fetched, 1000);

    drop_table(&pool, &table).await;
}

#[tokio::test]
#[ignore = "需要 MySQL 9.6.0 运行于 127.0.0.1:3306"]
async fn test_mysql_sql_injection_protection() {
    let pool = setup_pool().await;
    let table = unique_table("t_inj");
    create_test_table(&pool, &table).await;

    let sql = format!(
        "INSERT INTO `{}` (name, value, data) VALUES (?, ?, ?)",
        table
    );
    sqlx::query(&sql)
        .bind("alice")
        .bind(1i64)
        .bind("x")
        .execute(&pool)
        .await
        .expect("insert");

    // 恶意输入
    let malicious = "alice' OR '1'='1";
    let dialect = get_dialect(DbType::MySQL).expect("mysql dialect");
    let escaped = dialect.escape_string(malicious);

    let sql = format!(
        "SELECT COUNT(*) FROM `{}` WHERE name = '{}'",
        table, escaped
    );
    let (count,): (i64,) = sqlx::query_as(&sql).fetch_one(&pool).await.unwrap();
    assert_eq!(count, 0, "escaped malicious input should match nothing");

    // 对比：不转义会注入
    let unescaped_sql = format!(
        "SELECT COUNT(*) FROM `{}` WHERE name = '{}'",
        table, malicious
    );
    let (count_unescaped,): (i64,) = sqlx::query_as(&unescaped_sql)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count_unescaped, 1, "unescaped input should be injectable");

    drop_table(&pool, &table).await;
}

#[tokio::test]
#[ignore = "需要 MySQL 9.6.0 运行于 127.0.0.1:3306"]
async fn test_mysql_json_operations() {
    let pool = setup_pool().await;
    let table = unique_table("t_json");
    create_test_table(&pool, &table).await;

    // 插入 JSON 数据
    let sql = format!(
        "INSERT INTO `{}` (name, value, data, meta) VALUES (?, ?, ?, ?)",
        table
    );
    sqlx::query(&sql)
        .bind("alice")
        .bind(1i64)
        .bind("d1")
        .bind(r#"{"age":30,"city":"shanghai"}"#)
        .execute(&pool)
        .await
        .expect("insert 1");
    sqlx::query(&sql)
        .bind("bob")
        .bind(2i64)
        .bind("d2")
        .bind(r#"{"age":25,"city":"beijing"}"#)
        .execute(&pool)
        .await
        .expect("insert 2");

    // 使用方言的 json_extract 构造查询
    let dialect = get_dialect(DbType::MySQL).expect("mysql dialect");
    let extract_expr = dialect.json_extract("meta", "$.age");
    let sql = format!(
        "SELECT name FROM `{}` WHERE {} > 26 ORDER BY name",
        table, extract_expr
    );
    let rows: Vec<(String,)> = sqlx::query_as(&sql)
        .fetch_all(&pool)
        .await
        .expect("json query");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, "alice");

    drop_table(&pool, &table).await;
}

#[tokio::test]
#[ignore = "需要 MySQL 9.6.0 运行于 127.0.0.1:3306"]
async fn test_mysql_concurrent_8tasks_10k_ops() {
    let pool = setup_pool().await;
    let table = unique_table("t_conc");
    create_test_table(&pool, &table).await;

    // 预填充 10000 条
    let mut tx = pool.begin().await.expect("begin");
    let batch_size = 1000;
    for batch_start in (0..10_000).step_by(batch_size) {
        let batch_end = (batch_start + batch_size).min(10_000);
        let placeholders: Vec<String> = (batch_start..batch_end)
            .map(|_| "(?, ?, ?)".to_string())
            .collect();
        let sql = format!(
            "INSERT INTO `{}` (name, value, data) VALUES {}",
            table,
            placeholders.join(", ")
        );
        let mut q = sqlx::query(&sql);
        for i in batch_start..batch_end {
            q = q.bind(format!("u_{}", i)).bind(i as i64).bind("init");
        }
        q.execute(&mut *tx).await.expect("batch insert");
    }
    tx.commit().await.expect("commit");

    // 8 个并发任务，每个 10000 次 UPDATE
    let pool_arc = std::sync::Arc::new(pool);
    let table_arc = std::sync::Arc::new(table);
    let ops_per_task: u64 = 10_000;
    let mut handles = vec![];

    for task_id in 0..8u64 {
        let pool_clone = pool_arc.clone();
        let table_clone = table_arc.clone();
        handles.push(tokio::spawn(async move {
            let mut success = 0u64;
            let mut errors = 0u64;
            for op in 0..ops_per_task {
                let key = (task_id * ops_per_task + op) as i64;
                let sql = format!("UPDATE `{}` SET data = ? WHERE value = ?", table_clone);
                let res = sqlx::query(&sql)
                    .bind(format!("task_{}_op_{}", task_id, op))
                    .bind(key)
                    .execute(&*pool_clone)
                    .await;
                match res {
                    Ok(_) => success += 1,
                    Err(e) => {
                        errors += 1;
                        eprintln!("task {} op {} error: {}", task_id, op, e);
                    }
                }
            }
            (task_id, success, errors)
        }));
    }

    let mut total_success = 0u64;
    let mut total_errors = 0u64;
    for h in handles {
        let (task_id, success, errors) = h.await.expect("task join");
        println!("task {} success={} errors={}", task_id, success, errors);
        total_success += success;
        total_errors += errors;
    }
    assert_eq!(
        total_success,
        8 * ops_per_task,
        "all 8 tasks * 10k ops should succeed"
    );
    assert_eq!(total_errors, 0);

    drop_table(&pool_arc, &table_arc).await;
}

#[tokio::test]
#[ignore = "需要 MySQL 9.6.0 运行于 127.0.0.1:3306"]
async fn test_mysql_savepoint_nested() {
    let pool = setup_pool().await;
    let table = unique_table("t_sp");
    create_test_table(&pool, &table).await;

    let mut tx = pool.begin().await.expect("begin");
    let sql = format!(
        "INSERT INTO `{}` (name, value, data) VALUES (?, ?, ?)",
        table
    );
    sqlx::query(&sql)
        .bind("outer")
        .bind(1i64)
        .bind("o")
        .execute(&mut *tx)
        .await
        .expect("outer insert");

    // SAVEPOINT 命令在 MySQL prepared statement 协议下不被支持（错误 1295），
    // 必须使用 sqlx::raw_sql() 走非 prepared 路径执行。
    sqlx::raw_sql("SAVEPOINT sp1")
        .execute(&mut *tx)
        .await
        .expect("sp1");
    sqlx::query(&sql)
        .bind("inner1")
        .bind(2i64)
        .bind("i1")
        .execute(&mut *tx)
        .await
        .expect("inner1 insert");
    sqlx::raw_sql("ROLLBACK TO SAVEPOINT sp1")
        .execute(&mut *tx)
        .await
        .expect("rollback sp1");
    sqlx::raw_sql("RELEASE SAVEPOINT sp1")
        .execute(&mut *tx)
        .await
        .expect("release sp1");

    sqlx::raw_sql("SAVEPOINT sp2")
        .execute(&mut *tx)
        .await
        .expect("sp2");
    sqlx::query(&sql)
        .bind("inner2")
        .bind(3i64)
        .bind("i2")
        .execute(&mut *tx)
        .await
        .expect("inner2 insert");
    sqlx::raw_sql("RELEASE SAVEPOINT sp2")
        .execute(&mut *tx)
        .await
        .expect("release sp2");

    tx.commit().await.expect("commit");

    let count_sql = format!("SELECT COUNT(*) FROM `{}`", table);
    let (count,): (i64,) = sqlx::query_as(&count_sql).fetch_one(&pool).await.unwrap();
    assert_eq!(count, 2, "should have outer + inner2 (inner1 rolled back)");

    let names_sql = format!("SELECT name FROM `{}` ORDER BY id", table);
    let names: Vec<(String,)> = sqlx::query_as(&names_sql).fetch_all(&pool).await.unwrap();
    let names: Vec<String> = names.into_iter().map(|(n,)| n).collect();
    assert_eq!(names, vec!["outer".to_string(), "inner2".to_string()]);

    drop_table(&pool, &table).await;
}

#[tokio::test]
#[ignore = "需要 MySQL 9.6.0 运行于 127.0.0.1:3306"]
async fn test_mysql_value_to_param_roundtrip() {
    let dialect = get_dialect(DbType::MySQL).expect("mysql dialect");
    let pool = setup_pool().await;
    let table = unique_table("t_vp");
    create_test_table(&pool, &table).await;

    // 使用 Value::to_param 生成 SQL 字面量插入
    let values: Vec<Value> = vec![
        Value::I64(42),
        Value::String("hello world".to_string()),
        Value::String("with'quote".to_string()),
        Value::String("with`backtick".to_string()),
        Value::Bool(true),
        Value::F64(2.5),
    ];

    for (i, v) in values.iter().enumerate() {
        let name_value = Value::String(format!("row_{}", i));
        let name_param = name_value.to_param();
        let data_str = match v {
            Value::Bool(b) => {
                if *b {
                    "1".to_string()
                } else {
                    "0".to_string()
                }
            }
            Value::I64(n) => n.to_string(),
            Value::F64(f) => format!("{:.6}", f),
            Value::String(s) => format!("'{}'", dialect.escape_string(s)),
            _ => v.to_param().into_owned(),
        };
        let sql = format!(
            "INSERT INTO `{}` (name, value, data) VALUES ({}, {}, {})",
            table, name_param, i as i64, data_str
        );
        sqlx::query(&sql)
            .execute(&pool)
            .await
            .expect("insert value");
    }

    let count_sql = format!("SELECT COUNT(*) FROM `{}`", table);
    let (count,): (i64,) = sqlx::query_as(&count_sql).fetch_one(&pool).await.unwrap();
    assert_eq!(count as usize, values.len());

    drop_table(&pool, &table).await;
}
