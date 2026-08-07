//! CompetitorAdapter — 竞品适配层统一接口（v2.3.0 任务 B）
//!
//! 定义统一的基准测试接口，使 sz-orm / Diesel / SeaORM / SQLx
//! 在同一框架下运行全维度 benchmark。
//!
//! # 维度
//!
//! - CRUD 单条/批量
//! - 关联查询（HasOne / HasMany / ManyToMany）
//! - 事务（含 savepoint）
//! - 连接池
//! - 分页（OFFSET / 游标）

use std::sync::Arc;
use diesel::prelude::*;
use sea_orm::ConnectionTrait;

/// 统一基准记录结构
#[derive(Debug, Clone)]
pub struct BenchRecord {
    #[allow(dead_code)]
    pub id: i64,
    pub name: String,
    pub email: String,
    pub age: i32,
}

impl BenchRecord {
    pub fn new(id: i64) -> Self {
        Self {
            id,
            name: format!("user_{}", id),
            email: format!("user_{}@test.com", id),
            age: (id % 100) as i32,
        }
    }
}

/// 竞品能力枚举（标注不支持维度）
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum CompetitorCapability<T> {
    /// 竞品不支持该维度
    Unsupported(String),
    /// 执行出错
    Error(String),
    /// 成功结果
    Ok(T),
}

impl<T> CompetitorCapability<T> {
    #[allow(dead_code)]
    pub fn is_unsupported(&self) -> bool {
        matches!(self, CompetitorCapability::Unsupported(_))
    }

    #[allow(dead_code)]
    pub fn is_error(&self) -> bool {
        matches!(self, CompetitorCapability::Error(_))
    }

    #[allow(dead_code)]
    pub fn is_ok(&self) -> bool {
        matches!(self, CompetitorCapability::Ok(_))
    }
}

/// 基准测试统一接口 trait
#[async_trait::async_trait]
#[allow(dead_code)]
pub trait CompetitorAdapter: Send + Sync {
    /// 竞品名称
    fn name(&self) -> &str;

    /// 是否异步
    fn is_async(&self) -> bool;

    /// 初始化（建表 + 插入数据集）
    async fn setup(&mut self, dataset_size: usize) -> Result<(), String>;

    /// 清理（删表）
    async fn teardown(&mut self) -> Result<(), String>;

    /// CRUD 单条插入
    async fn insert_one(&mut self, record: &BenchRecord) -> CompetitorCapability<()>;

    /// CRUD 单条查询
    async fn find_one(&mut self, id: i64) -> CompetitorCapability<Option<BenchRecord>>;

    /// CRUD 单条更新
    async fn update_one(&mut self, id: i64, name: &str) -> CompetitorCapability<()>;

    /// CRUD 单条删除
    async fn delete_one(&mut self, id: i64) -> CompetitorCapability<()>;

    /// CRUD 批量插入
    async fn insert_batch(&mut self, records: &[BenchRecord]) -> CompetitorCapability<usize>;

    /// CRUD 批量查询
    async fn find_batch(&mut self, ids: &[i64]) -> CompetitorCapability<Vec<BenchRecord>>;

    /// 关联查询 HasOne（1:1）
    async fn find_with_has_one(&mut self, id: i64) -> CompetitorCapability<Vec<BenchRecord>>;

    /// 关联查询 HasMany（1:N）
    async fn find_with_has_many(&mut self, id: i64) -> CompetitorCapability<Vec<BenchRecord>>;

    /// 关联查询 ManyToMany（N:M）
    async fn find_with_many_to_many(&mut self, id: i64) -> CompetitorCapability<Vec<BenchRecord>>;

    /// 事务提交
    async fn transaction_commit(&mut self) -> CompetitorCapability<()>;

    /// 事务回滚
    async fn transaction_rollback(&mut self) -> CompetitorCapability<()>;

    /// 嵌套事务 / savepoint
    async fn nested_transaction(&mut self) -> CompetitorCapability<()>;

    /// 连接池获取
    async fn pool_acquire(&mut self) -> CompetitorCapability<()>;

    /// 分页 OFFSET/LIMIT
    async fn paginate_offset(&mut self, offset: usize, limit: usize) -> CompetitorCapability<Vec<BenchRecord>>;

    /// 分页游标（Keyset）
    async fn paginate_cursor(&mut self, last_id: i64, limit: usize) -> CompetitorCapability<Vec<BenchRecord>>;
}

/// 数据集规模档位
pub const DATASET_SIZES: &[usize] = &[10, 100, 1000, 10000];

/// 基准维度名称
#[allow(dead_code)]
pub const BENCH_DIMENSIONS: &[&str] = &[
    "crud_single",
    "crud_batch",
    "relation_has_one",
    "relation_has_many",
    "relation_many_to_many",
    "transaction",
    "pool",
    "pagination",
];

// ============================================================================
// 共享 SQL 常量
// ============================================================================

const CREATE_USERS: &str = "CREATE TABLE IF NOT EXISTS bench_users (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, email TEXT NOT NULL, age INTEGER NOT NULL)";
const CREATE_PROFILES: &str = "CREATE TABLE IF NOT EXISTS bench_profiles (id INTEGER PRIMARY KEY AUTOINCREMENT, user_id INTEGER NOT NULL, bio TEXT NOT NULL)";
const CREATE_POSTS: &str = "CREATE TABLE IF NOT EXISTS bench_posts (id INTEGER PRIMARY KEY AUTOINCREMENT, user_id INTEGER NOT NULL, title TEXT NOT NULL)";
const CREATE_TAGS: &str = "CREATE TABLE IF NOT EXISTS bench_tags (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL)";
const CREATE_POST_TAGS: &str = "CREATE TABLE IF NOT EXISTS bench_post_tags (post_id INTEGER NOT NULL, tag_id INTEGER NOT NULL)";

const DROP_USERS: &str = "DROP TABLE IF EXISTS bench_users";
const DROP_PROFILES: &str = "DROP TABLE IF EXISTS bench_profiles";
const DROP_POSTS: &str = "DROP TABLE IF EXISTS bench_posts";
const DROP_TAGS: &str = "DROP TABLE IF EXISTS bench_tags";
const DROP_POST_TAGS: &str = "DROP TABLE IF EXISTS bench_post_tags";

// ============================================================================
// SzOrmAdapter — sz-orm-core + sz-orm-sqlx（全维度支持）
// ============================================================================

pub struct SzOrmAdapter {
    pool: Option<sz_orm_core::Pool>,
    handle: Option<Arc<sz_orm_sqlx::SqlitePoolHandle>>,
}

impl SzOrmAdapter {
    pub fn new() -> Self {
        Self { pool: None, handle: None }
    }

    async fn conn(&self) -> Result<sz_orm_core::PooledConnection, String> {
        self.pool.as_ref().ok_or("pool not initialized")?
            .acquire().await
            .map_err(|e| format!("acquire: {}", e))
    }
}

#[async_trait::async_trait]
impl CompetitorAdapter for SzOrmAdapter {
    fn name(&self) -> &str { "sz-orm" }
    fn is_async(&self) -> bool { true }

    async fn setup(&mut self, dataset_size: usize) -> Result<(), String> {
        let handle = Arc::new(
            sz_orm_sqlx::SqlitePoolHandle::connect("sqlite::memory:?cache=shared")
                .await
                .map_err(|e| format!("connect: {}", e))?,
        );
        let factory = Arc::new(sz_orm_sqlx::SqlxSqliteConnectionFactory::new(handle.clone()));
        let config = sz_orm_core::PoolConfigBuilder::new().max_size(10).build().map_err(|e| format!("config: {}", e))?;
        let pool = sz_orm_core::Pool::new(config, factory).map_err(|e| format!("pool: {}", e))?;

        {
            let mut conn = pool.acquire().await.map_err(|e| format!("acquire: {}", e))?;
            conn.execute(CREATE_USERS).await.map_err(|e| format!("create users: {}", e))?;
            conn.execute(CREATE_PROFILES).await.map_err(|e| format!("create profiles: {}", e))?;
            conn.execute(CREATE_POSTS).await.map_err(|e| format!("create posts: {}", e))?;
            conn.execute(CREATE_TAGS).await.map_err(|e| format!("create tags: {}", e))?;
            conn.execute(CREATE_POST_TAGS).await.map_err(|e| format!("create post_tags: {}", e))?;

            for i in 1..=(dataset_size as i64) {
                let rec = BenchRecord::new(i);
                conn.execute_with_params(
                    "INSERT INTO bench_users (name, email, age) VALUES (?, ?, ?)",
                    &[sz_orm_core::Value::String(rec.name), sz_orm_core::Value::String(rec.email), sz_orm_core::Value::I32(rec.age)],
                ).await.map_err(|e| format!("insert user: {}", e))?;

                conn.execute_with_params(
                    "INSERT INTO bench_profiles (user_id, bio) VALUES (?, ?)",
                    &[sz_orm_core::Value::I64(i), sz_orm_core::Value::String(format!("bio_{}", i))],
                ).await.map_err(|e| format!("insert profile: {}", e))?;

                conn.execute_with_params(
                    "INSERT INTO bench_posts (user_id, title) VALUES (?, ?)",
                    &[sz_orm_core::Value::I64(i), sz_orm_core::Value::String(format!("post_{}", i))],
                ).await.map_err(|e| format!("insert post: {}", e))?;
            }

            conn.execute("INSERT INTO bench_tags (name) VALUES ('rust')").await.map_err(|e| format!("insert tag: {}", e))?;
            conn.execute("INSERT INTO bench_tags (name) VALUES ('orm')").await.map_err(|e| format!("insert tag: {}", e))?;
            conn.execute("INSERT INTO bench_post_tags (post_id, tag_id) VALUES (1, 1)").await.map_err(|e| format!("insert post_tag: {}", e))?;
            conn.execute("INSERT INTO bench_post_tags (post_id, tag_id) VALUES (1, 2)").await.map_err(|e| format!("insert post_tag: {}", e))?;
        }

        self.pool = Some(pool);
        self.handle = Some(handle);
        Ok(())
    }

