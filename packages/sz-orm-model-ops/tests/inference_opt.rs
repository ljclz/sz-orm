//! TASK-027 集成测试：推理优化端到端验证

use sz_orm_model_ops::inference_opt::{ConfigChoice, InferenceOptConfig, InferenceOptimizer};
use sz_orm_model_ops::types::Quantization;

#[test]
fn test_optimize_int4_quantization() {
    let optimizer = InferenceOptimizer::new(8000.0, 200.0);
    let config = InferenceOptConfig {
        quantization: Quantization::Int4,
        ..Default::default()
    };
    let result = optimizer.optimize(&config).unwrap();

    assert!(result.estimated_speedup > 2.0, "INT4 应显著提速");
    assert!(result.estimated_memory_mb < 4000.0, "INT4 应大幅减少内存");
    assert!(result.optimizations.iter().any(|o| o.contains("INT4")));
}

#[test]
fn test_optimize_int8_quantization() {
    let optimizer = InferenceOptimizer::new(8000.0, 200.0);
    let config = InferenceOptConfig {
        quantization: Quantization::Int8,
        ..Default::default()
    };
    let result = optimizer.optimize(&config).unwrap();

    assert!(result.estimated_speedup > 1.0, "INT8 应提速");
    assert!(result.optimizations.iter().any(|o| o.contains("INT8")));
}

#[test]
fn test_optimize_batch_processing() {
    let optimizer = InferenceOptimizer::new(8000.0, 200.0);
    let config = InferenceOptConfig {
        batch_size: 16,
        ..Default::default()
    };
    let result = optimizer.optimize(&config).unwrap();

    assert!(result.estimated_speedup > 1.0, "批处理应提速");
    assert!(result.optimizations.iter().any(|o| o.contains("批处理")));
}

#[test]
fn test_optimize_kv_cache() {
    let optimizer = InferenceOptimizer::new(8000.0, 200.0);
    let config = InferenceOptConfig {
        enable_kv_cache: true,
        ..Default::default()
    };
    let result = optimizer.optimize(&config).unwrap();

    assert!(result.optimizations.iter().any(|o| o.contains("KV Cache")));
}

#[test]
fn test_optimize_flash_attention() {
    let optimizer = InferenceOptimizer::new(8000.0, 200.0);
    let config = InferenceOptConfig {
        enable_flash_attention: true,
        ..Default::default()
    };
    let result = optimizer.optimize(&config).unwrap();

    assert!(result
        .optimizations
        .iter()
        .any(|o| o.contains("Flash Attention")));
}

#[test]
fn test_optimize_all_combined() {
    let optimizer = InferenceOptimizer::new(8000.0, 200.0);
    let config = InferenceOptConfig {
        quantization: Quantization::Int4,
        batch_size: 8,
        enable_kv_cache: true,
        enable_flash_attention: true,
        ..Default::default()
    };
    let result = optimizer.optimize(&config).unwrap();

    assert!(result.estimated_speedup > 5.0, "所有优化组合应大幅提速");
    assert!(result.optimizations.len() >= 4);
}

#[test]
fn test_compare_configs_picks_better() {
    let optimizer = InferenceOptimizer::new(8000.0, 200.0);
    let config_a = InferenceOptConfig {
        quantization: Quantization::Int4,
        enable_kv_cache: true,
        ..Default::default()
    };
    let config_b = InferenceOptConfig {
        quantization: Quantization::None,
        enable_kv_cache: false,
        ..Default::default()
    };
    let comparison = optimizer.compare_configs(&config_a, &config_b).unwrap();
    assert_eq!(comparison.better_config, ConfigChoice::A);
    assert!(comparison.speedup_ratio > 1.0);
}

#[test]
fn test_estimate_latency_reduced() {
    let optimizer = InferenceOptimizer::new(8000.0, 200.0);
    let config = InferenceOptConfig {
        quantization: Quantization::Int4,
        enable_kv_cache: true,
        enable_flash_attention: true,
        ..Default::default()
    };
    let latency = optimizer.estimate_latency(&config).unwrap();
    assert!(latency < 50.0, "优化后延迟应大幅降低: {}", latency);
}

#[test]
fn test_invalid_config_rejected() {
    let optimizer = InferenceOptimizer::new(8000.0, 200.0);

    let bad_batch = InferenceOptConfig {
        batch_size: 0,
        ..Default::default()
    };
    assert!(optimizer.optimize(&bad_batch).is_err());

    let bad_temp = InferenceOptConfig {
        temperature: 3.0,
        ..Default::default()
    };
    assert!(optimizer.optimize(&bad_temp).is_err());

    let bad_top_p = InferenceOptConfig {
        top_p: 1.5,
        ..Default::default()
    };
    assert!(optimizer.optimize(&bad_top_p).is_err());
}
