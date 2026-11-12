//! PostgreSQL 方言矩阵真连验证（PG1~PG7，2026-11-13）
//!
//! 按 MY/OR 方言矩阵模板，对 sz-orm `PostgreSqlDialect` 生成面做独立矩阵：
//! 每载荷 = 方言层生成 + 真库执行 + 行为断言三步。
//!
//! 三分类口径：
//! - sz-orm 方言缺口（生成 SQL 真库拒绝/结果错位）→ 修复
//! - PG 环境特有 → 登记
//! - 文档措辞 → 登记
//!
//! 运行方式：cargo test --package sz-orm-core --test integration_pg_matrix -- --ignored --nocapture

use sqlx::postgres::{PgPool, PgPoolOptions};
use sqlx::Row;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use sz_orm_core::dialect::{get_dialect, ColumnDef, FullTextMode, TableChange};
use sz_orm_core::{DbType, Model, ModelExt, QueryBuilder, Value};

const PG_URL_DEFAULT: &str = "postgres://postgres:test123@127.0.0.1:5432/sz_orm_test";

fn pg_url() -> String {
    std::env::var("SZ_ORM_PG_URL").unwrap_or_else(|_| PG_URL_DEFAULT.to_string())
}

static TABLE_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_table(prefix: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let counter = TABLE_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{}_{}_{}", prefix, nanos % 1_000_000, counter)
}

async fn setup_pool() -> PgPool {
    PgPoolOptions::new()
        .max_connections(8)
        .acquire_timeout(Duration::from_secs(30))
        .connect(&pg_url())
        .await
        .expect("pg connect failed - is PostgreSQL 18 running?")
}

/// 执行方言生成的 SQL（? → $n 重写；批量 insert/upsert 的 $n 原生形态原样保留——
/// 重写器对已含 $n 的 SQL 不混用，与 sz-orm-sqlx pg_rewrite_placeholders 同语义）
async fn exec_sql(pool: &PgPool, sql: &str) {
    sqlx::query(sqlx::AssertSqlSafe(sql))
        .execute(pool)
        .await
        .unwrap_or_else(|e| panic!("exec failed: {e}\nSQL: {sql}"));
}

async fn drop_table(pool: &PgPool, table: &str) {
    let sql = format!("DROP TABLE IF EXISTS \"{}\"", table);
    let _ = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .execute(pool)
        .await;
}

async fn drop_index(pool: &PgPool, index: &str) {
    let sql = format!("DROP INDEX IF EXISTS \"{}\"", index);
    let _ = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .execute(pool)
        .await;
}

/// core 生成的占位符是 `?`；真实驱动层的 `? → $n` 重写在 sz-orm-sqlx
/// （pg_rewrite_placeholders）。本测试直连 sqlx，需要本地等价重写
/// （含单/双引号状态机，避免字面量内的 `?` 被误改写）。
fn pg_rewrite_placeholders_for_test(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len() + 8);
    let mut n = 0usize;
    let mut in_squote = false;
    let mut in_dquote = false;
    for ch in sql.chars() {
        match ch {
            '\'' if !in_dquote => {
                in_squote = !in_squote;
                out.push(ch);
            }
            '"' if !in_squote => {
                in_dquote = !in_dquote;
                out.push(ch);
            }
            '?' if !in_squote && !in_dquote => {
                n += 1;
                out.push_str(&format!("${}", n));
            }
            _ => out.push(ch),
        }
    }
    out
}

fn bind_value_pg<'q>(
    q: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
    v: &'q Value,
) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments> {
    match v {
        Value::Null => q.bind(Option::<i64>::None),
        Value::Bool(b) => q.bind(*b),
        Value::I8(n) => q.bind(*n),
        Value::I16(n) => q.bind(*n),
        Value::I32(n) => q.bind(*n),
        Value::I64(n) => q.bind(*n),
        Value::U8(n) => q.bind(*n as i16),
        Value::U16(n) => q.bind(*n as i32),
        Value::U32(n) => q.bind(*n as i64),
        Value::U64(n) => q.bind(*n as i64),
        Value::F32(f) => q.bind(*f),
        Value::F64(f) => q.bind(*f),
        Value::String(s) => q.bind(s.as_str()),
        // NUMERIC 38 位：BigDecimal 通道（同 sz-orm-sqlx bind_pg_value R2 口径）
        Value::Decimal(s) => match s.parse::<bigdecimal::BigDecimal>() {
            Ok(d) => q.bind(d),
            Err(_) => q.bind(s.clone()),
        },
        Value::Uuid(s) => match sqlx::types::Uuid::parse_str(s) {
            Ok(u) => q.bind(u),
            Err(_) => q.bind(s.clone()),
        },
        Value::Date(s) | Value::DateTime(s) | Value::Time(s) | Value::Json(s) => q.bind(s.clone()),
        Value::Bytes(b) => q.bind(b.clone()),
        // Value 为 #[non_exhaustive]（集成测试是独立 crate，须留 wildcard）
        _ => q.bind(serde_json::to_string(v).unwrap_or_default()),
    }
}

/// 执行方言生成的参数化语句（? → $n 重写 + Value 绑定）
async fn exec_params(pool: &PgPool, sql: &str, params: &[Value]) {
    let mut q = sqlx::query(sqlx::AssertSqlSafe(
        pg_rewrite_placeholders_for_test(sql).as_str(),
    ));
    for v in params {
        q = bind_value_pg(q, v);
    }
    q.execute(pool)
        .await
        .unwrap_or_else(|e| panic!("exec_params failed: {e}\nSQL: {sql}"));
}

/// 执行参数化 SELECT，返回第一列字符串值列表
async fn fetch_scalars(pool: &PgPool, sql: &str, params: &[Value]) -> Vec<String> {
    let mut q = sqlx::query(sqlx::AssertSqlSafe(
        pg_rewrite_placeholders_for_test(sql).as_str(),
    ));
    for v in params {
        q = bind_value_pg(q, v);
    }
    let rows = q
        .fetch_all(pool)
        .await
        .unwrap_or_else(|e| panic!("fetch failed: {e}\nSQL: {sql}"));
    rows.iter()
        .map(|r| {
            // TEXT/int4/int8 列统一转字符串（count(*) 返回 int8）
            if let Ok(v) = r.try_get::<Option<String>, _>(0) {
                v.unwrap_or_default()
            } else if let Ok(v) = r.try_get::<i64, _>(0) {
                v.to_string()
            } else {
                r.try_get::<i32, _>(0).unwrap_or_default().to_string()
            }
        })
        .collect()
}