    async fn teardown(&mut self) -> Result<(), String> {
        if let Some(pool) = &self.pool {
            let mut conn = pool.acquire().await.map_err(|e| format!("acquire: {}", e))?;
            conn.execute(DROP_POST_TAGS).await.ok();
            conn.execute(DROP_TAGS).await.ok();
            conn.execute(DROP_POSTS).await.ok();
            conn.execute(DROP_PROFILES).await.ok();
            conn.execute(DROP_USERS).await.ok();
        }
        if let Some(pool) = &self.pool {
            pool.close_all().await;
        }
        self.pool = None;
        self.handle = None;
        Ok(())
    }

    async fn insert_one(&mut self, record: &BenchRecord) -> CompetitorCapability<()> {
        let mut conn = match self.conn().await { Ok(c) => c, Err(e) => return CompetitorCapability::Error(e) };
        match conn.execute_with_params(
            "INSERT INTO bench_users (name, email, age) VALUES (?, ?, ?)",
            &[sz_orm_core::Value::String(record.name.clone()), sz_orm_core::Value::String(record.email.clone()), sz_orm_core::Value::I32(record.age)],
        ).await {
            Ok(_) => CompetitorCapability::Ok(()),
            Err(e) => CompetitorCapability::Error(format!("insert: {}", e)),
        }
    }

    async fn find_one(&mut self, id: i64) -> CompetitorCapability<Option<BenchRecord>> {
        let mut conn = match self.conn().await { Ok(c) => c, Err(e) => return CompetitorCapability::Error(e) };
        match conn.query_with_params("SELECT id, name, email, age FROM bench_users WHERE id = ?", &[sz_orm_core::Value::I64(id)]).await {
            Ok(rows) => {
                let rec = rows.first().map(|row| {
                    let name = match row.get("name") { Some(sz_orm_core::Value::String(s)) => s.clone(), _ => String::new() };
                    let email = match row.get("email") { Some(sz_orm_core::Value::String(s)) => s.clone(), _ => String::new() };
                    let age = match row.get("age") { Some(sz_orm_core::Value::I32(n)) => *n, Some(v) => v.as_i64().unwrap_or(0) as i32, None => 0 };
                    BenchRecord { id, name, email, age }
                });
                CompetitorCapability::Ok(rec)
            }
            Err(e) => CompetitorCapability::Error(format!("find: {}", e)),
        }
    }

    async fn update_one(&mut self, id: i64, name: &str) -> CompetitorCapability<()> {
        let mut conn = match self.conn().await { Ok(c) => c, Err(e) => return CompetitorCapability::Error(e) };
        match conn.execute_with_params("UPDATE bench_users SET name = ? WHERE id = ?", &[sz_orm_core::Value::String(name.to_string()), sz_orm_core::Value::I64(id)]).await {
            Ok(_) => CompetitorCapability::Ok(()),
            Err(e) => CompetitorCapability::Error(format!("update: {}", e)),
        }
    }

    async fn delete_one(&mut self, id: i64) -> CompetitorCapability<()> {
        let mut conn = match self.conn().await { Ok(c) => c, Err(e) => return CompetitorCapability::Error(e) };
        match conn.execute_with_params("DELETE FROM bench_users WHERE id = ?", &[sz_orm_core::Value::I64(id)]).await {
            Ok(_) => CompetitorCapability::Ok(()),
            Err(e) => CompetitorCapability::Error(format!("delete: {}", e)),
        }
    }

    async fn insert_batch(&mut self, records: &[BenchRecord]) -> CompetitorCapability<usize> {
        let mut conn = match self.conn().await { Ok(c) => c, Err(e) => return CompetitorCapability::Error(e) };
        let mut count = 0usize;
        for rec in records {
            match conn.execute_with_params(
                "INSERT INTO bench_users (name, email, age) VALUES (?, ?, ?)",
                &[sz_orm_core::Value::String(rec.name.clone()), sz_orm_core::Value::String(rec.email.clone()), sz_orm_core::Value::I32(rec.age)],
            ).await {
                Ok(_) => count += 1,
                Err(e) => return CompetitorCapability::Error(format!("batch insert: {}", e)),
            }
        }
        CompetitorCapability::Ok(count)
    }

    async fn find_batch(&mut self, ids: &[i64]) -> CompetitorCapability<Vec<BenchRecord>> {
        let mut conn = match self.conn().await { Ok(c) => c, Err(e) => return CompetitorCapability::Error(e) };
        let mut results = Vec::with_capacity(ids.len());
        for &id in ids {
            match conn.query_with_params("SELECT id, name, email, age FROM bench_users WHERE id = ?", &[sz_orm_core::Value::I64(id)]).await {
                Ok(rows) => {
                    if let Some(row) = rows.first() {
                        let name = match row.get("name") { Some(sz_orm_core::Value::String(s)) => s.clone(), _ => String::new() };
                        let email = match row.get("email") { Some(sz_orm_core::Value::String(s)) => s.clone(), _ => String::new() };
                        let age = match row.get("age") { Some(sz_orm_core::Value::I32(n)) => *n, Some(v) => v.as_i64().unwrap_or(0) as i32, None => 0 };
                        results.push(BenchRecord { id, name, email, age });
                    }
                }
                Err(e) => return CompetitorCapability::Error(format!("batch find: {}", e)),
            }
        }
        CompetitorCapability::Ok(results)
    }

    async fn find_with_has_one(&mut self, id: i64) -> CompetitorCapability<Vec<BenchRecord>> {
        let mut conn = match self.conn().await { Ok(c) => c, Err(e) => return CompetitorCapability::Error(e) };
        match conn.query_with_params(
            "SELECT u.id, u.name, u.email, u.age FROM bench_users u JOIN bench_profiles p ON p.user_id = u.id WHERE u.id = ?",
            &[sz_orm_core::Value::I64(id)],
        ).await {
            Ok(rows) => CompetitorCapability::Ok(rows.iter().map(|row| {
                let rid = match row.get("id") { Some(sz_orm_core::Value::I64(n)) => *n, Some(v) => v.as_i64().unwrap_or(0), None => 0 };
                let name = match row.get("name") { Some(sz_orm_core::Value::String(s)) => s.clone(), _ => String::new() };
                let email = match row.get("email") { Some(sz_orm_core::Value::String(s)) => s.clone(), _ => String::new() };
                let age = match row.get("age") { Some(sz_orm_core::Value::I32(n)) => *n, Some(v) => v.as_i64().unwrap_or(0) as i32, None => 0 };
                BenchRecord { id: rid, name, email, age }
            }).collect()),
            Err(e) => CompetitorCapability::Error(format!("has_one: {}", e)),
        }
    }

    async fn find_with_has_many(&mut self, id: i64) -> CompetitorCapability<Vec<BenchRecord>> {
        let mut conn = match self.conn().await { Ok(c) => c, Err(e) => return CompetitorCapability::Error(e) };
        match conn.query_with_params(
            "SELECT id, title FROM bench_posts WHERE user_id = ?",
            &[sz_orm_core::Value::I64(id)],
        ).await {
            Ok(rows) => CompetitorCapability::Ok(rows.iter().map(|row| {
                let rid = match row.get("id") { Some(sz_orm_core::Value::I64(n)) => *n, Some(v) => v.as_i64().unwrap_or(0), None => 0 };
                let title = match row.get("title") { Some(sz_orm_core::Value::String(s)) => s.clone(), _ => String::new() };
                BenchRecord { id: rid, name: title, email: String::new(), age: 0 }
            }).collect()),
            Err(e) => CompetitorCapability::Error(format!("has_many: {}", e)),
        }
    }

    async fn find_with_many_to_many(&mut self, id: i64) -> CompetitorCapability<Vec<BenchRecord>> {
        let mut conn = match self.conn().await { Ok(c) => c, Err(e) => return CompetitorCapability::Error(e) };
        match conn.query_with_params(
            "SELECT t.id, t.name FROM bench_tags t JOIN bench_post_tags pt ON pt.tag_id = t.id WHERE pt.post_id = ?",
            &[sz_orm_core::Value::I64(id)],
        ).await {
            Ok(rows) => CompetitorCapability::Ok(rows.iter().map(|row| {
                let rid = match row.get("id") { Some(sz_orm_core::Value::I64(n)) => *n, Some(v) => v.as_i64().unwrap_or(0), None => 0 };
                let name = match row.get("name") { Some(sz_orm_core::Value::String(s)) => s.clone(), _ => String::new() };
                BenchRecord { id: rid, name, email: String::new(), age: 0 }
            }).collect()),
            Err(e) => CompetitorCapability::Error(format!("m2m: {}", e)),
        }
    }

