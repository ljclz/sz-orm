//! PolarDB-X（MySQL 族）方言矩阵真连验证（PL1，2026-11-14 评测批次）
//!
//! 评测对象：**sz-orm 的 `DbType::MySQL` 方言生成面**在 PolarDB-X（阿里云分布式
//! MySQL 兼容数据库，开源版 polardbx/polardb-x 一键镜像）上的真实兼容性。
//!
//! 供给/方言匹配边界（本批先决裁定，与 GBase 批边界登记同型）：
//! - 可用社区容器只有 **PolarDB-X（MySQL 协议族）**；`PolarDbDialect` 委托
//!   PostgreSqlDialect，面向阿里云 **PolarDB for PostgreSQL**——社区无 PG 版
//!   容器镜像，PG 语义通道本机不可真连，留云实例/CI 补批（PL2 静态覆盖）。
//! - 因此本批真连走 MySQL 协议：PolarDB-X 用户应选 `DbType::MySQL`。
//!
//! 矩阵（PL1~PL5 载荷对齐 MY 矩阵）：
//! - PLX1 引擎身份：`SELECT VERSION()` 含 `PXC`；`SHOW CREATE TABLE` 引擎/字符集
//! - PLX2 DDL 与 quoting：反引号标识符；IF NOT EXISTS 幂等；保留字列名
//! - PLX3 UPSERT 行为级：`ON DUPLICATE KEY UPDATE` 命中与插入两分支
//! - PLX4 分页与排序：`LIMIT/OFFSET`；ORDER BY 语义
//! - PLX5 注入惰性：参数化输入恒作数据（真库行为级）
//! - PLX6 分布式探针：单表模式下 `PARTITION BY` 被拒（PXC-4500）登记
//!
//! 运行：cargo test -p sz-orm-core --test integration_polardbx -- --ignored
//! 凭据经环境变量 `SZ_ORM_POLARDBX_URL` 注入（如
//! `mysql://polardbx_root@127.0.0.1:18506/szorm_plx`）；未配置自动跳过，密码不入库。

use sqlx::mysql::{MySqlPool, MySqlPoolOptions};
use sqlx::Row;
use std::collections::HashMap;
use std::time::Duration;
use sz_orm_core::dialect::{get_dialect, ColumnDef, Dialect};
use sz_orm_core::{DbType, Model, ModelExt, QueryBuilder, Relation, Value};

struct MatrixModel;

impl Model for MatrixModel {
    type PrimaryKey = i64;
    fn table_name() -> &'static str {
        "matrix_dummy"
    }
    fn pk(&self) -> Self::PrimaryKey {
        0
    }
    fn set_pk(&mut self, _pk: Self::PrimaryKey) {}
}

impl ModelExt for MatrixModel {
    fn columns() -> Vec<&'static str> {
        vec!["id"]
    }
    fn fillable() -> Vec<&'static str> {
        vec![]
    }
    fn guarded() -> Vec<&'static str> {
        vec!["id"]
    }
    fn hidden() -> Vec<&'static str> {
        vec![]
    }
    fn relations() -> HashMap<&'static str, Relation> {
        HashMap::new()
    }
    fn fill(&mut self, _data: HashMap<String, Value>) {}
    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({})
    }
}

fn unique_table(prefix: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{}_{}_{}_{}", prefix, pid, nanos % 1_000_000, counter)
}

/// 凭据经 `SZ_ORM_POLARDBX_URL` 注入；未配置返回 None（测试自动跳过）
fn polardbx_url() -> Option<String> {
    std::env::var("SZ_ORM_POLARDBX_URL")
        .ok()
        .filter(|s| !s.is_empty())
}

async fn try_pool() -> Option<MySqlPool> {
    let url = polardbx_url()?;
    match MySqlPoolOptions::new()
        .max_connections(4)
        .acquire_timeout(Duration::from_secs(15))
        .connect(&url)
        .await
    {
        Ok(pool) => Some(pool),
        Err(e) => {
            eprintln!("skip: PolarDB-X 连接失败（{e}）");
            None
        }
    }
}

