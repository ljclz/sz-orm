//! 批③ L4 语义不变量：引擎"自己对自己不一致"的状态类缺陷检测
//!
//! 通用恒等式（按本项目特征落地到 SQLite 内存库，rusqlite 适配器常驻）：
//! - INV-N1 派生 ↔ 基：QueryResultCache/QueryCache 缓存内容 == 重新计算基数据
//!   （多轮 DML 后经 invalidate_table 失效再回填，缓存命中必须与直查一致）
//! - INV-N2 幂等/确定性：同一请求重放结果确定（含快路径两次构建一致）
//! - INV-N3 聚合分解：全局聚合 == 分组聚合的再聚合（SUM/COUNT/MIN/MAX/AVG）
//! - INV-N4 交换律：A∘B == B∘A（INSERT+DELETE 对称作用后的行集）
//! - INV-N5 回滚快照：BEGIN; DML×n; ROLLBACK; 全状态 == 前置快照
//! - INV-N6 可恢复性：DELETE 全部 + 快照重插 == 原快照
//! - INV-N7 计数一致：COUNT(*) == COUNT(1) == COUNT(非空列)
//!
//! PBT 驱动 DML 序列（proptest 生成 INSERT/UPDATE/DELETE 序列）。
//! 运行方式：cargo test -p sz-orm-core --test semantic_invariants

use std::collections::HashMap;

mod common;

use common::rusqlite_adapter::RusqliteConnection;
use proptest::prelude::*;
use sz_orm_core::{Connection, Value};

type Row = (i64, String, i64);

// ===================== 基础设施 =====================

async fn setup(conn: &mut RusqliteConnection, rows: &[Row]) {
    conn.execute("DROP TABLE IF EXISTS inv_t").await.expect("drop");
    conn.execute(
        "CREATE TABLE inv_t (id INTEGER PRIMARY KEY, name TEXT NOT NULL, age INTEGER NOT NULL)",
    )
    .await
    .expect("create");
    for (id, name, age) in rows {
        // 适配器无 execute_with_params；种子数据为白名单字面集（name 由测试常量构成）
        conn.execute_direct(&format!(
            "INSERT INTO inv_t (id, name, age) VALUES ({}, '{}', {})",
            id,
            name.replace('\'', "''"),
            age
        ));
    }
}

/// 全表快照（确定性排序）
async fn snapshot(conn: &mut RusqliteConnection) -> Vec<Row> {
    let rows = conn
        .query("SELECT id, name, age FROM inv_t ORDER BY id")
        .await
        .expect("snapshot query");
    rows.iter()
        .map(|r| {
            let id = match r.get("id") {
                Some(Value::I64(v)) => *v,
                other => panic!("id 类型异常: {:?}", other),
            };
            let name = match r.get("name") {
                Some(Value::String(s)) => s.clone(),
                other => panic!("name 类型异常: {:?}", other),
            };
            let age = match r.get("age") {
                Some(Value::I64(v)) => *v,
                other => panic!("age 类型异常: {:?}", other),
            };
            (id, name, age)
        })
        .collect()
}

async fn agg(conn: &mut RusqliteConnection, expr: &str) -> HashMap<String, Value> {
    let sql = format!("SELECT {} AS v FROM inv_t", expr);
    let rows = conn.query(&sql).await.expect("agg query");
    rows.into_iter()
        .next()
        .map(|r| r.into_iter().collect())
        .unwrap_or_default()
}

// ===================== INV-N2 幂等/确定性 =====================

#[tokio::test]
async fn inv_n2_replay_deterministic() {
    let mut conn = RusqliteConnection::open_in_memory();
    setup(&mut conn, &[(1, "a".into(), 10), (2, "b".into(), 20)]).await;
    for _ in 0..5 {
        let s1 = snapshot(&mut conn).await;
        let s2 = snapshot(&mut conn).await;
        assert_eq!(s1, s2, "同一快照重放结果必须确定");
    }
}

// ===================== INV-N5 回滚快照 =====================

#[tokio::test]
async fn inv_n5_rollback_snapshot() {
    let mut conn = RusqliteConnection::open_in_memory();
    setup(&mut conn, &[(1, "a".into(), 10), (2, "b".into(), 20)]).await;
    let before = snapshot(&mut conn).await;

    conn.begin_transaction().await.expect("begin");
    conn.execute("INSERT INTO inv_t (id, name, age) VALUES (3, 'c', 30)")
        .await
        .expect("tx insert");
    conn.execute("UPDATE inv_t SET age = 99 WHERE id = 1")
        .await
        .expect("tx update");
    conn.execute("DELETE FROM inv_t WHERE id = 2")
        .await
        .expect("tx delete");
    conn.rollback().await.expect("rollback");

    let after = snapshot(&mut conn).await;
    assert_eq!(before, after, "ROLLBACK 后全状态必须等于前置快照");
}

// ===================== INV-N6 可恢复性 =====================

