#![cfg(feature = "io-uring")]

//! v7.6.0 任务 1.9：IO_uring 异步 IO 端到端测试
//!
//! 验证 Linux 平台 IO_uring 异步 IO + 非 Linux 平台回退验证

#[cfg(test)]
mod tests {
    use sz_orm_core::io_uring_io::{
        fallback_reason, is_io_uring_available, platform_name, IoUringIo,
    };

    #[test]
    fn test_io_uring_availability_consistent_with_platform() {
        let available = is_io_uring_available();
        let platform = platform_name();
        if platform == "linux" {
            assert!(available, "Linux 平台应支持 IO_uring");
        } else {
            assert!(!available, "非 Linux 平台不应支持 IO_uring");
        }
    }

    #[test]
    fn test_io_uring_new_returns_none_on_non_linux() {
        #[cfg(not(target_os = "linux"))]
        {
            assert!(IoUringIo::new(256).is_none(), "非 Linux 平台应返回 None");
        }
    }

    #[test]
    fn test_io_uring_new_returns_some_on_linux() {
        #[cfg(target_os = "linux")]
        {
            let io = IoUringIo::new(256);
            assert!(io.is_some(), "Linux 平台应返回 Some");
            assert_eq!(io.unwrap().entries(), 256);
        }
    }

    #[test]
    fn test_fallback_reason_consistent() {
        let reason = fallback_reason();
        if is_io_uring_available() {
            assert_eq!(reason, "no_fallback");
        } else {
            assert_eq!(reason, "non_linux_platform");
        }
    }

    #[test]
    fn test_io_uring_zero_entries_returns_none() {
        assert!(IoUringIo::new(0).is_none());
    }

    #[test]
    fn test_platform_name_not_empty() {
        let name = platform_name();
        assert!(!name.is_empty());
        assert!(["linux", "windows", "macos", "unknown"].contains(&name));
    }
}