async fn drop_table(pool: &MySqlPool, table: &str) {
    let dialect = get_dialect(DbType::MySQL).expect("mysql dialect");
    let sql = dialect.build_drop_table(table, true);
    let _ = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .execute(pool)
        .await;
}

macro_rules! bind_params {
    ($q:expr, $params:expr) => {{
        let mut q = $q;
        for p in $params.iter() {
            q = match p {
                Value::Null => q.bind(Option::<i64>::None),
                Value::Bool(b) => q.bind(if *b { 1i64 } else { 0i64 }),
                Value::I8(v) => q.bind(*v),
                Value::I16(v) => q.bind(*v),
                Value::I32(v) => q.bind(*v),
                Value::I64(v) => q.bind(*v),
                Value::U8(v) => q.bind(*v),
                Value::U16(v) => q.bind(*v),
                Value::U32(v) => q.bind(*v),
                Value::U64(v) => q.bind(*v),
                Value::F32(v) => q.bind(*v),
                Value::F64(v) => q.bind(*v),
                Value::String(s) => q.bind(s.clone()),
                Value::Decimal(s) | Value::Uuid(s) | Value::Date(s) | Value::DateTime(s)
                | Value::Time(s) | Value::Json(s) => q.bind(s.clone()),
                Value::Bytes(b) => q.bind(b.clone()),
                // Value 为 #[non_exhaustive]（集成测试是独立 crate，须留 wildcard）：
                // 剩余变体（BoxedStr/Object/未来扩展）按 JSON 文本兜底绑定，矩阵载荷不使用
                _ => q.bind(serde_json::to_string(p).unwrap_or_default()),
            };
        }
        q
    }};
}

fn mysql_dialect() -> Box<dyn Dialect> {
    get_dialect(DbType::MySQL).expect("mysql dialect")
}

fn q<'a>(sql: &'a str) -> sqlx::query::Query<'a, sqlx::MySql, sqlx::mysql::MySqlArguments> {
    sqlx::query(sqlx::AssertSqlSafe(sql))
}

fn qb(table: &str) -> QueryBuilder<MatrixModel> {
    QueryBuilder::new(get_dialect(DbType::MySQL).expect("mysql dialect")).table(table)
}

// ---------------------------------------------------------------------------
// PLX1 引擎身份
// ---------------------------------------------------------------------------

/// PLX1：引擎身份 + SHOW CREATE TABLE 引擎/字符集——锚定「真 PolarDB-X 而非普通 MySQL」。
#[tokio::test]
#[ignore = "需要 PolarDB-X 运行（SZ_ORM_POLARDBX_URL 注入）"]
async fn matrix_plx1_engine_identity() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: SZ_ORM_POLARDBX_URL 未配置或不可达");
        return;
    };
    // 引擎身份：PolarDB-X 自报版本含 PXC 标识
    let row = q("SELECT VERSION()")
        .fetch_one(&pool)
        .await
        .expect("select version");
    let version: String = row.try_get(0).expect("version column");
    assert!(
        version.contains("PXC"),
        "PolarDB-X 引擎版本应含 PXC 标识，实际: {version}"
    );

    let dialect = mysql_dialect();
    let table = unique_table("plx_id");
    let columns = vec![ColumnDef {
        name: "id".to_string(),
        sql_type: "BIGINT".to_string(),
        nullable: false,
        default: None,
        auto_increment: true,
        primary_key: true,
    }];
    let ddl = dialect.build_create_table(&table, &columns);
    sqlx::query(sqlx::AssertSqlSafe(ddl.as_str()))
        .execute(&pool)
        .await
        .expect("create table");

    // SHOW CREATE TABLE：方言建表产物在 PolarDB-X 上落为 InnoDB 表
    let show = format!("SHOW CREATE TABLE {}", dialect.quote(&table));
    let row = q(&show).fetch_one(&pool).await.expect("show create");
    let create_sql: String = row.try_get(1).expect("create statement");
    assert!(
        create_sql.contains("InnoDB"),
        "PolarDB-X 单表模式应为 InnoDB: {create_sql}"
    );

    drop_table(&pool, &table).await;
}

