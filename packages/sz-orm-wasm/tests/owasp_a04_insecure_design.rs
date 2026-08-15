#![cfg(all(feature = "owasp-pentest-suite", feature = "wasm-real-db"))]

//! OWASP A04: 不安全设计渗透测试（wasm 包）
//!
//! 对应 REQ-V49-004（OWASP A04）
//!
//! 渗透测试向量：
//! - 缺失限流被强制执行：1000 次登录尝试 + 限流 100/min，第 101 次拒绝

use std::time::Duration;
use sz_orm_wasm::real_db::rate_limiter::WasmDbRateLimiter;

/// A04-1：限流被强制执行
///
/// 构造 1000 次登录尝试 + 限流 100/window，
/// 断言第 101 次拒绝。
#[test]
fn a04_missing_rate_limiting_enforced() {
    let mut limiter = WasmDbRateLimiter::with_window(100, Duration::from_secs(60));

    let mut allowed = 0;
    let mut denied = 0;

    for _ in 0..1000 {
        if limiter.check_and_increment() {
            allowed += 1;
        } else {
            denied += 1;
        }
    }

    assert_eq!(allowed, 100, "前 100 次请求必须被允许");
    assert_eq!(denied, 900, "后 900 次请求必须被限流拒绝");
    assert_eq!(limiter.current_count(), 100, "当前计数为 100");
}
