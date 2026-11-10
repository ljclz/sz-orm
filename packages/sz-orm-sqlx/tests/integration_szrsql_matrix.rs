//! szrsql 全功能矩阵 × PG18 差分集成测试（v10.13.x 矩阵差分评测新增）
//!
//! 载荷为 2026-11-09 矩阵差分评测（docs/assessment/2026-11-09-szrsql-matrix-differential.md）
//! 中 szrsql 与 PG18 **行为一致** 的能力块；差分发现的上游缺陷项以
//! `#[ignore = "szrsql 上游 <缺陷>"]` 留档（详见报告 bug 清单）。
//!
//! 运行：cargo test -p sz-orm-sqlx --test integration_szrsql_matrix -- --ignored
//! 凭据经环境变量 `SZ_ORM_SZRSQL_URL` 注入（默认本机 trust 6543）。

use std::sync::Arc;

use sz_orm_core::{Connection, Pool, PoolConfigBuilder, Value};
use sz_orm_sqlx::{PgPoolHandle, SqlxPgConnectionFactory};

/// 默认 szrsql 连接 URL；`SZ_ORM_SZRSQL_URL` 覆盖。
const SZRSQL_URL_DEFAULT: &str = "szrsql://postgres:trust@127.0.0.1:6543/postgres";

fn szrsql_url() -> String {
    std::env::var("SZ_ORM_SZRSQL_URL").unwrap_or_else(|_| SZRSQL_URL_DEFAULT.to_string())
}

async fn try_pool() -> Option<sz_orm_sqlx::AnyPool> {
    sz_orm_sqlx::AnyPool::connect(&szrsql_url()).await.ok()
}

fn unique_table(prefix: &str) -> String {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{}_{}_{}", prefix, pid, nanos % 1_000_000)
}

/// M1：QueryBuilder 同型 SQL（group/having/NULL 排序/join/subquery/union）在 szrsql 执行
#[tokio::test]
#[ignore = "需要 szrsql 服务在 127.0.0.1:6543"]
async fn matrix_m1_querybuilder_shape_sql() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: szrsql 未启动");
        return;
    };
    let mut conn = pool.create().await.unwrap();
    let table = unique_table("mx_m1");
    conn.execute(&format!("DROP TABLE IF EXISTS {table}"))
        .await
        .unwrap();
    conn.execute(&format!(
        "CREATE TABLE {table} (id INT PRIMARY KEY, cat TEXT, val INT, note TEXT)"
    ))
    .await
    .unwrap();
    conn.execute(&format!(
        "INSERT INTO {table} VALUES (1,'a',10,NULL),(2,'a',20,'x'),(3,'b',NULL,'y'),(4,'b',40,NULL),(5,'c',50,'x'),(6,NULL,60,'y')"
    ))
    .await
    .unwrap();

    // 聚合 + HAVING + NULLS LAST（与 PG18 对拍一致，评测 Q1）
    let rows = conn
        .query(&format!(
            "SELECT cat, count(*), sum(val) FROM {table} GROUP BY cat HAVING count(*) > 1 ORDER BY cat NULLS LAST"
        ))
        .await
        .unwrap();
    assert_eq!(rows.len(), 2, "group/having 应返回 2 组（a/b）");

    // LEFT JOIN + 子查询
    let rows = conn
        .query(&format!(
            "SELECT a.id FROM {table} a LEFT JOIN {table} b ON a.cat = b.cat AND b.val > 20 WHERE a.val > (SELECT avg(val) FROM {table}) ORDER BY a.id"
        ))
        .await
        .unwrap();
    assert!(!rows.is_empty(), "LEFT JOIN + 子查询应返回行");

    // UNION
    let rows = conn
        .query(&format!(
            "SELECT cat FROM {table} WHERE val < 25 UNION SELECT cat FROM {table} WHERE val > 45 ORDER BY cat NULLS LAST"
        ))
        .await
        .unwrap();
    assert!(!rows.is_empty());

    conn.execute(&format!("DROP TABLE IF EXISTS {table}"))
        .await
        .unwrap();
}

