//! 幂等去重器

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use super::types::IdempotencyEntry;

/// 幂等去重器（60s 滑动窗口内同事件去重）
#[derive(Debug, Clone)]
pub struct IdempotencyDeduplicator {
    window: Duration,
    entries: VecDeque<IdempotencyEntry>,
    max_capacity: usize,
}

impl IdempotencyDeduplicator {
    pub fn new(window: Duration, max_capacity: usize) -> Self {
        Self {
            window,
            entries: VecDeque::new(),
            max_capacity,
        }
    }

    /// 检查事件是否重复（true=重复，应跳过；false=首次，已记录）
    pub fn is_duplicate(&mut self, event_hash: u64) -> bool {
        self.evict_expired();
        if self.entries.iter().any(|e| e.event_hash == event_hash) {
            return true;
        }
        if self.entries.len() >= self.max_capacity {
            self.entries.pop_front();
        }
        self.entries.push_back(IdempotencyEntry {
            event_hash,
            timestamp: Instant::now(),
        });
        false
    }

    /// 清除过期条目
    fn evict_expired(&mut self) {
        let now = Instant::now();
        while let Some(front) = self.entries.front() {
            if now.duration_since(front.timestamp) > self.window {
                self.entries.pop_front();
            } else {
                break;
            }
        }
    }

    /// 当前条目数
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for IdempotencyDeduplicator {
    fn default() -> Self {
        Self::new(Duration::from_secs(60), 10000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_first_occurrence_not_duplicate() {
        let mut dedup = IdempotencyDeduplicator::default();
        assert!(!dedup.is_duplicate(123));
    }

    #[test]
    fn test_second_occurrence_is_duplicate() {
        let mut dedup = IdempotencyDeduplicator::default();
        dedup.is_duplicate(123);
        assert!(dedup.is_duplicate(123));
    }

    #[test]
    fn test_different_events_not_duplicate() {
        let mut dedup = IdempotencyDeduplicator::default();
        dedup.is_duplicate(123);
        assert!(!dedup.is_duplicate(456));
    }

    #[test]
    fn test_expired_entry_not_duplicate() {
        let mut dedup = IdempotencyDeduplicator::new(Duration::from_millis(10), 100);
        dedup.is_duplicate(123);
        std::thread::sleep(Duration::from_millis(20));
        assert!(!dedup.is_duplicate(123));
    }

    #[test]
    fn test_capacity_eviction() {
        let mut dedup = IdempotencyDeduplicator::new(Duration::from_secs(60), 3);
        dedup.is_duplicate(1);
        dedup.is_duplicate(2);
        dedup.is_duplicate(3);
        dedup.is_duplicate(4);
        assert_eq!(dedup.len(), 3);
        assert!(!dedup.is_duplicate(1));
    }

    #[test]
    fn test_same_event_three_times_only_first_executed() {
        let mut dedup = IdempotencyDeduplicator::default();
        let first = dedup.is_duplicate(999);
        let second = dedup.is_duplicate(999);
        let third = dedup.is_duplicate(999);
        assert!(!first);
        assert!(second);
        assert!(third);
    }
}
