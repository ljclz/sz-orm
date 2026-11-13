//! MySQL 方言矩阵真连验证（MY1~MY6，2026-11-11 评测批次）
//!
//! 评测对象是 **sz-orm 自己的方言层**：同一功能载荷按 `DbType::MySQL` 方言生成 SQL，
//! 在真 MySQL 上执行，验证生成被接受且结果符合 MySQL 自身语义。
//! MySQL 是独立方言族，无「偏离 PG18 即缺陷」的仲裁关系（区别于 2026-11-09 SzRSQL 矩阵差分）。
//!
//! 矩阵：
//! - MY1 排序与 NULL：`ORDER BY` ASC/DESC 含 NULL 列（MySQL ASC 时 NULL 排最前）
//! - MY2 标识符与保留字：反引号 quoting；保留字表名列名；Unicode 标识符；63/64 长标识符
//! - MY3 类型与 DDL：AUTO_INCREMENT 主键回填（LAST_INSERT_ID()）；TEXT/BLOB/JSON/DECIMAL 往返；utf8mb4
//! - MY4 UPSERT 与批量：`ON DUPLICATE KEY UPDATE`；批量 INSERT 参数上限（65535 占位符协议极限）
//! - MY5 分页与窗口：`LIMIT n OFFSET m`；CTE；窗口函数；`GROUP_CONCAT` separator 与截断
//! - MY6 边缘语义：三值 NULL；除零（sql_mode 探针）；隐式类型转换；空串 vs NULL；弱类型
//!
//! 三分类：sz-orm 方言缺口（修复）/ MySQL 环境特有（登记）/ 文档措辞。
//! 详见 docs/assessment/2026-11-11-mysql-dialect-matrix.md。
//!
//! 运行：cargo test -p sz-orm-core --test integration_mysql_matrix -- --ignored
//! 凭据经环境变量 `SZ_ORM_MYSQL_URL` 注入（未配置自动跳过），密码不入库。

use sqlx::mysql::{MySqlPool, MySqlPoolOptions};
use sqlx::Row;
use std::collections::HashMap;
use std::time::Duration;
use sz_orm_core::dialect::{
    get_dialect, ColumnDef, Dialect, ForeignKeyDef, FullTextMode, TableChange, TableOption,
};
use sz_orm_core::{DbType, Model, ModelExt, QueryBuilder, Relation, Value};

/// QueryBuilder 需要 Model 类型参数；矩阵载荷直接走 SQL 生成面，用最小占位模型
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

/// 全局唯一表名（pid + 纳秒 + 计数器，避免并行冲突）
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

/// 凭据经 `SZ_ORM_MYSQL_URL` 注入；未配置返回 None（测试自动跳过）
fn mysql_url() -> Option<String> {
    std::env::var("SZ_ORM_MYSQL_URL")
        .ok()
        .filter(|s| !s.is_empty())
}

async fn try_pool() -> Option<MySqlPool> {
    let url = mysql_url()?;
    match MySqlPoolOptions::new()
        .max_connections(4)
        .acquire_timeout(Duration::from_secs(15))
        .connect(&url)
        .await
    {
        Ok(pool) => Some(pool),
        Err(e) => {
            eprintln!("skip: MySQL 连接失败（{e}）");
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

/// 把 sz-orm `Value` 参数列表绑定到 sqlx 查询（门禁 9：所有 WHERE 条件参数化）
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

/// 方言生成的 SQL（内部表名经 `quote()`，值经 `?` 绑定）——审计后包装 `AssertSqlSafe`
/// （sqlx 0.9 要求动态 SQL 显式安全声明；与既有 integration_mysql.rs 同一模式）
fn q<'a>(sql: &'a str) -> sqlx::query::Query<'a, sqlx::MySql, sqlx::mysql::MySqlArguments> {
    sqlx::query(sqlx::AssertSqlSafe(sql))
}

fn qb(table: &str) -> QueryBuilder<MatrixModel> {
    QueryBuilder::new(get_dialect(DbType::MySQL).expect("mysql dialect")).table(table)
}

// ---------------------------------------------------------------------------
// MY1 排序与 NULL
// ---------------------------------------------------------------------------

/// MY1：`ORDER BY` ASC/DESC 含 NULL 列——MySQL ASC 时 NULL 排最前（PG 排最后）。
/// 方言层必须原样透传（不得注入 MySQL 8.0 不支持的 NULLS FIRST/LAST），
/// 并以真库执行结果锚定 MySQL 自身语义。
#[tokio::test]
#[ignore = "需要 MySQL 运行（SZ_ORM_MYSQL_URL 注入）"]
async fn matrix_my1_order_by_null_semantics() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: SZ_ORM_MYSQL_URL 未配置或不可达");
        return;
    };
    let dialect = mysql_dialect();
    let table = unique_table("mx_my1");
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
            name: "val".to_string(),
            sql_type: "BIGINT".to_string(),
            nullable: true,
            default: None,
            auto_increment: false,
            primary_key: false,
        },
        ColumnDef {
            name: "tag".to_string(),
            sql_type: "VARCHAR(50)".to_string(),
            nullable: false,
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

    // 插入：id=1 val=2, id=2 val=NULL, id=3 val=1
    let insert = format!(
        "INSERT INTO {} (`val`, `tag`) VALUES (?, ?)",
        dialect.quote(&table)
    );
    for (val, tag) in [(Some(2i64), "a"), (None, "a"), (Some(1i64), "b")] {
        let q = q(&insert).bind(val).bind(tag);
        q.execute(&pool).await.expect("insert");
    }

    // 方言生成面：QueryBuilder order_by 应 quote 字段且不注入 NULLS FIRST/LAST
    let (sql, params) = qb(&table)
        .select(vec!["id"])
        .unwrap()
        .order_by("val")
        .build_select_with_params();
    assert!(params.is_empty(), "纯排序不应产生参数");
    assert!(sql.contains("ORDER BY `val` ASC"), "生成的排序 SQL: {sql}");
    assert!(
        !sql.to_uppercase().contains("NULLS"),
        "MySQL 8.0 不支持 NULLS FIRST/LAST，方言不得注入: {sql}"
    );

    // 真库语义锚定：MySQL ASC 时 NULL 排最前
    let rows = bind_params!(q(&sql), params)
        .fetch_all(&pool)
        .await
        .expect("asc execute");
    let ids: Vec<i64> = rows.iter().map(|r| r.get::<i64, _>("id")).collect();
    assert_eq!(ids, vec![2, 3, 1], "MySQL ASC: NULL 最前，随后按值升序");

    let (sql_desc, _) = qb(&table)
        .select(vec!["id"])
        .unwrap()
        .order_desc("val")
        .build_select_with_params();
    let rows = q(&sql_desc).fetch_all(&pool).await.expect("desc execute");
    let ids: Vec<i64> = rows.iter().map(|r| r.get::<i64, _>("id")).collect();
    assert_eq!(ids, vec![1, 3, 2], "MySQL DESC: NULL 最后");

    // 多键排序：tag ASC, val DESC
    let (sql_multi, _) = qb(&table)
        .select(vec!["id"])
        .unwrap()
        .order_by("tag")
        .order_desc("val")
        .build_select_with_params();
    let rows = q(&sql_multi).fetch_all(&pool).await.expect("multi execute");
    let ids: Vec<i64> = rows.iter().map(|r| r.get::<i64, _>("id")).collect();
    // 数据：id1(val=2,tag=a)、id2(val=NULL,tag=a)、id3(val=1,tag=b)。
    // 多键 tag ASC, val DESC：a 组内 val DESC = 2 先 NULL 后 → id1, id2；随后 b 组 id3
    assert_eq!(ids, vec![1, 2, 3], "多键排序 MySQL 语义: {sql_multi}");

    drop_table(&pool, &table).await;
}

