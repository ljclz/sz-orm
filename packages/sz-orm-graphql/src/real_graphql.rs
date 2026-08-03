//! Real GraphQL execution and serving backed by async-graphql + axum.
//!
//! This module is only compiled with the `real` feature enabled. It builds a
//! dynamic async-graphql schema from the declarative [`GraphQLSchema`],
//! executes queries against it and exposes it over HTTP via axum.
//!
//! # P1-1 / P2-1 说明：root field resolver
//!
//! ## P2-1 修复 C-3：DB resolver 注入（当前状态）
//!
//! 自 P2-1 起，本模块支持通过 [`crate::resolver::DbResolver`] 注入真实数据源。
//! 调用方通过 `GraphQLServer::with_db_resolver` 注入 resolver 后，root field
//! 将通过 resolver 异步查询真实数据库（见 [`resolver_root_field`]）。
//! 未注入 resolver 时，回退到 [`mock_root_field`] 返回硬编码 mock 数据（向后兼容）。
//!
//! 这意味着：
//! - ✅ **Schema 构建是真实的**：使用 `async-graphql::dynamic::Schema`，类型/字段定义
//!   均根据 `GraphQLSchema` 动态注册
//! - ✅ **GraphQL 执行是真实的**：查询由 `async-graphql` 引擎按 GraphQL 规范解析与执行
//! - ✅ **HTTP 服务是真实的**：通过 axum 暴露 `/graphql` POST 端点
//! - ✅ **数据解析可注入真实 DB resolver**（P2-1）：注入 resolver 后查询真实数据库
//! - ⚠️ **未注入 resolver 时数据为 mock**：root field 返回硬编码数据
//!
//! ## 如何接入真实数据源
//!
//! 实现 [`crate::resolver::DbResolver`] trait，并通过
//! `GraphQLServer::with_db_resolver` 注入即可，无需修改本模块代码。

use async_graphql::dynamic::{
    Field, FieldFuture, InputValue, Object, ResolverContext, Schema, TypeRef,
};
use async_graphql::Value;

use crate::{GraphQLField, GraphQLSchema, GraphQLType};

/// Parse a GraphQL type reference like `ID!`, `User` or `[User!]!` into a
/// dynamic [`TypeRef`]. List nesting beyond one level is rejected.
fn parse_type_ref(type_name: &str) -> Result<TypeRef, String> {
    let unsupported = || format!("Unsupported type reference '{type_name}'");
    let trimmed = type_name.trim();
    let (inner, non_null) = match trimmed.strip_suffix('!') {
        Some(rest) => (rest.trim(), true),
        None => (trimmed, false),
    };
    if let Some(list_inner) = inner.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        let list_inner = list_inner.trim();
        let (item, item_non_null) = match list_inner.strip_suffix('!') {
            Some(rest) => (rest.trim(), true),
            None => (list_inner, false),
        };
        if item.is_empty() || item.starts_with('[') {
            return Err(unsupported());
        }
        Ok(match (item_non_null, non_null) {
            (false, false) => TypeRef::named_list(item),
            (true, false) => TypeRef::named_nn_list(item),
            (false, true) => TypeRef::named_list_nn(item),
            (true, true) => TypeRef::named_nn_list_nn(item),
        })
    } else if inner.is_empty() {
        Err(unsupported())
    } else if non_null {
        Ok(TypeRef::named_nn(inner))
    } else {
        Ok(TypeRef::named(inner))
    }
}

/// Build the mock JSON payload for a root field, mirroring the data shape of
/// the in-memory implementation.
fn mock_payload(field_name: &str, is_list: bool) -> Value {
    let object = |id: &str| {
        serde_json::json!({
            "id": id,
            "name": format!("{field_name}_{id}"),
            "createdAt": "2024-01-01T00:00:00Z",
            "updatedAt": "2024-01-01T00:00:00Z",
        })
    };
    let json = if is_list {
        serde_json::json!([object("1"), object("2")])
    } else {
        object("1")
    };
    // The payload contains only strings, arrays and objects, which always
    // convert into a GraphQL value.
    Value::from_json(json).unwrap_or(Value::Null)
}

