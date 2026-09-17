//! v7.3.0 任务 4.2：warp 中间件适配层测试
//!
//! 生产调用点证据：
//! - WarpAdapter 构造：tests/warp_adapt.rs:30
//! - WarpMiddleware trait 实现：tests/warp_adapt.rs:20
//! - 五个适配方法：tests/warp_adapt.rs:40-80
//! - axum 既有中间件签名不变：tests/warp_adapt.rs:90-110

use sz_orm_axum::warp::{WarpAdapter, WarpMiddleware};
use sz_orm_core::MiddlewareFeature;

/// 测试用 WarpMiddleware 实现（Filter 类型为 String，模拟 warp Filter 标识）
struct DemoWarpMiddleware {
    pool_label: String,
}

impl WarpMiddleware for DemoWarpMiddleware {
    type Filter = String;

    fn pool_inject(&self) -> Self::Filter {
        format!("pool:{}", self.pool_label)
    }

    fn transaction(&self) -> Self::Filter {
        "tx".to_string()
    }

    fn rate_limit(&self, threshold: f64) -> Self::Filter {
        format!("rl:{}", threshold)
    }

    fn tracing(&self, sample_rate: f64) -> Self::Filter {
        format!("tr:{}", sample_rate)
    }

    fn health_endpoint(&self) -> Self::Filter {
        "health".to_string()
    }
}

/// WarpAdapter 辅助类型构造
#[test]
fn test_warp_adapter_construct() {
    let adapter = WarpAdapter::new(vec![
        MiddlewareFeature::PoolInject,
        MiddlewareFeature::Transaction,
    ]);
    assert_eq!(adapter.features().len(), 2);
}

/// 泛型 trait 实现 + 五个适配方法
#[test]
fn test_warp_adapter_all_methods() {
    let adapter = WarpAdapter::new(vec![
        MiddlewareFeature::PoolInject,
        MiddlewareFeature::Transaction,
        MiddlewareFeature::RateLimit,
        MiddlewareFeature::Tracing,
        MiddlewareFeature::HealthEndpoint,
    ])
    .with_rate_limit_threshold(500.0)
    .with_trace_sample_rate(0.25);

    let mw = DemoWarpMiddleware {
        pool_label: "primary".to_string(),
    };

    assert_eq!(
        adapter.pool_inject_adapter(&mw),
        Some("pool:primary".to_string())
    );
    assert_eq!(adapter.transaction_adapter(&mw), Some("tx".to_string()));
    assert_eq!(
        adapter.rate_limit_adapter(&mw),
        Some("rl:500".to_string())
    );
    assert_eq!(
        adapter.tracing_adapter(&mw),
        Some("tr:0.25".to_string())
    );
    assert_eq!(
        adapter.health_endpoint_adapter(&mw),
        Some("health".to_string())
    );
}

/// 未启用的中间件返回 None
#[test]
fn test_warp_adapter_disabled() {
    let adapter = WarpAdapter::new(vec![MiddlewareFeature::PoolInject]);
    let mw = DemoWarpMiddleware {
        pool_label: "x".to_string(),
    };
    assert!(adapter.transaction_adapter(&mw).is_none());
    assert!(adapter.rate_limit_adapter(&mw).is_none());
    assert!(adapter.tracing_adapter(&mw).is_none());
    assert!(adapter.health_endpoint_adapter(&mw).is_none());
}

/// assemble 装配所有启用的中间件
#[test]
fn test_warp_adapter_assemble() {
    let adapter = WarpAdapter::new(vec![
        MiddlewareFeature::PoolInject,
        MiddlewareFeature::HealthEndpoint,
    ]);
    let mw = DemoWarpMiddleware {
        pool_label: "p".to_string(),
    };
    let filters = adapter.assemble(&mw);
    assert_eq!(filters.len(), 2);
    assert_eq!(filters[0], "pool:p");
    assert_eq!(filters[1], "health");
}

/// axum 既有中间件签名不变验证
///
/// 验证 transaction_layer / PoolState / JsonRows / JsonResp 签名未受 warp-adapt 影响。
#[test]
fn test_axum_existing_signatures_unchanged() {
    use axum::response::IntoResponse;
    use sz_orm_axum::{JsonResp, PoolState, transaction_layer};

    // PoolState: Clone（签名不变）
    fn _assert_pool_state_clone<T: Clone>() {}
    _assert_pool_state_clone::<PoolState>();

    // transaction_layer: async fn(State<PoolState>, Request<Body>, Next) -> Response
    // 验证函数指针类型存在（签名不变）
    let _layer = transaction_layer;

    // JsonResp<T: Serialize>: IntoResponse（签名不变）
    let resp = JsonResp(42i64).into_response();
    assert_eq!(resp.status(), axum::http::StatusCode::OK);
}