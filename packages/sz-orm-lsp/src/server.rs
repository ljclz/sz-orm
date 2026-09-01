//! LSP 服务端实现
//!
//! 简化的 LSP 协议处理，提供 completion + hover + diagnostics。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// LSP 位置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspPosition {
    pub line: u32,
    pub character: u32,
}

/// LSP 范围
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspRange {
    pub start: LspPosition,
    pub end: LspPosition,
}

/// 诊断严重级别
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Information,
    Hint,
}

/// 诊断信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub range: LspRange,
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub source: String,
}

/// Completion 项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionItem {
    pub label: String,
    pub kind: u32,
    pub detail: Option<String>,
    pub documentation: Option<String>,
    pub insert_text: Option<String>,
}

/// Completion 列表
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionList {
    pub is_incomplete: bool,
    pub items: Vec<CompletionItem>,
}

/// Hover 信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hover {
    pub contents: String,
    pub range: Option<LspRange>,
}

/// 文本文档
#[derive(Debug, Clone)]
pub struct LspTextDocument {
    pub uri: String,
    pub text: String,
    pub version: i32,
}

impl LspTextDocument {
    pub fn new(uri: impl Into<String>, text: impl Into<String>, version: i32) -> Self {
        Self {
            uri: uri.into(),
            text: text.into(),
            version,
        }
    }

    pub fn get_line(&self, line: u32) -> Option<&str> {
        self.text.lines().nth(line as usize)
    }
}

/// LSP 服务端
///
/// 处理 LSP 协议请求：completion + hover + diagnostics。
pub struct LspServer {
    documents: HashMap<String, LspTextDocument>,
    schema_keywords: Vec<String>,
}

impl Default for LspServer {
    fn default() -> Self {
        Self::new()
    }
}

impl LspServer {
    /// 创建 LSP 服务端
    pub fn new() -> Self {
        Self {
            documents: HashMap::new(),
            schema_keywords: vec![
                "SELECT".to_string(),
                "FROM".to_string(),
                "WHERE".to_string(),
                "JOIN".to_string(),
                "INNER JOIN".to_string(),
                "LEFT JOIN".to_string(),
                "RIGHT JOIN".to_string(),
                "GROUP BY".to_string(),
                "ORDER BY".to_string(),
                "LIMIT".to_string(),
                "INSERT INTO".to_string(),
                "UPDATE".to_string(),
                "DELETE FROM".to_string(),
                "CREATE TABLE".to_string(),
                "ALTER TABLE".to_string(),
                "DROP TABLE".to_string(),
                "sz_orm::query".to_string(),
                "sz_orm::model".to_string(),
                "sz_orm::pool".to_string(),
                "where_eq".to_string(),
                "where_like".to_string(),
                "where_in".to_string(),
                "detect_n_plus_one".to_string(),
            ],
        }
    }

    /// 打开/更新文档
    pub fn did_open(&mut self, document: LspTextDocument) {
        self.documents.insert(document.uri.clone(), document);
    }

    /// 获取文档
    pub fn get_document(&self, uri: &str) -> Option<&LspTextDocument> {
        self.documents.get(uri)
    }

    /// textDocument/completion
    pub fn completion(&self, uri: &str, position: &LspPosition) -> CompletionList {
        let doc = match self.documents.get(uri) {
            Some(d) => d,
            None => {
                return CompletionList {
                    is_incomplete: false,
                    items: vec![],
                }
            }
        };

        let line = doc.get_line(position.line).unwrap_or("");
        let prefix = &line[..position.character as usize];

        let last_word: String = prefix
            .rsplit(|c: char| c.is_whitespace() || c == '.')
            .next()
            .unwrap_or("")
            .to_string();

        let items: Vec<CompletionItem> = self
            .schema_keywords
            .iter()
            .filter(|kw| {
                last_word.is_empty() || kw.to_lowercase().starts_with(&last_word.to_lowercase())
            })
            .map(|kw| CompletionItem {
                label: kw.clone(),
                kind: 1,
                detail: Some(format!("SZ-ORM keyword: {}", kw)),
                documentation: Some(self.get_keyword_doc(kw)),
                insert_text: Some(kw.clone()),
            })
            .collect();

        CompletionList {
            is_incomplete: false,
            items,
        }
    }

    /// textDocument/hover
    pub fn hover(&self, uri: &str, position: &LspPosition) -> Option<Hover> {
        let doc = self.documents.get(uri)?;
        let line = doc.get_line(position.line)?;
        let word = self.get_word_at_position(line, position.character);

        if word.is_empty() {
            return None;
        }

        let doc_text = self.get_keyword_doc(&word);
        if doc_text.is_empty() {
            return None;
        }

        Some(Hover {
            contents: doc_text,
            range: None,
        })
    }

    /// textDocument/diagnostics
    pub fn diagnostics(&self, uri: &str) -> Vec<Diagnostic> {
        let doc = match self.documents.get(uri) {
            Some(d) => d,
            None => return vec![],
        };

        let mut diagnostics = Vec::new();

        for (line_idx, line) in doc.text.lines().enumerate() {
            self.check_select_star(line, line_idx as u32, &mut diagnostics);
            self.check_n_plus_one(line, line_idx as u32, &mut diagnostics);
            self.check_sql_injection(line, line_idx as u32, &mut diagnostics);
        }

        diagnostics
    }

