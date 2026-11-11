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

// ============================================================================
// OR2 标识符大小写折叠 —— 未加引号 → 大写；双引号 → 原样保留
//
// OracleDialect.quote() 恒用双引号包裹（dialect.rs:1109），因此 sz-orm 生成的
// 标识符按原样（区分大小写）存储。本块验证：
//   - 未引号混合大小写标识符在真库折叠为大写
//   - 引号包裹的小写标识符按原样存储 + ALL_TAB_COLUMNS 元数据探测面
//   - 128 字节标识符上限（12.2+）；129 字节 ORA-00972
// ============================================================================

#[test]
#[ignore = "需要 Oracle 26ai 运行于 127.0.0.1:1521（设置 SZ_ORM_ORACLE_* 环境变量覆盖）"]
fn or2_unquoted_identifier_folds_to_uppercase() {
    let conn = open_conn();
    let dialect = get_dialect(DbType::Oracle).unwrap();

    // 载荷：未加引号的混合大小写标识符（不经 dialect.quote，直接拼接）
    let pid = std::process::id() % 10000;
    let raw_name = format!("Or2_Fold_{}", pid);
    let upper_name = raw_name.to_uppercase();

    // 清理残留（大写折叠后的名字）
    let _ = conn.execute(&format!("DROP TABLE {} PURGE", upper_name), &[]);

    conn.execute(&format!("CREATE TABLE {} (id NUMBER)", raw_name), &[])
        .expect("create unquoted table");

    // 行为断言：user_tables 中按大写存储
    let count_sql = "SELECT COUNT(*) FROM user_tables WHERE table_name = :1";
    let n: i64 = conn
        .query_row_as::<i64>(count_sql, &[&upper_name])
        .expect("check user_tables");
    assert_eq!(n, 1, "未引号标识符 {raw_name} 应在真库折叠为大写 {upper_name}");

    // 行为断言：未引号 SELECT 可用（再次折叠回大写）
    let sel_sql = format!("SELECT COUNT(*) FROM {}", raw_name);
    let c: i64 = conn
        .query_row_as::<i64>(&sel_sql, &[])
        .expect("unquoted select");
    assert_eq!(c, 0);

    // 行为断言：小写双引号 SELECT 不可用（ORA-00942：引号保留原样，小写名不存在）
    let quoted_lower_sql = format!("SELECT COUNT(*) FROM \"{}\"", raw_name);
    let err = conn
        .query_row_as::<i64>(&quoted_lower_sql, &[])
        .expect_err("小写引号名应不存在");
    assert_ora_code(&err, 942, "小写引号标识符查询");

    conn.execute(&format!("DROP TABLE {} PURGE", upper_name), &[])
        .expect("drop folded table");
}

