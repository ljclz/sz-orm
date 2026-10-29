//! Web GUI 服务器

use std::sync::Arc;

use axum::response::Html;
use axum::routing::{get, put};
use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::handlers::{DataStore, StudioData};

/// 服务器配置
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// 监听地址
    pub addr: String,
    /// 端口
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            addr: "127.0.0.1".to_string(),
            port: 8080,
        }
    }
}

impl ServerConfig {
    /// 创建配置
    pub fn new(addr: impl Into<String>, port: u16) -> Self {
        Self {
            addr: addr.into(),
            port,
        }
    }

    /// 绑定地址
    pub fn bind_addr(&self) -> String {
        format!("{}:{}", self.addr, self.port)
    }
}

/// Web GUI 服务器
///
/// 启动 axum HTTP 服务，提供表数据浏览/筛选/编辑/关系导航 REST API。
pub struct WebGuiServer {
    config: ServerConfig,
    data: DataStore,
}

impl WebGuiServer {
    /// 创建 Web GUI 服务器（内存数据源，演示/测试用）
    pub fn new(config: ServerConfig) -> Self {
        Self {
            config,
            data: Arc::new(parking_lot::RwLock::new(StudioData::default())),
        }
    }

    /// 创建带初始内存数据的 Web GUI 服务器
    pub fn with_data(config: ServerConfig, data: StudioData) -> Self {
        Self {
            config,
            data: Arc::new(parking_lot::RwLock::new(data)),
        }
    }

    /// 创建带真实 SQLite 数据源的 Web GUI 服务器（v10.13.0 增强）
    pub fn with_sqlite(config: ServerConfig, conn: rusqlite::Connection) -> Self {
        Self {
            config,
            data: Arc::new(parking_lot::RwLock::new(StudioData::with_sqlite(conn))),
        }
    }

    /// 构建路由
    pub fn router(&self) -> Router {
        Router::new()
            .route("/", get(index_page))
            .route("/tables", get(crate::handlers::get_tables))
            .route("/tables/:name/data", get(crate::handlers::get_table_data))
            .route("/tables/:name/data/:id", put(crate::handlers::edit_record))
            .route(
                "/tables/:name/relations",
                get(crate::handlers::get_table_relations),
            )
            .layer(CorsLayer::permissive())
            .layer(TraceLayer::new_for_http())
            .with_state(self.data.clone())
    }

    /// 启动 HTTP 服务
    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        let addr = self.config.bind_addr();
        let listener = tokio::net::TcpListener::bind(&addr).await?;
        let router = self.router();
        axum::serve(listener, router).await?;
        Ok(())
    }

    /// 获取配置
    pub fn config(&self) -> &ServerConfig {
        &self.config
    }

    /// 获取数据存储引用
    pub fn data(&self) -> &DataStore {
        &self.data
    }
}

/// 前端页面（v10.13.0 增强：从纯 JSON API 变为可浏览的 Web UI）
async fn index_page() -> Html<&'static str> {
    Html(INDEX_HTML)
}

const INDEX_HTML: &str = r#"<!DOCTYPE html>
<html>
<head>
    <title>SZ-ORM Studio</title>
    <style>
        body { font-family: sans-serif; margin: 0; padding: 20px; }
        table { border-collapse: collapse; width: 100%; margin-top: 8px; }
        th, td { border: 1px solid #ccc; padding: 6px; text-align: left; }
        th { background: #f0f0f0; }
        .active { background: #e8f0e8; }
    </style>
</head>
<body>
    <h1>SZ-ORM Studio</h1>
    <div id="tables"></div>
    <div id="data">
        <h2 id="table-title">Select a table</h2>
        <table id="data-table" style="display:none">
            <thead id="data-head"></thead>
            <tbody id="data-body"></tbody>
        </table>
    </div>
    <script>
        async function loadTables() {
            const res = await fetch('/tables');
            const tables = await res.json();
            const div = document.getElementById('tables');
            div.innerHTML = '';
            tables.forEach(t => {
                const btn = document.createElement('button');
                btn.textContent = t.name + ' (' + t.row_count + ')';
                btn.style.margin = '2px';
                btn.onclick = () => loadTableData(t.name);
                div.appendChild(btn);
            });
        }
        async function loadTableData(name) {
            document.querySelectorAll('#tables button').forEach(b => b.classList.remove('active'));
            event.target.classList.add('active');
            document.getElementById('table-title').textContent = name;
            const res = await fetch('/tables/' + encodeURIComponent(name) + '/data');
            const rows = await res.json();
            const thead = document.getElementById('data-head');
            const tbody = document.getElementById('data-body');
            document.getElementById('data-table').style.display = 'table';
            if (rows.length === 0) {
                thead.innerHTML = '';
                tbody.innerHTML = '<tr><td>No rows</td></tr>';
                return;
            }
            const cols = Object.keys(rows[0].data);
            thead.innerHTML = '<tr>' + cols.map(c => '<th>' + c + '</th>').join('') + '</tr>';
            tbody.innerHTML = rows.map(r =>
                '<tr>' + cols.map(c => '<td>' + (r.data[c] ?? '') + '</td>').join('') + '</tr>'
            ).join('');
        }
        loadTables();
    </script>
</body>
</html>"#;
