//! ORM 对比基准测试：SZ-ORM vs Diesel vs SeaORM vs SQLx vs rusqlite
//!
//! 测试环境：SQLite in-memory（公平对比，无网络开销）
//! 测试场景：CRUD（INSERT / SELECT / UPDATE / DELETE）
//! 运行：cargo bench --bench orm_comparison
//! 报告：target/criterion/index.html
//!
//! v0.4 修复：
//! 1. select_by_id/select_all/update：异步 ORM 改为 setup 在外（rt.block_on），
//!    b.to_async().iter() 每次迭代只做单次操作，与同步 ORM 结构一致
//! 2. 所有异步 ORM 使用 cache=shared + max_connections(10)，确保多连接共享
//!    同一 in-memory 数据库（SQLite :memory: 默认每连接独立）
//! 3. SZ-ORM SzOrmCtx 实现 Clone，支持在闭包中 clone 传递
//! 4. 数据量：BENCH_SIZES=[1000,10000,100000]，PREPARE_SIZE=100000

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

// ============================================================================
// 表结构：所有 ORM 使用相同的表结构
// ============================================================================

const CREATE_TABLE_SQL: &str = "CREATE TABLE IF NOT EXISTS bench_users (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    email TEXT NOT NULL,
    age INTEGER NOT NULL
)";

const DROP_TABLE_SQL: &str = "DROP TABLE IF EXISTS bench_users";

// ============================================================================
// rusqlite baseline（同步，最底层）
// ============================================================================

mod baseline_rusqlite {
    use super::*;
    use rusqlite::{params, Connection};

    pub fn setup() -> Connection {
        let conn = Connection::open_in_memory().expect("open sqlite");
        conn.execute(CREATE_TABLE_SQL, []).expect("create table");
        conn
    }

    pub fn teardown(conn: Connection) {
        conn.execute(DROP_TABLE_SQL, []).ok();
    }

    pub fn insert_one(conn: &Connection, name: &str, email: &str, age: i64) {
        conn.execute(
            "INSERT INTO bench_users (name, email, age) VALUES (?, ?, ?)",
            params![name, email, age],
        )
        .expect("insert");
    }

    pub fn select_by_id(conn: &Connection, id: i64) -> (String, String, i64) {
        let mut stmt = conn
            .prepare("SELECT name, email, age FROM bench_users WHERE id = ?")
            .expect("prepare");
        let mut rows = stmt.query(params![id]).expect("query");
        let row = rows.next().expect("next").expect("row");
        (
            row.get::<_, String>(0).expect("name"),
            row.get::<_, String>(1).expect("email"),
            row.get::<_, i64>(2).expect("age"),
        )
    }

    pub fn select_all(conn: &Connection) -> Vec<(i64, String, String, i64)> {
        let mut stmt = conn
            .prepare("SELECT id, name, email, age FROM bench_users")
            .expect("prepare");
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })
            .expect("query");
        rows.filter_map(|r| r.ok()).collect()
    }

    pub fn update_by_id(conn: &Connection, id: i64, name: &str) {
        conn.execute(
            "UPDATE bench_users SET name = ? WHERE id = ?",
            params![name, id],
        )
        .expect("update");
    }

    pub fn delete_by_id(conn: &Connection, id: i64) {
        conn.execute("DELETE FROM bench_users WHERE id = ?", params![id])
            .expect("delete");
    }
}

// ============================================================================
// SQLx（异步，直接 SQL）
// ============================================================================

mod async_sqlx {
    use super::*;
    use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
    use sqlx::Row;

    pub async fn setup() -> SqlitePool {
        // cache=shared 确保多连接共享同一 in-memory 数据库
        let pool = SqlitePoolOptions::new()
            .max_connections(10)
            .connect("sqlite::memory:?cache=shared")
            .await
            .expect("connect sqlite");
        sqlx::query(CREATE_TABLE_SQL)
            .execute(&pool)
            .await
            .expect("create table");
        pool
    }

    pub async fn teardown(pool: &SqlitePool) {
        sqlx::query(DROP_TABLE_SQL)
            .execute(pool)
            .await
            .expect("drop table");
        pool.close().await;
    }

