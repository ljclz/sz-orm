//! sz-orm JavaScript/Node.js bindings (napi-rs)
//!
//! Exposes four core API categories of sz-orm-core: Model, QueryBuilder, Pool, Transaction.

mod batch;
mod enhanced;
mod error;
mod migration;
mod model;
mod model_def;
mod pool;
mod query;
mod transaction;
mod types;

pub use batch::{
    BatchDeleteBuilder, BatchDeleteResult, BatchInsertBuilder, BatchInsertResult, BatchStats,
    BatchUpdateBuilder, BatchUpdateResult,
};
pub use enhanced::{
    EnhancedQueryResult, ErrorCategory, ErrorHandler, JoinClause, JoinType, PoolConfig,
    PoolConfigBuilder, QueryBuilderEnhanced, RetryPolicy,
};
pub use migration::{MigrationAction, MigrationStep, MigrationTool, SchemaDiff, SchemaDiffResult};
pub use model_def::{
    FieldDefinition, FieldType, IndexDefinition, ModelDefinition, RelationDefinition, RelationType,
};