struct DummyModel;
impl Model for DummyModel {
    type PrimaryKey = i64;
    fn table_name() -> &'static str {
        "dummy"
    }
    fn pk(&self) -> Self::PrimaryKey {
        0
    }
    fn set_pk(&mut self, _pk: Self::PrimaryKey) {}
}

impl ModelExt for DummyModel {
    fn columns() -> Vec<&'static str> {
        vec!["id", "name", "value"]
    }
    fn fillable() -> Vec<&'static str> {
        vec!["name", "value"]
    }
    fn guarded() -> Vec<&'static str> {
        vec!["id"]
    }
    fn hidden() -> Vec<&'static str> {
        vec![]
    }
    fn relations() -> HashMap<&'static str, sz_orm_core::Relation> {
        HashMap::new()
    }
    fn fill(&mut self, _data: HashMap<String, Value>) {}
    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({})
    }
}

fn pg_dialect() -> Box<dyn sz_orm_core::dialect::Dialect> {
    get_dialect(DbType::PostgreSQL).expect("pg dialect")
}

/// 原始布尔片段桥接（测试内固定字符串，调用方信任口径同 select_expr）：
/// 让 `where_expr` 接受透传 SQL 片段（如 CTE 的 EXISTS 半连接）。
struct RawBool<'a>(&'a str);

impl sz_orm_core::typed_ast::TypedExpression for RawBool<'_> {
    type SqlType = sz_orm_core::typed_ast::Bool;
    fn to_sql(&self, _dialect: &dyn sz_orm_core::dialect::Dialect) -> (String, Vec<String>) {
        (self.0.to_string(), Vec::new())
    }
}

fn qb(table: &str) -> QueryBuilder<DummyModel> {
    QueryBuilder::new(pg_dialect()).table(table)
}

