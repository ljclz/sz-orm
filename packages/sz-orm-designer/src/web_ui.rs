use std::sync::{Arc, RwLock};

use axum::extract::{Query, State};
use axum::response::Html;
use axum::routing::{get, post};
use axum::Json;
use axum::Router;
use serde::Deserialize;

use crate::design_ir::{
    Cardinality, ColumnType, DesignColumn, DesignRelation, DesignTable, SchemaDesign,
};
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
        .route("/api/design/table-simple", post(add_table_simple))
        .route("/api/design/relation", post(add_relation))
        .route("/api/design/relation-simple", post(add_relation_simple))
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

// ---------------------------------------------------------------------------
// 简化编辑接口（2026-10-27）：前端表单友好，后端解析列规格
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct SimpleTableRequest {
    name: String,
    /// 列规格："id:i64:pk:ai,name:string,email:string:nullable"
    columns: String,
}

#[derive(Deserialize)]
struct SimpleRelationRequest {
    from_table: String,
    to_table: String,
    from_column: String,
    to_column: String,
    /// "one_to_one" | "one_to_many" | "many_to_one" | "many_to_many"（默认 one_to_many）
    cardinality: Option<String>,
}

/// 解析简化列规格（`名称:类型[:pk][:ai][:nullable]`）为 `DesignColumn` 列表。
fn parse_simple_columns(spec: &str) -> Result<Vec<DesignColumn>, String> {
    let mut columns = Vec::new();
    for part in spec.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let mut segs = part.split(':');
        let name = segs.next().unwrap_or("").trim();
        if name.is_empty() {
            return Err(format!("列规格缺少列名: \"{}\"", part));
        }
        let type_str = segs.next().unwrap_or("string").trim().to_lowercase();
        let col_type = match type_str.as_str() {
            "i8" | "tinyint" => ColumnType::TinyInt,
            "i16" | "smallint" => ColumnType::SmallInt,
            "i32" | "int" | "integer" => ColumnType::Int,
            "i64" | "bigint" | "long" => ColumnType::BigInt,
            "string" | "varchar" => ColumnType::Varchar(Some(255)),
            "text" => ColumnType::Text,
            "bool" | "boolean" => ColumnType::Boolean,
            "f32" | "float" => ColumnType::Float,
            "f64" | "double" => ColumnType::Double,
            "date" => ColumnType::Date,
            "datetime" => ColumnType::DateTime,
            "timestamp" => ColumnType::Timestamp,
            "time" => ColumnType::Time,
            "json" => ColumnType::Json,
            "uuid" => ColumnType::Uuid,
            other => return Err(format!("不支持的列类型: \"{}\"（列 {}）", other, name)),
        };
        let mut col = DesignColumn::new(name, col_type);
        for flag in segs {
            match flag.trim().to_lowercase().as_str() {
                "pk" | "primary_key" => col = col.primary_key(),
                "ai" | "auto_increment" => col = col.auto_increment(),
                "nullable" => col = col.nullable(),
                "" => {}
                other => return Err(format!("不支持的列标记: \"{}\"（列 {}）", other, name)),
            }
        }
        columns.push(col);
    }
    if columns.is_empty() {
        return Err("列规格不能为空".to_string());
    }
    Ok(columns)
}

async fn add_table_simple(
    State(state): State<SharedState>,
    Json(req): Json<SimpleTableRequest>,
) -> Result<Json<SchemaDesign>, (axum::http::StatusCode, String)> {
    let columns =
        parse_simple_columns(&req.columns).map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e))?;
    let table = DesignTable::new(&req.name, columns);
    let mut designer = state.write().unwrap();
    designer
        .add_table(table)
        .map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e.to_string()))?;
    Ok(Json(designer.design.clone()))
}

