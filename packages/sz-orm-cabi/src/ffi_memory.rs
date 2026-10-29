//! FFI 内存管理器（注册表模式）
//!
//! Rust 侧分配/释放内存，语言侧仅持有句柄。管理器**不参与实际分配**，
//! 只跟踪由 `Box::into_raw` / `CString::into_raw` 产生的活跃指针，
//! 提供：
//! - double-free 防护：`untrack` 返回 `false` 表示指针未知/已释放，
//!   调用方应避免再次释放（把未定义行为变成安全静默返回）；
//! - 泄漏计数：`allocation_count()` / `total_allocated()`。

use parking_lot::Mutex;
use std::collections::HashMap;
use std::ffi::c_void;

/// FFI 内存管理器（注册表模式）
pub struct FfiMemoryManager {
    allocations: Mutex<HashMap<usize, usize>>,
}

impl FfiMemoryManager {
    pub fn new() -> Self {
        Self {
            allocations: Mutex::new(HashMap::new()),
        }
    }

    /// 登记一个由 Rust 侧分配的指针（`Box::into_raw` / `CString::into_raw`）。
    ///
    /// `size` 用于泄漏统计；`ptr` 为 null 时静默忽略。
    pub fn track(&self, ptr: *mut c_void, size: usize) {
        if ptr.is_null() {
            return;
        }
        self.allocations.lock().insert(ptr as usize, size);
    }

    /// 注销指针并返回它是否曾被登记。
    ///
    /// 返回 `false` 表示指针未知、已释放或为 null——调用方应**避免**再次释放
    /// （double-free 防护）。
    pub fn untrack(&self, ptr: *mut c_void) -> bool {
        if ptr.is_null() {
            return false;
        }
        self.allocations.lock().remove(&(ptr as usize)).is_some()
    }

    /// 检查指针是否由本管理器登记
    pub fn is_tracked(&self, ptr: *mut c_void) -> bool {
        if ptr.is_null() {
            return false;
        }
        self.allocations.lock().contains_key(&(ptr as usize))
    }

    /// 返回当前活跃登记数量
    pub fn allocation_count(&self) -> usize {
        self.allocations.lock().len()
    }

    /// 返回当前活跃登记总字节数
    pub fn total_allocated(&self) -> usize {
        self.allocations.lock().values().sum()
    }
}

impl Default for FfiMemoryManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_track_untrack() {
        let manager = FfiMemoryManager::new();
        let ptr = Box::into_raw(Box::new(42u64)) as *mut c_void;
        manager.track(ptr, std::mem::size_of::<u64>());
        assert!(manager.is_tracked(ptr));
        assert_eq!(manager.allocation_count(), 1);
        assert_eq!(manager.total_allocated(), std::mem::size_of::<u64>());
        assert!(manager.untrack(ptr), "first untrack should succeed");
        assert!(!manager.is_tracked(ptr));
        assert_eq!(manager.allocation_count(), 0);
        // SAFETY: ptr 来自 Box::into_raw，配对释放
        unsafe {
            drop(Box::from_raw(ptr as *mut u64));
        }
    }

    #[test]
    fn test_untrack_unknown_returns_false() {
        let manager = FfiMemoryManager::new();
        let ptr = 0x1234 as *mut c_void;
        assert!(!manager.untrack(ptr), "unknown pointer should return false");
    }

    #[test]
    fn test_untrack_null_is_false() {
        let manager = FfiMemoryManager::new();
        assert!(!manager.untrack(std::ptr::null_mut()));
    }

    #[test]
    fn test_track_null_is_noop() {
        let manager = FfiMemoryManager::new();
        manager.track(std::ptr::null_mut(), 128);
        assert_eq!(manager.allocation_count(), 0);
    }

    #[test]
    fn test_multiple_track_untrack() {
        let manager = FfiMemoryManager::new();
        let ptr1 = Box::into_raw(Box::new(1u64)) as *mut c_void;
        let ptr2 = Box::into_raw(Box::new(2u64)) as *mut c_void;
        let ptr3 = Box::into_raw(Box::new(3u64)) as *mut c_void;
        manager.track(ptr1, 8);
        manager.track(ptr2, 8);
        manager.track(ptr3, 8);
        assert_eq!(manager.allocation_count(), 3);
        assert!(manager.untrack(ptr2));
        assert_eq!(manager.allocation_count(), 2);
        assert!(manager.untrack(ptr1));
        assert!(manager.untrack(ptr3));
        assert_eq!(manager.allocation_count(), 0);
        // SAFETY: 指针均来自 Box::into_raw，配对释放
        unsafe {
            drop(Box::from_raw(ptr1 as *mut u64));
            drop(Box::from_raw(ptr2 as *mut u64));
            drop(Box::from_raw(ptr3 as *mut u64));
        }
    }

    #[test]
    fn test_double_untrack_safe() {
        let manager = FfiMemoryManager::new();
        let ptr = Box::into_raw(Box::new(7u64)) as *mut c_void;
        manager.track(ptr, 8);
        assert!(manager.untrack(ptr));
        assert!(!manager.untrack(ptr), "second untrack should return false");
        // SAFETY: 配对释放
        unsafe {
            drop(Box::from_raw(ptr as *mut u64));
        }
    }
}