/// 用方言 build_create_table 生成并执行测试表（BIGSERIAL 主键 + name + value）
async fn create_dialect_table(pool: &PgPool, table: &str) {
    let dialect = pg_dialect();
    let columns = vec![
        ColumnDef {
            name: "id".to_string(),
            sql_type: "BIGSERIAL".to_string(),
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
    ];
    let sql = dialect.build_create_table(table, &columns);
    exec_sql(pool, &sql).await;
}

async fn insert_rows(pool: &PgPool, table: &str, rows: &[(&str, i64)]) {
    let dialect = pg_dialect();
    let mut m = HashMap::new();
    m.insert("name".to_string(), Value::String(rows[0].0.to_string()));
    m.insert("value".to_string(), Value::I64(rows[0].1));
    let (mut sql, mut params) = qb(table).build_insert_with_params(&m);
    for (name, v) in &rows[1..] {
        // 后续行直接拼 VALUES（参数化占位符追加）
        let mut m2 = HashMap::new();
        m2.insert("name".to_string(), Value::String(name.to_string()));
        m2.insert("value".to_string(), Value::I64(*v));
        let (s2, p2) = qb(table).build_insert_with_params(&m2);
        sql = format!("{}; {}", sql, s2);
        params.extend(p2);
        let _ = &dialect;
    }
    // 逐条执行（多语句带各自参数占位符编号需重置，sqlx 不支持一条多 INSERT）
    let mut m3 = HashMap::new();
    for (name, v) in rows {
        m3.insert("name".to_string(), Value::String(name.to_string()));
        m3.insert("value".to_string(), Value::I64(*v));
        let (s, p) = qb(table).build_insert_with_params(&m3);
        exec_params(pool, &s, &p).await;
    }
}

// =============================================================================
// PG1 标识符：双引号 quoting / 大小写保留 / 63 字节上限 / 约束名全 quote
// =============================================================================

/// PG1-a：混合大小写标识符经方言 quote 后真库往返，大小写保留
/// （PG 裸标识符折叠小写——与 Oracle 相反的方向，quote 保留语义对照）
#[tokio::test]
#[ignore = "需要 PostgreSQL 18 运行于 127.0.0.1:5432"]
async fn pg1_quoted_mixed_case_roundtrip() {
    let pool = setup_pool().await;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let mixed = format!("t_pg1case_{}_Mixed", nanos % 1_000_000);
    let dialect = pg_dialect();

    let columns = vec![
        ColumnDef {
            name: "id".to_string(),
            sql_type: "BIGSERIAL".to_string(),
            nullable: false,
            default: None,
            auto_increment: true,
            primary_key: true,
        },
        ColumnDef {
            name: "titleText".to_string(),
            sql_type: "TEXT".to_string(),
            nullable: false,
            default: None,
            auto_increment: false,
            primary_key: false,
        },
    ];
    let create_sql = dialect.build_create_table(&mixed, &columns);
    assert!(
        create_sql.contains("\"t_pg1case") && create_sql.contains("\"titleText\""),
        "build_create_table 应 quote 表名与列名：{}",
        create_sql
    );
    exec_sql(&pool, &create_sql).await;

    // 裸混合大小写表名在 PG 折叠为小写而找不到表（quote 必要性对照）
    let bare = format!("SELECT count(*) FROM {}", mixed);
    let err = sqlx::query(sqlx::AssertSqlSafe(bare.as_str()))
        .fetch_all(&pool)
        .await;
    assert!(
        err.is_err(),
        "裸混合大小写表名应被折叠为小写而找不到表（quote 保留语义对照）"
    );

    // 方言生成面：插入 + 查询（混合大小写列名）
    let mut data = HashMap::new();
    data.insert("titleText".to_string(), Value::String("你好".to_string()));
    let (insert_sql, params) = qb(&mixed).build_insert_with_params(&data);
    exec_params(&pool, &insert_sql, &params).await;

    let (sel_sql, sel_params) = qb(&mixed)
        .select_expr(vec!["\"titleText\""])
        .build_select_with_params();
    let rows = fetch_scalars(&pool, &sel_sql, &sel_params).await;
    assert_eq!(rows, vec!["你好".to_string()], "混合大小写列往返应保留");

    drop_table(&pool, &mixed).await;
}

/// PG1-b：63 字节标识符真库可用；>63 字节 quote_checked 拦截（G4 同型方言感知上限）
#[tokio::test]
#[ignore = "需要 PostgreSQL 18 运行于 127.0.0.1:5432"]
async fn pg1_identifier_63_byte_limit() {
    let pool = setup_pool().await;
    let dialect = pg_dialect();

    // 63 字节：PG NAMEDATALEN-1 上限，真库应接受
    let ok_name = "a".repeat(63);
    let sql = format!("CREATE TABLE \"{}\" (id INT)", ok_name);
    exec_sql(&pool, &sql).await;

    // quote_checked：63 通过 / 64 拒绝（方言感知上限，PG 默认 63）
    assert!(dialect.quote_checked(&ok_name).is_ok());
    let too_long = "b".repeat(64);
    assert!(
        dialect.quote_checked(&too_long).is_err(),
        "64 字节标识符应被 quote_checked 拦截"
    );

    drop_table(&pool, &ok_name).await;
}

/// PG1-c：build_alter_table 三臂（AddIndex/DropIndex/AddForeignKey）混合大小写真库往返 + FK 行为级
#[tokio::test]
#[ignore = "需要 PostgreSQL 18 运行于 127.0.0.1:5432"]
async fn pg1_alter_table_arms_quoted() {
    let pool = setup_pool().await;
    let dialect = pg_dialect();
    let table = unique_table("t_pg1fk");
    let parent = unique_table("t_pg1parent");

    let col = |name: &str, ty: &str| ColumnDef {
        name: name.to_string(),
        sql_type: ty.to_string(),
        nullable: false,
        default: None,
        auto_increment: false,
        primary_key: false,
    };
    exec_sql(
        &pool,
        &dialect.build_create_table(
            &parent,
            &vec![col("refId", "BIGINT PRIMARY KEY"), col("refName", "TEXT")],
        ),
    )
    .await;
    exec_sql(
        &pool,
        &dialect.build_create_table(
            &table,
            &vec![col("orderId", "BIGINT PRIMARY KEY"), col("refId", "BIGINT")],
        ),
    )
    .await;

    // AddIndex：索引名+列名 quote 往返
    let idx_name = format!("idx_{}_Mixed", table);
    let add_index = dialect.build_alter_table(
        &table,
        &[TableChange::AddIndex(
            idx_name.clone(),
            vec!["refId".to_string(), "orderId".to_string()],
        )],
    );
    assert!(
        add_index.contains(&format!("\"{}\"", idx_name)) && add_index.contains("\"refId\""),
        "AddIndex 索引名与列名应 quote：{}",
        add_index
    );
    exec_sql(&pool, &add_index).await;
    let cnt: (i64,) = sqlx::query_as(sqlx::AssertSqlSafe(
        format!(
            "SELECT count(*) FROM pg_indexes WHERE indexname = '{}'",
            idx_name
        )
        .as_str(),
    ))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        cnt.0, 1,
        "AddIndex 生成 SQL 应在真库建出索引：{}",
        add_index
    );

    // DropIndex
    let drop_index_sql =
        dialect.build_alter_table(&table, &[TableChange::DropIndex(idx_name.clone())]);
    exec_sql(&pool, &drop_index_sql).await;
    let cnt: (i64,) = sqlx::query_as(sqlx::AssertSqlSafe(
        format!(
            "SELECT count(*) FROM pg_indexes WHERE indexname = '{}'",
            idx_name
        )
        .as_str(),
    ))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(cnt.0, 0, "DropIndex 应删除索引");

    // AddForeignKey：约束名组装后 quote + 行为级（无父行插入被拒）
    let fk_sql = dialect.build_alter_table(
        &table,
        &[TableChange::AddForeignKey {
            columns: vec!["refId".to_string()],
            reference_table: parent.clone(),
            reference_columns: vec!["refId".to_string()],
        }],
    );
    assert!(
        fk_sql.contains("ADD CONSTRAINT \"fk_"),
        "FK 约束名应 quote：{}",
        fk_sql
    );
    exec_sql(&pool, &fk_sql).await;
    exec_sql(
        &pool,
        &format!(
            "INSERT INTO \"{}\" (\"refId\",\"refName\") VALUES (1,'p')",
            parent
        ),
    )
    .await;
    exec_sql(
        &pool,
        &format!(
            "INSERT INTO \"{}\" (\"orderId\",\"refId\") VALUES (10,1)",
            table
        ),
    )
    .await;
    let bad = format!(
        "INSERT INTO \"{}\" (\"orderId\",\"refId\") VALUES (11,99)",
        table
    );
    let err = sqlx::query(sqlx::AssertSqlSafe(bad.as_str()))
        .execute(&pool)
        .await;
    assert!(err.is_err(), "FK 应拒绝无父行插入：{}", fk_sql);

    drop_table(&pool, &table).await;
    drop_table(&pool, &parent).await;
}

// =============================================================================
// PG2 主键回填与 RETURNING
// =============================================================================

/// PG2：BIGSERIAL + INSERT/UPDATE/DELETE RETURNING 全家（方言生成面 + RETURNING 尾接）
#[tokio::test]
#[ignore = "需要 PostgreSQL 18 运行于 127.0.0.1:5432"]
async fn pg2_returning_family() {
    let pool = setup_pool().await;
    let table = unique_table("t_pg2ret");
    create_dialect_table(&pool, &table).await;
    assert!(pg_dialect().supports_returning(), "PG 应声明支持 RETURNING");

    let builder = qb(&table);

    // INSERT..RETURNING id
    let mut data = HashMap::new();
    data.insert("name".to_string(), Value::String("r1".to_string()));
    data.insert("value".to_string(), Value::I64(10));
    let (ins_sql, ins_params) = builder.build_insert_with_params(&data);
    let ins_sql = format!("{} RETURNING id", ins_sql);
    let mut q = sqlx::query(sqlx::AssertSqlSafe(
        pg_rewrite_placeholders_for_test(&ins_sql).as_str(),
    ));
    for v in &ins_params {
        q = bind_value_pg(q, v);
    }
    let rows = q.fetch_one(&pool).await.expect("insert returning");
    let id: i64 = rows.try_get(0).unwrap();
    assert!(id > 0, "BIGSERIAL 主键应经 RETURNING 回填");

    // UPDATE..RETURNING（build_update_with_params 生成面）
    let mut upd = HashMap::new();
    upd.insert("value".to_string(), Value::I64(20));
    let upd_builder = qb(&table).where_eq("id", Value::I64(id));
    let (upd_sql, upd_params) = upd_builder.build_update_with_params(&upd);
    let upd_sql = format!("{} RETURNING \"value\"", upd_sql);
    let mut q = sqlx::query(sqlx::AssertSqlSafe(
        pg_rewrite_placeholders_for_test(&upd_sql).as_str(),
    ));
    for v in &upd_params {
        q = bind_value_pg(q, v);
    }
    let rows = q.fetch_one(&pool).await.expect("update returning");
    let new_value: i64 = rows.try_get(0).unwrap();
    assert_eq!(new_value, 20, "UPDATE RETURNING 应回填新值");

    // DELETE..RETURNING（build_force_delete_with_params 生成面）
    let del_builder = qb(&table).where_eq("id", Value::I64(id));
    let (del_sql, del_params) = del_builder.build_force_delete_with_params();
    let del_sql = format!("{} RETURNING id", del_sql);
    let mut q = sqlx::query(sqlx::AssertSqlSafe(
        pg_rewrite_placeholders_for_test(&del_sql).as_str(),
    ));
    for v in &del_params {
        q = bind_value_pg(q, v);
    }
    let rows = q.fetch_one(&pool).await.expect("delete returning");
    let deleted_id: i64 = rows.try_get(0).unwrap();
    assert_eq!(deleted_id, id, "DELETE RETURNING 应回填被删主键");

    drop_table(&pool, &table).await;
}