    async fn transaction_commit(&mut self) -> CompetitorCapability<()> {
        let mut conn = match self.conn().await { Ok(c) => c, Err(e) => return CompetitorCapability::Error(e) };
        match conn.execute("BEGIN").await {
            Ok(_) => {}
            Err(e) => return CompetitorCapability::Error(format!("begin: {}", e)),
        }
        match conn.execute_with_params("INSERT INTO bench_users (name, email, age) VALUES (?, ?, ?)", &[sz_orm_core::Value::String("tx_user".to_string()), sz_orm_core::Value::String("tx@test.com".to_string()), sz_orm_core::Value::I32(25)]).await {
            Ok(_) => {}
            Err(e) => return CompetitorCapability::Error(format!("insert: {}", e)),
        }
        match conn.execute("COMMIT").await {
            Ok(_) => CompetitorCapability::Ok(()),
            Err(e) => CompetitorCapability::Error(format!("commit: {}", e)),
        }
    }

    async fn transaction_rollback(&mut self) -> CompetitorCapability<()> {
        let mut conn = match self.conn().await { Ok(c) => c, Err(e) => return CompetitorCapability::Error(e) };
        let _ = conn.execute("BEGIN").await;
        let _ = conn.execute_with_params("INSERT INTO bench_users (name, email, age) VALUES (?, ?, ?)", &[sz_orm_core::Value::String("rb_user".to_string()), sz_orm_core::Value::String("rb@test.com".to_string()), sz_orm_core::Value::I32(30)]).await;
        match conn.execute("ROLLBACK").await {
            Ok(_) => CompetitorCapability::Ok(()),
            Err(e) => CompetitorCapability::Error(format!("rollback: {}", e)),
        }
    }

    async fn nested_transaction(&mut self) -> CompetitorCapability<()> {
        let mut conn = match self.conn().await { Ok(c) => c, Err(e) => return CompetitorCapability::Error(e) };
        let _ = conn.execute("BEGIN").await;
        let _ = conn.execute("SAVEPOINT sp1").await;
        let _ = conn.execute_with_params("INSERT INTO bench_users (name, email, age) VALUES (?, ?, ?)", &[sz_orm_core::Value::String("sp_user".to_string()), sz_orm_core::Value::String("sp@test.com".to_string()), sz_orm_core::Value::I32(35)]).await;
        let _ = conn.execute("RELEASE SAVEPOINT sp1").await;
        match conn.execute("COMMIT").await {
            Ok(_) => CompetitorCapability::Ok(()),
            Err(e) => CompetitorCapability::Error(format!("commit: {}", e)),
        }
    }

    async fn pool_acquire(&mut self) -> CompetitorCapability<()> {
        match self.conn().await {
            Ok(_) => CompetitorCapability::Ok(()),
            Err(e) => CompetitorCapability::Error(e),
        }
    }

    async fn paginate_offset(&mut self, offset: usize, limit: usize) -> CompetitorCapability<Vec<BenchRecord>> {
        let mut conn = match self.conn().await { Ok(c) => c, Err(e) => return CompetitorCapability::Error(e) };
        match conn.query_with_params("SELECT id, name, email, age FROM bench_users ORDER BY id LIMIT ? OFFSET ?", &[sz_orm_core::Value::I64(limit as i64), sz_orm_core::Value::I64(offset as i64)]).await {
            Ok(rows) => CompetitorCapability::Ok(rows.iter().map(|row| {
                let id = match row.get("id") { Some(sz_orm_core::Value::I64(n)) => *n, Some(v) => v.as_i64().unwrap_or(0), None => 0 };
                let name = match row.get("name") { Some(sz_orm_core::Value::String(s)) => s.clone(), _ => String::new() };
                let email = match row.get("email") { Some(sz_orm_core::Value::String(s)) => s.clone(), _ => String::new() };
                let age = match row.get("age") { Some(sz_orm_core::Value::I32(n)) => *n, Some(v) => v.as_i64().unwrap_or(0) as i32, None => 0 };
                BenchRecord { id, name, email, age }
            }).collect()),
            Err(e) => CompetitorCapability::Error(format!("paginate: {}", e)),
        }
    }

    async fn paginate_cursor(&mut self, last_id: i64, limit: usize) -> CompetitorCapability<Vec<BenchRecord>> {
        let mut conn = match self.conn().await { Ok(c) => c, Err(e) => return CompetitorCapability::Error(e) };
        match conn.query_with_params("SELECT id, name, email, age FROM bench_users WHERE id > ? ORDER BY id LIMIT ?", &[sz_orm_core::Value::I64(last_id), sz_orm_core::Value::I64(limit as i64)]).await {
            Ok(rows) => CompetitorCapability::Ok(rows.iter().map(|row| {
                let id = match row.get("id") { Some(sz_orm_core::Value::I64(n)) => *n, Some(v) => v.as_i64().unwrap_or(0), None => 0 };
                let name = match row.get("name") { Some(sz_orm_core::Value::String(s)) => s.clone(), _ => String::new() };
                let email = match row.get("email") { Some(sz_orm_core::Value::String(s)) => s.clone(), _ => String::new() };
                let age = match row.get("age") { Some(sz_orm_core::Value::I32(n)) => *n, Some(v) => v.as_i64().unwrap_or(0) as i32, None => 0 };
                BenchRecord { id, name, email, age }
            }).collect()),
            Err(e) => CompetitorCapability::Error(format!("cursor: {}", e)),
        }
    }
}

// ============================================================================
// SqlxAdapter — sqlx 直接驱动（CRUD/事务/池/分页，关联 Unsupported）
// ============================================================================

pub struct SqlxAdapter {
    pool: Option<sqlx::sqlite::SqlitePool>,
}

impl SqlxAdapter {
    pub fn new() -> Self {
        Self { pool: None }
    }
}

#[async_trait::async_trait]
impl CompetitorAdapter for SqlxAdapter {
    fn name(&self) -> &str { "sqlx" }
    fn is_async(&self) -> bool { true }

    async fn setup(&mut self, dataset_size: usize) -> Result<(), String> {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(10)
            .connect("sqlite::memory:?cache=shared")
            .await
            .map_err(|e| format!("connect: {}", e))?;
        sqlx::query(CREATE_USERS).execute(&pool).await.map_err(|e| format!("create users: {}", e))?;
        sqlx::query(CREATE_PROFILES).execute(&pool).await.map_err(|e| format!("create profiles: {}", e))?;
        sqlx::query(CREATE_POSTS).execute(&pool).await.map_err(|e| format!("create posts: {}", e))?;
        sqlx::query(CREATE_TAGS).execute(&pool).await.map_err(|e| format!("create tags: {}", e))?;
        sqlx::query(CREATE_POST_TAGS).execute(&pool).await.map_err(|e| format!("create post_tags: {}", e))?;

        for i in 1..=(dataset_size as i64) {
            let rec = BenchRecord::new(i);
            sqlx::query("INSERT INTO bench_users (name, email, age) VALUES (?, ?, ?)")
                .bind(&rec.name).bind(&rec.email).bind(rec.age)
                .execute(&pool).await.map_err(|e| format!("insert user: {}", e))?;
            sqlx::query("INSERT INTO bench_profiles (user_id, bio) VALUES (?, ?)")
                .bind(i).bind(format!("bio_{}", i))
                .execute(&pool).await.map_err(|e| format!("insert profile: {}", e))?;
            sqlx::query("INSERT INTO bench_posts (user_id, title) VALUES (?, ?)")
                .bind(i).bind(format!("post_{}", i))
                .execute(&pool).await.map_err(|e| format!("insert post: {}", e))?;
        }
        sqlx::query("INSERT INTO bench_tags (name) VALUES ('rust')").execute(&pool).await.map_err(|e| format!("insert tag: {}", e))?;
        sqlx::query("INSERT INTO bench_tags (name) VALUES ('orm')").execute(&pool).await.map_err(|e| format!("insert tag: {}", e))?;
        sqlx::query("INSERT INTO bench_post_tags (post_id, tag_id) VALUES (1, 1)").execute(&pool).await.map_err(|e| format!("insert pt: {}", e))?;
        sqlx::query("INSERT INTO bench_post_tags (post_id, tag_id) VALUES (1, 2)").execute(&pool).await.map_err(|e| format!("insert pt: {}", e))?;

        self.pool = Some(pool);
        Ok(())
    }

    async fn teardown(&mut self) -> Result<(), String> {
        if let Some(pool) = &self.pool {
            sqlx::query(DROP_POST_TAGS).execute(pool).await.ok();
            sqlx::query(DROP_TAGS).execute(pool).await.ok();
            sqlx::query(DROP_POSTS).execute(pool).await.ok();
            sqlx::query(DROP_PROFILES).execute(pool).await.ok();
            sqlx::query(DROP_USERS).execute(pool).await.ok();
            pool.close().await;
        }
        self.pool = None;
        Ok(())
    }