    pub async fn insert_one(pool: &SqlitePool, name: &str, email: &str, age: i64) -> i64 {
        let result = sqlx::query(
            "INSERT INTO bench_users (name, email, age) VALUES (?, ?, ?)",
        )
        .bind(name)
        .bind(email)
        .bind(age)
        .execute(pool)
        .await
        .expect("insert");
        result.last_insert_rowid()
    }

    pub async fn select_by_id(pool: &SqlitePool, id: i64) -> (String, String, i64) {
        let row = sqlx::query("SELECT name, email, age FROM bench_users WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await
            .expect("query");
        (
            row.get::<String, _>(0),
            row.get::<String, _>(1),
            row.get::<i64, _>(2),
        )
    }

    pub async fn select_all(pool: &SqlitePool) -> Vec<(i64, String, String, i64)> {
        let rows = sqlx::query("SELECT id, name, email, age FROM bench_users")
            .fetch_all(pool)
            .await
            .expect("query");
        rows.iter()
            .map(|r| {
                (
                    r.get::<i64, _>(0),
                    r.get::<String, _>(1),
                    r.get::<String, _>(2),
                    r.get::<i64, _>(3),
                )
            })
            .collect()
    }

    pub async fn update_by_id(pool: &SqlitePool, id: i64, name: &str) {
        sqlx::query("UPDATE bench_users SET name = ? WHERE id = ?")
            .bind(name)
            .bind(id)
            .execute(pool)
            .await
            .expect("update");
    }

    pub async fn delete_by_id(pool: &SqlitePool, id: i64) {
        sqlx::query("DELETE FROM bench_users WHERE id = ?")
            .bind(id)
            .execute(pool)
            .await
            .expect("delete");
    }
}

// ============================================================================
// SZ-ORM（QueryBuilder + sz-orm-sqlx Connection）
// ============================================================================

mod sz_orm {
    use super::*;
    use sz_orm_core::{Pool, PoolConfigBuilder, Value};
    use sz_orm_sqlx::{SqlitePoolHandle, SqlxSqliteConnectionFactory};
    use std::sync::Arc;

    #[derive(Clone)]
    pub struct SzOrmCtx {
        pub pool: Pool,
        _handle: Arc<SqlitePoolHandle>,
    }

    pub async fn setup() -> SzOrmCtx {
        // cache=shared 确保多连接共享同一 in-memory 数据库
        let handle = Arc::new(SqlitePoolHandle::connect("sqlite::memory:?cache=shared").await.expect("connect"));
        let factory = Arc::new(SqlxSqliteConnectionFactory::new(handle.clone()));
        let config = PoolConfigBuilder::new().max_size(10).build().expect("config");
        let pool = Pool::new(config, factory).expect("pool");
        let mut conn = pool.acquire().await.expect("acquire");
        conn.execute(CREATE_TABLE_SQL).await.expect("create table");
        // conn drop 时自动归还（无需显式 release）
        SzOrmCtx { pool, _handle: handle }
    }

    pub async fn teardown(ctx: SzOrmCtx) {
        let mut conn = ctx.pool.acquire().await.expect("acquire");
        conn.execute(DROP_TABLE_SQL).await.ok();
        // conn drop 时自动归还
        ctx.pool.close_all().await;
    }

    /// SZ-ORM Connection trait 当前仅支持 &str SQL（不支持参数绑定），
    /// 使用 format!() + 手动转义构造 SQL。这是 SZ-ORM API 的已知限制。
    pub async fn insert_one(ctx: &SzOrmCtx, name: &str, email: &str, age: i64) {
        let sql = format!(
            "INSERT INTO bench_users (name, email, age) VALUES ('{}', '{}', {})",
            name.replace('\'', "''"),
            email.replace('\'', "''"),
            age
        );
        let mut conn = ctx.pool.acquire().await.expect("acquire");
        conn.execute(&sql).await.expect("insert");
        // conn drop 时自动归还
    }

