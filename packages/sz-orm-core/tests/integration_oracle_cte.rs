//! OR10 CTE builder 的 Oracle 真连验证——Part 1 非递归（逐步落盘规避误报定位）
//!
//! 承接 2026-11-12 边界补齐批 OR10（对称 MySQL 批 MY10-B4 QueryBuilder CTE）。
//! 真库实测（26ai 23.26.3.0.0）：非递归 `WITH name AS (...)` 接受（subquery factoring）。
//! 复用 integration_oracle.rs env 注入 + assert 拒跑（凭据不入库）。

use oracle::Connection as OracleConn;
use sz_orm_core::dialect::{get_dialect, DbType};
use sz_orm_core::{Model, QueryBuilder, Value};

const ORACLE_USER_DEFAULT: &str = "sz_orm_test";
const ORACLE_PASSWORD_DEFAULT: &str = "";
const ORACLE_CONNECT_STRING_DEFAULT: &str = "127.0.0.1:1521/freepdb1";

fn oracle_user() -> String {
    std::env::var("SZ_ORM_ORACLE_USER").unwrap_or_else(|_| ORACLE_USER_DEFAULT.to_string())
}

fn oracle_password() -> String {
    let pw = std::env::var("SZ_ORM_ORACLE_PASSWORD")
        .unwrap_or_else(|_| ORACLE_PASSWORD_DEFAULT.to_string());
    assert!(
        !pw.is_empty(),
        "缺少 SZ_ORM_ORACLE_PASSWORD 环境变量（本机数据库测试密码，见 AGENTS.md「本机数据库」小节；2026-10-28 P0-3 起凭据不入库）"
    );
    pw
}

fn oracle_connect_string() -> String {
    std::env::var("SZ_ORM_ORACLE_CONNECT_STRING")
        .unwrap_or_else(|_| ORACLE_CONNECT_STRING_DEFAULT.to_string())
}

fn open_conn() -> OracleConn {
    OracleConn::connect(oracle_user(), oracle_password(), oracle_connect_string())
        .expect("oracle connect failed - is Oracle 26ai running on 127.0.0.1:1521?")
}

/// OR10 最小测试模型（仅作 QueryBuilder 类型参数）
#[derive(Default)]
struct OrCteRow;
impl Model for OrCteRow {
    type PrimaryKey = i64;
    fn table_name() -> &'static str {
        "or_cte_dummy"
    }
    fn pk(&self) -> i64 {
        0
    }
    fn set_pk(&mut self, _pk: i64) {}
}

/// 断言错误为指定 ORA 错误码（rust-oracle DbError.code 即 ORA 号码）。
#[allow(dead_code)]
fn assert_ora_code(err: &oracle::Error, code: i32, context: &str) {
    match err.db_error() {
        Some(db_err) => {
            assert_eq!(
                db_err.code(),
                code,
                "{context}: 期望 ORA-{code}，实际 ORA-{}（{db_err}）",
                db_err.code()
            );
        }
        None => panic!("{context}: 期望 ORA-{code} 的数据库错误，实际 {err:?}"),
    }
}

