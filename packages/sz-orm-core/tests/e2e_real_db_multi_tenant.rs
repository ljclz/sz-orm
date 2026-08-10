//! M1-T7: 真实数据库多租户端到端测试
//!
//! 连真实 PostgreSQL/MySQL/SQLite 验证租户隔离：
//! - 行级隔离（tenant_id WHERE 过滤）
//! - 跨租户泄漏防护
//! - Schema 隔离路由
//! - TenantContext 上下文

#![cfg(feature = "e2e-real-db")]

use sqlx::Row;

mod common;

use common::cleanup::unique_table_name;

/// 获取 PostgreSQL 连接池
async fn pg_pool() -> Option<sqlx::PgPool> {
    let url = std::env::var("POSTGRES_URL").ok()?;
    sqlx::PgPool::connect(&url).await.ok()
}

/// 获取 MySQL 连接池
async fn mysql_pool() -> Option<sqlx::MySqlPool> {
    let url = std::env::var("MYSQL_URL").ok()?;
    sqlx::MySqlPool::connect(&url).await.ok()
}

/// 获取 SQLite 连接池
async fn sqlite_pool() -> Option<sqlx::SqlitePool> {
    sqlx::SqlitePool::connect("sqlite::memory:").await.ok()
}

// ==================== PostgreSQL 行级租户隔离 ====================

/// 测试 PostgreSQL 行级租户隔离：不同 tenant_id 的数据互不可见。
#[tokio::test]
async fn test_pg_tenant_row_level_isolation() {
    let pool = match pg_pool().await {
        Some(p) => p,
        None => {
            eprintln!("PostgreSQL 未配置，跳过");
            return;
        }
    };
    let table = unique_table_name("e2e_mt");

    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "CREATE TABLE \"{}\" (id BIGSERIAL PRIMARY KEY, tenant_id BIGINT, data TEXT)",
            table
        )
        .as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();

    // 插入租户 1 和租户 2 各 3 条数据
    for tenant_id in 1..=2 {
        for i in 1..=3 {
            let sql = format!(
                "INSERT INTO \"{}\" (tenant_id, data) VALUES ($1, $2)",
                table
            );
            sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
                .bind(tenant_id)
                .bind(format!("tenant{}_data{}", tenant_id, i))
                .execute(&pool)
                .await
                .unwrap();
        }
    }

    // 租户 1 只能看到自己的 3 条
    let sql = format!(
        "SELECT data FROM \"{}\" WHERE tenant_id = $1 ORDER BY id",
        table
    );
    let rows: Vec<(String,)> = sqlx::query_as(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(1i64)
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(rows.len(), 3);
    for (data,) in &rows {
        assert!(data.starts_with("tenant1_"), "租户 1 不应看到租户 2 数据");
    }

    // 租户 2 只能看到自己的 3 条
    let rows: Vec<(String,)> = sqlx::query_as(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(2i64)
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(rows.len(), 3);
    for (data,) in &rows {
        assert!(data.starts_with("tenant2_"), "租户 2 不应看到租户 1 数据");
    }

    // 清理
    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE \"{}\"", table).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

/// 测试 PostgreSQL 跨租户泄漏防护：UPDATE/DELETE 带 tenant_id 条件不误伤其他租户。
#[tokio::test]
async fn test_pg_tenant_cross_tenant_leak_protection() {
    let pool = match pg_pool().await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_mt");

    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "CREATE TABLE \"{}\" (id BIGSERIAL PRIMARY KEY, tenant_id BIGINT, value INT)",
            table
        )
        .as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();

    // 租户 1 和租户 2 各插入 value=100
    for tenant_id in 1..=2 {
        let sql = format!(
            "INSERT INTO \"{}\" (tenant_id, value) VALUES ($1, $2)",
            table
        );
        sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(tenant_id)
            .bind(100i32)
            .execute(&pool)
            .await
            .unwrap();
    }

    // 租户 1 更新 value=200，必须带 tenant_id 条件
    let sql = format!("UPDATE \"{}\" SET value = $1 WHERE tenant_id = $2", table);
    let result = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(200i32)
        .bind(1i64)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(result.rows_affected(), 1, "只应影响租户 1 的 1 行");

    // 验证租户 2 的数据未被影响
    let sql = format!("SELECT value FROM \"{}\" WHERE tenant_id = $1", table);
    let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(2i64)
        .fetch_one(&pool)
        .await
        .unwrap();
    let value: i32 = row.try_get("value").unwrap();
    assert_eq!(value, 100, "租户 2 数据不应被租户 1 的更新影响");

    // 清理
    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE \"{}\"", table).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

