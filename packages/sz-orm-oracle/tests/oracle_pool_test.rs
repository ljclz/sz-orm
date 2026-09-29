//! T23: OraclePoolHandle 池操作集成测试（4 tests，#[ignore]）
//!
//! 需要本机 Oracle 23ai Free: 127.0.0.1:1521/freepdb1.FALSE (sz_orm_test/SzOrmTest2026)
//! 运行: cargo test -p sz-orm-oracle --test oracle_pool_test -- --ignored

use sz_orm_oracle::{OracleBlockingPoolConfig, OraclePoolHandle};

fn user() -> String {
    std::env::var("SZ_ORM_ORACLE_USER").unwrap_or_else(|_| "sz_orm_test".to_string())
}

fn pwd() -> String {
    std::env::var("SZ_ORM_ORACLE_PASSWORD").unwrap_or_else(|_| "SzOrmTest2026".to_string())
}

fn cs() -> String {
    std::env::var("SZ_ORM_ORACLE_CONNECT_STRING")
        .unwrap_or_else(|_| "127.0.0.1:1521/freepdb1.FALSE".to_string())
}

#[test]
#[ignore]
fn test_oracle_pool_connect_default() {
    let cs = cs();
    let pool = OraclePoolHandle::connect(&user(), &pwd(), &cs).expect("connect");
    assert_eq!(pool.connect_string(), &cs);
    assert_eq!(pool.max_size(), 10);
}

#[test]
#[ignore]
fn test_oracle_pool_connect_with_pool_custom_config() {
    let config = OracleBlockingPoolConfig {
        max_blocking_threads: 32,
    };
    let pool = OraclePoolHandle::connect_with_pool(&user(), &pwd(), &cs(), config).expect("connect");
    assert_eq!(pool.max_size(), 10);
}

#[test]
#[ignore]
fn test_oracle_pool_connect_with_max_size() {
    let config = OracleBlockingPoolConfig::default();
    let cs = cs();
    let pool =
        OraclePoolHandle::connect_with_max_size(&user(), &pwd(), &cs, config.clone(), 5).expect("connect");
    assert_eq!(pool.max_size(), 5);

    let pool_zero =
        OraclePoolHandle::connect_with_max_size(&user(), &pwd(), &cs, config, 0).expect("connect");
    assert_eq!(pool_zero.max_size(), 10);
}

#[test]
#[ignore]
fn test_oracle_pool_acquire_and_reuse() {
    let cs = cs();
    let pool = OraclePoolHandle::connect(&user(), &pwd(), &cs).expect("connect");
    let guard = pool.acquire().expect("acquire");
    assert_eq!(pool.connect_string(), &cs);
    drop(guard);
    let guard2 = pool.acquire().expect("acquire reuse");
    assert_eq!(pool.connect_string(), &cs);
    drop(guard2);
}