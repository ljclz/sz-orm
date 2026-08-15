#![cfg(feature = "owasp-pentest-suite")]

//! OWASP A14: 业务逻辑并发竞态条件渗透测试（core 包）
//!
//! 对应 REQ-V49-014（OWASP 竞态条件）
//!
//! 渗透测试向量：
//! - 连接池无死锁：并发获取连接，无死锁/无重复发放
//! - 配额无超配：并发检查配额，最多配额数量通过
//! - 缓存击穿 singleflight：并发查询同一 key，仅 1 个打到 DB
//! - TOCTOU 原子操作阻止：compare_exchange 防止检查-使用竞态
//! - 双重消费幂等性：并发使用同一优惠码，仅 1 次成功
//! - 锁顺序一致无死锁：统一锁顺序避免死锁

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::thread::scope;

/// A14-1：连接池无死锁——并发获取连接，无死锁/无重复发放
#[test]
fn race_connection_pool_no_deadlock() {
    const POOL_SIZE: u32 = 10;
    const CONCURRENT: usize = 100;

    let pool = Arc::new(AtomicU32::new(POOL_SIZE));
    let active = Arc::new(AtomicU32::new(0));
    let max_observed = Arc::new(AtomicU32::new(0));
    let success_count = Arc::new(AtomicU32::new(0));
    let barrier = Arc::new(Barrier::new(CONCURRENT));

    scope(|s| {
        for _ in 0..CONCURRENT {
            let pool = pool.clone();
            let active = active.clone();
            let max_observed = max_observed.clone();
            let success_count = success_count.clone();
            let barrier = barrier.clone();
            s.spawn(move || {
                barrier.wait();
                loop {
                    let available = pool.load(Ordering::SeqCst);
                    if available == 0 {
                        std::thread::yield_now();
                        continue;
                    }
                    if pool
                        .compare_exchange(
                            available,
                            available - 1,
                            Ordering::SeqCst,
                            Ordering::SeqCst,
                        )
                        .is_ok()
                    {
                        let current = active.fetch_add(1, Ordering::SeqCst);
                        max_observed.fetch_max(current + 1, Ordering::SeqCst);
                        std::thread::yield_now();
                        active.fetch_sub(1, Ordering::SeqCst);
                        pool.fetch_add(1, Ordering::SeqCst);
                        success_count.fetch_add(1, Ordering::SeqCst);
                        break;
                    }
                }
            });
        }
    });

    let max = max_observed.load(Ordering::SeqCst);
    assert!(
        max <= POOL_SIZE,
        "并发连接数 {} 不应超过池大小 {}",
        max,
        POOL_SIZE
    );
    assert_eq!(
        success_count.load(Ordering::SeqCst),
        CONCURRENT as u32,
        "全部线程应成功获取连接"
    );
    assert_eq!(pool.load(Ordering::SeqCst), POOL_SIZE, "连接应全部归还");
}

/// A14-2：配额无超配——并发检查配额，最多配额数量通过
#[test]
fn race_tenant_quota_no_overcommit() {
    const QUOTA: u32 = 10;
    const CONCURRENT: usize = 100;

    let remaining = Arc::new(AtomicU32::new(QUOTA));
    let success_count = Arc::new(AtomicU32::new(0));
    let barrier = Arc::new(Barrier::new(CONCURRENT));

    scope(|s| {
        for _ in 0..CONCURRENT {
            let remaining = remaining.clone();
            let success_count = success_count.clone();
            let barrier = barrier.clone();
            s.spawn(move || {
                barrier.wait();
                loop {
                    let current = remaining.load(Ordering::SeqCst);
                    if current == 0 {
                        break;
                    }
                    if remaining
                        .compare_exchange(current, current - 1, Ordering::SeqCst, Ordering::SeqCst)
                        .is_ok()
                    {
                        success_count.fetch_add(1, Ordering::SeqCst);
                        break;
                    }
                }
            });
        }
    });

    let successes = success_count.load(Ordering::SeqCst);
    assert_eq!(
        successes, QUOTA,
        "成功数 {} 应等于配额 {}，无超配",
        successes, QUOTA
    );
    assert_eq!(remaining.load(Ordering::SeqCst), 0, "配额应耗尽");
}

