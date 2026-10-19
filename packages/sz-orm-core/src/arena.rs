//! Arena 分配器：bump allocation，批量行处理一次性释放。
//!
//! 用标准库 `Vec<u8>` 实现，不引入新外部依赖。
//! 适用于短生命周期、高频分配场景（如行解码缓冲区）。

use std::alloc::Layout;

/// Bump arena 分配器。
///
/// 预分配连续内存块，通过 bump 指针前进分配。
/// `reset()` 重置指针可复用，`Drop` 一次性释放。
pub struct ArenaAllocator {
    buffer: Vec<u8>,
    offset: usize,
}

impl ArenaAllocator {
    /// 创建指定容量的 arena，预分配 `capacity` 字节。
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: vec![0u8; capacity],
            offset: 0,
        }
    }

    /// 分配 `layout.size()` 字节，对齐 `layout.align()`。
    ///
    /// 返回借用切片，bump 指针前进。容量不足时自动扩容。
    pub fn alloc(&mut self, layout: Layout) -> &mut [u8] {
        let align = layout.align().max(1);
        let size = layout.size();

        let aligned_offset = (self.offset + align - 1) & !(align - 1);
        let end = aligned_offset + size;

        if end > self.buffer.len() {
            let new_cap = end.max(self.buffer.capacity() * 2).max(end + 4096);
            self.buffer.resize(new_cap, 0);
        }

        self.offset = end;
        &mut self.buffer[aligned_offset..end]
    }

    /// 重置 bump 指针为 0，不释放内存，可复用。
    pub fn reset(&mut self) {
        self.offset = 0;
    }

    /// 已使用字节数。
    pub fn used(&self) -> usize {
        self.offset
    }

    /// 剩余字节数（基于已分配 buffer 长度，非 capacity）。
    pub fn remaining(&self) -> usize {
        self.buffer.len().saturating_sub(self.offset)
    }
}

impl Default for ArenaAllocator {
    fn default() -> Self {
        Self::new(4096)
    }
}

/// 类型化 arena（v10.10.0）
///
/// 预分配连续内存，批量分配 T 类型对象，减少堆分配。
/// 适用于短生命周期的批量数据处理（如行解码后的 Value 向量）。
/// `reset()` 清空但保留容量，可复用；`Drop` 一次性释放。
pub struct TypedArena<T> {
    storage: Vec<T>,
    allocation_count: u64,
}

impl<T> TypedArena<T> {
    /// 创建指定容量的类型化 arena。
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            storage: Vec::with_capacity(capacity),
            allocation_count: 0,
        }
    }

    /// 分配一个 T 对象，返回引用。容量不足时自动扩容（触发堆分配）。
    pub fn alloc(&mut self, value: T) -> &T {
        if self.storage.len() == self.storage.capacity() {
            self.allocation_count += 1;
        }
        self.storage.push(value);
        &self.storage[self.storage.len() - 1]
    }

    /// 批量分配，返回切片引用。
    pub fn alloc_extend(&mut self, values: impl IntoIterator<Item = T>) -> &[T] {
        let start = self.storage.len();
        for v in values {
            self.alloc(v);
        }
        &self.storage[start..]
    }

    /// 清空但保留容量，可复用。不调用 T 的 drop（需 T: Default 时用 `clear_with`）。
    pub fn reset(&mut self) {
        self.storage.clear();
    }

    /// 已分配元素数。
    pub fn len(&self) -> usize {
        self.storage.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.storage.is_empty()
    }

    /// 预分配容量。
    pub fn capacity(&self) -> usize {
        self.storage.capacity()
    }

    /// 堆分配触发次数（容量不足时的扩容次数）。
    pub fn allocation_count(&self) -> u64 {
        self.allocation_count
    }
}

impl<T> Default for TypedArena<T> {
    fn default() -> Self {
        Self::with_capacity(64)
    }
}

/// 行解码 arena（v10.10.0）
///
/// 行解码时复用内存，减少 Vec 分配。
/// 每行解码时将数据复制到预分配缓冲区，`reset()` 后可复用于下一行。
pub struct RowArena {
    buffer: Vec<u8>,
    offset: usize,
    rows_decoded: u64,
    allocation_count: u64,
}

impl RowArena {
    /// 创建指定容量的行解码 arena。
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            buffer: vec![0u8; capacity],
            offset: 0,
            rows_decoded: 0,
            allocation_count: 0,
        }
    }

    /// 解码一行：将 `data` 复制到 arena 中，返回借用切片。
    /// 容量不足时自动扩容（触发堆分配，allocation_count +1）。
    pub fn decode_row(&mut self, data: &[u8]) -> &[u8] {
        let start = self.offset;
        let end = start + data.len();

        if end > self.buffer.len() {
            let new_cap = end.max(self.buffer.len() * 2).max(end + 4096);
            self.buffer.resize(new_cap, 0);
            self.allocation_count += 1;
        }

        self.buffer[start..end].copy_from_slice(data);
        self.offset = end;
        self.rows_decoded += 1;
        &self.buffer[start..end]
    }

    /// 重置偏移，可复用于下一批行。不释放内存。
    pub fn reset(&mut self) {
        self.offset = 0;
    }

    /// 已使用字节数。
    pub fn used(&self) -> usize {
        self.offset
    }

    /// 已解码行数。
    pub fn rows_decoded(&self) -> u64 {
        self.rows_decoded
    }

    /// 堆分配触发次数（容量不足时的扩容次数）。
    pub fn allocation_count(&self) -> u64 {
        self.allocation_count
    }

    /// 预分配容量。
    pub fn capacity(&self) -> usize {
        self.buffer.len()
    }
}

