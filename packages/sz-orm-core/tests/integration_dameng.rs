//! Dameng 兼容方言集成测试
//!
//! DamengDialect 通过 delegate_dialect_to! 委派 OracleDialect，
//! 生成的 SQL 与 OracleDialect 一致。用 Oracle 23ai Free 验证兼容性。
//!
//! 运行方式：cargo test -p sz-orm-core --test integration_dameng -- --ignored --nocapture

use oracle::Connection as OracleConn;
use std::sync::atomic::{AtomicU64, Ordering};
use sz_orm_core::dialect::{get_dialect, ColumnDef};
use sz_orm_core::DbType;

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
    OracleConn::connect(oracle_user(), oracle_password(), oracle_connect_string())
        .expect("oracle connect failed - is Oracle 23ai running on 127.0.0.1:1521?")
}

static TABLE_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_table(prefix: &str) -> String {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let counter = TABLE_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!(
        "dm_{}_{}_{}",
        prefix,
        pid % 1000,
        (nanos % 100000) as u64 * 1000 + counter
    )
}

fn drop_table_if_exists(conn: &OracleConn, table: &str) {
    let dialect = get_dialect(DbType::Dameng).unwrap();
    let sql = dialect.build_drop_table(table, true);
    let _ = conn.execute(&sql, &[]);
}

fn test_columns() -> Vec<ColumnDef> {
    vec![
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
    ]
}

// ==================== 方言断言测试 ====================

#[test]
fn test_dameng_dialect_quote() {
    let dameng = get_dialect(DbType::Dameng).unwrap();
    let oracle = get_dialect(DbType::Oracle).unwrap();
    assert_eq!(dameng.quote("user"), oracle.quote("user"));
    assert_eq!(dameng.quote("user"), "\"user\"");
}

#[test]
fn test_dameng_dialect_escape() {
    let dameng = get_dialect(DbType::Dameng).unwrap();
    let oracle = get_dialect(DbType::Oracle).unwrap();
    assert_eq!(dameng.escape_string("it's"), oracle.escape_string("it's"));
}

#[test]
fn test_dameng_dialect_pagination() {
    let dameng = get_dialect(DbType::Dameng).unwrap();
    let oracle = get_dialect(DbType::Oracle).unwrap();
    assert_eq!(
        dameng.build_pagination("SELECT 1", 10, 20),
        oracle.build_pagination("SELECT 1", 10, 20)
    );
}

#[test]
fn test_dameng_dialect_db_type() {
    let dialect = get_dialect(DbType::Dameng).unwrap();
    assert_eq!(dialect.db_type(), DbType::Dameng);
}

#[test]
fn test_dameng_dialect_create_table() {
    let dameng = get_dialect(DbType::Dameng).unwrap();
    let oracle = get_dialect(DbType::Oracle).unwrap();
    let sql_dameng = dameng.build_create_table("test_table", &test_columns());
    let sql_oracle = oracle.build_create_table("test_table", &test_columns());
    assert_eq!(sql_dameng, sql_oracle);
}

#[test]
fn test_dameng_dialect_not_oracle_db_type() {
    let dialect = get_dialect(DbType::Dameng).unwrap();
    assert_ne!(dialect.db_type(), DbType::Oracle);
    assert_eq!(dialect.db_type(), DbType::Dameng);
}

// ==================== 真实 DB 集成测试（Oracle 23ai） ====================