#[test]
#[ignore = "需要 Oracle 26ai 运行于 127.0.0.1:1521（设置 SZ_ORM_ORACLE_* 环境变量覆盖）"]
fn or2_quoted_lowercase_roundtrip_and_metadata_probe() {
    let conn = open_conn();
    let dialect = get_dialect(DbType::Oracle).unwrap();

    // 载荷：sz-orm 生成面——quote() 双引号包裹的小写混合标识符
    let table = unique_table("or2mixed");
    drop_table_if_exists(&conn, &table);
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
            name: "userName".to_string(), // 混合大小写列名
            sql_type: "VARCHAR2(100)".to_string(),
            nullable: true,
            default: None,
            auto_increment: false,
            primary_key: false,
        },
    ];
    let create_sql = dialect.build_create_table(&table, &columns);
    conn.execute(&create_sql, &[]).expect("create quoted table");

    // 行为断言 1：quote 往返——混合大小写列名插入/查询按原样工作
    let insert_sql = format!(
        "INSERT INTO {} ({}) VALUES (:1)",
        dialect.quote(&table),
        dialect.quote("userName"),
    );
    conn.execute(&insert_sql, &[&"alice"]).expect("insert");
    conn.commit().expect("commit");
    let sel_sql = format!(
        "SELECT {} FROM {} WHERE {} IS NOT NULL",
        dialect.quote("userName"),
        dialect.quote(&table),
        dialect.quote("userName"),
    );
    let v: String = conn
        .query_row_as::<String>(&sel_sql, &[])
        .expect("select mixed case");
    assert_eq!(v, "alice", "混合大小写引号标识符应按原样往返");

    // 行为断言 2：user_tables 按原样（小写）存储引号名
    let n: i64 = conn
        .query_row_as::<i64>(
            "SELECT COUNT(*) FROM user_tables WHERE table_name = :1",
            &[&table],
        )
        .expect("check exact name");
    assert_eq!(n, 1, "引号小写表名应在 user_tables 按【原样】存储");

    // 行为断言 3：元数据探测面——UPPER() 匹配（调用方按大写探测时的正确姿势）
    let n_up: i64 = conn
        .query_row_as::<i64>(
            "SELECT COUNT(*) FROM user_tables WHERE UPPER(table_name) = :1",
            &[&table.to_uppercase()],
        )
        .expect("check upper match");
    assert_eq!(n_up, 1, "UPPER(table_name) 探测应命中");

    // 行为断言 4：ALL_TAB_COLUMNS 元数据列可见性——表名按存储原样匹配，
    // 列名经 UPPER() 折叠探测（引号小写表名的正确探测姿势）
    let n_col: i64 = conn
        .query_row_as::<i64>(
            "SELECT COUNT(*) FROM all_tab_columns WHERE owner = :1 AND table_name = :2 AND UPPER(column_name) = :3",
            &[&"SZ_ORM_TEST", &table, &"USERNAME"],
        )
        .expect("check all_tab_columns");
    assert_eq!(n_col, 1, "ALL_TAB_COLUMNS 探测应命中混合大小写列");

    // 行为断言 5：不带引号的小写名查询不可用（ORA-00942）
    let unquoted_sql = format!("SELECT COUNT(*) FROM {}", table);
    let err = conn
        .query_row_as::<i64>(&unquoted_sql, &[])
        .expect_err("不带引号小写名应折叠为大写而查无此表");
    assert_ora_code(&err, 942, "不带引号小写表名查询");

    drop_table_if_exists(&conn, &table);
}

#[test]
#[ignore = "需要 Oracle 26ai 运行于 127.0.0.1:1521（设置 SZ_ORM_ORACLE_* 环境变量覆盖）"]
fn or2_identifier_length_limit_128() {
    let conn = open_conn();

    // 载荷 1：128 字节引号标识符（12.2+ 上限）——应成功
    let name_128 = format!("a{}", "b".repeat(127)); // 恰好 128 字节
    assert_eq!(name_128.len(), 128);
    let _ = conn.execute(&format!("DROP TABLE \"{}\" PURGE", name_128), &[]);

    conn.execute(&format!("CREATE TABLE \"{}\" (id NUMBER)", name_128), &[])
        .expect("128 字节标识符应被接受（12.2+ 上限）");

    // 行为断言：按原样存储
    let n: i64 = conn
        .query_row_as::<i64>(
            "SELECT COUNT(*) FROM user_tables WHERE table_name = :1",
            &[&name_128],
        )
        .expect("check 128-byte name");
    assert_eq!(n, 1, "128 字节引号标识符应按原样存储");

    conn.execute(&format!("DROP TABLE \"{}\" PURGE", name_128), &[])
        .expect("drop 128");

    // 载荷 2：129 字节——ORA-00972（标识符过长）
    let name_129 = format!("a{}", "b".repeat(128));
    assert_eq!(name_129.len(), 129);
    let err = conn
        .execute(&format!("CREATE TABLE \"{}\" (id NUMBER)", name_129), &[])
        .expect_err("129 字节标识符应被 ORA-00972 拒绝");
    assert_ora_code(&err, 972, "129 字节标识符");
}