    /// 处理 JSON-RPC 请求，返回 JSON-RPC 响应
    pub fn handle_json_rpc(&mut self, request: &str) -> String {
        let v: serde_json::Value = match serde_json::from_str(request) {
            Ok(v) => v,
            Err(e) => {
                return serde_json::json!({
                    "jsonrpc": "2.0",
                    "error": {"code": -32700, "message": format!("Parse error: {}", e)}
                })
                .to_string();
            }
        };

        let id = v.get("id").cloned().unwrap_or(serde_json::Value::Null);
        let method = v.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let params = v.get("params").cloned().unwrap_or(serde_json::Value::Null);

        let result = match method {
            "initialize" => serde_json::json!({
                "capabilities": {
                    "completionProvider": {},
                    "hoverProvider": {},
                    "textDocumentSync": 1
                }
            }),
            "textDocument/didOpen" => {
                if let Some(td) = params.get("textDocument") {
                    let uri = td
                        .get("uri")
                        .and_then(|u| u.as_str())
                        .unwrap_or("")
                        .to_string();
                    let text = td
                        .get("text")
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .to_string();
                    let version = td.get("version").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                    self.did_open(LspTextDocument::new(uri, text, version));
                }
                serde_json::Value::Null
            }
            "textDocument/completion" => {
                let uri = params
                    .get("textDocument")
                    .and_then(|td| td.get("uri"))
                    .and_then(|u| u.as_str())
                    .unwrap_or("");
                let pos = params.get("position");
                let position = LspPosition {
                    line: pos
                        .and_then(|p| p.get("line"))
                        .and_then(|l| l.as_u64())
                        .unwrap_or(0) as u32,
                    character: pos
                        .and_then(|p| p.get("character"))
                        .and_then(|c| c.as_u64())
                        .unwrap_or(0) as u32,
                };
                serde_json::to_value(self.completion(uri, &position))
                    .unwrap_or(serde_json::Value::Null)
            }
            "textDocument/hover" => {
                let uri = params
                    .get("textDocument")
                    .and_then(|td| td.get("uri"))
                    .and_then(|u| u.as_str())
                    .unwrap_or("");
                let pos = params.get("position");
                let position = LspPosition {
                    line: pos
                        .and_then(|p| p.get("line"))
                        .and_then(|l| l.as_u64())
                        .unwrap_or(0) as u32,
                    character: pos
                        .and_then(|p| p.get("character"))
                        .and_then(|c| c.as_u64())
                        .unwrap_or(0) as u32,
                };
                serde_json::to_value(self.hover(uri, &position)).unwrap_or(serde_json::Value::Null)
            }
            _ => serde_json::Value::Null,
        };

        serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result
        })
        .to_string()
    }

    fn check_select_star(&self, line: &str, line_idx: u32, diagnostics: &mut Vec<Diagnostic>) {
        if line.to_uppercase().contains("SELECT *") {
            diagnostics.push(Diagnostic {
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
            });
        }
    }

    fn check_n_plus_one(&self, line: &str, line_idx: u32, diagnostics: &mut Vec<Diagnostic>) {
        if line.contains("for") && line.contains("query") && !line.contains("detect_n_plus_one") {
            diagnostics.push(Diagnostic {
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
                message: "可能的 N+1 查询：在循环中执行查询，建议添加 #[detect_n_plus_one] 检测"
                    .to_string(),
                source: "sz-orm-lsp".to_string(),
            });
        }
    }

    fn check_sql_injection(&self, line: &str, line_idx: u32, diagnostics: &mut Vec<Diagnostic>) {
        let lower = line.to_lowercase();
        if lower.contains("format!") && lower.contains("where") {
            diagnostics.push(Diagnostic {
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
                message: "可能的 SQL 注入：避免使用 format! 拼接 SQL，建议使用参数化查询"
                    .to_string(),
                source: "sz-orm-lsp".to_string(),
            });
        }
    }

    fn get_word_at_position(&self, line: &str, character: u32) -> String {
        let chars: Vec<char> = line.chars().collect();
        if chars.is_empty() {
            return String::new();
        }
        let pos = (character as usize).min(chars.len());
        if pos == 0 {
            return String::new();
        }

        let is_ident = |c: char| c.is_alphanumeric() || c == '_';

        let mut start = pos - 1;
        let mut end = pos;

        while start > 0 && is_ident(chars[start - 1]) {
            start -= 1;
        }
        while end < chars.len() && is_ident(chars[end]) {
            end += 1;
        }

        chars[start..end].iter().collect()
    }

    fn get_keyword_doc(&self, keyword: &str) -> String {
        match keyword.to_uppercase().as_str() {
            "SELECT" => "SELECT 语句用于查询数据。建议指定列名而非使用 *。".to_string(),
            "WHERE_EQ" => {
                "参数化等值查询：where_eq(\"column\", value)。防止 SQL 注入。".to_string()
            }
            "WHERE_LIKE" => "参数化 LIKE 查询：where_like(\"column\", pattern)。".to_string(),
            "WHERE_IN" => "参数化 IN 查询：where_in(\"column\", values)。".to_string(),
            "DETECT_N_PLUS_ONE" => "#[detect_n_plus_one] 编译期 N+1 查询静态检测。".to_string(),
            _ => {
                if self.schema_keywords.contains(&keyword.to_string()) {
                    format!("SZ-ORM 关键字: {}", keyword)
                } else {
                    String::new()
                }
            }
        }
    }
}