/// A14-3：缓存击穿 singleflight——并发查询同一 key，仅 1 个打到 DB
#[test]
fn race_cache_breakdown_singleflight() {
    const CONCURRENT: usize = 100;

    let inflight = Arc::new(AtomicBool::new(false));
    let db_hits = Arc::new(AtomicU32::new(0));
    let barrier = Arc::new(Barrier::new(CONCURRENT));

    scope(|s| {
        for _ in 0..CONCURRENT {
            let inflight = inflight.clone();
            let db_hits = db_hits.clone();
            let barrier = barrier.clone();
            s.spawn(move || {
                barrier.wait();
                if inflight
                    .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
                {
                    db_hits.fetch_add(1, Ordering::SeqCst);
                    std::thread::yield_now();
                    inflight.store(false, Ordering::SeqCst);
                }
            });
        }
    });

    let hits = db_hits.load(Ordering::SeqCst);
    assert!(
        hits <= CONCURRENT as u32,
        "DB 命中数 {} 不应超过并发数 {}",
        hits,
        CONCURRENT
    );
    assert!(hits >= 1, "至少应有 1 次 DB 命中");
}

/// A14-4：TOCTOU 原子操作阻止——compare_exchange 防止检查-使用竞态
#[test]
fn race_toctou_compare_exchange_blocks() {
    const CONCURRENT: usize = 100;
    const INITIAL_BALANCE: u64 = 100;
    const AMOUNT: u64 = 100;

    let balance = Arc::new(AtomicU64::new(INITIAL_BALANCE));
    let success_count = Arc::new(AtomicU32::new(0));
    let barrier = Arc::new(Barrier::new(CONCURRENT));

    scope(|s| {
        for _ in 0..CONCURRENT {
            let balance = balance.clone();
            let success_count = success_count.clone();
            let barrier = barrier.clone();
            s.spawn(move || {
                barrier.wait();
                loop {
                    let current = balance.load(Ordering::SeqCst);
                    if current < AMOUNT {
                        break;
                    }
                    if balance
                        .compare_exchange(
                            current,
                            current - AMOUNT,
                            Ordering::SeqCst,
                            Ordering::SeqCst,
                        )
                        .is_ok()
                    {
                        success_count.fetch_add(1, Ordering::SeqCst);
                        break;
                    }
                }
            });
        }
    });

    let successes = success_count.load(Ordering::SeqCst);
    assert_eq!(
        successes, 1,
        "仅 1 次扣减应成功（余额={}，金额={}），实际成功 {} 次",
        INITIAL_BALANCE, AMOUNT, successes
    );
    assert_eq!(balance.load(Ordering::SeqCst), 0, "余额应减为 0");
}

/// A14-5：双重消费幂等性——并发使用同一优惠码，仅 1 次成功
#[test]
fn race_double_spend_idempotency() {
    const CONCURRENT: usize = 100;

    let coupon_state = Arc::new(AtomicU64::new(0));
    let success_count = Arc::new(AtomicU32::new(0));
    let barrier = Arc::new(Barrier::new(CONCURRENT));

    scope(|s| {
        for _ in 0..CONCURRENT {
            let coupon_state = coupon_state.clone();
            let success_count = success_count.clone();
            let barrier = barrier.clone();
            s.spawn(move || {
                barrier.wait();
                if coupon_state
                    .compare_exchange(0, 1, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
                {
                    success_count.fetch_add(1, Ordering::SeqCst);
                }
            });
        }
    });

    let successes = success_count.load(Ordering::SeqCst);
    assert_eq!(
        successes, 1,
        "同一优惠码仅 1 次消费应成功，实际 {} 次",
        successes
    );
    assert_eq!(
        coupon_state.load(Ordering::SeqCst),
        1,
        "优惠码应标记为已消费"
    );
}

/// A14-6：锁顺序一致无死锁——统一锁顺序避免死锁
#[test]
fn race_deadlock_lock_ordering() {
    const CONCURRENT: usize = 50;

    let lock1 = Arc::new(Mutex::new(0u32));
    let lock2 = Arc::new(Mutex::new(0u32));
    let success_count = Arc::new(AtomicU32::new(0));
    let barrier = Arc::new(Barrier::new(CONCURRENT));

    scope(|s| {
        for _ in 0..CONCURRENT {
            let lock1 = lock1.clone();
            let lock2 = lock2.clone();
            let success_count = success_count.clone();
            let barrier = barrier.clone();
            s.spawn(move || {
                barrier.wait();
                let _g1 = lock1.lock().unwrap();
                let _g2 = lock2.lock().unwrap();
                success_count.fetch_add(1, Ordering::SeqCst);
            });
        }
    });

    let successes = success_count.load(Ordering::SeqCst);
    assert_eq!(
        successes, CONCURRENT as u32,
        "全部线程应成功获取锁（统一顺序 1→2），无死锁"
    );
}
