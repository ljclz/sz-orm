//! CockroachDB 真实 DB 验证测试（v7.5.0 组6.1）
//!
//! CockroachDB 使用 PostgreSQL 协议，复用既有 PgPoolHandle，不引入新驱动依赖。
//! 运行方式：cargo test -p sz-orm-sqlx --features cockroachdb --test cockroachdb_test -- --ignored --nocapture
//!
//! 环境变量 SZ_ORM_COCKROACHDB_URL 指向真实 CockroachDB 实例，
//! 默认 postgres://root@127.0.0.1:26257/sz_orm_test?sslmode=disable

use std::sync::Arc;
use sz_orm_sqlx::PgPoolHandle;

const COCKROACHDB_URL_DEFAULT: &str = "postgres://root@127.0.0.1:26257/sz_orm_test?sslmode=disable";

fn cockroachdb_url() -> String {
    std::env::var("SZ_ORM_COCKROACHDB_URL").unwrap_or_else(|_| COCKROACHDB_URL_DEFAULT.to_string())
}

fn unique_table(prefix: &str) -> String {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{}_{}_{}", prefix, pid, nanos % 1_000_000)
}

async fn cleanup_table(pool_handle: &PgPoolHandle, table: &str) {
    let mut conn = pool_handle.pool().acquire().await.unwrap();
    let sql = format!("DROP TABLE IF EXISTS {}", table);
    let _ = sqlx::query(sqlx::AssertSqlSafe(&*sql))
        .execute(&mut *conn)
        .await;
}

// ===================== CRUD 兼容性 =====================

#[tokio::test]
#[ignore = "需要 CockroachDB 在 127.0.0.1:26257"]
async fn cockroachdb_crud_compatibility() {
    let pool_handle = Arc::new(PgPoolHandle::connect(&cockroachdb_url()).await.unwrap());
    let table = unique_table("crdb_crud");
    {
        let mut conn = pool_handle.pool().acquire().await.unwrap();
        let sql = format!(
            "CREATE TABLE {} (id INT PRIMARY KEY, name TEXT NOT NULL, value REAL)",
            table
        );
        sqlx::query(sqlx::AssertSqlSafe(&*sql))
            .execute(&mut *conn)
            .await
            .unwrap();
    }

    {
        let mut conn = pool_handle.pool().acquire().await.unwrap();
        let sql = format!(
            "INSERT INTO {} (id, name, value) VALUES ($1, $2, $3)",
            table
        );
        sqlx::query(sqlx::AssertSqlSafe(&*sql))
            .bind(1i32)
            .bind("alpha")
            .bind(1.5f32)
            .execute(&mut *conn)
            .await
            .unwrap();
    }

    {
        let mut conn = pool_handle.pool().acquire().await.unwrap();
        let sql = format!("SELECT id, name, value FROM {} WHERE id = $1", table);
        let rows = sqlx::query_as::<_, (i32, String, f32)>(sqlx::AssertSqlSafe(&*sql))
            .bind(1i32)
            .fetch_all(&mut *conn)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, 1);
        assert_eq!(rows[0].1, "alpha");
        assert!((rows[0].2 - 1.5f32).abs() < 1e-6);
    }

    {
        let mut conn = pool_handle.pool().acquire().await.unwrap();
        let sql = format!("UPDATE {} SET name = $1 WHERE id = $2", table);
        sqlx::query(sqlx::AssertSqlSafe(&*sql))
            .bind("beta")
            .bind(1i32)
            .execute(&mut *conn)
            .await
            .unwrap();
    }

    {
        let mut conn = pool_handle.pool().acquire().await.unwrap();
        let sql = format!("SELECT name FROM {} WHERE id = $1", table);
        let rows = sqlx::query_as::<_, (String,)>(sqlx::AssertSqlSafe(&*sql))
            .bind(1i32)
            .fetch_all(&mut *conn)
            .await
            .unwrap();
        assert_eq!(rows[0].0, "beta");
    }

    {
        let mut conn = pool_handle.pool().acquire().await.unwrap();
        let sql = format!("DELETE FROM {} WHERE id = $1", table);
        sqlx::query(sqlx::AssertSqlSafe(&*sql))
            .bind(1i32)
            .execute(&mut *conn)
            .await
            .unwrap();
    }

    {
        let mut conn = pool_handle.pool().acquire().await.unwrap();
        let sql = format!("SELECT COUNT(*) FROM {}", table);
        let count: (i64,) = sqlx::query_as(sqlx::AssertSqlSafe(&*sql))
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(count.0, 0);
    }

    cleanup_table(&pool_handle, &table).await;
}

