//! 共享测试工具：模拟连接、连接工厂、数据库状态
//!
//! 供 fuzz/stress/jepsen/soak 集成测试使用

#![allow(dead_code)]

pub mod soak;

use async_trait::async_trait;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use sz_orm_core::DbError;
use sz_orm_core::Value;
use sz_orm_core::{Connection, ConnectionFactory};
use tokio::sync::Mutex;

/// 简单的内存数据库状态，用于模拟真实数据库行为
/// 支持基本的 INSERT/SELECT/UPDATE/DELETE 语义
#[derive(Debug, Default)]
pub struct InMemoryDb {
    /// 表名 -> 行列表（每行是字段名到值的映射）
    tables: std::collections::HashMap<String, Vec<std::collections::HashMap<String, Value>>>,
}

impl InMemoryDb {
    pub fn new() -> Self {
        Self {
            tables: std::collections::HashMap::new(),
        }
    }

    /// 创建表
    pub fn create_table(&mut self, name: &str) {
        self.tables.insert(name.to_string(), Vec::new());
    }

    /// 插入行
    pub fn insert(&mut self, table: &str, row: std::collections::HashMap<String, Value>) {
        self.tables.entry(table.to_string()).or_default().push(row);
    }

    /// 查询所有行
    pub fn select_all(&self, table: &str) -> &[std::collections::HashMap<String, Value>] {
        self.tables.get(table).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// 按字段等于条件删除
    pub fn delete_where(&mut self, table: &str, field: &str, value: &Value) -> u64 {
        if let Some(rows) = self.tables.get_mut(table) {
            let before = rows.len();
            rows.retain(|r| r.get(field) != Some(value));
            (before - rows.len()) as u64
        } else {
            0
        }
    }

    /// 更新字段等于条件的行
    pub fn update_where(
        &mut self,
        table: &str,
        cond_field: &str,
        cond_value: &Value,
        set_field: &str,
        set_value: Value,
    ) -> u64 {
        if let Some(rows) = self.tables.get_mut(table) {
            let mut count = 0u64;
            for row in rows.iter_mut() {
                if row.get(cond_field) == Some(cond_value) {
                    row.insert(set_field.to_string(), set_value.clone());
                    count += 1;
                }
            }
            count
        } else {
            0
        }
    }

    /// 获取行数
    pub fn count(&self, table: &str) -> usize {
        self.tables.get(table).map(|v| v.len()).unwrap_or(0)
    }

    /// 获取某个字段的总和（i64）
    pub fn sum_i64(&self, table: &str, field: &str) -> i64 {
        self.tables
            .get(table)
            .map(|rows| {
                rows.iter()
                    .filter_map(|r| r.get(field).and_then(|v| v.as_i64()))
                    .sum()
            })
            .unwrap_or(0)
    }
}

/// 模拟连接：持有共享数据库状态的引用
pub struct MockConnection {
    db: Arc<Mutex<InMemoryDb>>,
    connected: bool,
    /// 是否处于事务中
    in_transaction: bool,
    /// 事务期间的待提交更改（简化版：直接操作共享状态）
    pub executed_sql: Vec<String>,
}

impl MockConnection {
    pub fn new(db: Arc<Mutex<InMemoryDb>>) -> Self {
        Self {
            db,
            connected: true,
            in_transaction: false,
            executed_sql: Vec::new(),
        }
    }
}

impl Connection for MockConnection {
    fn execute<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<u64, DbError>> + Send + 'a>> {
        Box::pin(async move {
            self.executed_sql.push(sql.to_string());
            // 简化：只解析特定格式的 SQL 用于测试
            // INSERT INTO table (k,v) VALUES (...)
            // DELETE FROM table WHERE k=v
            // UPDATE table SET v=... WHERE k=...
            Ok(1)
        })
    }

    fn query<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<Vec<std::collections::HashMap<String, Value>>, DbError>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            self.executed_sql.push(sql.to_string());
            Ok(vec![])
        })
    }

    fn begin_transaction<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move {
            self.in_transaction = true;
            Ok(())
        })
    }

    fn commit<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move {
            self.in_transaction = false;
            Ok(())
        })
    }

    fn rollback<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move {
            self.in_transaction = false;
            Ok(())
        })
    }

    fn is_connected(&self) -> bool {
        self.connected
    }

    fn ping<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move { self.connected })
    }

    fn close<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move {
            self.connected = false;
            Ok(())
        })
    }
}

