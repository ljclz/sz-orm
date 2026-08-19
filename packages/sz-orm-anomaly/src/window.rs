//! 滑动窗口：按时间戳淘汰旧数据，内存上限保护
//!
//! 使用 `VecDeque` 双端队列，头尾 O(1) 操作。写入时检查头部是否超过窗口大小，
//! 超过则 `pop_front` 淘汰。内存上限 10 MB，超限时降采样丢弃最旧数据。

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use parking_lot::RwLock;

use crate::collector::{ErrorMetric, PoolMetric, SlowQueryMetric};

/// 滑动窗口内存上限（10 MB，DFX 4.1.3）
const MAX_MEMORY_BYTES: usize = 10 * 1024 * 1024;

/// 当前时间戳（毫秒，UNIX_EPOCH 起）
pub fn current_timestamp_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 滑动窗口
///
/// 存储三类指标（慢查询/错误/连接池），按时间戳淘汰旧数据。
/// 线程安全：内部使用 `RwLock<VecDeque>`，读多写少场景高效。
pub struct SlidingWindow {
    slow_query_metrics: RwLock<VecDeque<SlowQueryMetric>>,
    error_metrics: RwLock<VecDeque<ErrorMetric>>,
    pool_metrics: RwLock<VecDeque<PoolMetric>>,
    window_size: Duration,
    /// 累计丢弃数（降采样计数）
    dropped_count: AtomicU64,
    /// 累计写入数
    written_count: AtomicU64,
}

impl SlidingWindow {
    /// 创建滑动窗口
    pub fn new(window_size: Duration) -> Self {
        Self {
            slow_query_metrics: RwLock::new(VecDeque::new()),
            error_metrics: RwLock::new(VecDeque::new()),
            pool_metrics: RwLock::new(VecDeque::new()),
            window_size,
            dropped_count: AtomicU64::new(0),
            written_count: AtomicU64::new(0),
        }
    }

    /// 写入慢查询指标
    pub fn push_slow_query(&self, metric: SlowQueryMetric) {
        let mut metrics = self.slow_query_metrics.write();
        metrics.push_back(metric);
        self.evict_old(&mut metrics, |m| m.timestamp);
        self.enforce_memory_limit(&mut metrics, |m| m.estimated_size());
        self.written_count.fetch_add(1, Ordering::Relaxed);
    }

    /// 写入错误指标
    pub fn push_error(&self, metric: ErrorMetric) {
        let mut metrics = self.error_metrics.write();
        metrics.push_back(metric);
        self.evict_old(&mut metrics, |m| m.timestamp);
        self.enforce_memory_limit(&mut metrics, |m| m.estimated_size());
        self.written_count.fetch_add(1, Ordering::Relaxed);
    }

    /// 写入连接池指标
    pub fn push_pool(&self, metric: PoolMetric) {
        let mut metrics = self.pool_metrics.write();
        metrics.push_back(metric);
        self.evict_old(&mut metrics, |m| m.timestamp);
        self.enforce_memory_limit(&mut metrics, |m| m.estimated_size());
        self.written_count.fetch_add(1, Ordering::Relaxed);
    }

    /// 按时间戳淘汰旧数据
    fn evict_old<T, F: Fn(&T) -> u64>(&self, metrics: &mut VecDeque<T>, get_ts: F) {
        let now = current_timestamp_ms();
        let threshold = now.saturating_sub(self.window_size.as_millis() as u64);
        while let Some(front) = metrics.front() {
            if get_ts(front) < threshold {
                metrics.pop_front();
                self.dropped_count.fetch_add(1, Ordering::Relaxed);
            } else {
                break;
            }
        }
    }

    /// 内存上限保护：超限时丢弃最旧数据
    fn enforce_memory_limit<T, F: Fn(&T) -> usize>(&self, metrics: &mut VecDeque<T>, size: F) {
        let mut total: usize = metrics.iter().map(&size).sum();
        while total > MAX_MEMORY_BYTES {
            if let Some(removed) = metrics.pop_front() {
                total = total.saturating_sub(size(&removed));
                self.dropped_count.fetch_add(1, Ordering::Relaxed);
            } else {
                break;
            }
        }
    }

    /// 获取慢查询指标快照
    pub fn slow_query_metrics(&self) -> Vec<SlowQueryMetric> {
        self.slow_query_metrics.read().iter().cloned().collect()
    }

    /// 获取错误指标快照
    pub fn error_metrics(&self) -> Vec<ErrorMetric> {
        self.error_metrics.read().iter().cloned().collect()
    }

    /// 获取连接池指标快照
    pub fn pool_metrics(&self) -> Vec<PoolMetric> {
        self.pool_metrics.read().iter().cloned().collect()
    }

    /// 慢查询指标数量
    pub fn slow_query_count(&self) -> usize {
        self.slow_query_metrics.read().len()
    }

    /// 错误指标数量
    pub fn error_count(&self) -> usize {
        self.error_metrics.read().len()
    }

    /// 连接池指标数量
    pub fn pool_count(&self) -> usize {
        self.pool_metrics.read().len()
    }

    /// 累计丢弃数（含时间淘汰 + 内存上限丢弃）
    pub fn dropped_count(&self) -> u64 {
        self.dropped_count.load(Ordering::Relaxed)
    }

    /// 累计写入数
    pub fn written_count(&self) -> u64 {
        self.written_count.load(Ordering::Relaxed)
    }