// =============================================================================
// PG3 Upsert：ON CONFLICT 显式目标 / 缺失目标（约束推断）/ DO UPDATE excluded
// =============================================================================

fn row3(id: i64, name: &str, v: i64) -> HashMap<String, Value> {
    let mut m = HashMap::new();
    m.insert("id".to_string(), Value::I64(id));
    m.insert("name".to_string(), Value::String(name.to_string()));
    m.insert("value".to_string(), Value::I64(v));
    m
}

/// PG3-a：build_batch_upsert_with_params 生成 ON CONFLICT (col) DO UPDATE SET col=EXCLUDED.col
/// 真库行为级（冲突行更新、新行插入、未列入 update 的列保持旧值）
#[tokio::test]
#[ignore = "需要 PostgreSQL 18 运行于 127.0.0.1:5432"]
async fn pg3_upsert_do_update_excluded() {
    let pool = setup_pool().await;
    let table = unique_table("t_pg3ups");
    exec_sql(
        &pool,
        &format!(
            "CREATE TABLE \"{}\" (id BIGINT NOT NULL PRIMARY KEY, name VARCHAR(255) NOT NULL, value BIGINT)",
            table
        ),
    )
    .await;
    let builder = qb(&table);

    let (sql1, p1) = builder
        .build_batch_upsert_with_params(
            &[row3(1, "a", 100), row3(2, "b", 200)],
            &["id"],
            &["value"],
        )
        .expect("upsert build");
    exec_params(&pool, &sql1, &p1).await;
    let (sql2, p2) = builder
        .build_batch_upsert_with_params(
            &[row3(1, "a2", 111), row3(3, "c", 300)],
            &["id"],
            &["value"],
        )
        .expect("upsert build");
    exec_params(&pool, &sql2, &p2).await;

    // 行为断言：id=1 冲突更新（value=111，name 不在 update_columns → 保持 "a"）；共 3 行
    let (sel_sql, sel_params) = qb(&table)
        .select_expr(vec!["id", "name", "value"])
        .order_by("id")
        .build_select_with_params();
    let mut q = sqlx::query(sqlx::AssertSqlSafe(
        pg_rewrite_placeholders_for_test(&sel_sql).as_str(),
    ));
    for v in &sel_params {
        q = bind_value_pg(q, v);
    }
    let rows = q.fetch_all(&pool).await.expect("select after upsert");
    assert_eq!(rows.len(), 3, "upsert 后应共 3 行");
    let r1_name: String = rows[0].try_get("name").unwrap();
    let r1_value: i64 = rows[0].try_get("value").unwrap();
    assert_eq!(r1_name, "a", "name 不在 update_columns，应保持旧值");
    assert_eq!(
        r1_value, 111,
        "DO UPDATE SET value=EXCLUDED.value 应更新 value"
    );
    let r3_name: String = rows[2].try_get("name").unwrap();
    assert_eq!(r3_name, "c", "id=3 应为新插入行");

    drop_table(&pool, &table).await;
}

/// PG3-b：冲突目标缺失形态（insert_or_ignore → ON CONFLICT DO NOTHING，依赖约束推断）
#[tokio::test]
#[ignore = "需要 PostgreSQL 18 运行于 127.0.0.1:5432"]
async fn pg3_upsert_do_nothing_without_target() {
    let pool = setup_pool().await;
    let table = unique_table("t_pg3ign");
    exec_sql(
        &pool,
        &format!(
            "CREATE TABLE \"{}\" (id BIGINT NOT NULL PRIMARY KEY, name VARCHAR(255) NOT NULL)",
            table
        ),
    )
    .await;
    let builder = QueryBuilder::<DummyModel>::new(pg_dialect())
        .table(&table)
        .insert_or_ignore();

    let mut data = HashMap::new();
    data.insert("id".to_string(), Value::I64(1));
    data.insert("name".to_string(), Value::String("first".to_string()));
    let (sql1, p1) = builder.build_insert_with_params(&data);
    assert!(
        sql1.ends_with("ON CONFLICT DO NOTHING"),
        "PG insert_or_ignore 应尾接 ON CONFLICT DO NOTHING：{}",
        sql1
    );
    exec_params(&pool, &sql1, &p1).await;

    let mut data2 = HashMap::new();
    data2.insert("id".to_string(), Value::I64(1));
    data2.insert("name".to_string(), Value::String("second".to_string()));
    let (sql2, p2) = builder.build_insert_with_params(&data2);
    exec_params(&pool, &sql2, &p2).await; // 冲突被忽略，不报错

    // 行为断言：仍 1 行且 name 保持 first
    let (sel, sp) = qb(&table)
        .select_expr(vec!["name"])
        .build_select_with_params();
    let rows = fetch_scalars(&pool, &sel, &sp).await;
    assert_eq!(rows, vec!["first".to_string()], "冲突行应被忽略，原值保留");
    drop_table(&pool, &table).await;
}

// =============================================================================
// PG4 类型与 JSONB
// =============================================================================

