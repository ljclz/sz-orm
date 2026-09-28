//! T23: OraclePoolHandle 池操作集成测试（4 tests，#[ignore]）
//!
//! 需要本机 Oracle 23ai Free: 127.0.0.1:1521/freepdb1.FALSE (sz_orm_test/SzOrmTest2026)
//! 运行: cargo test -p sz-orm-oracle --test oracle_pool_test -- --ignored

use sz_orm_oracle::{OracleBlockingPoolConfig, OraclePoolHandle};

const USER: &str = "sz_orm_test";
const PWD: &str = "SzOrmTest2026";
const CS: &str = "127.0.0.1:1521/freepdb1.FALSE";

#[test]
#[ignore]
fn test_oracle_pool_connect_default() {
    let pool = OraclePoolHandle::connect(USER, PWD, CS).expect("connect");
    assert_eq!(pool.connect_string(), CS);
    assert_eq!(pool.max_size(), 10);
}

#[test]
#[ignore]
fn test_oracle_pool_connect_with_pool_custom_config() {
    let config = OracleBlockingPoolConfig {
        max_blocking_threads: 32,
    };
    let pool = OraclePoolHandle::connect_with_pool(USER, PWD, CS, config).expect("connect");
    assert_eq!(pool.max_size(), 10);
}

#[test]
#[ignore]
fn test_oracle_pool_connect_with_max_size() {
    let config = OracleBlockingPoolConfig::default();
    let pool =
        OraclePoolHandle::connect_with_max_size(USER, PWD, CS, config.clone(), 5).expect("connect");
    assert_eq!(pool.max_size(), 5);

    let pool_zero =
        OraclePoolHandle::connect_with_max_size(USER, PWD, CS, config, 0).expect("connect");
    assert_eq!(pool_zero.max_size(), 10);
}

#[test]
#[ignore]
fn test_oracle_pool_acquire_and_reuse() {
    let pool = OraclePoolHandle::connect(USER, PWD, CS).expect("connect");
    let guard = pool.acquire().expect("acquire");
    assert_eq!(pool.connect_string(), CS);
    drop(guard);
    let guard2 = pool.acquire().expect("acquire reuse");
    assert_eq!(pool.connect_string(), CS);
    drop(guard2);
}