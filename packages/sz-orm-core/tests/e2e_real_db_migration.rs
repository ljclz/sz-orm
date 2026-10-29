//! v8.3.0: 真实数据库迁移端到端测试
//!
//! 连第二台服务器 MySQL/PostgreSQL + 本机 Oracle 验证迁移
//! 创建表/修改表/版本管理/幂等性。

#![cfg(feature = "e2e-real-db")]

use sqlx::Row;

mod common;

use common::cleanup::unique_table_name;
use common::e2e_env::{e2e_mysql_pool, e2e_oracle_conn, e2e_pg_pool, E2eMysqlDb};

// ==================== MySQL 迁移 ====================

#[tokio::test]
async fn test_e2e_mysql_migration_create_table() {
    let pool = match e2e_mysql_pool(E2eMysqlDb::Test).await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_mig_mysql");
    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "CREATE TABLE `{}` (id BIGINT AUTO_INCREMENT PRIMARY KEY, name VARCHAR(255), email VARCHAR(255))",
            table
        )
        .as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();

    let row = sqlx::query(sqlx::AssertSqlSafe(
        "SELECT COUNT(*) as cnt FROM information_schema.columns WHERE table_name = ?",
    ))
    .bind(table.as_str())
    .fetch_one(&pool)
    .await
    .unwrap();
    let cnt: i64 = row.try_get("cnt").unwrap();
    assert_eq!(cnt, 3);

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE `{}`", table).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn test_e2e_mysql_migration_idempotent() {
    let pool = match e2e_mysql_pool(E2eMysqlDb::Test).await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_mig_idem_mysql");
    let create_sql = format!(
        "CREATE TABLE IF NOT EXISTS `{}` (id BIGINT AUTO_INCREMENT PRIMARY KEY, name VARCHAR(255))",
        table
    );
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();

    let row = sqlx::query(sqlx::AssertSqlSafe(
        format!("SELECT COUNT(*) as cnt FROM `{}`", table).as_str(),
    ))
    .fetch_one(&pool)
    .await
    .unwrap();
    let count: i64 = row.try_get("cnt").unwrap();
    assert_eq!(count, 0);

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE `{}`", table).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

// ==================== PostgreSQL 迁移 ====================

