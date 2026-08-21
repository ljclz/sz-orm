//! Read-Write Splitting 适配层端到端测试
use sz_orm_core::rw_adapter::{rw_query_count, rw_route_read, rw_route_write};

#[test]
fn test_rw_route_read_returns_slave() {
    let target = rw_route_read();
    assert!(
        !target.is_empty(),
        "read route should return a slave target"
    );
}

#[test]
fn test_rw_route_write_returns_master() {
    let target = rw_route_write();
    assert_eq!(target, "master", "write route should return master");
}

#[test]
fn test_rw_count_increments() {
    let before = rw_query_count();
    let _ = rw_route_read();
    let after = rw_query_count();
    assert!(after > before);
}