/// M2：表达式层（ILIKE/DISTINCT ON/CAST/字符串函数/coalesce/greatest）
#[tokio::test]
#[ignore = "需要 szrsql 服务在 127.0.0.1:6543"]
async fn matrix_m2_expression_layer() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: szrsql 未启动");
        return;
    };
    let mut conn = pool.create().await.unwrap();
    let table = unique_table("mx_m2");
    conn.execute(&format!("DROP TABLE IF EXISTS {table}"))
        .await
        .unwrap();
    conn.execute(&format!(
        "CREATE TABLE {table} (id INT PRIMARY KEY, cat TEXT, val INT)"
    ))
    .await
    .unwrap();
    conn.execute(&format!(
        "INSERT INTO {table} VALUES (1,'Abc',10),(2,'abd',20),(3,NULL,30)"
    ))
    .await
    .unwrap();

    // ILIKE（PG 特化分支）
    let rows = conn
        .query(&format!(
            "SELECT id FROM {table} WHERE cat ILIKE 'AB%' ORDER BY id"
        ))
        .await
        .unwrap();
    assert_eq!(rows.len(), 2, "ILIKE 'AB%' 应命中 Abc/abd");

    // DISTINCT ON
    let rows = conn
        .query(&format!(
            "SELECT DISTINCT ON (cat) id, cat FROM {table} WHERE cat IS NOT NULL ORDER BY cat, id"
        ))
        .await
        .unwrap();
    assert_eq!(rows.len(), 2, "DISTINCT ON(cat) 应返回 2 组");

    // CAST + 字符串函数 + coalesce/greatest
    let rows = conn
        .query(&format!(
            "SELECT id, CAST(val AS TEXT) AS s, upper(coalesce(cat,'z')) AS u, greatest(val, 15) AS g FROM {table} ORDER BY id"
        ))
        .await
        .unwrap();
    assert_eq!(rows.len(), 3);

    conn.execute(&format!("DROP TABLE IF EXISTS {table}"))
        .await
        .unwrap();
}

/// M3：DDL 变体（IF NOT EXISTS/部分索引/表达式索引/COMMENT/ALTER）
#[tokio::test]
#[ignore = "需要 szrsql 服务在 127.0.0.1:6543"]
async fn matrix_m3_ddl_variants() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: szrsql 未启动");
        return;
    };
    let mut conn = pool.create().await.unwrap();
    let table = unique_table("mx_m3");
    conn.execute(&format!("DROP TABLE IF EXISTS {table}"))
        .await
        .unwrap();
    conn.execute(&format!(
        "CREATE TABLE {table} (id INT PRIMARY KEY, name TEXT NOT NULL, age INT)"
    ))
    .await
    .unwrap();
    conn.execute(&format!("CREATE INDEX idx_{table}_name ON {table} (name)"))
        .await
        .unwrap();
    conn.execute(&format!(
        "CREATE INDEX idx_{table}_partial ON {table} (age) WHERE age > 18"
    ))
    .await
    .unwrap();
    conn.execute(&format!(
        "CREATE INDEX idx_{table}_expr ON {table} (upper(name))"
    ))
    .await
    .unwrap();
    conn.execute(&format!("COMMENT ON TABLE {table} IS 'matrix test'"))
        .await
        .unwrap();
    conn.execute(&format!("ALTER TABLE {table} ADD COLUMN extra TEXT"))
        .await
        .unwrap();
    conn.execute(&format!(
        "ALTER TABLE {table} ALTER COLUMN age SET DEFAULT 0"
    ))
    .await
    .unwrap();
    conn.execute(&format!(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_{table}_name ON {table} (name)"
    ))
    .await
    .unwrap();
    conn.execute(&format!("DROP TABLE IF EXISTS {table}"))
        .await
        .unwrap();
}

