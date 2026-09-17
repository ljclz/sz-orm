//! v7.3.0 任务 4.2：warp 中间件适配层（`warp-adapt` feature gate）
//!
//! 提供泛型 trait 适配层，**不直接依赖 warp crate**。
//! 用户项目自行实现 warp Filter 装配，本模块提供 WarpAdapter 辅助类型与泛型 trait。
//!
//! # 设计
//!
//! - `WarpAdapter`：辅助类型，持有中间件特性列表
//! - `WarpMiddleware`：泛型 trait，用户实现具体 warp Filter 装配
//! - 五个适配方法：pool_inject/transaction/rate_limit/tracing/health_endpoint
//!
//! # 示例
//!
//! ```ignore
//! use sz_orm_axum::warp::{WarpAdapter, WarpMiddleware, WarpFilter};
//! use sz_orm_core::{MiddlewareFeature, Pool};
//!
//! struct MyWarpMiddleware {
//!     pool: Pool,
//! }
//!
//! impl WarpMiddleware for MyWarpMiddleware {
//!     type Filter = (); // 用户项目中为具体 warp::Filter
//!     fn pool_inject(&self) -> Self::Filter { () }
//!     fn transaction(&self) -> Self::Filter { () }
//!     fn rate_limit(&self, threshold: f64) -> Self::Filter { () }
//!     fn tracing(&self, sample_rate: f64) -> Self::Filter { () }
//!     fn health_endpoint(&self) -> Self::Filter { () }
//! }
//!
//! let adapter = WarpAdapter::new(vec![MiddlewareFeature::PoolInject]);
//! let middleware = MyWarpMiddleware { pool };
//! let filter = adapter.assemble(&middleware);
//! ```

use sz_orm_core::MiddlewareFeature;

/// warp Filter 类型别名（泛型，用户项目中绑定具体 warp::filter::Filter）
///
/// 本模块不依赖 warp，此 trait 由用户项目实现。
pub trait WarpMiddleware: Send + Sync {
    /// 用户项目中绑定的 Filter 类型
    type Filter;

    /// 连接池注入 Filter
    fn pool_inject(&self) -> Self::Filter;

    /// 事务 Filter
    fn transaction(&self) -> Self::Filter;

    /// 限流 Filter
    fn rate_limit(&self, threshold: f64) -> Self::Filter;

    /// 追踪 Filter
    fn tracing(&self, sample_rate: f64) -> Self::Filter;

    /// 健康端点 Filter
    fn health_endpoint(&self) -> Self::Filter;
}

/// WarpAdapter 辅助类型（v7.3.0 任务 4.2）
///
/// 持有中间件特性列表，提供五个适配方法与 assemble 装配入口。
/// 不直接依赖 warp crate，泛型 trait 由用户项目实现。
#[derive(Debug, Clone)]
pub struct WarpAdapter {
    /// 启用的中间件特性列表
    features: Vec<MiddlewareFeature>,
    /// 限流阈值（每秒请求数）
    rate_limit_threshold: f64,
    /// 追踪采样率 ∈ [0,1]
    trace_sample_rate: f64,
}

impl WarpAdapter {
    /// 创建 WarpAdapter
    pub fn new(features: Vec<MiddlewareFeature>) -> Self {
        Self {
            features,
            rate_limit_threshold: 100.0,
            trace_sample_rate: 1.0,
        }
    }

    /// 设置限流阈值
    pub fn with_rate_limit_threshold(mut self, threshold: f64) -> Self {
        self.rate_limit_threshold = threshold;
        self
    }

    /// 设置追踪采样率
    pub fn with_trace_sample_rate(mut self, rate: f64) -> Self {
        self.trace_sample_rate = rate;
        self
    }

    /// 启用的中间件特性列表
    pub fn features(&self) -> &[MiddlewareFeature] {
        &self.features
    }

    /// 连接池注入适配
    pub fn pool_inject_adapter<M: WarpMiddleware>(&self, middleware: &M) -> Option<M::Filter> {
        if self.features.contains(&MiddlewareFeature::PoolInject) {
            Some(middleware.pool_inject())
        } else {
            None
        }
    }

    /// 事务适配
    pub fn transaction_adapter<M: WarpMiddleware>(&self, middleware: &M) -> Option<M::Filter> {
        if self.features.contains(&MiddlewareFeature::Transaction) {
            Some(middleware.transaction())
        } else {
            None
        }
    }

    /// 限流适配
    pub fn rate_limit_adapter<M: WarpMiddleware>(&self, middleware: &M) -> Option<M::Filter> {
        if self.features.contains(&MiddlewareFeature::RateLimit) {
            Some(middleware.rate_limit(self.rate_limit_threshold))
        } else {
            None
        }
    }

    /// 追踪适配
    pub fn tracing_adapter<M: WarpMiddleware>(&self, middleware: &M) -> Option<M::Filter> {
        if self.features.contains(&MiddlewareFeature::Tracing) {
            Some(middleware.tracing(self.trace_sample_rate))
        } else {
            None
        }
    }

