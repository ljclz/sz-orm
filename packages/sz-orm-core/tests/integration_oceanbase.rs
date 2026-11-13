//! OceanBase 兼容方言集成测试
//!
//! OceanBase 使用 delegate_dialect_to! 委托 MySqlDialect，
//! 生成的 SQL 与 MySqlDialect 一致。真连验证：OceanBase CE 4.4.2.1（MySQL 模式，
//! 2026-11-13 方言矩阵批）；`matrix_oc_*` 为矩阵载荷，既有 8 项为基线。
//!
//! 运行方式：cargo test -p sz-orm-core --test integration_oceanbase -- --ignored --nocapture
//! 环境变量：SZ_ORM_MYSQL_URL（例：mysql://root%40szorm:Szorm2026@127.0.0.1:12881/sz_orm_test）
//! 注意：daemon 容器约 2min 退出重启循环，跑前须守门探活（见报告 §1）。

use sqlx::mysql::{MySqlPool, MySqlPoolOptions};
use sqlx::Row;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use sz_orm_core::dialect::{get_dialect, ColumnDef};
use sz_orm_core::DbType;

const MYSQL_URL_DEFAULT: &str = "mysql://root:test123@127.0.0.1:3306/sz_orm_test";

fn mysql_url() -> String {
    std::env::var("SZ_ORM_MYSQL_URL").unwrap_or_else(|_| MYSQL_URL_DEFAULT.to_string())
}

static TABLE_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_table(prefix: &str) -> String {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let counter = TABLE_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{}_{}_{}_{}", prefix, pid, nanos % 1_000_000, counter)
}

async fn setup_pool() -> MySqlPool {
    MySqlPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(30))
        .connect(&mysql_url())
        .await
        .expect("mysql connect failed - is MySQL running?")
}

fn test_columns() -> Vec<ColumnDef> {
    vec![
        ColumnDef {
            name: "id".to_string(),
            sql_type: "BIGINT".to_string(),
            nullable: false,
            default: None,
            auto_increment: true,
            primary_key: true,
        },
        ColumnDef {
            name: "name".to_string(),
            sql_type: "VARCHAR(255)".to_string(),
            nullable: false,
            default: None,
            auto_increment: false,
            primary_key: false,
        },
        ColumnDef {
            name: "value".to_string(),
            sql_type: "BIGINT".to_string(),
            nullable: true,
            default: None,
            auto_increment: false,
            primary_key: false,
        },
    ]
}

// ==================== 方言断言测试 ====================

#[test]
fn test_oceanbase_dialect_quote() {
    let oceanbase = get_dialect(DbType::OceanBase).unwrap();
    let mysql = get_dialect(DbType::MySQL).unwrap();
    assert_eq!(oceanbase.quote("user"), mysql.quote("user"));
    assert_eq!(oceanbase.quote("user"), "`user`");
}

#[test]
fn test_oceanbase_dialect_escape() {
    let oceanbase = get_dialect(DbType::OceanBase).unwrap();
    let mysql = get_dialect(DbType::MySQL).unwrap();
    assert_eq!(oceanbase.escape_string("it's"), mysql.escape_string("it's"));
}

#[test]
fn test_oceanbase_dialect_pagination() {
    let oceanbase = get_dialect(DbType::OceanBase).unwrap();
    let mysql = get_dialect(DbType::MySQL).unwrap();
    assert_eq!(
        oceanbase.build_pagination("SELECT 1", 10, 20),
        mysql.build_pagination("SELECT 1", 10, 20)
    );
}

#[test]
fn test_oceanbase_dialect_db_type() {
    let dialect = get_dialect(DbType::OceanBase).unwrap();
    assert_eq!(dialect.db_type(), DbType::OceanBase);
}

#[test]
fn test_oceanbase_dialect_create_table() {
    let oceanbase = get_dialect(DbType::OceanBase).unwrap();
    let mysql = get_dialect(DbType::MySQL).unwrap();
    let sql_oceanbase = oceanbase.build_create_table("test_table", &test_columns());
    let sql_mysql = mysql.build_create_table("test_table", &test_columns());
    assert_eq!(sql_oceanbase, sql_mysql);
}

// ==================== 真实 DB 集成测试 ====================

