//! v8.0.0 强一致缓存模块（`dist-cache-coherent` feature gate）
//!
//! 提供 `StrongConsistencyCache` 强一致缓存，复用既有 `cache_coherence.rs` 和 `dist_cache.rs`。

pub mod strong_consistency_cache;

pub use strong_consistency_cache::{DistError, StrongConsistencyCache, StrongConsistencyConfig};
