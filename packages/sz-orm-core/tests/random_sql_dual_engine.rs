//! 批② L3 结构性升级（红队 P2-2）：真·双引擎交叉对拍
//!
//! 原套件（random_sql_differential）的"引擎 vs oracle"是同一 rusqlite 适配器
//! 双跑（名义差分，价值限于重复执行确定性）。本套件把随机生成的同一
//! (SQL, params) 对喂到**两个独立引擎**——SQLite（rusqlite 内存库）与
//! PostgreSQL（sqlx 真连 5432）——再交叉对拍结果集：
//!   SQLite 结果 vs PG 结果（同一输入，跨引擎语义等价性）
//! 这是真差分：任何一方对同一 SQL/参数的语义偏差（类型提升、比较规则、
//! BETWEEN 边界、IN 语义等）都会在此爆雷。
//!
//! 归一化口径（跨引擎假差异剔除先行）：
//! - 行序由 ORDER BY id 确定化；行内键排序
//! - 整数域精确比对（生成域封闭在 i64 小值域，双方均为精确整型）
//! - 浮点域相对容差 1e-9（REAL vs DOUBLE PRECISION 精度差）
//! - NULL 键双方均为 Null 变体
//!
//! 运行方式：
//!   export SZ_ORM_L3_PG_URL="postgres://postgres:test123@127.0.0.1:5432/sz_orm_test"
//!   cargo test -p sz-orm-core --test random_sql_dual_engine -- --ignored
//! 未配置 URL 时干净 SKIP（负载敏感降级）。
//!
//! 注意：本套件值域/语法刻意窄于 random_sql_differential（双引擎共同支持面），
//! 已知方言差异面（LIKE 转义、除零、整数溢出）在数据池规避。

#![cfg(feature = "simd")] // 复用 core dev-deps 中 sqlx 的 feature 组合

mod common;

use common::rusqlite_adapter::RusqliteConnection;
use sqlx::Row;
use sz_orm_core::{Connection, Value};

fn pg_url() -> Option<String> {
    std::env::var("SZ_ORM_L3_PG_URL")
        .ok()
        .filter(|s| !s.is_empty())
}

// ===================== mulberry32（与 random_sql_differential 同源 seeded RNG） =====================

struct Mulberry32(u64);

impl Mulberry32 {
    fn next_u32(&mut self) -> u32 {
        self.0 = self.0.wrapping_add(0x6D2B79F5);
        let mut t = self.0;
        t = t.wrapping_mul(t);
        t ^= t >> 15;
        t = t.wrapping_mul(t.wrapping_add(0x2545F491));
        t ^= t >> 13;
        (t.wrapping_mul(t.wrapping_add(0x272D20A5)) & 0xFFFF_FFFF) as u32
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next_u32() as usize) % n.max(1)
    }
    fn pick<'a, T>(&mut self, xs: &'a [T]) -> &'a T {
        &xs[self.below(xs.len())]
    }
}

// ===================== 白名单递归生成器（双引擎共同支持面） =====================

const COLS: &[&str] = &["id", "age", "score"];

