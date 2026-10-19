//! SmallVec：栈上 Vec，短 Vec 减少堆分配。
//!
//! 用标准库 `MaybeUninit` + `Vec<T>` 实现，不引入新外部依赖。
//! `len <= N` 时数据在栈上，`len > N` 时溢出到堆（溢出后不再迁回栈）。

use std::mem::MaybeUninit;

/// 栈上 Vec，容量 N 在栈上，溢出到堆。
pub struct SmallVec<T, const N: usize> {
    stack: MaybeUninit<[T; N]>,
    heap: Option<Vec<T>>,
    len: usize,
}

impl<T, const N: usize> SmallVec<T, N> {
    /// 创建空 SmallVec。
    pub fn new() -> Self {
        Self {
            stack: MaybeUninit::uninit(),
            heap: None,
            len: 0,
        }
    }

    fn stack_ptr(&self) -> *const T {
        self.stack.as_ptr() as *const T
    }

    fn stack_mut_ptr(&mut self) -> *mut T {
        self.stack.as_mut_ptr() as *mut T
    }

    /// 追加元素。`len < N` 时栈上分配，`len == N` 时迁移至堆。
    pub fn push(&mut self, value: T) {
        if let Some(vec) = &mut self.heap {
            vec.push(value);
            self.len += 1;
        } else if self.len < N {
            // SAFETY: self.len < N，写入栈上第 len 个槽位。
            unsafe {
                self.stack_mut_ptr().add(self.len).write(value);
            }
            self.len += 1;
        } else {
            let mut vec = Vec::with_capacity(N * 2);
            // SAFETY: 读取栈上已初始化的 N 个元素并转移所有权。
            unsafe {
                let ptr = self.stack_ptr();
                for i in 0..N {
                    vec.push(ptr.add(i).read());
                }
            }
            vec.push(value);
            self.heap = Some(vec);
            self.len = N + 1;
        }
    }

    /// 弹出末尾元素。
    pub fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }

        if let Some(vec) = &mut self.heap {
            self.len -= 1;
            return vec.pop();
        }

        self.len -= 1;
        // SAFETY: 读取栈上第 len 个已初始化元素（len 已减 1）。
        unsafe { Some(self.stack_ptr().add(self.len).read()) }
    }

    /// 元素数量。
    pub fn len(&self) -> usize {
        self.len
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// 是否在堆上。
    pub fn is_on_heap(&self) -> bool {
        self.heap.is_some()
    }

    /// 返回切片。
    pub fn as_slice(&self) -> &[T] {
        if let Some(vec) = &self.heap {
            &vec[..self.len]
        } else {
            // SAFETY: 前 self.len 个栈上元素已初始化。
            unsafe { std::slice::from_raw_parts(self.stack_ptr(), self.len) }
        }
    }

    /// 返回可变切片。
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        if let Some(vec) = self.heap.as_mut() {
            &mut vec[..self.len]
        } else {
            // SAFETY: 前 self.len 个栈上元素已初始化。
            unsafe { std::slice::from_raw_parts_mut(self.stack.as_mut_ptr() as *mut T, self.len) }
        }
    }
}

impl<T, const N: usize> Default for SmallVec<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, const N: usize> Drop for SmallVec<T, N> {
    fn drop(&mut self) {
        if self.heap.is_some() {
            return;
        }
        // SAFETY: 前 self.len 个栈上元素已初始化，逐个 drop。
        unsafe {
            let ptr = self.stack_mut_ptr();
            for i in 0..self.len {
                ptr.add(i).drop_in_place();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_smallvec_stack_alloc() {
        let mut sv: SmallVec<i32, 3> = SmallVec::new();
        sv.push(1);
        sv.push(2);
        sv.push(3);
        assert_eq!(sv.len(), 3);
        assert!(!sv.is_on_heap());
        assert_eq!(sv.as_slice(), &[1, 2, 3]);
    }

    #[test]
    fn test_smallvec_overflow_to_heap() {
        let mut sv: SmallVec<i32, 3> = SmallVec::new();
        for i in 1..=10 {
            sv.push(i);
        }
        assert_eq!(sv.len(), 10);
        assert!(sv.is_on_heap());
        assert_eq!(sv.as_slice(), &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
    }

    #[test]
    fn test_smallvec_pop_and_as_slice() {
        let mut sv: SmallVec<i32, 4> = SmallVec::new();
        for i in 1..=5 {
            sv.push(i);
        }
        assert_eq!(sv.pop(), Some(5));
        assert_eq!(sv.pop(), Some(4));
        assert_eq!(sv.len(), 3);
        assert_eq!(sv.as_slice(), &[1, 2, 3]);
    }

    #[test]
    fn test_smallvec_eq_vec_behavior() {
        let mut sv: SmallVec<i32, 3> = SmallVec::new();
        let mut vec: Vec<i32> = Vec::new();

        for i in 1..=7 {
            sv.push(i);
            vec.push(i);
        }
        assert_eq!(sv.as_slice(), vec.as_slice());

        sv.pop();
        vec.pop();
        assert_eq!(sv.as_slice(), vec.as_slice());
    }

    #[test]
    fn test_smallvec_empty() {
        let sv: SmallVec<i32, 4> = SmallVec::new();
        assert!(sv.is_empty());
        assert_eq!(sv.len(), 0);
        assert_eq!(sv.as_slice().len(), 0);
    }

    #[test]
    fn test_smallvec_pop_empty() {
        let mut sv: SmallVec<i32, 4> = SmallVec::new();
        assert_eq!(sv.pop(), None);
    }

    #[test]
    fn test_smallvec_mut_slice() {
        let mut sv: SmallVec<i32, 4> = SmallVec::new();
        sv.push(1);
        sv.push(2);
        sv.as_mut_slice()[0] = 10;
        assert_eq!(sv.as_slice(), &[10, 2]);
    }

    #[test]
    fn test_smallvec_string_drop() {
        let mut sv: SmallVec<String, 2> = SmallVec::new();
        sv.push("hello".to_string());
        sv.push("world".to_string());
        sv.push("overflow".to_string());
        assert_eq!(sv.len(), 3);
        drop(sv);
    }

    #[test]
    fn test_smallvec_pop_from_stack() {
        let mut sv: SmallVec<i32, 4> = SmallVec::new();
        sv.push(1);
        sv.push(2);
        assert_eq!(sv.pop(), Some(2));
        assert_eq!(sv.pop(), Some(1));
        assert!(sv.is_empty());
    }
}
