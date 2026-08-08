//! 共享测试工具：模拟连接、连接工厂、数据库状态
//!
//! 供 fuzz/stress/jepsen/soak 集成测试使用

#![allow(dead_code)]

pub mod equivalence;
pub mod rusqlite_adapter;
pub mod schema_builder;
pub mod soak;
pub mod sqlx_mysql_adapter;
pub mod sqlx_pg_adapter;

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
///
/// P1-3：实现 Clone 以支持事务快照（begin_transaction 时克隆当前状态，
/// rollback 时恢复快照，commit 时丢弃快照）
#[derive(Debug, Default, Clone)]
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

    /// P1-3：按字段等于条件查询行（返回克隆，避免借用冲突）
    ///
    /// 用于端到端测试验证事务回滚后的数据状态。
    pub fn select_where(
        &self,
        table: &str,
        field: &str,
        value: &Value,
    ) -> Vec<std::collections::HashMap<String, Value>> {
        self.tables
            .get(table)
            .map(|rows| {
                rows.iter()
                    .filter(|r| r.get(field) == Some(value))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// P1-3：按字段等于条件查询第一个匹配行
    pub fn find_where(
        &self,
        table: &str,
        field: &str,
        value: &Value,
    ) -> Option<std::collections::HashMap<String, Value>> {
        self.tables
            .get(table)
            .and_then(|rows| rows.iter().find(|r| r.get(field) == Some(value)).cloned())
    }

    /// P1-3：获取表的完整快照（克隆所有行），用于测试断言
    pub fn snapshot(&self, table: &str) -> Vec<std::collections::HashMap<String, Value>> {
        self.tables.get(table).cloned().unwrap_or_default()
    }

    /// P1-3：直接替换表的所有行（用于测试初始化或快照恢复）
    pub fn replace_table(
        &mut self,
        table: &str,
        rows: Vec<std::collections::HashMap<String, Value>>,
    ) {
        self.tables.insert(table.to_string(), rows);
    }

    /// P1-3：深克隆整个数据库状态（事务快照用）
    ///
    /// 由于已实现 Clone，可直接调用 `.clone()`，此方法为语义清晰提供别名。
    pub fn deep_clone(&self) -> Self {
        self.clone()
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

// ==================== P1-3：事务感知连接 ====================

/// 支持事务语义的内存连接（P1-3 端到端回滚验证基础设施）
///
/// 与 `MockConnection` 的区别：
/// - `MockConnection` 的 `execute` 只记录 SQL，不实际操作 `InMemoryDb`
/// - `TransactionalConnection` 会解析简单 SQL 并实际读写 `InMemoryDb`
/// - 支持 `begin_transaction` / `commit` / `rollback` 的真实事务语义：
///   - `begin_transaction`：克隆当前 db 状态作为快照
///   - `commit`：丢弃快照（事务中的修改持久化）
///   - `rollback`：恢复快照（事务中的修改被撤销）
/// - 支持 `SAVEPOINT` / `ROLLBACK TO SAVEPOINT` / `RELEASE SAVEPOINT`
///   - `SAVEPOINT name`：在快照栈上 push 当前 db 状态
///   - `ROLLBACK TO SAVEPOINT name`：弹出并恢复到对应快照
///   - `RELEASE SAVEPOINT name`：丢弃对应快照
///
/// 支持的 SQL 格式（简化版，仅用于测试）：
/// - `INSERT INTO <table> (<col1>, <col2>, ...) VALUES (<val1>, <val2>, ...)`
/// - `DELETE FROM <table> WHERE <col>=<val>`
/// - `UPDATE <table> SET <col>=<val> WHERE <col>=<val>`
/// - `SAVEPOINT <name>`
/// - `ROLLBACK TO SAVEPOINT <name>`
/// - `RELEASE SAVEPOINT <name>`
///
/// 值格式：整数（`123`）、字符串（`'abc'`）、NULL（`NULL`）
pub struct TransactionalConnection {
    db: Arc<Mutex<InMemoryDb>>,
    /// 事务快照栈
    ///
    /// - `begin_transaction` 时 push 当前 db 状态（索引 0）
    /// - `SAVEPOINT` 时 push 当前 db 状态（索引 1+）
    /// - `commit` 时清空栈（保留所有修改）
    /// - `rollback` 时弹出索引 0 的快照并恢复（撤销所有修改）
    /// - `ROLLBACK TO SAVEPOINT name` 时弹出对应索引的快照并恢复
    /// - `RELEASE SAVEPOINT name` 时丢弃对应索引的快照（不恢复）
    snapshots: Vec<InMemoryDb>,
    /// 保存点名称到 snapshots 栈索引的映射
    savepoint_names: std::collections::HashMap<String, usize>,
    connected: bool,
    /// 记录所有执行过的 SQL（用于测试断言）
    pub executed_sql: Vec<String>,
}

impl TransactionalConnection {
    pub fn new(db: Arc<Mutex<InMemoryDb>>) -> Self {
        Self {
            db,
            snapshots: Vec::new(),
            savepoint_names: std::collections::HashMap::new(),
            connected: true,
            executed_sql: Vec::new(),
        }
    }

    /// 获取 db 的共享句柄（用于测试外部直接查询状态）
    pub fn db_handle(&self) -> Arc<Mutex<InMemoryDb>> {
        self.db.clone()
    }

    /// 解析 SQL 值字面量为 `Value`
    ///
    /// 支持：整数、字符串（单引号包裹）、NULL
    fn parse_value(literal: &str) -> Value {
        let trimmed = literal.trim();
        if trimmed.eq_ignore_ascii_case("NULL") {
            return Value::Null;
        }
        // 字符串字面量：'xxx'
        if trimmed.starts_with('\'') && trimmed.ends_with('\'') && trimmed.len() >= 2 {
            let inner = &trimmed[1..trimmed.len() - 1];
            return Value::String(inner.to_string());
        }
        // 整数
        if let Ok(i) = trimmed.parse::<i64>() {
            return Value::I64(i);
        }
        // 浮点数
        if let Ok(f) = trimmed.parse::<f64>() {
            return Value::F64(f);
        }
        // 布尔
        if trimmed.eq_ignore_ascii_case("true") {
            return Value::Bool(true);
        }
        if trimmed.eq_ignore_ascii_case("false") {
            return Value::Bool(false);
        }
        // 默认按字符串处理
        Value::String(trimmed.to_string())
    }

    /// 拆分 CSV 列名/值列表（支持带括号）
    fn split_csv(s: &str) -> Vec<String> {
        s.split(',').map(|p| p.trim().to_string()).collect()
    }

    /// 应用 INSERT 语句到 db
    fn apply_insert(db: &mut InMemoryDb, sql: &str) -> u64 {
        // 格式：INSERT INTO <table> (<cols>) VALUES (<vals>)
        let upper = sql.to_uppercase();
        let Some(values_pos) = upper.find("VALUES") else {
            return 0;
        };
        let Some(open_paren) = sql.find('(') else {
            return 0;
        };
        let Some(close_paren) = sql[open_paren..].find(')') else {
            return 0;
        };
        let cols_str = &sql[open_paren + 1..open_paren + close_paren];
        // VALUES 后的括号
        let vals_start = values_pos + 6; // "VALUES".len()
        let Some(vals_open) = sql[vals_start..].find('(') else {
            return 0;
        };
        let vals_open_abs = vals_start + vals_open;
        let Some(vals_close) = sql[vals_open_abs..].find(')') else {
            return 0;
        };
        let vals_close_abs = vals_open_abs + vals_close;
        let vals_str = &sql[vals_open_abs + 1..vals_close_abs];

        // 解析表名：INSERT INTO <table> (
        let after_into = sql.find("INTO").map(|p| p + 4).unwrap_or(0);
        let table_part = sql[after_into..open_paren].trim();
        let table = table_part.trim_matches(|c: char| c == '`' || c == '"');

        let cols = Self::split_csv(cols_str);
        let vals = Self::split_csv(vals_str);

        let mut row = std::collections::HashMap::new();
        for (col, val) in cols.iter().zip(vals.iter()) {
            row.insert(col.clone(), Self::parse_value(val));
        }
        db.insert(table, row);
        1
    }

    /// 应用 DELETE 语句到 db
    fn apply_delete(db: &mut InMemoryDb, sql: &str) -> u64 {
        // 格式：DELETE FROM <table> WHERE <col>=<val>
        let upper = sql.to_uppercase();
        let Some(from_pos) = upper.find("FROM") else {
            return 0;
        };
        let where_pos = upper.find("WHERE");
        let (table_part, cond_part) = if let Some(wp) = where_pos {
            (sql[from_pos + 4..wp].trim(), sql[wp + 5..].trim())
        } else {
            (sql[from_pos + 4..].trim(), "")
        };
        let table = table_part.trim_matches(|c: char| c == '`' || c == '"');

        if cond_part.is_empty() {
            // 无条件删除全部
            let count = db.count(table) as u64;
            let rows: Vec<_> = db.snapshot(table);
            db.replace_table(table, Vec::new());
            let _ = rows;
            return count;
        }

        // 解析 <col>=<val>
        if let Some(eq_pos) = cond_part.find('=') {
            let col = cond_part[..eq_pos]
                .trim()
                .trim_matches(|c: char| c == '`' || c == '"');
            let val = Self::parse_value(&cond_part[eq_pos + 1..]);
            return db.delete_where(table, col, &val);
        }
        0
    }

    /// 应用 UPDATE 语句到 db
    fn apply_update(db: &mut InMemoryDb, sql: &str) -> u64 {
        // 格式：UPDATE <table> SET <col>=<val> WHERE <col>=<val>
        let upper = sql.to_uppercase();
        let Some(set_pos) = upper.find("SET") else {
            return 0;
        };
        let where_pos = upper.find("WHERE");
        let table_part = sql[7..set_pos].trim(); // "UPDATE ".len() == 7
        let table = table_part.trim_matches(|c: char| c == '`' || c == '"');

        let (set_part, cond_part) = if let Some(wp) = where_pos {
            (sql[set_pos + 3..wp].trim(), sql[wp + 5..].trim())
        } else {
            (sql[set_pos + 3..].trim(), "")
        };

        // 解析 SET <col>=<val>
        let Some(set_eq) = set_part.find('=') else {
            return 0;
        };
        let set_col = set_part[..set_eq]
            .trim()
            .trim_matches(|c: char| c == '`' || c == '"');
        let set_val = Self::parse_value(&set_part[set_eq + 1..]);

        if cond_part.is_empty() {
            // 无条件更新全部
            let rows = db.snapshot(table);
            let count = rows.len() as u64;
            for mut row in rows {
                row.insert(set_col.to_string(), set_val.clone());
                db.insert(table, row);
            }
            return count;
        }

        // 解析 WHERE <col>=<val>
        if let Some(eq_pos) = cond_part.find('=') {
            let cond_col = cond_part[..eq_pos]
                .trim()
                .trim_matches(|c: char| c == '`' || c == '"');
            let cond_val = Self::parse_value(&cond_part[eq_pos + 1..]);
            return db.update_where(table, cond_col, &cond_val, set_col, set_val);
        }
        0
    }

    /// 处理 SAVEPOINT 语句
    async fn handle_savepoint(&mut self, name: &str) -> u64 {
        let db = self.db.lock().await;
        self.snapshots.push(db.clone());
        self.savepoint_names
            .insert(name.to_string(), self.snapshots.len() - 1);
        0
    }

    /// 处理 ROLLBACK TO SAVEPOINT 语句
    async fn handle_rollback_to_savepoint(&mut self, name: &str) -> u64 {
        if let Some(&idx) = self.savepoint_names.get(name) {
            if idx < self.snapshots.len() {
                let snapshot = self.snapshots[idx].clone();
                let mut db = self.db.lock().await;
                *db = snapshot;
                // 弹出 idx 及之后的快照
                self.snapshots.truncate(idx);
                // 清理 idx 及之后的保存点名称
                self.savepoint_names.retain(|_, v| *v < idx);
            }
        }
        0
    }

    /// 处理 RELEASE SAVEPOINT 语句
    async fn handle_release_savepoint(&mut self, name: &str) -> u64 {
        if let Some(idx) = self.savepoint_names.remove(name) {
            // 丢弃对应快照（不恢复）
            if idx < self.snapshots.len() {
                self.snapshots.remove(idx);
                // 调整后续保存点的索引
                for v in self.savepoint_names.values_mut() {
                    if *v > idx {
                        *v -= 1;
                    }
                }
            }
        }
        0
    }
}

impl Connection for TransactionalConnection {
    fn execute<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<u64, DbError>> + Send + 'a>> {
        Box::pin(async move {
            self.executed_sql.push(sql.to_string());
            let upper = sql.to_uppercase();

            // 事务控制语句
            if upper.starts_with("SAVEPOINT") {
                let name = sql[9..].trim();
                return Ok(Self::handle_savepoint(self, name).await);
            }
            if upper.starts_with("ROLLBACK TO SAVEPOINT") {
                let name = sql[21..].trim();
                return Ok(Self::handle_rollback_to_savepoint(self, name).await);
            }
            if upper.starts_with("RELEASE SAVEPOINT") {
                let name = sql[17..].trim();
                return Ok(Self::handle_release_savepoint(self, name).await);
            }

            // DML 语句：解析并应用到 db
            let mut db = self.db.lock().await;
            let affected = if upper.starts_with("INSERT") {
                Self::apply_insert(&mut db, sql)
            } else if upper.starts_with("DELETE") {
                Self::apply_delete(&mut db, sql)
            } else if upper.starts_with("UPDATE") {
                Self::apply_update(&mut db, sql)
            } else {
                // 其他语句（如 SELECT、CREATE TABLE）返回 0
                0
            };
            Ok(affected)
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
            let upper = sql.to_uppercase();
            if !upper.starts_with("SELECT") {
                return Ok(vec![]);
            }

            // 解析 SELECT <cols> FROM <table> [WHERE <col>=<val>]
            let Some(from_pos) = upper.find("FROM") else {
                return Ok(vec![]);
            };
            let where_pos = upper.find("WHERE");

            // 解析列：SELECT <cols> FROM
            let cols_str = sql[6..from_pos].trim(); // "SELECT".len() == 6
            let cols: Vec<String> = if cols_str == "*" {
                Vec::new() // 空表示所有列
            } else {
                cols_str.split(',').map(|s| s.trim().to_string()).collect()
            };

            // 解析表名
            let (table_part, cond_part) = if let Some(wp) = where_pos {
                (sql[from_pos + 4..wp].trim(), sql[wp + 5..].trim())
            } else {
                (sql[from_pos + 4..].trim(), "")
            };
            let table = table_part.trim_matches(|c: char| c == '`' || c == '"');

            let db = self.db.lock().await;
            let all_rows = db.select_all(table);

            // 过滤条件（简化：只支持 <col>=<val>）
            let filtered: Vec<&std::collections::HashMap<String, Value>> = if cond_part.is_empty() {
                all_rows.iter().collect()
            } else if let Some(eq_pos) = cond_part.find('=') {
                let col = cond_part[..eq_pos]
                    .trim()
                    .trim_matches(|c: char| c == '`' || c == '"');
                let val = Self::parse_value(&cond_part[eq_pos + 1..]);
                all_rows
                    .iter()
                    .filter(|r| r.get(col) == Some(&val))
                    .collect()
            } else {
                all_rows.iter().collect()
            };

            // 投影列
            let result: Vec<std::collections::HashMap<String, Value>> = if cols.is_empty() {
                filtered.iter().map(|r| (*r).clone()).collect()
            } else {
                filtered
                    .iter()
                    .map(|r| {
                        let mut row = std::collections::HashMap::new();
                        for col in &cols {
                            let col_clean = col.trim_matches(|c: char| c == '`' || c == '"');
                            if let Some(v) = r.get(col_clean) {
                                row.insert(col_clean.to_string(), v.clone());
                            }
                        }
                        row
                    })
                    .collect()
            };

            Ok(result)
        })
    }

    fn begin_transaction<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move {
            let db = self.db.lock().await;
            self.snapshots.clear();
            self.savepoint_names.clear();
            self.snapshots.push(db.clone());
            Ok(())
        })
    }

    fn commit<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move {
            // 提交：丢弃所有快照（事务中的修改持久化）
            self.snapshots.clear();
            self.savepoint_names.clear();
            Ok(())
        })
    }

    fn rollback<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move {
            // 回滚：恢复到事务开始时的快照
            if let Some(snapshot) = self.snapshots.first().cloned() {
                let mut db = self.db.lock().await;
                *db = snapshot;
            }
            self.snapshots.clear();
            self.savepoint_names.clear();
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

/// P1-3：TransactionalConnection 的连接工厂
pub struct TransactionalConnectionFactory {
    db: Arc<Mutex<InMemoryDb>>,
}

impl TransactionalConnectionFactory {
    pub fn new(db: Arc<Mutex<InMemoryDb>>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl ConnectionFactory for TransactionalConnectionFactory {
    async fn create(&self) -> Result<Box<dyn Connection>, DbError> {
        Ok(Box::new(TransactionalConnection::new(self.db.clone())))
    }
}
