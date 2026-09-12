//! # 增量诊断推送（v6.9.0 REQ-DEV-003）
//!
//! 文档变更后增量解析，500ms debounce，诊断含唯一代码（SZ-001 等）。

use crate::server::{Diagnostic, DiagnosticSeverity, LspPosition, LspRange};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// 诊断代码
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticCode {
    /// SELECT * 警告
    Sz001,
    /// N+1 查询警告
    Sz002,
    /// SQL 注入错误
    Sz003,
    /// 缺少参数化查询
    Sz004,
    /// 未使用 detect_n_plus_one
    Sz005,
}

impl DiagnosticCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Sz001 => "SZ-001",
            Self::Sz002 => "SZ-002",
            Self::Sz003 => "SZ-003",
            Self::Sz004 => "SZ-004",
            Self::Sz005 => "SZ-005",
        }
    }
}

/// 带代码的诊断信息
#[derive(Debug, Clone)]
pub struct CodedDiagnostic {
    pub diagnostic: Diagnostic,
    pub code: DiagnosticCode,
    /// 修复建议（TextEdit）
    pub suggestion: Option<String>,
}

/// 增量诊断 provider
pub struct IncrementalDiagnostics {
    /// debounce 计时器
    last_change: HashMap<String, Instant>,
    /// debounce 间隔
    debounce_interval: Duration,
}

impl Default for IncrementalDiagnostics {
    fn default() -> Self {
        Self::new()
    }
}

impl IncrementalDiagnostics {
    /// 创建增量诊断 provider（默认 500ms debounce）
    pub fn new() -> Self {
        Self {
            last_change: HashMap::new(),
            debounce_interval: Duration::from_millis(500),
        }
    }

    /// 设置 debounce 间隔
    pub fn with_debounce(mut self, interval: Duration) -> Self {
        self.debounce_interval = interval;
        self
    }

    /// 文档变更事件
    ///
    /// 返回 `true` 表示 debounce 已过，应推送诊断；`false` 表示仍在 debounce 中。
    pub fn did_change(&mut self, uri: &str) -> bool {
        let now = Instant::now();
        let should_push = match self.last_change.get(uri) {
            Some(last) => now.duration_since(*last) >= self.debounce_interval,
            None => true,
        };
        self.last_change.insert(uri.to_string(), now);
        should_push
    }

    /// 对文档内容执行增量诊断
    pub fn diagnose(&self, text: &str) -> Vec<CodedDiagnostic> {
        let mut diagnostics = Vec::new();

        for (line_idx, line) in text.lines().enumerate() {
            self.check_select_star(line, line_idx as u32, &mut diagnostics);
            self.check_n_plus_one(line, line_idx as u32, &mut diagnostics);
            self.check_sql_injection(line, line_idx as u32, &mut diagnostics);
            self.check_format_where(line, line_idx as u32, &mut diagnostics);
        }

        diagnostics
    }

    fn check_select_star(&self, line: &str, line_idx: u32, diagnostics: &mut Vec<CodedDiagnostic>) {
        if line.to_uppercase().contains("SELECT *") {
            diagnostics.push(CodedDiagnostic {
                diagnostic: Diagnostic {
                    range: LspRange {
                        start: LspPosition {
                            line: line_idx,
                            character: 0,
                        },
                        end: LspPosition {
                            line: line_idx,
                            character: line.len() as u32,
                        },
                    },
                    severity: DiagnosticSeverity::Warning,
                    message: "避免使用 SELECT *，建议指定列名".to_string(),
                    source: "sz-orm-lsp".to_string(),
                },
                code: DiagnosticCode::Sz001,
                suggestion: Some("将 SELECT * 替换为 SELECT col1, col2, ...".to_string()),
            });
        }
    }

    fn check_n_plus_one(&self, line: &str, line_idx: u32, diagnostics: &mut Vec<CodedDiagnostic>) {
        if line.contains("for") && line.contains("query") && !line.contains("detect_n_plus_one") {
            diagnostics.push(CodedDiagnostic {
                diagnostic: Diagnostic {
                    range: LspRange {
                        start: LspPosition {
                            line: line_idx,
                            character: 0,
                        },
                        end: LspPosition {
                            line: line_idx,
                            character: line.len() as u32,
                        },
                    },
                    severity: DiagnosticSeverity::Warning,
                    message: "可能的 N+1 查询".to_string(),
                    source: "sz-orm-lsp".to_string(),
                },
                code: DiagnosticCode::Sz002,
                suggestion: Some("添加 #[detect_n_plus_one] 标注或使用批量查询".to_string()),
            });
        }
    }