// ---------------------------------------------------------------------------
// MY2 标识符与保留字
// ---------------------------------------------------------------------------

/// MY2：反引号 quoting；保留字表名列名；Unicode 标识符；63/64 长标识符。
#[tokio::test]
#[ignore = "需要 MySQL 运行（SZ_ORM_MYSQL_URL 注入）"]
async fn matrix_my2_identifier_quoting() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: SZ_ORM_MYSQL_URL 未配置或不可达");
        return;
    };
    let dialect = mysql_dialect();

    // 方言生成面：保留字与反引号转义
    assert_eq!(dialect.quote("order"), "`order`");
    assert_eq!(dialect.quote("with`quote"), "`with``quote`");

    // 真库：保留字表名 + 保留字列名端到端（QueryBuilder 生成 → 真库执行）
    let table = unique_table("mx_my2");
    let order_table = format!("{table}_order");
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
            name: "order".to_string(),
            sql_type: "BIGINT".to_string(),
            nullable: true,
            default: None,
            auto_increment: false,
            primary_key: false,
        },
    ];
    let ddl = dialect.build_create_table(&order_table, &columns);
    assert!(
        ddl.contains("CREATE TABLE `"),
        "建表 SQL 应 quote 表名: {ddl}"
    );
    sqlx::query(sqlx::AssertSqlSafe(ddl.as_str()))
        .execute(&pool)
        .await
        .expect("保留字列名建表被真库接受");

    let mut data = HashMap::new();
    data.insert("order".to_string(), Value::I64(42));
    let (insert_sql, params) = qb(&order_table).build_insert_with_params(&data);
    assert!(
        insert_sql.contains("`order`"),
        "INSERT 应 quote 保留字列: {insert_sql}"
    );
    bind_params!(q(&insert_sql), params)
        .execute(&pool)
        .await
        .expect("保留字列 INSERT 被真库接受");

    let (sel_sql, sel_params) = qb(&order_table)
        .select(vec!["order"])
        .unwrap()
        .where_eq("order", Value::I64(42))
        .build_select_with_params();
    let rows = bind_params!(q(&sel_sql), sel_params)
        .fetch_all(&pool)
        .await
        .expect("保留字列 SELECT 被真库接受");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get::<i64, _>("order"), 42);

    // Unicode 标识符：quote() 原样透传，真库接受；quote_checked 按 ASCII 白名单拒绝（安全策略，登记不修）
    let uni_table = unique_table("mx_my2_uni");
    let uni_ddl = format!(
        "CREATE TABLE {} (`id` BIGINT PRIMARY KEY, {} BIGINT)",
        dialect.quote(&uni_table),
        dialect.quote("列名")
    );
    sqlx::query(sqlx::AssertSqlSafe(uni_ddl.as_str()))
        .execute(&pool)
        .await
        .expect("Unicode 标识符被真库接受");
    assert_eq!(dialect.quote("列名"), "`列名`");
    assert!(
        dialect.quote_checked("列名").is_err(),
        "quote_checked 对非 ASCII 标识符 fail-closed（登记：MySQL 实际接受，sz-orm 安全白名单更严）"
    );
    drop_table(&pool, &uni_table).await;

    // 长标识符：MySQL 上限 64 字符（PG 是 63）——quote_checked 应按方言上限放行 64
    let ident64 = "a".repeat(64);
    let ident65 = "a".repeat(65);
    // 上一轮断言失败中断时可能残留，先清场（IF EXISTS）
    drop_table(&pool, &ident64).await;
    let long_ddl = format!(
        "CREATE TABLE {} (`id` BIGINT PRIMARY KEY)",
        dialect.quote(&ident64)
    );
    sqlx::query(sqlx::AssertSqlSafe(long_ddl.as_str()))
        .execute(&pool)
        .await
        .expect("64 字符标识符在 MySQL 合法（上限 64）");
    let over_ddl = format!(
        "CREATE TABLE {} (`id` BIGINT PRIMARY KEY)",
        dialect.quote(&ident65)
    );
    assert!(
        sqlx::query(sqlx::AssertSqlSafe(over_ddl.as_str()))
            .execute(&pool)
            .await
            .is_err(),
        "65 字符标识符应被真库拒绝"
    );
    assert!(
        dialect.quote_checked(&ident64).is_ok(),
        "quote_checked 应按 MySQL 方言上限 64 放行 64 字符标识符"
    );
    assert!(
        dialect.quote_checked(&ident65).is_err(),
        "65 字符标识符应被 quote_checked 拒绝"
    );
    drop_table(&pool, &ident64).await;
    drop_table(&pool, &order_table).await;
}