#[tokio::test]
#[ignore]
async fn test_oceanbase_crud_on_mysql() {
    let pool = setup_pool().await;
    let dialect = get_dialect(DbType::OceanBase).unwrap();
    let table = unique_table("oceanbase");

    let create_sql = dialect.build_create_table(&table, &test_columns());
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .expect("create table");

    sqlx::query(sqlx::AssertSqlSafe(
        format!("INSERT INTO `{}` (`name`, `value`) VALUES (?, ?)", table).as_str(),
    ))
    .bind("alice")
    .bind(100i64)
    .execute(&pool)
    .await
    .expect("insert");

    let row = sqlx::query(sqlx::AssertSqlSafe(
        format!("SELECT `name`, `value` FROM `{}` WHERE `name` = ?", table).as_str(),
    ))
    .bind("alice")
    .fetch_one(&pool)
    .await
    .expect("select");

    assert_eq!(row.get::<String, _>("name"), "alice");
    assert_eq!(row.get::<i64, _>("value"), 100);

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE `{}`", table).as_str(),
    ))
    .execute(&pool)
    .await
    .expect("drop table");
}

#[tokio::test]
#[ignore]
async fn test_oceanbase_pagination_on_mysql() {
    let pool = setup_pool().await;
    let dialect = get_dialect(DbType::OceanBase).unwrap();
    let table = unique_table("oceanbase_pg");

    let create_sql = dialect.build_create_table(&table, &test_columns());
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .expect("create table");

    for i in 0..5i64 {
        sqlx::query(sqlx::AssertSqlSafe(
            format!("INSERT INTO `{}` (`name`, `value`) VALUES (?, ?)", table).as_str(),
        ))
        .bind(format!("user_{}", i))
        .bind(i)
        .execute(&pool)
        .await
        .expect("insert");
    }

    let pagination_sql = dialect.build_pagination(&format!("SELECT `name` FROM `{}`", table), 2, 2);
    let rows = sqlx::query(sqlx::AssertSqlSafe(pagination_sql.as_str()))
        .fetch_all(&pool)
        .await
        .expect("select pagination");
    assert_eq!(rows.len(), 2);

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE `{}`", table).as_str(),
    ))
    .execute(&pool)
    .await
    .expect("drop table");
}

#[tokio::test]
#[ignore]
async fn test_oceanbase_sql_injection_protection() {
    let pool = setup_pool().await;
    let dialect = get_dialect(DbType::OceanBase).unwrap();
    let table = unique_table("oceanbase_inj");

    let create_sql = dialect.build_create_table(&table, &test_columns());
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .expect("create table");

    sqlx::query(sqlx::AssertSqlSafe(
        format!("INSERT INTO `{}` (`name`, `value`) VALUES (?, ?)", table).as_str(),
    ))
    .bind("alice")
    .bind(1i64)
    .execute(&pool)
    .await
    .expect("insert");

    let malicious = "' OR '1'='1";
    let rows = sqlx::query(sqlx::AssertSqlSafe(
        format!("SELECT * FROM `{}` WHERE `name` = ?", table).as_str(),
    ))
    .bind(malicious)
    .fetch_all(&pool)
    .await
    .expect("select");
    assert_eq!(rows.len(), 0, "SQL injection should return 0 rows");

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE `{}`", table).as_str(),
    ))
    .execute(&pool)
    .await
    .expect("drop table");
}

// ============================================================================
// OceanBase 方言矩阵（2026-11-13，OC1~OC6 × OceanBase CE 4.4.2.1 @12881）
// 与既有 8 项不重复：聚焦 sz-orm 方言生成面 × OB MySQL 模式行为契约。
// 环境事实（登记）：daemon 模式容器约 2min 退出重启循环（restart=always 兜底），
// 拉起后 observer 需 ~2.5min 才可连（8001 initializing）——跑前守门探活。
// ============================================================================

use sz_orm_core::dialect::TableOption;

/// 守门探活：连接池可用才继续（容器拉起窗口期的 PoolTimedOut 不算方言缺陷）
async fn try_pool() -> Option<MySqlPool> {
    let url = mysql_url();
    let res = MySqlPoolOptions::new()
        .max_connections(3)
        .acquire_timeout(Duration::from_secs(10))
        .connect(&url)
        .await;
    res.ok()
}

