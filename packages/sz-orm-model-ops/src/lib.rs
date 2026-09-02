//! AI model operations: local inference, routing, finetuning, evaluation.

#![cfg_attr(not(feature = "model-ops"), allow(unused_imports))]

#[cfg(feature = "model-ops-llamacpp")]
pub mod llamacpp;
#[cfg(feature = "model-ops-router")]
pub mod router;
#[cfg(feature = "model-ops")]
pub mod types;
#[cfg(feature = "model-ops-vllm")]
pub mod vllm;
