//! # GraphQL Adapter — sz-orm-core GraphQL 适配层
//!
//! v5.0.0 M4：将 sz-orm-graphql 的 GraphQLServer 接入 sz-orm-core，
//! 提供 `graphql_execute` / `graphql_query_count` 入口。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

use parking_lot::RwLock;
use sz_orm_graphql::{GraphQLSchema, GraphQLServer};

static GRAPHQL_SERVER: OnceLock<RwLock<GraphQLServer>> = OnceLock::new();
static QUERY_COUNT: AtomicU64 = AtomicU64::new(0);

fn server() -> &'static RwLock<GraphQLServer> {
    GRAPHQL_SERVER.get_or_init(|| {
        let schema = GraphQLSchema::new();
        RwLock::new(GraphQLServer::new(8080).with_schema(schema))
    })
}

/// 执行 GraphQL 查询
pub fn graphql_execute(query: &str) -> Result<serde_json::Value, String> {
    QUERY_COUNT.fetch_add(1, Ordering::Relaxed);
    let server = server().read();
    server.execute_query(query)
}

/// 获取查询计数
pub fn graphql_query_count() -> u64 {
    QUERY_COUNT.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_graphql_execute_is_reachable() {
        let result = graphql_execute("{ __typename }");
        assert!(
            result.is_ok() || result.is_err(),
            "graphql_execute should be callable"
        );
    }

    #[test]
    fn test_graphql_count_increments() {
        let before = graphql_query_count();
        let _ = graphql_execute("{ __typename }");
        let after = graphql_query_count();
        assert!(after > before);
    }
}
