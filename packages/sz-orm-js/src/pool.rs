//! Pool — 连接池（真实 SQLite 后端）
//!
//! v10.12.0 修复：由「返回伪造数据」的桩实现改为真实连接。
//! `connect(dsn)` 建立真实 SQLite 连接池；查询/执行/表检查/计数/事务
//! 均通过 sz-orm-core + sz-orm-sqlx 真实执行。

use napi_derive::napi;
use std::sync::{Arc, Mutex};
use sz_orm_core::{DbType, Pool as CorePool, PoolConfigBuilder, PooledConnection, Value};
use sz_orm_sqlx::{SqlitePoolHandle, SqlxSqliteConnectionFactory};

type Result<T> = napi::bindgen_prelude::Result<T>;
use napi::bindgen_prelude::Error;

/// 校验 SQL 标识符（表名/字段名），防标识符注入。
/// 规则：非空；首字符为字母或下划线；其余为字母/数字/下划线。
fn validate_identifier(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let mut chars = name.chars();
    let first = chars.next().unwrap();
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// 将 serde_json::Value 转为 sz-orm-core 的 Value（参数绑定用）
fn json_to_value(v: &serde_json::Value) -> Value {
    match v {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::I64(i)
            } else if let Some(u) = n.as_u64() {
                Value::U64(u)
            } else if let Some(f) = n.as_f64() {
                Value::F64(f)
            } else {
                Value::Null
            }
        }
        serde_json::Value::String(s) => Value::String(s.clone()),
        serde_json::Value::Array(arr) => Value::Array(arr.iter().map(json_to_value).collect()),
        serde_json::Value::Object(obj) => {
            let mut map = std::collections::HashMap::with_capacity(obj.len());
            for (k, v) in obj {
                map.insert(k.clone(), json_to_value(v));
            }
            Value::Object(map)
        }
    }
}

#[napi]
pub struct Pool {
    db_type: DbType,
    max_size: u32,
    min_idle: u32,
    acquire_timeout_secs: u64,
    idle_timeout_secs: u64,
    max_lifetime_secs: u64,
    /// 真实连接池（connect 后 Some）
    inner: Mutex<Option<Arc<CorePool>>>,
    /// 当前活动事务持有的独占连接（async_begin 后 Some，commit/rollback 释放）
    tx_conn: Arc<tokio::sync::Mutex<Option<PooledConnection>>>,
}

#[napi]
impl Pool {
    #[napi(constructor)]
    pub fn new(
        db_type: Option<String>,
        max_size: Option<u32>,
        min_idle: Option<u32>,
        acquire_timeout: Option<i64>,
        idle_timeout: Option<i64>,
        max_lifetime: Option<i64>,
    ) -> Result<Self> {
        let dt = db_type.unwrap_or_else(|| "mysql".to_string());
        let db_type = DbType::from_str(&dt)
            .ok_or_else(|| Error::from_reason(format!("unknown DbType: {}", dt)))?;
        Ok(Self {
            db_type,
            max_size: max_size.unwrap_or(100),
            min_idle: min_idle.unwrap_or(0),
            acquire_timeout_secs: acquire_timeout.unwrap_or(30) as u64,
            idle_timeout_secs: idle_timeout.unwrap_or(600) as u64,
            max_lifetime_secs: max_lifetime.unwrap_or(1800) as u64,
            inner: Mutex::new(None),
            tx_conn: Arc::new(tokio::sync::Mutex::new(None)),
        })
    }

    /// 建立真实连接（SQLite 后端）。
    ///
    /// dsn 示例："sqlite::memory:" 或 "sqlite://path/to/db.sqlite"
    #[napi]
    pub async fn connect(&self, dsn: String) -> Result<()> {
        {
            let guard = self
                .inner
                .lock()
                .map_err(|e| Error::from_reason(format!("lock: {e}")))?;
            if guard.is_some() {
                return Ok(());
            }
        }
        let cfg = PoolConfigBuilder::new()
            .max_size(self.max_size.max(1))
            .min_idle(self.min_idle.min(self.max_size.max(1)))
            .acquire_timeout(self.acquire_timeout_secs)
            .idle_timeout(self.idle_timeout_secs)
            .build()
            .map_err(|e| Error::from_reason(format!("pool config: {e}")))?;

        let handle = SqlitePoolHandle::connect(&dsn)
            .await
            .map_err(|e| Error::from_reason(format!("connect: {e}")))?;
        let factory = Arc::new(SqlxSqliteConnectionFactory::new(Arc::new(handle)));
        let pool =
            CorePool::new(cfg, factory).map_err(|e| Error::from_reason(format!("pool: {e}")))?;

        let mut guard = self
            .inner
            .lock()
            .map_err(|e| Error::from_reason(format!("lock: {e}")))?;
        *guard = Some(Arc::new(pool));
        Ok(())
    }