async fn drop_table(pool: &MySqlPool, table: &str) {
    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE IF EXISTS `{}`", table).as_str(),
    ))
    .execute(pool)
    .await
    .ok();
}

/// OC1：引擎身份契约——@@version_comment / ob_compatibility_mode / 方言身份映射
#[tokio::test]
#[ignore = "需要 OceanBase 运行（SZ_ORM_MYSQL_URL 指向 root%40szorm@12881）"]
async fn matrix_oc1_engine_identity() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: SZ_ORM_MYSQL_URL 未配置或不可达（守门探活失败）");
        return;
    };
    // 真库身份：OceanBase_CE 4.x MySQL 模式
    let comment: String = sqlx::query_scalar("SELECT @@version_comment")
        .fetch_one(&pool)
        .await
        .expect("version_comment");
    assert!(
        comment.contains("OceanBase"),
        "真库应为 OceanBase，实测: {comment}"
    );
    let row = sqlx::query("SHOW VARIABLES LIKE 'ob_compatibility_mode'")
        .fetch_one(&pool)
        .await
        .expect("ob_compatibility_mode row");
    let mode: String = row.get(1);
    assert_eq!(mode.to_uppercase(), "MYSQL", "兼容模式应为 MYSQL: {mode}");

    // 方言身份：委托映射链
    let d = get_dialect(DbType::OceanBase).unwrap();
    assert_eq!(d.db_type(), DbType::OceanBase);
    assert!(DbType::OceanBase.is_mysql_family(), "OB 属 MySQL 族");
}

/// OC2：DDL 生成面——表选项（MY7 B1 API）在 OB 上的接受度 + quote 往返
#[tokio::test]
#[ignore = "需要 OceanBase 运行"]
async fn matrix_oc2_ddl_table_options_and_quote() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: 不可达");
        return;
    };
    let d = get_dialect(DbType::OceanBase).unwrap();
    let table = unique_table("oc2");

    // 表选项：OB MySQL 模式对 ENGINE/CHARSET 的接受度（与 MySQL 8 行为对拍面）
    let ddl = d.build_create_table_with_options(
        &table,
        &test_columns(),
        &[
            TableOption::Engine("InnoDB".to_string()),
            TableOption::Charset("utf8mb4".to_string()),
            TableOption::Collate("utf8mb4_general_ci".to_string()),
        ],
    );
    let res = sqlx::query(sqlx::AssertSqlSafe(ddl.as_str()))
        .execute(&pool)
        .await;
    match res {
        Ok(_) => {
            // 接受路径：元数据面验证 COLLATION 真实生效（OB 对 MySQL information_schema 兼容）
            let row = sqlx::query(
                "SELECT TABLE_COLLATION FROM information_schema.TABLES \
                 WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = ?",
            )
            .bind(&table)
            .fetch_optional(&pool)
            .await
            .expect("information_schema query")
            .expect("table row");
            let collation: String = row.get(0);
            assert_eq!(
                collation, "utf8mb4_general_ci",
                "OB 接受表选项且 COLLATION 生效"
            );
        }
        Err(e) => {
            // 拒绝路径：登记 OB 引擎特有差异（方言产物合法 MySQL DDL 被 OB 拒绝）
            panic!("OB MySQL 模式拒绝 MySQL 表选项 DDL（引擎差异，须登记）: {e}\nDDL: {ddl}");
        }
    }

    // quote 往返：保留字列名经方言 quote 后可执行（G2/G37 同型回归）
    let reserved = unique_table("oc2_order");
    let cols = vec![
        ColumnDef {
            name: "id".to_string(),
            sql_type: "BIGINT".to_string(),
            nullable: false,
            default: None,
            auto_increment: false,
            primary_key: true,
        },
        ColumnDef {
            // OB 引擎事实（同 MySQL）：TEXT 列不可作主键（1167 can't index column），
            // 保留字列名覆盖改用非主键列验证
            name: "select".to_string(),
            sql_type: "TEXT".to_string(),
            nullable: true,
            default: None,
            auto_increment: false,
            primary_key: false,
        },
    ];
    let sql = d.build_create_table(&reserved, &cols);
    sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .execute(&pool)
        .await
        .expect("保留字列名经方言 quote 后可执行");
    drop_table(&pool, &reserved).await;
    drop_table(&pool, &table).await;
}

