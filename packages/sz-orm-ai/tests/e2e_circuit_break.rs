//! 端到端测试：连续失败 5 次熔断 30min，熔断期间仅记录不执行
//!
//! 验证 AutonomousCircuitBreaker 的状态机：
//! Closed → (5 次失败) → Open → (30min 后) → HalfOpen → (成功) → Closed

use std::time::Duration;

use sz_orm_ai::autonomous::{AutonomousCircuitBreaker, CircuitBreakerConfig, CircuitState};

/// 验证连续失败 5 次后熔断
#[test]
fn e2e_circuit_break_after_5_failures() {
    let config = CircuitBreakerConfig {
        failure_threshold: 5,
        break_duration: Duration::from_secs(1800),
    };
    let mut cb = AutonomousCircuitBreaker::new(config);

    for i in 1..=4 {
        cb.record_failure();
        assert_eq!(
            *cb.state(),
            CircuitState::Closed,
            "第 {} 次失败后应仍为 Closed",
            i
        );
        assert!(cb.allow_request(), "第 {} 次失败后应仍允许请求", i);
    }

    cb.record_failure();
    assert_eq!(
        *cb.state(),
        CircuitState::Open,
        "第 5 次失败后应熔断为 Open"
    );
    assert!(!cb.allow_request(), "熔断后应拒绝请求");
}

/// 验证熔断期间仅记录不执行
#[test]
fn e2e_circuit_break_blocks_during_open() {
    let config = CircuitBreakerConfig {
        failure_threshold: 2,
        break_duration: Duration::from_secs(1800),
    };
    let mut cb = AutonomousCircuitBreaker::new(config);

    cb.record_failure();
    cb.record_failure();
    assert_eq!(*cb.state(), CircuitState::Open);

    for i in 1..=10 {
        assert!(!cb.allow_request(), "熔断期间第 {} 次请求应被拒绝", i);
    }
}

/// 验证熔断恢复后重新放行
#[test]
fn e2e_circuit_break_recovery_to_half_open() {
    let config = CircuitBreakerConfig {
        failure_threshold: 1,
        break_duration: Duration::from_millis(10),
    };
    let mut cb = AutonomousCircuitBreaker::new(config);

    cb.record_failure();
    assert_eq!(*cb.state(), CircuitState::Open);

    std::thread::sleep(Duration::from_millis(20));
    assert!(cb.allow_request(), "熔断恢复后应放行");
    assert_eq!(*cb.state(), CircuitState::HalfOpen);
}

/// 验证 HalfOpen 状态下成功后回到 Closed
#[test]
fn e2e_circuit_break_half_open_success_to_closed() {
    let config = CircuitBreakerConfig {
        failure_threshold: 1,
        break_duration: Duration::from_millis(10),
    };
    let mut cb = AutonomousCircuitBreaker::new(config);

    cb.record_failure();
    std::thread::sleep(Duration::from_millis(20));
    cb.allow_request();
    assert_eq!(*cb.state(), CircuitState::HalfOpen);

    cb.record_success();
    assert_eq!(*cb.state(), CircuitState::Closed);
    assert!(cb.allow_request());
}

/// 验证手动熔断安全阀
#[test]
fn e2e_circuit_break_manual_break() {
    let mut cb = AutonomousCircuitBreaker::default();

    cb.manual_break();
    assert_eq!(*cb.state(), CircuitState::Open);
    assert!(!cb.allow_request());

    cb.manual_reset();
    assert_eq!(*cb.state(), CircuitState::Closed);
    assert!(cb.allow_request());
}