    pub async fn select_by_id(ctx: &SzOrmCtx, id: i64) -> (String, String, i64) {
        let sql = format!("SELECT name, email, age FROM bench_users WHERE id = {}", id);
        let mut conn = ctx.pool.acquire().await.expect("acquire");
        let rows = conn.query(&sql).await.expect("query");
        // conn drop 时自动归还
        if let Some(row) = rows.first() {
            let name = match row.get("name") {
                Some(Value::String(s)) => s.clone(),
                _ => String::new(),
            };
            let email = match row.get("email") {
                Some(Value::String(s)) => s.clone(),
                _ => String::new(),
            };
            let age = match row.get("age") {
                Some(Value::I64(n)) => *n,
                Some(v) => v.as_i64().unwrap_or(0),
                None => 0,
            };
            (name, email, age)
        } else {
            (String::new(), String::new(), 0)
        }
    }

    pub async fn select_all(ctx: &SzOrmCtx) -> Vec<(i64, String, String, i64)> {
        let mut conn = ctx.pool.acquire().await.expect("acquire");
        let rows = conn
            .query("SELECT id, name, email, age FROM bench_users")
            .await
            .expect("query");
        // conn drop 时自动归还
        rows.iter()
            .map(|row| {
                let id = match row.get("id") {
                    Some(Value::I64(n)) => *n,
                    Some(v) => v.as_i64().unwrap_or(0),
                    None => 0,
                };
                let name = match row.get("name") {
                    Some(Value::String(s)) => s.clone(),
                    _ => String::new(),
                };
                let email = match row.get("email") {
                    Some(Value::String(s)) => s.clone(),
                    _ => String::new(),
                };
                let age = match row.get("age") {
                    Some(Value::I64(n)) => *n,
                    Some(v) => v.as_i64().unwrap_or(0),
                    None => 0,
                };
                (id, name, email, age)
            })
            .collect()
    }

    pub async fn update_by_id(ctx: &SzOrmCtx, id: i64, name: &str) {
        let sql = format!(
            "UPDATE bench_users SET name = '{}' WHERE id = {}",
            name.replace('\'', "''"),
            id
        );
        let mut conn = ctx.pool.acquire().await.expect("acquire");
        conn.execute(&sql).await.expect("update");
        // conn drop 时自动归还
    }

    pub async fn delete_by_id(ctx: &SzOrmCtx, id: i64) {
        let sql = format!("DELETE FROM bench_users WHERE id = {}", id);
        let mut conn = ctx.pool.acquire().await.expect("acquire");
        conn.execute(&sql).await.expect("delete");
        // conn drop 时自动归还
    }
}

// ============================================================================
// Diesel（同步，ORM）
// ============================================================================

mod diesel_orm {
    use super::*;
    use diesel::prelude::*;
    use diesel::sqlite::SqliteConnection;

    pub fn setup() -> SqliteConnection {
        let mut conn = SqliteConnection::establish(":memory:").expect("connect");
        diesel::sql_query(CREATE_TABLE_SQL)
            .execute(&mut conn)
            .expect("create table");
        conn
    }

    pub fn teardown(conn: &mut SqliteConnection) {
        diesel::sql_query(DROP_TABLE_SQL)
            .execute(conn)
            .ok();
    }

    pub fn insert_one(conn: &mut SqliteConnection, name: &str, email: &str, age: i64) {
        diesel::sql_query(format!(
            "INSERT INTO bench_users (name, email, age) VALUES ('{}', '{}', {})",
            name.replace('\'', "''"),
            email.replace('\'', "''"),
            age
        ))
        .execute(conn)
        .expect("insert");
    }

    pub fn select_by_id(conn: &mut SqliteConnection, id: i64) -> (String, String, i64) {
        use diesel::sql_types::*;
        #[derive(QueryableByName)]
        struct UserRow {
            #[diesel(sql_type = Text)]
            name: String,
            #[diesel(sql_type = Text)]
            email: String,
            #[diesel(sql_type = Integer)]
            age: i32,
        }
        let row: UserRow = diesel::sql_query(format!(
            "SELECT name, email, age FROM bench_users WHERE id = {}",
            id
        ))
        .get_result(conn)
        .expect("query");
        (row.name, row.email, row.age as i64)
    }

    pub fn select_all(conn: &mut SqliteConnection) -> Vec<(i64, String, String, i64)> {
        use diesel::sql_types::*;
        #[derive(QueryableByName)]
        struct UserRowAll {
            #[diesel(sql_type = BigInt)]
            id: i64,
            #[diesel(sql_type = Text)]
            name: String,
            #[diesel(sql_type = Text)]
            email: String,
            #[diesel(sql_type = Integer)]
            age: i32,
        }
        let rows: Vec<UserRowAll> = diesel::sql_query(
            "SELECT id, name, email, age FROM bench_users",
        )
        .get_results(conn)
        .expect("query");
        rows.into_iter()
            .map(|r| (r.id, r.name, r.email, r.age as i64))
            .collect()
    }

