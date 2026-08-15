#![cfg(feature = "owasp-pentest-suite")]

//! OWASP A04: 不安全设计渗透测试（grpc 包）
//!
//! 对应 REQ-V49-004（OWASP A04）
//!
//! 渗透测试向量：
//! - 缺失重试上限被强制执行：max_retries=3 + 连续失败 10 次，第 4 次后停止重试

use sz_orm_grpc::{GrpcError, RetryPolicy, RetryableErrorKind};

/// A04-1：重试上限被强制执行
///
/// 构造 `RetryPolicy` max_retries=3 + 连续失败 10 次，
/// 断言第 4 次后停止重试；max_retries=0 + 失败，断言不重试直接返回错误。
#[test]
fn a04_missing_retry_limit_enforced() {
    let policy = RetryPolicy {
        max_retries: 3,
        initial_delay_ms: 1,
        max_delay_ms: 10,
        multiplier: 2.0,
        retryable_errors: vec![RetryableErrorKind::ConnectionFailed],
    };

    let error = GrpcError::ConnectionFailed("simulated failure".to_string());

    let mut retry_count = 0;
    for attempt in 0..10 {
        if policy.should_retry(&error, attempt).is_some() {
            retry_count += 1;
        } else {
            break;
        }
    }

    assert_eq!(retry_count, 3, "max_retries=3 时，最多重试 3 次后停止");

    let no_retry_policy = RetryPolicy {
        max_retries: 0,
        initial_delay_ms: 1,
        max_delay_ms: 10,
        multiplier: 2.0,
        retryable_errors: vec![RetryableErrorKind::ConnectionFailed],
    };

    let result = no_retry_policy.should_retry(&error, 0);
    assert!(result.is_none(), "max_retries=0 时，不重试直接返回错误");

    let non_retryable_error = GrpcError::ServiceNotFound("unknown service".to_string());
    let result2 = policy.should_retry(&non_retryable_error, 0);
    assert!(result2.is_none(), "不可重试的错误不触发重试");
}
