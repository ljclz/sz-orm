//! 分布式锁端到端接线测试

use std::sync::Arc;
use std::time::Duration;

use sz_orm_dtx::coordination::{
    CoordinationBackend, DistributedLock, InMemoryBackend, LockWatchdog,
};

#[tokio::test]
async fn wiring_lock_acquire_release() {
    let backend: Arc<dyn CoordinationBackend> = Arc::new(InMemoryBackend::new());
    let lock = DistributedLock::new(backend, "wiring_lock", 10);
    lock.acquire().await.unwrap();
    assert!(lock.is_held());
    lock.release().await.unwrap();
    assert!(!lock.is_held());
}

#[tokio::test]
async fn wiring_lock_reentrant() {
    let backend: Arc<dyn CoordinationBackend> = Arc::new(InMemoryBackend::new());
    let lock = DistributedLock::new(backend, "reentrant", 10);
    lock.acquire().await.unwrap();
    lock.acquire().await.unwrap();
    lock.release().await.unwrap();
    assert!(lock.is_held());
    lock.release().await.unwrap();
    assert!(!lock.is_held());
}

#[tokio::test]
async fn wiring_lock_contention() {
    let backend: Arc<dyn CoordinationBackend> = Arc::new(InMemoryBackend::new());
    let lock1 = DistributedLock::new(backend.clone(), "contended", 10);
    let lock2 = DistributedLock::new(backend, "contended", 10);
    lock1.acquire().await.unwrap();
    assert!(lock2.try_acquire().await.unwrap().is_none());
    lock1.release().await.unwrap();
    assert!(lock2.try_acquire().await.unwrap().is_some());
}

#[tokio::test]
async fn wiring_watchdog_keeps_lock() {
    let backend: Arc<dyn CoordinationBackend> = Arc::new(InMemoryBackend::new());
    let lock = Arc::new(DistributedLock::new(backend, "wd_wiring", 3));
    lock.acquire().await.unwrap();
    let watchdog = LockWatchdog::new(lock.clone(), 3);
    let handle = watchdog.start().await.unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(lock.is_held());
    watchdog.stop().await;
    handle.abort();
}

#[tokio::test]
async fn wiring_lock_renew() {
    let backend: Arc<dyn CoordinationBackend> = Arc::new(InMemoryBackend::new());
    let lock = DistributedLock::new(backend, "renew_wiring", 10);
    lock.acquire().await.unwrap();
    assert!(lock.renew().await.unwrap());
    lock.release().await.unwrap();
}

#[tokio::test]
async fn wiring_multiple_locks_independent() {
    let backend: Arc<dyn CoordinationBackend> = Arc::new(InMemoryBackend::new());
    let lock1 = DistributedLock::new(backend.clone(), "key_a", 10);
    let lock2 = DistributedLock::new(backend, "key_b", 10);
    lock1.acquire().await.unwrap();
    lock2.acquire().await.unwrap();
    assert!(lock1.is_held());
    assert!(lock2.is_held());
    lock1.release().await.unwrap();
    lock2.release().await.unwrap();
}