/// PG4-a：:: cast + NUMERIC 38 位精度无损（Decimal 参数绑定走 BigDecimal 通道）
#[tokio::test]
#[ignore = "需要 PostgreSQL 18 运行于 127.0.0.1:5432"]
async fn pg4_cast_and_numeric_precision() {
    let pool = setup_pool().await;
    let table = unique_table("t_pg4num");
    exec_sql(
        &pool,
        &format!(
            "CREATE TABLE \"{}\" (id BIGSERIAL PRIMARY KEY, amt NUMERIC(38,0) NOT NULL)",
            table
        ),
    )
    .await;
    // Decimal 参数化插入 38 位 + 读回
    let (sql, params) = qb(&table)
        .select_expr(vec!["count(*)"])
        .build_select_with_params();
    let _ = (sql, params);
    let ins = format!("INSERT INTO \"{}\" (amt) VALUES (?)", table);
    exec_params(
        &pool,
        &ins,
        &[Value::Decimal(
            "12345678901234567890123456789012345678".to_string(),
        )],
    )
    .await;
    let sel = format!("SELECT amt::text FROM \"{}\"", table);
    let rows = fetch_scalars(&pool, &sel, &[]).await;
    assert_eq!(
        rows[0], "12345678901234567890123456789012345678",
        "NUMERIC 38 位经 Decimal/BigDecimal 通道应无损"
    );
    drop_table(&pool, &table).await;

    // json_extract 生成面（#>> 路径）真库可用
    let dialect = pg_dialect();
    let expr = dialect.json_extract("meta", "$.a.b");
    let sql = format!(
        "SELECT {} FROM (SELECT '{{\"a\":{{\"b\":\"ok\"}}}}'::jsonb AS meta) t",
        expr
    );
    let rows = fetch_scalars(&pool, &sql, &[]).await;
    assert_eq!(rows[0], "ok", "json_extract #>> 生成面真库应可用：{}", expr);
}

