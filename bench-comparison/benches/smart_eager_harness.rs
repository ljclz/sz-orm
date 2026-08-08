//! SmartEagerBenchHarness — 性能基准数据生成/清理工具（v2.4.0 任务 3.1）
//!
//! 使用 SQLite in-memory 避免外部依赖与网络/DB 负载干扰。

use rusqlite::Connection as RusqliteConn;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;
use sz_orm_core::{Connection, DbError, Value};

/// SQLite 连接适配器（Mutex 包装，实现 Connection trait）
pub struct BenchSqliteConn {
    conn: Mutex<RusqliteConn>,
}

impl BenchSqliteConn {
    pub fn open_in_memory() -> Self {
        let conn = RusqliteConn::open_in_memory().expect("open sqlite in-memory");
        Self { conn: Mutex::new(conn) }
    }

    pub fn execute_direct(&self, sql: &str) {
        let _ = self.conn.lock().unwrap().execute(sql, []);
    }

    fn value_ref_to_value(v: rusqlite::types::ValueRef) -> Value {
        use rusqlite::types::ValueRef;
        match v {
            ValueRef::Null => Value::Null,
            ValueRef::Integer(i) => Value::I64(i),
            ValueRef::Real(f) => Value::F64(f),
            ValueRef::Text(b) => Value::String(String::from_utf8_lossy(b).to_string()),
            ValueRef::Blob(b) => Value::Bytes(b.to_vec()),
        }
    }

    fn value_to_rusqlite(v: &Value) -> Box<dyn rusqlite::types::ToSql> {
        use rusqlite::types::Value as Rv;
        match v {
            Value::Null => Box::new(None::<Rv>),
            Value::Bool(b) => Box::new(*b),
            Value::I32(i) => Box::new(*i),
            Value::I64(i) => Box::new(*i),
            Value::F64(f) => Box::new(*f),
            Value::String(s) => Box::new(s.clone()),
            _ => Box::new(None::<Rv>),
        }
    }

    fn query_inner(&self, sql: &str) -> Result<Vec<HashMap<String, Value>>, DbError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(sql).map_err(|e| DbError::QueryError(e.to_string()))?;
        let col_names: Vec<String> = (0..stmt.column_count())
            .map(|i| stmt.column_name(i).map(|n| n.to_string()).unwrap_or_else(|_| format!("col_{i}")))
            .collect();
        let mut rows = stmt.query([]).map_err(|e| DbError::QueryError(e.to_string()))?;
        let mut result = Vec::new();
        while let Some(row) = rows.next().unwrap_or(None) {
            let mut map = HashMap::new();
            for (i, name) in col_names.iter().enumerate() {
                let val = row.get_ref(i).map(Self::value_ref_to_value).unwrap_or(Value::Null);
                map.insert(name.clone(), val);
            }
            result.push(map);
        }
        Ok(result)
    }

    fn query_with_params_inner(&self, sql: &str, params: &[Value]) -> Result<Vec<HashMap<String, Value>>, DbError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(sql).map_err(|e| DbError::QueryError(e.to_string()))?;
        let col_names: Vec<String> = (0..stmt.column_count())
            .map(|i| stmt.column_name(i).map(|n| n.to_string()).unwrap_or_else(|_| format!("col_{i}")))
            .collect();
        let boxed: Vec<Box<dyn rusqlite::types::ToSql>> = params.iter().map(Self::value_to_rusqlite).collect();
        let refs: Vec<&dyn rusqlite::types::ToSql> = boxed.iter().map(|b| b.as_ref()).collect();
        let mut rows = stmt.query(refs.as_slice()).map_err(|e| DbError::QueryError(e.to_string()))?;
        let mut result = Vec::new();
        while let Some(row) = rows.next().unwrap_or(None) {
            let mut map = HashMap::new();
            for (i, name) in col_names.iter().enumerate() {
                let val = row.get_ref(i).map(Self::value_ref_to_value).unwrap_or(Value::Null);
                map.insert(name.clone(), val);
            }
            result.push(map);
        }
        Ok(result)
    }
}

impl Connection for BenchSqliteConn {
    fn execute<'a>(&'a mut self, sql: &'a str) -> Pin<Box<dyn Future<Output = Result<u64, DbError>> + Send + 'a>> {
        let result = self.conn.lock().unwrap().execute(sql, [])
            .map(|n| n as u64)
            .map_err(|e| DbError::QueryError(e.to_string()));
        Box::pin(async move { result })
    }

    fn query<'a>(&'a mut self, sql: &'a str) -> Pin<Box<dyn Future<Output = Result<Vec<HashMap<String, Value>>, DbError>> + Send + 'a>> {
        let result = self.query_inner(sql);
        Box::pin(async move { result })
    }

    fn query_with_params<'a>(&'a mut self, sql: &'a str, params: &'a [Value]) -> Pin<Box<dyn Future<Output = Result<Vec<HashMap<String, Value>>, DbError>> + Send + 'a>> {
        let result = self.query_with_params_inner(sql, params);
        Box::pin(async move { result })
    }

    fn begin_transaction<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        let r = self.conn.lock().unwrap().execute("BEGIN", []).map_err(|e| DbError::QueryError(e.to_string()));
        Box::pin(async move { r.map(|_| ()) })
    }

    fn commit<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        let r = self.conn.lock().unwrap().execute("COMMIT", []).map_err(|e| DbError::QueryError(e.to_string()));
        Box::pin(async move { r.map(|_| ()) })
    }

    fn rollback<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        let r = self.conn.lock().unwrap().execute("ROLLBACK", []).map_err(|e| DbError::QueryError(e.to_string()));
        Box::pin(async move { r.map(|_| ()) })
    }

    fn is_connected(&self) -> bool { true }
    fn ping<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> { Box::pin(async move { true }) }
    fn close<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> { Box::pin(async move { Ok(()) }) }
}

/// 性能基准数据生成/清理工具
pub struct SmartEagerBenchHarness {
    conn: BenchSqliteConn,
}

/// 四规模档位
pub const BENCH_SIZES: &[usize] = &[10, 100, 1000, 10000];

impl SmartEagerBenchHarness {
    pub fn new() -> Self {
        Self { conn: BenchSqliteConn::open_in_memory() }
    }

    /// 获取连接引用（供基准测试使用）
    pub fn conn(&mut self) -> &mut BenchSqliteConn {
        &mut self.conn
    }

    /// 按规模 N 生成主表 N 条 + 关联表 ≈N 条
    pub fn setup(&mut self, scale: usize) {
        self.conn.execute_direct("CREATE TABLE IF NOT EXISTS users (id INTEGER PRIMARY KEY, name TEXT NOT NULL)");
        self.conn.execute_direct("CREATE TABLE IF NOT EXISTS orders (id INTEGER PRIMARY KEY, user_id INTEGER NOT NULL, total REAL NOT NULL)");

        for i in 1..=scale {
            self.conn.execute_direct(&format!(
                "INSERT INTO users (id, name) VALUES ({}, 'user_{}')",
                i, i
            ));
        }

        for i in 1..=scale {
            let user_id = ((i - 1) % scale) + 1;
            self.conn.execute_direct(&format!(
                "INSERT INTO orders (id, user_id, total) VALUES ({}, {}, {})",
                i, user_id, i as f64 * 1.5
            ));
        }
    }

    /// 清理数据
    pub fn teardown(&mut self, _scale: usize) {
        self.conn.execute_direct("DROP TABLE IF EXISTS orders");
        self.conn.execute_direct("DROP TABLE IF EXISTS users");
    }
}