    pub fn update_by_id(conn: &mut SqliteConnection, id: i64, name: &str) {
        diesel::sql_query(format!(
            "UPDATE bench_users SET name = '{}' WHERE id = {}",
            name.replace('\'', "''"),
            id
        ))
        .execute(conn)
        .expect("update");
    }

    pub fn delete_by_id(conn: &mut SqliteConnection, id: i64) {
        diesel::sql_query(format!("DELETE FROM bench_users WHERE id = {}", id))
            .execute(conn)
            .expect("delete");
    }
}

// ============================================================================
// SeaORM（异步，ORM）
// ============================================================================

mod sea_orm_async {
    use super::*;
    use sea_orm::{ConnectOptions, ConnectionTrait, Database, DatabaseBackend, DatabaseConnection, Statement};

    pub async fn setup() -> DatabaseConnection {
        // cache=shared 确保多连接共享同一 in-memory 数据库
        let mut opt = ConnectOptions::new("sqlite::memory:?cache=shared");
        opt.max_connections(10);
        let db = Database::connect(opt).await.expect("connect");
        db.execute(Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            CREATE_TABLE_SQL,
            vec![],
        ))
        .await
        .expect("create table");
        db
    }

    pub async fn teardown(db: &DatabaseConnection) {
        db.execute(Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            DROP_TABLE_SQL,
            vec![],
        ))
        .await
        .ok();
        // SQLite in-memory 连接在 drop 时自动清理，无需显式 close
    }

    pub async fn insert_one(db: &DatabaseConnection, name: &str, email: &str, age: i64) {
        db.execute(Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            "INSERT INTO bench_users (name, email, age) VALUES (?, ?, ?)",
            vec![name.into(), email.into(), age.into()],
        ))
        .await
        .expect("insert");
    }

    pub async fn select_by_id(db: &DatabaseConnection, id: i64) -> (String, String, i64) {
        let result = db
            .query_one(Statement::from_sql_and_values(
                DatabaseBackend::Sqlite,
                "SELECT name, email, age FROM bench_users WHERE id = ?",
                vec![id.into()],
            ))
            .await
            .expect("query")
            .expect("row");
        let name: String = result.try_get("", "name").expect("name");
        let email: String = result.try_get("", "email").expect("email");
        let age: i64 = result.try_get_by_index(2).expect("age");
        (name, email, age)
    }

    pub async fn select_all(db: &DatabaseConnection) -> Vec<(i64, String, String, i64)> {
        let results = db
            .query_all(Statement::from_sql_and_values(
                DatabaseBackend::Sqlite,
                "SELECT id, name, email, age FROM bench_users",
                vec![],
            ))
            .await
            .expect("query");
        results
            .iter()
            .map(|row| {
                let id: i64 = row.try_get_by_index(0).expect("id");
                let name: String = row.try_get_by_index(1).expect("name");
                let email: String = row.try_get_by_index(2).expect("email");
                let age: i64 = row.try_get_by_index(3).expect("age");
                (id, name, email, age)
            })
            .collect()
    }

    pub async fn update_by_id(db: &DatabaseConnection, id: i64, name: &str) {
        db.execute(Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            "UPDATE bench_users SET name = ? WHERE id = ?",
            vec![name.into(), id.into()],
        ))
        .await
        .expect("update");
    }

    pub async fn delete_by_id(db: &DatabaseConnection, id: i64) {
        db.execute(Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            "DELETE FROM bench_users WHERE id = ?",
            vec![id.into()],
        ))
        .await
        .expect("delete");
    }
}

// ============================================================================
// Benchmark 定义
// ============================================================================

const BENCH_SIZES: &[usize] = &[1000, 10000, 100000];