#[test]
#[ignore = "需要 Oracle 26ai 运行于 127.0.0.1:1521（设置 SZ_ORM_ORACLE_* 环境变量覆盖）"]
fn or2_add_index_unquoted_mixed_case_and_reserved_columns() {
    let conn = open_conn();
    let dialect = get_dialect(DbType::Oracle).unwrap();

    // ---- 载荷 1：混合大小写列上建索引（方言生成面：AddIndex 列名裸拼接）----
    let table = unique_table("or2idx");
    drop_table_if_exists(&conn, &table);
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
            name: "userName".to_string(), // 混合大小写列（quote 创建，区分大小写存储）
            sql_type: "VARCHAR2(100)".to_string(),
            nullable: true,
            default: None,
            auto_increment: false,
            primary_key: false,
        },
    ];
    let create_sql = dialect.build_create_table(&table, &columns);
    conn.execute(&create_sql, &[]).expect("create table");

    // 方言生成：AddIndex("idx_or2_mixed", ["userName"]) —— 列名未经 quote
    let alter_sql = dialect.build_alter_table(
        &table,
        &[sz_orm_core::dialect::TableChange::AddIndex(
            format!("idx_{}_mixed", table).as_str().to_string(),
            vec!["userName".to_string()],
        )],
    );
    // 真库执行：裸拼 userName 被折叠为 USERNAME，与区分大小写的 "userName" 列不匹配
    let err = conn
        .execute(&alter_sql, &[])
        .expect_err("裸拼混合大小写列建索引应被真库拒绝");
    assert_ora_code(&err, 904, "混合大小写列建索引");

    // ---- 载荷 2：保留字列上建索引（方言生成面同缺口）----
    let rsv_table = unique_table("or2rsv");
    drop_table_if_exists(&conn, &rsv_table);
    let rsv_columns = vec![
        ColumnDef {
            name: "id".to_string(),
            sql_type: "NUMBER".to_string(),
            nullable: false,
            default: None,
            auto_increment: true,
            primary_key: true,
        },
        ColumnDef {
            name: "order".to_string(), // Oracle 保留字列（quote 创建合法）
            sql_type: "NUMBER".to_string(),
            nullable: true,
            default: None,
            auto_increment: false,
            primary_key: false,
        },
    ];
    let rsv_create_sql = dialect.build_create_table(&rsv_table, &rsv_columns);
    conn.execute(&rsv_create_sql, &[])
        .expect("create reserved-word column table");

    // 方言生成：AddIndex 列名裸拼 order（保留字）→ 真库语法层拒绝
    let rsv_alter_sql = dialect.build_alter_table(
        &rsv_table,
        &[sz_orm_core::dialect::TableChange::AddIndex(
            format!("idx_{}_rsv", rsv_table).as_str().to_string(),
            vec!["order".to_string()],
        )],
    );
    let rsv_err = conn
        .execute(&rsv_alter_sql, &[])
        .expect_err("裸拼保留字列建索引应被真库拒绝");
    assert_ora_code(&rsv_err, 936, "保留字列建索引（order 被解析为关键字→缺表达式）");

    // ---- 对照：quote 后的等价 DDL 在真库可执行（修复目标形态）----
    let fixed_sql = format!(
        "CREATE INDEX {} ON {} ({})",
        dialect.quote(&format!("idx_{}_fixed", table)),
        dialect.quote(&table),
        dialect.quote("userName"),
    );
    conn.execute(&fixed_sql, &[]).expect("quote 后建索引应成功");
    let idx_n: i64 = conn
        .query_row_as::<i64>(
            "SELECT COUNT(*) FROM user_indexes WHERE table_name = :1 AND index_name = :2",
            &[&table, &format!("idx_{}_fixed", table)],
        )
        .expect("check index exists");
    assert_eq!(idx_n, 1, "quote 生成的索引应按原样登记");

    drop_table_if_exists(&conn, &table);
    drop_table_if_exists(&conn, &rsv_table);
}

// ============================================================================
// OR3 主键回填 —— IDENTITY 列（12.1+）+ RETURNING INTO 通道 + 序列触发器旧路径
//
// sz-orm 生成面：auto_increment_keyword() = "GENERATED BY DEFAULT AS IDENTITY"
// （dialect.rs:1240）。BY DEFAULT 允许显式 id 插入（应用侧指定主键的合法路径）；
// ALWAYS 拒绝显式 id（ORA-32795）。主键回填通道为
// `INSERT .. RETURNING id INTO :bind`（rust-oracle 0.6.3 out-bind）。
// ============================================================================

