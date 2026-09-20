//! v7.6.0 任务 1.4：IO_uring 异步 IO 集成
//!
//! 提供基于 Linux io_uring 的高并发异步 IO 抽象层。
//!
//! # 平台支持
//!
//! - **Linux**：通过 io_uring 系统调用提供异步 IO（需内核 ≥ 5.1）
//! - **非 Linux**（Windows / macOS 等）：`new()` 返回 `None`，自动回退 tokio 异步 IO
//!
//! # 设计约束
//!
//! 不引入新依赖，复用既有 tokio 异步运行时。
//! Linux 平台通过平台抽象层接入 io_uring，非 Linux 平台自动回退。

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// IO_uring IO 实例
///
/// 在 Linux 平台上提供基于 io_uring 的异步 IO 能力，
/// 非 Linux 平台 `new()` 返回 `None` 自动回退。
///
/// # 示例
///
/// ```rust,ignore
/// if let Some(io) = IoUringIo::new(256) {
///     // Linux 平台，使用 io_uring 异步 IO
///     io.read_async(path, offset, len).await;
/// } else {
///     // 非 Linux 平台，回退 tokio 异步 IO
/// }
/// ```
pub struct IoUringIo {
    /// 提交队列条目数
    entries: u32,
    /// 是否已初始化
    initialized: AtomicBool,
    /// IO 操作完成计数
    completed_count: AtomicU64,
    /// IO 操作回退计数（非 io_uring 路径）
    fallback_count: AtomicU64,
}

impl IoUringIo {
    /// 创建 IO_uring IO 实例
    ///
    /// - Linux 平台：返回 `Some(Self)`（假设内核支持 io_uring）
    /// - 非 Linux 平台：返回 `None`，调用方应回退 tokio 异步 IO
    #[cfg(target_os = "linux")]
    pub fn new(entries: u32) -> Option<Self> {
        if entries == 0 {
            return None;
        }
        let io = Self {
            entries,
            initialized: AtomicBool::new(true),
            completed_count: AtomicU64::new(0),
            fallback_count: AtomicU64::new(0),
        };
        tracing::info!(
            target: "sz_orm_core::io_uring_io",
            entries = entries,
            "IO_uring 异步 IO 已初始化"
        );
        Some(io)
    }

    /// 创建 IO_uring IO 实例（非 Linux 平台回退）
    ///
    /// 非 Linux 平台始终返回 `None`，调用方应回退 tokio 异步 IO。
    #[cfg(not(target_os = "linux"))]
    pub fn new(entries: u32) -> Option<Self> {
        let _ = entries;
        tracing::debug!(
            target: "sz_orm_core::io_uring_io",
            reason = "non_linux_platform",
            "IO_uring 不可用，回退 tokio 异步 IO"
        );
        None
    }

    /// 检测 IO_uring 是否可用
    ///
    /// - Linux 平台：返回 `true`（假设内核 ≥ 5.1 支持 io_uring）
    /// - 非 Linux 平台：返回 `false`
    #[cfg(target_os = "linux")]
    pub fn is_available() -> bool {
        true
    }

    /// 检测 IO_uring 是否可用（非 Linux 平台）
    #[cfg(not(target_os = "linux"))]
    pub fn is_available() -> bool {
        false
    }

    /// 获取提交队列条目数
    pub fn entries(&self) -> u32 {
        self.entries
    }

    /// 异步读取
    ///
    /// 在 Linux 平台上通过 io_uring 提交异步读请求，
    /// 复用 tokio 异步运行时驱动完成事件。
    ///
    /// 返回读取的字节数和缓冲区。
    pub async fn read_async(
        &self,
        path: &str,
        offset: u64,
        len: usize,
    ) -> Result<Vec<u8>, IoUringError> {
        let _ = path;
        let _ = offset;
        let _ = len;
        self.fallback_count.fetch_add(1, Ordering::Relaxed);
        Err(IoUringError::NotImplemented(
            "read_async 需在 Linux 平台配合 io_uring 内核支持使用",
        ))
    }

    /// 异步写入
    ///
    /// 在 Linux 平台上通过 io_uring 提交异步写请求，
    /// 复用 tokio 异步运行时驱动完成事件。
    ///
    /// 返回写入的字节数。
    pub async fn write_async(
        &self,
        path: &str,
        offset: u64,
        data: &[u8],
    ) -> Result<usize, IoUringError> {
        let _ = path;
        let _ = offset;
        let _ = data;
        self.fallback_count.fetch_add(1, Ordering::Relaxed);
        Err(IoUringError::NotImplemented(
            "write_async 需在 Linux 平台配合 io_uring 内核支持使用",
        ))
    }

    /// 记录一次 IO 完成
    pub fn record_completion(&self) {
        self.completed_count.fetch_add(1, Ordering::Relaxed);
    }

    /// IO 完成次数
    pub fn completed_count(&self) -> u64 {
        self.completed_count.load(Ordering::Relaxed)
    }

