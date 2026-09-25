use sz_orm_graphql::resolver::{DbResolver, ResolverContext};
use serde_json::json;
use std::pin::Pin;
use std::future::Future;

struct MockResolver;

impl DbResolver for MockResolver {
    fn resolve_query(
        &self,
        ctx: &ResolverContext,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, String>> + Send>> {
        let name = ctx.field_name.clone();
        Box::pin(async move { Ok(json!({"field": name})) })
    }
}

#[tokio::test]
async fn test_resolve_query() {
    let resolver = MockResolver;
    let ctx = ResolverContext {
        field_name: "getUser".into(),
        type_name: "User".into(),
        is_list: false,
        args: json!({"id": "1"}),
    };
    let result = resolver.resolve_query(&ctx).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap()["field"], "getUser");
}

#[tokio::test]
async fn test_resolve_mutation_default() {
    let resolver = MockResolver;
    let ctx = ResolverContext {
        field_name: "createUser".into(),
        type_name: "User".into(),
        is_list: false,
        args: json!({}),
    };
    let result = resolver.resolve_mutation(&ctx).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("not implemented"));
}

#[test]
fn test_resolver_context_clone() {
    let ctx = ResolverContext {
        field_name: "x".into(),
        type_name: "T".into(),
        is_list: true,
        args: json!({"a": 1}),
    };
    let ctx2 = ctx.clone();
    assert_eq!(ctx.field_name, ctx2.field_name);
    assert_eq!(ctx.is_list, ctx2.is_list);
}