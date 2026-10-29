//! # sz-orm-studio — Web GUI 数据浏览器
//!
//! 基于 axum 的 HTTP 服务，提供表数据浏览/筛选/编辑/关系导航 REST API。
//!
//! ## REST 端点
//!
//! - `GET /` — 浏览器页面
//! - `GET /tables` — 表列表
//! - `GET /tables/:name/data` — 表数据
//! - `PUT /tables/:name/data/:id` — 编辑记录
//! - `GET /tables/:name/relations` — 关系导航

pub mod handlers;
pub mod server;

pub use handlers::{DataStore, EditRequest, RelationInfo, StudioData, TableInfo, TableRow};
pub use server::{ServerConfig, WebGuiServer};

pub fn parse_args(argv: &[String]) -> ServerConfig {
    let addr = argv
        .iter()
        .skip(1)
        .find(|a| a.starts_with("--addr="))
        .and_then(|a| a.strip_prefix("--addr=").map(|s| s.to_string()))
        .unwrap_or_else(|| "127.0.0.1".to_string());
    let port: u16 = argv
        .iter()
        .skip(1)
        .find(|a| a.starts_with("--port="))
        .and_then(|a| a.strip_prefix("--port=").and_then(|s| s.parse().ok()))
        .unwrap_or(8080);
    ServerConfig::new(addr, port)
}

/// 解析 `--db=` 参数：`memory`（默认）或 SQLite 文件路径
pub fn parse_db_arg(argv: &[String]) -> Option<String> {
    argv.iter()
        .skip(1)
        .find(|a| a.starts_with("--db="))
        .and_then(|a| a.strip_prefix("--db=").map(|s| s.to_string()))
}

pub fn parse_args_and_run(argv: Vec<String>) -> std::process::ExitCode {
    let config = parse_args(&argv);
    let db = parse_db_arg(&argv);
    println!(
        "sz-orm-studio 启动于 http://{}（数据源: {}）",
        config.bind_addr(),
        db.as_deref().unwrap_or("memory")
    );
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("创建 tokio runtime 失败: {}", e);
            return std::process::ExitCode::FAILURE;
        }
    };
    let server = match db.as_deref() {
        Some("memory") | None => WebGuiServer::new(config),
        Some(path) => match rusqlite::Connection::open(path) {
            Ok(conn) => WebGuiServer::with_sqlite(config, conn),
            Err(e) => {
                eprintln!("打开数据库 {} 失败: {}", path, e);
                return std::process::ExitCode::FAILURE;
            }
        },
    };
    match runtime.block_on(server.start()) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("服务器错误: {}", e);
            std::process::ExitCode::FAILURE
        }
    }
}
// v8.1.0 组 6：开发者门户（developer-portal feature gate）
#[cfg(feature = "developer-portal")]
pub mod developer_portal;
#[cfg(feature = "developer-portal")]
pub use developer_portal::{
    DeveloperPortal, EcoError as PortalEcoError, PortalHealthMetrics, PortalPage, PortalRequest,
    PortalResponse,
};
