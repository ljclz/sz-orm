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

use std::collections::HashMap;

/// 统一基准记录结构
#[derive(Debug, Clone)]
pub struct BenchRecord {
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
pub enum CompetitorCapability<T> {
    /// 竞品不支持该维度
    Unsupported(String),
    /// 执行出错
    Error(String),
    /// 成功结果
    Ok(T),
}

impl<T> CompetitorCapability<T> {
    pub fn is_unsupported(&self) -> bool {
        matches!(self, CompetitorCapability::Unsupported(_))
    }

    pub fn is_error(&self) -> bool {
        matches!(self, CompetitorCapability::Error(_))
    }

    pub fn is_ok(&self) -> bool {
        matches!(self, CompetitorCapability::Ok(_))
    }
}

/// 基准测试统一接口 trait
#[async_trait::async_trait]
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