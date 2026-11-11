//! Oracle 方言矩阵真连验证（OR1~OR6，2026-11-11 提示词批次）
//!
//! 评测对象是 **sz-orm 自己的 OracleDialect 生成面**：载荷按 `DbType::Oracle`
//! 生成 SQL，在真 Oracle 26ai Free（23ai 家族）上执行，验证「生成被接受」且
//! 「结果符合 Oracle 自身语义」。语义基准为 Oracle 官方行为，禁止拿 PG/MySQL
//! 当仲裁。
//!
//! 矩阵块：
//! - OR1 空串与 NULL（Oracle 空串即 NULL，与 MySQL/PG 相反）
//! - OR2 标识符大小写折叠（未引号→大写；引号→原样；128 字节上限）
//! - OR3 主键回填（IDENTITY BY DEFAULT/ALWAYS；RETURNING INTO 通道）
//! - OR4 分页与排序（OFFSET/FETCH 成对；NULLS FIRST/LAST）
//! - OR5 类型往返（NUMBER 精度 / VARCHAR2(4000) 边界 / CLOB / RAW / TIMESTAMP）
//! - OR6 边缘语义（三值 NULL / ORA-01476 除零 / 隐式转换 / DUAL / DDL 隐式提交）
//!
//! 三分类口径：
//! - **sz-orm 方言缺口**：生成 SQL 被真库拒绝（ORA-xxxx）/结果错位 → 修复 dialect.rs
//! - **Oracle 环境特有**：真库自身行为（DDL 隐式提交等）→ 登记 + 文档标注
//! - **文档措辞**：行为正确但文档描述误导 → 修正措辞
//!
//! 复用 `integration_oracle.rs` 的 env 注入 + assert 拒跑模式（凭据不入库）。

use oracle::Connection as OracleConn;
use std::sync::atomic::{AtomicU64, Ordering};
use sz_orm_core::dialect::{get_dialect, ColumnDef};
use sz_orm_core::DbType;

/// 默认 Oracle 连接参数（本机 26ai Free）；可通过环境变量覆盖。
/// 使用专用测试用户 sz_orm_test（已授予 DBA 权限），避免 sys/SYSDBA 特权。
const ORACLE_USER_DEFAULT: &str = "sz_orm_test";
// 凭据不入库（2026-10-28 P0-3）：默认空串，运行时必须设置对应 SZ_ORM_*_PASSWORD 环境变量
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
    // 普通用户连接（sz_orm_test 已授予 DBA 权限，无需 SYSDBA 特权）
    OracleConn::connect(oracle_user(), oracle_password(), oracle_connect_string())
        .expect("oracle connect failed - is Oracle 26ai running on 127.0.0.1:1521?")
}

/// 断言错误为指定 ORA 错误码（rust-oracle DbError.code 即 ORA 号码）。
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

static TABLE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// 生成唯一的临时表名（保持小写；配合 OracleDialect.quote() 的双引号包裹
/// 使真库按原样存储，与本文件其余测试的可见性约定一致）。
fn unique_table(prefix: &str) -> String {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let counter = TABLE_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!(
        "t_ormx_{}_{}_{}",
        prefix,
        pid % 1000,
        (nanos % 100000) as u64 * 1000 + counter
    )
}

/// 使用 OracleDialect 生成 CREATE TABLE（方言生成面）并执行
fn create_test_table(conn: &OracleConn, table: &str) {
    let dialect = get_dialect(DbType::Oracle).expect("oracle dialect");
    let columns = vec![
        ColumnDef {
            name: "id".to_string(),
            sql_type: "NUMBER".to_string(),
            nullable: false,
            default: None,
            auto_increment: true,
            primary_key: true,
        },
        ColumnDef {
            name: "name".to_string(),
            sql_type: "VARCHAR2(255)".to_string(),
            nullable: false,
            default: None,
            auto_increment: false,
            primary_key: false,
        },
        ColumnDef {
            name: "value".to_string(),
            sql_type: "NUMBER".to_string(),
            nullable: true,
            default: None,
            auto_increment: false,
            primary_key: false,
        },
    ];
    let sql = dialect.build_create_table(table, &columns);
    conn.execute(&sql, &[]).expect("create table");
}

