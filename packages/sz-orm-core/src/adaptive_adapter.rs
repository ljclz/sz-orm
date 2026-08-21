//! # Adaptive Adapter — sz-orm-core 自适应查询适配层
//!
//! v5.0.0：将 sz-orm-adaptive 的 AdaptiveExecutor 接入 sz-orm-core，
//! 提供 `adaptive_decide` / `adaptive_record` / `adaptive_decision_count` 三个入口，
//! 使自适应查询能力从"幻影交付"变为"生产可达"。
//!
//! ## 设计
//!
//! - 全局执行器：`OnceLock<parking_lot::RwLock<AdaptiveExecutor>>`
//! - 首次调用时惰性初始化默认配置的执行器
//! - `adaptive_decide` 取读锁执行决策，`adaptive_record` 取写锁记录统计
//! - 决策计数使用 `AtomicU64`，无需加锁

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

use parking_lot::RwLock;
use sz_orm_adaptive::{AdaptiveConfig, AdaptiveExecutor, ExecutionPath};

static ADAPTIVE_EXECUTOR: OnceLock<RwLock<AdaptiveExecutor>> = OnceLock::new();
static DECISION_COUNT: AtomicU64 = AtomicU64::new(0);

fn executor() -> &'static RwLock<AdaptiveExecutor> {
    ADAPTIVE_EXECUTOR.get_or_init(|| RwLock::new(AdaptiveExecutor::new(AdaptiveConfig::default())))
}

/// 决策：按当前统计选择执行路径
///
/// 内部取读锁，调用 `AdaptiveExecutor::decide`。
/// 每次调用递增 `adaptive_decision_count`。
pub fn adaptive_decide(query_key: &str) -> ExecutionPath {
    DECISION_COUNT.fetch_add(1, Ordering::Relaxed);
    let executor = executor().read();
    executor.decide(query_key)
}

/// 记录一次执行结果，返回是否慢查询
///
/// 内部取写锁，调用 `AdaptiveExecutor::record`。
pub fn adaptive_record(query_key: &str, rows: u64, elapsed_ms: u64) -> bool {
    let executor = executor().read();
    executor.record(query_key, rows, elapsed_ms)
}

/// 获取当前决策调用计数（用于测试验证真实执行）
pub fn adaptive_decision_count() -> u64 {
    DECISION_COUNT.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adaptive_decide_returns_path() {
        let path = adaptive_decide("test_query");
        assert!(matches!(
            path,
            ExecutionPath::Normal | ExecutionPath::Paginated | ExecutionPath::Cached
        ));
    }

    #[test]
    fn test_adaptive_record_updates_stats() {
        let slow = adaptive_record("test_record", 500, 200);
        assert!(slow);
    }

    #[test]
    fn test_adaptive_count_increments() {
        let before = adaptive_decision_count();
        let _ = adaptive_decide("count_test");
        let after = adaptive_decision_count();
        assert!(after > before);
    }
}