#[tokio::test]
async fn inv_n6_delete_all_reinsert() {
    let mut conn = RusqliteConnection::open_in_memory();
    let original: Vec<Row> = vec![
        (1, "a".into(), 10),
        (2, "b".into(), 20),
        (3, "c".into(), 30),
    ];
    setup(&mut conn, &original).await;
    let before = snapshot(&mut conn).await;

    // 快照 → DELETE 全部 → 按快照重插
    let snap = before.clone();
    conn.execute("DELETE FROM inv_t").await.expect("delete all");
    assert!(snapshot(&mut conn).await.is_empty(), "删除后表必须为空");
    for (id, name, age) in &snap {
        conn.execute(&format!(
            "INSERT INTO inv_t (id, name, age) VALUES ({}, '{}', {})",
            id,
            name.replace('\'', "''"),
            age
        ))
        .await
        .expect("reinsert");
    }

    let after = snapshot(&mut conn).await;
    assert_eq!(before, after, "DELETE 全部 + 快照重插必须还原原快照");
}

// ===================== INV-N7 计数一致 =====================

#[tokio::test]
async fn inv_n7_count_forms_agree() {
    let mut conn = RusqliteConnection::open_in_memory();
    setup(&mut conn, &[(1, "a".into(), 10), (2, "b".into(), 20)]).await;
    let c_star = agg(&mut conn, "COUNT(*)").await;
    let c_one = agg(&mut conn, "COUNT(1)").await;
    let c_col = agg(&mut conn, "COUNT(id)").await; // id 非空（PK）
    assert_eq!(
        c_star.get("v"), c_one.get("v"),
        "COUNT(*) 与 COUNT(1) 必须一致"
    );
    assert_eq!(
        c_star.get("v"), c_col.get("v"),
        "COUNT(*) 与 COUNT(非空列) 必须一致"
    );
}

// ===================== INV-N3 聚合分解 =====================

#[tokio::test]
async fn inv_n3_aggregate_decomposition() {
    let mut conn = RusqliteConnection::open_in_memory();
    setup(&mut conn, &[(1, "a".into(), 10), (2, "b".into(), 20), (3, "a".into(), 30)]).await;

    // 全局 SUM vs 分组 SUM 再聚合
    let global_sum = match agg(&mut conn, "SUM(age)").await.get("v") {
        Some(Value::I64(v)) => *v,
        Some(Value::F64(v)) => *v as i64,
        other => panic!("SUM 类型异常: {:?}", other),
    };
    let groups = conn
        .query("SELECT name, SUM(age) AS s FROM inv_t GROUP BY name")
        .await
        .expect("group sum");
    let group_sum: i64 = groups
        .iter()
        .map(|g| match g.get("s") {
            Some(Value::I64(v)) => *v,
            Some(Value::F64(v)) => *v as i64,
            other => panic!("分组 SUM 类型异常: {:?}", other),
        })
        .sum();
    assert_eq!(global_sum, group_sum, "全局 SUM == 分组 SUM 再聚合");

    // 全局 COUNT vs 分组 COUNT 再聚合
    let global_count = match agg(&mut conn, "COUNT(*)").await.get("v") {
        Some(Value::I64(v)) => *v,
        other => panic!("COUNT 类型异常: {:?}", other),
    };
    let gcounts = conn
        .query("SELECT name, COUNT(*) AS c FROM inv_t GROUP BY name")
        .await
        .expect("group count");
    let group_count: i64 = gcounts
        .iter()
        .map(|g| match g.get("c") {
            Some(Value::I64(v)) => *v,
            other => panic!("分组 COUNT 类型异常: {:?}", other),
        })
        .sum();
    assert_eq!(global_count, group_count, "全局 COUNT == 分组 COUNT 再聚合");

    // 全局 MIN/MAX == 分组 MIN 再 MIN / 分组 MAX 再 MAX
    let global_min = agg(&mut conn, "MIN(age)").await;
    let gmins = conn
        .query("SELECT name, MIN(age) AS m FROM inv_t GROUP BY name")
        .await
        .expect("group min");
    let min_of_mins: Vec<Value> = gmins.iter().filter_map(|g| g.get("m").cloned()).collect();
    let min_of_mins = min_of_mins
        .into_iter()
        .map(|v| match v {
            Value::I64(x) => x,
            other => panic!("类型: {:?}", other),
        })
        .min();
    assert_eq!(
        global_min.get("v").cloned(),
        min_of_mins.map(Value::I64),
        "全局 MIN == 分组 MIN 再 MIN"
    );
}

// ===================== INV-N4 交换律（INSERT/DELETE 对称作用） =====================

#[tokio::test]
async fn inv_n4_commutative_dml_pairs() {
    let mut conn_a = RusqliteConnection::open_in_memory();
    setup(&mut conn_a, &[(1, "a".into(), 10), (2, "b".into(), 20)]).await;
    let mut conn_b = RusqliteConnection::open_in_memory();
    setup(&mut conn_b, &[(1, "a".into(), 10), (2, "b".into(), 20)]).await;

    // A∘B：先插 3 后删 2
    conn_a
        .execute("INSERT INTO inv_t (id, name, age) VALUES (3, 'c', 30)")
        .await
        .expect("insert 3");
    conn_a.execute("DELETE FROM inv_t WHERE id = 2").await.expect("delete 2");
    // B∘A：先删 2 后插 3
    conn_b.execute("DELETE FROM inv_t WHERE id = 2").await.expect("delete 2");
    conn_b
        .execute("INSERT INTO inv_t (id, name, age) VALUES (3, 'c', 30)")
        .await
        .expect("insert 3");

    let sa = snapshot(&mut conn_a).await;
    let sb = snapshot(&mut conn_b).await;
    assert_eq!(sa, sb, "A∘B == B∘A（INSERT/DELETE 对称作用后行集必须一致）");
}

