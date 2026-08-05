//! RedisBackend 真实集成测试（需要本地 Redis 服务，默认 ignored）
//!
//! 运行方式：
//! ```bash
//! cargo test -p sz-orm-core --test integration_redis -- --ignored
//! ```
//! 默认连接 `redis://127.0.0.1:6379/0`，可通过环境变量 `SZ_ORM_REDIS_URL` 覆盖。

use std::time::Duration;
use sz_orm_core::l2_cache::{L2CacheBackend, RedisBackend};

fn redis_url() -> String {
    std::env::var("SZ_ORM_REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379/0".to_string())
}

/// 清理测试键，避免影响其他测试
async fn clean(backend: &RedisBackend, keys: &[&str]) {
    for k in keys {
        let _ = backend.delete(k).await;
    }
}

#[tokio::test]
#[ignore]
async fn test_redis_backend_set_get_roundtrip() {
    let backend = RedisBackend::new(redis_url()).await.expect("connect redis");
    let key = "szorm_test:roundtrip";
    clean(&backend, &[key]).await;

    backend.set(key, b"hello".as_slice(), None).await.unwrap();
    let val = backend.get(key).await.unwrap();
    assert_eq!(val, Some(b"hello".to_vec()));
}

#[tokio::test]
#[ignore]
async fn test_redis_backend_set_with_ttl() {
    let backend = RedisBackend::new(redis_url()).await.expect("connect redis");
    let key = "szorm_test:ttl";
    clean(&backend, &[key]).await;

    backend
        .set(key, b"v".as_slice(), Some(Duration::from_secs(1)))
        .await
        .unwrap();
    let val = backend.get(key).await.unwrap();
    assert_eq!(val, Some(b"v".to_vec()));

    // 等待 TTL 过期
    tokio::time::sleep(Duration::from_millis(1100)).await;
    let val = backend.get(key).await.unwrap();
    assert_eq!(val, None, "TTL 过期后键应不存在");
}

#[tokio::test]
#[ignore]
async fn test_redis_backend_delete() {
    let backend = RedisBackend::new(redis_url()).await.expect("connect redis");
    let key = "szorm_test:delete";
    clean(&backend, &[key]).await;

    backend.set(key, b"v".as_slice(), None).await.unwrap();
    assert!(backend.get(key).await.unwrap().is_some());
    backend.delete(key).await.unwrap();
    assert_eq!(backend.get(key).await.unwrap(), None, "删除后键应不存在");
}

#[tokio::test]
#[ignore]
async fn test_redis_backend_invalidate_prefix() {
    let backend = RedisBackend::new(redis_url()).await.expect("connect redis");
    let prefix = "szorm_test:pfx";
    let keys = [
        "szorm_test:pfx:a",
        "szorm_test:pfx:b",
        "szorm_test:pfx:deep:c",
    ];
    clean(&backend, &keys).await;

    for k in keys {
        backend.set(k, b"v".as_slice(), None).await.unwrap();
    }
    // 写入一个不匹配的键，验证不会误删
    let other = "szorm_test:other:x";
    clean(&backend, &[other]).await;
    backend.set(other, b"v".as_slice(), None).await.unwrap();

    backend.invalidate_prefix(prefix).await.unwrap();

    for k in keys {
        assert_eq!(
            backend.get(k).await.unwrap(),
            None,
            "前缀 {} 下键应被删除",
            k
        );
    }
    assert!(
        backend.get(other).await.unwrap().is_some(),
        "非匹配前缀的键不应被删除"
    );
    clean(&backend, &[other]).await;
}
