//! sz-orm Python bindings (PyO3)
//!
//! Exposes four core API categories of sz-orm-core: Model, QueryBuilder, Pool, Transaction.
//! Async methods are bridged to asyncio via pyo3-asyncio.

use pyo3::prelude::*;

mod error;
mod model;
mod pool;
mod query;
mod transaction;
mod types;

#[pymodule]
fn sz_orm(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<model::PyModel>()?;
    m.add_class::<query::PyQueryBuilder>()?;
    m.add_class::<pool::PyPool>()?;
    m.add_class::<transaction::PyTransaction>()?;
    m.add("DbType", _py.get_type::<types::PyDbType>())?;
    m.add("DbError", _py.get_type::<error::PyDbError>())?;
    Ok(())
}