    #[napi(getter)]
    pub fn db_type(&self) -> String {
        self.db_type.as_str().to_string()
    }

    #[napi(getter)]
    pub fn max_size(&self) -> u32 {
        self.max_size
    }

    #[napi(getter)]
    pub fn min_idle(&self) -> u32 {
        self.min_idle
    }

    #[napi(getter)]
    pub fn acquire_timeout(&self) -> i64 {
        self.acquire_timeout_secs as i64
    }

    #[napi(getter)]
    pub fn idle_timeout(&self) -> i64 {
        self.idle_timeout_secs as i64
    }

    #[napi(getter)]
    pub fn max_lifetime(&self) -> i64 {
        self.max_lifetime_secs as i64
    }

    #[napi(getter)]
    pub fn is_connected(&self) -> bool {
        self.inner.lock().map(|g| g.is_some()).unwrap_or(false)
    }

    #[napi]
    pub fn status(&self) -> String {
        format!(
            "Pool(db={}, max={}, min_idle={}, connected={})",
            self.db_type.as_str(),
            self.max_size,
            self.min_idle,
            self.is_connected()
        )
    }

    /// 健康检查（真实 acquire + ping）
    #[napi]
    pub async fn async_ping(&self) -> Result<bool> {
        let pool = self.pool_ref()?;
        let mut conn = pool
            .acquire()
            .await
            .map_err(|e| Error::from_reason(format!("acquire: {e}")))?;
        Ok(conn.ping().await)
    }

    /// 执行查询，返回 JSON 行数组字符串
    #[napi]
    pub async fn async_query(&self, sql: String) -> Result<String> {
        let pool = self.pool_ref()?;
        let rows = pool
            .query_with_timeout(&sql)
            .await
            .map_err(|e| Error::from_reason(format!("query: {e}")))?;
        serde_json::to_string(&rows).map_err(|e| Error::from_reason(format!("serialize: {e}")))
    }

    /// 执行写语句（INSERT/UPDATE/DELETE），返回影响行数
    #[napi]
    pub async fn async_execute(&self, sql: String) -> Result<u32> {
        let pool = self.pool_ref()?;
        let mut conn = pool
            .acquire()
            .await
            .map_err(|e| Error::from_reason(format!("acquire: {e}")))?;
        let n = conn
            .execute(&sql)
            .await
            .map_err(|e| Error::from_reason(format!("execute: {e}")))?;
        Ok(n as u32)
    }

    /// 关闭连接池（释放全部连接）
    #[napi]
    pub async fn close(&self) {
        let pool = self.inner.lock().map(|mut g| g.take()).unwrap_or(None);
        if let Some(pool) = pool {
            pool.close_all().await;
        }
    }

    /// 获取池指标（JSON 字符串）
    #[napi]
    pub async fn metrics(&self) -> Result<String> {
        let pool = self.pool_ref()?;
        let stats = pool.status().await;
        Ok(format!(
            r#"{{"idle":{},"active":{},"max":{},"min":{},"waiters":{}}}"#,
            stats.idle, stats.active, stats.max, stats.min, stats.waiters
        ))
    }

    /// 查询单行，返回 JSON 字符串（空行返回 "{}"）
    #[napi]
    pub async fn async_query_one(&self, sql: String) -> Result<String> {
        let pool = self.pool_ref()?;
        let rows = pool
            .query_with_timeout(&sql)
            .await
            .map_err(|e| Error::from_reason(format!("query: {e}")))?;
        if rows.is_empty() {
            Ok("{}".to_string())
        } else {
            serde_json::to_string(&rows[0])
                .map_err(|e| Error::from_reason(format!("serialize: {e}")))
        }
    }

