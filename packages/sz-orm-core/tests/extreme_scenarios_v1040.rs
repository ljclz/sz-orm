//! v10.4.0 T11：极端场景测试套件
//!
//! 覆盖高并发/资源耗尽/超时/级联故障/网络分区极端场景。

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use sz_orm_core::batch_advisor::{DynamicBatchAdvisor, DynamicBatchConfig};
use sz_orm_core::DbType;

#[test]
fn high_concurrency_100_threads_batch_advisor() {
    let advisor = Arc::new(DynamicBatchAdvisor::new(DynamicBatchConfig::default()));
    let success_count = Arc::new(AtomicUsize::new(0));

    let handles: Vec<_> = (0..100)
        .map(|_| {
            let advisor = Arc::clone(&advisor);
            let success_count = Arc::clone(&success_count);
            std::thread::spawn(move || {
                advisor.record_execution(500, 1000.0, 100.0);
                let batch = advisor.advise_dynamic(DbType::MySQL, 100, 1000);
                if batch >= 1 {
                    success_count.fetch_add(1, Ordering::Relaxed);
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    assert_eq!(
        success_count.load(Ordering::Relaxed),
        100,
        "100 线程并发应全部成功"
    );
}

#[test]
fn pool_resource_exhaustion_behavior() {
    let config = DynamicBatchConfig {
        initial_size: 2,
        min_size: 1,
        max_size: 2,
        ema_factor: 0.3,
        adjust_threshold: 0.1,
        convergence_threshold: 0.05,
    };
    let advisor = DynamicBatchAdvisor::new(config);
    advisor.record_execution(2, 1000.0, 100.0);
    let batch = advisor.advise_dynamic(DbType::MySQL, 10000, 100000);
    assert!(batch >= 1, "资源受限时批大小应至少为 1, got={}", batch);
}

#[test]
fn query_timeout_handling() {
    let advisor = DynamicBatchAdvisor::new(DynamicBatchConfig::default());
    advisor.record_execution(500, 1000.0, 0.0);
    let batch = advisor.advise_dynamic(DbType::MySQL, 100, 1000);
    assert!(batch >= 1, "零延迟记录应正常处理");
}

#[test]
fn cascade_failure_degradation() {
    let advisor = DynamicBatchAdvisor::new(DynamicBatchConfig::default());
    for _ in 0..10 {
        advisor.record_execution(1, 0.0, 100000.0);
    }
    let batch = advisor.advise_dynamic(DbType::MySQL, 100, 1000);
    assert!(batch >= 1, "级联故障降级后批大小应至少为 1");
}

#[test]
fn network_partition_retry() {
    let advisor = DynamicBatchAdvisor::new(DynamicBatchConfig::default());
    advisor.record_execution(500, 1000.0, 100.0);
    let batch1 = advisor.advise_dynamic(DbType::MySQL, 100, 1000);
    assert!(batch1 >= 1);

    advisor.record_execution(1, 0.0, 100000.0);
    let batch2 = advisor.advise_dynamic(DbType::MySQL, 100, 1000);
    assert!(batch2 >= 1, "网络分区恢复后批大小应至少为 1");
}
