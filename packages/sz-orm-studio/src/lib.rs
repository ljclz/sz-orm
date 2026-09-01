//! # sz-orm-studio — Web GUI 数据浏览器
//!
//! 基于 axum 的 HTTP 服务，提供表数据浏览/筛选/编辑/关系导航 REST API。
//!
//! ## REST 端点
//!
//! - `GET /tables` — 表列表
//! - `GET /tables/:name/data` — 表数据
//! - `PUT /tables/:name/data/:id` — 编辑记录
//! - `GET /tables/:name/relations` — 关系导航

pub mod handlers;
pub mod server;

pub use handlers::{DataStore, EditRequest, RelationInfo, StudioData, TableInfo, TableRow};
pub use server::{ServerConfig, WebGuiServer};