    /// 批量执行 SQL，返回影响行数
    #[napi]
    pub async fn async_execute_batch(&self, sql: String) -> Result<u32> {
        self.async_execute(sql).await
    }

    /// 检查表是否存在（SQLite）
    #[napi]
    pub async fn table_exists(&self, table: String) -> Result<bool> {
        let pool = self.pool_ref()?;
        if !validate_identifier(&table) {
            return Err(Error::from_reason("invalid table name"));
        }
        let sql = format!(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='{}'",
            table.replace('\'', "''")
        );
        let rows = pool
            .query_with_timeout(&sql)
            .await
            .map_err(|e| Error::from_reason(format!("query: {e}")))?;
        Ok(!rows.is_empty())
    }

    /// 统计表行数
    #[napi]
    pub async fn async_count(&self, table: String) -> Result<i64> {
        let pool = self.pool_ref()?;
        if !validate_identifier(&table) {
            return Err(Error::from_reason("invalid table name"));
        }
        let sql = format!("SELECT COUNT(*) AS cnt FROM {table}");
        let rows = pool
            .query_with_timeout(&sql)
            .await
            .map_err(|e| Error::from_reason(format!("query: {e}")))?;
        if rows.is_empty() {
            return Err(Error::from_reason("count query returned no rows"));
        }
        match rows[0].get("cnt") {
            Some(Value::I64(n)) => Ok(*n),
            Some(Value::U64(n)) => Ok(*n as i64),
            Some(Value::F64(n)) => Ok(*n as i64),
            _ => Err(Error::from_reason("count returned unexpected type")),
        }
    }