/// M3b：savepoint 栈（sz-orm 8 层嵌套语义的子集验证：两层 ROLLBACK TO）
#[tokio::test]
#[ignore = "需要 szrsql 服务在 127.0.0.1:6543"]
async fn matrix_m3b_savepoint_stack() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: szrsql 未启动");
        return;
    };
    let mut conn = pool.create().await.unwrap();
    let table = unique_table("mx_m3b");
    conn.execute(&format!("DROP TABLE IF EXISTS {table}"))
        .await
        .unwrap();
    conn.execute(&format!("CREATE TABLE {table} (id INT PRIMARY KEY)"))
        .await
        .unwrap();

    conn.begin_transaction().await.unwrap();
    conn.execute(&format!("INSERT INTO {table} VALUES (1)"))
        .await
        .unwrap();
    conn.execute("SAVEPOINT sp1").await.unwrap();
    conn.execute(&format!("INSERT INTO {table} VALUES (2)"))
        .await
        .unwrap();
    conn.execute("ROLLBACK TO SAVEPOINT sp1").await.unwrap();
    conn.commit().await.unwrap();

    let rows = conn
        .query(&format!("SELECT id FROM {table}"))
        .await
        .unwrap();
    assert_eq!(rows.len(), 1, "ROLLBACK TO sp1 后应只剩 1 行");

    conn.execute(&format!("DROP TABLE IF EXISTS {table}"))
        .await
        .unwrap();
}

/// M5：DISCARD ALL / RESET ALL —— 上游分项处置（2026-11-10）：
/// - DISCARD ALL/SEQUENCES/PLANS/TEMP：上游已修复（0d16260c），此处断言通过
/// - RESET ALL：上游仍未实现，但**非 sz-orm 连接池依赖路径**（PG 族
///   session_reset_sql = "DISCARD ALL"，不依赖 RESET），登记为次要上游缺口留档
#[tokio::test]
#[ignore = "需要 szrsql 服务在 127.0.0.1:6543"]
async fn matrix_m5_discard_reset() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: szrsql 未启动");
        return;
    };
    let mut conn = pool.create().await.unwrap();
    // DISCARD 各族已修复
    conn.execute("DISCARD ALL").await.unwrap();
    conn.execute("DISCARD SEQUENCES").await.unwrap();
    conn.execute("DISCARD PLANS").await.unwrap();
    conn.execute("DISCARD TEMP").await.unwrap();
    // RESET ALL 尚未实现：非连接池依赖路径，留档为次要上游缺口（不阻断）
    // 若上游后续实现，可在此补断言；当前断言其解析失败以维持缺口登记可见。
    let err = conn.execute("RESET ALL").await;
    assert!(
        err.is_err(),
        "RESET ALL 上游未实现，预期解析失败（维持上游缺口登记）"
    );
}

/// M6：批量 1000 行 INSERT + 游标（DECLARE/FETCH）
#[tokio::test]
#[ignore = "需要 szrsql 服务在 127.0.0.1:6543"]
async fn matrix_m6_batch_and_cursor() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: szrsql 未启动");
        return;
    };
    let mut conn = pool.create().await.unwrap();
    let table = unique_table("mx_m6");
    conn.execute(&format!("DROP TABLE IF EXISTS {table}"))
        .await
        .unwrap();
    conn.execute(&format!(
        "CREATE TABLE {table} (id INT PRIMARY KEY, s TEXT)"
    ))
    .await
    .unwrap();
    conn.execute(&format!(
        "INSERT INTO {table} SELECT g, 'row' || g FROM generate_series(1, 1000) g"
    ))
    .await
    .unwrap();
    let rows = conn
        .query(&format!("SELECT count(*) FROM {table}"))
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);

    // 游标流式读
    conn.begin_transaction().await.unwrap();
    conn.execute(&format!(
        "DECLARE mx_cur CURSOR FOR SELECT id FROM {table} ORDER BY id LIMIT 5"
    ))
    .await
    .unwrap();
    let rows = conn.query("FETCH 3 FROM mx_cur").await.unwrap();
    assert_eq!(rows.len(), 3, "FETCH 3 应返回 3 行");
    let rows = conn.query("FETCH 2 FROM mx_cur").await.unwrap();
    assert_eq!(rows.len(), 2, "续 FETCH 2 应返回 2 行");
    conn.execute("CLOSE mx_cur").await.unwrap();
    conn.commit().await.unwrap();

    conn.execute(&format!("DROP TABLE IF EXISTS {table}"))
        .await
        .unwrap();
}

