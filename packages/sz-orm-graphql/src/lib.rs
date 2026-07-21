//! # SZ-ORM GraphQL — GraphQL Schema 解析与查询
//!
//! 提供 GraphQL Schema 定义、类型/字段/查询/变更构建与查询执行，
//! 启用 `real` feature 后接入真实 GraphQL 引擎。
//!
//! ## 主要类型
//!
//! - [`GraphQLSchema`] — Schema 容器
//! - [`GraphQLType`] / [`GraphQLField`] — 类型与字段定义

use serde::{Deserialize, Serialize};

#[cfg(feature = "real")]
mod real_graphql;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphQLSchema {
    pub types: Vec<GraphQLType>,
    pub queries: Vec<GraphQLField>,
    pub mutations: Vec<GraphQLField>,
}

impl GraphQLSchema {
    pub fn new() -> Self {
        Self {
            types: vec![],
            queries: vec![],
            mutations: vec![],
        }
    }

    pub fn add_type(mut self, t: GraphQLType) -> Self {
        self.types.push(t);
        self
    }

    pub fn add_query(mut self, f: GraphQLField) -> Self {
        self.queries.push(f);
        self
    }

    pub fn add_mutation(mut self, f: GraphQLField) -> Self {
        self.mutations.push(f);
        self
    }

    /// Render the schema to a GraphQL SDL string.
    pub fn to_sdl(&self) -> String {
        let mut out = String::new();
        for t in &self.types {
            out.push_str(&format!("type {} {{\n", t.name));
            for f in &t.fields {
                out.push_str(&format!("    {}: {}\n", f.name, f.type_name));
            }
            out.push_str("}\n\n");
        }
        out.push_str("type Query {\n");
        for q in &self.queries {
            out.push_str(&format!("    {}: {}\n", q.name, q.type_name));
        }
        out.push_str("}\n");
        if !self.mutations.is_empty() {
            out.push_str("\ntype Mutation {\n");
            for m in &self.mutations {
                out.push_str(&format!("    {}: {}\n", m.name, m.type_name));
            }
            out.push_str("}\n");
        }
        out
    }
}

impl Default for GraphQLSchema {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphQLType {
    pub name: String,
    pub fields: Vec<GraphQLField>,
}

impl GraphQLType {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            fields: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphQLField {
    pub name: String,
    pub type_name: String,
}

/// Convert a model name like "users" or "order_items" to a PascalCase singular
/// GraphQL type name like "User" / "OrderItem".
fn to_pascal_singular(input: &str) -> String {
    let mut out = String::new();
    let mut cap_next = true;
    for ch in input.chars() {
        if ch == '_' || ch == '-' || ch == ' ' {
            cap_next = true;
        } else if cap_next {
            out.extend(ch.to_uppercase());
            cap_next = false;
        } else {
            out.push(ch);
        }
    }
    let len = out.len();
    if len > 1 && out.ends_with('s') && !out.ends_with("ss") {
        out.truncate(len - 1);
    }
    out
}

pub struct GraphQLSchemaGenerator;

impl GraphQLSchemaGenerator {
    /// Generate a real GraphQL schema from model names: for each model, emit a
    /// `type` definition with id/name/createdAt/updatedAt fields plus a
    /// `getX` query and a `listXs` query.
    pub fn generate_schema(models: &[&str]) -> GraphQLSchema {
        let mut schema = GraphQLSchema::new();
        for m in models {
            let type_name = to_pascal_singular(m);
            let mut t = GraphQLType::new(&type_name);
            t.fields.push(GraphQLField {
                name: "id".to_string(),
                type_name: "ID!".to_string(),
            });
            t.fields.push(GraphQLField {
                name: "name".to_string(),
                type_name: "String!".to_string(),
            });
            t.fields.push(GraphQLField {
                name: "createdAt".to_string(),
                type_name: "String!".to_string(),
            });
            t.fields.push(GraphQLField {
                name: "updatedAt".to_string(),
                type_name: "String!".to_string(),
            });
            schema = schema.add_type(t);
            schema = schema.add_query(GraphQLField {
                name: format!("get{}", type_name),
                type_name: type_name.clone(),
            });
            schema = schema.add_query(GraphQLField {
                name: format!("list{}s", type_name),
                type_name: format!("[{}!]!", type_name),
            });
        }
        schema
    }
}

pub struct GraphQLServer {
    port: u16,
    schema: Option<GraphQLSchema>,
    #[cfg(feature = "real")]
    dynamic_schema: std::sync::OnceLock<Result<async_graphql::dynamic::Schema, String>>,
}

impl GraphQLServer {
    pub fn new(port: u16) -> Self {
        Self {
            port,
            schema: None,
            #[cfg(feature = "real")]
            dynamic_schema: std::sync::OnceLock::new(),
        }
    }

