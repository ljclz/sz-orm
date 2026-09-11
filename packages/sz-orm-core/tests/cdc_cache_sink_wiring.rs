//! CDC-CACHE-01 接线验证测试（v6.8.0）
//!
//! 验证 CDC 变更事件 → CacheInvalidationSink → DistCacheGateway::invalidate 端到端管线。

use std::sync::Arc;

use serde_json::json;
use sz_orm_core::cdc::dispatcher::CdcEventDispatcher;
use sz_orm_core::cdc::event::{ChangeEvent, ChangeEventType, ChangePosition};
use sz_orm_core::cdc::sinks::cache::{
    CacheInvalidationSink, CacheKeyStrategy, EventTypeFilteredCacheSink, TableFilteredCacheSink,
};
use sz_orm_core::dist_cache_cluster::DistCacheGateway;

fn make_update_event(table: &str, pk: &str, position: u64) -> ChangeEvent {
    ChangeEvent::new(
        ChangeEventType::Update,
        "test_db",
        table,
        json!({ "id": pk, "name": "updated" }),
        ChangePosition::MysqlBinlog {
            filename: "bin.000001".to_string(),
            position,
        },
        1000 + position,
    )
}

fn make_insert_event(table: &str, pk: &str, position: u64) -> ChangeEvent {
    ChangeEvent::new(
        ChangeEventType::Insert,
        "test_db",
        table,
        json!({ "id": pk, "name": "new" }),
        ChangePosition::MysqlBinlog {
            filename: "bin.000001".to_string(),
            position,
        },
        1000 + position,
    )
}

fn make_delete_event(table: &str, pk: &str, position: u64) -> ChangeEvent {
    ChangeEvent::new(
        ChangeEventType::Delete,
        "test_db",
        table,
        json!({ "id": pk }),
        ChangePosition::MysqlBinlog {
            filename: "bin.000001".to_string(),
            position,
        },
        1000 + position,
    )
}

#[tokio::test]
async fn wiring_update_event_invalidates_cache_key() {
    let gateway = Arc::new(DistCacheGateway::with_default());
    let sink = Arc::new(CacheInvalidationSink::new(
        gateway.clone(),
        CacheKeyStrategy::TablePk {
            pk_column: "id".to_string(),
        },
    ));
    let dispatcher = CdcEventDispatcher::new(vec![sink], 100);

    let event = make_update_event("users", "42", 100);
    dispatcher.dispatch(event).await.unwrap();

    assert!(gateway.is_invalidated("users:42"));
    assert!(!gateway.is_invalidated("users:43"));
}

#[tokio::test]
async fn wiring_insert_event_invalidates_cache_key() {
    let gateway = Arc::new(DistCacheGateway::with_default());
    let sink = Arc::new(CacheInvalidationSink::new(
        gateway.clone(),
        CacheKeyStrategy::TablePk {
            pk_column: "id".to_string(),
        },
    ));
    let dispatcher = CdcEventDispatcher::new(vec![sink], 100);

    let event = make_insert_event("orders", "100", 200);
    dispatcher.dispatch(event).await.unwrap();

    assert!(gateway.is_invalidated("orders:100"));
}

#[tokio::test]
async fn wiring_delete_event_invalidates_cache_key() {
    let gateway = Arc::new(DistCacheGateway::with_default());
    let sink = Arc::new(CacheInvalidationSink::new(
        gateway.clone(),
        CacheKeyStrategy::TablePk {
            pk_column: "id".to_string(),
        },
    ));
    let dispatcher = CdcEventDispatcher::new(vec![sink], 100);

    let event = make_delete_event("sessions", "abc", 300);
    dispatcher.dispatch(event).await.unwrap();

    assert!(gateway.is_invalidated("sessions:abc"));
}

