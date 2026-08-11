//! FFI 内存管理器
//!
//! Rust 侧分配/释放内存，语言侧仅持有句柄，确保不泄漏不悬空。

use parking_lot::Mutex;
use std::collections::HashMap;
use std::ffi::c_void;

/// FFI 内存管理器
pub struct FfiMemoryManager {
    allocations: Mutex<HashMap<usize, usize>>,
}

impl FfiMemoryManager {
    pub fn new() -> Self {
        Self {
            allocations: Mutex::new(HashMap::new()),
        }
    }

    /// 分配指定大小的内存，返回指针
    ///
    /// # Safety
    ///
    /// �# SAFETY: 使用 `std::alloc::alloc` 分配内存，返回的指针必须通过 `free` 释放。
    pub fn alloc(&self, size: usize) -> *mut c_void {
        if size == 0 {
            return std::ptr::null_mut();
        }
        let layout = std::alloc::Layout::from_size_align(size, 8).unwrap();
        // SAFETY: layout 的 size > 0，符合 alloc 的安全要求
        let ptr = unsafe { std::alloc::alloc(layout) } as *mut c_void;
        if !ptr.is_null() {
            self.allocations.lock().insert(ptr as usize, size);
        }
        ptr
    }

    /// 释放 Rust 侧分配的内存
    ///
    /// # Safety
    ///
    /// SAFETY: ptr 必须是 `alloc` 返回的指针，且尚未被释放。
    pub fn free(&self, ptr: *mut c_void) {
        if ptr.is_null() {
            return;
        }
        let mut allocations = self.allocations.lock();
        if let Some(size) = allocations.remove(&(ptr as usize)) {
            let layout = std::alloc::Layout::from_size_align(size, 8).unwrap();
            // SAFETY: ptr 来自 alloc，layout 与分配时一致
            unsafe { std::alloc::dealloc(ptr as *mut u8, layout) };
        }
    }

    /// 检查指针是否由本管理器分配
    pub fn is_tracked(&self, ptr: *mut c_void) -> bool {
        if ptr.is_null() {
            return false;
        }
        self.allocations.lock().contains_key(&(ptr as usize))
    }

    /// 返回当前活跃分配数量
    pub fn allocation_count(&self) -> usize {
        self.allocations.lock().len()
    }

    /// 返回当前活跃分配总字节数
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
    fn test_alloc_free() {
        let manager = FfiMemoryManager::new();
        let ptr = manager.alloc(128);
        assert!(!ptr.is_null());
        assert!(manager.is_tracked(ptr));
        assert_eq!(manager.allocation_count(), 1);
        assert_eq!(manager.total_allocated(), 128);
        manager.free(ptr);
        assert!(!manager.is_tracked(ptr));
        assert_eq!(manager.allocation_count(), 0);
    }

    #[test]
    fn test_alloc_zero_returns_null() {
        let manager = FfiMemoryManager::new();
        let ptr = manager.alloc(0);
        assert!(ptr.is_null());
    }

    #[test]
    fn test_free_null_is_noop() {
        let manager = FfiMemoryManager::new();
        manager.free(std::ptr::null_mut());
        assert_eq!(manager.allocation_count(), 0);
    }

    #[test]
    fn test_multiple_allocations() {
        let manager = FfiMemoryManager::new();
        let ptr1 = manager.alloc(64);
        let ptr2 = manager.alloc(128);
        let ptr3 = manager.alloc(256);
        assert_eq!(manager.allocation_count(), 3);
        assert_eq!(manager.total_allocated(), 448);
        manager.free(ptr2);
        assert_eq!(manager.allocation_count(), 2);
        assert_eq!(manager.total_allocated(), 320);
        manager.free(ptr1);
        manager.free(ptr3);
        assert_eq!(manager.allocation_count(), 0);
    }

    #[test]
    fn test_double_free_safe() {
        let manager = FfiMemoryManager::new();
        let ptr = manager.alloc(64);
        manager.free(ptr);
        manager.free(ptr);
        assert_eq!(manager.allocation_count(), 0);
    }
}