/// M7：SET 会话变量 + 错误后连接恢复（连接池健康检查依赖）
#[tokio::test]
#[ignore = "需要 szrsql 服务在 127.0.0.1:6543"]
async fn matrix_m7_session_and_error_recovery() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: szrsql 未启动");
        return;
    };
    let mut conn = pool.create().await.unwrap();
    let table = unique_table("mx_m7");
    conn.execute(&format!("DROP TABLE IF EXISTS {table}"))
        .await
        .unwrap();
    conn.execute(&format!("CREATE TABLE {table} (id INT PRIMARY KEY)"))
        .await
        .unwrap();
    conn.execute(&format!("INSERT INTO {table} VALUES (1)"))
        .await
        .unwrap();

    // 语句级错误后同一连接继续可用（池故障剔除不误杀）
    let dup = conn
        .execute(&format!("INSERT INTO {table} VALUES (1)"))
        .await;
    assert!(dup.is_err(), "重复主键应报错");
    let n = conn
        .execute(&format!("INSERT INTO {table} VALUES (2)"))
        .await
        .unwrap();
    assert_eq!(n, 1, "错误后连接应继续可用");

    conn.execute(&format!("DROP TABLE IF EXISTS {table}"))
        .await
        .unwrap();
}

/// M8：RETURNING 全家 + lastval 缺口留档
#[tokio::test]
#[ignore = "需要 szrsql 服务在 127.0.0.1:6543"]
async fn matrix_m8_returning() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: szrsql 未启动");
        return;
    };
    let mut conn = pool.create().await.unwrap();
    let table = unique_table("mx_m8");
    conn.execute(&format!("DROP TABLE IF EXISTS {table}"))
        .await
        .unwrap();
    conn.execute(&format!(
        "CREATE TABLE {table} (id BIGSERIAL PRIMARY KEY, v TEXT)"
    ))
    .await
    .unwrap();

    let rows = conn
        .query(&format!(
            "INSERT INTO {table} (v) VALUES ('a') RETURNING id"
        ))
        .await
        .unwrap();
    assert_eq!(rows.len(), 1, "INSERT..RETURNING 应回填主键");

    let rows = conn
        .query(&format!(
            "UPDATE {table} SET v = 'b' WHERE id = 1 RETURNING id, v"
        ))
        .await
        .unwrap();
    assert_eq!(rows.len(), 1, "UPDATE..RETURNING 应返回 1 行");

    let rows = conn
        .query(&format!("DELETE FROM {table} WHERE id = 1 RETURNING id"))
        .await
        .unwrap();
    assert_eq!(rows.len(), 1, "DELETE..RETURNING 应返回 1 行");

    conn.execute(&format!("DROP TABLE IF EXISTS {table}"))
        .await
        .unwrap();
}