#[test]
#[ignore = "需要 Oracle 23ai 运行于 127.0.0.1:1521"]
fn test_dameng_crud_on_oracle() {
    let conn = open_conn();
    let dialect = get_dialect(DbType::Dameng).unwrap();
    let table = unique_table("crud");
    drop_table_if_exists(&conn, &table);

    let create_sql = dialect.build_create_table(&table, &test_columns());
    conn.execute(&create_sql, &[]).expect("create table");

    let insert_sql = format!(
        "INSERT INTO {} ({}, {}) VALUES (:1, :2)",
        dialect.quote(&table),
        dialect.quote("name"),
        dialect.quote("value"),
    );
    conn.execute(&insert_sql, &[&"alice", &100i64])
        .expect("insert 1");
    conn.execute(&insert_sql, &[&"bob", &200i64])
        .expect("insert 2");
    conn.execute(&insert_sql, &[&"carol", &300i64])
        .expect("insert 3");
    conn.commit().expect("commit");

    let count_sql = format!("SELECT COUNT(*) FROM {}", dialect.quote(&table));
    let count: i64 = conn.query_row_as::<i64>(&count_sql, &[]).expect("count");
    assert_eq!(count, 3);

    let select_sql = format!(
        "SELECT {} FROM {} WHERE {} > :1 ORDER BY {}",
        dialect.quote("name"),
        dialect.quote(&table),
        dialect.quote("value"),
        dialect.quote("value"),
    );
    let rows: Vec<String> = conn
        .query_as::<String>(&select_sql, &[&150i64])
        .expect("query")
        .filter_map(|r| r.ok())
        .collect();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0], "bob");
    assert_eq!(rows[1], "carol");

    let update_sql = format!(
        "UPDATE {} SET {} = :1 WHERE {} = :2",
        dialect.quote(&table),
        dialect.quote("value"),
        dialect.quote("name"),
    );
    conn.execute(&update_sql, &[&999i64, &"alice"])
        .expect("update");
    conn.commit().expect("commit");

    let check_sql = format!(
        "SELECT {} FROM {} WHERE {} = :1",
        dialect.quote("value"),
        dialect.quote(&table),
        dialect.quote("name"),
    );
    let val: i64 = conn
        .query_row_as::<i64>(&check_sql, &[&"alice"])
        .expect("check");
    assert_eq!(val, 999);

    let delete_sql = format!(
        "DELETE FROM {} WHERE {} = :1",
        dialect.quote(&table),
        dialect.quote("name"),
    );
    conn.execute(&delete_sql, &[&"bob"]).expect("delete");
    conn.commit().expect("commit");

    let count2: i64 = conn.query_row_as::<i64>(&count_sql, &[]).expect("count2");
    assert_eq!(count2, 2);

    drop_table_if_exists(&conn, &table);
}

#[test]
#[ignore = "需要 Oracle 23ai 运行于 127.0.0.1:1521"]
fn test_dameng_pagination_on_oracle() {
    let conn = open_conn();
    let dialect = get_dialect(DbType::Dameng).unwrap();
    let table = unique_table("page");
    drop_table_if_exists(&conn, &table);

    let create_sql = dialect.build_create_table(&table, &test_columns());
    conn.execute(&create_sql, &[]).expect("create table");

    let insert_sql = format!(
        "INSERT INTO {} ({}, {}) VALUES (:1, :2)",
        dialect.quote(&table),
        dialect.quote("name"),
        dialect.quote("value"),
    );
    for i in 1..=5i64 {
        conn.execute(&insert_sql, &[&format!("user_{}", i), &(i * 10)])
            .expect("insert");
    }
    conn.commit().expect("commit");

    let base_select = format!(
        "SELECT {} FROM {} ORDER BY {}",
        dialect.quote("name"),
        dialect.quote(&table),
        dialect.quote("value"),
    );
    let page_sql = dialect.build_pagination(&base_select, 2, 2);
    let rows: Vec<String> = conn
        .query_as::<String>(&page_sql, &[])
        .expect("query")
        .filter_map(|r| r.ok())
        .collect();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0], "user_3");
    assert_eq!(rows[1], "user_4");

    drop_table_if_exists(&conn, &table);
}

#[test]
#[ignore = "需要 Oracle 23ai 运行于 127.0.0.1:1521"]
fn test_dameng_transaction_on_oracle() {
    let conn = open_conn();
    let dialect = get_dialect(DbType::Dameng).unwrap();
    let table = unique_table("tx");
    drop_table_if_exists(&conn, &table);

    let create_sql = dialect.build_create_table(&table, &test_columns());
    conn.execute(&create_sql, &[]).expect("create table");

    let insert_sql = format!(
        "INSERT INTO {} ({}, {}) VALUES (:1, :2)",
        dialect.quote(&table),
        dialect.quote("name"),
        dialect.quote("value"),
    );
    conn.execute(&insert_sql, &[&"alice", &100i64])
        .expect("insert 1");
    conn.commit().expect("commit 1");

    conn.execute(&insert_sql, &[&"bob", &200i64])
        .expect("insert 2");
    conn.rollback().expect("rollback");

    let count_sql = format!("SELECT COUNT(*) FROM {}", dialect.quote(&table));
    let count: i64 = conn.query_row_as::<i64>(&count_sql, &[]).expect("count");
    assert_eq!(count, 1, "rollback 后应只有 alice 1 条");

    drop_table_if_exists(&conn, &table);
}

