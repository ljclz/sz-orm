//! sz-orm JavaScript/Node.js 绑定（napi-rs）
//!
//! 暴露 sz-orm-core 的四类核心 API：Model、QueryBuilder、Pool、Transaction。

mod error;
mod model;
mod pool;
mod query;
mod transaction;
mod types;
