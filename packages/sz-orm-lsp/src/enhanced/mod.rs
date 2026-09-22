//! v8.0.0 组 6：LSP 增强补全与 N+1 诊断（toolchain-lsp-enhance feature gate）
//!
//! 提供 `LspEnhancedCompletion`，复用既有 `completion.rs` 和 `diagnostics.rs`。

pub mod lsp_enhanced_completion;

pub use lsp_enhanced_completion::{
    CompletionRequest, LspEnhancedCompletion, LspEnhancedConfig, NPlusOneDiagnostic,
};