fn bench_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("insert");
    group.throughput(Throughput::Elements(1));

    // rusqlite baseline
    for &size in BENCH_SIZES {
        group.bench_with_input(BenchmarkId::new("rusqlite", size), &size, |b, &n| {
            b.iter(|| {
                let conn = baseline_rusqlite::setup();
                for i in 0..n {
                    baseline_rusqlite::insert_one(
                        &conn,
                        &format!("user_{}", i),
                        &format!("user_{}@test.com", i),
                        (i % 100) as i64,
                    );
                }
                baseline_rusqlite::teardown(conn);
                black_box(n);
            });
        });
    }

    // Diesel (sync)
    for &size in BENCH_SIZES {
        group.bench_with_input(BenchmarkId::new("diesel", size), &size, |b, &n| {
            b.iter(|| {
                let mut conn = diesel_orm::setup();
                for i in 0..n {
                    diesel_orm::insert_one(
                        &mut conn,
                        &format!("user_{}", i),
                        &format!("user_{}@test.com", i),
                        (i % 100) as i64,
                    );
                }
                diesel_orm::teardown(&mut conn);
                black_box(n);
            });
        });
    }

    // SQLx (async)
    let rt = tokio::runtime::Runtime::new().expect("rt");
    for &size in BENCH_SIZES {
        group.bench_with_input(BenchmarkId::new("sqlx", size), &size, |b, &n| {
            b.to_async(&rt).iter(|| async move {
                let pool = async_sqlx::setup().await;
                for i in 0..n {
                    async_sqlx::insert_one(
                        &pool,
                        &format!("user_{}", i),
                        &format!("user_{}@test.com", i),
                        (i % 100) as i64,
                    )
                    .await;
                }
                async_sqlx::teardown(&pool).await;
                black_box(n);
            });
        });
    }

    // SeaORM (async)
    for &size in BENCH_SIZES {
        group.bench_with_input(BenchmarkId::new("sea-orm", size), &size, |b, &n| {
            b.to_async(&rt).iter(|| async move {
                let db = sea_orm_async::setup().await;
                for i in 0..n {
                    sea_orm_async::insert_one(
                        &db,
                        &format!("user_{}", i),
                        &format!("user_{}@test.com", i),
                        (i % 100) as i64,
                    )
                    .await;
                }
                sea_orm_async::teardown(&db).await;
                black_box(n);
            });
        });
    }

    // SZ-ORM (async)
    for &size in BENCH_SIZES {
        group.bench_with_input(BenchmarkId::new("sz-orm", size), &size, |b, &n| {
            b.to_async(&rt).iter(|| async move {
                let ctx = sz_orm::setup().await;
                for i in 0..n {
                    sz_orm::insert_one(
                        &ctx,
                        &format!("user_{}", i),
                        &format!("user_{}@test.com", i),
                        (i % 100) as i64,
                    )
                    .await;
                }
                sz_orm::teardown(ctx).await;
                black_box(n);
            });
        });
    }

    group.finish();
}