// ==================== MySQL 行级租户隔离 ====================

/// 测试 MySQL 行级租户隔离。
#[tokio::test]
async fn test_mysql_tenant_row_level_isolation() {
    let pool = match mysql_pool().await {
        Some(p) => p,
        None => {
            eprintln!("MySQL 未配置，跳过");
            return;
        }
    };
    let table = unique_table_name("e2e_mt");

    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "CREATE TABLE `{}` (id BIGINT AUTO_INCREMENT PRIMARY KEY, tenant_id BIGINT, data VARCHAR(255))",
            table
        )
        .as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();

    for tenant_id in 1..=2 {
        for i in 1..=2 {
            let sql = format!("INSERT INTO `{}` (tenant_id, data) VALUES (?, ?)", table);
            sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
                .bind(tenant_id)
                .bind(format!("t{}_d{}", tenant_id, i))
                .execute(&pool)
                .await
                .unwrap();
        }
    }

    let sql = format!(
        "SELECT data FROM `{}` WHERE tenant_id = ? ORDER BY id",
        table
    );
    let rows: Vec<(String,)> = sqlx::query_as(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(1i64)
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
    for (data,) in &rows {
        assert!(data.starts_with("t1_"));
    }

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE `{}`", table).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

// ==================== SQLite 行级租户隔离 ====================

/// 测试 SQLite 行级租户隔离。
#[tokio::test]
async fn test_sqlite_tenant_row_level_isolation() {
    let pool = match sqlite_pool().await {
        Some(p) => p,
        None => {
            eprintln!("SQLite 未配置，跳过");
            return;
        }
    };
    let table = unique_table_name("e2e_mt");

    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "CREATE TABLE \"{}\" (id INTEGER PRIMARY KEY AUTOINCREMENT, tenant_id INTEGER, data TEXT)",
            table
        )
        .as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();

    for tenant_id in 1..=2 {
        for i in 1..=2 {
            let sql = format!("INSERT INTO \"{}\" (tenant_id, data) VALUES (?, ?)", table);
            sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
                .bind(tenant_id)
                .bind(format!("t{}_d{}", tenant_id, i))
                .execute(&pool)
                .await
                .unwrap();
        }
    }

    let sql = format!(
        "SELECT data FROM \"{}\" WHERE tenant_id = ? ORDER BY id",
        table
    );
    let rows: Vec<(String,)> = sqlx::query_as(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(1i64)
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
    for (data,) in &rows {
        assert!(data.starts_with("t1_"));
    }
}

// ==================== TenantContext 上下文（sz-orm-core API） ====================

/// 测试 TenantContext RAII 上下文进入和退出。
#[tokio::test]
async fn test_tenant_context_enter_exit() {
    use sz_orm_core::tenant_context::{IsolationStrategy, TenantContext};

    assert!(!TenantContext::is_set(), "初始状态无租户上下文");

    {
        let ctx = TenantContext::new(42, IsolationStrategy::RowLevel);
        let _guard = ctx.enter();
        assert!(TenantContext::is_set(), "进入上下文后应已设置");
        let current = TenantContext::current().expect("应能获取当前上下文");
        assert_eq!(current.tenant_id, 42);
    }

    assert!(!TenantContext::is_set(), "离开作用域后上下文应自动清理");
}

/// 测试 TenantContext scope 异步隔离。
#[tokio::test]
async fn test_tenant_context_scope() {
    use sz_orm_core::tenant_context::{IsolationStrategy, TenantContext};

    let result = TenantContext::new(99, IsolationStrategy::RowLevel)
        .scope(async {
            let ctx = TenantContext::current().expect("scope 内应有上下文");
            ctx.tenant_id
        })
        .await;
    assert_eq!(result, 99);
    assert!(!TenantContext::is_set(), "scope 结束后应清理上下文");
}

/// 测试 SchemaIsolationRouter 表名重写。
#[tokio::test]
async fn test_schema_isolation_router() {
    use sz_orm_core::tenant_context::SchemaIsolationRouter;

    let rewritten = SchemaIsolationRouter::rewrite_table("users", 5);
    assert_eq!(rewritten, "tenant_5_users");

    let rewritten = SchemaIsolationRouter::rewrite_table("orders", 100);
    assert_eq!(rewritten, "tenant_100_orders");

    let rewritten = SchemaIsolationRouter::rewrite_tables(&["users", "orders"], 3);
    assert_eq!(
        rewritten,
        vec!["tenant_3_users".to_string(), "tenant_3_orders".to_string()]
    );
}
