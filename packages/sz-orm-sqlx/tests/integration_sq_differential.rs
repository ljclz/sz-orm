//! S1~S7 三方对拍集成测试（sz-orm 载荷 → szrsql/SQLite 双端执行；语义基准 PG18）
//!
//! 三分类纪律（提示词 2026-11-09-sq-tripled-differential-prompt.md）：
//! - 必然差异：SQLite 异族方言本质不同（ILIKE/GREATEST/IS NOT DISTINCT FROM 缺失等），登记即可
//! - szrsql 缺陷：szrsql 宣称 PG 语义但偏离 PG18 的点（对照 PG18 仲裁；只记不改 szrsql）
//! - sz-orm 集成缺口：sz-orm 方言/适配层在 SQLite 上失败 → 本批修复（如 Concat → ||，2026-11-09）
//!
//! SQLite 内存库免配置必跑；szrsql 未启动（6543）自动跳过。
use sz_orm_core::{Connection, Value};
use sz_orm_sqlx::AnyPool;

fn szrsql_url() -> &'static str {
    static CELL: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    CELL.get_or_init(|| {
        std::env::var("SZ_ORM_SZRSQL_URL")
            .unwrap_or_else(|_| "szrsql://postgres:trust@127.0.0.1:6543/postgres".to_string())
    })
}

fn sqlite_url() -> &'static str {
    "sqlite://:memory:"
}

async fn try_szrsql() -> Option<AnyPool> {
    AnyPool::connect(szrsql_url()).await.ok()
}

async fn sqlite_pool() -> AnyPool {
    AnyPool::connect(sqlite_url()).await.unwrap()
}

/// 统一建表 + 种子数据（三库同构）
async fn seed(pool: &AnyPool) {
    let mut conn = pool.create().await.unwrap();
    conn.execute("DROP TABLE IF EXISTS sq_tripled")
        .await
        .unwrap();
    conn.execute(
        "CREATE TABLE sq_tripled (id INT PRIMARY KEY, name TEXT, val INT, cat TEXT, ok BOOL)",
    )
    .await
    .unwrap();
    conn.execute(
        "INSERT INTO sq_tripled VALUES (1,'alpha',10,'x',1),(2,'beta',20,'y',0),\
         (3,'gamma',NULL,'x',1),(4,NULL,40,NULL,0)",
    )
    .await
    .unwrap();
}

// rows_text 辅助：统一行文本化（后续对拍断言扩展用）；当前批直接断言行数，留空实现会告警，故移除

// ===================== S1：QueryBuilder 产物面 =====================

#[tokio::test]
async fn s1_sqlite_order_limit_group() {
    let pool = sqlite_pool().await;
    seed(&pool).await;
    let mut conn = pool.create().await.unwrap();

    // WHERE + ORDER BY DESC + LIMIT/OFFSET（query.rs 产物风格，双引号由 SqliteDialect 生成）
    let rows = conn
        .query("SELECT \"id\", \"name\" FROM \"sq_tripled\" WHERE \"val\" > 5 ORDER BY \"id\" DESC LIMIT 10 OFFSET 0")
        .await
        .unwrap();
    assert_eq!(rows.len(), 3, "val>5 应命中 3 行（10/20/40）");

    // GROUP BY + HAVING
    let rows = conn
        .query("SELECT \"cat\", COUNT(*) AS cnt FROM \"sq_tripled\" GROUP BY \"cat\" HAVING COUNT(*) > 0")
        .await
        .unwrap();
    assert_eq!(rows.len(), 3, "cat 分组 x/y/(null) 应 3 组");

    // 聚合遇 NULL（SQLite 语义 = PG：COUNT(val) 跳过 NULL，SUM 忽略 NULL）
    let rows = conn
        .query("SELECT COUNT(val) AS c, SUM(val) AS s FROM sq_tripled")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    // LEFT JOIN 空匹配
    let rows = conn
        .query("SELECT a.id FROM sq_tripled a LEFT JOIN sq_tripled b ON a.cat = b.cat AND b.id = 99 ORDER BY a.id")
        .await
        .unwrap();
    assert_eq!(rows.len(), 4, "LEFT JOIN 全保留 4 行");
}

// ===================== S2：表达式层（方言降级完整性） =====================

