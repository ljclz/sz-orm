# sz-orm-lsp

> Language Server Protocol server for sz-orm query DSL — v5.1.0

## Overview

`sz-orm-lsp` provides a simplified LSP server offering completion, hover, and diagnostics for sz-orm query builder DSL. It does not depend on `tower-lsp`; instead it implements a minimal JSON-RPC protocol handler suitable for embedding in editors.

## Features

- **Completion**: `completion()` — suggests query builder methods (`where_eq`, `select`, `join`, etc.)
- **Hover**: `hover()` — shows type signatures and documentation for symbols
- **Diagnostics**: `diagnostics()` — detects common query mistakes (SELECT *, missing WHERE on UPDATE/DELETE)
- **Document tracking**: `LspTextDocument` — tracks open documents with versioning

## Quick Start

```rust
use sz_orm_lsp::server::{LspServer, LspTextDocument};

let mut server = LspServer::new();
server.open_document("file:///query.rs".into(), 1,
    "Query::new().select(\"*\").from(\"users\")".into());
let diags = server.diagnostics("file:///query.rs");
assert!(!diags.is_empty()); // SELECT * warning
```

## Architecture

- `server.rs` — `LspServer` + `CompletionItem` + `Hover` + `Diagnostic` + `LspTextDocument`
- Dependencies: serde, serde_json (no tower-lsp dependency)

## Tests

11 unit tests in `tests/lsp_protocol_test.rs` covering completion, hover, diagnostics, and document lifecycle.