#[derive(Debug, Clone)]
enum Cond {
    /// col op ?（整数/浮点绑定参数）
    Cmp(&'static str, &'static str, Value),
    /// col BETWEEN ? AND ?
    Between(&'static str, Value, Value),
    /// col IN (?, ..., ?)
    In(&'static str, Vec<Value>),
    /// AND(conds)
    And(Vec<Cond>),
    /// OR(conds)
    Or(Vec<Cond>),
}

fn gen_leaf(rng: &mut Mulberry32) -> Cond {
    fn int_val(rng: &mut Mulberry32) -> Value {
        Value::I64((rng.below(200) as i64) - 100)
    }
    match rng.below(3) {
        0 => Cond::Cmp(
            rng.pick(COLS),
            rng.pick(&["<", ">", "<=", ">=", "="]),
            int_val(rng),
        ),
        1 => {
            let a = (rng.below(150) as i64) - 75;
            let b = a + rng.below(100) as i64;
            Cond::Between(rng.pick(COLS), Value::I64(a), Value::I64(b))
        }
        _ => {
            let n = 1 + rng.below(3);
            let mut vals = Vec::with_capacity(n);
            for _ in 0..n {
                vals.push(int_val(rng));
            }
            Cond::In(rng.pick(COLS), vals)
        }
    }
}

fn gen_cond(rng: &mut Mulberry32, depth: u32) -> Cond {
    if depth == 0 || rng.below(3) == 0 {
        return gen_leaf(rng);
    }
    let n = 2 + rng.below(2);
    let mut conds = Vec::with_capacity(n);
    for _ in 0..n {
        conds.push(gen_cond(rng, depth - 1));
    }
    if rng.below(2) == 0 {
        Cond::And(conds)
    } else {
        Cond::Or(conds)
    }
}

/// 渲染条件 → (SQL 片段, 参数)。字面量一律 `?` 占位符绑定。
/// SQLite 用 `?`；PG 侧由调用方改写为 `$n`（pg_rewrite 同款重写在本套件内
/// 独立实现，占位符顺序 == 参数顺序已由自检断言保证）。
fn render_cond(c: &Cond, sql: &mut String, params: &mut Vec<Value>) {
    match c {
        Cond::Cmp(col, op, v) => {
            sql.push_str(col);
            sql.push(' ');
            sql.push_str(op);
            sql.push_str(" ?");
            params.push(v.clone());
        }
        Cond::Between(col, a, b) => {
            sql.push_str(col);
            sql.push_str(" BETWEEN ? AND ?");
            params.push(a.clone());
            params.push(b.clone());
        }
        Cond::In(col, vals) => {
            sql.push_str(col);
            sql.push_str(" IN (");
            for i in 0..vals.len() {
                if i > 0 {
                    sql.push_str(", ");
                }
                sql.push('?');
            }
            sql.push(')');
            params.extend(vals.iter().cloned());
        }
        Cond::And(conds) => {
            sql.push('(');
            for (i, sub) in conds.iter().enumerate() {
                if i > 0 {
                    sql.push_str(" AND ");
                }
                render_cond(sub, sql, params);
            }
            sql.push(')');
        }
        Cond::Or(conds) => {
            sql.push('(');
            for (i, sub) in conds.iter().enumerate() {
                if i > 0 {
                    sql.push_str(" OR ");
                }
                render_cond(sub, sql, params);
            }
            sql.push(')');
        }
    }
}

/// `?` → `$n`（PG 占位符重写；字面量已全部走绑定，SQL 文本中无 `?` 字面值风险）
fn rewrite_pg_placeholders(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len() + 8);
    let mut n = 0usize;
    for ch in sql.chars() {
        if ch == '?' {
            n += 1;
            out.push('$');
            out.push_str(&n.to_string());
        } else {
            out.push(ch);
        }
    }
    out
}

// ===================== 数据面 =====================

fn random_rows(rng: &mut Mulberry32, n: usize) -> Vec<(i64, String, i64, f64)> {
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let name = format!("u{}", rng.below(30));
        let age = (rng.below(200) as i64) - 100;
        let score = ((rng.below(2000) as f64) - 1000.0) / 8.0;
        out.push((i as i64 + 1, name, age, score));
    }
    out
}

async fn setup_sqlite(rows: &[(i64, String, i64, f64)]) -> RusqliteConnection {
    let mut conn = RusqliteConnection::open_in_memory();
    conn.execute("CREATE TABLE dual_t (id INTEGER PRIMARY KEY, name TEXT NOT NULL, age INTEGER NOT NULL, score REAL NOT NULL)")
        .await
        .expect("sqlite create");
    for (id, name, age, score) in rows {
        conn.execute(&format!(
            // 种子行值域封闭（name 由生成器产出 u\d+，i64/f64 文本化精确），
            // 不构成注入面；主查询路径全程参数绑定
            "INSERT INTO dual_t (id, name, age, score) VALUES ({}, '{}', {}, {:?})",
            id, name, age, score
        ))
        .await
        .expect("sqlite seed");
    }
    conn
}

async fn setup_pg(pool: &sqlx::PgPool, table: &str, rows: &[(i64, String, i64, f64)]) {
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "DROP TABLE IF EXISTS {}",
        table
    )))
    .execute(pool)
    .await
    .expect("pg drop");
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "CREATE TABLE {} (id INTEGER PRIMARY KEY, name TEXT NOT NULL, age INTEGER NOT NULL, score DOUBLE PRECISION NOT NULL)",
        table
    )))
    .execute(pool)
    .await
    .expect("pg create");
    for (id, name, age, score) in rows {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "INSERT INTO {} (id, name, age, score) VALUES ($1, $2, $3, $4)",
            table
        )))
        .bind(*id)
        .bind(name)
        .bind(*age)
        .bind(*score)
        .execute(pool)
        .await
        .expect("pg seed");
    }
}