/// OC3：Upsert 家族——ON DUPLICATE KEY UPDATE（方言渲染）+ INSERT IGNORE 真库行为
#[tokio::test]
#[ignore = "需要 OceanBase 运行"]
async fn matrix_oc3_upsert_family() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: 不可达");
        return;
    };
    let d = get_dialect(DbType::OceanBase).unwrap();
    let table = unique_table("oc3");

    let create_sql = d.build_create_table(&table, &test_columns());
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .expect("create table");

    // 方言渲染的冲突子句（VALUES() 引用形态——MySqlDialect 路径）
    let clause = d
        .build_upsert_on_conflict(
            &["id"],
            &["value"],
            &["id".to_string(), "name".to_string(), "value".to_string()],
        )
        .expect("MySQL 族支持 upsert");
    assert!(
        clause.contains("ON DUPLICATE KEY UPDATE") && clause.contains("VALUES("),
        "MySQL 族渲染形态: {clause}"
    );
    sqlx::query(sqlx::AssertSqlSafe(
        format!("INSERT INTO `{table}` (`id`,`name`,`value`) VALUES (?,?,?) {clause}").as_str(),
    ))
    .bind(1i64)
    .bind("a")
    .bind(10i64)
    .execute(&pool)
    .await
    .expect("upsert insert");
    // 冲突路径：value 更新为 EXCLUDED 值
    sqlx::query(sqlx::AssertSqlSafe(
        format!("INSERT INTO `{table}` (`id`,`name`,`value`) VALUES (?,?,?) {clause}").as_str(),
    ))
    .bind(1i64)
    .bind("a2")
    .bind(99i64)
    .execute(&pool)
    .await
    .expect("upsert update");
    let v: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(
        format!("SELECT `value` FROM `{table}` WHERE `id` = 1").as_str(),
    ))
    .fetch_one(&pool)
    .await
    .expect("read back");
    assert_eq!(v, 99, "ON DUPLICATE KEY UPDATE 生效");

    // INSERT IGNORE：冲突保留原值
    let prefix = d.build_insert_or_ignore_prefix(&table);
    sqlx::query(sqlx::AssertSqlSafe(
        format!("{prefix} (`id`,`name`,`value`) VALUES (?,?,?)").as_str(),
    ))
    .bind(1i64)
    .bind("ignored")
    .bind(77i64)
    .execute(&pool)
    .await
    .expect("INSERT IGNORE");
    let v: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(
        format!("SELECT `value` FROM `{table}` WHERE `id` = 1").as_str(),
    ))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(v, 99, "IGNORE 冲突保留原值");

    drop_table(&pool, &table).await;
}

/// OC4：类型与值面——U64 顶格往返 + DECIMAL(38,6) 大数 + UNIQUE 冲突 1062 映射
#[tokio::test]
#[ignore = "需要 OceanBase 运行"]
async fn matrix_oc4_types_and_error_codes() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: 不可达");
        return;
    };
    let table = unique_table("oc4");
    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "CREATE TABLE `{table}` (id BIGINT PRIMARY KEY, u BIGINT UNSIGNED, dec_col DECIMAL(38,6), name VARCHAR(64) UNIQUE)"
        )
        .as_str(),
    ))
    .execute(&pool)
    .await
    .expect("create");

    // U64 上界（OB BIGINT UNSIGNED = MySQL 语义）
    let u64_max = u64::MAX;
    sqlx::query(sqlx::AssertSqlSafe(
        format!("INSERT INTO `{table}` (id, u, dec_col, name) VALUES (?,?,?,?)").as_str(),
    ))
    .bind(1i64)
    .bind(u64_max)
    .bind("99999999999999999999.123456") // 20 位整数 + 6 小数 = 38 位内
    .bind("r1")
    .execute(&pool)
    .await
    .expect("insert boundary");

    let (gi, gu, gd): (i64, u64, String) = sqlx::query_as(sqlx::AssertSqlSafe(
        format!("SELECT id, u, CAST(dec_col AS CHAR) FROM `{table}` WHERE id = 1").as_str(),
    ))
    .fetch_one(&pool)
    .await
    .expect("read back");
    assert_eq!(gi, 1);
    assert_eq!(gu, u64_max, "BIGINT UNSIGNED 顶格往返");
    let dec_norm = gd.trim_start_matches('0');
    assert!(
        dec_norm.starts_with("99999999999999999999.12345"),
        "DECIMAL(38,6) 大数无损: {gd}"
    );

    // UNIQUE 冲突错误码 1062（MySqlDialect 错误映射假设的一致性）
    let res = sqlx::query(sqlx::AssertSqlSafe(
        format!("INSERT INTO `{table}` (id, name) VALUES (2, 'r1')").as_str(),
    ))
    .execute(&pool)
    .await;
    match res {
        Err(e) => {
            let msg = e.to_string();
            assert!(
                msg.contains("1062") || msg.to_lowercase().contains("duplicate"),
                "OB UNIQUE 冲突应报 1062 形态: {msg}"
            );
        }
        Ok(_) => panic!("UNIQUE 冲突必须被拒绝"),
    }
    drop_table(&pool, &table).await;
}

