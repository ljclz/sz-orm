//! rusqlite 到 sz_orm_core::Connection 的适配器
//!
//! 用 Mutex 包装 rusqlite::Connection 使其满足 Send + Sync。

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;

use rusqlite::types::ValueRef;
use rusqlite::types::{ToSql, Value as RusqliteValue};
use rusqlite::Connection as RusqliteConn;
use sz_orm_core::Connection;
use sz_orm_core::DbError;
use sz_orm_core::Value;

/// rusqlite 连接适配器，实现 sz_orm_core::Connection trait
pub struct RusqliteConnection {
    conn: Mutex<RusqliteConn>,
}

impl RusqliteConnection {
    pub fn new(conn: RusqliteConn) -> Self {
        Self {
            conn: Mutex::new(conn),
        }
    }

    pub fn open_in_memory() -> Self {
        let conn = RusqliteConn::open_in_memory().expect("open sqlite in-memory");
        Self::new(conn)
    }

    /// 直接执行 SQL（非异步，供 setup 使用）
    pub fn execute_direct(&self, sql: &str) {
        let conn = self.conn.lock().unwrap();
        let _ = conn.execute(sql, []);
    }

    fn value_ref_to_value(v: ValueRef) -> Value {
        match v {
            ValueRef::Null => Value::Null,
            ValueRef::Integer(i) => Value::I64(i),
            ValueRef::Real(f) => Value::F64(f),
            ValueRef::Text(bytes) => Value::String(String::from_utf8_lossy(bytes).to_string()),
            ValueRef::Blob(bytes) => Value::Bytes(bytes.to_vec()),
        }
    }

    fn execute_inner(&self, sql: &str) -> Result<u64, DbError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(sql, [])
            .map(|n| n as u64)
            .map_err(|e| DbError::QueryError(e.to_string()))
    }

    fn query_inner(&self, sql: &str) -> Result<Vec<HashMap<String, Value>>, DbError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(sql)
            .map_err(|e| DbError::QueryError(e.to_string()))?;
        let col_count = stmt.column_count();
        let col_names: Vec<String> = (0..col_count)
            .map(|i| {
                stmt.column_name(i)
                    .map(|n| n.to_string())
                    .unwrap_or_else(|_| format!("col_{i}"))
            })
            .collect();

        let mut rows = stmt
            .query([])
            .map_err(|e| DbError::QueryError(e.to_string()))?;

        let mut result = Vec::new();
        while let Some(row) = rows.next().unwrap_or(None) {
            let mut map = HashMap::new();
            for (i, name) in col_names.iter().enumerate() {
                let val = row
                    .get_ref(i)
                    .map(Self::value_ref_to_value)
                    .unwrap_or(Value::Null);
                map.insert(name.clone(), val);
            }
            result.push(map);
        }
        Ok(result)
    }

    fn value_to_rusqlite(v: &Value) -> Box<dyn ToSql> {
        match v {
            Value::Null => Box::new(None::<RusqliteValue>),
            Value::Bool(b) => Box::new(*b),
            Value::I8(i) => Box::new(*i as i64),
            Value::I16(i) => Box::new(*i as i64),
            Value::I32(i) => Box::new(*i as i64),
            Value::I64(i) => Box::new(*i),
            Value::U8(u) => Box::new(*u as i64),
            Value::U16(u) => Box::new(*u as i64),
            Value::U32(u) => Box::new(*u as i64),
            Value::U64(u) => Box::new(*u as i64),
            Value::F32(f) => Box::new(*f as f64),
            Value::F64(f) => Box::new(*f),
            Value::String(s) => Box::new(s.clone()),
            Value::Decimal(s) => Box::new(s.clone()),
            Value::Uuid(s) => Box::new(s.clone()),
            Value::Bytes(b) => Box::new(b.clone()),
            _ => Box::new(None::<RusqliteValue>),
        }
    }

    fn query_with_params_inner(
        &self,
        sql: &str,
        params: &[Value],
    ) -> Result<Vec<HashMap<String, Value>>, DbError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(sql)
            .map_err(|e| DbError::QueryError(e.to_string()))?;
        let col_count = stmt.column_count();
        let col_names: Vec<String> = (0..col_count)
            .map(|i| {
                stmt.column_name(i)
                    .map(|n| n.to_string())
                    .unwrap_or_else(|_| format!("col_{i}"))
            })
            .collect();

        let params_boxed: Vec<Box<dyn ToSql>> =
            params.iter().map(Self::value_to_rusqlite).collect();
        let params_refs: Vec<&dyn ToSql> = params_boxed.iter().map(|b| b.as_ref()).collect();

        let mut rows = stmt
            .query(params_refs.as_slice())
            .map_err(|e| DbError::QueryError(e.to_string()))?;

        let mut result = Vec::new();
        while let Some(row) = rows.next().unwrap_or(None) {
            let mut map = HashMap::new();
            for (i, name) in col_names.iter().enumerate() {
                let val = row
                    .get_ref(i)
                    .map(Self::value_ref_to_value)
                    .unwrap_or(Value::Null);
                map.insert(name.clone(), val);
            }
            result.push(map);
        }
        Ok(result)
    }
}

impl Connection for RusqliteConnection {
    fn execute<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<u64, DbError>> + Send + 'a>> {
        let result = self.execute_inner(sql);
        Box::pin(async move { result })
    }

    fn query<'a>(
        &'a mut self,
        sql: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<HashMap<String, Value>>, DbError>> + Send + 'a>>
    {
        let result = self.query_inner(sql);
        Box::pin(async move { result })
    }

    fn query_with_params<'a>(
        &'a mut self,
        sql: &'a str,
        params: &'a [Value],
    ) -> Pin<Box<dyn Future<Output = Result<Vec<HashMap<String, Value>>, DbError>> + Send + 'a>>
    {
        let result = self.query_with_params_inner(sql, params);
        Box::pin(async move { result })
    }

    fn begin_transaction<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        let result = self
            .conn
            .lock()
            .unwrap()
            .execute("BEGIN", [])
            .map_err(|e| DbError::QueryError(e.to_string()));
        Box::pin(async move { result.map(|_| ()) })
    }

    fn commit<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        let result = self
            .conn
            .lock()
            .unwrap()
            .execute("COMMIT", [])
            .map_err(|e| DbError::QueryError(e.to_string()));
        Box::pin(async move { result.map(|_| ()) })
    }

    fn rollback<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        let result = self
            .conn
            .lock()
            .unwrap()
            .execute("ROLLBACK", [])
            .map_err(|e| DbError::QueryError(e.to_string()));
        Box::pin(async move { result.map(|_| ()) })
    }

    fn is_connected(&self) -> bool {
        true
    }

    fn ping<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move { true })
    }

    fn close<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<(), DbError>> + Send + 'a>> {
        Box::pin(async move { Ok(()) })
    }
}