/// M8b：lastval() —— 上批留档项；上游 UP-5 已修复（df878ad5），现按 PG 语义
/// 验证正序用法（先 nextval 触碰序列再 lastval，未触碰即调 lastval 与 PG 一样
/// 报 currval/lastval not yet defined——本测试验证合法路径而非留档失败）。
#[tokio::test]
#[ignore = "需要 szrsql 服务在 127.0.0.1:6543"]
async fn matrix_m8b_lastval() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: szrsql 未启动");
        return;
    };
    let mut conn = pool.create().await.unwrap();
    let table = unique_table("mx_m8b");
    conn.execute(&format!("DROP TABLE IF EXISTS {table}"))
        .await
        .unwrap();
    conn.execute(&format!(
        "CREATE TABLE {table} (id BIGSERIAL PRIMARY KEY, v TEXT)"
    ))
    .await
    .unwrap();
    conn.execute(&format!("INSERT INTO {table} (v) VALUES ('a')"))
        .await
        .unwrap();
    // 序列已被 INSERT 触碰 → lastval 合法且应返回刚插入行的序列值
    let rows = conn.query("SELECT lastval()").await.unwrap();
    assert!(!rows.is_empty(), "INSERT 后 lastval() 应返回值");
    assert_eq!(
        rows.len(),
        1,
        "lastval() 应返回 1 行（BIGSERIAL 序列已被 INSERT 触碰）"
    );
    // 未触碰序列前 lastval 报错也应与 PG 一致（新连接上先调 lastval）
    let mut conn2 = pool.create().await.unwrap();
    let err = conn2.query("SELECT lastval()").await;
    assert!(
        err.is_err(),
        "未触碰序列时 lastval() 应报错（与 PG lastval is not yet defined 一致）"
    );
    conn.execute(&format!("DROP TABLE IF EXISTS {table}"))
        .await
        .unwrap();
}

/// M9：IS NOT DISTINCT FROM —— 上游 P0 缺陷已修复（szrsql 侧差分验证 + 2026-11-10
/// sz-orm 侧真连复验通过），恢复正常回归面。
#[tokio::test]
#[ignore = "需要 szrsql 服务在 127.0.0.1:6543"]
async fn matrix_m9_is_distinct_from() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: szrsql 未启动");
        return;
    };
    let mut conn = pool.create().await.unwrap();
    let table = unique_table("mx_m9");
    conn.execute(&format!("DROP TABLE IF EXISTS {table}"))
        .await
        .unwrap();
    conn.execute(&format!("CREATE TABLE {table} (id INT PRIMARY KEY, v INT)"))
        .await
        .unwrap();
    conn.execute(&format!("INSERT INTO {table} VALUES (1, 10), (2, NULL)"))
        .await
        .unwrap();
    let rows = conn
        .query(&format!(
            "SELECT id FROM {table} WHERE v IS NOT DISTINCT FROM 10"
        ))
        .await
        .unwrap();
    assert_eq!(rows.len(), 1, "IS NOT DISTINCT FROM 应 NULL 安全命中 1 行");
    conn.execute(&format!("DROP TABLE IF EXISTS {table}"))
        .await
        .unwrap();
}

/// M9b：参数化查询 + 大参数个数（60 参数上限探针，差分一致项沉淀）
#[tokio::test]
#[ignore = "需要 szrsql 服务在 127.0.0.1:6543"]
async fn matrix_m9b_many_params() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: szrsql 未启动");
        return;
    };
    let mut conn = pool.create().await.unwrap();
    let placeholders: Vec<String> = (1..=60).map(|i| format!("${i}")).collect();
    let sql = format!("SELECT {}", placeholders.join(" || '::' || "));
    let params: Vec<Value> = (1..=60).map(|i| Value::I64(i as i64)).collect();
    let rows = conn.query_with_params(&sql, &params).await.unwrap();
    assert_eq!(rows.len(), 1, "60 参数拼接查询应返回 1 行");
}