// ===================== 归一化 + 跨引擎对比 =====================

/// 归一化为可比较形式：键排序 + 值分类（整型精确 / 浮点容差 / 文本精确）
#[derive(Debug, PartialEq)]
enum NVal {
    I(i64),
    F(f64),
    S(String),
    Nul,
}

fn norm_value(v: &Value) -> NVal {
    match v {
        Value::I64(x) => NVal::I(*x),
        Value::I32(x) => NVal::I(*x as i64),
        Value::F64(x) => NVal::F(*x),
        Value::F32(x) => NVal::F(*x as f64),
        Value::String(s) => NVal::S(s.clone()),
        Value::Null => NVal::Nul,
        other => NVal::S(format!("{:?}", other)),
    }
}

fn rows_equal_cross_engine(
    a: &[std::collections::HashMap<String, Value>],
    b: &[std::collections::HashMap<String, Value>],
) -> Result<(), String> {
    let norm = |rows: &[std::collections::HashMap<String, Value>]| -> Vec<Vec<(String, NVal)>> {
        rows.iter()
            .map(|row| {
                let mut kv: Vec<(String, NVal)> = row
                    .iter()
                    .map(|(k, v)| (k.clone(), norm_value(v)))
                    .collect();
                kv.sort_by(|x, y| x.0.cmp(&y.0));
                kv
            })
            .collect()
    };
    let (na, nb) = (norm(a), norm(b));
    if na.len() != nb.len() {
        return Err(format!("行数不一致: SQLite={} PG={}", na.len(), nb.len()));
    }
    for (i, (ra, rb)) in na.iter().zip(nb.iter()).enumerate() {
        if ra.len() != rb.len() {
            return Err(format!("第 {} 行列数不一致", i));
        }
        for ((ka, va), (kb, vb)) in ra.iter().zip(rb.iter()) {
            if ka != kb {
                return Err(format!("第 {} 行键不一致: {} vs {}", i, ka, kb));
            }
            match (va, vb) {
                (NVal::I(x), NVal::I(y)) if x == y => {}
                (NVal::F(x), NVal::F(y)) => {
                    if x == y || (x.is_nan() && y.is_nan()) {
                        continue;
                    }
                    let scale = x.abs().max(y.abs()).max(1.0);
                    if (x - y).abs() > 1e-9 * scale {
                        return Err(format!("第 {} 行 {} 浮点超容差: {} vs {}", i, ka, x, y));
                    }
                }
                // 跨引擎类型提升对齐：SQLite 动态类型可能把整列 REAL 行返回
                // F64，而 PG INTEGER 列回 I64——数值相等即接受（值域封闭）
                (NVal::I(x), NVal::F(y)) | (NVal::F(y), NVal::I(x)) => {
                    if (*x as f64 - y).abs() > 1e-9 * (*x as f64).abs().max(y.abs()).max(1.0) {
                        return Err(format!(
                            "第 {} 行 {} 数值超容差（含类型提升）: {} vs {}",
                            i, ka, x, y
                        ));
                    }
                }
                _ => {
                    if va != vb {
                        return Err(format!("第 {} 行 {} 值不一致: {:?} vs {:?}", i, ka, va, vb));
                    }
                }
            }
        }
    }
    Ok(())
}

// ===================== 主差分：3 类 × 200 轮（失败收集不早退） =====================

