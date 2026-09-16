//! Pool — 连接池

use napi_derive::napi;
use sz_orm_core::DbType;

type Result<T> = napi::bindgen_prelude::Result<T>;
use napi::bindgen_prelude::Error;

#[napi]
pub struct Pool {
    db_type: DbType,
    max_size: u32,
    min_idle: u32,
    acquire_timeout_secs: u64,
    idle_timeout_secs: u64,
    max_lifetime_secs: u64,
    connected: bool,
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
            connected: false,
        })
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
        self.connected
    }

    #[napi]
    pub fn status(&self) -> String {
        format!(
            "Pool(db={}, max={}, min_idle={}, connected={})",
            self.db_type.as_str(),
            self.max_size,
            self.min_idle,
            self.connected
        )
    }

    #[napi]
    pub async fn async_ping(&self) -> Result<bool> {
        Ok(self.connected)
    }

    #[napi]
    pub async fn async_query(&self, sql: String) -> Result<String> {
        if !self.connected {
            return Err(Error::from_reason("pool not connected"));
        }
        Ok(format!("{{\"sql\": {:?}, \"rows\": []}}", sql))
    }

    #[napi]
    pub async fn async_execute(&self, _sql: String) -> Result<u32> {
        if !self.connected {
            return Err(Error::from_reason("pool not connected"));
        }
        Ok(0)
    }

    #[napi]
    pub fn close(&mut self) {
        self.connected = false;
    }

    #[napi]
    pub fn metrics(&self) -> String {
        format!(
            r#"{{"idle":0,"active":0,"max":{},"min":{},"waiters":0}}"#,
            self.max_size, self.min_idle
        )
    }

    #[napi]
    pub async fn async_query_one(&self, _sql: String) -> Result<String> {
        if !self.connected {
            return Err(Error::from_reason("pool not connected"));
        }
        Ok("{}".to_string())
    }

    #[napi]
    pub async fn async_execute_batch(&self, sql: String) -> Result<u32> {
        self.async_execute(sql).await
    }

    #[napi]
    pub async fn table_exists(&self, table: String) -> Result<bool> {
        if !self.connected {
            return Err(Error::from_reason("pool not connected"));
        }
        Ok(!table.is_empty())
    }

    #[napi]
    pub async fn async_count(&self, _table: String) -> Result<i64> {
        if !self.connected {
            return Err(Error::from_reason("pool not connected"));
        }
        Ok(0)
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

    #[napi]
    pub async fn async_begin(&self) -> Result<bool> {
        if !self.connected {
            return Err(Error::from_reason("pool not connected"));
        }
        Ok(true)
    }

    #[napi]
    pub async fn async_commit(&self) -> Result<bool> {
        if !self.connected {
            return Err(Error::from_reason("pool not connected"));
        }
        Ok(true)
    }

    #[napi]
    pub async fn async_rollback(&self) -> Result<bool> {
        if !self.connected {
            return Err(Error::from_reason("pool not connected"));
        }
        Ok(true)
    }

    #[napi]
    pub async fn async_execute_transaction(&self, sql: String) -> Result<u32> {
        self.async_execute(sql).await
    }

    #[napi]
    pub async fn async_insert(&self, _table: String, _data: String) -> Result<u32> {
        if !self.connected {
            return Err(Error::from_reason("pool not connected"));
        }
        Ok(0)
    }

    #[napi]
    pub async fn async_update(
        &self,
        _table: String,
        _data: String,
        _where_clause: String,
    ) -> Result<u32> {
        if !self.connected {
            return Err(Error::from_reason("pool not connected"));
        }
        Ok(0)
    }

    #[napi]
    pub async fn async_delete(&self, _table: String, _where_clause: String) -> Result<u32> {
        if !self.connected {
            return Err(Error::from_reason("pool not connected"));
        }
        Ok(0)
    }

    #[napi]
    pub async fn async_find(&self, _table: String, _where_clause: String) -> Result<String> {
        if !self.connected {
            return Err(Error::from_reason("pool not connected"));
        }
        Ok(r#"[]"#.to_string())
    }

    #[napi]
    pub async fn async_insert_tx(&self, table: String, data: String) -> Result<u32> {
        self.async_insert(table, data).await
    }

    #[napi]
    pub async fn async_update_tx(
        &self,
        table: String,
        data: String,
        where_clause: String,
    ) -> Result<u32> {
        self.async_update(table, data, where_clause).await
    }

    #[napi]
    pub async fn async_delete_tx(&self, table: String, where_clause: String) -> Result<u32> {
        self.async_delete(table, where_clause).await
    }

    #[napi]
    pub async fn async_find_tx(&self, table: String, where_clause: String) -> Result<String> {
        self.async_find(table, where_clause).await
    }

    #[napi]
    pub fn query_result_free(&self) {
        // JS GC manages memory
    }

    #[napi]
    pub fn string_free(&self) {
        // JS GC manages memory
    }
}