fn bench_select_by_id(c: &mut Criterion) {
    let mut group = c.benchmark_group("select_by_id");
    group.throughput(Throughput::Elements(1));

    const PREPARE_SIZE: usize = 100000;

    // rusqlite baseline
    group.bench_function("rusqlite", |b| {
        let conn = baseline_rusqlite::setup();
        for i in 0..PREPARE_SIZE {
            baseline_rusqlite::insert_one(
                &conn,
                &format!("user_{}", i),
                &format!("user_{}@test.com", i),
                i as i64,
            );
        }
        let mut idx = 0i64;
        b.iter(|| {
            let id = (idx % PREPARE_SIZE as i64) + 1;
            idx += 1;
            let r = baseline_rusqlite::select_by_id(&conn, id);
            black_box(r);
        });
        baseline_rusqlite::teardown(conn);
    });

    // Diesel
    group.bench_function("diesel", |b| {
        let mut conn = diesel_orm::setup();
        for i in 0..PREPARE_SIZE {
            diesel_orm::insert_one(
                &mut conn,
                &format!("user_{}", i),
                &format!("user_{}@test.com", i),
                i as i64,
            );
        }
        let mut idx = 0i64;
        b.iter(|| {
            let id = (idx % PREPARE_SIZE as i64) + 1;
            idx += 1;
            let r = diesel_orm::select_by_id(&mut conn, id);
            black_box(r);
        });
        diesel_orm::teardown(&mut conn);
    });

    // Async (sqlx + sea-orm + sz-orm)：setup 在外，每次迭代只做单次查询
    let rt = tokio::runtime::Runtime::new().expect("rt");

    // SQLx：setup + 插入数据在 b.iter 外
    group.bench_function("sqlx", |b| {
        let pool = rt.block_on(async {
            let pool = async_sqlx::setup().await;
            for i in 0..PREPARE_SIZE {
                async_sqlx::insert_one(
                    &pool,
                    &format!("user_{}", i),
                    &format!("user_{}@test.com", i),
                    i as i64,
                )
                .await;
            }
            pool
        });
        let mut idx = 0i64;
        b.to_async(&rt).iter(|| {
            let id = (idx % PREPARE_SIZE as i64) + 1;
            idx += 1;
            let pool = pool.clone();
            async move {
                let r = async_sqlx::select_by_id(&pool, id).await;
                black_box(r);
            }
        });
        rt.block_on(async { async_sqlx::teardown(&pool).await; });
    });

    // SeaORM：setup + 插入数据在 b.iter 外
    group.bench_function("sea-orm", |b| {
        let db = rt.block_on(async {
            let db = sea_orm_async::setup().await;
            for i in 0..PREPARE_SIZE {
                sea_orm_async::insert_one(
                    &db,
                    &format!("user_{}", i),
                    &format!("user_{}@test.com", i),
                    i as i64,
                )
                .await;
            }
            db
        });
        let mut idx = 0i64;
        b.to_async(&rt).iter(|| {
            let id = (idx % PREPARE_SIZE as i64) + 1;
            idx += 1;
            let db = db.clone();
            async move {
                let r = sea_orm_async::select_by_id(&db, id).await;
                black_box(r);
            }
        });
        rt.block_on(async { sea_orm_async::teardown(&db).await; });
    });

    // SZ-ORM：setup + 插入数据在 b.iter 外
    group.bench_function("sz-orm", |b| {
        let ctx = rt.block_on(async {
            let ctx = sz_orm::setup().await;
            for i in 0..PREPARE_SIZE {
                sz_orm::insert_one(
                    &ctx,
                    &format!("user_{}", i),
                    &format!("user_{}@test.com", i),
                    i as i64,
                )
                .await;
            }
            ctx
        });
        let mut idx = 0i64;
        b.to_async(&rt).iter(|| {
            let id = (idx % PREPARE_SIZE as i64) + 1;
            idx += 1;
            let ctx = ctx.clone();
            async move {
                let r = sz_orm::select_by_id(&ctx, id).await;
                black_box(r);
            }
        });
        rt.block_on(async { sz_orm::teardown(ctx).await; });
    });

    group.finish();
}

fn bench_select_all(c: &mut Criterion) {
    let mut group = c.benchmark_group("select_all_100000");
    group.throughput(Throughput::Elements(100000));

    const N: usize = 100000;

    // rusqlite
    group.bench_function("rusqlite", |b| {
        let conn = baseline_rusqlite::setup();
        for i in 0..N {
            baseline_rusqlite::insert_one(
                &conn,
                &format!("user_{}", i),
                &format!("user_{}@test.com", i),
                i as i64,
            );
        }
        b.iter(|| {
            let rows = baseline_rusqlite::select_all(&conn);
            black_box(rows);
        });
        baseline_rusqlite::teardown(conn);
    });

    // Diesel
    group.bench_function("diesel", |b| {
        let mut conn = diesel_orm::setup();
        for i in 0..N {
            diesel_orm::insert_one(
                &mut conn,
                &format!("user_{}", i),
                &format!("user_{}@test.com", i),
                i as i64,
            );
        }
        b.iter(|| {
            let rows = diesel_orm::select_all(&mut conn);
            black_box(rows);
        });
        diesel_orm::teardown(&mut conn);
    });

    // Async：setup + 插入数据在 b.iter 外，每次迭代只做 select_all
    let rt = tokio::runtime::Runtime::new().expect("rt");

    // SQLx
    group.bench_function("sqlx", |b| {
        let pool = rt.block_on(async {
            let pool = async_sqlx::setup().await;
            for i in 0..N {
                async_sqlx::insert_one(
                    &pool,
                    &format!("user_{}", i),
                    &format!("user_{}@test.com", i),
                    i as i64,
                )
                .await;
            }
            pool
        });
        b.to_async(&rt).iter(|| {
            let pool = pool.clone();
            async move {
                let rows = async_sqlx::select_all(&pool).await;
                black_box(rows);
            }
        });
        rt.block_on(async { async_sqlx::teardown(&pool).await; });
    });

    // SeaORM
    group.bench_function("sea-orm", |b| {
        let db = rt.block_on(async {
            let db = sea_orm_async::setup().await;
            for i in 0..N {
                sea_orm_async::insert_one(
                    &db,
                    &format!("user_{}", i),
                    &format!("user_{}@test.com", i),
                    i as i64,
                )
                .await;
            }
            db
        });
        b.to_async(&rt).iter(|| {
            let db = db.clone();
            async move {
                let rows = sea_orm_async::select_all(&db).await;
                black_box(rows);
            }
        });
        rt.block_on(async { sea_orm_async::teardown(&db).await; });
    });

    // SZ-ORM
    group.bench_function("sz-orm", |b| {
        let ctx = rt.block_on(async {
            let ctx = sz_orm::setup().await;
            for i in 0..N {
                sz_orm::insert_one(
                    &ctx,
                    &format!("user_{}", i),
                    &format!("user_{}@test.com", i),
                    i as i64,
                )
                .await;
            }
            ctx
        });
        b.to_async(&rt).iter(|| {
            let ctx = ctx.clone();
            async move {
                let rows = sz_orm::select_all(&ctx).await;
                black_box(rows);
            }
        });
        rt.block_on(async { sz_orm::teardown(ctx).await; });
    });

    group.finish();
}