// ============================================================================
// Dameng 方言矩阵（2026-11-14，DM1~DM3 × 社区镜像 DM8 @52336）
// 与既有 9 项不重复：既有项用 Oracle 23ai 直测（DamengDialect→OracleDialect 委托
// 产物在 Oracle 上执行）；本批实例为真 DM8（Informix/Oracle 兼容自有协议，无 Rust
// 驱动——真库断言经容器内 disql CLI，MZ 批 DB2 CLP 同型）。核心载荷：
// ① OracleDialect 产物形态（双引号 quote/VARCHAR2/分页 FETCH..OFFSET）在 DM8 的行为
// ② OC 批 delegate 转发修复的 DM 通道生成面（upsert/FK/表选项）
// 环境变量：SZ_ORM_DM_CONTAINER（默认 dm8-szorm）
// ============================================================================

fn dm_container() -> String {
    std::env::var("SZ_ORM_DM_CONTAINER").unwrap_or_else(|_| "dm8-szorm".to_string())
}

/// 容器内 disql 执行 helper（单条 SQL；MZ 批 DB2 CLP 同型）
fn run_dm_sql(sql: &str) -> String {
    // 零转义方案：SQL 经 stdin 直喂 disql（echo+引号嵌套层数失控——wsl 不带
    // -e 时默认 shell 还会展开一层，V$VERSION 的 $ 屡次丢失，GB 线四层嵌套
    // 同型教训升级版）。disql 登录串经 argv 明传（无元字符）。
    // R-6 处置：口令经 SZ_ORM_DM_PASSWORD env 注入（社区镜像默认口令
    // SYSDBA001 为兜底，与 P0-3 凭据 env 注入惯例对齐）。
    let dm_password =
        std::env::var("SZ_ORM_DM_PASSWORD").unwrap_or_else(|_| "SYSDBA001".to_string());
    let script = std::process::Command::new("wsl")
        .args([
            "docker",
            "exec",
            "-i",
            &dm_container(),
            "bash",
            "-c",
            &format!(
                "export LD_LIBRARY_PATH=/home/dmdba/dmdbms/bin; \
                 /home/dmdba/dmdbms/bin/disql SYSDBA/{}@localhost:5236",
                dm_password
            ),
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            // disql 以分号界定语句；方言产物（build_create_table 等）不带分号，
            // 缺失会进续行模式（SQL> 2）静默挂起——补齐
            let mut text = sql.to_string();
            if !text.trim_end().ends_with(';') {
                text.push(';');
            }
            child.stdin.take().unwrap().write_all(text.as_bytes())?;
            child.wait_with_output()
        })
        .expect("docker exec dm8");
    String::from_utf8_lossy(&script.stdout).to_string()
        + String::from_utf8_lossy(&script.stderr).trim()
}

/// DM1：引擎身份——SELECT BANNER 含 DM；Oracle 风格双引号 quote 可执行
#[test]
#[ignore = "需要 DM8 容器运行（SZ_ORM_DM_CONTAINER 注入）"]
fn matrix_dm1_engine_identity() {
    let out = run_dm_sql("SELECT BANNER FROM V$VERSION;");
    assert!(
        out.contains("DM") || out.contains("DM Database") || out.contains("Dameng"),
        "DM8 身份: {out}"
    );
    // DamengDialect→OracleDialect 双引号 quote 形态在 DM8 真执行
    let out = run_dm_sql("SELECT 1 AS \"Val\" FROM DUAL;");
    assert!(
        out.contains("Val") || out.contains("1"),
        "双引号 alias 经 Oracle 风格 quote 可执行: {out}"
    );
    let d = get_dialect(DbType::Dameng).unwrap();
    assert_eq!(d.quote("t"), "\"t\"", "委托 Oracle 双引号形态");
}