/// Create a root (query/mutation) field that resolves to mock data.
fn mock_root_field(field: &GraphQLField) -> Result<Field, String> {
    let type_ref = parse_type_ref(&field.type_name)?;
    let is_list = field.type_name.trim_start().starts_with('[');
    let value = mock_payload(&field.name, is_list);
    let mut root = Field::new(field.name.clone(), type_ref, move |_ctx| {
        FieldFuture::from_value(Some(value.clone()))
    });
    if !is_list {
        // Single-object lookups conventionally accept an optional `id`.
        root = root.argument(InputValue::new("id", TypeRef::named(TypeRef::ID)));
    }
    Ok(root)
}

/// Create a root field backed by a real DB resolver — P2-1 修复 C-3
///
/// 当调用方注入了 DbResolver 时，root field 通过 resolver 异步查询真实数据库，
/// 而非返回 mock 数据。闭包从 async-graphql 执行上下文提取参数，构造
/// [`crate::resolver::ResolverContext`] 后调用 `DbResolver::resolve_query`。
fn resolver_root_field(
    field: &GraphQLField,
    resolver: crate::resolver::SharedDbResolver,
) -> Result<Field, String> {
    let type_ref = parse_type_ref(&field.type_name)?;
    let is_list = field.type_name.trim_start().starts_with('[');
    let field_name = field.name.clone();
    let type_name = field.type_name.clone();
    let resolver = resolver;

    let mut root = Field::new(
        field.name.clone(),
        type_ref,
        move |ctx: ResolverContext<'_>| {
            // 从 GraphQL 执行上下文提取参数
            let mut args = serde_json::Map::new();
            for (key, val) in ctx.args.iter() {
                if let Ok(json_v) = val.as_value().clone().into_json() {
                    args.insert(key.to_string(), json_v);
                }
            }
            let resolver_ctx = crate::resolver::ResolverContext {
                field_name: field_name.clone(),
                type_name: type_name.clone(),
                is_list,
                args: serde_json::Value::Object(args),
            };
            let resolver = resolver.clone();
            FieldFuture::new(async move {
                match resolver.resolve_query(&resolver_ctx).await {
                    Ok(value) => {
                        let gql_value = Value::from_json(value).unwrap_or(Value::Null);
                        Ok(Some(gql_value))
                    }
                    Err(msg) => {
                        tracing::error!(
                            field = %resolver_ctx.field_name,
                            error = %msg,
                            "DB resolver failed"
                        );
                        Ok(Some(Value::Null))
                    }
                }
            })
        },
    );

    if !is_list {
        root = root.argument(InputValue::new("id", TypeRef::named(TypeRef::ID)));
    }
    Ok(root)
}

/// Create a dynamic object type whose fields read from the parent value
/// resolved by the root field.
fn object_type(t: &GraphQLType) -> Result<Object, String> {
    let mut obj = Object::new(t.name.clone());
    for field in &t.fields {
        let type_ref = parse_type_ref(&field.type_name)?;
        let field_name = field.name.clone();
        obj = obj.field(Field::new(
            field.name.clone(),
            type_ref,
            move |ctx: ResolverContext<'_>| {
                let value = ctx
                    .parent_value
                    .try_to_value()
                    .ok()
                    .and_then(|parent| match parent {
                        Value::Object(map) => map.get(field_name.as_str()).cloned(),
                        _ => None,
                    });
                FieldFuture::from_value(value)
            },
        ));
    }
    Ok(obj)
}