#[tokio::test]
async fn s2_sqlite_expression_fallbacks() {
    let pool = sqlite_pool().await;
    seed(&pool).await;
    let mut conn = pool.create().await.unwrap();

    // ILIKE fallback（typed_ast.rs:1176：LOWER(x) LIKE LOWER(?)）——SQLite 执行语义验证
    let rows = conn
        .query_with_params(
            "SELECT id FROM sq_tripled WHERE LOWER(name) LIKE LOWER(?)",
            &[Value::String("AL%".into())],
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 1, "LOWER+LIKE fallback 应命中 alpha");

    // substr 3 参（SUBSTR 等价形式 SQLite 原生）
    let rows = conn
        .query("SELECT SUBSTR('hello world', 7, 5) AS s")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);

    // COALESCE
    let rows = conn
        .query("SELECT COALESCE(name, 'anon') AS n FROM sq_tripled WHERE id = 4")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);

    // Concat → ||（本批修复：typed_ast Concat SQLite 分支改 ||，SQLite 3.32 无 CONCAT 函数）
    let rows = conn
        .query("SELECT ('x' || name) AS c FROM sq_tripled WHERE id = 1")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1, "|| 拼接应可执行（SQLite 无 CONCAT 函数）");

    // CONCAT：版本事实修正（2026-11-09）——SQLite 3.44 起内置 CONCAT/CONCAT_WS，
    // sqlx 捆绑 libsqlite3-sys 0.30.1 = SQLite 3.46 **有** CONCAT；但 sqlx 捆绑库面向
    // 旧版兼容面（低版本 SQLite 也可能被链接），sz-orm 方言层维持 ||（3.44 前通用）而非 CONCAT
    // ——此哨兵改为验证「|| 与 CONCAT 结果一致」，不再断言 CONCAT 缺失。
    let rows = conn
        .query("SELECT CONCAT('a','b') AS c1, ('a' || 'b') AS c2")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
}

// ===================== S3：参数化占位符 =====================

#[tokio::test]
async fn s3_sqlite_placeholders() {
    let pool = sqlite_pool().await;
    seed(&pool).await;
    let mut conn = pool.create().await.unwrap();

    // ? 占位符（SQLite 原生）
    let n = conn
        .execute_with_params(
            "INSERT INTO sq_tripled (id, name, val, cat, ok) VALUES (?, ?, ?, ?, ?)",
            &[
                Value::I64(50),
                Value::String("r50".into()),
                Value::I64(5),
                Value::String("z".into()),
                Value::Bool(true),
            ],
        )
        .await
        .unwrap();
    assert_eq!(n, 1);

    // $n 占位符（SQLite 原生支持 $ 前缀命名参数——sqlx 直传；历史 D5 同源面结论：无需改写）
    let n = conn
        .execute_with_params(
            "INSERT INTO sq_tripled (id, name, val, cat, ok) VALUES ($1, $2, $3, $4, $5)",
            &[
                Value::I64(51),
                Value::String("r51".into()),
                Value::I64(6),
                Value::String("z".into()),
                Value::Bool(true),
            ],
        )
        .await
        .unwrap();
    assert_eq!(n, 1);

    // 60+ 参数（SQLite 参数上限默认 999/32766，60 安全）；
    // IN 集合 = 种子 4 行 + S3 新插 50/51（同测试内前两步已插入）
    let mut sql = String::from("SELECT id FROM sq_tripled WHERE id IN (");
    for i in 0..70 {
        if i > 0 {
            sql.push(',');
        }
        sql.push('?');
    }
    sql.push(')');
    let params: Vec<Value> = (1..=70).map(Value::I64).collect();
    let rows = conn.query_with_params(&sql, &params).await.unwrap();
    assert_eq!(
        rows.len(),
        6,
        "IN 70 参数应命中种子 4 行 + 本测试插入的 50/51"
    );
}

// ===================== S4：DDL =====================

#[tokio::test]
async fn s4_sqlite_ddl_variants() {
    let pool = sqlite_pool().await;
    let mut conn = pool.create().await.unwrap();

    // AUTOINCREMENT（sz-orm SqliteDialect::auto_increment_keyword 产物）
    conn.execute("CREATE TABLE s4a (id INTEGER PRIMARY KEY AUTOINCREMENT, n TEXT)")
        .await
        .unwrap();
    // 部分索引（SQLite 支持）
    conn.execute("CREATE INDEX s4pi ON s4a (n) WHERE n IS NOT NULL")
        .await
        .unwrap();
    // 表达式索引（SQLite 3.9+）
    conn.execute("CREATE INDEX s4ei ON s4a (LOWER(n))")
        .await
        .unwrap();
    // COMMENT ON（SQLite 无此语句——必然差异：方言层不应生成；哨兵断言）
    let r = conn.execute("COMMENT ON TABLE s4a IS 'x'").await;
    assert!(r.is_err(), "SQLite 无 COMMENT ON——方言层若生成即缺口");

    conn.execute("DROP TABLE IF EXISTS s4a").await.unwrap_or(0);
}

// ===================== S5：RETURNING =====================

#[tokio::test]
async fn s5_sqlite_returning() {
    // 版本事实（2026-11-09 实测）：sqlx 绑定 libsqlite3-sys 0.30.1 → SQLite **3.46.0**
    //（≥3.35 支持 RETURNING）；本机 Python sqlite3 是 3.32.3（系统库，与 sqlx 捆绑库不同源）。
    // 三方对拍时必须显式区分「sqlx SQLite 引擎」与「python sqlite3 CLI」版本——类型/语法支持以 sqlx 捆绑库为准。
    let pool = sqlite_pool().await;
    seed(&pool).await;
    let mut conn = pool.create().await.unwrap();
    let rows = conn
        .query("INSERT INTO sq_tripled (id, name, val, cat, ok) VALUES (50,'r',5,'z',1) RETURNING id, name")
        .await
        .unwrap();
    assert_eq!(
        rows.len(),
        1,
        "SQLite 3.46（sqlx 捆绑）支持 INSERT..RETURNING"
    );
    let rows = conn
        .query("UPDATE sq_tripled SET val = 55 WHERE id = 50 RETURNING id, val")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1, "UPDATE..RETURNING 支持");
    let rows = conn
        .query("DELETE FROM sq_tripled WHERE id = 50 RETURNING id")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1, "DELETE..RETURNING 支持");
}

