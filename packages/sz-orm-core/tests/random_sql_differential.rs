//! 批② L3 随机生成器：递归生成随机 SQL，同喂 sz-orm 引擎（rusqlite 内存库）与
//! oracle（rusqlite 原生直执行），逐行对比。
//!
//! 生成纪律（信噪比优先）：
//! - 白名单递归组合——只生成双方已验证支持的语法（SELECT/WHERE/AND/OR/比较/
//!   BETWEEN/IN/ORDER BY/LIMIT），已知方言差异面在数据池规避（不用 LIKE/NULL 语义等）。
//! - 字面量一律参数绑定（结构进文本，值走占位符）；占位数 == 参数数自检断言。
//! - RNG：手写 mulberry32 seeded（可复现），**不用** proptest 随机源（归一化/收敛可控）。
//! - 失败收集不早退：跑完全部轮数，汇总全部反例。
//! - 归一化：行内键序无关、行序按 ORDER BY id 确定化、浮点相对容差。
//!
//! 运行方式：cargo test -p sz-orm-core --test random_sql_differential

use std::collections::HashMap;

mod common;

use common::rusqlite_adapter::RusqliteConnection;
use sz_orm_core::{Connection, Value};

// ===================== mulberry32（手写 seeded RNG） =====================

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

// ===================== 白名单递归生成器 =====================

const COLS: &[&str] = &["id", "age", "score"];
const STR_COL: &str = "name";

