//! v9.2.0 M7-T41：WriteBehindWriter write/delete/flush/spawn_auto_flush（4 tests）

use std::sync::Arc;
use std::time::Duration;
use sz_orm_core::l2_cache::{FlushCallback, InMemoryBackend, L2CacheBackend, WriteBehindWriter};
use sz_orm_core::CacheError;

fn noop_flush() -> FlushCallback {
    Arc::new(|ops| {
        Box::pin(async move {
            let _ = ops;
            Ok::<(), CacheError>(())
        })
    })
}

#[tokio::test]
async fn test_write_behind_write_updates_cache_and_enqueues() {
    let backend = Arc::new(InMemoryBackend::new());
    let writer = WriteBehindWriter::new(backend.clone(), noop_flush());
    writer.write(b"key1", b"value1", None).await.unwrap();
    let cached = backend.get("key1").await.unwrap();
    assert_eq!(cached, Some(b"value1".to_vec()));
    assert_eq!(writer.pending_count().await, 1);
}

#[tokio::test]
async fn test_write_behind_delete_removes_from_cache_and_enqueues() {
    let backend = Arc::new(InMemoryBackend::new());
    backend.set("key1", b"value1", None).await.unwrap();
    let writer = WriteBehindWriter::new(backend.clone(), noop_flush());
    writer.delete(b"key1").await.unwrap();
    let cached = backend.get("key1").await.unwrap();
    assert!(cached.is_none(), "delete 应立即从缓存移除");
    assert_eq!(writer.pending_count().await, 1);
}

#[tokio::test]
async fn test_write_behind_flush_drains_queue() {
    let backend = Arc::new(InMemoryBackend::new());
    let writer = WriteBehindWriter::new(backend.clone(), noop_flush());
    writer.write(b"k1", b"v1", None).await.unwrap();
    writer.write(b"k2", b"v2", None).await.unwrap();
    assert_eq!(writer.pending_count().await, 2);
    writer.flush().await.unwrap();
    assert_eq!(writer.pending_count().await, 0, "flush 后队列应清空");
    writer.flush().await.unwrap();
}

#[tokio::test]
async fn test_write_behind_spawn_auto_flush_runs_periodically() {
    let backend = Arc::new(InMemoryBackend::new());
    let writer = Arc::new(WriteBehindWriter::new(backend.clone(), noop_flush()));
    writer.write(b"k", b"v", None).await.unwrap();
    let handle = writer.clone().spawn_auto_flush(Duration::from_millis(50));
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(
        writer.pending_count().await,
        0,
        "自动刷新应清空队列"
    );
    handle.abort();
}