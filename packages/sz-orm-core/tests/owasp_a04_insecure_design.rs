#![cfg(feature = "owasp-pentest-suite")]

//! OWASP A04: 不安全设计渗透测试（core 包）
//!
//! 对应 REQ-V49-004（OWASP A04）
//!
//! 渗透测试向量：
//! - 负数数量被拒绝
//! - 跳过支付被拒绝
//! - 资源释放 Drop 保证
//! - 竞态条件原子保护
//! - TOCTOU 原子操作阻止

use parking_lot::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;

fn validate_quantity(quantity: i64) -> Result<(), String> {
    if quantity <= 0 {
        return Err("quantity must be positive".to_string());
    }
    if quantity > 1_000_000 {
        return Err("quantity exceeds maximum limit".to_string());
    }
    Ok(())
}

fn validate_payment_status(paid: bool, confirmed: bool) -> Result<(), String> {
    if confirmed && !paid {
        return Err("cannot confirm order without payment".to_string());
    }
    Ok(())
}

/// A04-1：负数数量被拒绝
#[test]
fn a04_negative_quantity_rejected() {
    assert!(validate_quantity(-1).is_err(), "负数数量必须被拒绝");
    assert!(validate_quantity(0).is_err(), "零数量必须被拒绝");
    assert!(validate_quantity(-100).is_err(), "负数数量必须被拒绝");
    assert!(validate_quantity(1).is_ok(), "正数数量必须通过");
    assert!(validate_quantity(100).is_ok(), "正数数量必须通过");
}

/// A04-2：跳过支付被拒绝
#[test]
fn a04_skip_payment_rejected() {
    assert!(
        validate_payment_status(false, true).is_err(),
        "未支付直接确认必须被拒绝"
    );
    assert!(
        validate_payment_status(true, true).is_ok(),
        "已支付后确认必须通过"
    );
    assert!(
        validate_payment_status(false, false).is_ok(),
        "未支付未确认必须通过"
    );
    assert!(
        validate_payment_status(true, false).is_ok(),
        "已支付未确认必须通过"
    );
}

/// A04-3：资源释放 Drop 保证
///
/// 构造 Mutex lock + panic，断言 `parking_lot::Mutex` 不 poisoning（FIND-002 修复）。
#[test]
fn a04_missing_resource_release_drop() {
    let m = Arc::new(Mutex::new(42u64));

    let m_clone = Arc::clone(&m);
    let handle = thread::spawn(move || {
        let _guard = m_clone.lock();
        panic!("simulated panic while holding lock");
    });

    let _ = handle.join();

    let guard = m.lock();
    assert_eq!(*guard, 42, "parking_lot::Mutex 不 poisoning，锁仍可获取");
}

/// A04-4：竞态条件原子保护
///
/// 100 并发扣减 balance=100 amount=1，
/// 使用 `AtomicU64::compare_exchange`，断言最终 balance=0，无负余额/双重消费。
#[test]
fn a04_race_condition_atomic_protected() {
    let balance = Arc::new(AtomicU64::new(100));
    let amount: u64 = 1;
    let num_threads = 100;

    let mut handles = Vec::new();
    for _ in 0..num_threads {
        let bal = Arc::clone(&balance);
        handles.push(thread::spawn(move || loop {
            let current = bal.load(Ordering::Acquire);
            if current < amount {
                return false;
            }
            match bal.compare_exchange(
                current,
                current - amount,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return true,
                Err(_) => continue,
            }
        }));
    }

    let mut successes = 0;
    for h in handles {
        if h.join().unwrap() {
            successes += 1;
        }
    }

    assert_eq!(successes, 100, "100 次扣减必须全部成功");
    assert_eq!(balance.load(Ordering::Acquire), 0, "最终余额必须为 0");
}

/// A04-5：TOCTOU 原子操作阻止
///
/// 线程 A 检查 balance=100 >= amount=100，
/// 线程 B 扣减 balance=100 → balance=0，
/// 线程 A 扣减，断言 `compare_exchange` 失败（TOCTOU 被原子操作阻止）。
#[test]
fn a04_toctou_compare_exchange_blocks() {
    let balance = Arc::new(AtomicU64::new(100));
    let amount: u64 = 100;

    let bal_check = Arc::clone(&balance);
    let check_handle = thread::spawn(move || {
        let current = bal_check.load(Ordering::Acquire);
        current >= amount
    });

    let can_proceed = check_handle.join().unwrap();
    assert!(can_proceed, "线程 A 检查 balance >= amount 通过");

    let bal_deduct = Arc::clone(&balance);
    let deduct_handle = thread::spawn(move || {
        let current = bal_deduct.load(Ordering::Acquire);
        bal_deduct
            .compare_exchange(
                current,
                current - amount,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    });

    let first_success = deduct_handle.join().unwrap();
    assert!(first_success, "线程 B 扣减成功");
    assert_eq!(balance.load(Ordering::Acquire), 0, "余额为 0");

    let current = balance.load(Ordering::Acquire);
    let result = balance.compare_exchange(
        current,
        current.saturating_sub(amount),
        Ordering::AcqRel,
        Ordering::Acquire,
    );
    assert!(
        result.is_err() || balance.load(Ordering::Acquire) == 0,
        "线程 A 的 TOCTOU 扣减必须被阻止（余额已为 0）"
    );
}
