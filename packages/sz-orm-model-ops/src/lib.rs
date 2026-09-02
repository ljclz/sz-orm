//! AI model operations: local inference, routing, finetuning, evaluation.

#[cfg(feature = "model-ops")]
pub mod ab_test;
#[cfg(feature = "model-ops")]
pub mod evaluator;
#[cfg(feature = "model-ops")]
pub mod inference_opt;
#[cfg(feature = "model-ops-llamacpp")]
pub mod llamacpp;
#[cfg(feature = "model-ops-router")]
pub mod router;
#[cfg(feature = "model-ops")]
pub mod types;
#[cfg(feature = "model-ops-vllm")]
pub mod vllm;