    pub fn with_schema(mut self, s: GraphQLSchema) -> Self {
        self.schema = Some(s);
        self
    }

    /// Start a background tokio task that binds a TCP listener on the port.
    /// Returns the URL the server is listening on.
    /// Must be called from within a tokio runtime context.
    #[cfg(not(feature = "real"))]
    pub fn start(&self) -> Result<String, String> {
        if self.schema.is_none() {
            return Err("No schema".to_string());
        }
        let port = self.port;
        let url = format!("http://localhost:{}", port);
        // Spawn a background task that occupies the port with a TCP listener.
        tokio::spawn(async move {
            let addr = format!("127.0.0.1:{}", port);
            match tokio::net::TcpListener::bind(&addr).await {
                Ok(listener) => {
                    while listener.accept().await.is_ok() {
                        // Accept and drop; this is a placeholder server.
                    }
                }
                Err(_) => {
                    // Port may already be in use; the spawn task just exits.
                }
            }
        });
        Ok(url)
    }

    /// Lazily build (and cache) the executable async-graphql schema.
    #[cfg(feature = "real")]
    fn executable_schema(&self) -> Result<&async_graphql::dynamic::Schema, String> {
        let schema = self.schema.as_ref().ok_or("No schema")?;
        self.dynamic_schema
            .get_or_init(|| real_graphql::build_dynamic_schema(schema))
            .as_ref()
            .map_err(Clone::clone)
    }

    /// Start a background tokio task that serves real GraphQL over HTTP via
    /// axum + async-graphql (`POST /graphql`). Returns the URL the server is
    /// listening on. Must be called from within a tokio runtime context.
    #[cfg(feature = "real")]
    pub fn start(&self) -> Result<String, String> {
        let schema = self.executable_schema()?.clone();
        let port = self.port;
        let url = format!("http://localhost:{}", port);
        tokio::spawn(async move {
            let addr = format!("127.0.0.1:{}", port);
            match tokio::net::TcpListener::bind(&addr).await {
                Ok(listener) => {
                    let _ = axum::serve(listener, real_graphql::router(schema)).await;
                }
                Err(_) => {
                    // Port may already be in use; the spawn task just exits.
                }
            }
        });
        Ok(url)
    }

    /// Execute a simple GraphQL query of the form `{ getX(id: 1) { id name } }`
    /// or `{ listXs { id name } }`. Returns mock JSON data based on the schema.
    #[cfg(not(feature = "real"))]
    pub fn execute_query(&self, query: &str) -> Result<serde_json::Value, String> {
        let schema = self.schema.as_ref().ok_or("No schema")?;
        let trimmed = query.trim();
        let brace_start = trimmed.find('{').ok_or("Missing '{' in query")?;
        let after_brace = trimmed[brace_start + 1..].trim_start();
        // The query name ends at the first whitespace or '('.
        let end = after_brace
            .find(|c: char| c.is_whitespace() || c == '(' || c == '{')
            .unwrap_or(after_brace.len());
        let query_name = after_brace[..end].trim();
        if query_name.is_empty() {
            return Err("Empty query name".to_string());
        }
        let field = schema
            .queries
            .iter()
            .find(|q| q.name == query_name)
            .ok_or_else(|| format!("Query '{}' not found in schema", query_name))?;
        if field.type_name.starts_with('[') {
            // List query: return an array of mock objects.
            Ok(serde_json::json!([
                {
                    "id": "1",
                    "name": format!("{}_1", field.name),
                    "createdAt": "2024-01-01T00:00:00Z",
                    "updatedAt": "2024-01-01T00:00:00Z"
                },
                {
                    "id": "2",
                    "name": format!("{}_2", field.name),
                    "createdAt": "2024-01-01T00:00:00Z",
                    "updatedAt": "2024-01-01T00:00:00Z"
                }
            ]))
        } else {
            // Single query: return one mock object.
            Ok(serde_json::json!({
                "id": "1",
                "name": format!("{}_1", field.name),
                "createdAt": "2024-01-01T00:00:00Z",
                "updatedAt": "2024-01-01T00:00:00Z"
            }))
        }
    }