fn drop_table_if_exists(conn: &OracleConn, table: &str) {
    let dialect = get_dialect(DbType::Oracle).expect("oracle dialect");
    let sql = dialect.build_drop_table(table, true);
    let _ = conn.execute(&sql, &[]);
}

// ============================================================================
// OR1 空串与 NULL —— Oracle 空串即 NULL（与 MySQL/PG 相反）
//
// 方言层生成不假设空串语义，断言锚定 Oracle 行为：
//   - INSERT '' 后 IS NULL 判定为真
//   - '' = '' 比较为三值逻辑 UNKNOWN（非真）
//   - || 拼接中 NULL 被当作空串（字符串拼接特例，与比较语义相反）
// ============================================================================

#[test]
#[ignore = "需要 Oracle 26ai 运行于 127.0.0.1:1521（设置 SZ_ORM_ORACLE_* 环境变量覆盖）"]
fn or1_empty_string_insert_then_is_null() {
    let conn = open_conn();
    let dialect = get_dialect(DbType::Oracle).unwrap();

    // ---- 第 1 部分：可空列上的空串 IS NULL 判定 ----
    let table = unique_table("or1");
    drop_table_if_exists(&conn, &table);
    // name 列可空（OracleDialect 生成面：nullable=true 不加 NOT NULL）
    let columns = vec![
        ColumnDef {
            name: "id".to_string(),
            sql_type: "NUMBER".to_string(),
            nullable: false,
            default: None,
            auto_increment: true,
            primary_key: true,
        },
        ColumnDef {
            name: "name".to_string(),
            sql_type: "VARCHAR2(255)".to_string(),
            nullable: true,
            default: None,
            auto_increment: false,
            primary_key: false,
        },
    ];
    let create_sql = dialect.build_create_table(&table, &columns);
    conn.execute(&create_sql, &[]).expect("create nullable table");

    let insert_sql = format!(
        "INSERT INTO {} ({}) VALUES (:1)",
        dialect.quote(&table),
        dialect.quote("name"),
    );

    // 载荷 1：参数化绑定空串 —— Oracle 存储为 NULL（空串即 NULL）
    conn.execute(&insert_sql, &[&""]).expect("insert empty");
    // 载荷 2：参数化绑定 NULL —— 同样是 NULL
    let null_str: Option<&str> = None;
    conn.execute(&insert_sql, &[&null_str]).expect("insert null");
    // 载荷 3：非空串 —— IS NOT NULL
    conn.execute(&insert_sql, &[&"alice"]).expect("insert alice");
    conn.commit().expect("commit");

    // 行为断言：'' 行与 NULL 行都满足 IS NULL
    let is_null_sql = format!(
        "SELECT COUNT(*) FROM {} WHERE {} IS NULL",
        dialect.quote(&table),
        dialect.quote("name"),
    );
    let n: i64 = conn
        .query_row_as::<i64>(&is_null_sql, &[])
        .expect("count is null");
    assert_eq!(n, 2, "空串与 NULL 应同样满足 IS NULL（Oracle 空串即 NULL）");

    // 行为断言：参数化绑定空串的等值比较不命中任何行（三值逻辑）
    let eq_empty_sql = format!(
        "SELECT COUNT(*) FROM {} WHERE {} = :1",
        dialect.quote(&table),
        dialect.quote("name"),
    );
    let n_eq: i64 = conn
        .query_row_as::<i64>(&eq_empty_sql, &[&""])
        .expect("count eq empty");
    assert_eq!(n_eq, 0, "col = '' 应命中 0 行（'' 即 NULL，等值比较为 UNKNOWN）");

    // 行为断言：IS NOT NULL 只命中非空串行
    let not_null_sql = format!(
        "SELECT COUNT(*) FROM {} WHERE {} IS NOT NULL",
        dialect.quote(&table),
        dialect.quote("name"),
    );
    let n_not: i64 = conn
        .query_row_as::<i64>(&not_null_sql, &[])
        .expect("count is not null");
    assert_eq!(n_not, 1, "仅 alice 一行 IS NOT NULL");

    drop_table_if_exists(&conn, &table);

    // ---- 第 2 部分：NOT NULL 约束拒绝空串（'' ≡ NULL 在约束层生效）----
    // Oracle 特有：参数化绑定 "" 到 NOT NULL VARCHAR2 列被 ORA-01400 拒绝，
    // 因为空串在绑定层已转换为 NULL。MySQL/PG 中 "" 是合法非 NULL 值。
    // —— Oracle 环境特有行为，登记；方言层不假设空串语义的佐证。
    let nn_table = unique_table("or1nn");
    drop_table_if_exists(&conn, &nn_table);
    create_test_table(&conn, &nn_table);
    let nn_insert_sql = format!(
        "INSERT INTO {} ({}) VALUES (:1)",
        dialect.quote(&nn_table),
        dialect.quote("name"),
    );
    let err = conn
        .execute(&nn_insert_sql, &[&""])
        .expect_err("空串插入 NOT NULL 列应被 ORA-01400 拒绝");
    assert_ora_code(&err, 1400, "空串插入 NOT NULL 列");
    drop_table_if_exists(&conn, &nn_table);
}

