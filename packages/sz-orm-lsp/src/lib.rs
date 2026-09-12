//! # sz-orm-lsp — VS Code 扩展 LSP 服务端
//!
//! 提供 LSP 协议处理：textDocument/completion + hover + diagnostics。
//! 复用 sz-orm-cli 的 AI 建议能力。

pub mod completion;
pub mod definition;
pub mod diagnostics;
pub mod server;

pub use completion::{analyze_context, context_aware_completion, CompletionContext};
pub use definition::{DefinitionProvider, Location, ModelDefinition, ModelField};
pub use diagnostics::{CodedDiagnostic, DiagnosticCode, IncrementalDiagnostics};
pub use server::{
    CompletionItem, CompletionList, Diagnostic, DiagnosticSeverity, Hover, LspPosition, LspRange,
    LspServer, LspTextDocument,
};