#[test]
#[ignore = "需要 Oracle 26ai 运行于 127.0.0.1:1521（设置 SZ_ORM_ORACLE_* 环境变量覆盖）"]
fn or10_cte_nonrecursive_realconn() {
    let conn = open_conn();
    let dialect = get_dialect(DbType::Oracle).unwrap();
    let org = "or10_cte_p1";

    let _ = conn.execute(&format!("DROP TABLE {} PURGE", dialect.quote(org)), &[]);
    let create_sql = format!(
        "CREATE TABLE {} ({} NUMBER PRIMARY KEY, {} NUMBER, {} VARCHAR2(50))",
        dialect.quote(org),
        dialect.quote("id"),
        dialect.quote("parent_id"),
        dialect.quote("name"),
    );
    conn.execute(&create_sql, &[]).expect("create org table");
    let insert_sql = format!(
        "INSERT INTO {} ({}, {}, {}) VALUES (:1, :2, :3)",
        dialect.quote(org),
        dialect.quote("id"),
        dialect.quote("parent_id"),
        dialect.quote("name"),
    );
    let null_p: Option<i64> = None;
    conn.execute(&insert_sql, &[&1i64, &null_p, &"ceo"]).expect("insert ceo");
    conn.execute(&insert_sql, &[&2i64, &1i64, &"cto"]).expect("insert cto");
    conn.execute(&insert_sql, &[&3i64, &2i64, &"dev1"]).expect("insert dev1");
    conn.execute(&insert_sql, &[&4i64, &2i64, &"dev2"]).expect("insert dev2");
    conn.commit().expect("commit");

    let qb = QueryBuilder::<OrCteRow>::new(get_dialect(DbType::Oracle).unwrap())
        .table(org)
        .with_cte(
            "active",
            format!(
                "SELECT {} FROM {} WHERE {} = ?",
                dialect.quote("id"),
                dialect.quote(org),
                dialect.quote("parent_id"),
            ),
            vec![Value::I64(1)],
        )
        .expect("with_cte")
        .select(vec!["id"])
        .expect("select")
        .where_gt("id", Value::I64(0));
    let (sql, params) = qb.build_select_with_params();
    assert!(
        sql.starts_with(&format!("WITH \"active\" AS (SELECT \"id\" FROM \"{}\"", org)),
        "非递归 CTE 前缀渲染（Oracle quote 形态）: {sql}"
    );
    assert_eq!(params.len(), 2, "CTE 参数(1) 应先于主查询参数(1)");

    // 绑定序真连验证：:1=CTE 参数(1) → CTE=parent_id=1 → 命中 [2]。
    // 若 :1/:2 顺序翻转（外层先取主查询参数），CTE 变查 parent_id=0 空集 →
    // 空结果，故「CTE 结果[2] 与主表交集 = [2]」断言同时证明「CTE 参数先于
    // 主查询绑定」的契约。
    let outer_intersect = format!(
        "SELECT t.\"id\" FROM {} t JOIN {} a ON t.\"id\" = a.\"id\"",
        dialect.quote(org),
        dialect.quote("active"),
    );
    let final_stmt = format!(
        "WITH {} AS (SELECT {} FROM {} WHERE {} = :1) {}",
        dialect.quote("active"),
        dialect.quote("id"),
        dialect.quote(org),
        dialect.quote("parent_id"),
        outer_intersect,
    );
    let ids: Vec<i64> = conn
        .query_as::<i64>(&final_stmt, &[&1i64])
        .expect("query non-recursive cte")
        .filter_map(|r| r.ok())
        .collect();
    assert_eq!(
        ids, vec![2],
        "CTE(parent_id=1) 结果 [2] 与主表交集应为 [2]；CTE 参数顺序可见于行集"
    );

    let _ = conn.execute(&format!("DROP TABLE {} PURGE", dialect.quote(org)), &[]);
}

// ============================================================================
// Part 2：递归 CTE 缺口证据（真库形态锚定）—— 独立测试锚定修复目标形态
// ============================================================================