impl Default for RowArena {
    fn default() -> Self {
        Self::with_capacity(4096)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arena_alloc_and_reset() {
        let mut arena = ArenaAllocator::new(1024);
        let layout = Layout::from_size_align(16, 8).unwrap();

        let a = arena.alloc(layout);
        a.fill(1);
        assert!(arena.used() >= 16);

        let b = arena.alloc(layout);
        b.fill(2);
        assert!(arena.used() >= 32);

        arena.reset();
        assert_eq!(arena.used(), 0);

        let c = arena.alloc(layout);
        c.fill(3);
        assert!(arena.used() >= 16);
    }

    #[test]
    fn test_arena_drop_releases_memory() {
        let mut arena = ArenaAllocator::new(1024);
        let layout = Layout::from_size_align(100, 1).unwrap();
        let _ = arena.alloc(layout);
        assert!(arena.used() >= 100);
        drop(arena);
    }

    #[test]
    fn test_arena_alignment() {
        let mut arena = ArenaAllocator::new(256);

        for align in [1usize, 2, 4, 8, 16] {
            let layout = Layout::from_size_align(1, align).unwrap();
            let ptr = arena.alloc(layout).as_ptr();
            assert_eq!(ptr as usize % align, 0, "align {} failed", align);
        }
    }

    #[test]
    fn test_arena_growth() {
        let mut arena = ArenaAllocator::new(64);
        let layout = Layout::from_size_align(128, 1).unwrap();
        let s = arena.alloc(layout);
        assert_eq!(s.len(), 128);
        assert!(arena.buffer.len() >= 128);
    }

    #[test]
    fn test_arena_remaining() {
        let mut arena = ArenaAllocator::new(512);
        let initial = arena.remaining();
        let layout = Layout::from_size_align(100, 1).unwrap();
        let _ = arena.alloc(layout);
        assert!(arena.remaining() < initial);
    }

    // v10.10.0 TypedArena 测试

    #[test]
    fn test_typed_arena_alloc_within_capacity() {
        let mut arena: TypedArena<i32> = TypedArena::with_capacity(8);
        for i in 0..8 {
            let val = arena.alloc(i);
            assert_eq!(*val, i);
        }
        assert_eq!(arena.len(), 8);
        assert_eq!(arena.allocation_count(), 0, "容量内分配不应触发堆分配");
    }

    #[test]
    fn test_typed_arena_alloc_exceeds_capacity() {
        let mut arena: TypedArena<i32> = TypedArena::with_capacity(4);
        for i in 0..10 {
            arena.alloc(i);
        }
        assert_eq!(arena.len(), 10);
        assert!(arena.allocation_count() > 0, "超出容量应触发堆分配");
    }

    #[test]
    fn test_typed_arena_reset_reuses_capacity() {
        let mut arena: TypedArena<i32> = TypedArena::with_capacity(4);
        for i in 0..4 {
            arena.alloc(i);
        }
        arena.reset();
        assert_eq!(arena.len(), 0);
        assert_eq!(arena.capacity(), 4, "reset 后容量应保留");

        for i in 0..4 {
            arena.alloc(i);
        }
        assert_eq!(arena.allocation_count(), 0, "reset 后复用不应触发堆分配");
    }

    #[test]
    fn test_typed_arena_alloc_extend() {
        let mut arena: TypedArena<i32> = TypedArena::with_capacity(8);
        let slice = arena.alloc_extend(vec![1, 2, 3, 4]);
        assert_eq!(slice, &[1, 2, 3, 4]);
        assert_eq!(arena.len(), 4);
    }

    // v10.10.0 RowArena 测试

    #[test]
    fn test_row_arena_decode_row_within_capacity() {
        let mut arena = RowArena::with_capacity(1024);
        let row1 = arena.decode_row(b"hello").to_vec();
        let row2 = arena.decode_row(b"world").to_vec();
        assert_eq!(row1, b"hello");
        assert_eq!(row2, b"world");
        assert_eq!(arena.rows_decoded(), 2);
        assert_eq!(arena.allocation_count(), 0, "容量内不应触发堆分配");
    }

    #[test]
    fn test_row_arena_decode_row_exceeds_capacity() {
        let mut arena = RowArena::with_capacity(16);
        arena.decode_row(b"short");
        arena.decode_row(b"this is a longer row that exceeds initial capacity");
        assert!(arena.allocation_count() > 0, "超出容量应触发堆分配");
        assert_eq!(arena.rows_decoded(), 2);
    }

    #[test]
    fn test_row_arena_reset_reuses_memory() {
        let mut arena = RowArena::with_capacity(256);
        arena.decode_row(b"first row data");
        arena.reset();
        assert_eq!(arena.used(), 0, "reset 后 used 应为 0");

        arena.decode_row(b"second row");
        assert_eq!(arena.allocation_count(), 0, "reset 后复用不应触发堆分配");
    }

    #[test]
    fn test_row_arena_multiple_rows() {
        let mut arena = RowArena::with_capacity(512);
        for i in 0..10 {
            let data = format!("row_{}", i);
            let row = arena.decode_row(data.as_bytes()).to_vec();
            assert_eq!(row, data.as_bytes());
        }
        assert_eq!(arena.rows_decoded(), 10);
        assert_eq!(arena.allocation_count(), 0);
    }
}
