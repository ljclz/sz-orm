//! v10.4.0 T10：边界条件测试套件
//!
//! 覆盖空集合/单元素/最大/最小/溢出/精度损失边界。

use sz_orm_core::batch_advisor::{DynamicBatchAdvisor, DynamicBatchConfig};
use sz_orm_core::DbType;

#[test]
fn empty_collection_query() {
    let advisor = DynamicBatchAdvisor::new(DynamicBatchConfig::default());
    let batch = advisor.advise_dynamic(DbType::MySQL, 0, 0);
    assert!(batch >= 1, "空集合查询批大小应至少为 1");
}

#[test]
fn single_element_aggregate() {
    let advisor = DynamicBatchAdvisor::new(DynamicBatchConfig::default());
    advisor.record_execution(1, 100.0, 10.0);
    let batch = advisor.advise_dynamic(DbType::MySQL, 1, 100);
    assert!(batch >= 1, "单元素聚合批大小应至少为 1");
}

#[test]
fn max_value_batch_size() {
    let config = DynamicBatchConfig {
        initial_size: 500,
        min_size: 1,
        max_size: 100,
        ema_factor: 0.3,
        adjust_threshold: 0.1,
        convergence_threshold: 0.05,
    };
    let advisor = DynamicBatchAdvisor::new(config);
    let batch = advisor.advise_dynamic(DbType::MySQL, 10000, 100000);
    assert!(batch <= 100, "批大小不应超过 max_size=100, got={}", batch);
}

#[test]
fn min_value_batch_size() {
    let config = DynamicBatchConfig {
        initial_size: 500,
        min_size: 10,
        max_size: 5000,
        ema_factor: 0.3,
        adjust_threshold: 0.1,
        convergence_threshold: 0.05,
    };
    let advisor = DynamicBatchAdvisor::new(config);
    advisor.record_execution(500, 1000.0, 100.0);
    advisor.record_execution(500, 1000.0, 100.0);
    let batch = advisor.advise_dynamic(DbType::MySQL, 1, 1);
    assert!(batch >= 1, "批大小应至少为 1, got={}", batch);
    assert!(batch <= 5000, "批大小不应超过 max_size=5000, got={}", batch);
}

#[test]
fn integer_overflow_boundary() {
    let advisor = DynamicBatchAdvisor::new(DynamicBatchConfig::default());
    let batch = advisor.advise_dynamic(DbType::MySQL, usize::MAX, usize::MAX);
    assert!(batch >= 1, "u64::MAX 输入应正常处理不溢出");
}

#[test]
fn float_precision_loss_boundary() {
    let config = DynamicBatchConfig {
        initial_size: 500,
        min_size: 1,
        max_size: 5000,
        ema_factor: 0.0000001,
        adjust_threshold: 0.0000001,
        convergence_threshold: 0.0000001,
    };
    let advisor = DynamicBatchAdvisor::new(config);
    advisor.record_execution(500, 1.0e-10, 1.0e-10);
    let batch = advisor.advise_dynamic(DbType::MySQL, 1000, 10000);
    assert!(batch >= 1, "极小浮点值应正常处理不精度损失");
}
