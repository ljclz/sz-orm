//! M8 集成测试：GraphQL 深度集成全流程

use serde_json::json;
use sz_orm_graphql::async_graphql_integration::{
    relay_paginate, AsyncGraphqlBridge, ErrorCategory, FederationGateway, PageInfo,
    RelayConnection, SubscriptionSource, TicketError,
};

#[test]
fn test_bridge_execute_and_dataloader() {
    let bridge = AsyncGraphqlBridge::new("type Query { users { name, orders { amount } } }");
    let result = bridge.execute("{ users { name } }").unwrap();
    assert!(result.is_ok());
}

#[test]
fn test_dataloader_n1_elimination() {
    let bridge = AsyncGraphqlBridge::new("type Query { users { orders } }");
    let mut values = std::collections::HashMap::new();
    for i in 1..=50 {
        values.insert(format!("user{i}"), json!({"orders": []}));
    }
    let keys: Vec<String> = (1..=50).map(|i| format!("user{i}")).collect();
    let results = bridge.batch_load_relations(&keys, values);
    assert_eq!(results.len(), 50);
    assert_eq!(bridge.dataloader().batch_count(), 1);
}

#[test]
fn test_relay_pagination_full() {
    let items: Vec<u32> = (1..=100).collect();
    let conn = relay_paginate(&items, 10, None, |i| format!("cursor-{i}")).unwrap();
    assert_eq!(conn.edges.len(), 10);
    assert!(conn.page_info.has_next_page);
    assert_eq!(conn.page_info.end_cursor, Some("cursor-10".to_string()));

    let conn2 = relay_paginate(&items, 10, Some("cursor-10"), |i| format!("cursor-{i}")).unwrap();
    assert_eq!(conn2.edges.len(), 10);
    assert_eq!(conn2.edges[0].node, 11);
    assert!(conn2.page_info.has_next_page);
    assert!(conn2.page_info.has_previous_page);
}

#[test]
fn test_subscription_from_cdc() {
    let source = SubscriptionSource::new();
    let _sub_id = source.subscribe(
        sz_orm_graphql::async_graphql_integration::subscription::SubscriptionEventType::UserUpdated,
    );

    let event =
        SubscriptionSource::from_cdc_change("Update", "users", json!({"id": 1, "name": "new"}));
    source.push_event(event).unwrap();

    let buffered = source.buffered_events();
    assert_eq!(buffered.len(), 1);
    assert_eq!(buffered[0].table, "users");
}

#[test]
fn test_federation_gateway() {
    let mut gateway = FederationGateway::new();
    gateway.add_service(
        sz_orm_graphql::async_graphql_integration::federation::FederatedService {
            name: "users".to_string(),
            sdl: "type User { id: ID! name: String }".to_string(),
            url: "http://localhost:4001".to_string(),
        },
    );
    gateway.add_service(
        sz_orm_graphql::async_graphql_integration::federation::FederatedService {
            name: "orders".to_string(),
            sdl: "type Order { id: ID! amount: Float user: User }".to_string(),
            url: "http://localhost:4002".to_string(),
        },
    );

    assert_eq!(gateway.services().len(), 2);
    assert!(gateway.merged_sdl().contains("type User"));
    assert!(gateway.merged_sdl().contains("type Order"));

    let entity = gateway.resolve_entity("users", "User", "1").unwrap();
    assert_eq!(entity["__typename"], "User");
}

#[test]
fn test_ticket_error_with_category() {
    let err = TicketError::new("ERR_001", ErrorCategory::ValidationError, "invalid input");
    assert_eq!(err.code, "ERR_001");
    assert_eq!(err.category, ErrorCategory::ValidationError);
    assert!(err.ticket_id.starts_with("ticket-"));
}

#[test]
fn test_ticket_error_unique_tickets() {
    let err1 = TicketError::internal("ERR_500", "error 1");
    let err2 = TicketError::internal("ERR_500", "error 2");
    assert_ne!(err1.ticket_id, err2.ticket_id);
}

#[test]
fn test_relay_empty_connection() {
    let conn: RelayConnection<u32> = RelayConnection::empty();
    assert!(conn.edges.is_empty());
    assert!(!conn.page_info.has_next_page);
}

#[test]
fn test_page_info() {
    let pi = PageInfo {
        has_next_page: true,
        has_previous_page: false,
        start_cursor: Some("a".to_string()),
        end_cursor: Some("b".to_string()),
    };
    assert!(pi.has_next_page);
    assert!(!pi.has_previous_page);
}
