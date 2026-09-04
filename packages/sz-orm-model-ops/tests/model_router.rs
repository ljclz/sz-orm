//! TASK-011 验证测试：模型路由策略

use sz_orm_model_ops::router::ModelRouter;
use sz_orm_model_ops::types::ModelRouterConfig;

#[test]
fn test_route_simple_query() {
    let router = ModelRouter::new(ModelRouterConfig::default());
    let model = router.route(0.1).unwrap();
    assert_eq!(model, "qwen-1.8b");
}

#[test]
fn test_route_medium_query() {
    let router = ModelRouter::new(ModelRouterConfig::default());
    let model = router.route(0.5).unwrap();
    assert_eq!(model, "qwen-7b");
}

#[test]
fn test_route_complex_query() {
    let router = ModelRouter::new(ModelRouterConfig::default());
    let model = router.route(0.9).unwrap();
    assert_eq!(model, "qwen-14b");
}

#[test]
fn test_route_boundary_values() {
    let router = ModelRouter::new(ModelRouterConfig::default());
    assert_eq!(router.route(0.0).unwrap(), "qwen-1.8b");
    assert_eq!(router.route(0.3).unwrap(), "qwen-7b");
    assert_eq!(router.route(0.7).unwrap(), "qwen-14b");
    assert_eq!(router.route(1.0).unwrap(), "qwen-14b");
}
