//! AsyncBackpressureController — 异步背压控制器
//!
//! 复用既有 BackpressureController 语义，扩展为异步 Stream 集成。
//! 消费者慢于生产者时暂停生产者拉取，背压检查开销 ≤ 1μs/次。

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use tokio::sync::Notify;

/// 异步背压控制器
pub struct AsyncBackpressureController {
    /// 背压阈值
    threshold: usize,
    /// 当前积压量
    current: Arc<AtomicUsize>,
    /// 异步通知
    notify: Arc<Notify>,
}

impl AsyncBackpressureController {
    pub fn new(threshold: usize) -> Self {
        Self {
            threshold,
            current: Arc::new(AtomicUsize::new(0)),
            notify: Arc::new(Notify::new()),
        }
    }

    /// 检查是否允许推送
    ///
    /// `current < threshold` 时返回 true，否则等待消费者处理。
    pub async fn allow_push(&self) -> bool {
        if self.threshold == 0 {
            self.notify.notified().await;
            return false;
        }
        loop {
            if self.current.load(Ordering::Relaxed) < self.threshold {
                return true;
            }
            self.notify.notified().await;
        }
    }

    /// 非阻塞检查是否允许推送
    pub fn try_allow_push(&self) -> bool {
        if self.threshold == 0 {
            return false;
        }
        self.current.load(Ordering::Relaxed) < self.threshold
    }

    /// 入队
    pub fn push(&self) {
        self.current.fetch_add(1, Ordering::Relaxed);
    }

    /// 出队 + 唤醒等待的生产者
    pub fn pop(&self) {
        self.current.fetch_sub(1, Ordering::Relaxed);
        self.notify.notify_one();
    }

    /// 当前积压量
    pub fn pending(&self) -> usize {
        self.current.load(Ordering::Relaxed)
    }

    /// 阈值
    pub fn threshold(&self) -> usize {
        self.threshold
    }
}

impl Clone for AsyncBackpressureController {
    fn clone(&self) -> Self {
        Self {
            threshold: self.threshold,
            current: Arc::clone(&self.current),
            notify: Arc::clone(&self.notify),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_below_threshold() {
        let controller = AsyncBackpressureController::new(100);
        assert!(controller.try_allow_push());
        assert_eq!(controller.pending(), 0);
    }

    #[test]
    fn push_increments_pending() {
        let controller = AsyncBackpressureController::new(100);
        controller.push();
        controller.push();
        assert_eq!(controller.pending(), 2);
    }

    #[test]
    fn pop_decrements_pending() {
        let controller = AsyncBackpressureController::new(100);
        controller.push();
        controller.push();
        controller.pop();
        assert_eq!(controller.pending(), 1);
    }

    #[test]
    fn try_allow_push_at_threshold() {
        let controller = AsyncBackpressureController::new(2);
        controller.push();
        controller.push();
        assert!(!controller.try_allow_push());
    }

    #[test]
    fn try_allow_push_after_pop() {
        let controller = AsyncBackpressureController::new(2);
        controller.push();
        controller.push();
        assert!(!controller.try_allow_push());
        controller.pop();
        assert!(controller.try_allow_push());
    }

    #[test]
    fn threshold_zero_always_block() {
        let controller = AsyncBackpressureController::new(0);
        assert!(!controller.try_allow_push());
    }

    #[tokio::test]
    async fn allow_push_below_threshold() {
        let controller = AsyncBackpressureController::new(10);
        controller.push();
        assert!(controller.allow_push().await);
    }

    #[tokio::test]
    async fn allow_push_with_pop_wakeup() {
        let controller = AsyncBackpressureController::new(1);
        controller.push();
        let controller_clone = controller.clone();
        let handle = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            controller_clone.pop();
        });
        assert!(controller.allow_push().await);
        handle.await.unwrap();
    }

    #[test]
    fn clone_shares_state() {
        let controller = AsyncBackpressureController::new(100);
        let clone = controller.clone();
        controller.push();
        assert_eq!(clone.pending(), 1);
        clone.pop();
        assert_eq!(controller.pending(), 0);
    }
}