#[test]
#[ignore = "需要 Oracle 26ai 运行于 127.0.0.1:1521（设置 SZ_ORM_ORACLE_* 环境变量覆盖）"]
fn or10_recursive_cte_oracle_form_and_gap() {
    let conn = open_conn();
    let dialect = get_dialect(DbType::Oracle).unwrap();
    let org = "or10_cte_p1";

    let _ = conn.execute(&format!("DROP TABLE {} PURGE", dialect.quote(org)), &[]);
    let create_sql = format!(
        "CREATE TABLE {} ({} NUMBER PRIMARY KEY, {} NUMBER, {} VARCHAR2(50))",
        dialect.quote(org),
        dialect.quote("id"),
        dialect.quote("parent_id"),
        dialect.quote("name"),
    );
    conn.execute(&create_sql, &[]).expect("create org table");
    let insert_sql = format!(
        "INSERT INTO {} ({}, {}, {}) VALUES (:1, :2, :3)",
        dialect.quote(org),
        dialect.quote("id"),
        dialect.quote("parent_id"),
        dialect.quote("name"),
    );
    let null_p: Option<i64> = None;
    conn.execute(&insert_sql, &[&1i64, &null_p, &"ceo"]).expect("insert ceo");
    conn.execute(&insert_sql, &[&2i64, &1i64, &"cto"]).expect("insert cto");
    conn.execute(&insert_sql, &[&3i64, &2i64, &"dev1"]).expect("insert dev1");
    conn.execute(&insert_sql, &[&4i64, &2i64, &"dev2"]).expect("insert dev2");
    conn.commit().expect("commit");

    // Oracle 递归因子化正确形态（修复目标）：WITH name (cols) AS (锚 UNION ALL 递归)
    let anchor = format!(
        "SELECT {}, {}, {}, 1 lvl FROM {} WHERE {} IS NULL",
        dialect.quote("id"),
        dialect.quote("parent_id"),
        dialect.quote("name"),
        dialect.quote(org),
        dialect.quote("parent_id"),
    );
    let recurse = format!(
        "SELECT o.{}, o.{}, o.{}, t.lvl + 1 FROM {} o JOIN org_tree t ON o.{} = t.{}",
        dialect.quote("id"),
        dialect.quote("parent_id"),
        dialect.quote("name"),
        dialect.quote(org),
        dialect.quote("parent_id"),
        dialect.quote("id"),
    );
    let oracle_form = format!(
        "WITH org_tree ({}, {}, {}, lvl) AS ({anchor} UNION ALL {recurse}) \
         SELECT {}, {}, lvl FROM org_tree ORDER BY {}",
        dialect.quote("id"),
        dialect.quote("parent_id"),
        dialect.quote("name"),
        dialect.quote("id"),
        dialect.quote("name"),
        dialect.quote("id"),
    );
    let tree: Vec<(i64, String, i64)> = conn
        .query_as::<(i64, String, i64)>(&oracle_form, &[])
        .expect("query oracle-form recursive cte")
        .filter_map(|r| r.ok())
        .collect();
    assert_eq!(
        tree,
        vec![
            (1, "ceo".to_string(), 1),
            (2, "cto".to_string(), 2),
            (3, "dev1".to_string(), 3),
            (4, "dev2".to_string(), 3),
        ],
        "Oracle 递归因子化正确形态应返回完整树"
    );

    // 反证 1：无列别名列表的递归 WITH → ORA-32039（Oracle 强制列列表）
    let no_cols_form = format!(
        "WITH org_tree AS ({anchor} UNION ALL {recurse}) SELECT {} FROM org_tree",
        dialect.quote("id"),
    );
    let err_no_cols = conn
        .query_row_as::<i64>(&no_cols_form, &[])
        .expect_err("无列别名列表的递归 WITH 应被 ORA-32039 拒绝");
    assert_ora_code(&err_no_cols, 32039, "递归 WITH 缺列别名列表");

    // 反证 2：sz-orm 现状生成面含 RECURSIVE 关键字（MySQL/PG 语法）→ ORA-02000。
    // 本批修复对象：Dialect::recursive_cte_keyword（Oracle 覆写为空串）。
    let qb = QueryBuilder::<OrCteRow>::new(get_dialect(DbType::Oracle).unwrap())
        .table("org_tree")
        .with_recursive_cte("org_tree", format!("{anchor} UNION ALL {recurse}"), vec![])
        .expect("with_recursive_cte")
        .select(vec!["id"])
        .expect("select");
    let (sql, _params) = qb.build_select_with_params();
    assert!(
        sql.contains("WITH RECURSIVE"),
        "现状生成面应含 RECURSIVE 关键字（缺口证据）: {sql}"
    );
    let err_recursive = conn
        .query_row_as::<i64>(&sql, &[])
        .expect_err("WITH RECURSIVE 应被 Oracle 拒绝（修复后 Oracle 覆写去掉关键字）");
    assert_ora_code(&err_recursive, 2000, "WITH RECURSIVE（Oracle 无该关键字）");

    let _ = conn.execute(&format!("DROP TABLE {} PURGE", dialect.quote(org)), &[]);
}