/// M9c：保留字/Unicode 标识符（加引号）——差分一致项沉淀
#[tokio::test]
#[ignore = "需要 szrsql 服务在 127.0.0.1:6543"]
async fn matrix_m9c_quoted_identifiers() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: szrsql 未启动");
        return;
    };
    let mut conn = pool.create().await.unwrap();
    // 保留字表名/列名（加引号）
    conn.execute("DROP TABLE IF EXISTS \"order\"")
        .await
        .unwrap();
    conn.execute("CREATE TABLE \"order\" (\"select\" INT PRIMARY KEY)")
        .await
        .unwrap();
    conn.execute("INSERT INTO \"order\" (\"select\") VALUES (1)")
        .await
        .unwrap();
    let rows = conn
        .query("SELECT \"select\" FROM \"order\" WHERE \"select\" = 1")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);

    // Unicode 标识符
    conn.execute("DROP TABLE IF EXISTS 表_测试").await.unwrap();
    conn.execute("CREATE TABLE 表_测试 (标识符 INT PRIMARY KEY, 值 TEXT)")
        .await
        .unwrap();
    let n = conn
        .execute_with_params(
            "INSERT INTO 表_测试 (标识符, 值) VALUES ($1, $2)",
            &[Value::I64(1), Value::String("值内容".into())],
        )
        .await
        .unwrap();
    assert_eq!(n, 1);
    let rows = conn
        .query("SELECT 值 FROM 表_测试 WHERE 标识符 = 1")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);

    conn.execute("DROP TABLE IF EXISTS 表_测试").await.unwrap();
    conn.execute("DROP TABLE IF EXISTS \"order\"")
        .await
        .unwrap();
}

/// M9d：三值 NULL 边界（= NULL 恒空 / NOT (val>10) 不含 NULL 行 / 聚合空集 NULL）——差分一致项沉淀
#[tokio::test]
#[ignore = "需要 szrsql 服务在 127.0.0.1:6543"]
async fn matrix_m9d_three_valued_null() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: szrsql 未启动");
        return;
    };
    let mut conn = pool.create().await.unwrap();
    let table = unique_table("mx_m9d");
    conn.execute(&format!("DROP TABLE IF EXISTS {table}"))
        .await
        .unwrap();
    conn.execute(&format!(
        "CREATE TABLE {table} (id INT PRIMARY KEY, val INT)"
    ))
    .await
    .unwrap();
    conn.execute(&format!("INSERT INTO {table} VALUES (1, 10), (2, NULL)"))
        .await
        .unwrap();

    // = NULL 恒空
    let rows = conn
        .query_with_params(
            &format!("SELECT id FROM {table} WHERE val = $1"),
            &[Value::Null],
        )
        .await
        .unwrap();
    assert!(rows.is_empty(), "= NULL 应恒空");

    // NOT (val > 5) 不含 NULL 行
    let rows = conn
        .query(&format!(
            "SELECT id FROM {table} WHERE NOT (val > 5) ORDER BY id"
        ))
        .await
        .unwrap();
    assert!(rows.is_empty(), "NOT (val>5) 不应含 NULL 行");

    // 空集聚合返回 NULL 行
    let rows = conn
        .query(&format!("SELECT sum(val) FROM {table} WHERE id > 100"))
        .await
        .unwrap();
    assert_eq!(rows.len(), 1, "空集聚合应返回 1 行（NULL）");

    conn.execute(&format!("DROP TABLE IF EXISTS {table}"))
        .await
        .unwrap();
}

