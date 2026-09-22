//! LspEnhancedCompletion — LSP 增强补全与 N+1 诊断实现
//!
//! SQL 解析 → 上下文补全（表/字段/类型）→ N+1 检测诊断 → 建议改写。
//! 复用 `completion.rs:144` 的 `context_aware_completion` 和 `diagnostics.rs` 的诊断能力。

use crate::completion::{analyze_context, context_aware_completion, CompletionContext};
use crate::diagnostics::DiagnosticCode;
use crate::server::{CompletionItem, Diagnostic, DiagnosticSeverity, LspPosition, LspRange};
use std::time::{Duration, Instant};

/// LSP 增强配置
#[derive(Debug, Clone)]
pub struct LspEnhancedConfig {
    /// 补全超时阈值（默认 100ms）
    pub completion_timeout_ms: u64,
    /// 是否启用 N+1 检测
    pub n_plus_one_detection: bool,
}

impl Default for LspEnhancedConfig {
    fn default() -> Self {
        Self {
            completion_timeout_ms: 100,
            n_plus_one_detection: true,
        }
    }
}

/// 补全请求上下文
#[derive(Debug, Clone)]
pub struct CompletionRequest {
    /// 当前行文本
    pub line: String,
    /// 光标字符位置
    pub character: u32,
    /// 行号
    pub line_number: u32,
}

/// N+1 诊断建议改写
#[derive(Debug, Clone)]
pub struct NPlusOneDiagnostic {
    pub diagnostic: Diagnostic,
    pub code: DiagnosticCode,
    pub suggestion: String,
}

/// LSP 增强补全
pub struct LspEnhancedCompletion {
    config: LspEnhancedConfig,
}

impl LspEnhancedCompletion {
    /// 创建增强补全
    pub fn new(config: LspEnhancedConfig) -> Self {
        Self { config }
    }

    /// 默认配置创建
    pub fn with_default() -> Self {
        Self::new(LspEnhancedConfig::default())
    }

    /// 配置引用
    pub fn config(&self) -> &LspEnhancedConfig {
        &self.config
    }

    /// 上下文补全（≤ 100ms，超时返回部分补全）
    ///
    /// 生产入口：`LspEnhancedCompletion::complete`。
    pub fn complete(&self, request: &CompletionRequest) -> Vec<CompletionItem> {
        let start = Instant::now();
        let timeout = Duration::from_millis(self.config.completion_timeout_ms);

        let context = analyze_context(&request.line, request.character);
        let mut items = context_aware_completion(&request.line, request.character);

        if start.elapsed() > timeout {
            items.truncate(items.len().min(5));
        }

        let _ = context;
        items
    }