#[test]
#[ignore = "需要 Oracle 26ai 运行于 127.0.0.1:1521（设置 SZ_ORM_ORACLE_* 环境变量覆盖）"]
fn or3_identity_by_default_and_returning_into() {
    let conn = open_conn();
    let dialect = get_dialect(DbType::Oracle).unwrap();

    // 方言生成面：build_create_table + auto_increment → GENERATED BY DEFAULT AS IDENTITY
    let table = unique_table("or3ret");
    drop_table_if_exists(&conn, &table);
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
            sql_type: "VARCHAR2(50)".to_string(),
            nullable: true,
            default: None,
            auto_increment: false,
            primary_key: false,
        },
    ];
    let create_sql = dialect.build_create_table(&table, &columns);
    assert!(
        create_sql.contains("GENERATED BY DEFAULT AS IDENTITY"),
        "方言应生成 BY DEFAULT IDENTITY，实际：{create_sql}"
    );
    conn.execute(&create_sql, &[]).expect("create identity table");

    // 载荷 1：RETURNING INTO 主键回填通道（rust-oracle out-bind 往返）
    let insert_sql = format!(
        "INSERT INTO {} ({}) VALUES (:1) RETURNING {} INTO :id",
        dialect.quote(&table),
        dialect.quote("name"),
        dialect.quote("id"),
    );
    let stmt = conn
        .execute(&insert_sql, &[&"row_a", &None::<i64>])
        .expect("insert with returning");
    let returned: Vec<i64> = stmt.returned_values("id").expect("returned ids");
    assert_eq!(returned.len(), 1, "RETURNING INTO 应回填 1 个 id");
    let backfilled = returned[0];
    assert!(backfilled > 0, "IDENTITY 回填 id 应为正整数");

    // 行为断言：回填 id 与库内一致
    let check_sql = format!(
        "SELECT {} FROM {} WHERE {} = :1",
        dialect.quote("name"),
        dialect.quote(&table),
        dialect.quote("id"),
    );
    let name: String = conn
        .query_row_as::<String>(&check_sql, &[&backfilled])
        .expect("check backfilled id");
    assert_eq!(name, "row_a", "回填 id 应能反查到插入行");

    // 载荷 2：连续两次回填 id 递增（IDENTITY 序列推进）
    let stmt2 = conn
        .execute(&insert_sql, &[&"row_b", &None::<i64>])
        .expect("insert 2 with returning");
    let returned2: Vec<i64> = stmt2.returned_values("id").expect("returned ids 2");
    assert_eq!(returned2[0], backfilled + 1, "第二次回填 id 应恰好 +1");
    conn.commit().expect("commit");

    // 载荷 3：BY DEFAULT 语义——显式 id 插入合法
    conn.execute(
        &format!(
            "INSERT INTO {} ({}, {}) VALUES (:1, :2)",
            dialect.quote(&table),
            dialect.quote("id"),
            dialect.quote("name"),
        ),
        &[&9999i64, &"explicit"],
    )
    .expect("BY DEFAULT 应允许显式 id");
    conn.commit().expect("commit explicit");
    let explicit_name: String = conn
        .query_row_as::<String>(&check_sql, &[&9999i64])
        .expect("check explicit id");
    assert_eq!(explicit_name, "explicit");

    drop_table_if_exists(&conn, &table);
}

