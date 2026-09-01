//! TASK-028: LSP 协议测试
//!
//! 验证 completion + hover + diagnostics 响应。

use sz_orm_lsp::{DiagnosticSeverity, LspPosition, LspServer, LspTextDocument};

#[test]
fn test_completion_basic() {
    let mut server = LspServer::new();
    server.did_open(LspTextDocument::new(
        "file:///test.sql",
        "SELECT * FROM users",
        1,
    ));

    let result = server.completion(
        "file:///test.sql",
        &LspPosition {
            line: 0,
            character: 0,
        },
    );

    assert!(!result.items.is_empty());
    assert!(result.items.iter().any(|i| i.label == "SELECT"));
}

#[test]
fn test_completion_with_prefix() {
    let mut server = LspServer::new();
    server.did_open(LspTextDocument::new("file:///test.sql", "SEL", 1));

    let result = server.completion(
        "file:///test.sql",
        &LspPosition {
            line: 0,
            character: 3,
        },
    );

    assert!(result.items.iter().any(|i| i.label == "SELECT"));
}

#[test]
fn test_hover_keyword() {
    let mut server = LspServer::new();
    server.did_open(LspTextDocument::new("file:///test.rs", "where_eq", 1));

    let result = server.hover(
        "file:///test.rs",
        &LspPosition {
            line: 0,
            character: 7,
        },
    );

    assert!(result.is_some());
    let hover = result.unwrap();
    assert!(hover.contents.contains("参数化"));
}

#[test]
fn test_diagnostics_select_star() {
    let mut server = LspServer::new();
    server.did_open(LspTextDocument::new(
        "file:///test.sql",
        "SELECT * FROM users",
        1,
    ));

    let diags = server.diagnostics("file:///test.sql");

    assert!(diags
        .iter()
        .any(|d| d.severity == DiagnosticSeverity::Warning));
    assert!(diags.iter().any(|d| d.message.contains("SELECT *")));
}

#[test]
fn test_diagnostics_n_plus_one() {
    let mut server = LspServer::new();
    server.did_open(LspTextDocument::new(
        "file:///test.rs",
        "for user in users { query(user.id); }",
        1,
    ));

    let diags = server.diagnostics("file:///test.rs");

    assert!(diags.iter().any(|d| d.message.contains("N+1")));
}

#[test]
fn test_diagnostics_sql_injection() {
    let mut server = LspServer::new();
    server.did_open(LspTextDocument::new(
        "file:///test.rs",
        "let sql = format!(\"SELECT * FROM users WHERE name = '{}'\", input);",
        1,
    ));

    let diags = server.diagnostics("file:///test.rs");

    assert!(diags
        .iter()
        .any(|d| d.severity == DiagnosticSeverity::Error));
    assert!(diags.iter().any(|d| d.message.contains("SQL 注入")));
}

#[test]
fn test_diagnostics_clean_code() {
    let mut server = LspServer::new();
    server.did_open(LspTextDocument::new(
        "file:///test.rs",
        "let q = query::select(\"id\").from(\"users\").where_eq(\"status\", 1);",
        1,
    ));

    let diags = server.diagnostics("file:///test.rs");

    assert!(diags.is_empty());
}

#[test]
fn test_completion_no_document() {
    let server = LspServer::new();
    let result = server.completion(
        "file:///nonexistent",
        &LspPosition {
            line: 0,
            character: 0,
        },
    );
    assert!(result.items.is_empty());
}

#[test]
fn test_hover_no_document() {
    let server = LspServer::new();
    let result = server.hover(
        "file:///nonexistent",
        &LspPosition {
            line: 0,
            character: 0,
        },
    );
    assert!(result.is_none());
}

#[test]
fn test_diagnostics_no_document() {
    let server = LspServer::new();
    let diags = server.diagnostics("file:///nonexistent");
    assert!(diags.is_empty());
}

#[test]
fn test_text_document_get_line() {
    let doc = LspTextDocument::new("file:///test", "line0\nline1\nline2", 1);
    assert_eq!(doc.get_line(0), Some("line0"));
    assert_eq!(doc.get_line(1), Some("line1"));
    assert_eq!(doc.get_line(3), None);
}