#[tokio::test]
async fn wiring_batch_events_invalidate_all_keys() {
    let gateway = Arc::new(DistCacheGateway::with_default());
    let sink = Arc::new(CacheInvalidationSink::new(
        gateway.clone(),
        CacheKeyStrategy::TablePk {
            pk_column: "id".to_string(),
        },
    ));
    let dispatcher = CdcEventDispatcher::new(vec![sink], 100);

    let events: Vec<_> = (1..=10)
        .map(|i| make_update_event("users", &i.to_string(), 100 + i))
        .collect();
    dispatcher.dispatch_batch(events).await.unwrap();

    for i in 1..=10 {
        assert!(gateway.is_invalidated(&format!("users:{}", i)));
    }
    assert_eq!(gateway.invalidated_count(), 10);
}

#[tokio::test]
async fn wiring_table_filter_only_invalidates_watched_tables() {
    let gateway = Arc::new(DistCacheGateway::with_default());
    let sink = Arc::new(TableFilteredCacheSink::new(
        gateway.clone(),
        CacheKeyStrategy::TablePk {
            pk_column: "id".to_string(),
        },
        vec!["users".to_string(), "orders".to_string()],
    ));
    let dispatcher = CdcEventDispatcher::new(vec![sink], 100);

    dispatcher
        .dispatch(make_update_event("users", "1", 100))
        .await
        .unwrap();
    dispatcher
        .dispatch(make_update_event("orders", "2", 200))
        .await
        .unwrap();
    dispatcher
        .dispatch(make_update_event("logs", "3", 300))
        .await
        .unwrap();

    assert!(gateway.is_invalidated("users:1"));
    assert!(gateway.is_invalidated("orders:2"));
    assert!(!gateway.is_invalidated("logs:3"));
}

#[tokio::test]
async fn wiring_event_type_filter_only_invalidates_updates() {
    let gateway = Arc::new(DistCacheGateway::with_default());
    let sink = Arc::new(EventTypeFilteredCacheSink::new(
        gateway.clone(),
        CacheKeyStrategy::TablePk {
            pk_column: "id".to_string(),
        },
        vec![ChangeEventType::Update, ChangeEventType::Delete],
    ));
    let dispatcher = CdcEventDispatcher::new(vec![sink], 100);

    dispatcher
        .dispatch(make_insert_event("users", "1", 100))
        .await
        .unwrap();
    dispatcher
        .dispatch(make_update_event("users", "2", 200))
        .await
        .unwrap();
    dispatcher
        .dispatch(make_delete_event("users", "3", 300))
        .await
        .unwrap();

    assert!(!gateway.is_invalidated("users:1"));
    assert!(gateway.is_invalidated("users:2"));
    assert!(gateway.is_invalidated("users:3"));
}

#[tokio::test]
async fn wiring_wildcard_strategy_invalidates_table_prefix() {
    let gateway = Arc::new(DistCacheGateway::with_default());
    let sink = Arc::new(CacheInvalidationSink::new(
        gateway.clone(),
        CacheKeyStrategy::TableWildcard,
    ));
    let dispatcher = CdcEventDispatcher::new(vec![sink], 100);

    dispatcher
        .dispatch(make_update_event("users", "1", 100))
        .await
        .unwrap();
    dispatcher
        .dispatch(make_update_event("users", "2", 200))
        .await
        .unwrap();

    assert!(gateway.is_invalidated("users:*"));
    assert_eq!(gateway.invalidated_count(), 1);
}

#[tokio::test]
async fn wiring_clear_invalidation_allows_cache_repopulation() {
    let gateway = Arc::new(DistCacheGateway::with_default());
    let sink = Arc::new(CacheInvalidationSink::new(
        gateway.clone(),
        CacheKeyStrategy::TablePk {
            pk_column: "id".to_string(),
        },
    ));
    let dispatcher = CdcEventDispatcher::new(vec![sink], 100);

    dispatcher
        .dispatch(make_update_event("users", "1", 100))
        .await
        .unwrap();
    assert!(gateway.is_invalidated("users:1"));

    gateway.clear_invalidation("users:1");
    assert!(!gateway.is_invalidated("users:1"));
    assert_eq!(gateway.invalidated_count(), 0);
}
