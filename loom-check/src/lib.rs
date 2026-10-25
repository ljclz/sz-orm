//! sz-orm 连接池并发状态机的 loom 模型检查
//!
//! 这是一个独立工作区（不属于 sz-orm 主 workspace），用 [`loom`] 穷举线程交错，
//! 对连接池的 acquire/release/shutdown 核心状态机做模型检查。
//!
//! # 建模说明
//!
//! 模型忠实复刻 `packages/sz-orm-core/src/pool.rs` 的关键并发结构：
//! - `total` 连接总数（`AtomicU32` + CAS 递增，对应 `total_count`）
//! - `idle` 空闲队列（`Mutex<VecDeque>`，对应 `ArrayQueue` 的有界语义）
//! - `closed` 关闭标志（对应 `closed`）
//! - 等待/通知（有界 spin，对应 `acquire` 的指数退避 + 超时重试，避免 condvar 永久阻塞）
//!
//! # 验证的不变量
//!
//! 1. **shutdown 后绝不新建连接**（2026-10-24 深度评测修复的竞态）
//! 2. **total 永不超过 max_size**
//! 3. **计数守恒**：所有连接最终全部关闭，`total == 0`

use loom::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use loom::sync::{Arc, Mutex};
use std::collections::VecDeque;

/// 池最大连接数
pub const MAX: u32 = 3;

/// 有界等待的最大 spin 次数（模拟 tokio 超时退避）
///
/// 1 次 yield 后若仍未收到通知则回到循环顶部重试，对应真实池的退避超时重试。
/// 有界 yield 让 loom 调度器能在线程间切换，避免纯忙等导致分支爆炸。
const MAX_SPINS: usize = 1;

/// 连接池并发状态机模型
pub struct Pool {
    idle: Mutex<VecDeque<u32>>,
    total: AtomicU32,
    created: AtomicU32,
    closed: AtomicBool,
    notify_flag: Mutex<bool>,
}

impl Pool {
    /// 创建新池
    pub fn new() -> Arc<Pool> {
        Arc::new(Pool {
            idle: Mutex::new(VecDeque::new()),
            total: AtomicU32::new(0),
            created: AtomicU32::new(0),
            closed: AtomicBool::new(false),
            notify_flag: Mutex::new(false),
        })
    }

    /// 当前已创建连接总数
    pub fn created(&self) -> u32 {
        self.created.load(Ordering::SeqCst)
    }

    /// 当前在池连接总数（含空闲与借出）
    pub fn total(&self) -> u32 {
        self.total.load(Ordering::SeqCst)
    }