    fn check_sql_injection(
        &self,
        line: &str,
        line_idx: u32,
        diagnostics: &mut Vec<CodedDiagnostic>,
    ) {
        let lower = line.to_lowercase();
        if lower.contains("format!") && lower.contains("where") {
            diagnostics.push(CodedDiagnostic {
                diagnostic: Diagnostic {
                    range: LspRange {
                        start: LspPosition {
                            line: line_idx,
                            character: 0,
                        },
                        end: LspPosition {
                            line: line_idx,
                            character: line.len() as u32,
                        },
                    },
                    severity: DiagnosticSeverity::Error,
                    message: "可能的 SQL 注入：避免 format! 拼接 SQL".to_string(),
                    source: "sz-orm-lsp".to_string(),
                },
                code: DiagnosticCode::Sz003,
                suggestion: Some("使用 where_eq(\"col\", value) 参数化查询".to_string()),
            });
        }
    }

    fn check_format_where(
        &self,
        line: &str,
        line_idx: u32,
        diagnostics: &mut Vec<CodedDiagnostic>,
    ) {
        if line.contains("where_cond") || line.contains("or_where") {
            diagnostics.push(CodedDiagnostic {
                diagnostic: Diagnostic {
                    range: LspRange {
                        start: LspPosition {
                            line: line_idx,
                            character: 0,
                        },
                        end: LspPosition {
                            line: line_idx,
                            character: line.len() as u32,
                        },
                    },
                    severity: DiagnosticSeverity::Warning,
                    message: "where_cond/or_where 已废弃，请使用 where_eq".to_string(),
                    source: "sz-orm-lsp".to_string(),
                },
                code: DiagnosticCode::Sz004,
                suggestion: Some("替换为 where_eq(\"col\", value)".to_string()),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diagnostic_code_as_str() {
        assert_eq!(DiagnosticCode::Sz001.as_str(), "SZ-001");
        assert_eq!(DiagnosticCode::Sz003.as_str(), "SZ-003");
    }

    #[test]
    fn test_debounce_first_change_pushes() {
        let mut provider = IncrementalDiagnostics::new();
        assert!(provider.did_change("file:///test.rs"));
    }

    #[test]
    fn test_debounce_rapid_change_suppressed() {
        let mut provider = IncrementalDiagnostics::new().with_debounce(Duration::from_secs(60));
        assert!(provider.did_change("file:///test.rs"));
        assert!(!provider.did_change("file:///test.rs"));
    }

    #[test]
    fn test_diagnose_select_star() {
        let provider = IncrementalDiagnostics::new();
        let diagnostics = provider.diagnose("SELECT * FROM users");
        assert!(diagnostics.iter().any(|d| d.code == DiagnosticCode::Sz001));
    }

    #[test]
    fn test_diagnose_n_plus_one() {
        let provider = IncrementalDiagnostics::new();
        let diagnostics = provider.diagnose("for u in users { query.find(u.id) }");
        assert!(diagnostics.iter().any(|d| d.code == DiagnosticCode::Sz002));
    }

    #[test]
    fn test_diagnose_sql_injection() {
        let provider = IncrementalDiagnostics::new();
        let diagnostics = provider.diagnose("format!(\"WHERE name = '{}'\", name)");
        assert!(diagnostics.iter().any(|d| d.code == DiagnosticCode::Sz003));
    }

    #[test]
    fn test_diagnose_deprecated_where_cond() {
        let provider = IncrementalDiagnostics::new();
        let diagnostics = provider.diagnose("query.where_cond(\"name = 'Alice'\")");
        assert!(diagnostics.iter().any(|d| d.code == DiagnosticCode::Sz004));
    }

    #[test]
    fn test_diagnose_clean_code() {
        let provider = IncrementalDiagnostics::new();
        let diagnostics = provider.diagnose("query.where_eq(\"name\", \"Alice\")");
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_coded_diagnostic_has_suggestion() {
        let provider = IncrementalDiagnostics::new();
        let diagnostics = provider.diagnose("SELECT * FROM users");
        assert!(diagnostics[0].suggestion.is_some());
    }

    #[test]
    fn test_diagnostic_severity_levels() {
        let provider = IncrementalDiagnostics::new();
        let diagnostics = provider.diagnose("format!(\"WHERE x = '{}'\", x)");
        assert_eq!(
            diagnostics[0].diagnostic.severity,
            DiagnosticSeverity::Error
        );
    }
}
