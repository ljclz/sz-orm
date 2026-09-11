//! CDC-SQLITE-01 接线验证测试（v6.8.0）
//!
//! 验证 SQLite 触发器 → _sz_cdc_events → SqliteHookCapturer → ChangeEvent 端到端管线。

use std::sync::Arc;

use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use sz_orm_core::cdc::event::{ChangeEvent, ChangeEventType};
use sz_orm_core::cdc::sqlite::{SqliteCdcConfig, SqliteHookCapturer};
use tokio::sync::mpsc;

async fn setup_pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::raw_sql(sqlx::AssertSqlSafe(
        "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, email TEXT)".to_string(),
    ))
    .execute(&pool)
    .await
    .unwrap();
    pool
}

#[tokio::test]
async fn wiring_insert_produces_insert_event() {
    let pool = setup_pool().await;
    let capturer = SqliteHookCapturer::new(SqliteCdcConfig {
        watched_tables: vec!["users".to_string()],
        ..Default::default()
    });
    capturer.install_hooks(&pool).await.unwrap();

    sqlx::query("INSERT INTO users (id, name, email) VALUES (1, 'alice', 'alice@test.com')")
        .execute(&pool)
        .await
        .unwrap();

    let (tx, mut rx) = mpsc::channel(100);
    capturer.poll_and_send(&pool, &tx).await.unwrap();

    let event = rx.recv().await.unwrap();
    assert_eq!(event.event_type, ChangeEventType::Insert);
    assert_eq!(event.source_table, "users");
    assert_eq!(event.source_db, "sqlite");
}

#[tokio::test]
async fn wiring_update_produces_update_event() {
    let pool = setup_pool().await;
    let capturer = SqliteHookCapturer::new(SqliteCdcConfig {
        watched_tables: vec!["users".to_string()],
        ..Default::default()
    });
    capturer.install_hooks(&pool).await.unwrap();

    sqlx::query("INSERT INTO users (id, name, email) VALUES (1, 'alice', 'a@t.com')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE users SET name = 'bob' WHERE id = 1")
        .execute(&pool)
        .await
        .unwrap();

    let (tx, mut rx) = mpsc::channel(100);
    capturer.poll_and_send(&pool, &tx).await.unwrap();

    let _insert = rx.recv().await.unwrap();
    let update = rx.recv().await.unwrap();
    assert_eq!(update.event_type, ChangeEventType::Update);
    assert_eq!(update.source_table, "users");
}

#[tokio::test]
async fn wiring_delete_produces_delete_event() {
    let pool = setup_pool().await;
    let capturer = SqliteHookCapturer::new(SqliteCdcConfig {
        watched_tables: vec!["users".to_string()],
        ..Default::default()
    });
    capturer.install_hooks(&pool).await.unwrap();

    sqlx::query("INSERT INTO users (id, name, email) VALUES (1, 'alice', 'a@t.com')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM users WHERE id = 1")
        .execute(&pool)
        .await
        .unwrap();

    let (tx, mut rx) = mpsc::channel(100);
    capturer.poll_and_send(&pool, &tx).await.unwrap();

    let _insert = rx.recv().await.unwrap();
    let delete = rx.recv().await.unwrap();
    assert_eq!(delete.event_type, ChangeEventType::Delete);
    assert_eq!(delete.source_table, "users");
}

#[tokio::test]
async fn wiring_multiple_operations_produce_ordered_events() {
    let pool = setup_pool().await;
    let capturer = SqliteHookCapturer::new(SqliteCdcConfig {
        watched_tables: vec!["users".to_string()],
        ..Default::default()
    });
    capturer.install_hooks(&pool).await.unwrap();

    sqlx::query("INSERT INTO users (id, name, email) VALUES (1, 'a', 'a@t.com')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO users (id, name, email) VALUES (2, 'b', 'b@t.com')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE users SET name = 'c' WHERE id = 1")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM users WHERE id = 2")
        .execute(&pool)
        .await
        .unwrap();

    let (tx, mut rx) = mpsc::channel(100);
    let count = capturer.poll_and_send(&pool, &tx).await.unwrap();
    assert_eq!(count, 4);

    let events: Vec<ChangeEvent> = vec![
        rx.recv().await.unwrap(),
        rx.recv().await.unwrap(),
        rx.recv().await.unwrap(),
        rx.recv().await.unwrap(),
    ];
    assert_eq!(events[0].event_type, ChangeEventType::Insert);
    assert_eq!(events[1].event_type, ChangeEventType::Insert);
    assert_eq!(events[2].event_type, ChangeEventType::Update);
    assert_eq!(events[3].event_type, ChangeEventType::Delete);
}