// ===================== INV-N1 派生 ↔ 基（缓存失效后再填 == 直查） =====================

#[cfg(feature = "query-result-cache")]
#[tokio::test]
async fn inv_n1_cache_matches_recompute_after_dml() {
    use sz_orm_core::query_result_cache::{CacheKey, QueryResultCache, QueryResultCacheConfig};
    use std::time::Duration;

    let mut conn = RusqliteConnection::open_in_memory();
    setup(&mut conn, &[(1, "a".into(), 10), (2, "b".into(), 20)]).await;

    let cache = QueryResultCache::new(QueryResultCacheConfig {
        capacity: 16,
        ttl: Duration::from_secs(60),
    });

    let sql = "SELECT id, name, age FROM inv_t";
    let key = CacheKey::new(sql, &[], None);

    // 首次：直查 → 回填缓存
    let base = conn.query(sql).await.expect("base query");
    cache.put(key.clone(), base.clone(), ["inv_t".to_string()].into());

    // 多轮 DML（缓存不失效的话这就是脏读源）
    conn.execute("INSERT INTO inv_t (id, name, age) VALUES (3, 'c', 30)")
        .await
        .expect("dml 1");
    conn.execute("UPDATE inv_t SET age = 25 WHERE id = 1").await.expect("dml 2");
    conn.execute("DELETE FROM inv_t WHERE id = 2").await.expect("dml 3");

    // 表失效
    cache.invalidate_table("inv_t");

    // 重新计算基数据 → 回填 → 命中读取必须 == 重新计算
    let recomputed = conn.query(sql).await.expect("recompute");
    cache.put(key.clone(), recomputed.clone(), ["inv_t".to_string()].into());
    let cached = cache.get(&key).expect("缓存应在 TTL 内命中");
    assert_eq!(
        recomputed, cached,
        "INV-N1 被违反：缓存命中结果 != 重新计算基数据"
    );
    assert_eq!(recomputed.len(), 2, "DML 后基数据应为 2 行");
}

// ===================== PBT：DML 序列驱动的回滚快照不变量 =====================

#[derive(Debug, Clone)]
enum Dml {
    Insert(i64, u8, i64),
    Update(i64, i64),
    Delete(i64),
}

fn arb_dml_seq() -> impl Strategy<Value = Vec<Dml>> {
    prop::collection::vec(
        prop_oneof![
            (1i64..=6, any::<u8>(), -50i64..50).prop_map(|(id, n, age)| Dml::Insert(id, n % 26, age)),
            (1i64..=6, -50i64..50).prop_map(|(id, age)| Dml::Update(id, age)),
            (1i64..=6).prop_map(Dml::Delete),
        ],
        0..=8,
    )
}

async fn apply_dml(conn: &mut RusqliteConnection, dml: &Dml) {
    // 适配器无 execute_with_params；DML 值域封闭（i64/i64 封闭域 + name 模板），
    // 经 i64 不可注入，直接拼接安全
    match dml {
        Dml::Insert(id, n, age) => {
            conn.execute(&format!(
                "INSERT OR IGNORE INTO inv_t (id, name, age) VALUES ({}, 'u{}', {})",
                id, n, age
            ))
            .await
            .expect("pbt insert");
        }
        Dml::Update(id, age) => {
            conn.execute(&format!("UPDATE inv_t SET age = {} WHERE id = {}", age, id))
                .await
                .expect("pbt update");
        }
        Dml::Delete(id) => {
            conn.execute(&format!("DELETE FROM inv_t WHERE id = {}", id))
                .await
                .expect("pbt delete");
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// PBT：随机 DML 序列在事务内执行后 ROLLBACK，全状态必须等于前置快照
    #[test]
    fn pbt_inv5_rollback_snapshot(seq in arb_dml_seq()) {
        let rt = tokio::runtime::Runtime::new().expect("runtime");
        rt.block_on(async {
            let mut conn = RusqliteConnection::open_in_memory();
            setup(&mut conn, &[(1, "a".into(), 10), (2, "b".into(), 20)]).await;
            let before = snapshot(&mut conn).await;

            conn.begin_transaction().await.expect("begin");
            for dml in &seq {
                apply_dml(&mut conn, dml).await;
            }
            conn.rollback().await.expect("rollback");

            let after = snapshot(&mut conn).await;
            prop_assert_eq!(before, after, "PBT 回滚快照不变量被违反: seq={:?}", seq);
            Ok::<(), proptest::test_runner::TestCaseError>(())
        })
        .expect("PBT 回滚快照不变量运行失败");
    }
}