    /// 池是否已关闭
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    /// 获取连接
    ///
    /// 复刻 `Pool::acquire`：
    /// - 入口检查 `closed`
    /// - 主循环顶部复查 `closed`（2026-10-24 修复点 1）
    /// - 空闲队列非空则弹出
    /// - CAS 递增 total 后、创建前复查 `closed`（2026-10-24 修复点 2）
    /// - 否则有界等待后重试
    pub fn acquire(&self) -> Result<u32, &'static str> {
        if self.closed.load(Ordering::Acquire) {
            return Err("Closed");
        }
        loop {
            // 修复点 1：循环顶部复查 closed
            if self.closed.load(Ordering::Acquire) {
                return Err("Closed");
            }

            // 弹出空闲连接
            {
                let mut q = self.idle.lock().unwrap();
                if let Some(c) = q.pop_front() {
                    return Ok(c);
                }
            }

            // CAS 递增 total 并创建新连接
            let cur = self.total.load(Ordering::Acquire);
            if cur < MAX {
                if self
                    .total
                    .compare_exchange(cur, cur + 1, Ordering::SeqCst, Ordering::Acquire)
                    .is_ok()
                {
                    // 修复点 2：CAS 成功后复查 closed
                    if self.closed.load(Ordering::Acquire) {
                        self.total.fetch_sub(1, Ordering::SeqCst);
                        return Err("Closed");
                    }
                    self.created.fetch_add(1, Ordering::SeqCst);
                    return Ok(self.created.load(Ordering::SeqCst) - 1);
                }
            }

            // 有界等待（模拟退避超时）：等通知或 closed，最多 spin MAX_SPINS 次
            let mut spins = 0;
            let mut notified = false;
            loop {
                {
                    let mut flag = self.notify_flag.lock().unwrap();
                    if *flag {
                        *flag = false;
                        notified = true;
                        break;
                    }
                }
                if self.closed.load(Ordering::Acquire) {
                    break;
                }
                if spins >= MAX_SPINS {
                    break; // 超时未通知，回到循环顶部重试
                }
                loom::thread::yield_now();
                spins += 1;
            }
            let _ = (notified, spins);
            // 回到循环顶部（若 closed 已在顶部拦截）
        }
    }

    /// 归还连接
    ///
    /// 复刻 `Pool::release`：
    /// - 池已关闭则直接关闭连接（total - 1）
    /// - 否则入空闲队列并通知等待者
    pub fn release(&self, conn: u32) {
        if self.closed.load(Ordering::Acquire) {
            self.total.fetch_sub(1, Ordering::SeqCst);
            return;
        }
        {
            let mut q = self.idle.lock().unwrap();
            if q.len() < MAX as usize {
                q.push_back(conn);
            } else {
                // 队列满（极端场景）：关闭连接
                self.total.fetch_sub(1, Ordering::SeqCst);
                return;
            }
        }
        let mut flag = self.notify_flag.lock().unwrap();
        *flag = true;
    }

    /// 关闭池
    ///
    /// 复刻 `Pool::shutdown`/`close_all`：
    /// - 置位 closed
    /// - 关闭所有空闲连接（total 减去空闲数）
    /// - 通知所有等待者
    pub fn shutdown(&self) {
        self.closed.store(true, Ordering::SeqCst);
        let idle_count = {
            let mut q = self.idle.lock().unwrap();
            let n = q.len() as u32;
            q.clear();
            n
        };
        if idle_count > 0 {
            self.total.fetch_sub(idle_count, Ordering::SeqCst);
        }
        let mut flag = self.notify_flag.lock().unwrap();
        *flag = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loom::thread;

    /// 场景 1：shutdown 后绝不新建连接（深度评测回归）
    ///
    /// 复现 2026-10-24 确认的竞态：池满时一个 acquire 等待者阻塞，
    /// 主线程归还一个连接后立即 shutdown。修复前，等待者在 close_all
    /// 递减 total 后可能 CAS 新建连接；修复后循环顶部复查 closed 拦截。
    #[test]
    fn shutdown_prevents_new_connection_after_close() {
        loom::model(|| {
            let pool = Pool::new();
            let c1 = pool.acquire().unwrap();
            let c2 = pool.acquire().unwrap();
            let c3 = pool.acquire().unwrap();
            assert_eq!(pool.created(), 3);
            assert_eq!(pool.total(), 3);

            // 等待中的 acquire（池已满，应阻塞）
            let pool2 = pool.clone();
            let waiter = thread::spawn(move || pool2.acquire());

            // 归还一个连接，随后立即 shutdown
            pool.release(c3);
            pool.shutdown();

            let result = waiter.join().unwrap();
            // 若 waiter 在 shutdown 前抢到了归还的连接，则为 Ok；否则应为 Err(Closed)
            if let Ok(conn) = result {
                pool.release(conn); // 归还后连接会被关闭
            }
            // 释放主线程持有的其余连接（shutdown 后归还会被关闭）
            pool.release(c1);
            pool.release(c2);

            // 不变量：shutdown 后 created 不得增加（最多 3 个）
            assert!(
                pool.created() <= 3,
                "shutdown 后新建了连接: created={}",
                pool.created()
            );
            // 所有连接最终关闭
            assert_eq!(pool.total(), 0, "最终 total 应为 0，实际 {}", pool.total());
        });
    }

    /// 场景 2：total 永不超过 max_size，且计数守恒（无连接泄漏）
    #[test]
    fn total_never_exceeds_max() {
        loom::model(|| {
            let pool = Pool::new();
            let mut handles = Vec::new();
            // 线程数小于 MAX：验证 acquire/release 正常路径与计数守恒，
            // 同时保持状态空间在 loom 可穷举范围内
            for _ in 0..2 {
                let p = pool.clone();
                handles.push(thread::spawn(move || {
                    if let Ok(c) = p.acquire() {
                        p.release(c);
                    }
                }));
            }
            for h in handles {
                h.join().unwrap();
            }
            // 在任何交错下 total 都不得超过 MAX
            assert!(
                pool.total() <= MAX,
                "total 超过 max: total={} max={}",
                pool.total(),
                MAX
            );
            // 计数守恒：池未关闭时所有创建的连接都应留在池中（idle），
            // 无泄漏（total == created）、无凭空消失（created == total）。
            assert_eq!(
                pool.total(),
                pool.created(),
                "计数不守恒: total={} created={}",
                pool.total(),
                pool.created()
            );
        });
    }

    /// 场景 3：shutdown 后所有后续 acquire 都被拒绝，且已借出连接归还后 total 归零
    #[test]
    fn shutdown_rejects_new_acquire_and_drains() {
        loom::model(|| {
            let pool = Pool::new();
            let c1 = pool.acquire().unwrap();
            let c2 = pool.acquire().unwrap();

            pool.shutdown();

            // shutdown 后 acquire 必须失败
            let p2 = pool.clone();
            let t = thread::spawn(move || p2.acquire());
            let r = t.join().unwrap();
            assert!(r.is_err(), "shutdown 后 acquire 应失败，实际 {:?}", r);

            // 归还已借出连接 → 被关闭，total 递减
            pool.release(c1);
            pool.release(c2);
            assert_eq!(pool.total(), 0, "shutdown 后归还连接应使 total 归零");
        });
    }
}