#[test]
#[ignore = "需要 Oracle 26ai 运行于 127.0.0.1:1521（设置 SZ_ORM_ORACLE_* 环境变量覆盖）"]
fn or1_empty_string_literal_semantics_on_dual() {
    let conn = open_conn();

    // 行为断言：'' IS NULL 为真（Oracle 特有；MySQL/PG 为假）
    let case_sql = "SELECT CASE WHEN '' IS NULL THEN 1 ELSE 0 END FROM DUAL";
    let v: i64 = conn.query_row_as::<i64>(case_sql, &[]).expect("case is null");
    assert_eq!(v, 1, "'' IS NULL 应为真（Oracle 空串即 NULL）");

    // 行为断言：'' = '' 为 UNKNOWN（走 ELSE 分支）
    let eq_sql = "SELECT CASE WHEN '' = '' THEN 1 ELSE 0 END FROM DUAL";
    let v_eq: i64 = conn.query_row_as::<i64>(eq_sql, &[]).expect("case eq");
    assert_eq!(v_eq, 0, "'' = '' 应为 UNKNOWN（非真）");

    // 行为断言：NVL 把空串当作 NULL 处理
    let nvl_sql = "SELECT NVL('', 'X') FROM DUAL";
    let v_nvl: String = conn
        .query_row_as::<String>(nvl_sql, &[])
        .expect("nvl");
    assert_eq!(v_nvl, "X", "NVL('', 'X') 应返回 'X'（空串按 NULL 处理）");
}

#[test]
#[ignore = "需要 Oracle 26ai 运行于 127.0.0.1:1521（设置 SZ_ORM_ORACLE_* 环境变量覆盖）"]
fn or1_concat_null_propagates_as_empty() {
    let conn = open_conn();

    // 方言生成面：OracleDialect::concat 用 || 连接（dialect.rs concat 实现面）
    let dialect = get_dialect(DbType::Oracle).unwrap();
    let expr = dialect.concat(&["'a'", ":1", "'c'"]);
    assert_eq!(expr, "'a' || :1 || 'c'", "OracleDialect::concat 应生成 || 连接");

    let sql = format!("SELECT {} FROM DUAL", expr);

    // 行为断言：|| 拼接中 NULL 被当作空串（字符串拼接特例，与比较语义相反）
    let null_part: Option<&str> = None;
    let v_null: String = conn
        .query_row_as::<String>(&sql, &[&null_part])
        .expect("concat null");
    assert_eq!(v_null, "ac", "NULL 参与 || 拼接应被当作空串");

    // 行为断言：空串参与 || 拼接同样为空串语义
    let v_empty: String = conn
        .query_row_as::<String>(&sql, &[&""])
        .expect("concat empty");
    assert_eq!(v_empty, "ac", "'' 参与 || 拼接应被当作空串");

    // 行为断言：非空值正常拼接
    let v_b: String = conn
        .query_row_as::<String>(&sql, &[&"b"])
        .expect("concat b");
    assert_eq!(v_b, "abc");
}
