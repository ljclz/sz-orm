//! OLAP 向量化接线测试（`olap-vectorized` feature）
//!
//! 验证 `VectorizedExecutor` 端到端执行：
//! 列存批量构建 → 向量化算子执行 → 聚合结果正确性 → 内存降级。
//!
//! 生产调用点证据：
//! - `VectorizedExecutor::execute` → `packages/sz-orm-parallel/src/vectorized.rs:108`
//! - `ColumnarBatch::push_row` → `packages/sz-orm-parallel/src/vectorized.rs:32`

use sz_orm_parallel::{ColumnarBatch, VectorizedExecutor, VectorizedOp};

fn sales_batch() -> ColumnarBatch {
    let mut batch = ColumnarBatch::new(vec!["dept_id".into(), "amount".into(), "qty".into()]);
    batch.push_row(&[1.0, 100.0, 10.0]);
    batch.push_row(&[1.0, 200.0, 20.0]);
    batch.push_row(&[2.0, 300.0, 30.0]);
    batch.push_row(&[2.0, 400.0, 40.0]);
    batch.push_row(&[3.0, 500.0, 50.0]);
    batch
}

/// W1: 向量化 SUM 聚合正确
#[test]
fn wiring_vectorized_sum() {
    let exec = VectorizedExecutor::new();
    let batch = sales_batch();
    let result = exec.execute(
        &VectorizedOp::Sum {
            column: "amount".into(),
        },
        &batch,
    );
    assert_eq!(result.values[0], 1500.0);
    assert_eq!(result.processed_rows, 5);
}

/// W2: 向量化 COUNT 聚合正确
#[test]
fn wiring_vectorized_count() {
    let exec = VectorizedExecutor::new();
    let batch = sales_batch();
    let result = exec.execute(&VectorizedOp::Count, &batch);
    assert_eq!(result.values[0], 5.0);
}

/// W3: 向量化 MIN/MAX 正确
#[test]
fn wiring_vectorized_min_max() {
    let exec = VectorizedExecutor::new();
    let batch = sales_batch();
    let min = exec.execute(
        &VectorizedOp::Min {
            column: "amount".into(),
        },
        &batch,
    );
    let max = exec.execute(
        &VectorizedOp::Max {
            column: "amount".into(),
        },
        &batch,
    );
    assert_eq!(min.values[0], 100.0);
    assert_eq!(max.values[0], 500.0);
}

/// W4: 向量化过滤正确
#[test]
fn wiring_vectorized_filter() {
    let exec = VectorizedExecutor::new();
    let batch = sales_batch();
    let result = exec.execute(
        &VectorizedOp::Filter {
            column: "amount".into(),
            threshold: 250.0,
        },
        &batch,
    );
    assert_eq!(result.values[0], 3.0);
}

/// W5: 内存超限降级
#[test]
fn wiring_memory_degradation() {
    let exec = VectorizedExecutor::new().with_memory_limit(0);
    let batch = sales_batch();
    let result = exec.execute(
        &VectorizedOp::Sum {
            column: "amount".into(),
        },
        &batch,
    );
    assert!(result.degraded);
    assert_eq!(result.values[0], 1500.0);
}