    /// 健康端点适配
    pub fn health_endpoint_adapter<M: WarpMiddleware>(&self, middleware: &M) -> Option<M::Filter> {
        if self.features.contains(&MiddlewareFeature::HealthEndpoint) {
            Some(middleware.health_endpoint())
        } else {
            None
        }
    }

    /// 装配所有启用的中间件，返回 Filter 列表（按 features 顺序）
    pub fn assemble<M: WarpMiddleware>(&self, middleware: &M) -> Vec<M::Filter> {
        let mut filters = Vec::new();
        for feature in &self.features {
            let filter = match feature {
                MiddlewareFeature::PoolInject => middleware.pool_inject(),
                MiddlewareFeature::Transaction => middleware.transaction(),
                MiddlewareFeature::RateLimit => middleware.rate_limit(self.rate_limit_threshold),
                MiddlewareFeature::Tracing => middleware.tracing(self.trace_sample_rate),
                MiddlewareFeature::HealthEndpoint => middleware.health_endpoint(),
            };
            filters.push(filter);
        }
        filters
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试用 WarpMiddleware 实现（Filter 类型为 String，模拟 Filter 标识）
    struct TestMiddleware;

    impl WarpMiddleware for TestMiddleware {
        type Filter = String;

        fn pool_inject(&self) -> Self::Filter {
            "pool_inject".to_string()
        }

        fn transaction(&self) -> Self::Filter {
            "transaction".to_string()
        }

        fn rate_limit(&self, threshold: f64) -> Self::Filter {
            format!("rate_limit:{}", threshold)
        }

        fn tracing(&self, sample_rate: f64) -> Self::Filter {
            format!("tracing:{}", sample_rate)
        }

        fn health_endpoint(&self) -> Self::Filter {
            "health_endpoint".to_string()
        }
    }

    #[test]
    fn test_warp_adapter_new() {
        let adapter = WarpAdapter::new(vec![MiddlewareFeature::PoolInject]);
        assert_eq!(adapter.features().len(), 1);
    }

    #[test]
    fn test_pool_inject_adapter_enabled() {
        let adapter = WarpAdapter::new(vec![MiddlewareFeature::PoolInject]);
        let mw = TestMiddleware;
        let filter = adapter.pool_inject_adapter(&mw);
        assert_eq!(filter, Some("pool_inject".to_string()));
    }

    #[test]
    fn test_pool_inject_adapter_disabled() {
        let adapter = WarpAdapter::new(vec![]);
        let mw = TestMiddleware;
        let filter = adapter.pool_inject_adapter(&mw);
        assert!(filter.is_none());
    }

    #[test]
    fn test_transaction_adapter() {
        let adapter = WarpAdapter::new(vec![MiddlewareFeature::Transaction]);
        let mw = TestMiddleware;
        assert_eq!(
            adapter.transaction_adapter(&mw),
            Some("transaction".to_string())
        );
    }

    #[test]
    fn test_rate_limit_adapter() {
        let adapter =
            WarpAdapter::new(vec![MiddlewareFeature::RateLimit]).with_rate_limit_threshold(200.0);
        let mw = TestMiddleware;
        assert_eq!(
            adapter.rate_limit_adapter(&mw),
            Some("rate_limit:200".to_string())
        );
    }

    #[test]
    fn test_tracing_adapter() {
        let adapter =
            WarpAdapter::new(vec![MiddlewareFeature::Tracing]).with_trace_sample_rate(0.5);
        let mw = TestMiddleware;
        assert_eq!(
            adapter.tracing_adapter(&mw),
            Some("tracing:0.5".to_string())
        );
    }

    #[test]
    fn test_health_endpoint_adapter() {
        let adapter = WarpAdapter::new(vec![MiddlewareFeature::HealthEndpoint]);
        let mw = TestMiddleware;
        assert_eq!(
            adapter.health_endpoint_adapter(&mw),
            Some("health_endpoint".to_string())
        );
    }

    #[test]
    fn test_assemble_all() {
        let adapter = WarpAdapter::new(vec![
            MiddlewareFeature::PoolInject,
            MiddlewareFeature::Transaction,
            MiddlewareFeature::RateLimit,
            MiddlewareFeature::Tracing,
            MiddlewareFeature::HealthEndpoint,
        ]);
        let mw = TestMiddleware;
        let filters = adapter.assemble(&mw);
        assert_eq!(filters.len(), 5);
        assert_eq!(filters[0], "pool_inject");
        assert_eq!(filters[1], "transaction");
        assert!(filters[2].starts_with("rate_limit:"));
        assert!(filters[3].starts_with("tracing:"));
        assert_eq!(filters[4], "health_endpoint");
    }

    #[test]
    fn test_assemble_empty() {
        let adapter = WarpAdapter::new(vec![]);
        let mw = TestMiddleware;
        let filters = adapter.assemble(&mw);
        assert!(filters.is_empty());
    }

    #[test]
    fn test_warp_adapter_clone() {
        let adapter = WarpAdapter::new(vec![MiddlewareFeature::PoolInject]);
        let cloned = adapter.clone();
        assert_eq!(adapter.features(), cloned.features());
    }
}
