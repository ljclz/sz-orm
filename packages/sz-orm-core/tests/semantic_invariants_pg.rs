//! 批③ L4 深化：语义不变量 PG 真连版（ignored，需 PostgreSQL@5432）
//!
//! SQLite（semantic_invariants.rs）覆盖常驻路径；本套件把同样的恒等式
//! 钉到 PG 方言真连接上（物化/隔离语义差异面：RR 快照、COUNT 形式、
//! 聚合分解在 PG 优化器下的等价性）。
//!
//! 运行方式：
//!   export SZ_ORM_INV_PG_URL="postgres://postgres:test123@127.0.0.1:5432/sz_orm_test"
//!   cargo test -p sz-orm-core --test semantic_invariants_pg -- --ignored
//! 未配置 URL 时干净 SKIP（负载敏感降级）。

#![cfg(feature = "simd")] // 复用 core dev-deps 中 sqlx 的 feature 组合

use std::collections::HashMap;

use sz_orm_core::Value;

fn pg_url() -> Option<String> {
    std::env::var("SZ_ORM_INV_PG_URL").ok().filter(|s| !s.is_empty())
}

type Row = (i64, String, i64);

async fn make_pool() -> sqlx::PgPool {
    let url = pg_url().expect("SZ_ORM_INV_PG_URL");
    sqlx::PgPool::connect(&url).await.expect("connect pg")
}

async fn setup(pool: &sqlx::PgPool, rows: &[Row]) {
    sqlx::query("DROP TABLE IF EXISTS inv_pg_t")
        .execute(pool)
        .await
        .expect("drop");
    sqlx::query("CREATE TABLE inv_pg_t (id INTEGER PRIMARY KEY, name TEXT NOT NULL, age INTEGER NOT NULL)")
        .execute(pool)
        .await
        .expect("create");
    for (id, name, age) in rows {
        sqlx::query("INSERT INTO inv_pg_t (id, name, age) VALUES ($1, $2, $3)")
            .bind(*id)
            .bind(name)
            .bind(*age)
            .execute(pool)
            .await
            .expect("seed");
    }
}

async fn snapshot(pool: &sqlx::PgPool) -> Vec<Row> {
    let rows: Vec<(i64, String, i64)> =
        sqlx::query_as("SELECT id, name, age FROM inv_pg_t ORDER BY id")
            .fetch_all(pool)
            .await
            .expect("snapshot");
    rows
}

fn normalize(rows: &[HashMap<String, Value>]) -> Vec<Vec<(String, String)>> {
    let mut out: Vec<Vec<(String, String)>> = rows
        .iter()
        .map(|r| {
            let mut kv: Vec<(String, String)> =
                r.iter().map(|(k, v)| (k.clone(), format!("{:?}", v))).collect();
            kv.sort();
            kv
        })
        .collect();
    out.sort();
    out
}

/// INV-N5 PG 版：事务内 DML + ROLLBACK 全状态还原
#[tokio::test]
#[ignore = "需要 PostgreSQL@5432（SZ_ORM_INV_PG_URL）"]
async fn pg_inv_n5_rollback_snapshot() {
    if pg_url().is_none() {
        eprintln!("SKIP: SZ_ORM_INV_PG_URL 未配置");
        return;
    }
    let pool = make_pool().await;
    setup(&pool, &[(1, "a".into(), 10), (2, "b".into(), 20)]).await;
    let before = snapshot(&pool).await;

    let mut tx = pool.begin().await.expect("begin");
    sqlx::query("INSERT INTO inv_pg_t (id, name, age) VALUES (3, 'c', 30)")
        .execute(&mut *tx)
        .await
        .expect("tx insert");
    sqlx::query("UPDATE inv_pg_t SET age = 99 WHERE id = 1")
        .execute(&mut *tx)
        .await
        .expect("tx update");
    sqlx::query("DELETE FROM inv_pg_t WHERE id = 2")
        .execute(&mut *tx)
        .await
        .expect("tx delete");
    tx.rollback().await.expect("rollback");

    let after = snapshot(&pool).await;
    assert_eq!(before, after, "PG ROLLBACK 后全状态必须等于前置快照");
}