/// OC5：分布式特有——SHOW VARIABLES ob_ 前面子集 + HASH 分区表 DDL（引擎能力登记）
#[tokio::test]
#[ignore = "需要 OceanBase 运行"]
async fn matrix_oc5_distributed_surface() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: 不可达");
        return;
    };
    // ob_ 前缀系统变量子集可读（分布式内核面存在性证明）
    let rows = sqlx::query("SHOW VARIABLES LIKE 'ob_%'")
        .fetch_all(&pool)
        .await
        .expect("show variables");
    assert!(
        rows.len() > 10,
        "OB 分布式内核变量应大量存在: {}",
        rows.len()
    );

    // 分区表 DDL：MySQL 兼容语法在 OB 上真执行（方言生成面之外——引擎能力登记）
    let table = unique_table("oc5_part");
    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "CREATE TABLE `{table}` (id BIGINT PRIMARY KEY, v INT) PARTITION BY HASH(id) PARTITIONS 4"
        )
        .as_str(),
    ))
    .execute(&pool)
    .await
    .expect("HASH 分区表 DDL 被 OB 接受");
    sqlx::query(sqlx::AssertSqlSafe(
        format!("INSERT INTO `{table}` VALUES (1, 10), (5, 50)").as_str(),
    ))
    .execute(&pool)
    .await
    .expect("分区表插入");
    let n: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(
        format!("SELECT COUNT(*) FROM `{table}`").as_str(),
    ))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(n, 2);
    drop_table(&pool, &table).await;
}

/// OC6：SQL 注入防护——参数化路径真连 + 方言分页产物真库执行（门禁 9 行为级补强）
#[tokio::test]
#[ignore = "需要 OceanBase 运行"]
async fn matrix_oc6_parametrized_and_pagination() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: 不可达");
        return;
    };
    let d = get_dialect(DbType::OceanBase).unwrap();
    let table = unique_table("oc6");
    let create_sql = d.build_create_table(&table, &test_columns());
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .expect("create");

    for i in 0..5i64 {
        sqlx::query(sqlx::AssertSqlSafe(
            format!("INSERT INTO `{table}` (`name`,`value`) VALUES (?,?)").as_str(),
        ))
        .bind(format!("row{i}"))
        .bind(i * 10)
        .execute(&pool)
        .await
        .expect("seed");
    }

    // 注入向量惰性：恶意输入作为绑定参数（不改变语义）
    let malicious = "x' OR '1'='1' --";
    let rows = sqlx::query(sqlx::AssertSqlSafe(
        format!("SELECT `id` FROM `{table}` WHERE `name` = ?").as_str(),
    ))
    .bind(malicious)
    .fetch_all(&pool)
    .await
    .expect("select");
    assert_eq!(rows.len(), 0, "注入向量作为参数必须惰性");

    // 方言分页产物真库执行
    let paged = d.build_pagination(&format!("SELECT `name` FROM `{table}`"), 2, 2);
    let rows = sqlx::query(sqlx::AssertSqlSafe(paged.as_str()))
        .fetch_all(&pool)
        .await
        .expect("pagination");
    assert_eq!(rows.len(), 2, "LIMIT 2 OFFSET 2 命中 2 行");
    drop_table(&pool, &table).await;
}
