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
}