fn bench_update(c: &mut Criterion) {
    let mut group = c.benchmark_group("update");
    group.throughput(Throughput::Elements(1));

    const N: usize = 100000;

    // rusqlite
    group.bench_function("rusqlite", |b| {
        let conn = baseline_rusqlite::setup();
        for i in 0..N {
            baseline_rusqlite::insert_one(
                &conn,
                &format!("user_{}", i),
                &format!("user_{}@test.com", i),
                i as i64,
            );
        }
        let mut idx = 0i64;
        b.iter(|| {
            let id = (idx % N as i64) + 1;
            idx += 1;
            baseline_rusqlite::update_by_id(&conn, id, "updated_name");
        });
        baseline_rusqlite::teardown(conn);
    });

    // Diesel
    group.bench_function("diesel", |b| {
        let mut conn = diesel_orm::setup();
        for i in 0..N {
            diesel_orm::insert_one(
                &mut conn,
                &format!("user_{}", i),
                &format!("user_{}@test.com", i),
                i as i64,
            );
        }
        let mut idx = 0i64;
        b.iter(|| {
            let id = (idx % N as i64) + 1;
            idx += 1;
            diesel_orm::update_by_id(&mut conn, id, "updated_name");
        });
        diesel_orm::teardown(&mut conn);
    });

    // Async：setup + 插入数据在 b.iter 外，每次迭代只做单次 update
    let rt = tokio::runtime::Runtime::new().expect("rt");

    // SQLx
    group.bench_function("sqlx", |b| {
        let pool = rt.block_on(async {
            let pool = async_sqlx::setup().await;
            for i in 0..N {
                async_sqlx::insert_one(
                    &pool,
                    &format!("user_{}", i),
                    &format!("user_{}@test.com", i),
                    i as i64,
                )
                .await;
            }
            pool
        });
        let mut idx = 0i64;
        b.to_async(&rt).iter(|| {
            let id = (idx % N as i64) + 1;
            idx += 1;
            let pool = pool.clone();
            async move {
                async_sqlx::update_by_id(&pool, id, "updated_name").await;
            }
        });
        rt.block_on(async { async_sqlx::teardown(&pool).await; });
    });

    // SeaORM
    group.bench_function("sea-orm", |b| {
        let db = rt.block_on(async {
            let db = sea_orm_async::setup().await;
            for i in 0..N {
                sea_orm_async::insert_one(
                    &db,
                    &format!("user_{}", i),
                    &format!("user_{}@test.com", i),
                    i as i64,
                )
                .await;
            }
            db
        });
        let mut idx = 0i64;
        b.to_async(&rt).iter(|| {
            let id = (idx % N as i64) + 1;
            idx += 1;
            let db = db.clone();
            async move {
                sea_orm_async::update_by_id(&db, id, "updated_name").await;
            }
        });
        rt.block_on(async { sea_orm_async::teardown(&db).await; });
    });

    // SZ-ORM
    group.bench_function("sz-orm", |b| {
        let ctx = rt.block_on(async {
            let ctx = sz_orm::setup().await;
            for i in 0..N {
                sz_orm::insert_one(
                    &ctx,
                    &format!("user_{}", i),
                    &format!("user_{}@test.com", i),
                    i as i64,
                )
                .await;
            }
            ctx
        });
        let mut idx = 0i64;
        b.to_async(&rt).iter(|| {
            let id = (idx % N as i64) + 1;
            idx += 1;
            let ctx = ctx.clone();
            async move {
                sz_orm::update_by_id(&ctx, id, "updated_name").await;
            }
        });
        rt.block_on(async { sz_orm::teardown(ctx).await; });
    });

    group.finish();
}