#[test]
#[ignore = "需要 Oracle 26ai 运行于 127.0.0.1:1521（设置 SZ_ORM_ORACLE_* 环境变量覆盖）"]
fn or3_identity_always_rejects_explicit_id() {
    let conn = open_conn();

    // 对照载荷：GENERATED ALWAYS AS IDENTITY（sz-orm 不生成该形态；语义对照面）
    let table = unique_table("or3always");
    let _ = conn.execute(&format!("DROP TABLE {} PURGE", table), &[]);
    conn.execute(
        &format!(
            "CREATE TABLE {} (id NUMBER GENERATED ALWAYS AS IDENTITY, name VARCHAR2(50))",
            table
        ),
        &[],
    )
    .expect("create always identity table");

    // 不带 id 插入：正常
    conn.execute(
        &format!("INSERT INTO {} (name) VALUES (:1)", table),
        &[&"auto"],
    )
    .expect("insert without id");

    // 显式 id 插入：ORA-32795（cannot insert into a generated always identity column）
    let err = conn
        .execute(
            &format!(
                "INSERT INTO {} (id, name) VALUES (:1, :2)",
                table
            ),
            &[&1i64, &"explicit"],
        )
        .expect_err("ALWAYS 应拒绝显式 id");
    assert_ora_code(&err, 32795, "GENERATED ALWAYS 显式 id 插入");

    conn.commit().expect("commit");
    let _ = conn.execute(&format!("DROP TABLE {} PURGE", table), &[]);
}

#[test]
#[ignore = "需要 Oracle 26ai 运行于 127.0.0.1:1521（设置 SZ_ORM_ORACLE_* 环境变量覆盖）"]
fn or3_sequence_trigger_legacy_path() {
    let conn = open_conn();
    let dialect = get_dialect(DbType::Oracle).unwrap();

    // 旧路径：序列 + BEFORE INSERT 触发器（12.1 之前的主键回填形态）
    // sz-orm 不生成触发器（登记：触发器由调用方/DBA 自建，方言层无生成面）
    let table = unique_table("or3seq");
    let seq = format!("seq_{}", table);
    let trig = format!("trg_{}", table);
    let _ = conn.execute(&format!("DROP TABLE {} PURGE", table), &[]);
    let _ = conn.execute(&format!("DROP SEQUENCE {}", seq), &[]);

    conn.execute(
        &format!(
            "CREATE TABLE {} ({} NUMBER PRIMARY KEY, {} VARCHAR2(50))",
            dialect.quote(&table),
            dialect.quote("id"),
            dialect.quote("name"),
        ),
        &[],
    )
    .expect("create plain table");
    conn.execute(
        &format!("CREATE SEQUENCE {} START WITH 1 INCREMENT BY 1", seq),
        &[],
    )
    .expect("create sequence");
    // 触发器体为 PL/SQL（sz-orm 不生成；此处手工构造以验证旧路径行为）
    // 注意：:NEW 引用的列名必须与建表引号形态一致（":NEW.\"id\""），
    // 未引号 id 会折叠为大写 ID 而与引号小写列不匹配（ORA-04098 重验证失败）
    conn.execute(
        &format!(
            "CREATE OR REPLACE TRIGGER {} BEFORE INSERT ON {} FOR EACH ROW BEGIN IF :NEW.{} IS NULL THEN SELECT {}.NEXTVAL INTO :NEW.{} FROM DUAL; END IF; END;",
            trig,
            dialect.quote(&table),
            dialect.quote("id"),
            seq,
            dialect.quote("id"),
        ),
        &[],
    )
    .expect("create trigger");

    // 行为断言：不带 id 插入由触发器回填
    conn.execute(
        &format!(
            "INSERT INTO {} ({}) VALUES (:1)",
            dialect.quote(&table),
            dialect.quote("name"),
        ),
        &[&"legacy"],
    )
    .expect("insert via trigger");
    conn.commit().expect("commit");
    let id: i64 = conn
        .query_row_as::<i64>(
            &format!("SELECT {} FROM {}", dialect.quote("id"), dialect.quote(&table)),
            &[],
        )
        .expect("read id");
    assert_eq!(id, 1, "触发器旧路径应从序列回填 id=1");

    // 清理（DROP 顺序：触发器/序列 → 表）
    let _ = conn.execute(&format!("DROP TRIGGER {}", trig), &[]);
    let _ = conn.execute(&format!("DROP SEQUENCE {}", seq), &[]);
    drop_table_if_exists(&conn, &table);
}