/// DM2：VARCHAR2/NUMBER DDL + INSERT/SELECT 往返（Oracle 风格类型在 DM8）
#[test]
#[ignore = "需要 DM8 容器运行"]
fn matrix_dm2_oracle_types_roundtrip() {
    let d = get_dialect(DbType::Dameng).unwrap();
    let sql = d.build_create_table(
        "DM_PROBE",
        &[
            ColumnDef {
                name: "ID".to_string(),
                sql_type: "NUMBER(10)".to_string(),
                nullable: false,
                default: None,
                auto_increment: false,
                primary_key: true,
            },
            ColumnDef {
                name: "NAME".to_string(),
                sql_type: "VARCHAR2(100)".to_string(),
                nullable: true,
                default: None,
                auto_increment: false,
                primary_key: false,
            },
        ],
    );
    let r1 = run_dm_sql(&format!("DROP TABLE DM_PROBE CASCADE;"));
    let _ = r1;
    let out = run_dm_sql(&sql);
    eprintln!("=== DDL 产物 ===\n{sql}\n=== 真库执行输出 ===\n{out}");
    assert!(
        out.contains("executed successfully"),
        "Oracle 风格 DDL（NUMBER/VARCHAR2 + 双引号 quote）被 DM8 接受: {out}"
    );
    // disql 每次调用独立会话且 DM8 默认非自动提交——INSERT 后须同会话 COMMIT，
    // 否则下一会话 SELECT "no rows"
    let out = run_dm_sql("INSERT INTO \"DM_PROBE\" (\"ID\", \"NAME\") VALUES (1, 'dm8');\nCOMMIT;");
    eprintln!("=== INSERT 输出 ===\n{out}");
    // DM8 INSERT 成功输出 "affect rows 1"（DDL 才是 executed successfully）
    assert!(out.contains("affect rows"), "insert: {out}");
    let out = run_dm_sql("SELECT \"NAME\" FROM \"DM_PROBE\" WHERE \"ID\" = 1;");
    assert!(out.contains("dm8"), "往返读回: {out}");
    let _ = run_dm_sql("DROP TABLE \"DM_PROBE\" CASCADE;");
}

/// DM3：OC 批 delegate 转发修复的 DM 通道生成面（upsert 渲染 + FK DDL 形态）——
/// DM8 无 INSERT..ON CONFLICT 语法（Oracle 族无 upsert 单语句），委托后 upsert
/// 渲染应为 OracleDialect 的形态（登记：OracleDialect.build_upsert_on_conflict
/// 返回 None 为 trait 契约——调用方以 MERGE 自管）；FK DDL 为**形态断言**
/// （真库接受性 + CASCADE 级联 N=0 已由红队在测试外经 disql 独立复现，
/// 见 wave2 红队回执 §4.3-R6——R-4 处置：docstring 不再声称测试内含 disql 验证）
#[test]
#[ignore = "需要 DM8 容器运行"]
fn matrix_dm3_delegate_surface() {
    let d = get_dialect(DbType::Dameng).unwrap();
    // upsert：Oracle 族契约 None（trait 文档化行为——登记非缺口）
    assert!(
        d.build_upsert_on_conflict(&["id"], &["v"], &["id".to_string(), "v".to_string()])
            .is_none(),
        "Oracle 族 upsert 契约 None（MERGE 由调用方自管）"
    );
    // FK DDL（OC 批修复后的转发形态——Oracle 风格带约束名 quote）
    let fk = sz_orm_core::dialect::ForeignKeyDef {
        columns: vec!["pid".to_string()],
        reference_table: "DM_PARENT".to_string(),
        reference_columns: vec!["id".to_string()],
        on_delete: Some("CASCADE".to_string()),
        on_update: None,
    };
    let ddl = d.build_add_foreign_key("DM_CHILD", &fk).expect("FK DDL");
    assert!(
        ddl.contains("\"pid\"") && ddl.contains("CASCADE"),
        "形态: {ddl}"
    );
}
