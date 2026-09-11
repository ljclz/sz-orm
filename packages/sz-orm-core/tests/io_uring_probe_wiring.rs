//! W3-5 PERF-IOURING-01：io_uring 集成评估与降级接线测试
//!
//! 验证 `IoUringProbe::detect` 端到端接线：
//! - 非 Linux / 依赖约束 → 返回 `Unsupported` + 降级 tokio + 零拷贝深化
//! - 日志标注 `IO_URING_UNSUPPORTED` + 降级原因
//!
//! 生产入口：`IoUringProbe::detect`（packages/sz-orm-core/src/io_uring_probe.rs）

use sz_orm_core::io_uring_probe::{
    FallbackStrategy, IoUringProbe, IO_URING_DEPENDENCY_CONSTRAINT, IO_URING_UNSUPPORTED,
};

#[test]
fn test_probe_detect_returns_unsupported() {
    let availability = IoUringProbe::detect();
    assert!(availability.needs_fallback());
    assert!(!availability.is_available());
}

#[test]
fn test_probe_fallback_is_tokio_with_zero_copy() {
    let availability = IoUringProbe::detect();
    assert_eq!(
        availability.fallback(),
        Some(FallbackStrategy::TokioWithZeroCopy)
    );
}

#[test]
fn test_probe_reason_contains_io_uring_unsupported() {
    let availability = IoUringProbe::detect();
    assert!(
        availability.reason().contains(IO_URING_UNSUPPORTED),
        "reason should contain IO_URING_UNSUPPORTED constant"
    );
}

#[test]
fn test_probe_reason_contains_dependency_constraint() {
    let availability = IoUringProbe::detect();
    assert!(
        availability
            .reason()
            .contains(IO_URING_DEPENDENCY_CONSTRAINT),
        "reason should contain dependency constraint"
    );
}

#[test]
fn test_probe_log_message_contains_degradation() {
    let availability = IoUringProbe::detect();
    let msg = availability.log_message();
    assert!(
        msg.contains("io_uring 降级为"),
        "log should mention degradation: {msg}"
    );
    assert!(
        msg.contains("tokio + 零拷贝深化"),
        "log should mention fallback strategy: {msg}"
    );
}

#[test]
fn test_probe_platform_name_valid() {
    let name = IoUringProbe::platform_name();
    assert!(
        matches!(name, "linux" | "windows" | "macos" | "unknown"),
        "platform name should be valid: {name}"
    );
}

#[test]
fn test_probe_detect_with_platform_consistent() {
    let a1 = IoUringProbe::detect();
    let a2 = IoUringProbe::detect_with_platform();
    assert_eq!(a1.is_available(), a2.is_available());
    assert_eq!(a1.fallback(), a2.fallback());
}

#[test]
fn test_probe_non_linux_returns_unsupported() {
    // On Windows (our build platform), io_uring should be unsupported
    if IoUringProbe::is_windows() {
        let availability = IoUringProbe::detect();
        assert!(availability.needs_fallback());
        assert_eq!(
            availability.fallback(),
            Some(FallbackStrategy::TokioWithZeroCopy)
        );
    }
}