    /// Execute a GraphQL query with the real async-graphql engine and return
    /// the resolved value of the first root field as JSON.
    #[cfg(feature = "real")]
    pub fn execute_query(&self, query: &str) -> Result<serde_json::Value, String> {
        let schema = self.executable_schema()?;
        real_graphql::execute(schema, query)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_new() {
        let s = GraphQLSchema::new();
        assert!(s.types.is_empty());
    }

    #[test]
    fn test_schema_add_type() {
        let s = GraphQLSchema::new().add_type(GraphQLType::new("User"));
        assert_eq!(s.types.len(), 1);
    }

    #[test]
    fn test_generator_creates_types_and_queries() {
        let s = GraphQLSchemaGenerator::generate_schema(&["users", "orders"]);
        // 2 model types
        assert_eq!(s.types.len(), 2);
        // 2 queries per model (getX + listXs) = 4 queries
        assert_eq!(s.queries.len(), 4);
        // Verify each type has the required fields
        for t in &s.types {
            assert!(t
                .fields
                .iter()
                .any(|f| f.name == "id" && f.type_name == "ID!"));
            assert!(t.fields.iter().any(|f| f.name == "name"));
            assert!(t.fields.iter().any(|f| f.name == "createdAt"));
            assert!(t.fields.iter().any(|f| f.name == "updatedAt"));
        }
        // Verify both getX and listXs queries exist for "users" -> "User"
        assert!(s
            .queries
            .iter()
            .any(|q| q.name == "getUser" && q.type_name == "User"));
        assert!(s
            .queries
            .iter()
            .any(|q| q.name == "listUsers" && q.type_name == "[User!]!"));
        assert!(s
            .queries
            .iter()
            .any(|q| q.name == "getOrder" && q.type_name == "Order"));
        assert!(s
            .queries
            .iter()
            .any(|q| q.name == "listOrders" && q.type_name == "[Order!]!"));
    }

    #[test]
    fn test_schema_sdl_contains_all_models() {
        let s = GraphQLSchemaGenerator::generate_schema(&["users", "orders"]);
        let sdl = s.to_sdl();
        assert!(sdl.contains("type User {"));
        assert!(sdl.contains("type Order {"));
        assert!(sdl.contains("type Query {"));
        assert!(sdl.contains("getUser: User"));
        assert!(sdl.contains("listUsers: [User!]!"));
    }

    #[test]
    fn test_server_new() {
        let srv = GraphQLServer::new(4000);
        assert_eq!(srv.port, 4000);
    }

    #[test]
    fn test_server_start_without_schema_fails() {
        let srv = GraphQLServer::new(4000);
        assert!(srv.start().is_err());
    }

    #[tokio::test]
    async fn test_server_start_returns_url_and_binds_port() {
        let srv = GraphQLServer::new(4123)
            .with_schema(GraphQLSchemaGenerator::generate_schema(&["users"]));
        let url = srv.start().expect("start should succeed");
        assert!(url.contains("4123"));
        // Give the spawned task a moment to bind the port.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        // Verify the port is now bound by trying to bind to it again (should fail).
        let second = tokio::net::TcpListener::bind("127.0.0.1:4123").await;
        assert!(
            second.is_err(),
            "Port 4123 should already be bound by the spawned server task"
        );
    }

    #[test]
    fn test_execute_query_single() {
        let srv = GraphQLServer::new(4001)
            .with_schema(GraphQLSchemaGenerator::generate_schema(&["users"]));
        let result = srv.execute_query("{ getUser(id: 1) { id name } }");
        assert!(result.is_ok(), "expected ok, got {:?}", result);
        let v = result.unwrap();
        assert_eq!(v["id"], "1");
        assert!(v["name"].as_str().unwrap().contains("getUser"));
    }

    #[test]
    fn test_execute_query_list() {
        let srv = GraphQLServer::new(4002)
            .with_schema(GraphQLSchemaGenerator::generate_schema(&["users"]));
        let result = srv.execute_query("{ listUsers { id name } }");
        assert!(result.is_ok(), "expected ok, got {:?}", result);
        let v = result.unwrap();
        assert!(v.is_array());
        assert_eq!(v.as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_execute_query_unknown_returns_error() {
        let srv = GraphQLServer::new(4003)
            .with_schema(GraphQLSchemaGenerator::generate_schema(&["users"]));
        let result = srv.execute_query("{ unknownQuery { id } }");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknownQuery"));
    }

    #[test]
    fn test_execute_query_without_schema_fails() {
        let srv = GraphQLServer::new(4004);
        let result = srv.execute_query("{ getUser { id } }");
        assert!(result.is_err());
    }

    #[test]
    fn test_execute_query_malformed_no_brace() {
        let srv = GraphQLServer::new(4005)
            .with_schema(GraphQLSchemaGenerator::generate_schema(&["users"]));
        let result = srv.execute_query("getUser");
        assert!(result.is_err());
    }

    /// Send a real GraphQL POST request over HTTP/1.0 and return the status
    /// code together with the decoded JSON body. HTTP/1.0 keeps the response
    /// free of chunked transfer encoding, so the body ends at connection
    /// close.
    #[cfg(feature = "real")]
    async fn post_graphql(url: &str, body: &str) -> (u16, serde_json::Value) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let without_scheme = url
            .strip_prefix("http://")
            .expect("url must start with http://");
        let (addr, path) = without_scheme
            .split_once('/')
            .unwrap_or((without_scheme, ""));
        let mut stream = tokio::net::TcpStream::connect(addr)
            .await
            .expect("connect should succeed");
        let request = format!(
            "POST /{path} HTTP/1.0\r\nHost: {addr}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        stream
            .write_all(request.as_bytes())
            .await
            .expect("write should succeed");
        let mut raw = Vec::new();
        stream
            .read_to_end(&mut raw)
            .await
            .expect("read should succeed");
        let text = String::from_utf8(raw).expect("response must be valid UTF-8");
        let (head, body) = text
            .split_once("\r\n\r\n")
            .expect("response must contain a header/body separator");
        let status = head
            .split_whitespace()
            .nth(1)
            .and_then(|code| code.parse::<u16>().ok())
            .expect("status line must contain a numeric status code");
        let json = serde_json::from_str(body).expect("response body must be valid JSON");
        (status, json)
    }

    #[cfg(feature = "real")]
    #[tokio::test]
    #[ignore = "requires the real GraphQL server (feature `real`)"]
    async fn test_real_http_post_single_query() {
        let srv = GraphQLServer::new(4331)
            .with_schema(GraphQLSchemaGenerator::generate_schema(&["users"]));
        let url = srv.start().expect("start should succeed");
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let (status, body) = post_graphql(
            &format!("{url}/graphql"),
            &serde_json::json!({"query": "{ getUser(id: 1) { id name } }"}).to_string(),
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(body["data"]["getUser"]["id"], "1");
        assert!(body["data"]["getUser"]["name"]
            .as_str()
            .expect("name must be a string")
            .contains("getUser"));
    }

    #[cfg(feature = "real")]
    #[tokio::test]
    #[ignore = "requires the real GraphQL server (feature `real`)"]
    async fn test_real_http_post_list_query() {
        let srv = GraphQLServer::new(4332)
            .with_schema(GraphQLSchemaGenerator::generate_schema(&["users"]));
        let url = srv.start().expect("start should succeed");
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let (status, body) = post_graphql(
            &format!("{url}/graphql"),
            &serde_json::json!({"query": "{ listUsers { id name } }"}).to_string(),
        )
        .await;
        assert_eq!(status, 200);
        let users = body["data"]["listUsers"]
            .as_array()
            .expect("listUsers must be an array");
        assert_eq!(users.len(), 2);
        assert_eq!(users[0]["id"], "1");
        assert_eq!(users[1]["id"], "2");
    }

    #[cfg(feature = "real")]
    #[tokio::test]
    #[ignore = "requires the real GraphQL server (feature `real`)"]
    async fn test_real_http_post_unknown_query_returns_errors() {
        let srv = GraphQLServer::new(4333)
            .with_schema(GraphQLSchemaGenerator::generate_schema(&["users"]));
        let url = srv.start().expect("start should succeed");
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let (status, body) = post_graphql(
            &format!("{url}/graphql"),
            &serde_json::json!({"query": "{ unknownQuery { id } }"}).to_string(),
        )
        .await;
        assert_eq!(status, 200);
        let errors = body["errors"].as_array().expect("errors must be an array");
        assert!(!errors.is_empty());
        assert!(errors[0]["message"]
            .as_str()
            .expect("message must be a string")
            .contains("unknownQuery"));
    }

    #[cfg(feature = "real")]
    #[test]
    #[ignore = "requires the real GraphQL engine (feature `real`)"]
    fn test_real_execute_query_matches_mock_shape() {
        let srv = GraphQLServer::new(4334)
            .with_schema(GraphQLSchemaGenerator::generate_schema(&["users"]));
        let single = srv
            .execute_query("{ getUser(id: 1) { id name } }")
            .expect("single query should succeed");
        assert_eq!(single["id"], "1");
        assert!(single["name"]
            .as_str()
            .expect("name must be a string")
            .contains("getUser"));
        let list = srv
            .execute_query("{ listUsers { id name } }")
            .expect("list query should succeed");
        assert_eq!(list.as_array().expect("result must be an array").len(), 2);
        let err = srv
            .execute_query("{ unknownQuery { id } }")
            .expect_err("unknown query must fail");
        assert!(err.contains("unknownQuery"));
    }
}