// ---------------------------------------------------------------------------
// PLX2 DDL 与 quoting
// ---------------------------------------------------------------------------

/// PLX2：反引号 quoting + IF NOT EXISTS 幂等 + 保留字列名——方言 quote 产物真库可执行。
#[tokio::test]
#[ignore = "需要 PolarDB-X 运行（SZ_ORM_POLARDBX_URL 注入）"]
async fn matrix_plx2_ddl_quoting_idempotent() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: SZ_ORM_POLARDBX_URL 未配置或不可达");
        return;
    };
    let dialect = mysql_dialect();
    let table = unique_table("plx_ddl");
    let columns = vec![
        ColumnDef {
            name: "id".to_string(),
            sql_type: "BIGINT".to_string(),
            nullable: false,
            default: None,
            auto_increment: true,
            primary_key: true,
        },
        ColumnDef {
            // 保留字列名：quote 后应可建可查
            name: "order".to_string(),
            sql_type: "VARCHAR(50)".to_string(),
            nullable: true,
            default: None,
            auto_increment: false,
            primary_key: false,
        },
    ];
    let ddl = dialect.build_create_table(&table, &columns);
    assert!(
        ddl.contains('`'),
        "MySQL 族方言建表应含反引号 quoting: {ddl}"
    );
    sqlx::query(sqlx::AssertSqlSafe(ddl.as_str()))
        .execute(&pool)
        .await
        .expect("create table");

    // IF NOT EXISTS 幂等：再次执行不报错
    sqlx::query(sqlx::AssertSqlSafe(ddl.as_str()))
        .execute(&pool)
        .await
        .expect("create table if not exists 应幂等");

    // 保留字列读写往返
    let insert = format!("INSERT INTO {} (`order`) VALUES (?)", dialect.quote(&table));
    q(&insert)
        .bind("first")
        .execute(&pool)
        .await
        .expect("insert");
    let sel = format!("SELECT `order` FROM {} LIMIT 1", dialect.quote(&table));
    let row = q(&sel).fetch_one(&pool).await.expect("select reserved");
    let v: String = row.try_get(0).expect("order col");
    assert_eq!(v, "first", "保留字列名 quote 后应可读写");

    drop_table(&pool, &table).await;
}

// ---------------------------------------------------------------------------
// PLX3 UPSERT 行为级
// ---------------------------------------------------------------------------

/// PLX3：`ON DUPLICATE KEY UPDATE` 命中与插入两分支——行为级（读回验证）。
#[tokio::test]
#[ignore = "需要 PolarDB-X 运行（SZ_ORM_POLARDBX_URL 注入）"]
async fn matrix_plx3_upsert_behavior() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: SZ_ORM_POLARDBX_URL 未配置或不可达");
        return;
    };
    let dialect = mysql_dialect();
    let table = unique_table("plx_up");
    let columns = vec![
        ColumnDef {
            name: "id".to_string(),
            sql_type: "BIGINT".to_string(),
            nullable: false,
            default: None,
            auto_increment: false,
            primary_key: true,
        },
        ColumnDef {
            name: "v".to_string(),
            sql_type: "VARCHAR(50)".to_string(),
            nullable: true,
            default: None,
            auto_increment: false,
            primary_key: false,
        },
    ];
    let ddl = dialect.build_create_table(&table, &columns);
    sqlx::query(sqlx::AssertSqlSafe(ddl.as_str()))
        .execute(&pool)
        .await
        .expect("create table");

    // 分支 1：插入新行
    let upsert = format!(
        "INSERT INTO {} (`id`, `v`) VALUES (?, ?) ON DUPLICATE KEY UPDATE `v` = VALUES(`v`)",
        dialect.quote(&table)
    );
    bind_params!(q(&upsert), &[Value::I64(1), Value::String("a".into())])
        .execute(&pool)
        .await
        .expect("upsert insert");
    let sel = format!("SELECT `v` FROM {} WHERE `id` = ?", dialect.quote(&table));
    let row = bind_params!(q(&sel), &[Value::I64(1)])
        .fetch_one(&pool)
        .await
        .expect("select after insert");
    let v: String = row.try_get(0).unwrap();
    assert_eq!(v, "a", "首插分支应生效");

    // 分支 2：命中唯一键走 UPDATE
    bind_params!(q(&upsert), &[Value::I64(1), Value::String("up".into())])
        .execute(&pool)
        .await
        .expect("upsert update");
    let row = bind_params!(q(&sel), &[Value::I64(1)])
        .fetch_one(&pool)
        .await
        .expect("select after update");
    let v: String = row.try_get(0).unwrap();
    assert_eq!(v, "up", "命中分支应走 ON DUPLICATE KEY UPDATE");

    drop_table(&pool, &table).await;
}

