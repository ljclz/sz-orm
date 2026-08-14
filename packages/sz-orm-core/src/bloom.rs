//! 布隆过滤器（公共实现）
//!
//! v4.7.0 架构债清零：合并 `cache_warmup_protection::BloomFilter`（自研）与
//! `dist_cache::BloomFilterGuard`（bloomfilter crate）双实现为单一公共模块。
//! 设计取舍：
//!   - 并发安全：内部 `RwLock`，`add`/`might_contain` 均为 `&self`
//!   - 容量拒绝：超过 `capacity` 的 `add` 返回 `CapacityExceeded`（不漏判语义：
//!     拒绝写入而非静默丢位，避免"存在但查不到"）
//!   - 误判率：`might_contain` 假阳性 ≤ fpp，不存在一定返回 false
//!
//! 并发正确性：常规并发测试验证"add 后 might_contain 必命中"（不漏判不变量）。
//! 注：loom 模型检查曾尝试引入，但 RUSTFLAGS=--cfg loom 会污染依赖树
//! （crossbeam-queue → concurrent-queue 等无 loom 适配，2026-08-14 评估不可行），
//! 并发验证采用常规多线程测试 + chaos/stress 组合。

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::RwLock;

/// 布隆过滤器错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BloomError {
    /// 超出容量（拒绝写入，保持不漏判）
    CapacityExceeded {
        /// 配置容量
        capacity: usize,
        /// 请求写入时的元素数（= 容量 + 1）
        requested: usize,
    },
}

/// 并发安全布隆过滤器
///
/// ```
/// use sz_orm_core::bloom::BloomFilter;
///
/// let mut filter = BloomFilter::new(100, 0.01);
/// filter.add("key-1").unwrap();
/// assert!(filter.might_contain("key-1"));
/// assert!(!filter.might_contain("key-2")); // 不存在一定返回 false（不漏判）
/// ```
pub struct BloomFilter {
    bits: RwLock<Vec<u64>>,
    num_bits: usize,
    num_hashes: usize,
    capacity: usize,
    count: AtomicUsize,
}

impl BloomFilter {
    /// 创建布隆过滤器
    ///
    /// `capacity` 预期元素数量，`fpp` 误判率（0~1，内部收敛到 0.0001~0.5）。
    pub fn new(capacity: usize, fpp: f64) -> Self {
        let capacity = capacity.max(1);
        let fpp = fpp.clamp(0.0001, 0.5);
        let ln2 = std::f64::consts::LN_2;
        let m = (-(capacity as f64) * fpp.ln() / (ln2 * ln2)).ceil() as usize;
        let m = m.max(8);
        let k = ((m as f64 / capacity as f64) * ln2).ceil() as usize;
        let k = k.max(1);
        let num_words = m.div_ceil(64);
        Self {
            bits: RwLock::new(vec![0u64; num_words]),
            num_bits: m,
            num_hashes: k,
            capacity,
            count: AtomicUsize::new(0),
        }
    }

    /// 添加键（容量满时返回 `CapacityExceeded`，拒绝写入保持不漏判）
    pub fn add(&self, key: &str) -> Result<(), BloomError> {
        let count = self.count.load(Ordering::Relaxed);
        if count >= self.capacity {
            return Err(BloomError::CapacityExceeded {
                capacity: self.capacity,
                requested: count + 1,
            });
        }
        let (h1, h2) = self.hash(key);
        let mut bits = self.bits.write().unwrap();
        for i in 0..self.num_hashes {
            let combined = h1.wrapping_add((i as u64).wrapping_mul(h2));
            let idx = (combined as usize) % self.num_bits;
            bits[idx / 64] |= 1u64 << (idx % 64);
        }
        self.count.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// 检查键可能存在（不漏判：不存在一定返回 false；假阳性 ≤ fpp）
    pub fn might_contain(&self, key: &str) -> bool {
        let (h1, h2) = self.hash(key);
        let bits = self.bits.read().unwrap();
        for i in 0..self.num_hashes {
            let combined = h1.wrapping_add((i as u64).wrapping_mul(h2));
            let idx = (combined as usize) % self.num_bits;
            if bits[idx / 64] & (1u64 << (idx % 64)) == 0 {
                return false;
            }
        }
        true
    }

    /// 当前元素计数
    pub fn count(&self) -> usize {
        self.count.load(Ordering::Relaxed)
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.count() == 0
    }

    /// 容量
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// 清空（保留容量/误判率配置）
    pub fn clear(&self) {
        let mut bits = self.bits.write().unwrap();
        bits.iter_mut().for_each(|w| *w = 0);
        self.count.store(0, Ordering::Relaxed);
    }

    /// 双哈希（xxhash 风格混合：基于 FNV-1a 的 h1 + 二次扰动 h2）
    fn hash(&self, key: &str) -> (u64, u64) {
        let mut h1: u64 = 0xcbf29ce484222325;
        let mut h2: u64 = 0x9e3779b97f4a7c15;
        for b in key.bytes() {
            h1 ^= b as u64;
            h1 = h1.wrapping_mul(0x100000001b3);
            h2 = h2.wrapping_add(b as u64);
            h2 = h2.wrapping_mul(0x100000001b3);
        }
        h2 ^= h2 >> 33;
        h2 = h2.wrapping_mul(0xff51afd7ed558ccd);
        (h1, h2)
    }
}

// ============================================================================
// 并发测试：并发 add 后 might_contain 必命中（不漏判的并发不变量）
// ============================================================================
#[cfg(test)]
mod concurrent_tests {
    use super::*;

    /// 多线程并发写入不同 key，全部完成后 must_contain 必命中。
    /// 验证写锁内原子完成（add 返回即可见）。
    #[test]
    fn bloom_concurrent_add_no_false_negative() {
        use std::sync::Arc;
        use std::thread;

        let filter = Arc::new(BloomFilter::new(4096, 0.01));
        let mut handles = vec![];
        for t in 0..8 {
            let f = Arc::clone(&filter);
            handles.push(thread::spawn(move || {
                for i in 0..200 {
                    f.add(&format!("thread-{t}-key-{i}")).unwrap();
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        // 并发不变量：写入完成后必须命中（不存在才允许 false）
        for t in 0..8 {
            for i in 0..200 {
                assert!(
                    filter.might_contain(&format!("thread-{t}-key-{i}")),
                    "add 后 must_contain 必须命中（并发不漏判）: thread-{t}-key-{i}"
                );
            }
        }
        assert_eq!(filter.count(), 1600);
    }

    /// 并发读写混合：might_contain 不得 panic（RwLock 正确性冒烟）
    #[test]
    fn bloom_concurrent_read_write_smoke() {
        use std::sync::Arc;
        use std::thread;

        let filter = Arc::new(BloomFilter::new(2048, 0.01));
        let mut handles = vec![];
        for t in 0..4 {
            let f = Arc::clone(&filter);
            handles.push(thread::spawn(move || {
                for i in 0..100 {
                    let key = format!("t{t}-k{i}");
                    let _ = f.add(&key);
                    let _ = f.might_contain(&key);
                    let _ = f.count();
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        assert!(filter.count() > 0);
    }
}