#[tokio::test]
async fn test_e2e_pg_migration_alter_table() {
    let pool = match e2e_pg_pool().await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_mig_pg");
    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "CREATE TABLE \"{}\" (id BIGSERIAL PRIMARY KEY, name TEXT)",
            table
        )
        .as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(sqlx::AssertSqlSafe(
        format!("ALTER TABLE \"{}\" ADD COLUMN email TEXT", table).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(sqlx::AssertSqlSafe(
        format!("INSERT INTO \"{}\" (name, email) VALUES ($1, $2)", table).as_str(),
    ))
    .bind("Alice")
    .bind("alice@example.com")
    .execute(&pool)
    .await
    .unwrap();

    let row = sqlx::query(sqlx::AssertSqlSafe(
        format!("SELECT name, email FROM \"{}\" WHERE name = $1", table).as_str(),
    ))
    .bind("Alice")
    .fetch_one(&pool)
    .await
    .unwrap();
    let name: String = row.try_get("name").unwrap();
    let email: String = row.try_get("email").unwrap();
    assert_eq!(name, "Alice");
    assert_eq!(email, "alice@example.com");

    sqlx::query(sqlx::AssertSqlSafe(
        format!("ALTER TABLE \"{}\" DROP COLUMN email", table).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE \"{}\"", table).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn test_e2e_pg_migration_idempotent() {
    let pool = match e2e_pg_pool().await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_mig_idem_pg");
    let create_sql = format!(
        "CREATE TABLE IF NOT EXISTS \"{}\" (id BIGSERIAL PRIMARY KEY, name TEXT)",
        table
    );
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE \"{}\"", table).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

// ==================== Oracle 迁移 ====================

#[tokio::test]
async fn test_e2e_oracle_migration_create_table() {
    let conn = match e2e_oracle_conn() {
        Some(c) => c,
        None => return,
    };
    let table = unique_table_name("e2e_mig_ora");
    let table_lower = table.to_lowercase();
    conn.execute(
        &format!(
            "CREATE TABLE \"{}\" (id NUMBER GENERATED ALWAYS AS IDENTITY PRIMARY KEY, name VARCHAR2(255))",
            table_lower
        ),
        &[],
    )
    .unwrap();

    conn.execute(
        &format!("INSERT INTO \"{}\" (name) VALUES (:1)", table_lower),
        &[&"test_user"],
    )
    .unwrap();
    conn.commit().unwrap();

    let rows = conn
        .query(&format!("SELECT name FROM \"{}\"", table_lower), &[])
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let name: String = rows[0].get(0).unwrap();
    assert_eq!(name, "test_user");

    conn.execute(&format!("DROP TABLE \"{}\"", table_lower), &[])
        .unwrap();
}

#[tokio::test]
async fn test_e2e_oracle_migration_idempotent() {
    let conn = match e2e_oracle_conn() {
        Some(c) => c,
        None => return,
    };
    let table = unique_table_name("e2e_mig_idem_ora");
    let table_lower = table.to_lowercase();

    conn.execute(
        &format!(
            "CREATE TABLE \"{}\" (id NUMBER GENERATED ALWAYS AS IDENTITY PRIMARY KEY, name VARCHAR2(255))",
            table_lower
        ),
        &[],
    )
    .unwrap();

    let rows = conn
        .query(
            "SELECT COUNT(*) FROM user_tables WHERE table_name = :1",
            &[&table_lower.as_str()],
        )
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let count: i64 = rows[0].get(0).unwrap();
    assert_eq!(count, 1);

    conn.execute(&format!("DROP TABLE \"{}\"", table_lower), &[])
        .unwrap();
}
// ==================== SQLite 迁移 ====================

#[tokio::test]
async fn test_e2e_sqlite_migration_create_table() {
    let pool = match sqlx::SqlitePool::connect("sqlite::memory:").await.ok() {
        Some(p) => p,
        None => return,
    };
    sqlx::query(sqlx::AssertSqlSafe(
        "CREATE TABLE IF NOT EXISTS schema_version (version INTEGER PRIMARY KEY)",
    ))
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(sqlx::AssertSqlSafe(
        "INSERT INTO schema_version (version) VALUES (?)",
    ))
    .bind(1_i64)
    .execute(&pool)
    .await
    .unwrap();

    let row = sqlx::query(sqlx::AssertSqlSafe("SELECT version FROM schema_version"))
        .fetch_one(&pool)
        .await
        .unwrap();
    let version: i64 = row.try_get("version").unwrap();
    assert_eq!(version, 1);
}

// ==================== R7：迁移 checksum + 跨进程锁（PostgreSQL 真库） ====================

/// 专用单连接池：保证会话固定（会话级 advisory lock 必须落在同一物理连接上）
async fn pg_single_session_pool(url: &str) -> sqlx::PgPool {
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .min_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(10))
        .connect(url)
        .await
        .expect("单连接池建立失败")
}

fn pg_migrator(
    pool: sqlx::PgPool,
    table: &str,
    version: &str,
    sql_up: &str,
    sql_down: &str,
) -> sz_orm_core::migration::Migrator {
    use sz_orm_core::migration::{Migration, MigrationContext, Migrator};
    use sz_orm_core::DbType;
    let ctx = MigrationContext {
        table_name: table.to_string(),
        connection: Some(Box::new(common::sqlx_pg_adapter::SqlxPgAdapter::new(pool))),
        db_type: Some(DbType::PostgreSQL),
    };
    Migrator::new(ctx).add_migration(Migration::new(version, version, sql_up, sql_down))
}

#[tokio::test]
async fn test_r7_pg_checksum_mismatch_rejected() {
    let Some(url) = common::e2e_skip::pg_url() else {
        return;
    };
    let pool = pg_single_session_pool(&url).await;
    let suffix = std::process::id();
    let table = format!("__r7_checksum_{}", suffix);
    let data_table = format!("r7_checksum_data_{}", suffix);

    let up = format!("CREATE TABLE {} (id INT PRIMARY KEY)", data_table);
    let down = format!("DROP TABLE {}", data_table);

    let mut first = pg_migrator(pool.clone(), &table, "r7_001", &up, &down);
    let applied = first.migrate().await.expect("首次迁移必须成功");
    assert_eq!(applied, vec!["r7_001".to_string()]);

    // 篡改已执行迁移的 up SQL：必须被 checksum 校验拒绝
    let tampered_up = format!(
        "CREATE TABLE {} (id INT PRIMARY KEY, injected TEXT)",
        data_table
    );
    let mut second = pg_migrator(pool.clone(), &table, "r7_001", &tampered_up, &down);
    let err = second.migrate().await.expect_err("篡改后必须报错");
    let msg = err.to_string();
    assert!(msg.contains("checksum 不匹配"), "实际错误：{}", msg);

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE {}", data_table).as_str(),
    ))
    .execute(&pool)
    .await
    .expect("清理数据表失败");
    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE {}", table).as_str(),
    ))
    .execute(&pool)
    .await
    .expect("清理迁移表失败");
}