/// MY2b：ALTER TABLE 变更面——AddIndex 列名与索引名必须 quote（保留字列），
/// AddForeignKey 约束名必须 quote。
#[tokio::test]
#[ignore = "需要 MySQL 运行（SZ_ORM_MYSQL_URL 注入）"]
async fn matrix_my2b_alter_index_reserved_word() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: SZ_ORM_MYSQL_URL 未配置或不可达");
        return;
    };
    let dialect = mysql_dialect();
    let table = unique_table("mx_my2b");
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
            name: "order".to_string(),
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

    // 方言生成面：AddIndex 列名必须 quote，索引名必须 quote
    let changes = vec![TableChange::AddIndex(
        "idx_order".to_string(),
        vec!["order".to_string()],
    )];
    let alter_sql = dialect.build_alter_table(&table, &changes);
    assert!(
        alter_sql.contains("ADD INDEX `idx_order` (`order`)"),
        "AddIndex 应 quote 索引名与列名: {alter_sql}"
    );
    sqlx::query(sqlx::AssertSqlSafe(alter_sql.as_str()))
        .execute(&pool)
        .await
        .expect("AddIndex（保留字列）被真库接受");

    // DropIndex 同样需要 quote
    let drop_changes = vec![TableChange::DropIndex("idx_order".to_string())];
    let drop_sql = dialect.build_alter_table(&table, &drop_changes);
    assert!(
        drop_sql.contains("DROP INDEX `idx_order`"),
        "DropIndex 应 quote 索引名: {drop_sql}"
    );
    sqlx::query(sqlx::AssertSqlSafe(drop_sql.as_str()))
        .execute(&pool)
        .await
        .expect("DropIndex 被真库接受");

    // AddForeignKey 约束名 quote
    let ref_table = unique_table("mx_my2b_ref");
    let ref_ddl = format!(
        "CREATE TABLE {} (`id` BIGINT PRIMARY KEY)",
        dialect.quote(&ref_table)
    );
    sqlx::query(sqlx::AssertSqlSafe(ref_ddl.as_str()))
        .execute(&pool)
        .await
        .expect("create ref table");
    let fk_changes = vec![TableChange::AddForeignKey {
        columns: vec!["order".to_string()],
        reference_table: ref_table.clone(),
        reference_columns: vec!["id".to_string()],
    }];
    let fk_sql = dialect.build_alter_table(&table, &fk_changes);
    assert!(
        fk_sql.contains("ADD CONSTRAINT `fk_") && fk_sql.contains("`order`"),
        "AddForeignKey 应 quote 约束名与列名: {fk_sql}"
    );
    sqlx::query(sqlx::AssertSqlSafe(fk_sql.as_str()))
        .execute(&pool)
        .await
        .expect("AddForeignKey 被真库接受");

    drop_table(&pool, &table).await;
    drop_table(&pool, &ref_table).await;
}

// ---------------------------------------------------------------------------
// MY3 类型与 DDL
// ---------------------------------------------------------------------------

/// MY3：AUTO_INCREMENT 主键回填路径（LAST_INSERT_ID()，sz-pay 生产路径）；
/// TEXT/BLOB/JSON/DECIMAL 类型往返；utf8mb4 中文与 emoji 往返。
#[tokio::test]
#[ignore = "需要 MySQL 运行（SZ_ORM_MYSQL_URL 注入）"]
async fn matrix_my3_autoinc_and_type_roundtrip() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: SZ_ORM_MYSQL_URL 未配置或不可达");
        return;
    };
    let dialect = mysql_dialect();
    assert_eq!(
        dialect.last_insert_id_sql(),
        Some("LAST_INSERT_ID()"),
        "MySQL 主键回填通道：LAST_INSERT_ID()"
    );

    let table = unique_table("mx_my3");
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
            name: "body".to_string(),
            sql_type: "TEXT".to_string(),
            nullable: true,
            default: None,
            auto_increment: false,
            primary_key: false,
        },
        ColumnDef {
            name: "blob".to_string(),
            sql_type: "BLOB".to_string(),
            nullable: true,
            default: None,
            auto_increment: false,
            primary_key: false,
        },
        ColumnDef {
            name: "meta".to_string(),
            sql_type: "JSON".to_string(),
            nullable: true,
            default: None,
            auto_increment: false,
            primary_key: false,
        },
        ColumnDef {
            name: "amount".to_string(),
            sql_type: "DECIMAL(20,4)".to_string(),
            nullable: true,
            default: None,
            auto_increment: false,
            primary_key: false,
        },
    ];
    let ddl = dialect.build_create_table(&table, &columns);
    assert!(
        ddl.contains("AUTO_INCREMENT"),
        "建表 SQL 含 AUTO_INCREMENT: {ddl}"
    );
    assert!(ddl.contains("`meta` JSON"), "JSON 列类型生成面: {ddl}");
    sqlx::query(sqlx::AssertSqlSafe(ddl.as_str()))
        .execute(&pool)
        .await
        .expect("create table");

    // utf8mb4 中文 + emoji + TEXT/BLOB/JSON/DECIMAL 往返（独占连接保证 LAST_INSERT_ID 会话正确）
    let body = "鲜视达 sz-orm 🚀 中文与 emoji 混排——utf8mb4 往返载荷";
    let blob: Vec<u8> = vec![0x00, 0x01, 0x7F, 0x80, 0xFF, 0xFE];
    let meta_json = r#"{"k":"鲜视达🚀","n":3}"#;
    let amount = "12345678901234.5678";

    let mut conn = pool.acquire().await.expect("acquire");
    let insert = format!(
        "INSERT INTO {} (`body`, `blob`, `meta`, `amount`) VALUES (?, ?, ?, ?)",
        dialect.quote(&table)
    );
    let result = q(&insert)
        .bind(body)
        .bind(&blob)
        .bind(meta_json)
        .bind(amount)
        .execute(&mut *conn)
        .await
        .expect("insert");
    assert_eq!(result.rows_affected(), 1);

    // 主键回填：同会话 LAST_INSERT_ID()（方言 last_insert_id_sql 通道）
    // 注意：MySQL 协议中 LAST_INSERT_ID() 为 BIGINT UNSIGNED，sqlx 须按 u64 解码
    let backfill_sql = dialect.last_insert_id_sql().expect("mysql 必须支持");
    let row = q(&format!("SELECT {backfill_sql} AS last_id"))
        .fetch_one(&mut *conn)
        .await
        .expect("last_insert_id");
    let last_id: u64 = row.get("last_id");
    assert_eq!(last_id, 1, "AUTO_INCREMENT 回填 = 首行 id=1");

    // 往返断言
    let sel = format!(
        "SELECT `id`, `body`, `blob`, JSON_UNQUOTE(JSON_EXTRACT(`meta`, '$.k')) AS k, CAST(`amount` AS CHAR) AS amount_str FROM {}",
        dialect.quote(&table)
    );
    let row = q(&sel).fetch_one(&mut *conn).await.expect("select");
    assert_eq!(
        row.get::<i64, _>("id"),
        last_id as i64,
        "回填 id 与实际行一致"
    );
    assert_eq!(
        row.get::<String, _>("body"),
        body,
        "TEXT utf8mb4 中文+emoji 往返"
    );
    assert_eq!(row.get::<Vec<u8>, _>("blob"), blob, "BLOB 字节往返");
    assert_eq!(
        row.get::<String, _>("k"),
        "鲜视达🚀",
        "JSON 列 JSON_EXTRACT 往返"
    );
    assert_eq!(
        row.get::<String, _>("amount_str"),
        amount,
        "DECIMAL(20,4) 精度往返"
    );

    // 方言 json_extract 生成面被真库接受（JSON_EXTRACT 返回 JSON 类型，测试侧 CAST 解码）
    // json_extract quote 横向修复批：列名 quote（此处传裸名，方言内 quote——调用方不再预 quote）
    let je = dialect.json_extract("meta", "$.n");
    assert_eq!(je, "JSON_EXTRACT(`meta`, '$.n')");
    let je_sql = format!(
        "SELECT CAST({je} AS SIGNED) AS n FROM {}",
        dialect.quote(&table)
    );
    let row = q(&je_sql)
        .fetch_one(&mut *conn)
        .await
        .expect("json_extract");
    assert_eq!(row.get::<i64, _>("n"), 3);

    drop(conn);
    drop_table(&pool, &table).await;
}

