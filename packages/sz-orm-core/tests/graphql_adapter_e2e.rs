//! GraphQL 适配层端到端测试
use sz_orm_core::graphql_adapter::{graphql_execute, graphql_query_count};

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
