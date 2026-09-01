//! Web GUI 服务器

use std::sync::Arc;

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
    /// 创建 Web GUI 服务器
    pub fn new(config: ServerConfig) -> Self {
        Self {
            config,
            data: Arc::new(parking_lot::RwLock::new(StudioData::default())),
        }
    }

    /// 创建带初始数据的 Web GUI 服务器
    pub fn with_data(config: ServerConfig, data: StudioData) -> Self {
        Self {
            config,
            data: Arc::new(parking_lot::RwLock::new(data)),
        }
    }

    /// 构建路由
    pub fn router(&self) -> Router {
        Router::new()
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
