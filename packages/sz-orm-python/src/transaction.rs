//! PyTransaction — 事务（真实连接）
//!
//! v10.12.0 修复：由「仅有字段的空壳」改为真实事务。
//! 通过 `Pool.create_transaction()` 获取独占连接并执行 BEGIN；
//! `execute/commit/rollback/savepoint` 均真实执行 SQL。
//! 直接 `Transaction()` 构造的对象无连接，调用 execute 会返回明确错误。

use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;

use sz_orm_core::PooledConnection;

use crate::pool::runtime;

/// 创建真实事务（供 `Pool.create_transaction` 调用）
pub(crate) fn create_transaction(pool: &sz_orm_core::Pool) -> PyResult<PyTransaction> {
    let conn = runtime()
        .block_on(async {
            let mut conn = pool
                .acquire()
                .await
                .map_err(|e| PyRuntimeError::new_err(format!("acquire: {e}")))?;
            conn.execute("BEGIN")
                .await
                .map_err(|e| PyRuntimeError::new_err(format!("begin: {e}")))?;
            Ok(conn)
        })
        .map_err(|e: PyErr| e)?;
    Ok(PyTransaction {
        conn: Some(conn),
        active: true,
        isolation: "read_committed".to_string(),
        read_only: false,
    })
}

#[pyclass(name = "Transaction")]
pub struct PyTransaction {
    conn: Option<PooledConnection>,
    active: bool,
    isolation: String,
    read_only: bool,
}

impl Drop for PyTransaction {
    fn drop(&mut self) {
        // 若事务未显式 commit/rollback，归还连接（PooledConnection::drop
        // 需要 tokio runtime 上下文来 spawn 归还任务）。
        if self.conn.is_some() {
            if tokio::runtime::Handle::try_current().is_ok() {
                self.conn.take();
            } else {
                runtime().block_on(async {
                    self.conn.take();
                });
            }
        }
    }
}

#[pymethods]
impl PyTransaction {
    #[new]
    #[pyo3(signature = (isolation="read_committed".to_string(), read_only=false))]
    pub(crate) fn new(isolation: Option<String>, read_only: Option<bool>) -> Self {
        Self {
            conn: None,
            active: false,
            isolation: isolation.unwrap_or_else(|| "read_committed".to_string()),
            read_only: read_only.unwrap_or(false),
        }
    }

    /// 在事务内执行 SQL（INSERT/UPDATE/DELETE），返回影响行数。
    ///
    /// 需要事务对象来自 `Pool.create_transaction()`；直接 `Transaction()` 构造的
    /// 对象无连接，调用本方法返回明确错误。
    pub(crate) fn execute(&mut self, sql: &str) -> PyResult<u64> {
        if !self.active {
            return Err(PyRuntimeError::new_err(
                "transaction is not active; create it via Pool.create_transaction()",
            ));
        }
        let conn = self
            .conn
            .as_mut()
            .ok_or_else(|| PyRuntimeError::new_err("transaction has no active connection"))?;
        runtime()
            .block_on(async { conn.execute(sql).await })
            .map_err(|e| PyRuntimeError::new_err(format!("execute: {e}")))
    }

    /// 提交事务（真实 COMMIT，连接归还连接池）。
    pub(crate) fn commit(&mut self) -> PyResult<bool> {
        if !self.active {
            return Err(PyRuntimeError::new_err("transaction is not active"));
        }
        let mut conn = self
            .conn
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("transaction has no active connection"))?;
        runtime()
            .block_on(async move {
                let result = conn.execute("COMMIT").await;
                // 在 tokio 上下文中 drop 连接，确保 sqlx 连接释放不 panic
                drop(conn);
                result
            })
            .map_err(|e| PyRuntimeError::new_err(format!("commit: {e}")))?;
        self.active = false;
        Ok(true)
    }

    /// 回滚事务（真实 ROLLBACK，连接归还连接池）。
    pub(crate) fn rollback(&mut self) -> PyResult<bool> {
        if !self.active {
            return Err(PyRuntimeError::new_err("transaction is not active"));
        }
        let mut conn = self
            .conn
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("transaction has no active connection"))?;
        runtime()
            .block_on(async move {
                let result = conn.execute("ROLLBACK").await;
                // 在 tokio 上下文中 drop 连接，确保 sqlx 连接释放不 panic
                drop(conn);
                result
            })
            .map_err(|e| PyRuntimeError::new_err(format!("rollback: {e}")))?;
        self.active = false;
        Ok(true)
    }

    /// 创建保存点。
    pub(crate) fn savepoint(&mut self, name: &str) -> PyResult<bool> {
        if !self.active {
            return Err(PyRuntimeError::new_err("transaction is not active"));
        }
        if name.trim().is_empty() {
            return Err(PyValueError::new_err("savepoint name must not be empty"));
        }
        let conn = self
            .conn
            .as_mut()
            .ok_or_else(|| PyRuntimeError::new_err("transaction has no active connection"))?;
        let sql = format!("SAVEPOINT {}", name);
        runtime()
            .block_on(async { conn.execute(&sql).await })
            .map_err(|e| PyRuntimeError::new_err(format!("savepoint: {e}")))?;
        Ok(true)
    }

    /// 释放保存点。
    pub(crate) fn release_savepoint(&mut self, name: &str) -> PyResult<bool> {
        if !self.active {
            return Err(PyRuntimeError::new_err("transaction is not active"));
        }
        let conn = self
            .conn
            .as_mut()
            .ok_or_else(|| PyRuntimeError::new_err("transaction has no active connection"))?;
        let sql = format!("RELEASE SAVEPOINT {}", name);
        runtime()
            .block_on(async { conn.execute(&sql).await })
            .map_err(|e| PyRuntimeError::new_err(format!("release savepoint: {e}")))?;
        Ok(true)
    }

    #[getter]
    pub(crate) fn is_active(&self) -> bool {
        self.active
    }

    #[getter]
    pub(crate) fn isolation(&self) -> &str {
        &self.isolation
    }

    #[getter]
    pub(crate) fn read_only(&self) -> bool {
        self.read_only
    }

    pub(crate) fn __repr__(&self) -> String {
        format!(
            "Transaction(active={}, isolation={}, read_only={}, has_connection={})",
            self.active,
            self.isolation,
            self.read_only,
            self.conn.is_some()
        )
    }
}
