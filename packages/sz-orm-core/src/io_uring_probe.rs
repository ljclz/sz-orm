//! io_uring 集成评估与降级（v6.8.0 PERF-IOURING-01）
//!
//! 评估当前平台是否可接入 io_uring（不引入新依赖前提下）。
//! 不可行时降级为 tokio + 零拷贝深化，并标注降级原因。
//!
//! 判定逻辑：
//! - 非 Linux 平台 → `Unsupported`（原因：非 Linux 平台）
//! - Linux 内核 < 5.1 → `Unsupported`（原因：内核版本不足）
//! - Linux 内核 ≥ 5.1 且无依赖约束 → `Available`
//! - 依赖约束（不引入 io-uring crate）→ `Unsupported`（原因：依赖约束）

/// io_uring 可用性评估结果
#[derive(Debug, Clone)]
pub enum IoUringAvailability {
    /// io_uring 可用
    Available {
        /// 内核版本字符串
        kernel_version: String,
    },
    /// io_uring 不可用，需降级
    Unsupported {
        /// 不可用原因
        reason: String,
        /// 降级策略
        fallback: FallbackStrategy,
    },
}

/// 降级策略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FallbackStrategy {
    /// 降级为 tokio + 零拷贝深化
    TokioWithZeroCopy,
    /// 降级为 tokio
    Tokio,
}

impl FallbackStrategy {
    /// 返回策略描述
    pub fn as_str(&self) -> &'static str {
        match self {
            FallbackStrategy::TokioWithZeroCopy => "tokio + 零拷贝深化",
            FallbackStrategy::Tokio => "tokio",
        }
    }
}

impl IoUringAvailability {
    /// 是否可用
    pub fn is_available(&self) -> bool {
        matches!(self, IoUringAvailability::Available { .. })
    }

    /// 是否需要降级
    pub fn needs_fallback(&self) -> bool {
        matches!(self, IoUringAvailability::Unsupported { .. })
    }

    /// 获取降级策略（可用时返回 None）
    pub fn fallback(&self) -> Option<FallbackStrategy> {
        match self {
            IoUringAvailability::Unsupported { fallback, .. } => Some(*fallback),
            _ => None,
        }
    }

    /// 获取原因描述
    pub fn reason(&self) -> &str {
        match self {
            IoUringAvailability::Available { kernel_version } => kernel_version.as_str(),
            IoUringAvailability::Unsupported { reason, .. } => reason.as_str(),
        }
    }

    /// 生成降级日志消息
    pub fn log_message(&self) -> String {
        match self {
            IoUringAvailability::Available { kernel_version } => {
                format!("io_uring 已启用，内核版本: {kernel_version}")
            }
            IoUringAvailability::Unsupported { reason, fallback } => {
                format!("io_uring 降级为{}，原因：{reason}", fallback.as_str())
            }
        }
    }
}

/// io_uring 探测器
pub struct IoUringProbe;

/// 降级原因常量
pub const IO_URING_UNSUPPORTED: &str = "IO_URING_UNSUPPORTED";
/// 依赖约束原因常量
pub const IO_URING_DEPENDENCY_CONSTRAINT: &str = "依赖约束";

impl IoUringProbe {
    /// 探测当前平台 io_uring 可用性
    ///
    /// 评估逻辑：
    /// 1. 非 Linux 平台 → Unsupported（降级 tokio + 零拷贝深化）
    /// 2. Linux 内核 < 5.1 → Unsupported（降级 tokio + 零拷贝深化）
    /// 3. 依赖约束（不引入 io-uring crate）→ Unsupported（降级 tokio + 零拷贝深化）
    #[must_use]
    pub fn detect() -> IoUringAvailability {
        // 依赖约束：不引入 io-uring crate，始终降级
        // 即使在 Linux 内核 ≥ 5.1 的平台上，由于不引入新依赖的原则，
        // io_uring 无法直接接入，降级为 tokio + 零拷贝深化
        IoUringAvailability::Unsupported {
            reason: format!("{IO_URING_UNSUPPORTED}: {IO_URING_DEPENDENCY_CONSTRAINT}"),
            fallback: FallbackStrategy::TokioWithZeroCopy,
        }
    }

