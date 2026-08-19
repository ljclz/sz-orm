//! # sz-orm-stream — Async Streaming Result Set
//!
//! Based on `stream-resultset` feature, supports streaming return of large result sets, avoiding loading everything into memory at once.
//! v4.5.0 M3 implements StreamResultSet + KeysetPaginator + backpressure control.

pub mod config;

#[cfg(feature = "stream-resultset")]
pub mod backpressure;
#[cfg(feature = "stream-resultset")]
pub mod batch_processor;
#[cfg(feature = "stream-resultset")]
pub mod keyset;
#[cfg(feature = "stream-resultset")]
pub mod operators;
#[cfg(feature = "stream-resultset")]
pub mod paginator;
#[cfg(feature = "stream-resultset")]
pub mod result_set;

pub use config::{OrderDirection, PaginationStrategy, StreamResultSetConfig};

#[cfg(feature = "stream-resultset")]
pub use backpressure::AsyncBackpressureController;
#[cfg(feature = "stream-resultset")]
pub use batch_processor::{
    BatchProcessingStats, BatchProcessorConfig, BatchResult, StreamBatchProcessor,
};
#[cfg(feature = "stream-resultset")]
pub use keyset::KeysetPaginator;
#[cfg(feature = "stream-resultset")]
pub use operators::{
    AggregateFunction, AggregateResult, FilterCondition, MultiAggregator, StreamAggregator,
    StreamFilter, StreamMapper,
};
#[cfg(feature = "stream-resultset")]
pub use paginator::{PaginationState, PaginationStats, StreamPaginator, StreamPaginatorConfig};
#[cfg(feature = "stream-resultset")]
pub use result_set::StreamResultSet;