/// Build a real executable async-graphql [`Schema`] from the declarative
/// [`GraphQLSchema`].
///
/// 当 `resolver` 为 `Some` 时，root field 通过 [`resolver_root_field`] 查询真实
/// 数据库；为 `None` 时回退到 [`mock_root_field`]（向后兼容）。
pub fn build_dynamic_schema(
    schema: &GraphQLSchema,
    resolver: Option<&crate::resolver::SharedDbResolver>,
) -> Result<Schema, String> {
    let mutation_name = if schema.mutations.is_empty() {
        None
    } else {
        Some("Mutation")
    };
    let mut builder = Schema::build("Query", mutation_name, None);
    for t in &schema.types {
        builder = builder.register(object_type(t)?);
    }
    let mut query = Object::new("Query");
    for field in &schema.queries {
        let f = match resolver {
            Some(r) => resolver_root_field(field, std::sync::Arc::clone(r))?,
            None => mock_root_field(field)?,
        };
        query = query.field(f);
    }
    builder = builder.register(query);
    if mutation_name.is_some() {
        let mut mutation = Object::new("Mutation");
        for field in &schema.mutations {
            let f = match resolver {
                Some(r) => resolver_root_field(field, std::sync::Arc::clone(r))?,
                None => mock_root_field(field)?,
            };
            mutation = mutation.field(f);
        }
        builder = builder.register(mutation);
    }
    builder.finish().map_err(|e| e.to_string())
}

/// Build an axum router serving GraphQL POST requests at `/graphql`.
pub fn router(schema: Schema) -> axum::Router {
    async fn graphql_handler(
        axum::extract::State(schema): axum::extract::State<Schema>,
        request: async_graphql_axum::GraphQLRequest,
    ) -> async_graphql_axum::GraphQLResponse {
        schema.execute(request.into_inner()).await.into()
    }

    axum::Router::new()
        .route("/graphql", axum::routing::post(graphql_handler))
        .with_state(schema)
}

/// 异步执行 GraphQL 查询，返回第一个根字段的解析结果（JSON）。
///
/// v1.2.0 修复 P0：新增异步入口，直接 `schema.execute(query).await`，
/// 供异步调用方使用，避免在 async 上下文中调用 `block_on` 导致 tokio
/// runtime 死锁。
///
/// # 推荐用法
///
/// - 调用方处于 async 上下文（`async fn` / tokio task）时，**必须**使用本方法
/// - 调用方处于同步上下文时，使用 [`execute`]（同步包装）
pub async fn execute_async(schema: &Schema, query: &str) -> Result<serde_json::Value, String> {
    let response = schema.execute(query).await;
    response_to_json(response)
}

/// 将 async-graphql 响应转换为「第一个根字段的解析结果」JSON。
fn response_to_json(response: async_graphql::Response) -> Result<serde_json::Value, String> {
    if !response.errors.is_empty() {
        return Err(response
            .errors
            .iter()
            .map(|e| e.message.as_str())
            .collect::<Vec<_>>()
            .join("; "));
    }
    let data = response.data.into_json().map_err(|e| e.to_string())?;
    match data {
        serde_json::Value::Object(map) => Ok(map
            .into_iter()
            .next()
            .map(|(_, value)| value)
            .unwrap_or(serde_json::Value::Null)),
        other => Ok(other),
    }
}

/// 同步执行 GraphQL 查询，返回第一个根字段的解析结果（JSON）。
///
/// # 死锁安全性说明
///
/// 本函数为同步 API。它通过 `std::thread::scope` 在**独立的 OS 线程**上
/// 创建**全新的 current_thread tokio runtime** 再 `block_on`，因此：
///
/// - **不会**复用调用方已有的 tokio runtime，不会在已有 runtime 的 worker
///   线程上调用 `block_on`，故**不会死锁**
/// - 可安全地从 async 上下文中调用（但会阻塞调用线程直到查询完成）
///
/// # 调用方建议
///
/// - **同步上下文**：直接使用本函数
/// - **异步上下文**：优先使用 [`execute_async`]，避免阻塞当前 async 线程；
///   若必须使用同步 API，本函数也不会死锁，但会占用一个阻塞线程位
pub fn execute(schema: &Schema, query: &str) -> Result<serde_json::Value, String> {
    // 在独立 OS 线程上创建全新 runtime 再 block_on，确保不复用调用方的
    // tokio runtime，从根本上避免在 async 上下文调用 block_on 的死锁风险。
    let response = std::thread::scope(|scope| {
        scope
            .spawn(|| -> Result<async_graphql::Response, String> {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|e| format!("Failed to build tokio runtime: {e}"))?;
                Ok(runtime.block_on(schema.execute(query)))
            })
            .join()
            .map_err(|_| "GraphQL executor thread panicked".to_string())?
    })?;
    response_to_json(response)
}

