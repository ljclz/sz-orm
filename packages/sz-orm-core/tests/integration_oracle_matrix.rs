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

/// 轻量测试表：仅 id IDENTITY 主键 + 可空 value 列（OR4/OR6 排序与 NULL 语义用）
fn create_value_only_table(conn: &OracleConn, table: &str) {
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
            name: "value".to_string(),
            sql_type: "NUMBER".to_string(),
            nullable: true,
            default: None,
            auto_increment: false,
            primary_key: false,
        },
    ];
    let sql = dialect.build_create_table(table, &columns);
    conn.execute(&sql, &[]).expect("create value-only table");
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
fn or2_add_index_mixed_case_and_reserved_columns() {
    let conn = open_conn();
    let dialect = get_dialect(DbType::Oracle).unwrap();

    // ---- 载荷 1：混合大小写列上建索引（方言修复后：索引名/列名均 quote）----
    // 修复前缺口留档（commit 00760ab）：AddIndex 列名裸拼 userName 被折叠为
    // USERNAME 与引号列不匹配 → 真库 ORA-00904
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

    // 方言生成：AddIndex——修复后索引名与列名均 quote
    let idx_name = format!("idx_{}_mixed", table);
    let alter_sql = dialect.build_alter_table(
        &table,
        &[sz_orm_core::dialect::TableChange::AddIndex(
            idx_name.clone(),
            vec!["userName".to_string()],
        )],
    );
    assert!(
        alter_sql.contains(&dialect.quote("userName")),
        "AddIndex 生成应 quote 列名: {alter_sql}"
    );
    // 真库执行：quote 形态应成功
    conn.execute(&alter_sql, &[]).expect("quote 后混合大小写列建索引应成功");
    let idx_n: i64 = conn
        .query_row_as::<i64>(
            "SELECT COUNT(*) FROM user_indexes WHERE table_name = :1 AND index_name = :2",
            &[&table, &idx_name],
        )
        .expect("check index exists");
    assert_eq!(idx_n, 1, "索引应按引号原样登记");

    // ---- 载荷 2：保留字列上建索引（修复后 quote 形态应成功）----
    // 修复前缺口留档：裸拼 order 被解析为关键字 → 真库 ORA-00936
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

    let rsv_idx_name = format!("idx_{}_rsv", rsv_table);
    let rsv_alter_sql = dialect.build_alter_table(
        &rsv_table,
        &[sz_orm_core::dialect::TableChange::AddIndex(
            rsv_idx_name.clone(),
            vec!["order".to_string()],
        )],
    );
    conn.execute(&rsv_alter_sql, &[])
        .expect("quote 后保留字列建索引应成功");
    let rsv_n: i64 = conn
        .query_row_as::<i64>(
            "SELECT COUNT(*) FROM user_indexes WHERE table_name = :1 AND index_name = :2",
            &[&rsv_table, &rsv_idx_name],
        )
        .expect("check reserved index exists");
    assert_eq!(rsv_n, 1, "保留字列索引应成功登记");

    // ---- 生成面 3：DropIndex 往返（与 AddIndex 的 quote 形态一致）----
    let drop_sql = dialect.build_alter_table(
        &table,
        &[sz_orm_core::dialect::TableChange::DropIndex(idx_name.clone())],
    );
    assert!(
        drop_sql.contains(&dialect.quote(&idx_name)),
        "DropIndex 生成应 quote 索引名: {drop_sql}"
    );
    conn.execute(&drop_sql, &[]).expect("quote 后删索引应成功");
    let gone: i64 = conn
        .query_row_as::<i64>(
            "SELECT COUNT(*) FROM user_indexes WHERE table_name = :1 AND index_name = :2",
            &[&table, &idx_name],
        )
        .expect("check index dropped");
    assert_eq!(gone, 0, "DropIndex 往返后索引应不存在");

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

// ============================================================================
// OR4 分页与排序 —— OFFSET/FETCH 成对生成 + NULLS FIRST/LAST 透传
//
// sz-orm 生成面：
//   - build_pagination：`OFFSET n ROWS FETCH NEXT m ROWS ONLY`（dialect.rs:1141）
//   - build_pagination_clause：仅 offset 时省略 FETCH（成对约束，dialect.rs:1163）
//   - NULLS FIRST/LAST：Oracle 支持该语法（与 MySQL 不同），方言透传
// ============================================================================

#[test]
#[ignore = "需要 Oracle 26ai 运行于 127.0.0.1:1521（设置 SZ_ORM_ORACLE_* 环境变量覆盖）"]
fn or4_pagination_clause_offset_only_executes() {
    let conn = open_conn();
    let dialect = get_dialect(DbType::Oracle).unwrap();
    let table = unique_table("or4page");
    drop_table_if_exists(&conn, &table);
    create_value_only_table(&conn, &table);

    // 插入 12 行（value = 0..11）
    let insert_sql = format!(
        "INSERT INTO {} ({}) VALUES (:1)",
        dialect.quote(&table),
        dialect.quote("value"),
    );
    for i in 0..12i64 {
        conn.execute(&insert_sql, &[&i]).expect("insert");
    }
    conn.commit().expect("commit");

    let base = format!(
        "SELECT {} FROM {} ORDER BY {}",
        dialect.quote("value"),
        dialect.quote(&table),
        dialect.quote("value"),
    );

    // 生成面 1：仅 offset（limit=None）——省略 FETCH（成对约束）
    let clause = dialect.build_pagination_clause(None, Some(10usize));
    let sql = format!("{}{}", base, clause.expect("clause"));
    assert!(
        !sql.contains("FETCH"),
        "仅 offset 时不应生成 FETCH，实际：{sql}"
    );
    let rows: Vec<i64> = conn
        .query_as::<i64>(&sql, &[])
        .expect("query offset only")
        .filter_map(|r| r.ok())
        .collect();
    assert_eq!(rows.len(), 2, "OFFSET 10 ROWS 后应剩 2 行");
    assert_eq!((rows[0], rows[1]), (10, 11), "仅 offset 分页应取末尾 2 行");

    // 生成面 2：limit + offset 组合——OFFSET 与 FETCH 成对
    let clause2 = dialect.build_pagination_clause(Some(3usize), Some(2usize));
    let sql2 = format!("{}{}", base, clause2.expect("clause 2"));
    assert!(sql2.contains("OFFSET 2 ROWS"), "sql = {sql2}");
    assert!(sql2.contains("FETCH NEXT 3 ROWS ONLY"), "sql = {sql2}");
    let rows2: Vec<i64> = conn
        .query_as::<i64>(&sql2, &[])
        .expect("query limit offset")
        .filter_map(|r| r.ok())
        .collect();
    assert_eq!(rows2, vec![2, 3, 4], "OFFSET 2 + FETCH 3 应取 [2,3,4]");

    // 生成面 3：limit None + offset None → 不生成子句
    assert!(dialect.build_pagination_clause(None, None).is_none());

    drop_table_if_exists(&conn, &table);
}

#[test]
#[ignore = "需要 Oracle 26ai 运行于 127.0.0.1:1521（设置 SZ_ORM_ORACLE_* 环境变量覆盖）"]
fn or4_nulls_first_last_and_default_null_ordering() {
    let conn = open_conn();
    let dialect = get_dialect(DbType::Oracle).unwrap();
    let table = unique_table("or4nulls");
    drop_table_if_exists(&conn, &table);
    create_value_only_table(&conn, &table);

    // 插入：1、NULL、2（value 列可空）
    let insert_sql = format!(
        "INSERT INTO {} ({}) VALUES (:1)",
        dialect.quote(&table),
        dialect.quote("value"),
    );
    conn.execute(&insert_sql, &[&1i64]).expect("insert 1");
    let null_v: Option<i64> = None;
    conn.execute(&insert_sql, &[&null_v]).expect("insert null");
    conn.execute(&insert_sql, &[&2i64]).expect("insert 2");
    conn.commit().expect("commit");

    let asc_base = format!(
        "SELECT {} FROM {} ORDER BY {}",
        dialect.quote("value"),
        dialect.quote(&table),
        dialect.quote("value"),
    );

    // 行为断言：Oracle 默认 ASC → NULLS LAST（与 PG 一致，与 MySQL NULL 排最前的直觉不同）
    let rows: Vec<Option<i64>> = conn
        .query_as::<Option<i64>>(&asc_base, &[])
        .expect("query asc")
        .filter_map(|r| r.ok())
        .collect();
    assert_eq!(
        rows,
        vec![Some(1), Some(2), None],
        "Oracle ASC 默认 NULLS LAST"
    );

    // 生成面：NULLS FIRST 透传——真库执行 + 行序断言
    let nulls_first_sql = format!(
        "{} NULLS FIRST",
        format!(
            "SELECT {} FROM {} ORDER BY {}",
            dialect.quote("value"),
            dialect.quote(&table),
            dialect.quote("value"),
        )
    );
    let rows_nf: Vec<Option<i64>> = conn
        .query_as::<Option<i64>>(&nulls_first_sql, &[])
        .expect("query nulls first")
        .filter_map(|r| r.ok())
        .collect();
    assert_eq!(
        rows_nf,
        vec![None, Some(1), Some(2)],
        "NULLS FIRST 透传应把 NULL 排最前"
    );

    // 生成面：NULLS LAST 显式——与默认一致
    let desc_nulls_last = format!(
        "SELECT {} FROM {} ORDER BY {} DESC NULLS LAST",
        dialect.quote("value"),
        dialect.quote(&table),
        dialect.quote("value"),
    );
    let rows_dl: Vec<Option<i64>> = conn
        .query_as::<Option<i64>>(&desc_nulls_last, &[])
        .expect("query desc nulls last")
        .filter_map(|r| r.ok())
        .collect();
    assert_eq!(
        rows_dl,
        vec![Some(2), Some(1), None],
        "DESC NULLS LAST 应为 [2,1,NULL]"
    );

    // 行为断言：DESC 默认 → NULLS FIRST（Oracle 默认序：NULL 恒视作最大）
    let desc_default = format!(
        "SELECT {} FROM {} ORDER BY {} DESC",
        dialect.quote("value"),
        dialect.quote(&table),
        dialect.quote("value"),
    );
    let rows_dd: Vec<Option<i64>> = conn
        .query_as::<Option<i64>>(&desc_default, &[])
        .expect("query desc default")
        .filter_map(|r| r.ok())
        .collect();
    assert_eq!(
        rows_dd,
        vec![None, Some(2), Some(1)],
        "Oracle DESC 默认 NULLS FIRST"
    );

    drop_table_if_exists(&conn, &table);
}

// ============================================================================
// OR5 类型往返 —— NUMBER 精度 / VARCHAR2(4000) 边界 / CLOB / RAW / TIMESTAMP
//
// 方言生成面：map_to_oracle_type（dialect.rs:1060~）TEXT→CLOB、
// BOOLEAN→NUMBER(1)、INTEGER→NUMBER(10)、VARCHAR→VARCHAR2。
// 本块验证真库往返不丢位/不截断/不漂移。
// ============================================================================

#[test]
#[ignore = "需要 Oracle 26ai 运行于 127.0.0.1:1521（设置 SZ_ORM_ORACLE_* 环境变量覆盖）"]
fn or5_number_precision_roundtrip() {
    let conn = open_conn();
    let dialect = get_dialect(DbType::Oracle).unwrap();
    let table = unique_table("or5num");
    drop_table_if_exists(&conn, &table);

    // NUMBER(38,0)：38 位整数精度上限
    conn.execute(
        &format!("CREATE TABLE {} ({} NUMBER(38,0), {} NUMBER(10,2))",
            dialect.quote(&table),
            dialect.quote("big_num"),
            dialect.quote("dec_num"),
        ),
        &[],
    )
    .expect("create number table");

    // 载荷 1：30 位整数（超 i64 范围）——经字符串绑定 + TO_NUMBER 往返，TO_CHAR 读回逐字节比对
    let big_str = "123456789012345678901234567890"; // 30 位
    conn.execute(
        &format!(
            "INSERT INTO {} ({}) VALUES (TO_NUMBER(:1))",
            dialect.quote(&table),
            dialect.quote("big_num"),
        ),
        &[&big_str],
    )
    .expect("insert big number");
    conn.commit().expect("commit");
    let back: String = conn
        .query_row_as::<String>(
            &format!(
                "SELECT TO_CHAR({}) FROM {}",
                dialect.quote("big_num"),
                dialect.quote(&table)
            ),
            &[],
        )
        .expect("read big number");
    assert_eq!(back, big_str, "30 位整数 NUMBER 往返不应丢位");

    // 载荷 2：i64::MAX 参数化往返（rust-oracle i64 绑定通道）
    let i64_max: i64 = i64::MAX;
    conn.execute(
        &format!(
            "INSERT INTO {} ({}) VALUES (:1)",
            dialect.quote(&table),
            dialect.quote("big_num"),
        ),
        &[&i64_max],
    )
    .expect("insert i64 max");
    conn.commit().expect("commit i64");
    let back_i64: i64 = conn
        .query_row_as::<i64>(
            &format!(
                "SELECT {} FROM {} WHERE {} = :1",
                dialect.quote("big_num"),
                dialect.quote(&table),
                dialect.quote("big_num"),
            ),
            &[&i64_max],
        )
        .expect("read i64 max");
    assert_eq!(back_i64, i64::MAX, "i64::MAX 参数化往返应无损");

    // 载荷 3：NUMBER(10,2) 定点舍入——插入 123.456 存储舍入为 123.46（Oracle 四舍五入）
    conn.execute(
        &format!(
            "INSERT INTO {} ({}) VALUES (:1)",
            dialect.quote(&table),
            dialect.quote("dec_num"),
        ),
        &[&123.456f64],
    )
    .expect("insert decimal");
    conn.commit().expect("commit dec");
    let back_dec: String = conn
        .query_row_as::<String>(
            &format!(
                "SELECT TO_CHAR({}) FROM {} WHERE {} IS NOT NULL AND {} IS NULL",
                dialect.quote("dec_num"),
                dialect.quote(&table),
                dialect.quote("dec_num"),
                dialect.quote("big_num"),
            ),
            &[],
        )
        .expect("read decimal");
    assert_eq!(back_dec, "123.46", "NUMBER(10,2) 应存储舍入为 123.46");

    drop_table_if_exists(&conn, &table);
}

#[test]
#[ignore = "需要 Oracle 26ai 运行于 127.0.0.1:1521（设置 SZ_ORM_ORACLE_* 环境变量覆盖）"]
fn or5_varchar2_4000_boundary() {
    let conn = open_conn();
    let dialect = get_dialect(DbType::Oracle).unwrap();
    let table = unique_table("or5var");
    drop_table_if_exists(&conn, &table);

    conn.execute(
        &format!(
            "CREATE TABLE {} ({} VARCHAR2(4000))",
            dialect.quote(&table),
            dialect.quote("payload"),
        ),
        &[],
    )
    .expect("create varchar table");

    // 载荷 1：恰好 4000 字符——参数化绑定应成功且逐字节往返
    let payload_4000 = "x".repeat(4000);
    conn.execute(
        &format!(
            "INSERT INTO {} ({}) VALUES (:1)",
            dialect.quote(&table),
            dialect.quote("payload"),
        ),
        &[&payload_4000],
    )
    .expect("insert 4000 chars");
    conn.commit().expect("commit");
    let back: String = conn
        .query_row_as::<String>(
            &format!(
                "SELECT {} FROM {}",
                dialect.quote("payload"),
                dialect.quote(&table)
            ),
            &[],
        )
        .expect("read 4000");
    assert_eq!(back.len(), 4000, "4000 字符应完整往返");

    // 载荷 2：4001 字符——ORA-12899（值大于列精度）
    let payload_4001 = "x".repeat(4001);
    let err = conn
        .execute(
            &format!(
                "INSERT INTO {} ({}) VALUES (:1)",
                dialect.quote(&table),
                dialect.quote("payload"),
            ),
            &[&payload_4001],
        )
        .expect_err("4001 字符应被 ORA-12899 拒绝");
    assert_ora_code(&err, 12899, "VARCHAR2(4000) 溢出");

    drop_table_if_exists(&conn, &table);
}

#[test]
#[ignore = "需要 Oracle 26ai 运行于 127.0.0.1:1521（设置 SZ_ORM_ORACLE_* 环境变量覆盖）"]
fn or5_clob_raw_timestamp_roundtrip() {
    use oracle::sql_type::Timestamp;
    let conn = open_conn();
    let dialect = get_dialect(DbType::Oracle).unwrap();
    let table = unique_table("or5lob");
    drop_table_if_exists(&conn, &table);

    conn.execute(
        &format!(
            "CREATE TABLE {} ({} CLOB, {} RAW(16), {} TIMESTAMP(6))",
            dialect.quote(&table),
            dialect.quote("doc"),
            dialect.quote("fingerprint"),
            dialect.quote("ts"),
        ),
        &[],
    )
    .expect("create lob table");

    // 载荷 1：CLOB 10000 字符（方言映射面：TEXT→CLOB）参数化往返
    let doc: String = (0..1000).map(|i| format!("[seg-{}]", i % 97)).collect();
    assert!(doc.len() > 4000, "载荷应超 VARCHAR2 边界以证 CLOB 路径");
    conn.execute(
        &format!(
            "INSERT INTO {} ({}) VALUES (:1)",
            dialect.quote(&table),
            dialect.quote("doc"),
        ),
        &[&doc],
    )
    .expect("insert clob");
    conn.commit().expect("commit clob");
    let back_doc: String = conn
        .query_row_as::<String>(
            &format!(
                "SELECT {} FROM {}",
                dialect.quote("doc"),
                dialect.quote(&table)
            ),
            &[],
        )
        .expect("read clob");
    assert_eq!(back_doc, doc, "CLOB 10000 字符应逐字节往返");

    // 载荷 2：RAW(16) 二进制往返（Vec<u8> 绑定）
    let raw_bytes: Vec<u8> = (0u8..=15).collect();
    conn.execute(
        &format!(
            "INSERT INTO {} ({}) VALUES (:1)",
            dialect.quote(&table),
            dialect.quote("fingerprint"),
        ),
        &[&raw_bytes],
    )
    .expect("insert raw");
    conn.commit().expect("commit raw");
    let back_raw: Vec<u8> = conn
        .query_row_as::<Vec<u8>>(
            &format!(
                "SELECT {} FROM {} WHERE {} IS NOT NULL",
                dialect.quote("fingerprint"),
                dialect.quote(&table),
                dialect.quote("fingerprint"),
            ),
            &[],
        )
        .expect("read raw");
    assert_eq!(back_raw, raw_bytes, "RAW(16) 二进制应逐字节往返");

    // 载荷 3：TIMESTAMP(6) 亚秒精度往返（oracle::sql_type::Timestamp 绑定）
    let ts = Timestamp::new(2026, 11, 11, 10, 30, 15, 123_456_000).expect("ts");
    conn.execute(
        &format!(
            "INSERT INTO {} ({}) VALUES (:1)",
            dialect.quote(&table),
            dialect.quote("ts"),
        ),
        &[&ts],
    )
    .expect("insert timestamp");
    conn.commit().expect("commit ts");
    let back_ts: Timestamp = conn
        .query_row_as::<Timestamp>(
            &format!(
                "SELECT {} FROM {} WHERE {} IS NOT NULL",
                dialect.quote("ts"),
                dialect.quote(&table),
                dialect.quote("ts"),
            ),
            &[],
        )
        .expect("read timestamp");
    assert_eq!(back_ts.year(), 2026);
    assert_eq!(back_ts.month(), 11);
    assert_eq!(back_ts.day(), 11);
    assert_eq!(back_ts.hour(), 10);
    assert_eq!(back_ts.minute(), 30);
    assert_eq!(back_ts.second(), 15);
    assert_eq!(
        back_ts.nanosecond(),
        123_456_000,
        "TIMESTAMP(6) 亚秒精度应往返到微秒"
    );

    drop_table_if_exists(&conn, &table);
}

// ============================================================================
// OR6 边缘语义 —— 三值 NULL / ORA-01476 除零 / 隐式转换 / DUAL / DDL 隐式提交
//
// 关键探针：Oracle DDL 语句（CREATE/ALTER/DROP）会**隐式提交**当前事务——
// 与 MySQL/PG 可回滚 DDL 重大差异。对 sz-orm 事务管理器的影响：migration/
// schema_sync 在事务中执行 DDL 时，此前未提交的 DML 一并落库且不可回滚。
// ============================================================================

#[test]
#[ignore = "需要 Oracle 26ai 运行于 127.0.0.1:1521（设置 SZ_ORM_ORACLE_* 环境变量覆盖）"]
fn or6_three_valued_null_and_division_by_zero() {
    let conn = open_conn();
    let dialect = get_dialect(DbType::Oracle).unwrap();
    let table = unique_table("or6null");
    drop_table_if_exists(&conn, &table);
    create_value_only_table(&conn, &table);

    // 插入 1 行 value=NULL、1 行 value=1
    let insert_sql = format!(
        "INSERT INTO {} ({}) VALUES (:1)",
        dialect.quote(&table),
        dialect.quote("value"),
    );
    let null_v: Option<i64> = None;
    conn.execute(&insert_sql, &[&null_v]).expect("insert null");
    conn.execute(&insert_sql, &[&1i64]).expect("insert 1");
    conn.commit().expect("commit");

    // 行为断言：= NULL 恒空（三值逻辑）——即使库中确有 NULL 行
    let eq_null_sql = format!(
        "SELECT COUNT(*) FROM {} WHERE {} = :1",
        dialect.quote(&table),
        dialect.quote("value"),
    );
    let n: i64 = conn
        .query_row_as::<i64>(&eq_null_sql, &[&null_v])
        .expect("count eq null");
    assert_eq!(n, 0, "value = NULL 应恒命中 0 行（三值逻辑，须用 IS NULL）");

    // 行为断言：NOT IN 含 NULL 时全空（经典三值陷阱）
    let not_in_sql = format!(
        "SELECT COUNT(*) FROM {} WHERE {} NOT IN (:1, :2)",
        dialect.quote(&table),
        dialect.quote("value"),
    );
    let n2: i64 = conn
        .query_row_as::<i64>(&not_in_sql, &[&1i64, &null_v])
        .expect("count not in");
    assert_eq!(n2, 0, "NOT IN 含 NULL 时应恒空");

    // 行为断言：除零 ORA-01476 报错（无 MySQL sql_mode 类开关，不可配置宽容）
    let div_sql = "SELECT 1 / 0 FROM DUAL";
    let err = conn
        .query_row_as::<i64>(div_sql, &[])
        .expect_err("除零应报 ORA-01476");
    assert_ora_code(&err, 1476, "除零（无 sql_mode 类宽容开关）");

    drop_table_if_exists(&conn, &table);
}

#[test]
#[ignore = "需要 Oracle 26ai 运行于 127.0.0.1:1521（设置 SZ_ORM_ORACLE_* 环境变量覆盖）"]
fn or6_implicit_conversion_varchar2_number() {
    let conn = open_conn();
    let dialect = get_dialect(DbType::Oracle).unwrap();
    let table = unique_table("or6conv");
    drop_table_if_exists(&conn, &table);

    // 纯数字 VARCHAR2 表（全部行可隐式转换）
    conn.execute(
        &format!("CREATE TABLE {} ({} VARCHAR2(20))", dialect.quote(&table), dialect.quote("v")),
        &[],
    )
    .expect("create conv table");
    let insert_sql = format!(
        "INSERT INTO {} ({}) VALUES (:1)",
        dialect.quote(&table),
        dialect.quote("v"),
    );
    conn.execute(&insert_sql, &[&"123"]).expect("insert 123");
    conn.commit().expect("commit");

    // 行为断言：VARCHAR2 数字列与 NUMBER 绑定比较——隐式 TO_NUMBER 转换命中
    let hit_sql = format!(
        "SELECT COUNT(*) FROM {} WHERE {} = :1",
        dialect.quote(&table),
        dialect.quote("v"),
    );
    let n: i64 = conn
        .query_row_as::<i64>(&hit_sql, &[&123i64])
        .expect("implicit conversion hit");
    assert_eq!(n, 1, "VARCHAR2 '123' 与 NUMBER 123 比较应经隐式转换命中");

    // 载荷：混入不可转换行 'abc' → 同查询全表扫描触发 ORA-01722
    conn.execute(&insert_sql, &[&"abc"]).expect("insert abc");
    conn.commit().expect("commit abc");
    let err = conn
        .query_row_as::<i64>(&hit_sql, &[&123i64])
        .expect_err("含不可转换行应报 ORA-01722");
    assert_ora_code(&err, 1722, "VARCHAR2→NUMBER 隐式转换失败");

    drop_table_if_exists(&conn, &table);
}

#[test]
#[ignore = "需要 Oracle 26ai 运行于 127.0.0.1:1521（设置 SZ_ORM_ORACLE_* 环境变量覆盖）"]
fn or6_dual_and_fromless_select_semantics() {
    let conn = open_conn();

    // 行为断言 1：DUAL 表可执行标量 SELECT（传统形态，向后兼容保持）
    let v: i64 = conn
        .query_row_as::<i64>("SELECT :1 FROM DUAL", &[&42i64])
        .expect("select from dual");
    assert_eq!(v, 42, "SELECT :1 FROM DUAL 应正常执行");

    // 行为断言 2：FROM-less SELECT 在 23ai+ 已原生支持（提示词假设过时——实测修正）
    // Oracle Database 23ai 新特性：标量查询无需 FROM 子句（此前必须 FROM DUAL，
    // 老版本 19c/21c 报 ORA-00923）。26ai（23.26.3.0.0）实测直接返回。
    let fromless: i64 = conn
        .query_row_as::<i64>("SELECT 1", &[])
        .expect("FROM-less SELECT 应在 23ai+ 原生支持");
    assert_eq!(fromless, 1, "SELECT 1（无 FROM）应返回 1");

    // 行为断言 3：FROM-less 参数化标量查询同样可用
    let fromless_bind: i64 = conn
        .query_row_as::<i64>("SELECT :1", &[&99i64])
        .expect("FROM-less 参数化");
    assert_eq!(fromless_bind, 99);
}

#[test]
#[ignore = "需要 Oracle 26ai 运行于 127.0.0.1:1521（设置 SZ_ORM_ORACLE_* 环境变量覆盖）"]
fn or6_ddl_implicit_commit_probe() {
    // 核心探针：事务中执行 DDL → 隐式提交此前未提交的 DML → ROLLBACK 无法回滚。
    // 对 sz-orm 事务管理器的影响（登记项）：migration/schema_sync 在事务中
    // 混合 DDL+DML 时，Oracle 上 DML 部分「已提交」，与 MySQL/PG 可回滚 DDL 语义相反。
    let conn = open_conn();
    let dialect = get_dialect(DbType::Oracle).unwrap();
    let table = unique_table("or6ddl");
    drop_table_if_exists(&conn, &table);
    create_value_only_table(&conn, &table);

    // DDL 载荷：事务中途要建的辅助表
    let aux_table = unique_table("or6aux");
    drop_table_if_exists(&conn, &aux_table);

    // 步骤 1：未提交 DML
    let insert_sql = format!(
        "INSERT INTO {} ({}) VALUES (:1)",
        dialect.quote(&table),
        dialect.quote("value"),
    );
    conn.execute(&insert_sql, &[&777i64]).expect("insert uncommitted");

    // 步骤 2：事务中途 DDL（Oracle 隐式提交点）
    let create_aux_sql = dialect.build_create_table(
        &aux_table,
        &[ColumnDef {
            name: "id".to_string(),
            sql_type: "NUMBER".to_string(),
            nullable: false,
            default: None,
            auto_increment: false,
            primary_key: true,
        }],
    );
    conn.execute(&create_aux_sql, &[]).expect("ddl in transaction");

    // 步骤 3：ROLLBACK —— 若 DDL 隐式提交，此回滚无法撤销步骤 1 的插入
    conn.rollback().expect("rollback");

    // 行为断言（第二连接交叉验证）：插入行已随 DDL 提交，回滚无效
    let conn2 = open_conn();
    let count_sql = format!(
        "SELECT COUNT(*) FROM {} WHERE {} = :1",
        dialect.quote(&table),
        dialect.quote("value"),
    );
    let n: i64 = conn2
        .query_row_as::<i64>(&count_sql, &[&777i64])
        .expect("count via second conn");
    assert_eq!(
        n, 1,
        "DDL 隐式提交后 ROLLBACK 不应撤销先前 DML（第二连接可见 1 行）"
    );

    drop_table_if_exists(&conn, &aux_table);
    drop_table_if_exists(&conn, &table);
}
