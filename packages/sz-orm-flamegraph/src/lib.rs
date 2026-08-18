//! # sz-orm-flamegraph — 查询性能火焰图
//!
//! 采集查询各阶段耗时（查询构造 / 参数绑定 / 连接池获取 / SQL 执行 / 结果映射），
//! 输出 Brendan Gregg 折叠格式（`flamegraph.pl` 兼容）与内联 SVG 火焰图。
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
//! 输出格式：
//! - [`render::to_brendan_gregg`]：折叠栈格式，可直接喂给 `flamegraph.pl`
//! - [`render::to_svg`]：自绘内联 SVG（无外部依赖）
//!
//! `query-flamegraph` feature 启用后，可通过 [`collector::QueryTracer::with_tracer`]
//! 将阶段耗时写入既有 `sz-orm-tracing` 的 `Tracer` span。

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
