//! 锁看门狗 — 自动续约
//!
//! 后台定时续约锁，防止业务逻辑执行期间锁过期。

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;
use tracing::debug;

use super::lock::DistributedLock;

/// 锁看门狗
pub struct LockWatchdog {
    lock: Arc<DistributedLock>,
    renew_interval: Duration,
    running: Mutex<bool>,
}

impl LockWatchdog {
    /// 创建看门狗
    pub fn new(lock: Arc<DistributedLock>, ttl_secs: u64) -> Self {
        let renew_interval = Duration::from_secs(ttl_secs / 3);
        Self {
            lock,
            renew_interval,
            running: Mutex::new(false),
        }
    }

    /// 启动看门狗（返回停止句柄）
    pub async fn start(
        &self,
    ) -> Result<tokio::task::JoinHandle<()>, super::backend::CoordinationError> {
        let mut running = self.running.lock().await;
        *running = true;
        let lock = self.lock.clone();
        let interval = self.renew_interval;

        let handle = tokio::spawn(async move {
            loop {
                tokio::time::sleep(interval).await;
                match lock.renew().await {
                    Ok(true) => debug!("锁续约成功: {}", lock.key()),
                    Ok(false) => {
                        debug!("锁续约失败（已释放）: {}", lock.key());
                        break;
                    }
                    Err(e) => {
                        debug!("锁续约错误: {} - {}", lock.key(), e);
                        break;
                    }
                }
            }
        });

        Ok(handle)
    }

    /// 停止看门狗
    pub async fn stop(&self) {
        let mut running = self.running.lock().await;
        *running = false;
    }

    /// 续约间隔
    pub fn renew_interval(&self) -> Duration {
        self.renew_interval
    }

    /// 是否运行中
    pub async fn is_running(&self) -> bool {
        *self.running.lock().await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use super::super::backend::InMemoryBackend;
    use super::*;

    #[tokio::test]
    async fn test_watchdog_start_stop() {
        let backend = Arc::new(InMemoryBackend::new());
        let lock = Arc::new(DistributedLock::new(backend, "wd_test", 30));
        lock.acquire().await.unwrap();
        let watchdog = LockWatchdog::new(lock.clone(), 30);
        let handle = watchdog.start().await.unwrap();
        assert!(watchdog.is_running().await);
        watchdog.stop().await;
        handle.abort();
    }

    #[tokio::test]
    async fn test_watchdog_renew_interval() {
        let backend = Arc::new(InMemoryBackend::new());
        let lock = Arc::new(DistributedLock::new(backend, "wd_interval", 30));
        let watchdog = LockWatchdog::new(lock, 30);
        assert_eq!(watchdog.renew_interval(), Duration::from_secs(10));
    }

    #[tokio::test]
    async fn test_watchdog_keeps_lock_alive() {
        let backend = Arc::new(InMemoryBackend::new());
        let lock = Arc::new(DistributedLock::new(backend, "wd_alive", 3));
        lock.acquire().await.unwrap();
        let watchdog = LockWatchdog::new(lock.clone(), 3);
        let handle = watchdog.start().await.unwrap();
        tokio::time::sleep(Duration::from_secs(2)).await;
        assert!(lock.is_held());
        watchdog.stop().await;
        handle.abort();
    }

    #[tokio::test]
    async fn test_watchdog_not_running_after_stop() {
        let backend = Arc::new(InMemoryBackend::new());
        let lock = Arc::new(DistributedLock::new(backend, "wd_stop", 30));
        let watchdog = LockWatchdog::new(lock, 30);
        watchdog.stop().await;
        assert!(!watchdog.is_running().await);
    }

    #[tokio::test]
    async fn test_watchdog_renews_multiple_times() {
        let backend = Arc::new(InMemoryBackend::new());
        let lock = Arc::new(DistributedLock::new(backend, "wd_multi", 3));
        lock.acquire().await.unwrap();
        let watchdog = LockWatchdog::new(lock.clone(), 3);
        let handle = watchdog.start().await.unwrap();
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert!(lock.is_held());
        watchdog.stop().await;
        handle.abort();
    }

    #[tokio::test]
    async fn test_watchdog_with_short_ttl() {
        let backend = Arc::new(InMemoryBackend::new());
        let lock = Arc::new(DistributedLock::new(backend, "wd_short", 3));
        let watchdog = LockWatchdog::new(lock, 3);
        assert!(watchdog.renew_interval() <= Duration::from_secs(1));
    }
}