/// INV-N3 PG 版：全局聚合 == 分组聚合的再聚合（COUNT/SUM/MIN/MAX/AVG 全家）
#[tokio::test]
#[ignore = "需要 PostgreSQL@5432（SZ_ORM_INV_PG_URL）"]
async fn pg_inv_n3_aggregate_decomposition() {
    if pg_url().is_none() {
        eprintln!("SKIP: SZ_ORM_INV_PG_URL 未配置");
        return;
    }
    let pool = make_pool().await;
    setup(
        &pool,
        &[(1, "a".into(), 10), (2, "b".into(), 20), (3, "a".into(), 30)],
    )
    .await;

    // COUNT 分解
    let (gc,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM inv_pg_t")
        .fetch_one(&pool)
        .await
        .expect("global count");
    let groups: Vec<(String, i64)> =
        sqlx::query_as("SELECT name, COUNT(*) FROM inv_pg_t GROUP BY name")
            .fetch_all(&pool)
            .await
            .expect("group count");
    assert_eq!(gc, groups.iter().map(|(_, c)| *c).sum::<i64>());

    // SUM 分解
    let (gs,): (i64,) = sqlx::query_as("SELECT SUM(age) FROM inv_pg_t")
        .fetch_one(&pool)
        .await
        .expect("global sum");
    let sgroups: Vec<(String, i64)> =
        sqlx::query_as("SELECT name, SUM(age) FROM inv_pg_t GROUP BY name")
            .fetch_all(&pool)
            .await
            .expect("group sum");
    assert_eq!(gs, sgroups.iter().map(|(_, s)| *s).sum::<i64>());

    // MIN/MAX 分解
    let (gmin, gmax): (i64, i64) =
        sqlx::query_as("SELECT MIN(age), MAX(age) FROM inv_pg_t")
            .fetch_one(&pool)
            .await
            .expect("global minmax");
    let mgroups: Vec<(String, i64, i64)> =
        sqlx::query_as("SELECT name, MIN(age), MAX(age) FROM inv_pg_t GROUP BY name")
            .fetch_all(&pool)
            .await
            .expect("group minmax");
    assert_eq!(gmin, mgroups.iter().map(|(_, m, _)| *m).min().unwrap());
    assert_eq!(gmax, mgroups.iter().map(|(_, _, x)| *x).max().unwrap());

    // AVG 分解（容差）：全局 AVG == 分组 AVG 的加权和 / 总数
    let (gavg,): (f64,) = sqlx::query_as("SELECT AVG(age) FROM inv_pg_t")
        .fetch_one(&pool)
        .await
        .expect("global avg");
    let agroups: Vec<(String, f64, i64)> =
        sqlx::query_as("SELECT name, AVG(age), COUNT(*) FROM inv_pg_t GROUP BY name")
            .fetch_all(&pool)
            .await
            .expect("group avg");
    let n: i64 = agroups.iter().map(|(_, _, c)| *c).sum();
    let wavg: f64 = agroups.iter().map(|(_, a, c)| a * (*c as f64)).sum::<f64>() / n as f64;
    assert!(
        (gavg - wavg).abs() <= 1e-9 * gavg.abs().max(1.0),
        "AVG 分解超容差: {} vs {}",
        gavg,
        wavg
    );
}

/// INV-N7 PG 版：COUNT 三形式一致
#[tokio::test]
#[ignore = "需要 PostgreSQL@5432（SZ_ORM_INV_PG_URL）"]
async fn pg_inv_n7_count_forms_agree() {
    if pg_url().is_none() {
        eprintln!("SKIP: SZ_ORM_INV_PG_URL 未配置");
        return;
    }
    let pool = make_pool().await;
    setup(&pool, &[(1, "a".into(), 10), (2, "b".into(), 20)]).await;
    let (c_star, c_one, c_col): (i64, i64, i64) =
        sqlx::query_as("SELECT COUNT(*), COUNT(1), COUNT(id) FROM inv_pg_t")
            .fetch_one(&pool)
            .await
            .expect("counts");
    assert_eq!(c_star, c_one, "COUNT(*) == COUNT(1)");
    assert_eq!(c_star, c_col, "COUNT(*) == COUNT(非空列)");
}

/// INV-N2 PG 版：可重复读下重放确定（REPEATABLE READ 快照语义探针）
#[tokio::test]
#[ignore = "需要 PostgreSQL@5432（SZ_ORM_INV_PG_URL）"]
async fn pg_inv_n2_repeatable_read_replay() {
    if pg_url().is_none() {
        eprintln!("SKIP: SZ_ORM_INV_PG_URL 未配置");
        return;
    }
    let pool = make_pool().await;
    setup(&pool, &[(1, "a".into(), 10), (2, "b".into(), 20)]).await;

    // RR 事务内两次读同一结果（确定性）
    let mut tx = pool
        .begin()
        .await
        .expect("begin");
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ")
        .execute(&mut *tx)
        .await
        .expect("set iso");
    let r1: Vec<Row> = sqlx::query_as("SELECT id, name, age FROM inv_pg_t ORDER BY id")
        .fetch_all(&mut *tx)
        .await
        .expect("read 1");
    let r2: Vec<Row> = sqlx::query_as("SELECT id, name, age FROM inv_pg_t ORDER BY id")
        .fetch_all(&mut *tx)
        .await
        .expect("read 2");
    tx.commit().await.expect("commit");
    assert_eq!(r1, r2, "RR 快照内重放必须确定");
    assert_eq!(normalize_rows_probe(&r1), normalize_rows_probe(&r2));
}

fn normalize_rows_probe(rows: &[Row]) -> Vec<String> {
    rows.iter().map(|(i, n, a)| format!("{}|{}|{}", i, n, a)).collect()
}
