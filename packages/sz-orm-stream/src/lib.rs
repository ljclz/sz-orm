//! # sz-orm-stream — Async Streaming Result Set
//!
//! Based on `stream-resultset` feature, supports streaming return of large result sets, avoiding loading everything into memory at once.
//! v4.5.0 M3 implements StreamResultSet + KeysetPaginator + backpressure control.

pub mod config;

#[cfg(feature = "stream-resultset")]
pub mod backpressure;
#[cfg(feature = "stream-resultset")]
pub mod backpressure_stream;
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
// v7.0.0 stream-processing 模块
#[cfg(feature = "stream-processing")]
pub mod flink_adapter;
#[cfg(feature = "stream-processing")]
pub mod materialized_view;
#[cfg(feature = "stream-processing")]
pub mod unified_job;
#[cfg(feature = "stream-processing")]
pub mod window;

#[cfg(feature = "stream-processing")]
pub use backpressure::BackpressureStrategy;
#[cfg(feature = "stream-processing")]
pub use flink_adapter::{FlinkClient, FlinkError, JobArgs, JobStatus};
#[cfg(feature = "stream-processing")]
pub use materialized_view::{
    MaterializedViewDef, RefreshStrategy, StreamError, ViewId, ViewRefreshEngine,
};
#[cfg(feature = "stream-processing")]
pub use unified_job::{
    JobDag, JobHandle, JobMode, StreamBatchJob, TimeRange, Watermark as JobWatermark,
};
#[cfg(feature = "stream-processing")]
pub use window::{Event, Watermark, Window, WindowAssigner, WindowConfig, WindowType};
// v7.1.0 CDC 实时数据同步
#[cfg(feature = "cdc-realtime-sync")]
pub mod cdc_sync;

#[cfg(feature = "cdc-realtime-sync")]
pub use cdc_sync::{
    create_file_checkpoint, create_memory_checkpoint, CdcSyncConfig, CdcSyncCoordinator,
};