#[cfg(test)]
mod tests {
    use crate::resolver::{DbResolver, ResolverContext, SharedDbResolver};
    use crate::{GraphQLSchemaGenerator, GraphQLServer};
    use serde_json::json;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Arc;

    /// 测试用 DB resolver — 返回包含 "real" 标记的数据
    struct TestDbResolver;

    impl DbResolver for TestDbResolver {
        fn resolve_query(
            &self,
            ctx: &ResolverContext,
        ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, String>> + Send>> {
            let field_name = ctx.field_name.clone();
            let is_list = ctx.is_list;
            Box::pin(async move {
                if is_list {
                    Ok(json!([
                        {"id": "100", "name": format!("{}_real_100", field_name), "createdAt": "2024-01-01T00:00:00Z", "updatedAt": "2024-01-01T00:00:00Z"},
                        {"id": "200", "name": format!("{}_real_200", field_name), "createdAt": "2024-01-01T00:00:00Z", "updatedAt": "2024-01-01T00:00:00Z"}
                    ]))
                } else {
                    Ok(json!({
                        "id": "100",
                        "name": format!("{}_real", field_name),
                        "createdAt": "2024-01-01T00:00:00Z",
                        "updatedAt": "2024-01-01T00:00:00Z"
                    }))
                }
            })
        }
    }

    #[test]
    fn test_resolver_returns_real_data_single() {
        let resolver: SharedDbResolver = Arc::new(TestDbResolver);
        let srv = GraphQLServer::new(4501)
            .with_schema(GraphQLSchemaGenerator::generate_schema(&["users"]))
            .with_db_resolver(resolver);
        let result = srv.execute_query("{ getUser(id: 1) { id name } }");
        assert!(result.is_ok(), "expected ok, got {:?}", result);
        let v = result.unwrap();
        assert_eq!(v["id"], "100");
        assert!(
            v["name"].as_str().unwrap().contains("real"),
            "name should contain 'real': {}",
            v["name"]
        );
    }

    #[test]
    fn test_resolver_returns_real_data_list() {
        let resolver: SharedDbResolver = Arc::new(TestDbResolver);
        let srv = GraphQLServer::new(4502)
            .with_schema(GraphQLSchemaGenerator::generate_schema(&["users"]))
            .with_db_resolver(resolver);
        let result = srv.execute_query("{ listUsers { id name } }");
        assert!(result.is_ok(), "expected ok, got {:?}", result);
        let v = result.unwrap();
        assert!(v.is_array());
        let arr = v.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["id"], "100");
        assert_eq!(arr[1]["id"], "200");
        assert!(arr[0]["name"].as_str().unwrap().contains("real"));
    }

    #[test]
    fn test_no_resolver_falls_back_to_mock() {
        // 不注入 resolver 时，应回退到 mock 数据
        let srv = GraphQLServer::new(4503)
            .with_schema(GraphQLSchemaGenerator::generate_schema(&["users"]));
        let result = srv.execute_query("{ getUser(id: 1) { id name } }");
        assert!(result.is_ok(), "expected ok, got {:?}", result);
        let v = result.unwrap();
        assert_eq!(v["id"], "1"); // mock 数据 id 为 "1"
        assert!(
            v["name"].as_str().unwrap().contains("getUser"),
            "mock name should contain field name"
        );
    }
}