async fn add_relation_simple(
    State(state): State<SharedState>,
    Json(req): Json<SimpleRelationRequest>,
) -> Result<Json<SchemaDesign>, (axum::http::StatusCode, String)> {
    let cardinality = match req.cardinality.as_deref().unwrap_or("one_to_many") {
        "one_to_one" => Cardinality::OneToOne,
        "one_to_many" => Cardinality::OneToMany,
        "many_to_one" => Cardinality::ManyToOne,
        "many_to_many" => Cardinality::ManyToMany,
        other => {
            return Err((
                axum::http::StatusCode::BAD_REQUEST,
                format!("不支持的基数: {}", other),
            ))
        }
    };
    let rel = DesignRelation {
        from_table: req.from_table,
        to_table: req.to_table,
        from_column: req.from_column,
        to_column: req.to_column,
        cardinality,
    };
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
                <h2>Add Table</h2>
                <input id="tbl-name" placeholder="users" style="width:100%" />
                <input id="tbl-columns" placeholder="id:i64:pk:ai,name:string,email:string:nullable"
                       style="width:100%;margin-top:4px" />
                <button onclick="addTable()" style="margin-top:4px">Add Table</button>
                <h2>Add Relation</h2>
                <input id="rel-from" placeholder="from table" style="width:100%" />
                <input id="rel-to" placeholder="to table" style="width:100%;margin-top:4px" />
                <input id="rel-fk" placeholder="fk column" style="width:100%;margin-top:4px" />
                <input id="rel-pk" placeholder="pk column" style="width:100%;margin-top:4px" />
                <select id="rel-card" style="width:100%;margin-top:4px">
                    <option value="one_to_many">1:N</option>
                    <option value="many_to_one">N:1</option>
                    <option value="one_to_one">1:1</option>
                    <option value="many_to_many">N:N</option>
                </select>
                <button onclick="addRelation()" style="margin-top:4px">Add Relation</button>
                <div id="editor-msg" style="margin-top:6px;color:#080"></div>
                <details style="margin-top:8px">
                    <summary>API reference</summary>
                    <ul>
                        <li>GET /api/design - Get current design</li>
                        <li>POST /api/design/table - Add table (full JSON)</li>
                        <li>POST /api/design/table-simple - Add table (name + column spec)</li>
                        <li>POST /api/design/relation - Add relation (full JSON)</li>
                        <li>POST /api/design/relation-simple - Add relation (form fields)</li>
                        <li>GET /api/preview-ddl - Preview DDL</li>
                        <li>GET /api/export?format=svg - Export</li>
                    </ul>
                </details>
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
            document.getElementById('er-diagram').innerHTML = svgText;
        }
        async function postJson(url, body) {
            const res = await fetch(url, {
                method: 'POST',
                headers: {'Content-Type': 'application/json'},
                body: JSON.stringify(body),
            });
            if (!res.ok) {
                const err = await res.json().catch(() => ({}));
                throw new Error(err.message || res.statusText);
            }
            return res.json();
        }
        function setMsg(text, isError) {
            const el = document.getElementById('editor-msg');
            el.textContent = text;
            el.style.color = isError ? '#c00' : '#080';
        }
        async function addTable() {
            try {
                await postJson('/api/design/table-simple', {
                    name: document.getElementById('tbl-name').value,
                    columns: document.getElementById('tbl-columns').value,
                });
                setMsg('Table added', false);
                await loadDdl();
                await loadErDiagram();
            } catch (e) {
                setMsg('Error: ' + e.message, true);
            }
        }
        async function addRelation() {
            try {
                await postJson('/api/design/relation-simple', {
                    from_table: document.getElementById('rel-from').value,
                    to_table: document.getElementById('rel-to').value,
                    from_column: document.getElementById('rel-fk').value,
                    to_column: document.getElementById('rel-pk').value,
                    cardinality: document.getElementById('rel-card').value,
                });
                setMsg('Relation added', false);
                await loadDdl();
                await loadErDiagram();
            } catch (e) {
                setMsg('Error: ' + e.message, true);
            }
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

    #[test]
    fn test_web_ui_port_accessor() {
        let design = SchemaDesign::new(Dialect::PostgreSql);
        let designer = SchemaDesigner::new(design);
        let web_ui = SchemaDesignerWebUI::new(designer, 3000);
        assert_eq!(web_ui.port(), 3000);
    }

    #[test]
    fn test_html_contains_javascript() {
        assert!(
            INDEX_HTML.contains("loadErDiagram")
                || INDEX_HTML.contains("loadDdl")
                || INDEX_HTML.contains("script")
        );
    }

    #[test]
    fn test_html_contains_edit_forms() {
        // R6（2026-10-27）：页面从只读展示升级为可编辑
        assert!(INDEX_HTML.contains("Add Table"));
        assert!(INDEX_HTML.contains("Add Relation"));
        assert!(INDEX_HTML.contains("addTable"));
        assert!(INDEX_HTML.contains("addRelation"));
        assert!(INDEX_HTML.contains("/api/design/table-simple"));
        assert!(INDEX_HTML.contains("/api/design/relation-simple"));
    }

    #[test]
    fn test_parse_simple_columns_basic() {
        let cols = parse_simple_columns("id:i64:pk:ai, name:string, email:string:nullable")
            .expect("合法规格应解析成功");
        assert_eq!(cols.len(), 3);
        assert!(cols[0].is_primary_key);
        assert!(cols[0].is_auto_increment);
        assert!(!cols[0].nullable);
        assert!(!cols[1].is_primary_key);
        assert!(cols[2].nullable);
    }

    #[test]
    fn test_parse_simple_columns_rejects_unknown_type_and_flag() {
        assert!(parse_simple_columns("a:unknown_type").is_err());
        assert!(parse_simple_columns("a:i64:weird_flag").is_err());
        assert!(parse_simple_columns("").is_err());
    }

    #[test]
    fn test_parse_simple_columns_type_mapping() {
        let cols = parse_simple_columns("id:i64,b:bool,t:text,d:double,u:uuid")
            .expect("合法规格应解析成功");
        assert_eq!(cols.len(), 5);
        assert_eq!(cols[0].col_type.to_sql_type(), "BIGINT");
        assert_eq!(cols[1].col_type.to_sql_type(), "BOOLEAN");
        assert_eq!(cols[2].col_type.to_sql_type(), "TEXT");
        assert_eq!(cols[3].col_type.to_sql_type(), "DOUBLE");
        assert_eq!(cols[4].col_type.to_sql_type(), "UUID");
    }
}