    #[napi]
    pub fn version() -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }

    #[napi]
    pub fn error_message(code: i32) -> String {
        match code {
            0 => "success".to_string(),
            1 => "invalid argument".to_string(),
            2 => "connection failed".to_string(),
            3 => "query failed".to_string(),
            _ => format!("unknown error: {code}"),
        }
    }

    /// 开启事务（真实 BEGIN，独占一条连接直到 commit/rollback）
    #[napi]
    pub async fn async_begin(&self) -> Result<bool> {
        let pool = self.pool_ref()?;
        let mut tx = self.tx_conn.lock().await;
        if tx.is_some() {
            return Err(Error::from_reason("transaction already active"));
        }
        let mut conn = pool
            .acquire()
            .await
            .map_err(|e| Error::from_reason(format!("acquire: {e}")))?;
        conn.execute("BEGIN")
            .await
            .map_err(|e| Error::from_reason(format!("begin: {e}")))?;
        *tx = Some(conn);
        Ok(true)
    }

    /// 提交事务（真实 COMMIT，释放独占连接）
    #[napi]
    pub async fn async_commit(&self) -> Result<bool> {
        let mut tx = self.tx_conn.lock().await;
        let mut conn = tx
            .take()
            .ok_or_else(|| Error::from_reason("no active transaction"))?;
        conn.execute("COMMIT")
            .await
            .map_err(|e| Error::from_reason(format!("commit: {e}")))?;
        Ok(true)
    }

    /// 回滚事务（真实 ROLLBACK，释放独占连接）
    #[napi]
    pub async fn async_rollback(&self) -> Result<bool> {
        let mut tx = self.tx_conn.lock().await;
        let mut conn = tx
            .take()
            .ok_or_else(|| Error::from_reason("no active transaction"))?;
        conn.execute("ROLLBACK")
            .await
            .map_err(|e| Error::from_reason(format!("rollback: {e}")))?;
        Ok(true)
    }

    /// 在活动事务内执行 SQL
    #[napi]
    pub async fn async_execute_transaction(&self, sql: String) -> Result<u32> {
        let mut tx = self.tx_conn.lock().await;
        let conn = tx
            .as_mut()
            .ok_or_else(|| Error::from_reason("no active transaction"))?;
        let n = conn
            .execute(&sql)
            .await
            .map_err(|e| Error::from_reason(format!("execute: {e}")))?;
        Ok(n as u32)
    }

    /// 插入记录（table, json_data）→ 影响行数（参数化绑定）
    #[napi]
    pub async fn async_insert(&self, table: String, data: String) -> Result<u32> {
        if !validate_identifier(&table) {
            return Err(Error::from_reason("invalid table name"));
        }
        let v: serde_json::Value = serde_json::from_str(&data)
            .map_err(|e| Error::from_reason(format!("parse data: {e}")))?;
        let obj = v
            .as_object()
            .ok_or_else(|| Error::from_reason("data must be a JSON object"))?;
        let columns: Vec<&str> = obj.keys().map(|k| k.as_str()).collect();
        if columns.is_empty() {
            return Err(Error::from_reason("data must not be empty"));
        }
        let placeholders: Vec<&str> = columns.iter().map(|_| "?").collect();
        let sql = format!(
            "INSERT INTO {table} ({}) VALUES ({})",
            columns.join(", "),
            placeholders.join(", ")
        );
        let params: Vec<Value> = obj.values().map(json_to_value).collect();
        self.execute_with_params(&sql, &params).await
    }

    /// 更新记录（table, json_data, where_clause）→ 影响行数（set 值参数化）
    #[napi]
    pub async fn async_update(
        &self,
        table: String,
        data: String,
        where_clause: String,
    ) -> Result<u32> {
        if !validate_identifier(&table) {
            return Err(Error::from_reason("invalid table name"));
        }
        if where_clause.trim().is_empty() {
            return Err(Error::from_reason("where_clause must not be empty"));
        }
        let v: serde_json::Value = serde_json::from_str(&data)
            .map_err(|e| Error::from_reason(format!("parse data: {e}")))?;
        let obj = v
            .as_object()
            .ok_or_else(|| Error::from_reason("data must be a JSON object"))?;
        let sets: Vec<String> = obj.keys().map(|k| format!("{k} = ?")).collect();
        if sets.is_empty() {
            return Err(Error::from_reason("data must not be empty"));
        }
        let sql = format!(
            "UPDATE {table} SET {} WHERE {where_clause}",
            sets.join(", ")
        );
        let params: Vec<Value> = obj.values().map(json_to_value).collect();
        self.execute_with_params(&sql, &params).await
    }

    /// 删除记录（table, where_clause）→ 影响行数
    #[napi]
    pub async fn async_delete(&self, table: String, where_clause: String) -> Result<u32> {
        if !validate_identifier(&table) {
            return Err(Error::from_reason("invalid table name"));
        }
        if where_clause.trim().is_empty() {
            return Err(Error::from_reason("where_clause must not be empty"));
        }
        let sql = format!("DELETE FROM {table} WHERE {where_clause}");
        let pool = self.pool_ref()?;
        let mut conn = pool
            .acquire()
            .await
            .map_err(|e| Error::from_reason(format!("acquire: {e}")))?;
        let n = conn
            .execute(&sql)
            .await
            .map_err(|e| Error::from_reason(format!("execute: {e}")))?;
        Ok(n as u32)
    }

    /// 查找记录（table, where_clause）→ JSON 行数组字符串
    #[napi]
    pub async fn async_find(&self, table: String, where_clause: String) -> Result<String> {
        if !validate_identifier(&table) {
            return Err(Error::from_reason("invalid table name"));
        }
        let sql = format!("SELECT * FROM {table} WHERE {where_clause}");
        let pool = self.pool_ref()?;
        let rows = pool
            .query_with_timeout(&sql)
            .await
            .map_err(|e| Error::from_reason(format!("query: {e}")))?;
        serde_json::to_string(&rows).map_err(|e| Error::from_reason(format!("serialize: {e}")))
    }

    /// 事务内插入（table, json_data）→ 影响行数（参数化绑定）
    #[napi]
    pub async fn async_insert_tx(&self, table: String, data: String) -> Result<u32> {
        if !validate_identifier(&table) {
            return Err(Error::from_reason("invalid table name"));
        }
        let v: serde_json::Value = serde_json::from_str(&data)
            .map_err(|e| Error::from_reason(format!("parse data: {e}")))?;
        let obj = v
            .as_object()
            .ok_or_else(|| Error::from_reason("data must be a JSON object"))?;
        let columns: Vec<&str> = obj.keys().map(|k| k.as_str()).collect();
        let placeholders: Vec<&str> = columns.iter().map(|_| "?").collect();
        let sql = format!(
            "INSERT INTO {table} ({}) VALUES ({})",
            columns.join(", "),
            placeholders.join(", ")
        );
        let params: Vec<Value> = obj.values().map(json_to_value).collect();
        let mut tx = self.tx_conn.lock().await;
        let conn = tx
            .as_mut()
            .ok_or_else(|| Error::from_reason("no active transaction"))?;
        let n = conn
            .execute_with_params(&sql, &params)
            .await
            .map_err(|e| Error::from_reason(format!("execute: {e}")))?;
        Ok(n as u32)
    }

    /// 事务内更新（table, json_data, where_clause）→ 影响行数（set 值参数化）
    #[napi]
    pub async fn async_update_tx(
        &self,
        table: String,
        data: String,
        where_clause: String,
    ) -> Result<u32> {
        if !validate_identifier(&table) {
            return Err(Error::from_reason("invalid table name"));
        }
        let v: serde_json::Value = serde_json::from_str(&data)
            .map_err(|e| Error::from_reason(format!("parse data: {e}")))?;
        let obj = v
            .as_object()
            .ok_or_else(|| Error::from_reason("data must be a JSON object"))?;
        let sets: Vec<String> = obj.keys().map(|k| format!("{k} = ?")).collect();
        let sql = format!(
            "UPDATE {table} SET {} WHERE {where_clause}",
            sets.join(", ")
        );
        let params: Vec<Value> = obj.values().map(json_to_value).collect();
        let mut tx = self.tx_conn.lock().await;
        let conn = tx
            .as_mut()
            .ok_or_else(|| Error::from_reason("no active transaction"))?;
        let n = conn
            .execute_with_params(&sql, &params)
            .await
            .map_err(|e| Error::from_reason(format!("execute: {e}")))?;
        Ok(n as u32)
    }

    /// 事务内删除（table, where_clause）→ 影响行数
    #[napi]
    pub async fn async_delete_tx(&self, table: String, where_clause: String) -> Result<u32> {
        if !validate_identifier(&table) {
            return Err(Error::from_reason("invalid table name"));
        }
        let sql = format!("DELETE FROM {table} WHERE {where_clause}");
        let mut tx = self.tx_conn.lock().await;
        let conn = tx
            .as_mut()
            .ok_or_else(|| Error::from_reason("no active transaction"))?;
        let n = conn
            .execute(&sql)
            .await
            .map_err(|e| Error::from_reason(format!("execute: {e}")))?;
        Ok(n as u32)
    }

    /// 事务内查找（table, where_clause）→ JSON 行数组字符串
    #[napi]
    pub async fn async_find_tx(&self, table: String, where_clause: String) -> Result<String> {
        if !validate_identifier(&table) {
            return Err(Error::from_reason("invalid table name"));
        }
        let sql = format!("SELECT * FROM {table} WHERE {where_clause}");
        let mut tx = self.tx_conn.lock().await;
        let conn = tx
            .as_mut()
            .ok_or_else(|| Error::from_reason("no active transaction"))?;
        let rows = conn
            .query(&sql)
            .await
            .map_err(|e| Error::from_reason(format!("query: {e}")))?;
        serde_json::to_string(&rows).map_err(|e| Error::from_reason(format!("serialize: {e}")))
    }

    /// 释放查询结果（JS GC 管理内存，no-op）
    #[napi]
    pub fn query_result_free(&self) {
        // JS GC manages memory
    }

    /// 释放字符串（JS GC 管理内存，no-op）
    #[napi]
    pub fn string_free(&self) {
        // JS GC manages memory
    }

    /// 内部辅助：获取真实连接池引用
    fn pool_ref(&self) -> Result<Arc<CorePool>> {
        self.inner
            .lock()
            .map_err(|e| Error::from_reason(format!("lock: {e}")))?
            .clone()
            .ok_or_else(|| Error::from_reason("pool not connected, call connect() first"))
    }

    /// 内部辅助：参数化执行写语句
    async fn execute_with_params(&self, sql: &str, params: &[Value]) -> Result<u32> {
        let pool = self.pool_ref()?;
        let mut conn = pool
            .acquire()
            .await
            .map_err(|e| Error::from_reason(format!("acquire: {e}")))?;
        let n = conn
            .execute_with_params(sql, params)
            .await
            .map_err(|e| Error::from_reason(format!("execute: {e}")))?;
        Ok(n as u32)
    }
}