// ---------------------------------------------------------------------------
// MY4 UPSERT 与批量
// ---------------------------------------------------------------------------

/// MY4：`INSERT .. ON DUPLICATE KEY UPDATE`（build_batch_upsert_with_params 生成面）；
/// 批量 INSERT 1000+ 行；65535 占位符协议极限探针。
#[tokio::test]
#[ignore = "需要 MySQL 运行（SZ_ORM_MYSQL_URL 注入）"]
async fn matrix_my4_upsert_and_batch_limits() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: SZ_ORM_MYSQL_URL 未配置或不可达");
        return;
    };
    let dialect = mysql_dialect();
    let table = unique_table("mx_my4");
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
            name: "name".to_string(),
            sql_type: "VARCHAR(100)".to_string(),
            nullable: false,
            default: None,
            auto_increment: false,
            primary_key: false,
        },
        ColumnDef {
            name: "val".to_string(),
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

    // 方言生成面：ON DUPLICATE KEY UPDATE `name`=VALUES(`name`)
    let mk_row = |id: i64, name: &str| {
        let mut m = HashMap::new();
        m.insert("id".to_string(), Value::I64(id));
        m.insert("name".to_string(), Value::String(name.to_string()));
        m.insert("val".to_string(), Value::I64(id * 10));
        m
    };
    let rows = vec![mk_row(1, "a"), mk_row(2, "b"), mk_row(3, "c")];
    let (upsert_sql, params) = qb(&table)
        .build_batch_upsert_with_params(&rows, &[], &["name"])
        .expect("upsert");
    assert!(
        upsert_sql.contains("ON DUPLICATE KEY UPDATE `name`=VALUES(`name`)"),
        "MySQL upsert 冲突子句生成面: {upsert_sql}"
    );
    bind_params!(q(&upsert_sql), params)
        .execute(&pool)
        .await
        .expect("upsert 首插");

    // 冲突路径：同 id 不同 name → UPDATE 生效
    let rows2 = vec![mk_row(2, "b2"), mk_row(4, "d")];
    let (upsert2, params2) = qb(&table)
        .build_batch_upsert_with_params(&rows2, &[], &["name"])
        .expect("upsert2");
    let res = bind_params!(q(&upsert2), params2)
        .execute(&pool)
        .await
        .expect("upsert 冲突路径");
    let (sel, sel_p) = qb(&table)
        .select(vec!["id"])
        .unwrap()
        .where_eq("name", Value::String("b2".to_string()))
        .build_select_with_params();
    let got = bind_params!(q(&sel), sel_p)
        .fetch_all(&pool)
        .await
        .expect("select");
    assert_eq!(got.len(), 1, "ODKU 冲突路径已更新 name");
    assert_eq!(got[0].get::<i64, _>("id"), 2);
    let (cnt_sql, cnt_p) = qb(&table)
        .select_expr(vec!["COUNT(*) AS c"])
        .build_select_with_params();
    let c: i64 = bind_params!(q(&cnt_sql), cnt_p)
        .fetch_one(&pool)
        .await
        .expect("count")
        .get("c");
    assert_eq!(
        c,
        4,
        "冲突行更新而非新增（1+2+3+4 共 4 行）,affected={}",
        res.rows_affected()
    );

    // 批量 INSERT 1000 行（参数化，1000×3=3000 占位符 < 65535 协议极限）
    let batch: Vec<HashMap<String, Value>> = (0..1000).map(|i| mk_row(1000 + i, "n")).collect();
    let (batch_sql, batch_params) = qb(&table).build_batch_insert_with_params(&batch);
    bind_params!(q(&batch_sql), batch_params)
        .execute(&pool)
        .await
        .expect("批量 1000 行");
    let (cnt2, cnt2_p) = qb(&table)
        .select_expr(vec!["COUNT(*) AS c"])
        .build_select_with_params();
    let c2: i64 = bind_params!(q(&cnt2), cnt2_p)
        .fetch_one(&pool)
        .await
        .expect("count2")
        .get("c");
    assert_eq!(c2, 1004);

    // 参数上限探针：MySQL 协议每语句占位符上限 65535
    // 21845 行 × 3 列 = 65535 恰好达上限 → 被真库接受
    let max_rows = 65535usize / 3;
    let ok_batch: Vec<HashMap<String, Value>> = (0..max_rows)
        .map(|i| mk_row(100000 + i as i64, "p"))
        .collect();
    let (ok_sql, ok_params) = qb(&table).build_batch_insert_with_params(&ok_batch);
    bind_params!(q(&ok_sql), ok_params)
        .execute(&pool)
        .await
        .expect("65535 占位符恰达协议上限应被接受");

    // 21846 行 × 3 列 = 65538 超限 → 真库拒绝（ER_PS_MANY_PARAM）
    let over_batch: Vec<HashMap<String, Value>> = (0..max_rows + 1)
        .map(|i| mk_row(500000 + i as i64, "q"))
        .collect();
    let (over_sql, over_params) = qb(&table).build_batch_insert_with_params(&over_batch);
    let over = bind_params!(q(&over_sql), over_params).execute(&pool).await;
    assert!(over.is_err(), "65538 占位符应被真库拒绝（协议上限 65535）");

    // max_allowed_packet 探针（环境登记，本机 1GB；@@max_allowed_packet 为 UNSIGNED）
    let map_packet: u64 = sqlx::query("SELECT @@max_allowed_packet AS p")
        .fetch_one(&pool)
        .await
        .expect("packet")
        .get("p");
    assert!(
        map_packet >= 4 * 1024 * 1024,
        "max_allowed_packet ≥ 4MB（实测 {map_packet}）"
    );

    drop_table(&pool, &table).await;
}

