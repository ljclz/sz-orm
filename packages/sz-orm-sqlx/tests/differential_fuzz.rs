//! ORM 结果集差分模糊测试（Differential Fuzz Testing）
//!
//! 使用 proptest 生成随机数据，比较 sz-orm-sqlx 的 Connection::query 结果
//! 与原生 sqlx 查询结果的一致性，覆盖 5 种典型查询场景。
//!
//! 运行方式：
//!   cargo test -p sz-orm-sqlx --test differential_fuzz

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use proptest::prelude::*;
use sqlx::{Column, Row};
use sz_orm_core::{Connection, ConnectionFactory, DbError, Value};
use sz_orm_sqlx::{SqlitePoolHandle, SqlxSqliteConnectionFactory};

// ===================== proptest 策略 =====================

/// 生成随机用户记录（去重 ID）：Vec<(id, name, age)>
fn random_users() -> impl Strategy<Value = Vec<(i64, String, i64)>> {
    prop::collection::vec((1i64..10000i64, "[a-z]{2,10}", 1i64..120i64), 1..=30).prop_map(|users| {
        let mut seen = std::collections::HashSet::new();
        users
            .into_iter()
            .filter(|(id, _, _)| seen.insert(*id))
            .collect()
    })
}

// ===================== 测试数据库设置 =====================

/// 在 SQLite 内存数据库中建表并插入数据，返回 ORM 连接和原生 sqlx pool
async fn setup_db(
    users: &[(i64, String, i64)],
) -> Result<(Box<dyn Connection>, sqlx::SqlitePool), DbError> {
    let pool_handle = SqlitePoolHandle::connect("sqlite::memory:").await?;
    let native_pool = pool_handle.pool().clone();

    let factory = SqlxSqliteConnectionFactory::new(Arc::new(pool_handle));
    let mut conn = factory.create().await?;

    conn.execute(
        "CREATE TABLE diff_users (\
         id INTEGER PRIMARY KEY,\
         name TEXT NOT NULL,\
         age INTEGER NOT NULL)",
    )
    .await?;

    for (id, name, age) in users {
        conn.execute_with_params(
            "INSERT INTO diff_users (id, name, age) VALUES (?, ?, ?)",
            &[
                Value::I64(*id),
                Value::String(name.clone()),
                Value::I64(*age),
            ],
        )
        .await?;
    }

    Ok((conn, native_pool))
}

// ===================== 结果集比较 =====================

/// 将 sqlx 原生行转换为与 ORM 兼容的 `HashMap<String, Value>`
fn native_rows_to_map(rows: &[sqlx::sqlite::SqliteRow]) -> Vec<HashMap<String, Value>> {
    if rows.is_empty() {
        return Vec::new();
    }
    let col_names: Vec<String> = rows[0]
        .columns()
        .iter()
        .map(|c| c.name().to_string())
        .collect();
    rows.iter()
        .map(|row| {
            let mut map = HashMap::with_capacity(col_names.len());
            for (i, name) in col_names.iter().enumerate() {
                let val = if let Ok(Some(v)) = row.try_get::<Option<i64>, usize>(i) {
                    Value::I64(v)
                } else if let Ok(Some(v)) = row.try_get::<Option<String>, usize>(i) {
                    Value::String(v)
                } else if let Ok(Some(v)) = row.try_get::<Option<f64>, usize>(i) {
                    Value::F64(v)
                } else {
                    Value::Null
                };
                map.insert(name.clone(), val);
            }
            map
        })
        .collect()
}

/// 深度比较两个结果集是否一致
fn assert_results_eq(
    orm: &[HashMap<String, Value>],
    native: &[HashMap<String, Value>],
    label: &str,
) -> Result<(), TestCaseError> {
    prop_assert_eq!(
        orm.len(),
        native.len(),
        "[{}] 行数不一致: ORM={}, Native={}",
        label,
        orm.len(),
        native.len()
    );

    for (i, (o, n)) in orm.iter().zip(native.iter()).enumerate() {
        let keys: HashSet<&str> = o
            .keys()
            .map(|k| k.as_str())
            .chain(n.keys().map(|k| k.as_str()))
            .collect();
        for k in keys {
            prop_assert_eq!(
                o.get(k),
                n.get(k),
                "[{}] row={} col='{}' 不一致: ORM={:?}, Native={:?}",
                label,
                i,
                k,
                o.get(k),
                n.get(k)
            );
        }
    }
    Ok(())
}

/// 使用 sqlx 执行原生查询并转换为 HashMap 结果
async fn native_query(
    pool: &sqlx::SqlitePool,
    sql: &str,
) -> Result<Vec<HashMap<String, Value>>, DbError> {
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
        .fetch_all(pool)
        .await
        .map_err(|e| DbError::Internal(e.to_string()))?;
    Ok(native_rows_to_map(&rows))
}