    /// 估算内存占用（字节）
    pub fn estimated_memory_bytes(&self) -> usize {
        let slow = self
            .slow_query_metrics
            .read()
            .iter()
            .map(|m| m.estimated_size())
            .sum::<usize>();
        let error = self
            .error_metrics
            .read()
            .iter()
            .map(|m| m.estimated_size())
            .sum::<usize>();
        let pool = self
            .pool_metrics
            .read()
            .iter()
            .map(|m| m.estimated_size())
            .sum::<usize>();
        slow + error + pool
    }

    /// 窗口大小
    pub fn window_size(&self) -> Duration {
        self.window_size
    }

    /// 清空所有指标
    pub fn clear(&self) {
        self.slow_query_metrics.write().clear();
        self.error_metrics.write().clear();
        self.pool_metrics.write().clear();
    }

    /// 获取最近 N 毫秒内的慢查询指标
    pub fn recent_slow_query_metrics(&self, last_ms: u64) -> Vec<SlowQueryMetric> {
        let now = current_timestamp_ms();
        let threshold = now.saturating_sub(last_ms);
        self.slow_query_metrics
            .read()
            .iter()
            .filter(|m| m.timestamp >= threshold)
            .cloned()
            .collect()
    }

    /// 获取最近 N 毫秒内的错误指标
    pub fn recent_error_metrics(&self, last_ms: u64) -> Vec<ErrorMetric> {
        let now = current_timestamp_ms();
        let threshold = now.saturating_sub(last_ms);
        self.error_metrics
            .read()
            .iter()
            .filter(|m| m.timestamp >= threshold)
            .cloned()
            .collect()
    }

    /// 获取最近 N 毫秒内的连接池指标
    pub fn recent_pool_metrics(&self, last_ms: u64) -> Vec<PoolMetric> {
        let now = current_timestamp_ms();
        let threshold = now.saturating_sub(last_ms);
        self.pool_metrics
            .read()
            .iter()
            .filter(|m| m.timestamp >= threshold)
            .cloned()
            .collect()
    }
}

impl Default for SlidingWindow {
    fn default() -> Self {
        Self::new(Duration::from_secs(30 * 60))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_and_count() {
        let window = SlidingWindow::new(Duration::from_secs(60));
        let now = current_timestamp_ms();

        window.push_slow_query(SlowQueryMetric {
            timestamp: now,
            elapsed_ms: 150,
            sql_summary: "SELECT * FROM users".to_string(),
        });
        window.push_error(ErrorMetric {
            timestamp: now,
            error_type: crate::collector::ErrorType::SqlError,
        });
        window.push_pool(PoolMetric {
            timestamp: now,
            active: 10,
            idle: 5,
            waiting: 0,
            acquire_ms: 50,
        });

        assert_eq!(window.slow_query_count(), 1);
        assert_eq!(window.error_count(), 1);
        assert_eq!(window.pool_count(), 1);
        assert_eq!(window.written_count(), 3);
    }

    #[test]
    fn test_evict_old_data() {
        let window = SlidingWindow::new(Duration::from_millis(100));
        let old_ts = current_timestamp_ms().saturating_sub(200);
        let now = current_timestamp_ms();

        window.push_slow_query(SlowQueryMetric {
            timestamp: old_ts,
            elapsed_ms: 150,
            sql_summary: "old".to_string(),
        });
        window.push_slow_query(SlowQueryMetric {
            timestamp: now,
            elapsed_ms: 200,
            sql_summary: "new".to_string(),
        });

        let metrics = window.slow_query_metrics();
        assert_eq!(metrics.len(), 1);
        assert_eq!(metrics[0].sql_summary, "new");
        assert!(window.dropped_count() >= 1);
    }

    #[test]
    fn test_recent_metrics() {
        let window = SlidingWindow::new(Duration::from_secs(60));
        let now = current_timestamp_ms();
        let old = now.saturating_sub(5000);

        window.push_error(ErrorMetric {
            timestamp: old,
            error_type: crate::collector::ErrorType::SqlError,
        });
        window.push_error(ErrorMetric {
            timestamp: now,
            error_type: crate::collector::ErrorType::TimeoutError,
        });

        let recent = window.recent_error_metrics(1000);
        assert_eq!(recent.len(), 1);
        assert_eq!(
            recent[0].error_type,
            crate::collector::ErrorType::TimeoutError
        );
    }

    #[test]
    fn test_clear() {
        let window = SlidingWindow::new(Duration::from_secs(60));
        let now = current_timestamp_ms();
        window.push_slow_query(SlowQueryMetric {
            timestamp: now,
            elapsed_ms: 100,
            sql_summary: "test".to_string(),
        });
        assert_eq!(window.slow_query_count(), 1);
        window.clear();
        assert_eq!(window.slow_query_count(), 0);
    }

    #[test]
    fn test_memory_limit() {
        let window = SlidingWindow::new(Duration::from_secs(3600));
        let now = current_timestamp_ms();
        // 写入大量数据触发内存上限
        for i in 0..100_000 {
            window.push_slow_query(SlowQueryMetric {
                timestamp: now,
                elapsed_ms: 100,
                sql_summary: format!("SELECT * FROM table_{i} WHERE col = 'value'"),
            });
        }
        assert!(window.estimated_memory_bytes() <= MAX_MEMORY_BYTES);
    }
}
