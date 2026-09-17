//! v7.3.0 任务 2.3：ErrorRateCircuitBreaker 测试
//!
//! 验证：错误率超 50% 熔断打开 + 半开放行 N 个探测 + 探测成功恢复/失败重回打开
//!       + 滑动窗口统计 + 状态流转

use std::time::Duration;

use sz_orm_core::circuit_breaker::{CircuitBreaker, CircuitState, ErrorRateCircuitBreaker};

#[test]
fn test_error_rate_below_threshold_stays_closed() {
    // 错误率阈值 50%，窗口 10：3 次失败 = 30% < 50%，不熔断
    let mut cb = ErrorRateCircuitBreaker::new(0.5, Duration::from_secs(60), 1, 10);
    for _ in 0..7 {
        cb.record_success();
    }
    for _ in 0..3 {
        cb.record_failure();
    }
    assert_eq!(cb.state(), CircuitState::Closed);
    assert!((cb.error_rate() - 0.3).abs() < 0.001);
}

#[test]
fn test_error_rate_exceeds_threshold_opens() {
    // 错误率阈值 50%，窗口 10：6 次失败 = 60% > 50%，熔断打开
    let mut cb = ErrorRateCircuitBreaker::new(0.5, Duration::from_secs(60), 1, 10);
    for _ in 0..4 {
        cb.record_success();
    }
    for _ in 0..6 {
        cb.record_failure();
    }
    assert_eq!(cb.state(), CircuitState::Open);
    assert_eq!(cb.total_trips(), 1);
    assert!(!cb.can_execute());
}

#[test]
fn test_half_open_allows_n_probes() {
    // 半开探测数 = 3
    let mut cb = ErrorRateCircuitBreaker::new(0.5, Duration::from_millis(10), 3, 10);
    // 触发熔断
    for _ in 0..6 {
        cb.record_failure();
    }
    for _ in 0..4 {
        cb.record_success();
    }
    assert_eq!(cb.state(), CircuitState::Open);

    // 等待 reset_timeout 进入 HalfOpen
    std::thread::sleep(Duration::from_millis(20));
    assert!(cb.can_execute()); // 第 1 个探测放行
    assert_eq!(cb.state(), CircuitState::HalfOpen);
    assert!(cb.can_execute()); // 第 2 个探测放行
    assert!(cb.can_execute()); // 第 3 个探测放行
    assert!(!cb.can_execute()); // 第 4 个探测快速失败
}

#[test]
fn test_half_open_all_probes_success_closes() {
    let mut cb = ErrorRateCircuitBreaker::new(0.5, Duration::from_millis(10), 2, 10);
    for _ in 0..6 {
        cb.record_failure();
    }
    for _ in 0..4 {
        cb.record_success();
    }
    assert_eq!(cb.state(), CircuitState::Open);

    std::thread::sleep(Duration::from_millis(20));
    assert!(cb.can_execute());
    cb.record_success(); // 探测 1 成功
    assert_eq!(cb.state(), CircuitState::HalfOpen);

    assert!(cb.can_execute());
    cb.record_success(); // 探测 2 成功
    assert_eq!(cb.state(), CircuitState::Closed);
}

#[test]
fn test_half_open_any_failure_reopens() {
    let mut cb = ErrorRateCircuitBreaker::new(0.5, Duration::from_millis(10), 3, 10);
    for _ in 0..6 {
        cb.record_failure();
    }
    for _ in 0..4 {
        cb.record_success();
    }
    assert_eq!(cb.state(), CircuitState::Open);

    std::thread::sleep(Duration::from_millis(20));
    assert!(cb.can_execute());
    cb.record_success(); // 探测 1 成功
    assert_eq!(cb.state(), CircuitState::HalfOpen);

    assert!(cb.can_execute());
    cb.record_failure(); // 探测 2 失败，立即重回 Open
    assert_eq!(cb.state(), CircuitState::Open);
}

#[test]
fn test_sliding_window_evicts_old_samples() {
    // 窗口 5：前 5 次全失败（错误率 100%），后 5 次全成功（错误率 0%）
    let mut cb = ErrorRateCircuitBreaker::new(0.99, Duration::from_secs(60), 1, 5);
    for _ in 0..5 {
        cb.record_failure();
    }
    assert_eq!(cb.state(), CircuitState::Open);

    // 重置后测试滑动窗口淘汰
    cb.reset();
    for _ in 0..5 {
        cb.record_success();
    }
    assert_eq!(cb.state(), CircuitState::Closed);
    assert_eq!(cb.error_rate(), 0.0);
    assert_eq!(cb.window_samples(), 5);
}

#[test]
fn test_state_transition_full_cycle() {
    let mut cb = ErrorRateCircuitBreaker::new(0.5, Duration::from_millis(10), 1, 4);
    // Closed → Open：4 次失败 = 100% > 50%
    for _ in 0..4 {
        cb.record_failure();
    }
    assert_eq!(cb.state(), CircuitState::Open);

    // Open → HalfOpen：等待 reset_timeout
    std::thread::sleep(Duration::from_millis(20));
    assert!(cb.can_execute());
    assert_eq!(cb.state(), CircuitState::HalfOpen);

    // HalfOpen → Closed：探测成功
    cb.record_success();
    assert_eq!(cb.state(), CircuitState::Closed);

    // Closed → Open 再次熔断
    for _ in 0..4 {
        cb.record_failure();
    }
    assert_eq!(cb.state(), CircuitState::Open);
    assert_eq!(cb.total_trips(), 2);
}