    async fn insert_one(&mut self, record: &BenchRecord) -> CompetitorCapability<()> {
        let pool = match &self.pool { Some(p) => p, None => return CompetitorCapability::Error("not initialized".to_string()) };
        match sqlx::query("INSERT INTO bench_users (name, email, age) VALUES (?, ?, ?)")
            .bind(&record.name).bind(&record.email).bind(record.age)
            .execute(pool).await {
            Ok(_) => CompetitorCapability::Ok(()),
            Err(e) => CompetitorCapability::Error(format!("insert: {}", e)),
        }
    }

    async fn find_one(&mut self, id: i64) -> CompetitorCapability<Option<BenchRecord>> {
        let pool = match &self.pool { Some(p) => p, None => return CompetitorCapability::Error("not initialized".to_string()) };
        match sqlx::query_as::<_, (i64, String, String, i32)>("SELECT id, name, email, age FROM bench_users WHERE id = ?")
            .bind(id).fetch_optional(pool).await {
            Ok(Some((id, name, email, age))) => CompetitorCapability::Ok(Some(BenchRecord { id, name, email, age })),
            Ok(None) => CompetitorCapability::Ok(None),
            Err(e) => CompetitorCapability::Error(format!("find: {}", e)),
        }
    }

    async fn update_one(&mut self, id: i64, name: &str) -> CompetitorCapability<()> {
        let pool = match &self.pool { Some(p) => p, None => return CompetitorCapability::Error("not initialized".to_string()) };
        match sqlx::query("UPDATE bench_users SET name = ? WHERE id = ?").bind(name).bind(id).execute(pool).await {
            Ok(_) => CompetitorCapability::Ok(()),
            Err(e) => CompetitorCapability::Error(format!("update: {}", e)),
        }
    }

    async fn delete_one(&mut self, id: i64) -> CompetitorCapability<()> {
        let pool = match &self.pool { Some(p) => p, None => return CompetitorCapability::Error("not initialized".to_string()) };
        match sqlx::query("DELETE FROM bench_users WHERE id = ?").bind(id).execute(pool).await {
            Ok(_) => CompetitorCapability::Ok(()),
            Err(e) => CompetitorCapability::Error(format!("delete: {}", e)),
        }
    }

    async fn insert_batch(&mut self, records: &[BenchRecord]) -> CompetitorCapability<usize> {
        let pool = match &self.pool { Some(p) => p, None => return CompetitorCapability::Error("not initialized".to_string()) };
        let mut count = 0usize;
        for rec in records {
            match sqlx::query("INSERT INTO bench_users (name, email, age) VALUES (?, ?, ?)")
                .bind(&rec.name).bind(&rec.email).bind(rec.age)
                .execute(pool).await {
                Ok(_) => count += 1,
                Err(e) => return CompetitorCapability::Error(format!("batch insert: {}", e)),
            }
        }
        CompetitorCapability::Ok(count)
    }

    async fn find_batch(&mut self, ids: &[i64]) -> CompetitorCapability<Vec<BenchRecord>> {
        let pool = match &self.pool { Some(p) => p, None => return CompetitorCapability::Error("not initialized".to_string()) };
        let mut results = Vec::with_capacity(ids.len());
        for &id in ids {
            match sqlx::query_as::<_, (i64, String, String, i32)>("SELECT id, name, email, age FROM bench_users WHERE id = ?")
                .bind(id).fetch_optional(pool).await {
                Ok(Some((id, name, email, age))) => results.push(BenchRecord { id, name, email, age }),
                Ok(None) => {}
                Err(e) => return CompetitorCapability::Error(format!("batch find: {}", e)),
            }
        }
        CompetitorCapability::Ok(results)
    }

    async fn find_with_has_one(&mut self, _id: i64) -> CompetitorCapability<Vec<BenchRecord>> {
        CompetitorCapability::Unsupported("SQLx has no ORM-level relation abstraction".to_string())
    }

    async fn find_with_has_many(&mut self, _id: i64) -> CompetitorCapability<Vec<BenchRecord>> {
        CompetitorCapability::Unsupported("SQLx has no ORM-level relation abstraction".to_string())
    }

    async fn find_with_many_to_many(&mut self, _id: i64) -> CompetitorCapability<Vec<BenchRecord>> {
        CompetitorCapability::Unsupported("SQLx has no ORM-level relation abstraction".to_string())
    }

    async fn transaction_commit(&mut self) -> CompetitorCapability<()> {
        let pool = match &self.pool { Some(p) => p, None => return CompetitorCapability::Error("not initialized".to_string()) };
        match sqlx::query("BEGIN").execute(pool).await { Ok(_) => {}, Err(e) => return CompetitorCapability::Error(format!("begin: {}", e)) };
        match sqlx::query("INSERT INTO bench_users (name, email, age) VALUES (?, ?, ?)").bind("tx_user").bind("tx@test.com").bind(25).execute(pool).await {
            Ok(_) => {},
            Err(e) => return CompetitorCapability::Error(format!("insert: {}", e)),
        }
        match sqlx::query("COMMIT").execute(pool).await {
            Ok(_) => CompetitorCapability::Ok(()),
            Err(e) => CompetitorCapability::Error(format!("commit: {}", e)),
        }
    }

    async fn transaction_rollback(&mut self) -> CompetitorCapability<()> {
        let pool = match &self.pool { Some(p) => p, None => return CompetitorCapability::Error("not initialized".to_string()) };
        let _ = sqlx::query("BEGIN").execute(pool).await;
        let _ = sqlx::query("INSERT INTO bench_users (name, email, age) VALUES (?, ?, ?)").bind("rb_user").bind("rb@test.com").bind(30).execute(pool).await;
        match sqlx::query("ROLLBACK").execute(pool).await {
            Ok(_) => CompetitorCapability::Ok(()),
            Err(e) => CompetitorCapability::Error(format!("rollback: {}", e)),
        }
    }

    async fn nested_transaction(&mut self) -> CompetitorCapability<()> {
        let pool = match &self.pool { Some(p) => p, None => return CompetitorCapability::Error("not initialized".to_string()) };
        let _ = sqlx::query("BEGIN").execute(pool).await;
        let _ = sqlx::query("SAVEPOINT sp1").execute(pool).await;
        let _ = sqlx::query("INSERT INTO bench_users (name, email, age) VALUES (?, ?, ?)").bind("sp_user").bind("sp@test.com").bind(35).execute(pool).await;
        let _ = sqlx::query("RELEASE SAVEPOINT sp1").execute(pool).await;
        match sqlx::query("COMMIT").execute(pool).await {
            Ok(_) => CompetitorCapability::Ok(()),
            Err(e) => CompetitorCapability::Error(format!("commit: {}", e)),
        }
    }

    async fn pool_acquire(&mut self) -> CompetitorCapability<()> {
        let pool = match &self.pool { Some(p) => p, None => return CompetitorCapability::Error("not initialized".to_string()) };
        match pool.acquire().await {
            Ok(_) => CompetitorCapability::Ok(()),
            Err(e) => CompetitorCapability::Error(format!("acquire: {}", e)),
        }
    }

    async fn paginate_offset(&mut self, offset: usize, limit: usize) -> CompetitorCapability<Vec<BenchRecord>> {
        let pool = match &self.pool { Some(p) => p, None => return CompetitorCapability::Error("not initialized".to_string()) };
        match sqlx::query_as::<_, (i64, String, String, i32)>("SELECT id, name, email, age FROM bench_users ORDER BY id LIMIT ? OFFSET ?")
            .bind(limit as i64).bind(offset as i64).fetch_all(pool).await {
            Ok(rows) => CompetitorCapability::Ok(rows.into_iter().map(|(id, name, email, age)| BenchRecord { id, name, email, age }).collect()),
            Err(e) => CompetitorCapability::Error(format!("paginate: {}", e)),
        }
    }

    async fn paginate_cursor(&mut self, last_id: i64, limit: usize) -> CompetitorCapability<Vec<BenchRecord>> {
        let pool = match &self.pool { Some(p) => p, None => return CompetitorCapability::Error("not initialized".to_string()) };
        match sqlx::query_as::<_, (i64, String, String, i32)>("SELECT id, name, email, age FROM bench_users WHERE id > ? ORDER BY id LIMIT ?")
            .bind(last_id).bind(limit as i64).fetch_all(pool).await {
            Ok(rows) => CompetitorCapability::Ok(rows.into_iter().map(|(id, name, email, age)| BenchRecord { id, name, email, age }).collect()),
            Err(e) => CompetitorCapability::Error(format!("cursor: {}", e)),
        }
    }
}

// ============================================================================
// DieselAdapter — Diesel 同步 ORM（CRUD/事务/分页，关联用 raw SQL JOIN）
// ============================================================================

pub struct DieselAdapter {
    conn: Option<std::sync::Mutex<diesel::sqlite::SqliteConnection>>,
}

impl DieselAdapter {
    pub fn new() -> Self {
        Self { conn: None }
    }
}

#[async_trait::async_trait]
impl CompetitorAdapter for DieselAdapter {
    fn name(&self) -> &str { "diesel" }
    fn is_async(&self) -> bool { false }