// ---------------------------------------------------------------------------
// MY5 分页与窗口
// ---------------------------------------------------------------------------

/// MY5：`LIMIT n OFFSET m`；offset-only 的 `LIMIT 18446744073709551615` 占位；
/// CTE（`WITH`）；窗口函数（`OVER`）；`GROUP_CONCAT` separator 与默认 1024 截断。
#[tokio::test]
#[ignore = "需要 MySQL 运行（SZ_ORM_MYSQL_URL 注入）"]
async fn matrix_my5_pagination_cte_window_groupconcat() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: SZ_ORM_MYSQL_URL 未配置或不可达");
        return;
    };
    let dialect = mysql_dialect();
    let table = unique_table("mx_my5");
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
            name: "val".to_string(),
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
    let insert = format!("INSERT INTO {} (`val`) VALUES (?)", dialect.quote(&table));
    for i in 0..30 {
        q(&insert).bind(i).execute(&pool).await.expect("insert");
    }

    // 分页：build_pagination(page=2, limit=10) → LIMIT 10 OFFSET 10
    let (base, _) = qb(&table)
        .select(vec!["id"])
        .unwrap()
        .order_by("id")
        .build_select_with_params();
    let paged = dialect.build_pagination(&base, 2, 10);
    assert!(paged.ends_with("LIMIT 10 OFFSET 10"), "分页生成面: {paged}");
    let rows = q(&paged).fetch_all(&pool).await.expect("page2");
    let ids: Vec<i64> = rows.iter().map(|r| r.get::<i64, _>("id")).collect();
    assert_eq!(ids, (11..=20).collect::<Vec<i64>>(), "第 2 页应为 id 11~20");

    // offset-only：MySQL 语法要求 OFFSET 必须跟随 LIMIT → 方言用 u64::MAX 占位
    let clause = dialect
        .build_pagination_clause(None, Some(5))
        .expect("offset-only clause");
    assert_eq!(clause, " LIMIT 18446744073709551615 OFFSET 5");
    let offset_sql = format!("{base}{clause}");
    let rows = q(&offset_sql).fetch_all(&pool).await.expect("offset5");
    assert_eq!(
        rows.len(),
        25,
        "跳过前 5 行剩 25 行（u64::MAX 占位被真库接受）"
    );

    // CTE（8.0 透传）：WITH cte AS (SELECT .. WHERE ..) SELECT COUNT(*)
    let (inner, inner_params) = qb(&table)
        .select(vec!["id", "val"])
        .unwrap()
        .where_gt("val", Value::I64(20))
        .build_select_with_params();
    let cte_sql = format!("WITH cte AS ({inner}) SELECT COUNT(*) AS c FROM cte");
    let c: i64 = bind_params!(q(&cte_sql), inner_params)
        .fetch_one(&pool)
        .await
        .expect("cte")
        .get("c");
    assert_eq!(c, 9, "CTE 透传：val > 20 共 9 行（21~29）");

    // 窗口函数（8.0 透传）：ROW_NUMBER() OVER (ORDER BY val DESC)
    let win_sql = format!(
        "SELECT `id`, ROW_NUMBER() OVER (ORDER BY `val` DESC) AS rn FROM {} WHERE `val` < ?",
        dialect.quote(&table)
    );
    let rows = q(&win_sql)
        .bind(3i64)
        .fetch_all(&pool)
        .await
        .expect("window");
    // ROW_NUMBER() 在 MySQL 协议中为 BIGINT UNSIGNED，sqlx 须按 u64 解码
    let rns: Vec<u64> = rows.iter().map(|r| r.get::<u64, _>("rn")).collect();
    assert_eq!(rns, vec![1, 2, 3], "窗口函数透传：val 2,1,0 → rn 1,2,3");

    // GROUP_CONCAT separator 语义 + 默认 1024 截断（环境特有）
    let gc_table = unique_table("mx_my5_gc");
    let gc_ddl = format!(
        "CREATE TABLE {} (`id` BIGINT PRIMARY KEY, `name` VARCHAR(32))",
        dialect.quote(&gc_table)
    );
    sqlx::query(sqlx::AssertSqlSafe(gc_ddl.as_str()))
        .execute(&pool)
        .await
        .expect("create gc table");
    let gc_insert = format!(
        "INSERT INTO {} (`id`, `name`) VALUES (?, ?)",
        dialect.quote(&gc_table)
    );
    for i in 0..200 {
        q(&gc_insert)
            .bind(i)
            .bind(format!("n{i:07}")) // 每值 8 字符
            .execute(&pool)
            .await
            .expect("gc insert");
    }
    let gc_sql = format!(
        "SELECT GROUP_CONCAT(`name` SEPARATOR ';') AS g FROM {}",
        dialect.quote(&gc_table)
    );
    let g: String = q(&gc_sql)
        .fetch_one(&pool)
        .await
        .expect("group_concat")
        .get("g");
    let max_len: u64 = sqlx::query("SELECT @@group_concat_max_len AS m")
        .fetch_one(&pool)
        .await
        .expect("gc max")
        .get("m");
    let total = 200 * 8 + 199; // 200 个 8 字符值 + 199 个分隔符
    let expected = (total as u64).min(max_len) as usize;
    assert_eq!(
        g.len(),
        expected,
        "GROUP_CONCAT 截断于 group_concat_max_len（环境默认 1024）"
    );
    let parts: Vec<&str> = g.split(';').collect();
    // 每条目 8 字符 + 分隔符 1 字符 = 9；div_ceil 同时覆盖
    // 完整拼接（1799 → 200 条）与截断（1024 → 113 完整 + 1 截断尾部 = 114）
    assert_eq!(
        parts.len() as u64,
        (expected as u64).div_ceil(9),
        "SEPARATOR ';' 语义正确"
    );
    drop_table(&pool, &gc_table).await;

    drop_table(&pool, &table).await;
}

// ---------------------------------------------------------------------------
// MY6 边缘语义
// ---------------------------------------------------------------------------