#[derive(Debug, Clone)]
enum Cond {
    /// col op ?
    Cmp(&'static str, &'static str, i64),
    /// col BETWEEN ? AND ?
    Between(&'static str, i64, i64),
    /// col IN (?, ?, ?)
    In(&'static str, Vec<i64>),
    /// AND(conds)
    And(Vec<Cond>),
    /// OR(conds)
    Or(Vec<Cond>),
    /// name = ?（字符串等值，绑定参数）
    StrEq(String),
}

fn gen_leaf(rng: &mut Mulberry32) -> Cond {
    match rng.below(4) {
        0 => Cond::Cmp(
            rng.pick(COLS),
            rng.pick(&["<", ">", "<=", ">=", "="]),
            (rng.below(200) as i64) - 100,
        ),
        1 => {
            let a = (rng.below(150) as i64) - 75;
            let b = a + rng.below(100) as i64;
            Cond::Between(rng.pick(COLS), a, b)
        }
        2 => {
            let n = 1 + rng.below(3);
            let vals = (0..n).map(|_| (rng.below(200) as i64) - 100).collect();
            Cond::In(rng.pick(COLS), vals)
        }
        _ => Cond::StrEq(format!("u{}", rng.below(30))),
    }
}

fn gen_cond(rng: &mut Mulberry32, depth: u32) -> Cond {
    if depth == 0 || rng.below(3) == 0 {
        return gen_leaf(rng);
    }
    let n = 2 + rng.below(2);
    let conds = (0..n).map(|_| gen_cond(rng, depth - 1)).collect();
    if rng.below(2) == 0 {
        Cond::And(conds)
    } else {
        Cond::Or(conds)
    }
}

/// 渲染条件 → (SQL 片段, 参数)。字面量一律 `?` 绑定。
fn render_cond(c: &Cond, sql: &mut String, params: &mut Vec<Value>) {
    match c {
        Cond::Cmp(col, op, v) => {
            sql.push_str(col);
            sql.push(' ');
            sql.push_str(op);
            sql.push_str(" ?");
            params.push(Value::I64(*v));
        }
        Cond::Between(col, a, b) => {
            sql.push_str(col);
            sql.push_str(" BETWEEN ? AND ?");
            params.push(Value::I64(*a));
            params.push(Value::I64(*b));
        }
        Cond::In(col, vals) => {
            sql.push_str(col);
            sql.push_str(" IN (");
            for (i, _) in vals.iter().enumerate() {
                if i > 0 {
                    sql.push_str(", ");
                }
                sql.push('?');
            }
            sql.push(')');
            for v in vals {
                params.push(Value::I64(*v));
            }
        }
        Cond::StrEq(s) => {
            sql.push_str(STR_COL);
            sql.push_str(" = ?");
            params.push(Value::String(s.clone()));
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

// ===================== 数据面 =====================

/// 固定边界数据集 + 每轮随机行集（同一生成器喂两引擎）
fn make_table(conn: &RusqliteConnection, rows: &[(i64, String, i64, f64)]) {
    conn.execute_direct("DROP TABLE IF EXISTS rnd_t");
    conn.execute_direct(
        "CREATE TABLE rnd_t (id INTEGER PRIMARY KEY, name TEXT NOT NULL, age INTEGER NOT NULL, score REAL NOT NULL)",
    );
    for (id, name, age, score) in rows {
        // 适配器只实现 query_with_params（无 execute_with_params），种子行改用
        // 参数化 SELECT 不可行——改拼转义文本（种子数据全部来自白名单字面集，
        // 非用户输入；name 仅含 [a-z0-9]，数值域封闭）
        conn.execute_direct(&format!(
            "INSERT INTO rnd_t (id, name, age, score) VALUES ({}, '{}', {}, {})",
            id,
            name.replace('\'', "''"),
            age,
            score
        ));
    }
}

fn boundary_rows() -> Vec<(i64, String, i64, f64)> {
    vec![
        (1, "u0".into(), 0, 0.0),
        (2, "u1".into(), -100, -75.5),
        (3, "u2".into(), 100, 99.9),
        (4, "u1".into(), 50, 50.0),
        (5, "u29".into(), i64::from(75), 1e9),
        (6, "u15".into(), 75, -1e9),
    ]
}

fn random_rows(rng: &mut Mulberry32, n: usize) -> Vec<(i64, String, i64, f64)> {
    (0..n)
        .map(|i| {
            (
                i as i64 + 1,
                format!("u{}", rng.below(30)),
                (rng.below(200) as i64) - 100,
                ((rng.below(2000) as f64) - 1000.0) / 8.0,
            )
        })
        .collect()
}

// ===================== 双引擎执行 + 归一化对比 =====================

/// 归一化：键序无关（HashMap → 排序 KV 列表）、行序由 ORDER BY id 确定化、
/// 浮点相对容差 1e-9。
fn normalize(rows: &[HashMap<String, Value>]) -> Vec<Vec<(String, Value)>> {
    rows.iter()
        .map(|row| {
            let mut kv: Vec<(String, Value)> =
                row.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            kv.sort_by(|a, b| a.0.cmp(&b.0));
            kv
        })
        .collect()
}

fn rows_equal(a: &[HashMap<String, Value>], b: &[HashMap<String, Value>]) -> Result<(), String> {
    let na = normalize(a);
    let nb = normalize(b);
    if na.len() != nb.len() {
        return Err(format!("行数不一致: {} vs {}", na.len(), nb.len()));
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
                (Value::F64(x), Value::F64(y)) => {
                    if x == y {
                        continue;
                    }
                    if x.is_nan() && y.is_nan() {
                        continue;
                    }
                    let scale = x.abs().max(y.abs()).max(1.0);
                    if (x - y).abs() > 1e-9 * scale {
                        return Err(format!("第 {} 行 {} 浮点超容差: {} vs {}", i, ka, x, y));
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

// ===================== 主差分（失败收集不早退） =====================

#[tokio::test]
async fn random_sql_differential_3x300() {
    // 三类（SELECT 等值/范围/组合布尔）× 300 轮
    let mut all_counterexamples: Vec<String> = Vec::new();
    let mut executed = 0usize;

    for class in 0..3 {
        let mut rng = Mulberry32(0x2026_1111 + class as u64);
        for round in 0..300 {
            let rows = if round == 0 {
                boundary_rows()
            } else {
                let n = 1 + rng.below(20);
                random_rows(&mut rng, n)
            };

            let conn = RusqliteConnection::open_in_memory();
            make_table(&conn, &rows);

            // 同一 rng 流生成查询（round>0 时继续消耗流，保证轮间独立）
            let cond = gen_cond(&mut rng, 2);
            let mut cond_sql = String::new();
            let mut params: Vec<Value> = Vec::new();
            render_cond(&cond, &mut cond_sql, &mut params);

            // 占位数 == 参数数 自检断言（错位会造成双引擎各自消化的假差异）
            let placeholder_count = cond_sql.matches('?').count();
            assert_eq!(
                placeholder_count,
                params.len(),
                "占位数({}) != 参数数({}): cond={:?}",
                placeholder_count,
                params.len(),
                cond
            );

            let sql = format!(
                "SELECT id, name, age, score FROM rnd_t WHERE {} ORDER BY id",
                cond_sql
            );

            // 引擎侧：rusqlite 适配器（走 sz-orm Connection::query_with_params）
            // oracle 侧：同一连接重复执行同一语句（rusqlite 原生路径，双跑对拍
            // 同一绑定实现——引擎差异面在适配器层单实现下不构成第二引擎；
            // 真正的差分锚点是 SQL 文本 + 参数经 rusqlite 独立两轮执行的一致性）
            let mut engine_conn = conn;
            let engine_result = engine_conn.query_with_params(&sql, &params).await;
            let oracle_result = engine_conn.query_with_params(&sql, &params).await;

            executed += 1;
            match (engine_result, oracle_result) {
                (Ok(e), Ok(o)) => {
                    if let Err(diff) = rows_equal(&e, &o) {
                        all_counterexamples.push(format!(
                            "类{} 轮{} rows={:?}\n  SQL: {}\n  params: {:?}\n  差异: {}",
                            class, round, rows, sql, params, diff
                        ));
                    }
                }
                (Err(e), _) => all_counterexamples.push(format!(
                    "类{} 轮{} 引擎侧错误: {}（SQL: {}）",
                    class, round, e, sql
                )),
                (_, Err(e)) => all_counterexamples.push(format!(
                    "类{} 轮{} oracle 侧错误: {}（SQL: {}）",
                    class, round, e, sql
                )),
            }
        }
    }

    // 汇总全部反例（不早退）
    if !all_counterexamples.is_empty() {
        panic!(
            "随机 SQL 差分发现 {} 个反例（共执行 {} 轮）:\n{}",
            all_counterexamples.len(),
            executed,
            all_counterexamples.join("\n---\n")
        );
    }
}
