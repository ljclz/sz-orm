//! 批① L2 路径对标：池预处理语句缓存命中 vs 直执行（对拍，SQLite 内存库）
//!
//! 对标对象：
//! - 快路径：`PooledConnection::query_prepared` 命中 `PreparedStatementCache`
//!   （SZ_ORM_PREPARED_CACHE 运行时开关，pool.rs `prepared_cache_from_env`）
//! - 慢路径：`Connection::query_with_params` 直执行
//! - 不变量：开关开/关两种模式下，同一 SQL + 参数的查询结果集必须逐行逐值一致。
//! - 边界集：固定边界值 + 可复现随机行集（手写 xorshift，整数域精确比对）。
//!
//! 环境变量隔离说明：SZ_ORM_PREPARED_CACHE 是进程级开关（Pool 初始化时读取一次），
//! 因此同一进程内切换开关只影响各自新建的 Pool——本测试通过为每种模式独立建池规避。
//!
//! 运行方式：cargo test -p sz-orm-sqlx --test prepared_cache_parity

use std::collections::HashMap;

use sz_orm_core::Value;
use sz_orm_core::{Pool, PoolConfig};
use sz_orm_sqlx::{SqlitePoolHandle, SqlxSqliteConnectionFactory};

/// 统一结果归一化：行内键序无关 + 值精确比对（Debug 格式含变体名，i64 域无浮点歧义）
fn normalize_rows(rows: &[HashMap<String, Value>]) -> Vec<Vec<(String, String)>> {
    let mut out: Vec<Vec<(String, String)>> = rows
        .iter()
        .map(|row| {
            let mut kv: Vec<(String, String)> = row
                .iter()
                .map(|(k, v)| (k.clone(), format!("{:?}", v)))
                .collect();
            kv.sort();
            kv
        })
        .collect();
    out.sort();
    out
}

async fn setup_pool() -> Pool {
    let cfg = PoolConfig {
        max_size: 2,
        ..PoolConfig::default()
    };
    let pool = Pool::new(
        cfg,
        std::sync::Arc::new(SqlxSqliteConnectionFactory::new(std::sync::Arc::new(
            SqlitePoolHandle::connect("sqlite::memory:")
                .await
                .expect("connect sqlite"),
        ))),
    )
    .expect("pool new");
    {
        let mut conn = pool.acquire().await.expect("acquire");
        conn.execute(
            "CREATE TABLE parity_t (id INTEGER PRIMARY KEY, name TEXT NOT NULL, age INTEGER NOT NULL)",
        )
        .await
        .expect("create table");
    }
    pool
}

async fn seed(pool: &Pool, users: &[(i64, String, i64)]) {
    let mut conn = pool.acquire().await.expect("acquire");
    for (id, name, age) in users {
        conn.execute_with_params(
            "INSERT INTO parity_t (id, name, age) VALUES (?, ?, ?)",
            &[
                Value::I64(*id),
                Value::String(name.clone()),
                Value::I64(*age),
            ],
        )
        .await
        .expect("seed insert");
    }
}

/// 固定边界集：空表 / 单行 / 极值 / 重复值
#[tokio::test]
async fn parity_prepared_on_off_boundary() {
    let cases: Vec<Vec<(i64, String, i64)>> = vec![
        vec![],
        vec![(1, "a".into(), 1)],
        vec![
            (i64::MAX, "max".into(), i64::MAX),
            (i64::MIN, "min".into(), i64::MIN),
        ],
        vec![
            (7, "dup".into(), 5),
            (8, "dup".into(), 5),
            (9, "dup".into(), 5),
        ],
    ];
    for users in &cases {
        // 关缓存（慢路径基准）
        std::env::set_var("SZ_ORM_PREPARED_CACHE", "0");
        let pool_off = setup_pool().await;
        seed(&pool_off, users).await;
        let rows_off = {
            let mut conn = pool_off.acquire().await.expect("acquire");
            conn.query_with_params(
                "SELECT id, name, age FROM parity_t WHERE age >= ? ORDER BY id",
                &[Value::I64(0)],
            )
            .await
            .expect("query off")
        };

        // 开缓存（快路径：首次 miss prepare + 二次 hit）
        std::env::set_var("SZ_ORM_PREPARED_CACHE", "1");
        let pool_on = setup_pool().await;
        seed(&pool_on, users).await;
        let rows_on_1 = {
            let mut conn = pool_on.acquire().await.expect("acquire");
            conn.query_prepared(
                "SELECT id, name, age FROM parity_t WHERE age >= ? ORDER BY id",
                &[Value::I64(0)],
            )
            .await
            .expect("query on 1")
        };
        let rows_on_2 = {
            let mut conn = pool_on.acquire().await.expect("acquire");
            conn.query_prepared(
                "SELECT id, name, age FROM parity_t WHERE age >= ? ORDER BY id",
                &[Value::I64(0)],
            )
            .await
            .expect("query on 2")
        };

        assert_eq!(
            normalize_rows(&rows_off),
            normalize_rows(&rows_on_1),
            "边界集（len={}）：慢路径 vs 快路径首次执行结果不一致",
            users.len()
        );
        assert_eq!(
            normalize_rows(&rows_on_1),
            normalize_rows(&rows_on_2),
            "边界集（len={}）：快路径 miss vs hit 结果不一致",
            users.len()
        );
    }
    std::env::remove_var("SZ_ORM_PREPARED_CACHE");
}

/// 可复现随机行集（手写 xorshift，避免引入 rand 依赖）：50 行 × 阈值过滤
#[tokio::test]
async fn parity_prepared_on_off_random_rows() {
    let mut state: u64 = 0x9E3779B97F4A7C15;
    let mut users = Vec::new();
    for i in 0..50 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        users.push((i + 1, format!("n{}", state % 97), (state % 200) as i64));
    }

    std::env::set_var("SZ_ORM_PREPARED_CACHE", "0");
    let pool_off = setup_pool().await;
    seed(&pool_off, &users).await;
    let rows_off = {
        let mut conn = pool_off.acquire().await.expect("acquire");
        conn.query_with_params(
            "SELECT id, name, age FROM parity_t WHERE age >= ? ORDER BY id",
            &[Value::I64(50)],
        )
        .await
        .expect("query off")
    };

    std::env::set_var("SZ_ORM_PREPARED_CACHE", "1");
    let pool_on = setup_pool().await;
    seed(&pool_on, &users).await;
    let sql = "SELECT id, name, age FROM parity_t WHERE age >= ? ORDER BY id";
    let rows_on_1 = {
        let mut conn = pool_on.acquire().await.expect("acquire");
        conn.query_prepared(sql, &[Value::I64(50)])
            .await
            .expect("q1")
    };
    let rows_on_2 = {
        let mut conn = pool_on.acquire().await.expect("acquire");
        conn.query_prepared(sql, &[Value::I64(50)])
            .await
            .expect("q2")
    };

    assert_eq!(
        normalize_rows(&rows_off),
        normalize_rows(&rows_on_1),
        "随机行集：慢路径 vs 快路径结果不一致"
    );
    assert_eq!(
        normalize_rows(&rows_on_1),
        normalize_rows(&rows_on_2),
        "随机行集：miss vs hit 结果不一致"
    );
    std::env::remove_var("SZ_ORM_PREPARED_CACHE");
}