    async fn setup(&mut self, dataset_size: usize) -> Result<(), String> {
        let mut conn = diesel::sqlite::SqliteConnection::establish(":memory:")
            .map_err(|e| format!("connect: {}", e))?;
        diesel::sql_query(CREATE_USERS).execute(&mut conn).map_err(|e| format!("create users: {}", e))?;
        diesel::sql_query(CREATE_PROFILES).execute(&mut conn).map_err(|e| format!("create profiles: {}", e))?;
        diesel::sql_query(CREATE_POSTS).execute(&mut conn).map_err(|e| format!("create posts: {}", e))?;
        diesel::sql_query(CREATE_TAGS).execute(&mut conn).map_err(|e| format!("create tags: {}", e))?;
        diesel::sql_query(CREATE_POST_TAGS).execute(&mut conn).map_err(|e| format!("create post_tags: {}", e))?;

        for i in 1..=(dataset_size as i64) {
            let rec = BenchRecord::new(i);
            diesel::sql_query("INSERT INTO bench_users (name, email, age) VALUES (?, ?, ?)")
                .bind::<diesel::sql_types::Text, _>(rec.name)
                .bind::<diesel::sql_types::Text, _>(rec.email)
                .bind::<diesel::sql_types::Integer, _>(rec.age)
                .execute(&mut conn).map_err(|e| format!("insert user: {}", e))?;
            diesel::sql_query("INSERT INTO bench_profiles (user_id, bio) VALUES (?, ?)")
                .bind::<diesel::sql_types::BigInt, _>(i)
                .bind::<diesel::sql_types::Text, _>(format!("bio_{}", i))
                .execute(&mut conn).map_err(|e| format!("insert profile: {}", e))?;
            diesel::sql_query("INSERT INTO bench_posts (user_id, title) VALUES (?, ?)")
                .bind::<diesel::sql_types::BigInt, _>(i)
                .bind::<diesel::sql_types::Text, _>(format!("post_{}", i))
                .execute(&mut conn).map_err(|e| format!("insert post: {}", e))?;
        }
        diesel::sql_query("INSERT INTO bench_tags (name) VALUES ('rust')").execute(&mut conn).map_err(|e| format!("insert tag: {}", e))?;
        diesel::sql_query("INSERT INTO bench_tags (name) VALUES ('orm')").execute(&mut conn).map_err(|e| format!("insert tag: {}", e))?;
        diesel::sql_query("INSERT INTO bench_post_tags (post_id, tag_id) VALUES (1, 1)").execute(&mut conn).map_err(|e| format!("insert pt: {}", e))?;
        diesel::sql_query("INSERT INTO bench_post_tags (post_id, tag_id) VALUES (1, 2)").execute(&mut conn).map_err(|e| format!("insert pt: {}", e))?;

        self.conn = Some(std::sync::Mutex::new(conn));
        Ok(())
    }

    async fn teardown(&mut self) -> Result<(), String> {
        if let Some(conn) = &self.conn {
            let mut c = conn.lock().map_err(|e| format!("lock: {}", e))?;
            diesel::sql_query(DROP_POST_TAGS).execute(&mut *c).ok();
            diesel::sql_query(DROP_TAGS).execute(&mut *c).ok();
            diesel::sql_query(DROP_POSTS).execute(&mut *c).ok();
            diesel::sql_query(DROP_PROFILES).execute(&mut *c).ok();
            diesel::sql_query(DROP_USERS).execute(&mut *c).ok();
        }
        self.conn = None;
        Ok(())
    }

    async fn insert_one(&mut self, record: &BenchRecord) -> CompetitorCapability<()> {
        let conn = match &self.conn { Some(c) => c, None => return CompetitorCapability::Error("not initialized".to_string()) };
        let mut c = match conn.lock() { Ok(c) => c, Err(e) => return CompetitorCapability::Error(format!("lock: {}", e)) };
        match diesel::sql_query("INSERT INTO bench_users (name, email, age) VALUES (?, ?, ?)")
            .bind::<diesel::sql_types::Text, _>(&record.name)
            .bind::<diesel::sql_types::Text, _>(&record.email)
            .bind::<diesel::sql_types::Integer, _>(record.age)
            .execute(&mut *c) {
            Ok(_) => CompetitorCapability::Ok(()),
            Err(e) => CompetitorCapability::Error(format!("insert: {}", e)),
        }
    }