// ===================== 核心测试逻辑 =====================

/// 执行所有 5 个差分测试场景
async fn run_scenarios(
    conn: &mut Box<dyn Connection>,
    native_pool: &sqlx::SqlitePool,
) -> Result<(), DbError> {
    // ---------- 场景 1: 简单 SELECT ALL ----------
    let orm_all = conn
        .query("SELECT id, name, age FROM diff_users ORDER BY id")
        .await?;
    let native_all = native_query(
        native_pool,
        "SELECT id, name, age FROM diff_users ORDER BY id",
    )
    .await?;
    assert_results_eq(&orm_all, &native_all, "SELECT ALL")
        .map_err(|e| DbError::Internal(format!("{}", e)))?;

    // ---------- 场景 2: 条件 WHERE ----------
    let ages: Vec<i64> = orm_all
        .iter()
        .filter_map(|r| match r.get("age") {
            Some(Value::I64(a)) => Some(*a),
            _ => None,
        })
        .collect();
    let threshold = if ages.is_empty() {
        50
    } else {
        ages[ages.len() / 2]
    };
    let sql_where = format!(
        "SELECT id, name, age FROM diff_users WHERE age >= {} ORDER BY id",
        threshold
    );
    let orm_where = conn.query(&sql_where).await?;
    let native_where = native_query(native_pool, &sql_where).await?;
    assert_results_eq(&orm_where, &native_where, "WHERE age")
        .map_err(|e| DbError::Internal(format!("{}", e)))?;

    // ---------- 场景 3: 限定列 ----------
    let orm_cols = conn
        .query("SELECT id, name FROM diff_users ORDER BY id")
        .await?;
    let native_cols =
        native_query(native_pool, "SELECT id, name FROM diff_users ORDER BY id").await?;
    assert_results_eq(&orm_cols, &native_cols, "SELECT id,name")
        .map_err(|e| DbError::Internal(format!("{}", e)))?;

    // ---------- 场景 4: ORDER BY + LIMIT ----------
    let limit = if orm_all.len() > 5 {
        5
    } else {
        orm_all.len().max(1)
    };
    let sql_limit = format!(
        "SELECT id, name, age FROM diff_users ORDER BY age DESC, id ASC LIMIT {}",
        limit
    );
    let orm_limit = conn.query(&sql_limit).await?;
    let native_limit = native_query(native_pool, &sql_limit).await?;
    assert_results_eq(&orm_limit, &native_limit, "ORDER BY + LIMIT")
        .map_err(|e| DbError::Internal(format!("{}", e)))?;

    // ---------- 场景 5: 多条件组合 ----------
    if ages.len() >= 2 {
        let low = ages[0].min(ages[ages.len() - 1]);
        let high = ages[0].max(ages[ages.len() - 1]);
        let sql_multi = format!(
            "SELECT id, name, age FROM diff_users WHERE age >= {} AND age <= {} ORDER BY age ASC, id ASC",
            low.min(high),
            low.max(high)
        );
        let orm_multi = conn.query(&sql_multi).await?;
        let native_multi = native_query(native_pool, &sql_multi).await?;
        assert_results_eq(&orm_multi, &native_multi, "MULTI WHERE")
            .map_err(|e| DbError::Internal(format!("{}", e)))?;
    }

    Ok(())
}

// ===================== proptest 测试 =====================

proptest! {
    // 主差分模糊测试：proptest 自动生成随机数据并运行 256 轮
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn differential_fuzz(users in random_users()) {
        let rt = tokio::runtime::Runtime::new()
            .expect("failed to create tokio runtime");
        rt.block_on(async {
            let (mut conn, native_pool) = setup_db(&users)
                .await
                .expect("test DB setup failed");

            run_scenarios(&mut conn, &native_pool)
                .await
                .expect("differential test scenarios failed");
        });
    }
}

// ===================== 确定性测试（非 proptest，用于调试） =====================

#[tokio::test]
async fn deterministic_small() {
    let users = vec![
        (1, "alice".to_string(), 25),
        (2, "bob".to_string(), 30),
        (3, "charlie".to_string(), 35),
    ];
    let (mut conn, native_pool) = setup_db(&users).await.unwrap();
    run_scenarios(&mut conn, &native_pool).await.unwrap();
}

#[tokio::test]
async fn deterministic_single_row() {
    let users = vec![(42, "solo".to_string(), 99)];
    let (mut conn, native_pool) = setup_db(&users).await.unwrap();
    run_scenarios(&mut conn, &native_pool).await.unwrap();
}

#[tokio::test]
async fn deterministic_large() {
    let users: Vec<(i64, String, i64)> = (0..100)
        .map(|i| (i + 1, format!("user_{}", i), i % 100))
        .collect();
    let (mut conn, native_pool) = setup_db(&users).await.unwrap();
    run_scenarios(&mut conn, &native_pool).await.unwrap();
}
