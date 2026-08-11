use std::sync::{Arc, RwLock};

use axum::extract::{Query, State};
use axum::response::Html;
use axum::routing::{get, post};
use axum::Json;
use axum::Router;
use serde::Deserialize;

use crate::design_ir::{DesignRelation, DesignTable, SchemaDesign};
use crate::designer::{DesignerError, SchemaDesigner};

use crate::exporter::{DesignerExporter, ExportFormat};

type SharedState = Arc<RwLock<SchemaDesigner>>;

pub struct SchemaDesignerWebUI {
    designer: SharedState,
    port: u16,
}

impl SchemaDesignerWebUI {
    pub fn new(designer: SchemaDesigner, port: u16) -> Self {
        Self {
            designer: Arc::new(RwLock::new(designer)),
            port,
        }
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub async fn start(&self) -> Result<(), DesignerError> {
        let app = build_router(self.designer.clone());
        let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", self.port))
            .await
            .map_err(|_| DesignerError::WebUiUnavailable)?;
        axum::serve(listener, app)
            .await
            .map_err(|_| DesignerError::WebUiUnavailable)?;
        Ok(())
    }
}

fn build_router(state: SharedState) -> Router {
    Router::new()
        .route("/", get(root_handler))
        .route("/api/design", get(get_design))
        .route("/api/design/table", post(add_table))
        .route("/api/design/relation", post(add_relation))
        .route("/api/preview-ddl", get(preview_ddl))
        .route("/api/export", get(export_handler))
        .with_state(state)
}

async fn root_handler() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn get_design(State(state): State<SharedState>) -> Json<SchemaDesign> {
    let designer = state.read().unwrap();
    Json(designer.design.clone())
}

async fn add_table(
    State(state): State<SharedState>,
    Json(table): Json<DesignTable>,
) -> Result<Json<SchemaDesign>, (axum::http::StatusCode, String)> {
    let mut designer = state.write().unwrap();
    designer
        .add_table(table)
        .map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e.to_string()))?;
    Ok(Json(designer.design.clone()))
}

async fn add_relation(
    State(state): State<SharedState>,
    Json(rel): Json<DesignRelation>,
) -> Result<Json<SchemaDesign>, (axum::http::StatusCode, String)> {
    let mut designer = state.write().unwrap();
    designer
        .add_relation(rel)
        .map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e.to_string()))?;
    Ok(Json(designer.design.clone()))
}

async fn preview_ddl(
    State(state): State<SharedState>,
) -> Result<Json<Vec<String>>, (axum::http::StatusCode, String)> {
    let designer = state.read().unwrap();
    let ddl = designer
        .preview_ddl()
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(ddl))
}

#[derive(Deserialize)]
struct ExportQueryParams {
    format: Option<String>,
}

async fn export_handler(
    State(state): State<SharedState>,
    Query(params): Query<ExportQueryParams>,
) -> Result<Vec<u8>, (axum::http::StatusCode, String)> {
    let designer = state.read().unwrap();
    let format = match params.format.as_deref() {
        Some("svg") => ExportFormat::ErSvg,
        Some("json") => ExportFormat::JsonDesign,
        Some("ddl") => ExportFormat::DdlSql,
        Some("migration") => ExportFormat::Migration,
        Some("model") => ExportFormat::RustModel,
        _ => ExportFormat::ErSvg,
    };
    let result = DesignerExporter::export(&designer.design, format, designer.design.dialect)
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(result)
}

const INDEX_HTML: &str = r#"<!DOCTYPE html>
<html>
<head>
    <title>SZ-ORM Schema Designer</title>
    <style>
        body { font-family: sans-serif; margin: 0; padding: 20px; }
        #app { display: flex; gap: 20px; }
        #er-diagram { flex: 1; border: 1px solid #ccc; padding: 10px; min-height: 400px; }
        #sidebar { width: 400px; }
        #ddl-preview { background: #f5f5f5; padding: 10px; white-space: pre-wrap; min-height: 200px; }
        .fallback { color: #c00; }
    </style>
</head>
<body>
    <h1>SZ-ORM Schema Designer</h1>
    <div id="app">
        <div id="er-diagram">
            <svg id="er-svg" width="600" height="400"></svg>
        </div>
        <div id="sidebar">
            <div id="table-editor">
                <h2>Table Editor</h2>
                <p>Use the API endpoints to add tables and relations.</p>
                <ul>
                    <li>GET /api/design - Get current design</li>
                    <li>POST /api/design/table - Add table</li>
                    <li>POST /api/design/relation - Add relation</li>
                    <li>GET /api/preview-ddl - Preview DDL</li>
                    <li>GET /api/export?format=svg - Export</li>
                </ul>
            </div>
            <div id="ddl-preview">
                <h2>DDL Preview</h2>
                <pre id="ddl-output">Loading...</pre>
            </div>
        </div>
    </div>
    <noscript>
        <p class="fallback">JavaScript is required. Use CLI: sz-orm designer:export</p>
    </noscript>
    <script>
        async function loadDesign() {
            const res = await fetch('/api/design');
            const design = await res.json();
            return design;
        }
        async function loadDdl() {
            const res = await fetch('/api/preview-ddl');
            const ddl = await res.json();
            document.getElementById('ddl-output').textContent = ddl.join('\n');
        }
        async function loadErDiagram() {
            const res = await fetch('/api/export?format=svg');
            const svgText = await res.text();
            document.getElementById('er-svg').outerHTML = svgText;
        }
        async function init() {
            try {
                await loadDdl();
                await loadErDiagram();
            } catch (e) {
                document.getElementById('ddl-output').textContent = 'Error: ' + e.message;
            }
        }
        init();
    </script>
</body>
</html>"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design_ir::*;

    #[test]
    fn test_web_ui_new() {
        let design = SchemaDesign::new(Dialect::MySql);
        let designer = SchemaDesigner::new(design);
        let web_ui = SchemaDesignerWebUI::new(designer, 8080);
        assert_eq!(web_ui.port(), 8080);
    }

    #[test]
    fn test_html_contains_fallback() {
        assert!(INDEX_HTML.contains("fallback"));
        assert!(INDEX_HTML.contains("CLI"));
    }

    #[test]
    fn test_html_contains_api_docs() {
        assert!(INDEX_HTML.contains("/api/design"));
        assert!(INDEX_HTML.contains("/api/preview-ddl"));
        assert!(INDEX_HTML.contains("/api/export"));
    }
}
