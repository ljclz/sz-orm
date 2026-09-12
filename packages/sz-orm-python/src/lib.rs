//! sz-orm Python bindings (PyO3)
//!
//! Exposes core API categories of sz-orm-core: Model, ActiveModel, QueryBuilder,
//! Pool, Transaction, Repository, Hooks, Observer, Paginator.
//! Async methods are bridged to asyncio via pyo3-asyncio.

use pyo3::prelude::*;

mod active_model;
mod error;
mod hooks;
mod model;
mod observer;
mod paginator;
mod pool;
mod query;
mod repository;
mod transaction;
mod types;

#[pymodule]
fn sz_orm(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<model::PyModel>()?;
    m.add_class::<active_model::PyActiveModel>()?;
    m.add_class::<query::PyQueryBuilder>()?;
    m.add_class::<pool::PyPool>()?;
    m.add_class::<transaction::PyTransaction>()?;
    m.add_class::<repository::PyRepository>()?;
    m.add_class::<repository::PyPageResult>()?;
    m.add_class::<hooks::PyHooks>()?;
    m.add_class::<observer::PyObserver>()?;
    m.add_class::<paginator::PyPaginator>()?;
    m.add("DbType", _py.get_type::<types::PyDbType>())?;
    m.add("DbError", _py.get_type::<error::PyDbError>())?;
    m.add("HookEvent", _py.get_type::<hooks::PyHookEvent>())?;
    Ok(())
}