/// 故障注入连接：可以配置在指定操作时失败
pub struct FaultyConnection {
    db: Arc<Mutex<InMemoryDb>>,
    connected: bool,
    /// 在第 N 次 execute 时失败（用于故障注入）
    pub fail_on_execute_n: Option<u32>,
    execute_count: u32,
    /// 在 commit 时失败
    pub fail_on_commit: bool,
    /// 在 rollback 时失败
    pub fail_on_rollback: bool,
}

impl FaultyConnection {
    pub fn new(db: Arc<Mutex<InMemoryDb>>) -> Self {
        Self {
            db,
            connected: true,
            fail_on_execute_n: None,
            execute_count: 0,
            fail_on_commit: false,
            fail_on_rollback: false,
        }
    }
}

impl Connection for FaultyConnection {
    fn execute<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<u64, DbError>> + Send + 'a>> {
        Box::pin(async move {
            self.execute_count += 1;
            if let Some(n) = self.fail_on_execute_n {
                if self.execute_count >= n {
                    self.connected = false;
                    return Err(DbError::ConnectionError("injected fault".to_string()));
                }
            }
            let _ = sql;
            Ok(1)
        })
    }

    fn query<'a>(
        &'a mut self,
        _sql: &'a str,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<Vec<std::collections::HashMap<String, Value>>, DbError>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async move { Ok(vec![]) })
    }

    fn begin_transaction<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(()) })
    }

    fn commit<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move {
            if self.fail_on_commit {
                return Err(DbError::Internal("injected commit fault".to_string()));
            }
            Ok(())
        })
    }

    fn rollback<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move {
            if self.fail_on_rollback {
                return Err(DbError::Internal("injected rollback fault".to_string()));
            }
            Ok(())
        })
    }

    fn is_connected(&self) -> bool {
        self.connected
    }

    fn ping<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move { self.connected })
    }

    fn close<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move {
            self.connected = false;
            Ok(())
        })
    }
}

/// 模拟连接工厂
pub struct MockConnectionFactory {
    db: Arc<Mutex<InMemoryDb>>,
}

impl MockConnectionFactory {
    pub fn new(db: Arc<Mutex<InMemoryDb>>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl ConnectionFactory for MockConnectionFactory {
    async fn create(&self) -> Result<Box<dyn Connection>, DbError> {
        Ok(Box::new(MockConnection::new(self.db.clone())))
    }
}

/// 故障注入连接工厂
pub struct FaultyConnectionFactory {
    db: Arc<Mutex<InMemoryDb>>,
    /// 创建的第 N 个连接会是故障连接
    pub faulty_nth: u32,
    counter: Mutex<u32>,
}

impl FaultyConnectionFactory {
    pub fn new(db: Arc<Mutex<InMemoryDb>>, faulty_nth: u32) -> Self {
        Self {
            db,
            faulty_nth,
            counter: Mutex::new(0),
        }
    }
}

#[async_trait]
impl ConnectionFactory for FaultyConnectionFactory {
    async fn create(&self) -> Result<Box<dyn Connection>, DbError> {
        let mut count = self.counter.lock().await;
        *count += 1;
        if *count == self.faulty_nth {
            return Err(DbError::ConnectionError(
                "injected factory fault".to_string(),
            ));
        }
        Ok(Box::new(MockConnection::new(self.db.clone())))
    }
}

/// 简单的伪随机数生成器（不依赖外部库）
/// 使用 xorshift64 算法
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 0xdeadbeef } else { seed },
        }
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    pub fn next_u32(&mut self) -> u32 {
        self.next_u64() as u32
    }

    pub fn next_usize(&mut self, max: usize) -> usize {
        if max == 0 {
            return 0;
        }
        (self.next_u64() as usize) % max
    }

    pub fn next_i64(&mut self) -> i64 {
        self.next_u64() as i64
    }

    pub fn next_bool(&mut self) -> bool {
        self.next_u64().is_multiple_of(2)
    }

    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() as f64) / (u64::MAX as f64)
    }

    /// 生成随机字节串
    pub fn next_bytes(&mut self, len: usize) -> Vec<u8> {
        (0..len).map(|_| self.next_u64() as u8).collect()
    }

    /// 生成随机字符串（可包含特殊字符用于测试转义）
    pub fn next_string(&mut self, len: usize) -> String {
        let chars = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!@#$%^&*()'\"\\;-- \n\r\t\x00";
        (0..len)
            .map(|_| chars.as_bytes()[self.next_usize(chars.len())] as char)
            .collect()
    }
}