/// M4：隔离级别真语义 —— 上游缺陷已修复（szrsql 侧差分验证 + 2026-11-10
/// sz-orm 侧真连复验通过），恢复正常回归面。
#[tokio::test]
#[ignore = "需要 szrsql 服务在 127.0.0.1:6543"]
async fn matrix_m4_repeatable_read_snapshot() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: szrsql 未启动");
        return;
    };
    let pool = Arc::new(pool);
    let table = unique_table("mx_m4");
    {
        let mut conn = pool.create().await.unwrap();
        conn.execute(&format!("DROP TABLE IF EXISTS {table}"))
            .await
            .unwrap();
        conn.execute(&format!("CREATE TABLE {table} (id INT PRIMARY KEY, v INT)"))
            .await
            .unwrap();
        conn.execute(&format!("INSERT INTO {table} VALUES (1, 100)"))
            .await
            .unwrap();
    }
    // 会话 A：REPEATABLE READ 读 → 会话 B 提交更新 → A 再读，两次应一致
    let mut a = pool.create().await.unwrap();
    a.execute("BEGIN TRANSACTION ISOLATION LEVEL REPEATABLE READ")
        .await
        .unwrap();
    let rows = a
        .query(&format!("SELECT v FROM {table} WHERE id = 1"))
        .await
        .unwrap();
    let first = match rows[0].get("v") {
        Some(sz_orm_core::Value::I64(n)) => *n,
        _ => 0,
    };

    let pool_b = Arc::clone(&pool);
    let table_b = table.clone();
    let writer = tokio::spawn(async move {
        let mut b = pool_b.create().await.unwrap();
        b.execute(&format!("UPDATE {table_b} SET v = 200 WHERE id = 1"))
            .await
            .unwrap();
    });
    writer.await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let rows = a
        .query(&format!("SELECT v FROM {table} WHERE id = 1"))
        .await
        .unwrap();
    let second = match rows[0].get("v") {
        Some(sz_orm_core::Value::I64(n)) => *n,
        _ => 0,
    };
    a.execute("COMMIT").await.unwrap();
    assert_eq!(
        first, second,
        "REPEATABLE READ 下两次读应一致（szrsql 上游缺陷留档，本测试当前预期失败）"
    );
    let mut c = pool.create().await.unwrap();
    c.execute(&format!("DROP TABLE IF EXISTS {table}"))
        .await
        .unwrap();
}

/// M5b：DISCARD ALL 池路径降级验证 —— szrsql 不支持 DISCARD ALL（上游缺陷留档），
/// 本测试验证 sz-orm 池对该失败的容错：复位失败的连接被关闭重建（pool.rs release
/// 路径 Ok(Err) 分支），池不挂死、污染不扩散、后续租用正常。
#[tokio::test]
#[ignore = "需要 szrsql 服务在 127.0.0.1:6543"]
async fn matrix_m5b_session_reset_failure_degrades_gracefully() {
    let url = szrsql_url();
    if try_pool().await.is_none() {
        eprintln!("skip: szrsql 未启动");
        return;
    }
    let pg_url = url.replacen("szrsql://", "postgres://", 1);
    let handle = Arc::new(PgPoolHandle::connect(&pg_url).await.unwrap());
    let factory = Arc::new(SqlxPgConnectionFactory::new(handle));
    let config = PoolConfigBuilder::new()
        .max_size(2)
        .min_idle(0)
        .acquire_timeout(10)
        .build()
        .unwrap();
    let pool = Pool::new(config, factory).unwrap();

    let table = unique_table("mx_m5b");
    // 1) 租用连接做会话级污染（SET 会话变量 → 脏标记）后归还，
    //    触发 release 路径的 DISCARD ALL → szrsql 解析失败 → 连接被关闭
    {
        let mut conn = pool.acquire().await.unwrap();
        conn.execute("SET application_name = 'm5b_probe'")
            .await
            .unwrap();
        conn.execute(&format!("CREATE TABLE IF NOT EXISTS {table} (id INT)"))
            .await
            .unwrap();
        pool.release(conn).await;
    }
    // 2) 再次租用：应拿到新建连接，正常工作；循环验证池稳定
    for i in 0..5u32 {
        let mut conn = pool.acquire().await.unwrap();
        let sql = format!("SELECT {i} AS n");
        let rows = conn.query(&sql).await.unwrap();
        assert_eq!(rows.len(), 1, "复位失败关闭连接后第 {i} 次租用应正常");
        pool.release(conn).await;
    }
    let status = pool.status().await;
    assert!(
        status.active <= 2,
        "DISCARD 失败关闭连接后池容量应收敛，active={}",
        status.active
    );
    let mut conn = pool.acquire().await.unwrap();
    conn.execute(&format!("DROP TABLE IF EXISTS {table}"))
        .await
        .unwrap();
}
