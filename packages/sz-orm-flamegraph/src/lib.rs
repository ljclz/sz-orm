//! # sz-orm-flamegraph — Query Performance Flamegraph
//!
//! Collects timing for each query phase (query construction / parameter binding / connection pool acquire / SQL execution / result mapping),
//! outputs Brendan Gregg folded format (`flamegraph.pl` compatible) and inline SVG flamegraph.
//!
//! ```rust
//! use sz_orm_flamegraph::{Phase, QueryTracer};
//!
//! let (result, timings) = QueryTracer::trace_execute(|rec| {
//!     let sql = rec.record(Phase::Build, || "SELECT * FROM users".to_string());
//!     let _rows = rec.record(Phase::SqlExecute, || 42usize);
//!     sql
//! });
//! assert_eq!(result, "SELECT * FROM users");
//! assert_eq!(timings.len(), 2);
//! ```
//!
//! Output formats:
//! - [`render::to_brendan_gregg`]: Folded stack format, can be directly fed to `flamegraph.pl`
//! - [`render::to_svg`]: Self-rendered inline SVG (no external dependencies)
//!
//! When `query-flamegraph` feature is enabled, phase timing can be written to existing `sz-orm-tracing` `Tracer` spans
//! via [`collector::QueryTracer::with_tracer`].

pub mod collector;
pub mod config;
pub mod diff;
pub mod flame_node;
pub mod render;
pub mod stats;

pub use collector::{Phase, QueryPhaseTiming, QueryTracer};
pub use config::{
    ColorPalette, ColorScheme, LayoutConfig, OutputFormat, RenderConfig, RenderOptions,
};
pub use diff::{DiffEntry, DiffMode, DiffResult, DiffType, FlameDiff};
pub use flame_node::{
    FlameGraphBuilder, FlameGraphData, FlameGraphFilter, FlameGraphMerger, FlameNode,
};
pub use stats::{
    DepthDistribution, FlameStats, FrameStats, Hotspot, HotspotDetector, HotspotStrategy,
};