// ---------------------------------------------------------------------------
// PLX4 分页与排序
// ---------------------------------------------------------------------------

/// PLX4：QueryBuilder 分页 LIMIT/OFFSET + ORDER BY——分布式 CN 层语义与单机 MySQL 一致。
#[tokio::test]
#[ignore = "需要 PolarDB-X 运行（SZ_ORM_POLARDBX_URL 注入）"]
async fn matrix_plx4_pagination_order() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: SZ_ORM_POLARDBX_URL 未配置或不可达");
        return;
    };
    let dialect = mysql_dialect();
    let table = unique_table("plx_page");
    let columns = vec![
        ColumnDef {
            name: "id".to_string(),
            sql_type: "BIGINT".to_string(),
            nullable: false,
            default: None,
            auto_increment: false,
            primary_key: true,
        },
        ColumnDef {
            name: "v".to_string(),
            sql_type: "BIGINT".to_string(),
            nullable: true,
            default: None,
            auto_increment: false,
            primary_key: false,
        },
    ];
    let ddl = dialect.build_create_table(&table, &columns);
    sqlx::query(sqlx::AssertSqlSafe(ddl.as_str()))
        .execute(&pool)
        .await
        .expect("create table");

    let insert = format!(
        "INSERT INTO {} (`id`, `v`) VALUES (?, ?)",
        dialect.quote(&table)
    );
    for (id, v) in [(1i64, 30i64), (2, 10), (3, 20), (4, 10)] {
        bind_params!(q(&insert), &[Value::I64(id), Value::I64(v)])
            .execute(&pool)
            .await
            .expect("insert");
    }

    // QueryBuilder 排序 + 分页生成面
    let (sql, params) = qb(&table)
        .select(vec!["id"])
        .unwrap()
        .order_by("v")
        .order_desc("id")
        .limit(2)
        .offset(1)
        .build_select_with_params();
    assert!(
        sql.contains("LIMIT") && sql.contains("OFFSET"),
        "分页 SQL 应含 LIMIT/OFFSET: {sql}"
    );
    let rows = bind_params!(q(&sql), params)
        .fetch_all(&pool)
        .await
        .expect("paged execute");
    let ids: Vec<i64> = rows.iter().map(|r| r.get::<i64, _>("id")).collect();
    // v ASC, id DESC：v=10 组内 id DESC → 4,2；v=20 → 3；v=30 → 1。
    // OFFSET 1 后 LIMIT 2 → [2, 3]
    assert_eq!(ids, vec![2, 3], "PolarDB-X 排序+分页语义: {sql}");

    drop_table(&pool, &table).await;
}

// ---------------------------------------------------------------------------
// PLX5 注入惰性（真库行为级）
// ---------------------------------------------------------------------------