/// MY6：三值 NULL（`where_eq(col, Value::Null)` 必须生成 `IS NULL` 而非 `= NULL`）；
/// 除零（sql_mode 探针）；隐式类型转换；空串 vs NULL；弱类型。
#[tokio::test]
#[ignore = "需要 MySQL 运行（SZ_ORM_MYSQL_URL 注入）"]
async fn matrix_my6_edge_semantics() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: SZ_ORM_MYSQL_URL 未配置或不可达");
        return;
    };
    let dialect = mysql_dialect();
    let table = unique_table("mx_my6");
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
            name: "val".to_string(),
            sql_type: "BIGINT".to_string(),
            nullable: true,
            default: None,
            auto_increment: false,
            primary_key: false,
        },
        ColumnDef {
            name: "txt".to_string(),
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
    let insert = format!(
        "INSERT INTO {} (`val`, `txt`) VALUES (?, ?)",
        dialect.quote(&table)
    );
    for (val, txt) in [(Some(1i64), "x"), (None, "y"), (Some(2i64), "")] {
        q(&insert)
            .bind(val)
            .bind(txt)
            .execute(&pool)
            .await
            .expect("insert");
    }

    // --- 三值 NULL：where_eq(col, Value::Null) 生成 IS NULL（方言缺口 G1 修复锚点） ---
    let (sql_null, params_null) = qb(&table)
        .select(vec!["id"])
        .unwrap()
        .where_eq("val", Value::Null)
        .build_select_with_params();
    assert!(
        sql_null.contains("`val` IS NULL") && !sql_null.contains("= ?"),
        "where_eq Null 必须生成 IS NULL 而非 = ?: {sql_null}"
    );
    assert!(params_null.is_empty(), "IS NULL 不应产生绑定参数");
    let rows = q(&sql_null).fetch_all(&pool).await.expect("is null");
    assert_eq!(rows.len(), 1, "IS NULL 命中 1 行（id=2）");

    let (sql_notnull, _) = qb(&table)
        .select(vec!["id"])
        .unwrap()
        .where_ne("val", Value::Null)
        .build_select_with_params();
    assert!(
        sql_notnull.contains("`val` IS NOT NULL"),
        "where_ne Null 必须生成 IS NOT NULL: {sql_notnull}"
    );
    let rows = q(&sql_notnull).fetch_all(&pool).await.expect("is not null");
    assert_eq!(rows.len(), 2, "IS NOT NULL 命中 2 行");

    // 内联路径（sql() 参数内联渲染）同样不得生成 `= NULL`
    let inline = qb(&table)
        .select(vec!["id"])
        .unwrap()
        .where_eq("val", Value::Null)
        .sql();
    assert!(
        inline.contains("IS NULL") && !inline.contains("= NULL"),
        "内联路径 where_eq Null 同样生成 IS NULL: {inline}"
    );

    // MySQL 语义锚定：`= NULL` 恒空（三值逻辑）
    let raw = format!(
        "SELECT COUNT(*) AS c FROM {} WHERE `val` = ?",
        dialect.quote(&table)
    );
    let c: i64 = q(&raw)
        .bind(Option::<i64>::None)
        .fetch_one(&pool)
        .await
        .expect("three-valued")
        .get("c");
    assert_eq!(c, 0, "MySQL 三值逻辑：val = NULL 恒为 UNKNOWN → 0 行");

    // --- 除零探针：SELECT 1/0 在 SELECT 上下文恒 NULL；INSERT 受 sql_mode 影响 ---
    let div: Option<f64> = sqlx::query_scalar("SELECT 1/0")
        .fetch_one(&pool)
        .await
        .expect("div");
    assert!(
        div.is_none(),
        "SELECT 1/0 → NULL（SELECT 上下文与 sql_mode 无关）"
    );
    let mode: String = sqlx::query("SELECT @@sql_mode AS m")
        .fetch_one(&pool)
        .await
        .expect("sql_mode")
        .get("m");
    let div_table = unique_table("mx_my6_div");
    let div_ddl = format!(
        "CREATE TABLE {} (`id` BIGINT PRIMARY KEY AUTO_INCREMENT, `v` BIGINT)",
        dialect.quote(&div_table)
    );
    sqlx::query(sqlx::AssertSqlSafe(div_ddl.as_str()))
        .execute(&pool)
        .await
        .expect("create div table");
    let div_insert = format!(
        "INSERT INTO {} (`v`) VALUES (1/0)",
        dialect.quote(&div_table)
    );
    let strict_div =
        mode.contains("STRICT_TRANS_TABLES") && mode.contains("ERROR_FOR_DIVISION_BY_ZERO");
    let div_res = sqlx::query(sqlx::AssertSqlSafe(div_insert.as_str()))
        .execute(&pool)
        .await;
    if strict_div {
        assert!(
            div_res.is_err(),
            "STRICT + ERROR_FOR_DIVISION_BY_ZERO 时 INSERT 1/0 应报错"
        );
    } else {
        assert!(
            div_res.is_ok(),
            "当前 sql_mode（{mode}）下 INSERT 1/0 应存 NULL，实际错误: {:?}",
            div_res.as_ref().err()
        );
        let v: Option<i64> = q(&format!("SELECT `v` FROM {}", dialect.quote(&div_table)))
            .fetch_one(&pool)
            .await
            .expect("div value")
            .get("v");
        assert!(
            v.is_none(),
            "除零插入值为 NULL（宝塔默认无 ERROR_FOR_DIVISION_BY_ZERO，环境特有登记）"
        );
    }
    drop_table(&pool, &div_table).await;

    // --- 隐式类型转换：字符串参与算术（'5'+1=6，整型通道） ---
    let n: i64 = sqlx::query_scalar("SELECT ? + 1")
        .bind("5")
        .fetch_one(&pool)
        .await
        .expect("implicit");
    assert_eq!(n, 6, "MySQL 弱类型：'5' + 1 = 6");

    // --- 空串 vs NULL 有别（MySQL 有别于 Oracle） ---
    let (empty_sql, empty_p) = qb(&table)
        .select(vec!["id"])
        .unwrap()
        .where_eq("txt", Value::String(String::new()))
        .build_select_with_params();
    let rows = bind_params!(q(&empty_sql), empty_p)
        .fetch_all(&pool)
        .await
        .expect("empty");
    assert_eq!(rows.len(), 1, "空串是一等值：WHERE txt = '' 命中 1 行");
    let (isnull_sql, _) = qb(&table)
        .select(vec!["id"])
        .unwrap()
        .where_null("txt")
        .build_select_with_params();
    let rows = q(&isnull_sql).fetch_all(&pool).await.expect("txt null");
    assert_eq!(rows.len(), 0, "空串不等于 NULL：txt IS NULL 命中 0 行");

    // --- 弱类型：'false' = 0 为真 ---
    let weak: i64 = sqlx::query_scalar("SELECT 'false' = ?")
        .bind(0i64)
        .fetch_one(&pool)
        .await
        .expect("weak typing");
    assert_eq!(
        weak, 1,
        "MySQL 弱类型：'false' 隐式转型为 0，'false' = 0 为真（环境语义登记）"
    );

    drop_table(&pool, &table).await;
}