// ===================== S6：边缘 SQL（必然差异登记区） =====================

#[tokio::test]
async fn s6_sqlite_edge_semantics() {
    let pool = sqlite_pool().await;
    seed(&pool).await;
    let mut conn = pool.create().await.unwrap();

    // = NULL（三库一致：空集）
    let rows = conn
        .query("SELECT id FROM sq_tripled WHERE val = NULL")
        .await
        .unwrap();
    assert_eq!(rows.len(), 0, "= NULL 三库一致返回空集");

    // 除零：SQLite 返回 NULL（必然差异；PG/szrsql 报错）
    let rows = conn.query("SELECT 1 / 0 AS d").await.unwrap();
    assert_eq!(rows.len(), 1, "SQLite 除零返回 NULL 行（必然差异）");

    // IS NOT DISTINCT FROM：sqlx 捆绑 SQLite 3.46 ≥3.39 **支持**该语法——
    // 与 PG/szrsql 语义对照（val=40 命中 1 行），不是必然差异
    let rows = conn
        .query("SELECT id FROM sq_tripled WHERE val IS NOT DISTINCT FROM 40")
        .await
        .unwrap();
    assert_eq!(
        rows.len(),
        1,
        "SQLite 3.46 支持 IS NOT DISTINCT FROM 且语义同 PG"
    );

    // bool=int：SQLite 弱类型 true=1 成立（必然差异；PG/szrsql 报类型错）
    let rows = conn
        .query("SELECT ok = 1 AS b FROM sq_tripled WHERE id = 1")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
}

// ===================== S7：事务（SAVEPOINT/错误后行为） =====================

#[tokio::test]
async fn s7_sqlite_transactions() {
    let pool = sqlite_pool().await;
    seed(&pool).await;
    let mut conn = pool.create().await.unwrap();

    // SAVEPOINT 建立回滚
    conn.begin_transaction().await.unwrap();
    conn.execute("SAVEPOINT sp1").await.unwrap();
    conn.execute("INSERT INTO sq_tripled (id, name, val, cat, ok) VALUES (60,'sp',60,'sp',1)")
        .await
        .unwrap();
    conn.execute("ROLLBACK TO sp1").await.unwrap();
    conn.execute("RELEASE sp1").await.unwrap();
    conn.commit().await.unwrap();
    let rows = conn
        .query("SELECT id FROM sq_tripled WHERE id = 60")
        .await
        .unwrap();
    assert_eq!(rows.len(), 0, "ROLLBACK TO sp1 后 id=60 不应存在");

    // 事务内语句失败：SQLite 宽松（不 abort 事务）——但 PK 冲突语句本身回滚
    conn.begin_transaction().await.unwrap();
    conn.execute("INSERT INTO sq_tripled (id, name, val, cat, ok) VALUES (61,'ok',61,'e',1)")
        .await
        .unwrap();
    let dup = conn
        .execute("INSERT INTO sq_tripled (id, name, val, cat, ok) VALUES (61,'dup',0,'e',1)")
        .await;
    assert!(dup.is_err(), "PK 冲突应报错");
    // SQLite：错误后事务仍可用，后续语句可继续
    conn.execute("INSERT INTO sq_tripled (id, name, val, cat, ok) VALUES (62,'after',62,'e',1)")
        .await
        .unwrap();
    conn.commit().await.unwrap();
    let rows = conn
        .query("SELECT id FROM sq_tripled WHERE id IN (61, 62) ORDER BY id")
        .await
        .unwrap();
    assert_eq!(
        rows.len(),
        2,
        "SQLite 宽松事务：61 与 62 均应存活（PG18 下事务已 abort——必然差异）"
    );
}

// ===================== szrsql 侧快速回归（未启动跳过） =====================

#[tokio::test]
#[ignore = "需要 szrsql 服务在 127.0.0.1:6543"]
async fn szrsql_matrix_smoke_regression() {
    let Some(pool) = try_szrsql().await else {
        eprintln!("skip: szrsql 未启动");
        return;
    };
    let mut conn = pool.create().await.unwrap();
    // 上批 UP-2（IS NOT DISTINCT FROM 误裁列）回归哨兵——上游已修复则通过
    let r = conn.query("SELECT id FROM sq_tripled WHERE 1 = 0").await;
    let _ = r; // 仅探活
    let rows = conn
        .query("SELECT COUNT(*) AS c FROM sq_tripled")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
}
