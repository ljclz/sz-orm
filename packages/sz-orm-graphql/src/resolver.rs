//! DB Resolver trait — P2-1 修复 C-3：真实 DB resolver 集成
//!
//! 定义 GraphQL root field 与真实数据源（数据库/ORM）之间的桥接接口。
//!
//! # 设计
//!
//! - 使用 boxed future 手动实现 async trait，避免引入 async-trait 依赖
//! - 调用方实现此 trait 后，通过 `GraphQLServer::with_db_resolver` 注入
//! - 未注入 resolver 时，schema 回退到 mock 数据（向后兼容）
//!
//! # 使用示例
//!
//! ```ignore
//! use sz_orm_graphql::resolver::{DbResolver, ResolverContext};
//! use std::sync::Arc;
//!
//! struct MyDbResolver;
//!
//! impl DbResolver for MyDbResolver {
//!     fn resolve_query(
//!         &self,
//!         ctx: &ResolverContext,
//!     ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, String>> + Send>> {
//!         let field_name = ctx.field_name.clone();
//!         let args = ctx.args.clone();
//!         Box::pin(async move {
//!             // 在此处执行真实数据库查询
//!             Ok(serde_json::json!({"id": "1", "name": format!("{}_real", field_name)}))
//!         })
//!     }
//! }
//!
//! let resolver = Arc::new(MyDbResolver);
//! let server = sz_orm_graphql::GraphQLServer::new(4000)
//!     .with_schema(schema)
//!     .with_db_resolver(resolver);
//! ```

use serde_json::Value;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// Resolver 调用上下文 — 传递字段名、参数等信息给 resolver
#[derive(Debug, Clone)]
pub struct ResolverContext {
    /// Root field 名称（如 "getUser"、"listUsers"）
    pub field_name: String,
    /// 字段类型名（如 "User"、"[User!]!"）
    pub type_name: String,
    /// 是否为列表查询
    pub is_list: bool,
    /// GraphQL 参数（从查询中提取，如 {"id": "1"}）
    pub args: Value,
}

/// DB Resolver trait — P2-1 修复 C-3
///
/// 调用方实现此 trait，为 GraphQL root field 提供真实数据。
///
/// # 方法
///
/// - `resolve_query`：解析 Query 字段，返回单个对象或数组
/// - `resolve_mutation`：解析 Mutation 字段，返回操作结果
///
/// # async 实现
///
/// 使用 `Pin<Box<dyn Future>>` 返回异步结果，无需 async-trait 依赖。
pub trait DbResolver: Send + Sync {
    /// 解析 Query root field
    ///
    /// # 参数
    ///
    /// - `ctx`：resolver 上下文（字段名、参数等）
    ///
    /// # 返回
    ///
    /// - `Ok(Value)`：解析结果（单个对象为 `Value::Object`，列表为 `Value::Array`）
    /// - `Err(String)`：解析错误
    fn resolve_query(
        &self,
        ctx: &ResolverContext,
    ) -> Pin<Box<dyn Future<Output = Result<Value, String>> + Send>>;

    /// 解析 Mutation root field
    ///
    /// 默认实现返回错误，调用方按需覆盖。
    fn resolve_mutation(
        &self,
        ctx: &ResolverContext,
    ) -> Pin<Box<dyn Future<Output = Result<Value, String>> + Send>> {
        let _ = ctx;
        Box::pin(async { Err("Mutation resolver not implemented".to_string()) })
    }
}

/// 类型别名：Arc 包装的 DbResolver
pub type SharedDbResolver = Arc<dyn DbResolver>;