/// PLX5：参数化输入恒作数据——含 SQL 元字符的绑定值在 PolarDB-X 上不得改变查询语义。
#[tokio::test]
#[ignore = "需要 PolarDB-X 运行（SZ_ORM_POLARDBX_URL 注入）"]
async fn matrix_plx5_injection_inert() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: SZ_ORM_POLARDBX_URL 未配置或不可达");
        return;
    };
    let dialect = mysql_dialect();
    let table = unique_table("plx_inj");
    let columns = vec![
        ColumnDef {
            name: "id".to_string(),
            sql_type: "BIGINT".to_string(),
            nullable: false,
            default: None,
            auto_increment: false,
            primary_key: true,
        },
        ColumnDef {
            name: "payload".to_string(),
            sql_type: "VARCHAR(100)".to_string(),
            nullable: true,
            default: None,
            auto_increment: false,
            primary_key: false,
        },
    ];
    let ddl = dialect.build_create_table(&table, &columns);
    sqlx::query(sqlx::AssertSqlSafe(ddl.as_str()))
        .execute(&pool)
        .await
        .expect("create table");

    let insert = format!(
        "INSERT INTO {} (`id`, `payload`) VALUES (?, ?)",
        dialect.quote(&table)
    );
    bind_params!(
        q(&insert),
        &[Value::I64(1), Value::String("x' OR '1'='1".into())]
    )
    .execute(&pool)
    .await
    .expect("insert payload1");
    bind_params!(
        q(&insert),
        &[Value::I64(2), Value::String("'; DROP TABLE x;--".into())]
    )
    .execute(&pool)
    .await
    .expect("insert payload2");

    // 恶意输入经参数化匹配——精确等于字面量，不扩集
    let sel = format!(
        "SELECT `id` FROM {} WHERE `payload` = ?",
        dialect.quote(&table)
    );
    let rows = bind_params!(q(&sel), &[Value::String("x' OR '1'='1".into())])
        .fetch_all(&pool)
        .await
        .expect("parameterized match");
    let ids: Vec<i64> = rows.iter().map(|r| r.get::<i64, _>("id")).collect();
    assert_eq!(ids, vec![1], "参数化输入恒作数据：仅精确命中 1 行");

    // 表未被注入语句破坏（第二个 payload 含 DROP 形态，但只作数据）
    let cnt = q(&format!("SELECT COUNT(*) FROM {}", dialect.quote(&table)))
        .fetch_one(&pool)
        .await
        .expect("count");
    let n: i64 = cnt.try_get(0).unwrap();
    assert_eq!(n, 2, "注入形态 payload 不得破坏表");

    drop_table(&pool, &table).await;
}

// ---------------------------------------------------------------------------
// PLX6 分布式边界探针
// ---------------------------------------------------------------------------

/// PLX6：单表（非分区）模式下 `PARTITION BY` 被拒（PXC-4500）——引擎边界登记，
/// 非方言缺陷（sz-orm MySQL 方言不生成 PARTITION BY，此处仅为引擎事实锚定）。
#[tokio::test]
#[ignore = "需要 PolarDB-X 运行（SZ_ORM_POLARDBX_URL 注入）"]
async fn matrix_plx6_partition_by_rejected() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: SZ_ORM_POLARDBX_URL 未配置或不可达");
        return;
    };
    let table = unique_table("plx_part");
    let ddl = format!(
        "CREATE TABLE IF NOT EXISTS {} (`id` BIGINT PRIMARY KEY) PARTITION BY KEY(`id`) PARTITIONS 2",
        table
    );
    let result = q(&ddl).execute(&pool).await;
    let err = result.expect_err("单表模式下 PARTITION BY 应被 PolarDB-X 拒绝");
    let msg = err.to_string();
    assert!(
        msg.contains("4500") || msg.contains("partition"),
        "预期 PXC-4500 分区不支持错误，实际: {msg}"
    );
    // 该 DDL 因 CREATE 未落表，无清理负担
}