    async fn find_one(&mut self, id: i64) -> CompetitorCapability<Option<BenchRecord>> {
        let conn = match &self.conn { Some(c) => c, None => return CompetitorCapability::Error("not initialized".to_string()) };
        let mut c = match conn.lock() { Ok(c) => c, Err(e) => return CompetitorCapability::Error(format!("lock: {}", e)) };
        use diesel::prelude::*;
        #[derive(QueryableByName)]
        struct R { #[diesel(sql_type = diesel::sql_types::BigInt)] id: i64, #[diesel(sql_type = diesel::sql_types::Text)] name: String, #[diesel(sql_type = diesel::sql_types::Text)] email: String, #[diesel(sql_type = diesel::sql_types::Integer)] age: i32 }
        match diesel::sql_query("SELECT id, name, email, age FROM bench_users WHERE id = ?")
            .bind::<diesel::sql_types::BigInt, _>(id)
            .load::<R>(&mut *c) {
            Ok(rows) => CompetitorCapability::Ok(rows.into_iter().next().map(|r| BenchRecord { id: r.id, name: r.name, email: r.email, age: r.age })),
            Err(e) => CompetitorCapability::Error(format!("find: {}", e)),
        }
    }

    async fn update_one(&mut self, id: i64, name: &str) -> CompetitorCapability<()> {
        let conn = match &self.conn { Some(c) => c, None => return CompetitorCapability::Error("not initialized".to_string()) };
        let mut c = match conn.lock() { Ok(c) => c, Err(e) => return CompetitorCapability::Error(format!("lock: {}", e)) };
        match diesel::sql_query("UPDATE bench_users SET name = ? WHERE id = ?")
            .bind::<diesel::sql_types::Text, _>(name)
            .bind::<diesel::sql_types::BigInt, _>(id)
            .execute(&mut *c) {
            Ok(_) => CompetitorCapability::Ok(()),
            Err(e) => CompetitorCapability::Error(format!("update: {}", e)),
        }
    }

    async fn delete_one(&mut self, id: i64) -> CompetitorCapability<()> {
        let conn = match &self.conn { Some(c) => c, None => return CompetitorCapability::Error("not initialized".to_string()) };
        let mut c = match conn.lock() { Ok(c) => c, Err(e) => return CompetitorCapability::Error(format!("lock: {}", e)) };
        match diesel::sql_query("DELETE FROM bench_users WHERE id = ?")
            .bind::<diesel::sql_types::BigInt, _>(id)
            .execute(&mut *c) {
            Ok(_) => CompetitorCapability::Ok(()),
            Err(e) => CompetitorCapability::Error(format!("delete: {}", e)),
        }
    }

    async fn insert_batch(&mut self, records: &[BenchRecord]) -> CompetitorCapability<usize> {
        let conn = match &self.conn { Some(c) => c, None => return CompetitorCapability::Error("not initialized".to_string()) };
        let mut c = match conn.lock() { Ok(c) => c, Err(e) => return CompetitorCapability::Error(format!("lock: {}", e)) };
        let mut count = 0usize;
        for rec in records {
            match diesel::sql_query("INSERT INTO bench_users (name, email, age) VALUES (?, ?, ?)")
                .bind::<diesel::sql_types::Text, _>(&rec.name)
                .bind::<diesel::sql_types::Text, _>(&rec.email)
                .bind::<diesel::sql_types::Integer, _>(rec.age)
                .execute(&mut *c) {
                Ok(_) => count += 1,
                Err(e) => return CompetitorCapability::Error(format!("batch insert: {}", e)),
            }
        }
        CompetitorCapability::Ok(count)
    }

    async fn find_batch(&mut self, ids: &[i64]) -> CompetitorCapability<Vec<BenchRecord>> {
        let conn = match &self.conn { Some(c) => c, None => return CompetitorCapability::Error("not initialized".to_string()) };
        let mut c = match conn.lock() { Ok(c) => c, Err(e) => return CompetitorCapability::Error(format!("lock: {}", e)) };
        use diesel::prelude::*;
        #[derive(QueryableByName)]
        struct R { #[diesel(sql_type = diesel::sql_types::BigInt)] id: i64, #[diesel(sql_type = diesel::sql_types::Text)] name: String, #[diesel(sql_type = diesel::sql_types::Text)] email: String, #[diesel(sql_type = diesel::sql_types::Integer)] age: i32 }
        let mut results = Vec::with_capacity(ids.len());
        for &id in ids {
            match diesel::sql_query("SELECT id, name, email, age FROM bench_users WHERE id = ?")
                .bind::<diesel::sql_types::BigInt, _>(id)
                .load::<R>(&mut *c) {
                Ok(rows) => { if let Some(r) = rows.into_iter().next() { results.push(BenchRecord { id: r.id, name: r.name, email: r.email, age: r.age }); } }
                Err(e) => return CompetitorCapability::Error(format!("batch find: {}", e)),
            }
        }
        CompetitorCapability::Ok(results)
    }

    async fn find_with_has_one(&mut self, id: i64) -> CompetitorCapability<Vec<BenchRecord>> {
        let conn = match &self.conn { Some(c) => c, None => return CompetitorCapability::Error("not initialized".to_string()) };
        let mut c = match conn.lock() { Ok(c) => c, Err(e) => return CompetitorCapability::Error(format!("lock: {}", e)) };
        use diesel::prelude::*;
        #[derive(QueryableByName)]
        struct R { #[diesel(sql_type = diesel::sql_types::BigInt)] id: i64, #[diesel(sql_type = diesel::sql_types::Text)] name: String, #[diesel(sql_type = diesel::sql_types::Text)] email: String, #[diesel(sql_type = diesel::sql_types::Integer)] age: i32 }
        match diesel::sql_query("SELECT u.id, u.name, u.email, u.age FROM bench_users u JOIN bench_profiles p ON p.user_id = u.id WHERE u.id = ?")
            .bind::<diesel::sql_types::BigInt, _>(id)
            .load::<R>(&mut *c) {
            Ok(rows) => CompetitorCapability::Ok(rows.into_iter().map(|r| BenchRecord { id: r.id, name: r.name, email: r.email, age: r.age }).collect()),
            Err(e) => CompetitorCapability::Error(format!("has_one: {}", e)),
        }
    }

    async fn find_with_has_many(&mut self, id: i64) -> CompetitorCapability<Vec<BenchRecord>> {
        let conn = match &self.conn { Some(c) => c, None => return CompetitorCapability::Error("not initialized".to_string()) };
        let mut c = match conn.lock() { Ok(c) => c, Err(e) => return CompetitorCapability::Error(format!("lock: {}", e)) };
        use diesel::prelude::*;
        #[derive(QueryableByName)]
        struct R { #[diesel(sql_type = diesel::sql_types::BigInt)] id: i64, #[diesel(sql_type = diesel::sql_types::Text)] title: String }
        match diesel::sql_query("SELECT id, title FROM bench_posts WHERE user_id = ?")
            .bind::<diesel::sql_types::BigInt, _>(id)
            .load::<R>(&mut *c) {
            Ok(rows) => CompetitorCapability::Ok(rows.into_iter().map(|r| BenchRecord { id: r.id, name: r.title, email: String::new(), age: 0 }).collect()),
            Err(e) => CompetitorCapability::Error(format!("has_many: {}", e)),
        }
    }

    async fn find_with_many_to_many(&mut self, id: i64) -> CompetitorCapability<Vec<BenchRecord>> {
        let conn = match &self.conn { Some(c) => c, None => return CompetitorCapability::Error("not initialized".to_string()) };
        let mut c = match conn.lock() { Ok(c) => c, Err(e) => return CompetitorCapability::Error(format!("lock: {}", e)) };
        use diesel::prelude::*;
        #[derive(QueryableByName)]
        struct R { #[diesel(sql_type = diesel::sql_types::BigInt)] id: i64, #[diesel(sql_type = diesel::sql_types::Text)] name: String }
        match diesel::sql_query("SELECT t.id, t.name FROM bench_tags t JOIN bench_post_tags pt ON pt.tag_id = t.id WHERE pt.post_id = ?")
            .bind::<diesel::sql_types::BigInt, _>(id)
            .load::<R>(&mut *c) {
            Ok(rows) => CompetitorCapability::Ok(rows.into_iter().map(|r| BenchRecord { id: r.id, name: r.name, email: String::new(), age: 0 }).collect()),
            Err(e) => CompetitorCapability::Error(format!("m2m: {}", e)),
        }
    }

    async fn transaction_commit(&mut self) -> CompetitorCapability<()> {
        let conn = match &self.conn { Some(c) => c, None => return CompetitorCapability::Error("not initialized".to_string()) };
        let mut c = match conn.lock() { Ok(c) => c, Err(e) => return CompetitorCapability::Error(format!("lock: {}", e)) };
        diesel::sql_query("BEGIN").execute(&mut *c).ok();
        diesel::sql_query("INSERT INTO bench_users (name, email, age) VALUES (?, ?, ?)")
            .bind::<diesel::sql_types::Text, _>("tx_user")
            .bind::<diesel::sql_types::Text, _>("tx@test.com")
            .bind::<diesel::sql_types::Integer, _>(25)
            .execute(&mut *c).ok();
        match diesel::sql_query("COMMIT").execute(&mut *c) {
            Ok(_) => CompetitorCapability::Ok(()),
            Err(e) => CompetitorCapability::Error(format!("commit: {}", e)),
        }
    }

    async fn transaction_rollback(&mut self) -> CompetitorCapability<()> {
        let conn = match &self.conn { Some(c) => c, None => return CompetitorCapability::Error("not initialized".to_string()) };
        let mut c = match conn.lock() { Ok(c) => c, Err(e) => return CompetitorCapability::Error(format!("lock: {}", e)) };
        diesel::sql_query("BEGIN").execute(&mut *c).ok();
        diesel::sql_query("INSERT INTO bench_users (name, email, age) VALUES (?, ?, ?)")
            .bind::<diesel::sql_types::Text, _>("rb_user")
            .bind::<diesel::sql_types::Text, _>("rb@test.com")
            .bind::<diesel::sql_types::Integer, _>(30)
            .execute(&mut *c).ok();
        match diesel::sql_query("ROLLBACK").execute(&mut *c) {
            Ok(_) => CompetitorCapability::Ok(()),
            Err(e) => CompetitorCapability::Error(format!("rollback: {}", e)),
        }
    }

    async fn nested_transaction(&mut self) -> CompetitorCapability<()> {
        let conn = match &self.conn { Some(c) => c, None => return CompetitorCapability::Error("not initialized".to_string()) };
        let mut c = match conn.lock() { Ok(c) => c, Err(e) => return CompetitorCapability::Error(format!("lock: {}", e)) };
        diesel::sql_query("BEGIN").execute(&mut *c).ok();
        diesel::sql_query("SAVEPOINT sp1").execute(&mut *c).ok();
        diesel::sql_query("INSERT INTO bench_users (name, email, age) VALUES (?, ?, ?)")
            .bind::<diesel::sql_types::Text, _>("sp_user")
            .bind::<diesel::sql_types::Text, _>("sp@test.com")
            .bind::<diesel::sql_types::Integer, _>(35)
            .execute(&mut *c).ok();
        diesel::sql_query("RELEASE SAVEPOINT sp1").execute(&mut *c).ok();
        match diesel::sql_query("COMMIT").execute(&mut *c) {
            Ok(_) => CompetitorCapability::Ok(()),
            Err(e) => CompetitorCapability::Error(format!("commit: {}", e)),
        }
    }

    async fn pool_acquire(&mut self) -> CompetitorCapability<()> {
        CompetitorCapability::Unsupported("Diesel uses a single connection, no pool".to_string())
    }

    async fn paginate_offset(&mut self, offset: usize, limit: usize) -> CompetitorCapability<Vec<BenchRecord>> {
        let conn = match &self.conn { Some(c) => c, None => return CompetitorCapability::Error("not initialized".to_string()) };
        let mut c = match conn.lock() { Ok(c) => c, Err(e) => return CompetitorCapability::Error(format!("lock: {}", e)) };
        use diesel::prelude::*;
        #[derive(QueryableByName)]
        struct R { #[diesel(sql_type = diesel::sql_types::BigInt)] id: i64, #[diesel(sql_type = diesel::sql_types::Text)] name: String, #[diesel(sql_type = diesel::sql_types::Text)] email: String, #[diesel(sql_type = diesel::sql_types::Integer)] age: i32 }
        match diesel::sql_query("SELECT id, name, email, age FROM bench_users ORDER BY id LIMIT ? OFFSET ?")
            .bind::<diesel::sql_types::BigInt, _>(limit as i64)
            .bind::<diesel::sql_types::BigInt, _>(offset as i64)
            .load::<R>(&mut *c) {
            Ok(rows) => CompetitorCapability::Ok(rows.into_iter().map(|r| BenchRecord { id: r.id, name: r.name, email: r.email, age: r.age }).collect()),
            Err(e) => CompetitorCapability::Error(format!("paginate: {}", e)),
        }
    }

    async fn paginate_cursor(&mut self, last_id: i64, limit: usize) -> CompetitorCapability<Vec<BenchRecord>> {
        let conn = match &self.conn { Some(c) => c, None => return CompetitorCapability::Error("not initialized".to_string()) };
        let mut c = match conn.lock() { Ok(c) => c, Err(e) => return CompetitorCapability::Error(format!("lock: {}", e)) };
        use diesel::prelude::*;
        #[derive(QueryableByName)]
        struct R { #[diesel(sql_type = diesel::sql_types::BigInt)] id: i64, #[diesel(sql_type = diesel::sql_types::Text)] name: String, #[diesel(sql_type = diesel::sql_types::Text)] email: String, #[diesel(sql_type = diesel::sql_types::Integer)] age: i32 }
        match diesel::sql_query("SELECT id, name, email, age FROM bench_users WHERE id > ? ORDER BY id LIMIT ?")
            .bind::<diesel::sql_types::BigInt, _>(last_id)
            .bind::<diesel::sql_types::BigInt, _>(limit as i64)
            .load::<R>(&mut *c) {
            Ok(rows) => CompetitorCapability::Ok(rows.into_iter().map(|r| BenchRecord { id: r.id, name: r.name, email: r.email, age: r.age }).collect()),
            Err(e) => CompetitorCapability::Error(format!("cursor: {}", e)),
        }
    }
}

// ============================================================================
// SeaOrmAdapter — SeaORM 异步 ORM（CRUD/事务/分页，关联用 raw SQL JOIN）
// ============================================================================

pub struct SeaOrmAdapter {
    db: Option<sea_orm::DatabaseConnection>,
}

impl SeaOrmAdapter {
    pub fn new() -> Self {
        Self { db: None }
    }
}

#[async_trait::async_trait]
impl CompetitorAdapter for SeaOrmAdapter {
    fn name(&self) -> &str { "sea-orm" }
    fn is_async(&self) -> bool { true }

    async fn setup(&mut self, dataset_size: usize) -> Result<(), String> {
        use sea_orm::{ConnectOptions, ConnectionTrait, Database, DatabaseBackend, Statement};
        let mut opt = ConnectOptions::new("sqlite::memory:?cache=shared");
        opt.max_connections(10);
        let db = Database::connect(opt).await.map_err(|e| format!("connect: {}", e))?;
        db.execute(Statement::from_sql_and_values(DatabaseBackend::Sqlite, CREATE_USERS, vec![])).await.map_err(|e| format!("create users: {}", e))?;
        db.execute(Statement::from_sql_and_values(DatabaseBackend::Sqlite, CREATE_PROFILES, vec![])).await.map_err(|e| format!("create profiles: {}", e))?;
        db.execute(Statement::from_sql_and_values(DatabaseBackend::Sqlite, CREATE_POSTS, vec![])).await.map_err(|e| format!("create posts: {}", e))?;
        db.execute(Statement::from_sql_and_values(DatabaseBackend::Sqlite, CREATE_TAGS, vec![])).await.map_err(|e| format!("create tags: {}", e))?;
        db.execute(Statement::from_sql_and_values(DatabaseBackend::Sqlite, CREATE_POST_TAGS, vec![])).await.map_err(|e| format!("create post_tags: {}", e))?;

        for i in 1..=(dataset_size as i64) {
            let rec = BenchRecord::new(i);
            db.execute(Statement::from_sql_and_values(DatabaseBackend::Sqlite, "INSERT INTO bench_users (name, email, age) VALUES (?, ?, ?)", vec![rec.name.into(), rec.email.into(), rec.age.into()])).await.map_err(|e| format!("insert user: {}", e))?;
            db.execute(Statement::from_sql_and_values(DatabaseBackend::Sqlite, "INSERT INTO bench_profiles (user_id, bio) VALUES (?, ?)", vec![i.into(), format!("bio_{}", i).into()])).await.map_err(|e| format!("insert profile: {}", e))?;
            db.execute(Statement::from_sql_and_values(DatabaseBackend::Sqlite, "INSERT INTO bench_posts (user_id, title) VALUES (?, ?)", vec![i.into(), format!("post_{}", i).into()])).await.map_err(|e| format!("insert post: {}", e))?;
        }
        db.execute(Statement::from_sql_and_values(DatabaseBackend::Sqlite, "INSERT INTO bench_tags (name) VALUES ('rust')", vec![])).await.map_err(|e| format!("insert tag: {}", e))?;
        db.execute(Statement::from_sql_and_values(DatabaseBackend::Sqlite, "INSERT INTO bench_tags (name) VALUES ('orm')", vec![])).await.map_err(|e| format!("insert tag: {}", e))?;
        db.execute(Statement::from_sql_and_values(DatabaseBackend::Sqlite, "INSERT INTO bench_post_tags (post_id, tag_id) VALUES (1, 1)", vec![])).await.map_err(|e| format!("insert pt: {}", e))?;
        db.execute(Statement::from_sql_and_values(DatabaseBackend::Sqlite, "INSERT INTO bench_post_tags (post_id, tag_id) VALUES (1, 2)", vec![])).await.map_err(|e| format!("insert pt: {}", e))?;

        self.db = Some(db);
        Ok(())
    }

    async fn teardown(&mut self) -> Result<(), String> {
        use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};
        if let Some(db) = &self.db {
            db.execute(Statement::from_sql_and_values(DatabaseBackend::Sqlite, DROP_POST_TAGS, vec![])).await.ok();
            db.execute(Statement::from_sql_and_values(DatabaseBackend::Sqlite, DROP_TAGS, vec![])).await.ok();
            db.execute(Statement::from_sql_and_values(DatabaseBackend::Sqlite, DROP_POSTS, vec![])).await.ok();
            db.execute(Statement::from_sql_and_values(DatabaseBackend::Sqlite, DROP_PROFILES, vec![])).await.ok();
            db.execute(Statement::from_sql_and_values(DatabaseBackend::Sqlite, DROP_USERS, vec![])).await.ok();
        }
        self.db = None;
        Ok(())
    }

    async fn insert_one(&mut self, record: &BenchRecord) -> CompetitorCapability<()> {
        use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};
        let db = match &self.db { Some(d) => d, None => return CompetitorCapability::Error("not initialized".to_string()) };
        match db.execute(Statement::from_sql_and_values(DatabaseBackend::Sqlite, "INSERT INTO bench_users (name, email, age) VALUES (?, ?, ?)", vec![record.name.clone().into(), record.email.clone().into(), record.age.into()])).await {
            Ok(_) => CompetitorCapability::Ok(()),
            Err(e) => CompetitorCapability::Error(format!("insert: {}", e)),
        }
    }

    async fn find_one(&mut self, id: i64) -> CompetitorCapability<Option<BenchRecord>> {
        use sea_orm::{DatabaseBackend, Statement};
        let db = match &self.db { Some(d) => d, None => return CompetitorCapability::Error("not initialized".to_string()) };
        match db.query_one(Statement::from_sql_and_values(DatabaseBackend::Sqlite, "SELECT id, name, email, age FROM bench_users WHERE id = ?", vec![id.into()])).await {
            Ok(Some(row)) => {
                let id: i64 = row.try_get_by_index(0).unwrap_or(0);
                let name: String = row.try_get_by_index(1).unwrap_or_default();
                let email: String = row.try_get_by_index(2).unwrap_or_default();
                let age: i32 = row.try_get_by_index(3).unwrap_or(0);
                CompetitorCapability::Ok(Some(BenchRecord { id, name, email, age }))
            }
            Ok(None) => CompetitorCapability::Ok(None),
            Err(e) => CompetitorCapability::Error(format!("find: {}", e)),
        }
    }

    async fn update_one(&mut self, id: i64, name: &str) -> CompetitorCapability<()> {
        use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};
        let db = match &self.db { Some(d) => d, None => return CompetitorCapability::Error("not initialized".to_string()) };
        match db.execute(Statement::from_sql_and_values(DatabaseBackend::Sqlite, "UPDATE bench_users SET name = ? WHERE id = ?", vec![name.into(), id.into()])).await {
            Ok(_) => CompetitorCapability::Ok(()),
            Err(e) => CompetitorCapability::Error(format!("update: {}", e)),
        }
    }

    async fn delete_one(&mut self, id: i64) -> CompetitorCapability<()> {
        use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};
        let db = match &self.db { Some(d) => d, None => return CompetitorCapability::Error("not initialized".to_string()) };
        match db.execute(Statement::from_sql_and_values(DatabaseBackend::Sqlite, "DELETE FROM bench_users WHERE id = ?", vec![id.into()])).await {
            Ok(_) => CompetitorCapability::Ok(()),
            Err(e) => CompetitorCapability::Error(format!("delete: {}", e)),
        }
    }

    async fn insert_batch(&mut self, records: &[BenchRecord]) -> CompetitorCapability<usize> {
        use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};
        let db = match &self.db { Some(d) => d, None => return CompetitorCapability::Error("not initialized".to_string()) };
        let mut count = 0usize;
        for rec in records {
            match db.execute(Statement::from_sql_and_values(DatabaseBackend::Sqlite, "INSERT INTO bench_users (name, email, age) VALUES (?, ?, ?)", vec![rec.name.clone().into(), rec.email.clone().into(), rec.age.into()])).await {
                Ok(_) => count += 1,
                Err(e) => return CompetitorCapability::Error(format!("batch insert: {}", e)),
            }
        }
        CompetitorCapability::Ok(count)
    }

    async fn find_batch(&mut self, ids: &[i64]) -> CompetitorCapability<Vec<BenchRecord>> {
        use sea_orm::{DatabaseBackend, Statement};
        let db = match &self.db { Some(d) => d, None => return CompetitorCapability::Error("not initialized".to_string()) };
        let mut results = Vec::with_capacity(ids.len());
        for &id in ids {
            match db.query_one(Statement::from_sql_and_values(DatabaseBackend::Sqlite, "SELECT id, name, email, age FROM bench_users WHERE id = ?", vec![id.into()])).await {
                Ok(Some(row)) => {
                    let id: i64 = row.try_get_by_index(0).unwrap_or(0);
                    let name: String = row.try_get_by_index(1).unwrap_or_default();
                    let email: String = row.try_get_by_index(2).unwrap_or_default();
                    let age: i32 = row.try_get_by_index(3).unwrap_or(0);
                    results.push(BenchRecord { id, name, email, age });
                }
                Ok(None) => {}
                Err(e) => return CompetitorCapability::Error(format!("batch find: {}", e)),
            }
        }
        CompetitorCapability::Ok(results)
    }

    async fn find_with_has_one(&mut self, id: i64) -> CompetitorCapability<Vec<BenchRecord>> {
        use sea_orm::{DatabaseBackend, Statement};
        let db = match &self.db { Some(d) => d, None => return CompetitorCapability::Error("not initialized".to_string()) };
        match db.query_all(Statement::from_sql_and_values(DatabaseBackend::Sqlite, "SELECT u.id, u.name, u.email, u.age FROM bench_users u JOIN bench_profiles p ON p.user_id = u.id WHERE u.id = ?", vec![id.into()])).await {
            Ok(rows) => CompetitorCapability::Ok(rows.iter().map(|row| {
                let id: i64 = row.try_get_by_index(0).unwrap_or(0);
                let name: String = row.try_get_by_index(1).unwrap_or_default();
                let email: String = row.try_get_by_index(2).unwrap_or_default();
                let age: i32 = row.try_get_by_index(3).unwrap_or(0);
                BenchRecord { id, name, email, age }
            }).collect()),
            Err(e) => CompetitorCapability::Error(format!("has_one: {}", e)),
        }
    }

    async fn find_with_has_many(&mut self, id: i64) -> CompetitorCapability<Vec<BenchRecord>> {
        use sea_orm::{DatabaseBackend, Statement};
        let db = match &self.db { Some(d) => d, None => return CompetitorCapability::Error("not initialized".to_string()) };
        match db.query_all(Statement::from_sql_and_values(DatabaseBackend::Sqlite, "SELECT id, title FROM bench_posts WHERE user_id = ?", vec![id.into()])).await {
            Ok(rows) => CompetitorCapability::Ok(rows.iter().map(|row| {
                let id: i64 = row.try_get_by_index(0).unwrap_or(0);
                let title: String = row.try_get_by_index(1).unwrap_or_default();
                BenchRecord { id, name: title, email: String::new(), age: 0 }
            }).collect()),
            Err(e) => CompetitorCapability::Error(format!("has_many: {}", e)),
        }
    }

    async fn find_with_many_to_many(&mut self, id: i64) -> CompetitorCapability<Vec<BenchRecord>> {
        use sea_orm::{DatabaseBackend, Statement};
        let db = match &self.db { Some(d) => d, None => return CompetitorCapability::Error("not initialized".to_string()) };
        match db.query_all(Statement::from_sql_and_values(DatabaseBackend::Sqlite, "SELECT t.id, t.name FROM bench_tags t JOIN bench_post_tags pt ON pt.tag_id = t.id WHERE pt.post_id = ?", vec![id.into()])).await {
            Ok(rows) => CompetitorCapability::Ok(rows.iter().map(|row| {
                let id: i64 = row.try_get_by_index(0).unwrap_or(0);
                let name: String = row.try_get_by_index(1).unwrap_or_default();
                BenchRecord { id, name, email: String::new(), age: 0 }
            }).collect()),
            Err(e) => CompetitorCapability::Error(format!("m2m: {}", e)),
        }
    }

    async fn transaction_commit(&mut self) -> CompetitorCapability<()> {
        use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};
        let db = match &self.db { Some(d) => d, None => return CompetitorCapability::Error("not initialized".to_string()) };
        db.execute(Statement::from_sql_and_values(DatabaseBackend::Sqlite, "BEGIN", vec![])).await.ok();
        db.execute(Statement::from_sql_and_values(DatabaseBackend::Sqlite, "INSERT INTO bench_users (name, email, age) VALUES (?, ?, ?)", vec!["tx_user".into(), "tx@test.com".into(), 25.into()])).await.ok();
        match db.execute(Statement::from_sql_and_values(DatabaseBackend::Sqlite, "COMMIT", vec![])).await {
            Ok(_) => CompetitorCapability::Ok(()),
            Err(e) => CompetitorCapability::Error(format!("commit: {}", e)),
        }
    }

    async fn transaction_rollback(&mut self) -> CompetitorCapability<()> {
        use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};
        let db = match &self.db { Some(d) => d, None => return CompetitorCapability::Error("not initialized".to_string()) };
        db.execute(Statement::from_sql_and_values(DatabaseBackend::Sqlite, "BEGIN", vec![])).await.ok();
        db.execute(Statement::from_sql_and_values(DatabaseBackend::Sqlite, "INSERT INTO bench_users (name, email, age) VALUES (?, ?, ?)", vec!["rb_user".into(), "rb@test.com".into(), 30.into()])).await.ok();
        match db.execute(Statement::from_sql_and_values(DatabaseBackend::Sqlite, "ROLLBACK", vec![])).await {
            Ok(_) => CompetitorCapability::Ok(()),
            Err(e) => CompetitorCapability::Error(format!("rollback: {}", e)),
        }
    }