// ---------------------------------------------------------------------------
// MY7 建表表选项（B1） / MY8 外键动作（B2） / MY9 全文检索三模式（B3） / MY10 CTE builder（B4）
// ---------------------------------------------------------------------------

/// MY7（B1）：`build_create_table_with_options`——CHARSET/COLLATE/ENGINE 落到
/// 真库并经 information_schema 验证；既有 build_create_table 零破坏。
#[tokio::test]
#[ignore = "需要 MySQL 运行（SZ_ORM_MYSQL_URL 注入）"]
async fn matrix_my7_create_table_options() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: SZ_ORM_MYSQL_URL 未配置或不可达");
        return;
    };
    let dialect = mysql_dialect();
    let table = unique_table("mx_my7");
    let columns = vec![ColumnDef {
        name: "id".to_string(),
        sql_type: "BIGINT".to_string(),
        nullable: false,
        default: None,
        auto_increment: true,
        primary_key: true,
    }];
    // 零破坏基线：既有 build_create_table 产物不带表选项
    let base = dialect.build_create_table(&table, &columns);
    assert!(!base.contains("CHARSET"), "既有行为不变: {base}");

    let ddl = dialect.build_create_table_with_options(
        &table,
        &columns,
        &[
            TableOption::Engine("InnoDB".to_string()),
            TableOption::Charset("utf8mb4".to_string()),
            TableOption::Collate("utf8mb4_unicode_ci".to_string()),
        ],
    );
    sqlx::query(sqlx::AssertSqlSafe(ddl.as_str()))
        .execute(&pool)
        .await
        .expect("带表选项建表被真库接受");

    // 元数据面验证：ENGINE 与 COLLATION 真实生效
    let row = sqlx::query(
        "SELECT ENGINE, TABLE_COLLATION FROM information_schema.TABLES \
         WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = ?",
    )
    .bind(&table)
    .fetch_one(&pool)
    .await
    .expect("information_schema");
    let engine: String = row.get("ENGINE");
    let collation: String = row.get("TABLE_COLLATION");
    assert_eq!(engine, "InnoDB", "ENGINE 选项生效");
    assert_eq!(collation, "utf8mb4_unicode_ci", "COLLATE 选项生效");

    drop_table(&pool, &table).await;
}

/// MY8（B2）：`build_add_foreign_key`——ON DELETE CASCADE 行为级证明
/// （删除父行 → 子行级联消失；无动作时默认拒绝，行为即证据）。
#[tokio::test]
#[ignore = "需要 MySQL 运行（SZ_ORM_MYSQL_URL 注入）"]
async fn matrix_my8_fk_actions() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: SZ_ORM_MYSQL_URL 未配置或不可达");
        return;
    };
    let dialect = mysql_dialect();
    let parent = unique_table("mx_my8_parent");
    let child = unique_table("mx_my8_child");
    let parent_cols = vec![ColumnDef {
        name: "id".to_string(),
        sql_type: "BIGINT".to_string(),
        nullable: false,
        default: None,
        auto_increment: true,
        primary_key: true,
    }];
    let child_cols = vec![
        ColumnDef {
            name: "id".to_string(),
            sql_type: "BIGINT".to_string(),
            nullable: false,
            default: None,
            auto_increment: true,
            primary_key: true,
        },
        ColumnDef {
            name: "parent_id".to_string(),
            sql_type: "BIGINT".to_string(),
            nullable: false,
            default: None,
            auto_increment: false,
            primary_key: false,
        },
    ];
    sqlx::query(sqlx::AssertSqlSafe(
        dialect.build_create_table(&parent, &parent_cols).as_str(),
    ))
    .execute(&pool)
    .await
    .expect("create parent");
    sqlx::query(sqlx::AssertSqlSafe(
        dialect.build_create_table(&child, &child_cols).as_str(),
    ))
    .execute(&pool)
    .await
    .expect("create child");

    // 方言生成面：ON DELETE CASCADE 约束 DDL，真库接受
    let fk = ForeignKeyDef {
        columns: vec!["parent_id".to_string()],
        reference_table: parent.clone(),
        reference_columns: vec!["id".to_string()],
        on_delete: Some("CASCADE".to_string()),
        on_update: None,
    };
    let fk_ddl = dialect
        .build_add_foreign_key(&child, &fk)
        .expect("valid fk def");
    assert!(
        fk_ddl.contains("ON DELETE CASCADE"),
        "生成面含动作: {fk_ddl}"
    );
    sqlx::query(sqlx::AssertSqlSafe(fk_ddl.as_str()))
        .execute(&pool)
        .await
        .expect("ON DELETE CASCADE 外键被真库接受");

    // 行为证明：删父 → 子级联消失
    let ins_parent = format!("INSERT INTO {} () VALUES ()", dialect.quote(&parent));
    sqlx::query(sqlx::AssertSqlSafe(ins_parent.as_str()))
        .execute(&pool)
        .await
        .expect("insert parent");
    let ins_child = format!(
        "INSERT INTO {} (`parent_id`) VALUES (?)",
        dialect.quote(&child)
    );
    q(&ins_child)
        .bind(1i64)
        .execute(&pool)
        .await
        .expect("insert child");
    q(&ins_child)
        .bind(1i64)
        .execute(&pool)
        .await
        .expect("insert child 2");

    let del = format!("DELETE FROM {} WHERE `id` = 1", dialect.quote(&parent));
    sqlx::query(sqlx::AssertSqlSafe(del.as_str()))
        .execute(&pool)
        .await
        .expect("delete parent");
    let cnt_sql = format!("SELECT COUNT(*) AS c FROM {}", dialect.quote(&child));
    let c: i64 = q(&cnt_sql).fetch_one(&pool).await.expect("count").get("c");
    assert_eq!(c, 0, "ON DELETE CASCADE 行为级证明：子行已级联删除");

    // 非法动作 fail-closed（生成侧拒绝，DDL 不出笼）
    let bad = ForeignKeyDef {
        columns: vec!["parent_id".to_string()],
        reference_table: parent.clone(),
        reference_columns: vec!["id".to_string()],
        on_delete: Some("DROP TABLE x".to_string()),
        on_update: None,
    };
    assert!(dialect.build_add_foreign_key(&child, &bad).is_err());

    drop_table(&pool, &child).await;
    drop_table(&pool, &parent).await;
}