#[tokio::test]
async fn wiring_position_is_monotonic() {
    let pool = setup_pool().await;
    let capturer = SqliteHookCapturer::new(SqliteCdcConfig {
        watched_tables: vec!["users".to_string()],
        ..Default::default()
    });
    capturer.install_hooks(&pool).await.unwrap();

    for i in 1..=3 {
        let sql = format!(
            "INSERT INTO users (id, name, email) VALUES ({}, 'n{}', 'e{}@t.com')",
            i, i, i
        );
        sqlx::raw_sql(sqlx::AssertSqlSafe(sql))
            .execute(&pool)
            .await
            .unwrap();
    }

    let (tx, mut rx) = mpsc::channel(100);
    capturer.poll_and_send(&pool, &tx).await.unwrap();

    let e1 = rx.recv().await.unwrap();
    let e2 = rx.recv().await.unwrap();
    let e3 = rx.recv().await.unwrap();

    let seq1 = match &e1.position {
        sz_orm_core::cdc::event::ChangePosition::SqliteHook { seq } => *seq,
        _ => 0,
    };
    let seq2 = match &e2.position {
        sz_orm_core::cdc::event::ChangePosition::SqliteHook { seq } => *seq,
        _ => 0,
    };
    let seq3 = match &e3.position {
        sz_orm_core::cdc::event::ChangePosition::SqliteHook { seq } => *seq,
        _ => 0,
    };
    assert!(seq1 < seq2);
    assert!(seq2 < seq3);
}

#[tokio::test]
async fn wiring_poll_twice_does_not_repeat_events() {
    let pool = setup_pool().await;
    let capturer = SqliteHookCapturer::new(SqliteCdcConfig {
        watched_tables: vec!["users".to_string()],
        ..Default::default()
    });
    capturer.install_hooks(&pool).await.unwrap();

    sqlx::query("INSERT INTO users (id, name, email) VALUES (1, 'a', 'a@t.com')")
        .execute(&pool)
        .await
        .unwrap();

    let (tx, mut rx) = mpsc::channel(100);
    let count1 = capturer.poll_and_send(&pool, &tx).await.unwrap();
    assert_eq!(count1, 1);
    let _ = rx.recv().await.unwrap();

    let count2 = capturer.poll_and_send(&pool, &tx).await.unwrap();
    assert_eq!(count2, 0);
}

#[tokio::test]
async fn wiring_last_seq_tracks_progress() {
    let pool = setup_pool().await;
    let capturer = Arc::new(SqliteHookCapturer::new(SqliteCdcConfig {
        watched_tables: vec!["users".to_string()],
        ..Default::default()
    }));
    capturer.install_hooks(&pool).await.unwrap();

    assert_eq!(capturer.last_seq(), 0);

    sqlx::query("INSERT INTO users (id, name, email) VALUES (1, 'a', 'a@t.com')")
        .execute(&pool)
        .await
        .unwrap();

    let (tx, _rx) = mpsc::channel(100);
    capturer.poll_and_send(&pool, &tx).await.unwrap();
    assert!(capturer.last_seq() > 0);
}

#[tokio::test]
async fn wiring_custom_source_db_name() {
    let pool = setup_pool().await;
    let capturer = SqliteHookCapturer::new(SqliteCdcConfig {
        watched_tables: vec!["users".to_string()],
        source_db: "production_db".to_string(),
        ..Default::default()
    });
    capturer.install_hooks(&pool).await.unwrap();

    sqlx::query("INSERT INTO users (id, name, email) VALUES (1, 'a', 'a@t.com')")
        .execute(&pool)
        .await
        .unwrap();

    let (tx, mut rx) = mpsc::channel(100);
    capturer.poll_and_send(&pool, &tx).await.unwrap();

    let event = rx.recv().await.unwrap();
    assert_eq!(event.source_db, "production_db");
}
