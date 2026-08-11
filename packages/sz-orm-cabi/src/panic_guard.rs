//! FFI panic 捕获
//!
//! 捕获 Rust 侧 panic 转换为错误码，防止 panic 跨 FFI 边界导致 UB。

use std::panic::UnwindSafe;

/// panic 捕获结果
pub enum FfiResult<T> {
    Ok(T),
    Panic(String),
}

/// 捕获可能 panic 的闭包，返回 `FfiResult`
///
/// SAFETY: 使用 `catch_unwind` 捕获 panic，防止 panic 跨 FFI 边界。
pub fn catch_panic<F, T>(f: F) -> FfiResult<T>
where
    F: FnOnce() -> T + UnwindSafe,
{
    match std::panic::catch_unwind(f) {
        Ok(result) => FfiResult::Ok(result),
        Err(panic_payload) => {
            let msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                s.clone()
            } else {
                "unknown panic".to_string()
            };
            FfiResult::Panic(msg)
        }
    }
}

/// 捕获可能 panic 的闭包，panic 时返回默认值
pub fn catch_panic_or_default<F, T>(f: F, default: T) -> T
where
    F: FnOnce() -> T + UnwindSafe,
{
    match catch_panic(f) {
        FfiResult::Ok(result) => result,
        FfiResult::Panic(_) => default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_catch_panic_ok() {
        let result = catch_panic(|| 42);
        match result {
            FfiResult::Ok(v) => assert_eq!(v, 42),
            FfiResult::Panic(_) => panic!("expected Ok"),
        }
    }

    #[test]
    fn test_catch_panic_str() {
        let result = catch_panic(|| panic!("test panic"));
        match result {
            FfiResult::Ok(_) => panic!("expected Panic"),
            FfiResult::Panic(msg) => assert_eq!(msg, "test panic"),
        }
    }

    #[test]
    fn test_catch_panic_string() {
        let result = catch_panic(|| panic!("{}", "formatted panic"));
        match result {
            FfiResult::Ok(_) => panic!("expected Panic"),
            FfiResult::Panic(msg) => assert!(msg.contains("formatted panic")),
        }
    }

    #[test]
    fn test_catch_panic_or_default_ok() {
        let result = catch_panic_or_default(|| 100, 0);
        assert_eq!(result, 100);
    }

    #[test]
    fn test_catch_panic_or_default_panic() {
        let result = catch_panic_or_default(|| panic!("oops"), 0);
        assert_eq!(result, 0);
    }
}