/// MY9（B3）：`full_text_search_mode` 三模式真库执行
/// （载荷用英文 token——InnoDB 默认分词器不切中文，中文全文检索需 ngram，登记不修）。
#[tokio::test]
#[ignore = "需要 MySQL 运行（SZ_ORM_MYSQL_URL 注入）"]
async fn matrix_my9_fulltext_modes() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: SZ_ORM_MYSQL_URL 未配置或不可达");
        return;
    };
    let dialect = mysql_dialect();
    let table = unique_table("mx_my9");
    let ddl = format!(
        "CREATE TABLE {} (`id` BIGINT PRIMARY KEY AUTO_INCREMENT, `body` TEXT, \
         FULLTEXT KEY `ft_body` (`body`)) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4",
        dialect.quote(&table)
    );
    sqlx::query(sqlx::AssertSqlSafe(ddl.as_str()))
        .execute(&pool)
        .await
        .expect("create fulltext table");
    let insert = format!("INSERT INTO {} (`body`) VALUES (?)", dialect.quote(&table));
    for body in [
        "mysql database engine",
        "rust programming language",
        "database indexing performance",
    ] {
        q(&insert).bind(body).execute(&pool).await.expect("insert");
    }

    // 自然语言模式（旧行为等价路径）
    let nl = dialect.full_text_search_mode(&["body"], "database", FullTextMode::NaturalLanguage);
    let nl_sql = format!(
        "SELECT COUNT(*) AS c FROM {} WHERE {}",
        dialect.quote(&table),
        nl
    );
    let c: i64 = q(&nl_sql).fetch_one(&pool).await.expect("nl").get("c");
    assert_eq!(c, 2, "NATURAL LANGUAGE 命中 2 行（r1/r3 含 database）");

    // 布尔模式：+database -mysql 操作符过滤
    let boolean =
        dialect.full_text_search_mode(&["body"], "+database -mysql", FullTextMode::Boolean);
    let b_sql = format!(
        "SELECT COUNT(*) AS c FROM {} WHERE {}",
        dialect.quote(&table),
        boolean
    );
    let c: i64 = q(&b_sql).fetch_one(&pool).await.expect("boolean").get("c");
    assert_eq!(c, 1, "BOOLEAN MODE 操作符生效：database 且无 mysql → 仅 r3");

    // 查询扩展模式：可执行且至少命中自然语言的结果
    let qe = dialect.full_text_search_mode(&["body"], "database", FullTextMode::QueryExpansion);
    let qe_sql = format!(
        "SELECT COUNT(*) AS c FROM {} WHERE {}",
        dialect.quote(&table),
        qe
    );
    let c: i64 = q(&qe_sql).fetch_one(&pool).await.expect("qe").get("c");
    assert!(c >= 2, "QUERY EXPANSION 可执行且不缩圈: {c}");

    drop_table(&pool, &table).await;
}

/// MY10（B4）：QueryBuilder CTE builder——递归组织树行为验证 + 参数顺序证明。
#[tokio::test]
#[ignore = "需要 MySQL 运行（SZ_ORM_MYSQL_URL 注入）"]
async fn matrix_my10_cte_builder() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: SZ_ORM_MYSQL_URL 未配置或不可达");
        return;
    };
    let dialect = mysql_dialect();
    let org = unique_table("mx_my10_org");
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
            name: "parent_id".to_string(),
            sql_type: "BIGINT".to_string(),
            nullable: true,
            default: None,
            auto_increment: false,
            primary_key: false,
        },
        ColumnDef {
            name: "name".to_string(),
            sql_type: "VARCHAR(50)".to_string(),
            nullable: false,
            default: None,
            auto_increment: false,
            primary_key: false,
        },
    ];
    sqlx::query(sqlx::AssertSqlSafe(
        dialect.build_create_table(&org, &columns).as_str(),
    ))
    .execute(&pool)
    .await
    .expect("create org");
    let insert = format!(
        "INSERT INTO {} (`parent_id`, `name`) VALUES (?, ?)",
        dialect.quote(&org)
    );
    // id1=root(depth0) id2=id4 挂 root(depth1) id3 挂 id2(depth2)
    for (pid, name) in [
        (None, "root"),
        (Some(1i64), "a"),
        (Some(2i64), "b"),
        (Some(1i64), "c"),
    ] {
        q(&insert)
            .bind(pid)
            .bind(name)
            .execute(&pool)
            .await
            .expect("insert");
    }

    // 递归 CTE（逃生口 SQL 引用 `org` 与自引用 `org_tree`）
    let cte_sql = format!(
        "SELECT `id`, `parent_id`, `name`, 1 AS depth FROM {} WHERE `parent_id` IS NULL \
         UNION ALL \
         SELECT c.`id`, c.`parent_id`, c.`name`, t.depth + 1 FROM {} c \
         INNER JOIN `org_tree` t ON c.`parent_id` = t.id",
        dialect.quote(&org),
        dialect.quote(&org)
    );
    let (sql, params) = qb("org_tree")
        .with_recursive_cte("org_tree", cte_sql, vec![])
        .unwrap()
        .select(vec!["id", "depth"])
        .unwrap()
        .order_by("id")
        .build_select_with_params();
    assert!(
        sql.starts_with("WITH RECURSIVE `org_tree` AS ("),
        "递归 CTE 前缀: {sql}"
    );
    let rows = bind_params!(q(&sql), params)
        .fetch_all(&pool)
        .await
        .expect("recursive cte");
    assert_eq!(rows.len(), 4, "整棵树 4 节点");
    let depths: Vec<i64> = rows.iter().map(|r| r.get::<i64, _>("depth")).collect();
    assert_eq!(
        depths,
        vec![1, 2, 3, 2],
        "深度 root=1, a/c=2, b=3（行为级递归验证）"
    );

    // 参数顺序证明：CTE 参数(1) 先于主查询参数(2) 绑定
    let kids_sql = format!(
        "SELECT `id` FROM {} WHERE `parent_id` = ?",
        dialect.quote(&org)
    );
    let (sql2, params2) = qb("kids")
        .with_cte("kids", kids_sql, vec![Value::I64(1)])
        .unwrap()
        .select(vec!["id"])
        .unwrap()
        .where_gt("id", Value::I64(2))
        .build_select_with_params();
    assert_eq!(params2, vec![Value::I64(1), Value::I64(2)], "CTE 参数在前");
    let rows = bind_params!(q(&sql2), params2)
        .fetch_all(&pool)
        .await
        .expect("cte params");
    let ids: Vec<i64> = rows.iter().map(|r| r.get::<i64, _>("id")).collect();
    assert_eq!(
        ids,
        vec![4],
        "root 子节点 2、4 且主查询 id > 2 → 仅 4（参数顺序行为证明）"
    );

    drop_table(&pool, &org).await;
}