// ===================== JOIN / 子查询兼容性 =====================

#[tokio::test]
#[ignore = "需要 CockroachDB 在 127.0.0.1:26257"]
async fn cockroachdb_join_subquery_compatibility() {
    let pool_handle = Arc::new(PgPoolHandle::connect(&cockroachdb_url()).await.unwrap());
    let orders = unique_table("crdb_orders");
    let customers = unique_table("crdb_customers");
    {
        let mut conn = pool_handle.pool().acquire().await.unwrap();
        let sql = format!("CREATE TABLE {} (id INT PRIMARY KEY, name TEXT)", customers);
        sqlx::query(sqlx::AssertSqlSafe(&*sql))
            .execute(&mut *conn)
            .await
            .unwrap();
        let sql = format!(
            "CREATE TABLE {} (id INT PRIMARY KEY, customer_id INT, amount DECIMAL(10,2))",
            orders
        );
        sqlx::query(sqlx::AssertSqlSafe(&*sql))
            .execute(&mut *conn)
            .await
            .unwrap();
        let sql = format!(
            "INSERT INTO {} (id, name) VALUES (1, 'alice'), (2, 'bob')",
            customers
        );
        sqlx::query(sqlx::AssertSqlSafe(&*sql))
            .execute(&mut *conn)
            .await
            .unwrap();
        let sql = format!(
            "INSERT INTO {} (id, customer_id, amount) VALUES (1, 1, 100.00), (2, 1, 50.00), (3, 2, 200.00)",
            orders
        );
        sqlx::query(sqlx::AssertSqlSafe(&*sql))
            .execute(&mut *conn)
            .await
            .unwrap();
    }

    {
        let mut conn = pool_handle.pool().acquire().await.unwrap();
        let sql = format!(
            "SELECT c.name, SUM(o.amount) FROM {} c JOIN {} o ON c.id = o.customer_id GROUP BY c.name ORDER BY c.name",
            customers, orders
        );
        let rows: Vec<(String, rust_decimal::Decimal)> = sqlx::query_as(sqlx::AssertSqlSafe(&*sql))
            .fetch_all(&mut *conn)
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].0, "alice");
        assert_eq!(rows[1].0, "bob");
    }

    {
        let mut conn = pool_handle.pool().acquire().await.unwrap();
        let sql = format!(
            "SELECT id FROM {} WHERE customer_id IN (SELECT id FROM {} WHERE name = 'alice') ORDER BY id",
            orders, customers
        );
        let rows: Vec<(i32,)> = sqlx::query_as(sqlx::AssertSqlSafe(&*sql))
            .fetch_all(&mut *conn)
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].0, 1);
        assert_eq!(rows[1].0, 2);
    }

    cleanup_table(&pool_handle, &orders).await;
    cleanup_table(&pool_handle, &customers).await;
}

// ===================== 事务与隔离级别 =====================

#[tokio::test]
#[ignore = "需要 CockroachDB 在 127.0.0.1:26257"]
async fn cockroachdb_transaction_isolation() {
    let pool_handle = Arc::new(PgPoolHandle::connect(&cockroachdb_url()).await.unwrap());
    let table = unique_table("crdb_tx_iso");
    {
        let mut conn = pool_handle.pool().acquire().await.unwrap();
        let sql = format!(
            "CREATE TABLE {} (id INT PRIMARY KEY, counter INT NOT NULL DEFAULT 0)",
            table
        );
        sqlx::query(sqlx::AssertSqlSafe(&*sql))
            .execute(&mut *conn)
            .await
            .unwrap();
        let sql = format!("INSERT INTO {} (id, counter) VALUES (1, 0)", table);
        sqlx::query(sqlx::AssertSqlSafe(&*sql))
            .execute(&mut *conn)
            .await
            .unwrap();
    }

    {
        let mut tx = pool_handle.pool().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *tx)
            .await
            .unwrap();
        let sql = format!("SELECT counter FROM {} WHERE id = 1", table);
        let current: (i32,) = sqlx::query_as(sqlx::AssertSqlSafe(&*sql))
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        let sql = format!("UPDATE {} SET counter = $1 WHERE id = 1", table);
        sqlx::query(sqlx::AssertSqlSafe(&*sql))
            .bind(current.0 + 1)
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
    }

    {
        let mut conn = pool_handle.pool().acquire().await.unwrap();
        let sql = format!("SELECT counter FROM {} WHERE id = 1", table);
        let result: (i32,) = sqlx::query_as(sqlx::AssertSqlSafe(&*sql))
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(result.0, 1);
    }

    cleanup_table(&pool_handle, &table).await;
}

