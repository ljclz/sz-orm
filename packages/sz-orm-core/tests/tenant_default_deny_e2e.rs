//! R9 真库用例：租户作用域默认拒绝（fail-closed）
//!
//! QueryBuilder 生成的租户条件必须经真实 SQLite 执行验证：
//! - 模型声明了租户字段但无租户上下文 → 追加恒假条件，返回空集
//! - 显式 `with_tenant_id` → 只返回该租户的行
//! - 显式 `without_tenant()` → 跨租户出口
//!
//! `multi-tenant-enhanced` 下无上下文改为直接报错（同样 fail-closed），
//! 单独用例覆盖。

#[cfg(not(feature = "multi-tenant-enhanced"))]
use std::str::FromStr;

#[cfg(not(feature = "multi-tenant-enhanced"))]
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
#[cfg(not(feature = "multi-tenant-enhanced"))]
use sqlx::Row;
#[cfg(not(feature = "multi-tenant-enhanced"))]
use sz_orm_core::Value;
use sz_orm_core::{get_dialect, DbType, Model, QueryBuilder};

struct R9Model;

impl Model for R9Model {
    type PrimaryKey = i64;

    fn table_name() -> &'static str {
        "r9_orders"
    }

    fn pk(&self) -> Self::PrimaryKey {
        0
    }

    fn set_pk(&mut self, _pk: Self::PrimaryKey) {}

    fn tenant_field() -> Option<&'static str> {
        Some("tenant_id")
    }
}

/// 单连接内存库：SQLite `:memory:` 每个连接是独立数据库，
/// 必须固定 max_connections=1 才能让建表与查询看到同一份数据。
#[cfg(not(feature = "multi-tenant-enhanced"))]
async fn sqlite_pool() -> sqlx::SqlitePool {
    let opts = SqliteConnectOptions::from_str("sqlite::memory:")
        .expect("sqlite in-memory options")
        .create_if_missing(true);
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await
        .expect("SQLite 内存库连接失败（R9 真库用例不跳过）")
}

#[cfg(not(feature = "multi-tenant-enhanced"))]
async fn seed(pool: &sqlx::SqlitePool) {
    sqlx::query(
        "CREATE TABLE r9_orders (id INTEGER PRIMARY KEY, tenant_id INTEGER NOT NULL, name TEXT NOT NULL)",
    )
    .execute(pool)
    .await
    .expect("create table");

    for (id, tenant_id, name) in [(1_i64, 1_i64, "a1"), (2, 2, "b1"), (3, 1, "a2")] {
        sqlx::query("INSERT INTO r9_orders (id, tenant_id, name) VALUES (?, ?, ?)")
            .bind(id)
            .bind(tenant_id)
            .bind(name)
            .execute(pool)
            .await
            .expect("insert row");
    }
}

/// 按 QueryBuilder 的参数顺序绑定后执行
#[cfg(not(feature = "multi-tenant-enhanced"))]
async fn run(pool: &sqlx::SqlitePool, sql: &str, params: &[Value]) -> Vec<sqlx::sqlite::SqliteRow> {
    let mut q = sqlx::query(sqlx::AssertSqlSafe(sql));
    for p in params {
        q = match p {
            Value::I64(v) => q.bind(*v),
            Value::I32(v) => q.bind(*v),
            Value::U32(v) => q.bind(i64::from(*v)),
            Value::U64(v) => q.bind(*v as i64),
            Value::String(s) => q.bind(s.clone()),
            Value::Null => q.bind(Option::<i64>::None),
            other => panic!("R9 真库用例未覆盖的参数类型: {other:?}"),
        };
    }
    q.fetch_all(pool).await.expect("query executed")
}

#[cfg(not(feature = "multi-tenant-enhanced"))]
fn names(rows: &[sqlx::sqlite::SqliteRow]) -> Vec<String> {
    let mut out: Vec<String> = rows.iter().map(|r| r.get::<String, _>("name")).collect();
    out.sort();
    out
}

#[cfg(not(feature = "multi-tenant-enhanced"))]
#[tokio::test]
async fn r9_no_tenant_context_returns_empty_set_on_real_sqlite() {
    let pool = sqlite_pool().await;
    seed(&pool).await;

    // 无租户上下文：默认拒绝 → 真实执行必须 0 行
    let (sql, params) = QueryBuilder::<R9Model>::new(get_dialect(DbType::Sqlite).unwrap())
        .table("r9_orders")
        .build_select_with_params();
    assert!(sql.contains("1 = 0"), "缺少默认拒绝条件: {sql}");
    let rows = run(&pool, &sql, &params).await;
    assert!(
        rows.is_empty(),
        "无租户上下文必须返回空集，实际 {} 行",
        rows.len()
    );

    // 显式租户 1：只见自己的两行
    let (sql, params) = QueryBuilder::<R9Model>::new(get_dialect(DbType::Sqlite).unwrap())
        .table("r9_orders")
        .with_tenant_id(1)
        .build_select_with_params();
    let rows = run(&pool, &sql, &params).await;
    assert_eq!(
        names(&rows),
        vec!["a1".to_string(), "a2".to_string()],
        "租户 1 只能看到 tenant_id = 1 的行"
    );

    // 显式跨租户出口：without_tenant() 才放行全表
    let (sql, params) = QueryBuilder::<R9Model>::new(get_dialect(DbType::Sqlite).unwrap())
        .table("r9_orders")
        .without_tenant()
        .build_select_with_params();
    assert!(
        !sql.contains("1 = 0"),
        "without_tenant() 不应追加拒绝条件: {sql}"
    );
    let rows = run(&pool, &sql, &params).await;
    assert_eq!(rows.len(), 3, "without_tenant() 是显式跨租户出口");

    pool.close().await;
}

#[cfg(feature = "multi-tenant-enhanced")]
#[tokio::test]
#[should_panic(expected = "TenantContextRequired")]
async fn r9_no_tenant_context_fails_closed_under_enhanced() {
    // 增强模式下无租户上下文直接 panic（fail-closed），不生成可执行 SQL
    let _ = QueryBuilder::<R9Model>::new(get_dialect(DbType::Sqlite).unwrap())
        .table("r9_orders")
        .build_select_with_params();
}
