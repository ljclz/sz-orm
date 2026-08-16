//! PyPool — 连接池（真实连接，SQLite 后端）

use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use std::sync::Arc;
use std::sync::OnceLock;

use sz_orm_core::{DbType, Pool, PoolConfigBuilder};

/// 全局 tokio runtime（PyO3 同步方法需要 block_on 执行异步池操作）
fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("failed to build tokio runtime")
    })
}

/// 将 sz-orm-core 的 Pool 包装为 PyO3 对象
#[pyclass(name = "Pool")]
pub struct PyPool {
    db_type: DbType,
    max_size: u32,
    min_idle: u32,
    acquire_timeout_secs: u64,
    idle_timeout_secs: u64,
    max_lifetime_secs: u64,
    /// 真实连接池（connect 后 Some）
    inner: Option<Arc<Pool>>,
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
        let db_type = DbType::from_str(&dt)
            .ok_or_else(|| PyValueError::new_err(format!("unknown DbType: {}", dt)))?;
        Ok(Self {
            db_type,
            max_size: max_size.unwrap_or(100),
            min_idle: min_idle.unwrap_or(0),
            acquire_timeout_secs: acquire_timeout.unwrap_or(30),
            idle_timeout_secs: idle_timeout.unwrap_or(600),
            max_lifetime_secs: max_lifetime.unwrap_or(1800),
            inner: None,
        })
    }

    /// 建立真实连接（SQLite 后端）
    ///
    /// dsn 示例："sqlite::memory:" 或 "sqlite://path/to/db.sqlite"
    #[pyo3(signature = (dsn="sqlite::memory:".to_string()))]
    fn connect(&mut self, dsn: String) -> PyResult<()> {
        if self.inner.is_some() {
            return Ok(());
        }
        let cfg = PoolConfigBuilder::new()
            .max_size(self.max_size.max(1))
            .min_idle(self.min_idle.min(self.max_size.max(1)))
            .acquire_timeout(self.acquire_timeout_secs)
            .idle_timeout(self.idle_timeout_secs)
            .build()
            .map_err(|e| PyRuntimeError::new_err(format!("pool config: {e}")))?;

        let pool = runtime()
            .block_on(async {
                let handle = Arc::new(
                    sz_orm_sqlx::SqlitePoolHandle::connect(&dsn)
                        .await
                        .map_err(|e| PyRuntimeError::new_err(format!("connect: {e}")))?,
                );
                let factory = Arc::new(sz_orm_sqlx::SqlxSqliteConnectionFactory::new(handle));
                Pool::new(cfg, factory).map_err(|e| PyRuntimeError::new_err(format!("pool: {e}")))
            })
            .map_err(|e: PyErr| e)?;

        self.inner = Some(Arc::new(pool));
        Ok(())
    }

    /// 执行写语句（INSERT/UPDATE/DELETE），返回影响行数
    fn execute(&self, sql: &str) -> PyResult<u64> {
        let pool = self
            .inner
            .as_ref()
            .ok_or_else(|| PyRuntimeError::new_err("pool not connected, call connect() first"))?;
        let pool = pool.clone();
        runtime()
            .block_on(async move {
                let mut conn = pool
                    .acquire()
                    .await
                    .map_err(|e| PyRuntimeError::new_err(format!("acquire: {e}")))?;
                conn.execute(sql)
                    .await
                    .map_err(|e| PyRuntimeError::new_err(format!("execute: {e}")))
            })
            .map_err(|e: PyErr| e)
    }

    /// 执行查询，返回 JSON 行数组字符串
    fn query(&self, sql: &str) -> PyResult<String> {
        let pool = self
            .inner
            .as_ref()
            .ok_or_else(|| PyRuntimeError::new_err("pool not connected, call connect() first"))?;
        let pool = pool.clone();
        runtime()
            .block_on(async move {
                let rows = pool
                    .query_with_timeout(sql)
                    .await
                    .map_err(|e| PyRuntimeError::new_err(format!("query: {e}")))?;
                serde_json::to_string(&rows)
                    .map_err(|e| PyRuntimeError::new_err(format!("serialize: {e}")))
            })
            .map_err(|e: PyErr| e)
    }

    /// 健康检查（真实 acquire + ping）
    fn ping(&self) -> PyResult<bool> {
        let pool = self
            .inner
            .as_ref()
            .ok_or_else(|| PyRuntimeError::new_err("pool not connected, call connect() first"))?;
        let pool = pool.clone();
        Ok(runtime().block_on(async move {
            let mut conn = match pool.acquire().await {
                Ok(c) => c,
                Err(_) => return false,
            };
            conn.ping().await
        }))
    }

    /// 关闭连接池
    fn close(&mut self) {
        if let Some(pool) = self.inner.take() {
            runtime().block_on(async move {
                pool.close_all().await;
            });
        }
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
        self.inner.is_some()
    }

    fn status(&self) -> String {
        format!(
            "Pool(db={}, max={}, min_idle={}, connected={})",
            self.db_type.as_str(),
            self.max_size,
            self.min_idle,
            self.inner.is_some()
        )
    }

    fn __repr__(&self) -> String {
        self.status()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_python() {
        // extension-module 模式下不自动初始化解释器，测试需显式准备
        pyo3::prepare_freethreaded_python();
    }

    #[test]
    fn test_pool_connect_and_e2e() {
        init_python();
        pyo3::Python::with_gil(|_py| {
            let mut pool =
                PyPool::new(Some("sqlite".to_string()), Some(4), None, None, None, None).unwrap();
            assert!(!pool.is_connected());
            pool.connect("sqlite::memory:".to_string()).unwrap();
            assert!(pool.is_connected());
            assert!(pool.ping().unwrap(), "pool should be healthy");

            // 建表 + 插入（execute 返回 u64，无失败路径，仅断言调用成功）
            let _ = pool
                .execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)")
                .unwrap();
            let n = pool
                .execute("INSERT INTO users (name) VALUES ('Alice'), ('Bob'), ('Carol')")
                .unwrap();
            assert_eq!(n, 3, "3 rows inserted");

            // 查询 JSON
            let json = pool
                .query("SELECT id, name FROM users ORDER BY id")
                .unwrap();
            assert!(json.contains("Alice"), "missing Alice: {json}");
            assert!(json.contains("Bob"), "missing Bob: {json}");
            assert!(json.contains("Carol"), "missing Carol: {json}");

            pool.close();
            assert!(!pool.is_connected());
        });
    }

    #[test]
    fn test_pool_query_before_connect_errors() {
        init_python();
        pyo3::Python::with_gil(|_py| {
            let pool =
                PyPool::new(Some("sqlite".to_string()), Some(4), None, None, None, None).unwrap();
            let err = pool.query("SELECT 1");
            assert!(err.is_err(), "query before connect should error");
        });
    }

    #[test]
    fn test_pool_invalid_dsn_errors() {
        init_python();
        pyo3::Python::with_gil(|_py| {
            let mut pool =
                PyPool::new(Some("sqlite".to_string()), Some(4), None, None, None, None).unwrap();
            let err = pool.connect("not-a-valid-dsn::::".to_string());
            assert!(err.is_err(), "invalid dsn should error");
        });
    }

    #[test]
    fn test_module_registers_classes() {
        init_python();
        pyo3::Python::with_gil(|py| {
            let module = PyModule::new(py, "sz_orm").unwrap();
            crate::sz_orm(py, module).unwrap();
            assert!(module.getattr("Pool").is_ok());
            assert!(module.getattr("Model").is_ok());
            assert!(module.getattr("QueryBuilder").is_ok());
        });
    }
}