#[tokio::test]
async fn test_r7_pg_concurrent_migrate_is_serialized() {
    let Some(url) = common::e2e_skip::pg_url() else {
        return;
    };
    let suffix = std::process::id();
    let table = format!("__r7_lock_{}", suffix);
    let data_table = format!("r7_lock_data_{}", suffix);

    // 两个独立会话（各自单连接池），模拟两个应用实例同时启动
    let pool_a = pg_single_session_pool(&url).await;
    let pool_b = pg_single_session_pool(&url).await;
    let up = format!("CREATE TABLE {} (id INT PRIMARY KEY)", data_table);
    let down = format!("DROP TABLE {}", data_table);

    let mut migrator_a = pg_migrator(pool_a.clone(), &table, "r7_lock_001", &up, &down);
    let mut migrator_b = pg_migrator(pool_b.clone(), &table, "r7_lock_001", &up, &down);

    let (res_a, res_b) = tokio::join!(migrator_a.migrate(), migrator_b.migrate());
    let a = res_a.expect("实例 A 迁移必须成功（后到者等待锁）");
    let b = res_b.expect("实例 B 迁移必须成功（后到者等待锁）");
    assert_eq!(
        a.len() + b.len(),
        1,
        "同一迁移只能被执行一次，实际 A={:?} B={:?}",
        a,
        b
    );

    // 迁移表内同一版本只有一条记录
    let row = sqlx::query(sqlx::AssertSqlSafe(
        format!("SELECT COUNT(*) AS cnt FROM {}", table).as_str(),
    ))
    .fetch_one(&pool_a)
    .await
    .expect("查询迁移表失败");
    let cnt: i64 = row.try_get("cnt").expect("读取计数失败");
    assert_eq!(cnt, 1, "迁移记录必须只有一条");

    // 没有锁时两实例都会执行 CREATE TABLE，必然有一方因“已存在”失败；
    // 两个都成功即证明 DDL 被串行化。
    let exists = sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "SELECT COUNT(*) AS cnt FROM information_schema.tables WHERE table_name = '{}'",
            data_table
        )
        .as_str(),
    ))
    .fetch_one(&pool_a)
    .await
    .expect("查询 information_schema 失败");
    let table_cnt: i64 = exists.try_get("cnt").expect("读取表计数失败");
    assert_eq!(table_cnt, 1, "数据表必须存在且唯一");

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE {}", data_table).as_str(),
    ))
    .execute(&pool_a)
    .await
    .expect("清理数据表失败");
    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE {}", table).as_str(),
    ))
    .execute(&pool_a)
    .await
    .expect("清理迁移表失败");
}

#[tokio::test]
async fn test_r7_pg_lock_contention_waits_then_times_out() {
    let Some(url) = common::e2e_skip::pg_url() else {
        return;
    };
    let suffix = std::process::id();
    let table = format!("__r7_wait_{}", suffix);
    let lock_key = format!("szorm_migrate_{}", table);
    let data_table = format!("r7_wait_data_{}", suffix);

    let holder = pg_single_session_pool(&url).await;
    let worker = pg_single_session_pool(&url).await;

    // 外部会话先持锁，模拟“另一个实例正在迁移”
    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "SELECT pg_advisory_lock(hashtext('{}')) AS acquired",
            lock_key
        )
        .as_str(),
    ))
    .execute(&holder)
    .await
    .expect("外部会话持锁失败");

    std::env::set_var("SZ_ORM_MIGRATION_LOCK_TIMEOUT_SECS", "1");
    let up = format!("CREATE TABLE {} (id INT PRIMARY KEY)", data_table);
    let down = format!("DROP TABLE {}", data_table);
    let mut migrator = pg_migrator(worker.clone(), &table, "r7_wait_001", &up, &down);
    let err = migrator
        .migrate()
        .await
        .expect_err("锁被外部持有时必须等待到超时并报错");
    std::env::remove_var("SZ_ORM_MIGRATION_LOCK_TIMEOUT_SECS");
    let msg = err.to_string();
    assert!(msg.contains("等待迁移锁超时"), "实际错误：{}", msg);

    // 释放外部锁后同一迁移必须能正常执行（等待路径不会污染后续状态）
    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "SELECT pg_advisory_unlock(hashtext('{}')) AS released",
            lock_key
        )
        .as_str(),
    ))
    .execute(&holder)
    .await
    .expect("释放外部锁失败");

    let applied = migrator.migrate().await.expect("释放锁后迁移必须成功");
    assert_eq!(applied, vec!["r7_wait_001".to_string()]);

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE {}", data_table).as_str(),
    ))
    .execute(&worker)
    .await
    .expect("清理数据表失败");
    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE {}", table).as_str(),
    ))
    .execute(&worker)
    .await
    .expect("清理迁移表失败");
}
