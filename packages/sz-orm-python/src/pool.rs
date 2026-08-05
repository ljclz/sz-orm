//! PyPool — 连接池

use pyo3::prelude::*;
use sz_orm_core::DbType;

#[pyclass(name = "Pool")]
pub struct PyPool {
    db_type: DbType,
    max_size: u32,
    min_idle: u32,
    acquire_timeout_secs: u64,
    idle_timeout_secs: u64,
    max_lifetime_secs: u64,
    connected: bool,
}

#[pymethods]
impl PyPool {
    #[new]
    #[pyo3(signature = (db_type="mysql".to_string(), max_size=100, min_idle=0, acquire_timeout=30, idle_timeout=600, max_lifetime=1800))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        db_type: Option<String>,
        max_size: Option<u32>,
        min_idle: Option<u32>,
        acquire_timeout: Option<u64>,
        idle_timeout: Option<u64>,
        max_lifetime: Option<u64>,
    ) -> PyResult<Self> {
        let dt = db_type.unwrap_or_else(|| "mysql".to_string());
        let db_type = DbType::from_str(&dt).ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(format!("unknown DbType: {}", dt))
        })?;
        Ok(Self {
            db_type,
            max_size: max_size.unwrap_or(100),
            min_idle: min_idle.unwrap_or(0),
            acquire_timeout_secs: acquire_timeout.unwrap_or(30),
            idle_timeout_secs: idle_timeout.unwrap_or(600),
            max_lifetime_secs: max_lifetime.unwrap_or(1800),
            connected: false,
        })
    }

    #[getter]
    fn db_type(&self) -> &str {
        self.db_type.as_str()
    }

    #[getter]
    fn max_size(&self) -> u32 {
        self.max_size
    }

    #[getter]
    fn min_idle(&self) -> u32 {
        self.min_idle
    }

    #[getter]
    fn acquire_timeout(&self) -> u64 {
        self.acquire_timeout_secs
    }

    #[getter]
    fn idle_timeout(&self) -> u64 {
        self.idle_timeout_secs
    }

    #[getter]
    fn max_lifetime(&self) -> u64 {
        self.max_lifetime_secs
    }

    #[getter]
    fn is_connected(&self) -> bool {
        self.connected
    }

    fn status(&self) -> String {
        format!(
            "Pool(db={}, max={}, min_idle={}, connected={})",
            self.db_type.as_str(),
            self.max_size,
            self.min_idle,
            self.connected
        )
    }

    fn __repr__(&self) -> String {
        self.status()
    }
}