fn bench_delete(c: &mut Criterion) {
    let mut group = c.benchmark_group("delete");
    group.throughput(Throughput::Elements(1));

    const N: usize = 100000;

    // rusqlite
    group.bench_function("rusqlite", |b| {
        b.iter(|| {
            let conn = baseline_rusqlite::setup();
            for i in 0..N {
                baseline_rusqlite::insert_one(
                    &conn,
                    &format!("user_{}", i),
                    &format!("user_{}@test.com", i),
                    i as i64,
                );
            }
            for id in 1..=(N as i64) {
                baseline_rusqlite::delete_by_id(&conn, id);
            }
            baseline_rusqlite::teardown(conn);
        });
    });

    // Diesel
    group.bench_function("diesel", |b| {
        b.iter(|| {
            let mut conn = diesel_orm::setup();
            for i in 0..N {
                diesel_orm::insert_one(
                    &mut conn,
                    &format!("user_{}", i),
                    &format!("user_{}@test.com", i),
                    i as i64,
                );
            }
            for id in 1..=(N as i64) {
                diesel_orm::delete_by_id(&mut conn, id);
            }
            diesel_orm::teardown(&mut conn);
        });
    });

    // Async
    let rt = tokio::runtime::Runtime::new().expect("rt");

    group.bench_function("sqlx", |b| {
        b.to_async(&rt).iter_batched(
            || (),
            |_| async move {
                let pool = async_sqlx::setup().await;
                for i in 0..N {
                    async_sqlx::insert_one(
                        &pool,
                        &format!("user_{}", i),
                        &format!("user_{}@test.com", i),
                        i as i64,
                    )
                    .await;
                }
                for id in 1..=(N as i64) {
                    async_sqlx::delete_by_id(&pool, id).await;
                }
                async_sqlx::teardown(&pool).await;
            },
            criterion::BatchSize::PerIteration,
        );
    });

    group.bench_function("sea-orm", |b| {
        b.to_async(&rt).iter_batched(
            || (),
            |_| async move {
                let db = sea_orm_async::setup().await;
                for i in 0..N {
                    sea_orm_async::insert_one(
                        &db,
                        &format!("user_{}", i),
                        &format!("user_{}@test.com", i),
                        i as i64,
                    )
                    .await;
                }
                for id in 1..=(N as i64) {
                    sea_orm_async::delete_by_id(&db, id).await;
                }
                sea_orm_async::teardown(&db).await;
            },
            criterion::BatchSize::PerIteration,
        );
    });

    group.bench_function("sz-orm", |b| {
        b.to_async(&rt).iter_batched(
            || (),
            |_| async move {
                let ctx = sz_orm::setup().await;
                for i in 0..N {
                    sz_orm::insert_one(
                        &ctx,
                        &format!("user_{}", i),
                        &format!("user_{}@test.com", i),
                        i as i64,
                    )
                    .await;
                }
                for id in 1..=(N as i64) {
                    sz_orm::delete_by_id(&ctx, id).await;
                }
                sz_orm::teardown(ctx).await;
            },
            criterion::BatchSize::PerIteration,
        );
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_insert,
    bench_select_by_id,
    bench_select_all,
    bench_update,
    bench_delete,
);
criterion_main!(benches);
