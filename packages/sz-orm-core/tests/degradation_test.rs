#![cfg(feature = "circuit-breaker")]

//! v7.5.0 DegradationHandler 端到端测试

use std::collections::HashMap;
use sz_orm_core::degradation::{
    execute_with_degradation, CacheDegradation, DefaultDegradation, DegradationHandler,
    DegradationResult, DegradationStrategy, FastFailDegradation,
};

#[test]
fn test_cache_degradation_hit() {
    let mut handler = CacheDegradation::new();
    let mut row = HashMap::new();
    row.insert("id".into(), "42".into());
    handler.insert("query_1".into(), vec![row]);
    let result = handler.handle("query_1").unwrap();
    assert!(result.is_degraded);
    assert_eq!(
        result.degradation_strategy,
        DegradationStrategy::ReturnCache
    );
    assert!(result.data.is_some());
    assert_eq!(result.data.unwrap().len(), 1);
}

#[test]
fn test_cache_degradation_miss() {
    let handler = CacheDegradation::new();
    let result = handler.handle("nonexistent");
    assert!(result.is_err());
}

#[test]
fn test_default_degradation() {
    let handler = DefaultDegradation::new();
    let result = handler.handle("any_query").unwrap();
    assert!(result.is_degraded);
    assert_eq!(
        result.degradation_strategy,
        DegradationStrategy::ReturnDefault
    );
    assert!(result.data.is_some());
    assert!(result.data.unwrap().is_empty());
}

#[test]
fn test_fast_fail_degradation() {
    let handler = FastFailDegradation::new();
    let result = handler.handle("any_query");
    assert!(result.is_err());
}

#[test]
fn test_execute_with_degradation_normal_path() {
    let handler = DefaultDegradation::new();
    let result = execute_with_degradation(true, &handler, "query_1", || Ok(vec![HashMap::new()]));
    assert!(result.is_ok());
    let r = result.unwrap();
    assert!(!r.is_degraded);
    assert!(r.data.is_some());
}

#[test]
fn test_execute_with_degradation_circuit_open() {
    let handler = DefaultDegradation::new();
    let result = execute_with_degradation(false, &handler, "query_1", || Ok(vec![HashMap::new()]));
    assert!(result.is_ok());
    let r = result.unwrap();
    assert!(r.is_degraded);
}

#[test]
fn test_execute_with_degradation_fast_fail() {
    let handler = FastFailDegradation::new();
    let result = execute_with_degradation(false, &handler, "query_1", || Ok(vec![]));
    assert!(result.is_err());
}

#[test]
fn test_degradation_result_from_cache() {
    let result = DegradationResult::from_cache(vec![HashMap::new()]);
    assert!(result.is_degraded);
    assert_eq!(
        result.degradation_strategy,
        DegradationStrategy::ReturnCache
    );
    assert!(!result.mismatch_warning);
}

#[test]
fn test_degradation_result_with_mismatch_warning() {
    let result = DegradationResult::from_default().with_mismatch_warning();
    assert!(result.mismatch_warning);
}

#[test]
fn test_strategy_as_str() {
    assert_eq!(DegradationStrategy::ReturnCache.as_str(), "return_cache");
    assert_eq!(
        DegradationStrategy::ReturnDefault.as_str(),
        "return_default"
    );
    assert_eq!(DegradationStrategy::FastFail.as_str(), "fast_fail");
}