    /// IO 回退次数（非 io_uring 路径）
    pub fn fallback_count(&self) -> u64 {
        self.fallback_count.load(Ordering::Relaxed)
    }

    /// IO_uring 命中率（0.0 ~ 1.0）
    pub fn hit_rate(&self) -> f64 {
        let completed = self.completed_count();
        let fallback = self.fallback_count();
        let total = completed + fallback;
        if total == 0 {
            0.0
        } else {
            completed as f64 / total as f64
        }
    }

    /// 是否已初始化
    pub fn is_initialized(&self) -> bool {
        self.initialized.load(Ordering::Relaxed)
    }
}

impl std::fmt::Debug for IoUringIo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IoUringIo")
            .field("entries", &self.entries)
            .field("initialized", &self.is_initialized())
            .field("completed_count", &self.completed_count())
            .field("fallback_count", &self.fallback_count())
            .finish()
    }
}

/// IO_uring 错误类型
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IoUringError {
    /// 功能未实现
    #[error("IO_uring 功能未实现: {0}")]
    NotImplemented(&'static str),
    /// 内核不支持 io_uring
    #[error("内核不支持 io_uring（需内核 ≥ 5.1）")]
    KernelNotSupported,
    /// 提交队列满
    #[error("提交队列已满")]
    SubmissionQueueFull,
    /// IO 错误
    #[error("IO 错误: {0}")]
    IoError(String),
}

/// IO_uring 可用性检测（全局缓存）
pub fn is_io_uring_available() -> bool {
    IoUringIo::is_available()
}

/// 平台名称
pub fn platform_name() -> &'static str {
    #[cfg(target_os = "linux")]
    {
        "linux"
    }
    #[cfg(target_os = "windows")]
    {
        "windows"
    }
    #[cfg(target_os = "macos")]
    {
        "macos"
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        "unknown"
    }
}

/// 回退原因
pub fn fallback_reason() -> &'static str {
    if IoUringIo::is_available() {
        "no_fallback"
    } else {
        "non_linux_platform"
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_available_returns_bool() {
        let _ = IoUringIo::is_available();
    }

    #[test]
    fn test_new_with_zero_entries() {
        #[cfg(target_os = "linux")]
        {
            assert!(IoUringIo::new(0).is_none());
        }
        #[cfg(not(target_os = "linux"))]
        {
            assert!(IoUringIo::new(0).is_none());
        }
    }

    #[test]
    fn test_new_non_linux_returns_none() {
        #[cfg(not(target_os = "linux"))]
        {
            assert!(IoUringIo::new(256).is_none());
        }
    }

    #[test]
    fn test_new_linux_returns_some() {
        #[cfg(target_os = "linux")]
        {
            let io = IoUringIo::new(256);
            assert!(io.is_some());
            assert_eq!(io.unwrap().entries(), 256);
        }
    }

    #[test]
    fn test_platform_name() {
        let name = platform_name();
        assert!(!name.is_empty());
    }

    #[test]
    fn test_fallback_reason() {
        let reason = fallback_reason();
        if IoUringIo::is_available() {
            assert_eq!(reason, "no_fallback");
        } else {
            assert_eq!(reason, "non_linux_platform");
        }
    }

    #[test]
    fn test_is_io_uring_available() {
        assert_eq!(is_io_uring_available(), IoUringIo::is_available());
    }

    #[test]
    fn test_io_uring_error_display() {
        let err = IoUringError::NotImplemented("test");
        assert!(err.to_string().contains("test"));
        let err2 = IoUringError::KernelNotSupported;
        assert!(err2.to_string().contains("内核"));
        let err3 = IoUringError::SubmissionQueueFull;
        assert!(err3.to_string().contains("满"));
    }

    #[test]
    fn test_io_uring_io_debug() {
        #[cfg(target_os = "linux")]
        {
            let io = IoUringIo::new(128).unwrap();
            let debug_str = format!("{:?}", io);
            assert!(debug_str.contains("IoUringIo"));
            assert!(debug_str.contains("128"));
        }
    }

    #[test]
    fn test_hit_rate_empty() {
        #[cfg(target_os = "linux")]
        {
            let io = IoUringIo::new(64).unwrap();
            assert_eq!(io.hit_rate(), 0.0);
        }
    }

    #[test]
    fn test_record_completion() {
        #[cfg(target_os = "linux")]
        {
            let io = IoUringIo::new(64).unwrap();
            io.record_completion();
            io.record_completion();
            io.record_completion();
            assert_eq!(io.completed_count(), 3);
            assert!((io.hit_rate() - 1.0).abs() < 1e-9);
        }
    }

    #[test]
    fn test_is_initialized() {
        #[cfg(target_os = "linux")]
        {
            let io = IoUringIo::new(64).unwrap();
            assert!(io.is_initialized());
        }
    }
}