    async fn nested_transaction(&mut self) -> CompetitorCapability<()> {
        use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};
        let db = match &self.db { Some(d) => d, None => return CompetitorCapability::Error("not initialized".to_string()) };
        db.execute(Statement::from_sql_and_values(DatabaseBackend::Sqlite, "BEGIN", vec![])).await.ok();
        db.execute(Statement::from_sql_and_values(DatabaseBackend::Sqlite, "SAVEPOINT sp1", vec![])).await.ok();
        db.execute(Statement::from_sql_and_values(DatabaseBackend::Sqlite, "INSERT INTO bench_users (name, email, age) VALUES (?, ?, ?)", vec!["sp_user".into(), "sp@test.com".into(), 35.into()])).await.ok();
        db.execute(Statement::from_sql_and_values(DatabaseBackend::Sqlite, "RELEASE SAVEPOINT sp1", vec![])).await.ok();
        match db.execute(Statement::from_sql_and_values(DatabaseBackend::Sqlite, "COMMIT", vec![])).await {
            Ok(_) => CompetitorCapability::Ok(()),
            Err(e) => CompetitorCapability::Error(format!("commit: {}", e)),
        }
    }

    async fn pool_acquire(&mut self) -> CompetitorCapability<()> {
        let db = match &self.db { Some(d) => d, None => return CompetitorCapability::Error("not initialized".to_string()) };
        match db.execute(sea_orm::Statement::from_sql_and_values(sea_orm::DatabaseBackend::Sqlite, "SELECT 1", vec![])).await {
            Ok(_) => CompetitorCapability::Ok(()),
            Err(e) => CompetitorCapability::Error(format!("acquire: {}", e)),
        }
    }

    async fn paginate_offset(&mut self, offset: usize, limit: usize) -> CompetitorCapability<Vec<BenchRecord>> {
        use sea_orm::{DatabaseBackend, Statement};
        let db = match &self.db { Some(d) => d, None => return CompetitorCapability::Error("not initialized".to_string()) };
        match db.query_all(Statement::from_sql_and_values(DatabaseBackend::Sqlite, "SELECT id, name, email, age FROM bench_users ORDER BY id LIMIT ? OFFSET ?", vec![(limit as i64).into(), (offset as i64).into()])).await {
            Ok(rows) => CompetitorCapability::Ok(rows.iter().map(|row| {
                let id: i64 = row.try_get_by_index(0).unwrap_or(0);
                let name: String = row.try_get_by_index(1).unwrap_or_default();
                let email: String = row.try_get_by_index(2).unwrap_or_default();
                let age: i32 = row.try_get_by_index(3).unwrap_or(0);
                BenchRecord { id, name, email, age }
            }).collect()),
            Err(e) => CompetitorCapability::Error(format!("paginate: {}", e)),
        }
    }

    async fn paginate_cursor(&mut self, last_id: i64, limit: usize) -> CompetitorCapability<Vec<BenchRecord>> {
        use sea_orm::{DatabaseBackend, Statement};
        let db = match &self.db { Some(d) => d, None => return CompetitorCapability::Error("not initialized".to_string()) };
        match db.query_all(Statement::from_sql_and_values(DatabaseBackend::Sqlite, "SELECT id, name, email, age FROM bench_users WHERE id > ? ORDER BY id LIMIT ?", vec![last_id.into(), (limit as i64).into()])).await {
            Ok(rows) => CompetitorCapability::Ok(rows.iter().map(|row| {
                let id: i64 = row.try_get_by_index(0).unwrap_or(0);
                let name: String = row.try_get_by_index(1).unwrap_or_default();
                let email: String = row.try_get_by_index(2).unwrap_or_default();
                let age: i32 = row.try_get_by_index(3).unwrap_or(0);
                BenchRecord { id, name, email, age }
            }).collect()),
            Err(e) => CompetitorCapability::Error(format!("cursor: {}", e)),
        }
    }
}

/// 创建全部四竞品适配器
pub fn create_all_adapters() -> Vec<Box<dyn CompetitorAdapter>> {
    vec![
        Box::new(SzOrmAdapter::new()),
        Box::new(SqlxAdapter::new()),
        Box::new(DieselAdapter::new()),
        Box::new(SeaOrmAdapter::new()),
    ]
}