// ===================== 索引与视图 =====================

#[tokio::test]
#[ignore = "需要 CockroachDB 在 127.0.0.1:26257"]
async fn cockroachdb_index_view_compatibility() {
    let pool_handle = Arc::new(PgPoolHandle::connect(&cockroachdb_url()).await.unwrap());
    let table = unique_table("crdb_idx_view");
    let view = unique_table("crdb_v_idx_view");
    {
        let mut conn = pool_handle.pool().acquire().await.unwrap();
        let sql = format!(
            "CREATE TABLE {} (id INT PRIMARY KEY, category TEXT, score INT)",
            table
        );
        sqlx::query(sqlx::AssertSqlSafe(&*sql))
            .execute(&mut *conn)
            .await
            .unwrap();
        let sql = format!(
            "CREATE INDEX idx_{}_category ON {} (category)",
            table, table
        );
        sqlx::query(sqlx::AssertSqlSafe(&*sql))
            .execute(&mut *conn)
            .await
            .unwrap();
        let sql = format!(
            "INSERT INTO {} (id, category, score) VALUES (1, 'a', 90), (2, 'a', 80), (3, 'b', 70)",
            table
        );
        sqlx::query(sqlx::AssertSqlSafe(&*sql))
            .execute(&mut *conn)
            .await
            .unwrap();
        let sql = format!(
            "CREATE VIEW {} AS SELECT category, AVG(score) as avg_score FROM {} GROUP BY category",
            view, table
        );
        sqlx::query(sqlx::AssertSqlSafe(&*sql))
            .execute(&mut *conn)
            .await
            .unwrap();
    }

    {
        let mut conn = pool_handle.pool().acquire().await.unwrap();
        let sql = format!("SELECT * FROM {} ORDER BY category", view);
        let rows: Vec<(String, rust_decimal::Decimal)> = sqlx::query_as(sqlx::AssertSqlSafe(&*sql))
            .fetch_all(&mut *conn)
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].0, "a");
        assert_eq!(rows[1].0, "b");
    }

    {
        let mut conn = pool_handle.pool().acquire().await.unwrap();
        let sql = format!("DROP VIEW IF EXISTS {}", view);
        let _ = sqlx::query(sqlx::AssertSqlSafe(&*sql))
            .execute(&mut *conn)
            .await;
    }
    cleanup_table(&pool_handle, &table).await;
}

// ===================== 分布式事务重试 =====================

#[tokio::test]
#[ignore = "需要 CockroachDB 在 127.0.0.1:26257"]
async fn cockroachdb_distributed_tx_retry() {
    let pool_handle = Arc::new(PgPoolHandle::connect(&cockroachdb_url()).await.unwrap());
    let table = unique_table("crdb_retry");
    {
        let mut conn = pool_handle.pool().acquire().await.unwrap();
        let sql = format!("CREATE TABLE {} (id INT PRIMARY KEY, val INT)", table);
        sqlx::query(sqlx::AssertSqlSafe(&*sql))
            .execute(&mut *conn)
            .await
            .unwrap();
    }

    let max_retries = 5;
    let mut attempt = 0;
    let mut success = false;
    while attempt < max_retries {
        attempt += 1;
        let table_owned = table.clone();
        let result: Result<(), sqlx::Error> = async {
            let mut tx = pool_handle.pool().begin().await?;
            let sql = format!("INSERT INTO {} (id, val) VALUES (1, $1)", table_owned);
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(attempt)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            Ok::<(), sqlx::Error>(())
        }
        .await;
        if result.is_ok() {
            success = true;
            break;
        }
    }
    assert!(success, "事务应在有限次重试后成功");

    cleanup_table(&pool_handle, &table).await;
}