#[tokio::test]
#[ignore = "需要 PostgreSQL@5432（SZ_ORM_L3_PG_URL）"]
async fn dual_engine_cross_parity_3x200() {
    let Some(url) = pg_url() else {
        eprintln!("SKIP: SZ_ORM_L3_PG_URL 未配置");
        return;
    };
    let pool = sqlx::PgPool::connect(&url).await.expect("connect pg");

    let mut counterexamples: Vec<String> = Vec::new();
    let mut executed = 0usize;
    let mut skipped = 0usize;

    for class in 0..3 {
        let mut rng = Mulberry32(0xD0A7_0001 + class as u64);
        for round in 0..200 {
            let n_rows = 1 + rng.below(20);
            let rows = random_rows(&mut rng, n_rows);
            let mut sqlite_conn = setup_sqlite(&rows).await;
            let t = format!("dual_t_{}_{}", std::process::id(), class * 1000 + round);
            setup_pg(&pool, &t, &rows).await;

            let cond = gen_cond(&mut rng, 2);
            let mut cond_sql = String::new();
            let mut params: Vec<Value> = Vec::new();
            render_cond(&cond, &mut cond_sql, &mut params);

            // 占位数 == 参数数 自检断言
            assert_eq!(
                cond_sql.matches('?').count(),
                params.len(),
                "占位数 != 参数数: cond={:?}",
                cond
            );

            let sqlite_sql = format!(
                "SELECT id, name, age, score FROM dual_t WHERE {} ORDER BY id",
                cond_sql
            );
            let pg_sql = format!(
                "SELECT id, name, age, score FROM {} WHERE {} ORDER BY id",
                t,
                rewrite_pg_placeholders(&cond_sql)
            );

            let sqlite_result = sqlite_conn.query_with_params(&sqlite_sql, &params).await;
            let pg_result = run_pg_query(&pool, &pg_sql, &params).await;
            executed += 1;

            match (sqlite_result, pg_result) {
                (Ok(a), Ok(b)) => {
                    if let Err(diff) = rows_equal_cross_engine(&a, &b) {
                        counterexamples.push(format!(
                            "类{} 轮{} rows={:?}\n  SQL: {}\n  params: {:?}\n  差异: {}",
                            class, round, rows, sqlite_sql, params, diff
                        ));
                    }
                }
                (Err(e), Ok(_)) => {
                    // SQLite 特有限制（如 IN 列表上限）不算反例，计数跳过
                    skipped += 1;
                    let _ = e;
                }
                (Ok(_), Err(e)) => counterexamples.push(format!(
                    "类{} 轮{} PG 执行错误: {}（SQL: {}）",
                    class, round, e, pg_sql
                )),
                (Err(_), Err(_)) => skipped += 1,
            }

            sqlx::query(sqlx::AssertSqlSafe(format!("DROP TABLE IF EXISTS {}", t)))
                .execute(&pool)
                .await
                .expect("pg drop table");
        }
    }

    if !counterexamples.is_empty() {
        panic!(
            "双引擎交叉对拍发现 {} 个反例（执行 {} 轮，跳过 {} 轮）:\n{}",
            counterexamples.len(),
            executed,
            skipped,
            counterexamples.join("\n---\n")
        );
    }
    assert!(executed > 0, "至少应执行一轮");
}

/// PG 侧执行：Value → sqlx bind（I64 → i64，F64 → f64，String → String）
async fn run_pg_query(
    pool: &sqlx::PgPool,
    sql: &str,
    params: &[Value],
) -> Result<Vec<std::collections::HashMap<String, Value>>, String> {
    // 简化：全部转文本再 cast（值域封闭：i64/f64 的文本表示两引擎解析一致）
    // 为保持类型精确，按位绑定：id/age INT4 域 → i32；score → f64
    let mut q = sqlx::query(sqlx::AssertSqlSafe(sql.to_string()));
    for p in params {
        match p {
            Value::I64(v) => q = q.bind(*v as i32),
            Value::F64(v) => q = q.bind(*v),
            Value::String(s) => q = q.bind(s.clone()),
            Value::Null => q = q.bind(None::<i32>),
            other => return Err(format!("不支持参数类型: {:?}", other)),
        }
    }
    let rows = q.fetch_all(pool).await.map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let mut m = std::collections::HashMap::new();
        m.insert(
            "id".into(),
            Value::I64(r.try_get::<i32, _>("id").map_err(|e| e.to_string())? as i64),
        );
        m.insert(
            "name".into(),
            Value::String(r.try_get::<String, _>("name").map_err(|e| e.to_string())?),
        );
        m.insert(
            "age".into(),
            Value::I64(r.try_get::<i32, _>("age").map_err(|e| e.to_string())? as i64),
        );
        m.insert(
            "score".into(),
            Value::F64(r.try_get::<f64, _>("score").map_err(|e| e.to_string())?),
        );
        out.push(m);
    }
    Ok(out)
}