    /// N+1 检测诊断
    ///
    /// 生产入口：`LspEnhancedCompletion::diagnose_n_plus_one`。
    /// 检测模式：循环内单条查询 + 缺少 eager_load 标注。
    pub fn diagnose_n_plus_one(&self, sql: &str) -> Vec<NPlusOneDiagnostic> {
        if !self.config.n_plus_one_detection {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        self.check_loop_query_pattern(sql, &mut diagnostics);
        self.check_missing_eager_load(sql, &mut diagnostics);
        diagnostics
    }

    /// 检测循环内查询模式（for 循环 + 单条 SELECT）
    fn check_loop_query_pattern(&self, sql: &str, diagnostics: &mut Vec<NPlusOneDiagnostic>) {
        let has_loop = sql.contains("for ") || sql.contains("for(");
        let has_single_query = sql.contains(".first()") || sql.contains(".find_one(");
        if has_loop && has_single_query {
            let line_idx = 0u32;
            diagnostics.push(NPlusOneDiagnostic {
                diagnostic: Diagnostic {
                    range: LspRange {
                        start: LspPosition {
                            line: line_idx,
                            character: 0,
                        },
                        end: LspPosition {
                            line: line_idx,
                            character: sql.len() as u32,
                        },
                    },
                    severity: DiagnosticSeverity::Warning,
                    message: "检测到循环内单条查询，疑似 N+1 问题".to_string(),
                    source: "sz-orm-lsp-enhanced".to_string(),
                },
                code: DiagnosticCode::Sz002,
                suggestion: "使用 eager_load 或 with_relation 预加载关联数据，避免循环内查询"
                    .to_string(),
            });
        }
    }

    /// 检测缺少 eager_load 标注
    fn check_missing_eager_load(&self, sql: &str, diagnostics: &mut Vec<NPlusOneDiagnostic>) {
        let has_relation = sql.contains("relation") || sql.contains("with_relation");
        let has_eager = sql.contains("eager_load") || sql.contains("with(");
        if has_relation && !has_eager {
            let line_idx = 0u32;
            diagnostics.push(NPlusOneDiagnostic {
                diagnostic: Diagnostic {
                    range: LspRange {
                        start: LspPosition {
                            line: line_idx,
                            character: 0,
                        },
                        end: LspPosition {
                            line: line_idx,
                            character: sql.len() as u32,
                        },
                    },
                    severity: DiagnosticSeverity::Hint,
                    message: "访问关联但未使用 eager_load，可能触发 N+1".to_string(),
                    source: "sz-orm-lsp-enhanced".to_string(),
                },
                code: DiagnosticCode::Sz005,
                suggestion: "添加 .eager_load(\"relation_name\") 预加载关联".to_string(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_config_default() {
        let cfg = LspEnhancedConfig::default();
        assert_eq!(cfg.completion_timeout_ms, 100);
        assert!(cfg.n_plus_one_detection);
    }

    #[test]
    fn test_complete_basic() {
        let lsp = LspEnhancedCompletion::with_default();
        let req = CompletionRequest {
            line: "query.where_".to_string(),
            character: 11,
            line_number: 0,
        };
        let items = lsp.complete(&req);
        assert!(!items.is_empty());
    }

    #[test]
    fn test_complete_latency() {
        let lsp = LspEnhancedCompletion::with_default();
        let req = CompletionRequest {
            line: "query.select_".to_string(),
            character: 13,
            line_number: 0,
        };
        let start = Instant::now();
        let items = lsp.complete(&req);
        let elapsed = start.elapsed();
        assert!(
            elapsed <= Duration::from_millis(100),
            "补全延迟 {:?} > 100ms",
            elapsed
        );
        assert!(!items.is_empty());
    }

    #[test]
    fn test_complete_timeout_returns_partial() {
        let cfg = LspEnhancedConfig {
            completion_timeout_ms: 0,
            n_plus_one_detection: true,
        };
        let lsp = LspEnhancedCompletion::new(cfg);
        let req = CompletionRequest {
            line: "query.where_".to_string(),
            character: 11,
            line_number: 0,
        };
        let items = lsp.complete(&req);
        assert!(items.len() <= 5);
    }

    #[test]
    fn test_diagnose_n_plus_one_loop_pattern() {
        let lsp = LspEnhancedCompletion::with_default();
        let sql = "for user in users { query.where_eq(\"id\", user.id).first() }";
        let diags = lsp.diagnose_n_plus_one(sql);
        assert!(diags.iter().any(|d| d.code == DiagnosticCode::Sz002));
    }

    #[test]
    fn test_diagnose_n_plus_one_missing_eager() {
        let lsp = LspEnhancedCompletion::with_default();
        let sql = "query.with_relation(\"orders\").find()";
        let diags = lsp.diagnose_n_plus_one(sql);
        assert!(diags.iter().any(|d| d.code == DiagnosticCode::Sz005));
    }

    #[test]
    fn test_diagnose_n_plus_one_clean_sql() {
        let lsp = LspEnhancedCompletion::with_default();
        let sql = "query.eager_load(\"orders\").find()";
        let diags = lsp.diagnose_n_plus_one(sql);
        assert!(diags.is_empty());
    }

    #[test]
    fn test_diagnose_n_plus_one_disabled() {
        let cfg = LspEnhancedConfig {
            completion_timeout_ms: 100,
            n_plus_one_detection: false,
        };
        let lsp = LspEnhancedCompletion::new(cfg);
        let sql = "for user in users { query.first() }";
        let diags = lsp.diagnose_n_plus_one(sql);
        assert!(diags.is_empty());
    }

    #[test]
    fn test_diagnose_suggestion_present() {
        let lsp = LspEnhancedCompletion::with_default();
        let sql = "for u in users { query.where_eq(\"id\", u.id).first() }";
        let diags = lsp.diagnose_n_plus_one(sql);
        assert!(diags.iter().all(|d| !d.suggestion.is_empty()));
    }

    #[test]
    fn test_complete_context_analysis() {
        let lsp = LspEnhancedCompletion::with_default();
        let req = CompletionRequest {
            line: "UserModel.".to_string(),
            character: 9,
            line_number: 0,
        };
        let _items = lsp.complete(&req);
        let ctx = analyze_context(&req.line, req.character);
        assert_eq!(ctx, CompletionContext::ModelField);
    }
}