/// PG4-b：JSONB 参数往返（Value::Json 经 bind_pg_value 绑定 serde_json）+ @> 操作符透传
#[tokio::test]
#[ignore = "需要 PostgreSQL 18 运行于 127.0.0.1:5432"]
async fn pg4_jsonb_operators() {
    let pool = setup_pool().await;
    let table = unique_table("t_pg4json");
    exec_sql(
        &pool,
        &format!(
            "CREATE TABLE \"{}\" (id BIGSERIAL PRIMARY KEY, meta JSONB NOT NULL)",
            table
        ),
    )
    .await;

    // Value::Json 参数化插入（sz-orm bind_pg_value 对 Value::Json 走 serde_json 绑定）
    // 本测试直连 sqlx 复刻该通道：serde_json::Value 绑定 jsonb 列
    let json_col: serde_json::Value = serde_json::from_str(r#"{"tags":["a","b"],"k":1}"#).unwrap();
    let ins = format!("INSERT INTO \"{}\" (meta) VALUES ($1)", table);
    sqlx::query(sqlx::AssertSqlSafe(ins.as_str()))
        .bind(&json_col)
        .execute(&pool)
        .await
        .expect("Value::Json → jsonb serde 绑定应可用");
    exec_sql(
        &pool,
        &format!(
            "INSERT INTO \"{}\" (meta) VALUES ('{{\"tags\":[\"c\"],\"k\":2}}')",
            table
        ),
    )
    .await;

    // @> 包含操作符（typed_ast JsonContains 对 PG 生成 "col" @> ?）
    let s1 = format!("SELECT count(*) FROM \"{}\" WHERE meta @> ?::jsonb", table);
    let rows = sqlx::query(sqlx::AssertSqlSafe(
        pg_rewrite_placeholders_for_test(&s1).as_str(),
    ))
    .bind(r#"{"tags":["a"]}"#)
    .fetch_all(&pool)
    .await
    .expect("jsonb contains");
    let cnt: i64 = rows[0].try_get(0).unwrap();
    assert_eq!(cnt, 1, "@> 应命中 1 行");
    drop_table(&pool, &table).await;
}

// =============================================================================
// PG5 索引与约束
// =============================================================================

/// PG5-a：CREATE INDEX IF NOT EXISTS 幂等（方言 supports_if_not_exists 声明 + 真库幂等）
#[tokio::test]
#[ignore = "需要 PostgreSQL 18 运行于 127.0.0.1:5432"]
async fn pg5_index_if_not_exists_idempotent() {
    let pool = setup_pool().await;
    let table = unique_table("t_pg5idx");
    create_dialect_table(&pool, &table).await;
    assert!(
        pg_dialect().supports_if_not_exists(),
        "PG 应声明支持 IF NOT EXISTS"
    );

    let idx = format!("idx_{}_v", table);
    let s1 = format!(
        "CREATE INDEX IF NOT EXISTS \"{}\" ON \"{}\" (\"value\")",
        idx, table
    );
    exec_sql(&pool, &s1).await;
    exec_sql(&pool, &s1).await; // 幂等第二次不报错
    drop_index(&pool, &idx).await;
    drop_table(&pool, &table).await;
}

/// PG5-b：CONCURRENTLY 事务冲突探针（真库限制：CONCURRENTLY 不能在事务内——迁移/schema_sync 场景登记依据）
#[tokio::test]
#[ignore = "需要 PostgreSQL 18 运行于 127.0.0.1:5432"]
async fn pg5_concurrently_outside_transaction_only() {
    let pool = setup_pool().await;
    let table = unique_table("t_pg5conc");
    create_dialect_table(&pool, &table).await;

    // 事务外：CREATE INDEX CONCURRENTLY 成功
    let idx = format!("idx_{}_conc", table);
    let s = format!(
        "CREATE INDEX CONCURRENTLY IF NOT EXISTS \"{}\" ON \"{}\" (\"value\")",
        idx, table
    );
    exec_sql(&pool, &s).await;

    // 事务内：同形态被真库拒绝
    let mut tx = pool.begin().await.expect("begin");
    let s2 = format!(
        "CREATE INDEX CONCURRENTLY \"{}_c2\" ON \"{}\" (\"value\")",
        table, table
    );
    let err = sqlx::query(sqlx::AssertSqlSafe(s2.as_str()))
        .execute(&mut *tx)
        .await;
    assert!(
        err.is_err(),
        "事务内 CREATE INDEX CONCURRENTLY 应被真库拒绝"
    );
    tx.rollback().await.ok();

    drop_index(&pool, &idx).await;
    drop_table(&pool, &table).await;
}

/// PG5-c：部分索引 / 表达式索引 / GIN(JSONB)——sz-orm 未封装（登记），透传 SQL 真库可行
#[tokio::test]
#[ignore = "需要 PostgreSQL 18 运行于 127.0.0.1:5432"]
async fn pg5_partial_expression_gin_index() {
    let pool = setup_pool().await;
    let table = unique_table("t_pg5part");
    exec_sql(
        &pool,
        &format!(
            "CREATE TABLE \"{}\" (id BIGSERIAL PRIMARY KEY, value BIGINT, meta JSONB, name TEXT)",
            table
        ),
    )
    .await;

    let p1 = format!("idx_{}_partial", table);
    exec_sql(
        &pool,
        &format!(
            "CREATE INDEX \"{}\" ON \"{}\" (\"value\") WHERE \"value\" > 0",
            p1, table
        ),
    )
    .await;
    let p2 = format!("idx_{}_expr", table);
    exec_sql(
        &pool,
        &format!("CREATE INDEX \"{}\" ON \"{}\" (lower(\"name\"))", p2, table),
    )
    .await;
    let p3 = format!("idx_{}_gin", table);
    exec_sql(
        &pool,
        &format!(
            "CREATE INDEX \"{}\" ON \"{}\" USING gin (\"meta\")",
            p3, table
        ),
    )
    .await;

    let cnt: (i64,) = sqlx::query_as(sqlx::AssertSqlSafe(
        format!(
            "SELECT count(*) FROM pg_indexes WHERE tablename = '{}' AND indexname IN ('{}','{}','{}')",
            table, p1, p2, p3
        )
        .as_str(),
    ))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(cnt.0, 3, "部分/表达式/GIN 索引应全部建成");

    for i in [&p1, &p2, &p3] {
        drop_index(&pool, i).await;
    }
    drop_table(&pool, &table).await;
}

// =============================================================================
// PG6 查询面
// =============================================================================

/// PG6-a：CTE builder（with_cte / with_recursive_cte）PG 对称验证（MY10 B4）——参数先于主查询
#[tokio::test]
#[ignore = "需要 PostgreSQL 18 运行于 127.0.0.1:5432"]
async fn pg6_cte_builder_nonrecursive_and_recursive() {
    let pool = setup_pool().await;
    let table = unique_table("t_pg6cte");
    create_dialect_table(&pool, &table).await;
    insert_rows(&pool, &table, &[("a", 1), ("b", 2), ("c", 3)]).await;

    // 非递归：WITH kids AS (SELECT ...) 主查询经 EXISTS 透传引用 CTE
    // （原始布尔片段经自定义 TypedExpression 桥接 where_expr——调用方信任口径同 select_expr）
    let kids_sql = format!("SELECT \"id\" FROM \"{}\" WHERE \"value\" > ?", table);
    let semi_join = format!(
        "EXISTS (SELECT 1 FROM \"kids\" k WHERE k.\"id\" = \"{}\".\"id\")",
        table
    );
    let (sql, params) = qb(&table)
        .with_cte("kids", kids_sql, vec![Value::I64(1)])
        .expect("with_cte")
        .select_expr(vec!["count(*)"])
        .where_expr(RawBool(&semi_join))
        .build_select_with_params();
    assert!(sql.starts_with("WITH "), "CTE 前缀应渲染：{}", sql);
    assert_eq!(params, vec![Value::I64(1)], "CTE 参数应先于主查询参数");
    let rows = fetch_scalars(&pool, &sql, &params).await;
    assert_eq!(rows[0], "2", "CTE 过滤 value>1 应命中 2 行");

    // 递归：WITH RECURSIVE seq AS (锚 UNION ALL 递归) 计数 1..=5（主查询 FROM seq）
    let seq_sql = "SELECT 1 AS n UNION ALL SELECT n+1 FROM seq WHERE n < 5";
    let builder2 = QueryBuilder::<DummyModel>::new(pg_dialect());
    let builder2 = builder2.table("seq");
    let (sql2, params2) = builder2
        .with_recursive_cte("seq", seq_sql, vec![])
        .expect("with_recursive_cte")
        .select_expr(vec!["count(*)"])
        .build_select_with_params();
    assert!(
        sql2.starts_with("WITH RECURSIVE \"seq\" AS ("),
        "递归 CTE 前缀：{}",
        sql2
    );
    let rows2 = fetch_scalars(&pool, &sql2, &params2).await;
    assert_eq!(rows2[0], "5", "递归 CTE 1..5 应计 5 行");
    drop_table(&pool, &table).await;
}

/// PG6-b：DISTINCT ON（select_expr 透传）+ 方言分页 LIMIT/OFFSET 行为
#[tokio::test]
#[ignore = "需要 PostgreSQL 18 运行于 127.0.0.1:5432"]
async fn pg6_distinct_on_and_pagination() {
    let pool = setup_pool().await;
    let table = unique_table("t_pg6dist");
    exec_sql(
        &pool,
        &format!(
            "CREATE TABLE \"{}\" (id BIGSERIAL PRIMARY KEY, cat TEXT NOT NULL, v BIGINT NOT NULL)",
            table
        ),
    )
    .await;
    exec_sql(
        &pool,
        &format!(
            "INSERT INTO \"{}\" (cat,v) VALUES ('a',1),('a',2),('b',3),('b',4),('c',5)",
            table
        ),
    )
    .await;

    // DISTINCT ON (cat)：每类取第一行
    let s = format!(
        "SELECT DISTINCT ON (cat) cat, v FROM \"{}\" ORDER BY cat, v",
        table
    );
    let rows = sqlx::query(sqlx::AssertSqlSafe(s.as_str()))
        .fetch_all(&pool)
        .await
        .expect("distinct on");
    assert_eq!(rows.len(), 3, "DISTINCT ON 应按类去重为 3 行");

    // 方言分页生成面：LIMIT/OFFSET 子句 + 真库行为
    let (s2, _p2) = qb(&table)
        .select_expr(vec!["count(*)"])
        .limit(2)
        .offset(1)
        .build_select_with_params();
    assert!(s2.contains(" LIMIT 2 OFFSET 1"), "分页子句：{}", s2);
    let s3 = format!("SELECT v FROM \"{}\" ORDER BY v LIMIT 2 OFFSET 1", table);
    let rows3 = fetch_scalars(&pool, &s3, &[]).await;
    assert_eq!(rows3, vec!["2".to_string(), "3".to_string()]);
    drop_table(&pool, &table).await;
}

/// PG6-c：窗口函数 / NULLS LAST / ILIKE / FETCH FIRST（select_expr 透传通道语义完整）
#[tokio::test]
#[ignore = "需要 PostgreSQL 18 运行于 127.0.0.1:5432"]
async fn pg6_window_nulls_ilike_fetch() {
    let pool = setup_pool().await;
    let table = unique_table("t_pg6win");
    exec_sql(
        &pool,
        &format!(
            "CREATE TABLE \"{}\" (id BIGSERIAL PRIMARY KEY, cat TEXT, v BIGINT)",
            table
        ),
    )
    .await;
    exec_sql(
        &pool,
        &format!(
            "INSERT INTO \"{}\" (cat,v) VALUES ('a',1),('a',2),('b',NULL),('b',4)",
            table
        ),
    )
    .await;

    // 窗口函数：ROW_NUMBER() OVER (PARTITION BY ... ORDER BY ... NULLS LAST)
    let s1 = format!(
        "SELECT rn FROM (SELECT cat, v, ROW_NUMBER() OVER (PARTITION BY cat ORDER BY v DESC NULLS LAST) rn FROM \"{}\") t WHERE v IS NOT NULL ORDER BY v LIMIT 1",
        table
    );
    let rows = fetch_scalars(&pool, &s1, &[]).await;
    // 数据 (a,1)(a,2)(b,4)：a 组内 v DESC 首行 rn=1 的 v=2，b 组非空 v=4 → 最小非空 v=2
    assert_eq!(rows[0], "2", "窗口函数 + NULLS LAST 应可用");

    // ILIKE 透传（typed_ast ILike 对 PG 生成 ILIKE——本处验证透传通道）
    let s2 = format!("SELECT count(*) FROM \"{}\" WHERE cat ILIKE 'A%'", table);
    let rows2 = fetch_scalars(&pool, &s2, &[]).await;
    assert_eq!(rows2[0], "2", "ILIKE 大小写不敏感应命中 2 行");

    // FETCH FIRST n ROWS ONLY（PG 支持标准语法）
    let s3 = format!(
        "SELECT v FROM \"{}\" WHERE v IS NOT NULL ORDER BY v FETCH FIRST 1 ROWS ONLY",
        table
    );
    let rows3 = fetch_scalars(&pool, &s3, &[]).await;
    assert_eq!(rows3[0], "1", "FETCH FIRST 应可用");
    drop_table(&pool, &table).await;
}

// =============================================================================
// PG7 边缘与并发
// =============================================================================

/// PG7-a：三值 NULL（where_eq(Null) → IS NULL 真行为；where_ne(Null) → IS NOT NULL；空串 ≠ NULL）
#[tokio::test]
#[ignore = "需要 PostgreSQL 18 运行于 127.0.0.1:5432"]
async fn pg7_three_valued_null_and_empty_string() {
    let pool = setup_pool().await;
    let table = unique_table("t_pg7null");
    create_dialect_table(&pool, &table).await;
    insert_rows(&pool, &table, &[("n", 1), ("e", 0)]).await;
    // value=NULL 两行（insert_rows 走 name/value；NULL 行用原生 SQL）
    exec_sql(
        &pool,
        &format!(
            "INSERT INTO \"{}\" (\"name\",\"value\") VALUES ('n2',NULL),('z',NULL)",
            table
        ),
    )
    .await;

    let builder = qb(&table);
    // G1 同型：where_eq(col, Null) 生成 IS NULL 并真库命中 2 行
    let (sql, params) = builder
        .clone_for_count()
        .select_expr(vec!["count(*)"])
        .where_eq("value", Value::Null)
        .build_select_with_params();
    assert!(
        sql.contains("IS NULL"),
        "where_eq(Null) 应生成 IS NULL：{}",
        sql
    );
    let rows = fetch_scalars(&pool, &sql, &params).await;
    assert_eq!(rows[0], "2", "IS NULL 应命中 2 行");

    // where_ne(Null) → IS NOT NULL
    let (sql2, params2) = builder
        .clone_for_count()
        .select_expr(vec!["count(*)"])
        .where_ne("value", Value::Null)
        .build_select_with_params();
    assert!(
        sql2.contains("IS NOT NULL"),
        "where_ne(Null) 应生成 IS NOT NULL：{}",
        sql2
    );
    let rows2 = fetch_scalars(&pool, &sql2, &params2).await;
    assert_eq!(rows2[0], "2", "IS NOT NULL 应命中 2 行（n/e 两行）");

    // 空串 ≠ NULL：空串可精确匹配
    exec_sql(
        &pool,
        &format!(
            "INSERT INTO \"{}\" (\"name\",\"value\") VALUES ('',5)",
            table
        ),
    )
    .await;
    let (sql3, params3) = builder
        .select_expr(vec!["count(*)"])
        .where_eq("name", Value::String(String::new()))
        .build_select_with_params();
    let rows3 = fetch_scalars(&pool, &sql3, &params3).await;
    assert_eq!(rows3[0], "1", "空串应与 NULL 区分且可精确匹配");
    drop_table(&pool, &table).await;
}

/// PG7-b：除零真库报错（22012）+ 事务内错误 aborted 语义（25P02 后须 ROLLBACK）
#[tokio::test]
#[ignore = "需要 PostgreSQL 18 运行于 127.0.0.1:5432"]
async fn pg7_divide_by_zero_and_aborted_transaction() {
    let pool = setup_pool().await;
    let err = sqlx::query(sqlx::AssertSqlSafe("SELECT 1/0"))
        .fetch_all(&pool)
        .await;
    assert!(err.is_err(), "除零应报错（22012 division_by_zero）");

    // aborted 语义：事务内错误后不回滚直接再发语句 → 25P02
    let table = unique_table("t_pg7abort");
    create_dialect_table(&pool, &table).await;
    let mut tx = pool.begin().await.expect("begin");
    let bad = "SELECT 1/0";
    let _ = sqlx::query(sqlx::AssertSqlSafe(bad))
        .fetch_all(&mut *tx)
        .await;
    let after = format!("SELECT count(*) FROM \"{}\"", table);
    let err2 = sqlx::query(sqlx::AssertSqlSafe(after.as_str()))
        .fetch_all(&mut *tx)
        .await;
    assert!(err2.is_err(), "aborted 事务内后续语句应被拒绝（25P02）");
    tx.rollback().await.ok();
    drop_table(&pool, &table).await;
}

/// PG7-c：FOR UPDATE 生成面真库可用 + SKIP LOCKED 行为级（sz-pay 队列场景前置；
/// sz-orm 未封装 SKIP LOCKED——登记）
#[tokio::test]
#[ignore = "需要 PostgreSQL 18 运行于 127.0.0.1:5432"]
async fn pg7_for_update_skip_locked() {
    let pool = setup_pool().await;
    let table = unique_table("t_pg7lock");
    create_dialect_table(&pool, &table).await;
    insert_rows(&pool, &table, &[("a", 1), ("b", 2)]).await;

    // 方言 FOR UPDATE 生成面真库可用
    let (sql, params) = qb(&table)
        .select_expr(vec!["\"name\""])
        .where_eq("name", Value::String("a".to_string()))
        .lock_for_update()
        .expect("pg supports FOR UPDATE")
        .build_select_with_params();
    assert!(sql.ends_with("FOR UPDATE"), "锁子句应尾接：{}", sql);
    let mut tx = pool.begin().await.expect("begin");
    let mut q = sqlx::query(sqlx::AssertSqlSafe(
        pg_rewrite_placeholders_for_test(&sql).as_str(),
    ));
    for v in &params {
        q = bind_value_pg(q, v);
    }
    let rows = q.fetch_all(&mut *tx).await.expect("for update select");
    assert_eq!(rows.len(), 1);
    tx.commit().await.expect("commit");

    // SKIP LOCKED：真库行为（透传 SQL 验证行为可行性）
    let mut tx1 = pool.begin().await.expect("begin tx1");
    let mut tx2 = pool.begin().await.expect("begin tx2");
    let s1 = format!(
        "SELECT \"name\" FROM \"{}\" WHERE \"value\"=1 FOR UPDATE",
        table
    );
    let s2 = format!(
        "SELECT \"name\" FROM \"{}\" WHERE \"value\"=1 FOR UPDATE SKIP LOCKED",
        table
    );
    let _ = sqlx::query(sqlx::AssertSqlSafe(s1.as_str()))
        .fetch_all(&mut *tx1)
        .await
        .unwrap();
    let skipped = sqlx::query(sqlx::AssertSqlSafe(s2.as_str()))
        .fetch_all(&mut *tx2)
        .await
        .expect("skip locked");
    assert_eq!(skipped.len(), 0, "SKIP LOCKED 应跳过已锁行");
    tx1.rollback().await.ok();
    tx2.rollback().await.ok();
    drop_table(&pool, &table).await;
}

/// PG7-d：LISTEN/NOTIFY 行为级（szrsql 曾缺——PG 真库必须过；
/// 经 sz-orm-sqlx PgExtensions::listen/notify 通道 + sqlx PgListener 接收）
#[tokio::test]
#[ignore = "需要 PostgreSQL 18 运行于 127.0.0.1:5432"]
async fn pg7_listen_notify() {
    let pool = setup_pool().await;
    let mut listener = sqlx::postgres::PgListener::connect_with(&pool)
        .await
        .expect("pg listener connect");
    let chan = format!("sz_orm_pg_matrix_{}", unique_table("n"));
    listener.listen(chan.as_str()).await.expect("LISTEN");
    let payload = "hello-pg-notify";
    exec_sql(&pool, &format!("NOTIFY \"{}\", '{}'", chan, payload)).await;
    let notif = listener.recv().await.expect("recv notification");
    assert_eq!(notif.channel(), chan, "通知通道应匹配");
    assert_eq!(notif.payload(), payload, "通知载荷应匹配");
}

/// PG7-e：全文检索 to_tsvector/to_tsquery 生成面（FTS quote 批形态）真库行为级
#[tokio::test]
#[ignore = "需要 PostgreSQL 18 运行于 127.0.0.1:5432"]
async fn pg7_full_text_search_generation() {
    let pool = setup_pool().await;
    let table = unique_table("t_pg7fts");
    exec_sql(
        &pool,
        &format!(
            "CREATE TABLE \"{}\" (id BIGSERIAL PRIMARY KEY, \"titleText\" TEXT NOT NULL)",
            table
        ),
    )
    .await;
    exec_sql(
        &pool,
        &format!(
            "INSERT INTO \"{}\" (\"titleText\") VALUES ('quick brown fox'),('lazy dog')",
            table
        ),
    )
    .await;

    let dialect = pg_dialect();
    // 混合大小写列名 quote 往返 + 真库命中
    let expr = dialect.full_text_search(&["titleText"], "fox");
    assert!(
        expr.contains("\"titleText\"::text"),
        "FTS 列应 quote：{}",
        expr
    );
    let s = format!("SELECT count(*) FROM \"{}\" WHERE {}", table, expr);
    let rows = fetch_scalars(&pool, &s, &[]).await;
    assert_eq!(rows[0], "1", "to_tsvector @@ to_tsquery('fox') 应命中 1 行");

    // to_tsquery 整串语法面：操作符透传（'quick & fox'）
    let expr2 = dialect.full_text_search(&["titleText"], "quick & fox");
    let s2 = format!("SELECT count(*) FROM \"{}\" WHERE {}", table, expr2);
    let rows2 = fetch_scalars(&pool, &s2, &[]).await;
    assert_eq!(rows2[0], "1", "to_tsquery 操作符 '&' 应生效");
    drop_table(&pool, &table).await;
}

/// PG7-f：FTS 三模式 full_text_search_mode PG 默认委托（三模式同形——模式语义仅 MySQL）
#[tokio::test]
#[ignore = "需要 PostgreSQL 18 运行于 127.0.0.1:5432"]
async fn pg7_fts_mode_default_delegation() {
    let dialect = pg_dialect();
    let plain = dialect.full_text_search(&["titleText"], "fox");
    let nl = dialect.full_text_search_mode(&["titleText"], "fox", FullTextMode::NaturalLanguage);
    assert_eq!(
        plain, nl,
        "PG full_text_search_mode 默认应委托 full_text_search"
    );
    let boolean = dialect.full_text_search_mode(&["titleText"], "fox", FullTextMode::Boolean);
    assert_eq!(boolean, plain, "PG 默认三模式同形（模式仅 MySQL 语义）");
}