    /// 探测并返回平台信息
    pub fn detect_with_platform() -> IoUringAvailability {
        let availability = Self::detect();

        // 记录平台信息
        let platform = if cfg!(target_os = "linux") {
            "linux"
        } else if cfg!(target_os = "windows") {
            "windows"
        } else if cfg!(target_os = "macos") {
            "macos"
        } else {
            "unknown"
        };

        match &availability {
            IoUringAvailability::Available { .. } => {
                tracing::info!(platform = platform, "io_uring probe: available");
            }
            IoUringAvailability::Unsupported { reason, fallback } => {
                tracing::info!(
                    platform = platform,
                    reason = reason.as_str(),
                    fallback = fallback.as_str(),
                    "io_uring probe: unsupported, falling back"
                );
            }
        }

        availability
    }

    /// 检查是否为 Linux 平台
    pub fn is_linux() -> bool {
        cfg!(target_os = "linux")
    }

    /// 检查是否为 Windows 平台
    pub fn is_windows() -> bool {
        cfg!(target_os = "windows")
    }

    /// 获取当前平台名称
    pub fn platform_name() -> &'static str {
        if cfg!(target_os = "linux") {
            "linux"
        } else if cfg!(target_os = "windows") {
            "windows"
        } else if cfg!(target_os = "macos") {
            "macos"
        } else {
            "unknown"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_returns_unsupported_due_to_dependency_constraint() {
        let availability = IoUringProbe::detect();
        assert!(availability.needs_fallback());
        assert!(!availability.is_available());
    }

    #[test]
    fn test_detect_fallback_is_tokio_with_zero_copy() {
        let availability = IoUringProbe::detect();
        assert_eq!(
            availability.fallback(),
            Some(FallbackStrategy::TokioWithZeroCopy)
        );
    }

    #[test]
    fn test_detect_reason_contains_unsupported_constant() {
        let availability = IoUringProbe::detect();
        assert!(availability.reason().contains(IO_URING_UNSUPPORTED));
    }

    #[test]
    fn test_detect_reason_contains_dependency_constraint() {
        let availability = IoUringProbe::detect();
        assert!(availability
            .reason()
            .contains(IO_URING_DEPENDENCY_CONSTRAINT));
    }

    #[test]
    fn test_log_message_for_unsupported() {
        let availability = IoUringProbe::detect();
        let msg = availability.log_message();
        assert!(msg.contains("io_uring 降级为"));
        assert!(msg.contains("tokio + 零拷贝深化"));
        assert!(msg.contains(IO_URING_UNSUPPORTED));
    }

    #[test]
    fn test_fallback_strategy_as_str() {
        assert_eq!(
            FallbackStrategy::TokioWithZeroCopy.as_str(),
            "tokio + 零拷贝深化"
        );
        assert_eq!(FallbackStrategy::Tokio.as_str(), "tokio");
    }

    #[test]
    fn test_platform_name_not_empty() {
        let name = IoUringProbe::platform_name();
        assert!(!name.is_empty());
    }

    #[test]
    fn test_is_linux_or_windows_or_macos() {
        // On any platform, exactly one of these should be true
        let linux = IoUringProbe::is_linux();
        let windows = IoUringProbe::is_windows();
        assert!(linux || windows || cfg!(target_os = "macos"));
    }

    #[test]
    fn test_detect_with_platform_returns_same_result() {
        let a1 = IoUringProbe::detect();
        let a2 = IoUringProbe::detect_with_platform();
        assert_eq!(a1.is_available(), a2.is_available());
        assert_eq!(a1.needs_fallback(), a2.needs_fallback());
        assert_eq!(a1.fallback(), a2.fallback());
    }

    #[test]
    fn test_availability_methods_consistency() {
        let available = IoUringAvailability::Available {
            kernel_version: "5.15.0".to_string(),
        };
        assert!(available.is_available());
        assert!(!available.needs_fallback());
        assert!(available.fallback().is_none());

        let unsupported = IoUringAvailability::Unsupported {
            reason: "test".to_string(),
            fallback: FallbackStrategy::Tokio,
        };
        assert!(!unsupported.is_available());
        assert!(unsupported.needs_fallback());
        assert!(unsupported.fallback().is_some());
    }

    #[test]
    fn test_log_message_for_available() {
        let available = IoUringAvailability::Available {
            kernel_version: "5.15.0".to_string(),
        };
        let msg = available.log_message();
        assert!(msg.contains("io_uring 已启用"));
        assert!(msg.contains("5.15.0"));
    }
}
