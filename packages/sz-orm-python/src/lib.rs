//! sz-orm Python 绑定（PyO3）
//!
//! 暴露 sz-orm-core 的四类核心 API：Model、QueryBuilder、Pool、Transaction。
//! 异步方法通过 pyo3-asyncio 桥接到 asyncio。

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
