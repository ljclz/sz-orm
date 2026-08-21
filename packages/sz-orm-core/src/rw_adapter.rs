//! # Read-Write Splitting Adapter — sz-orm-core 读写分离适配层
//!
//! v5.0.0 M4：将 sz-orm-rw 的 ReadWriteRouter 接入 sz-orm-core，
//! 提供 `rw_route_read` / `rw_route_write` / `rw_query_count` 入口。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

use parking_lot::RwLock;
use sz_orm_rw::ReadWriteRouter;

static ROUTER: OnceLock<RwLock<ReadWriteRouter>> = OnceLock::new();
static QUERY_COUNT: AtomicU64 = AtomicU64::new(0);

fn router() -> &'static RwLock<ReadWriteRouter> {
    ROUTER.get_or_init(|| RwLock::new(ReadWriteRouter::new("master", vec!["slave1", "slave2"])))
}

/// 路由读请求到从库
pub fn rw_route_read() -> String {
    QUERY_COUNT.fetch_add(1, Ordering::Relaxed);
    let router = router().read();
    router.slave().to_string()
}

/// 路由写请求到主库
pub fn rw_route_write() -> String {
    QUERY_COUNT.fetch_add(1, Ordering::Relaxed);
    let router = router().read();
    router.master().to_string()
}

/// 获取路由计数
pub fn rw_query_count() -> u64 {
    QUERY_COUNT.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