impl Drop for Pool {
    fn drop(&mut self) {
        // 防止用户忘记调用 close()：在 Drop 中关闭连接池。
        // 连接池的 idle 队列持有 PooledConnection，其 Drop 需要 tokio
        // 上下文来归还 sqlx 连接；JS GC 线程通常无 tokio 上下文，
        // 此时创建临时 runtime 完成关闭。
        let pool = self.inner.lock().map(|mut g| g.take()).unwrap_or(None);
        if let Some(pool) = pool {
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(async move {
                    pool.close_all().await;
                });
            } else if let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                rt.block_on(async move {
                    pool.close_all().await;
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_connect_query_execute() {
        let pool = Pool::new(Some("sqlite".to_string()), Some(4), None, None, None, None).unwrap();
        assert!(!pool.is_connected());
        pool.connect("sqlite::memory:".to_string()).await.unwrap();
        assert!(pool.is_connected(), "connect should mark pool connected");
        assert!(pool.async_ping().await.unwrap(), "pool should be healthy");

        pool.async_execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)".to_string())
            .await
            .unwrap();
        let n = pool
            .async_execute("INSERT INTO users (name) VALUES ('Alice'), ('Bob')".to_string())
            .await
            .unwrap();
        assert_eq!(n, 2, "2 rows inserted");

        let json = pool
            .async_query("SELECT * FROM users ORDER BY id".to_string())
            .await
            .unwrap();
        assert!(json.contains("Alice"), "missing Alice: {json}");
        assert!(json.contains("Bob"), "missing Bob: {json}");

        assert!(
            pool.table_exists("users".to_string()).await.unwrap(),
            "users table should exist"
        );
        assert!(
            !pool.table_exists("nope".to_string()).await.unwrap(),
            "nope table should not exist"
        );
        assert_eq!(
            pool.async_count("users".to_string()).await.unwrap(),
            2,
            "count should be 2"
        );

        pool.close().await;
        assert!(!pool.is_connected(), "close should disconnect");
    }

    #[tokio::test]
    async fn test_transaction_commit() {
        let pool = Pool::new(Some("sqlite".to_string()), Some(4), None, None, None, None).unwrap();
        pool.connect("sqlite::memory:".to_string()).await.unwrap();
        pool.async_execute("CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)".to_string())
            .await
            .unwrap();

        assert!(pool.async_begin().await.unwrap(), "begin should succeed");
        pool.async_execute_transaction("INSERT INTO t (v) VALUES ('x')".to_string())
            .await
            .unwrap();
        assert!(pool.async_commit().await.unwrap(), "commit should succeed");

        let json = pool
            .async_query("SELECT COUNT(*) AS cnt FROM t".to_string())
            .await
            .unwrap();
        assert!(json.contains("\"cnt\":{\"I64\":1}"), "1 row: {json}");
    }

    #[tokio::test]
    async fn test_transaction_rollback() {
        let pool = Pool::new(Some("sqlite".to_string()), Some(4), None, None, None, None).unwrap();
        pool.connect("sqlite::memory:".to_string()).await.unwrap();
        pool.async_execute("CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)".to_string())
            .await
            .unwrap();

        assert!(pool.async_begin().await.unwrap(), "begin should succeed");
        pool.async_execute_transaction("INSERT INTO t (v) VALUES ('x')".to_string())
            .await
            .unwrap();
        assert!(
            pool.async_rollback().await.unwrap(),
            "rollback should succeed"
        );

        let json = pool
            .async_query("SELECT COUNT(*) AS cnt FROM t".to_string())
            .await
            .unwrap();
        assert!(json.contains("\"cnt\":{\"I64\":0}"), "0 rows: {json}");
    }

    #[tokio::test]
    async fn test_insert_find() {
        let pool = Pool::new(Some("sqlite".to_string()), Some(4), None, None, None, None).unwrap();
        pool.connect("sqlite::memory:".to_string()).await.unwrap();
        pool.async_execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)".to_string())
            .await
            .unwrap();

        let n = pool
            .async_insert("users".to_string(), r#"{"name":"Bob"}"#.to_string())
            .await
            .unwrap();
        assert_eq!(n, 1, "1 row inserted");

        let json = pool
            .async_find("users".to_string(), "id = 1".to_string())
            .await
            .unwrap();
        assert!(json.contains("Bob"), "missing Bob: {json}");
    }
}
