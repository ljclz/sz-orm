//! v9.5.0 sz-pay 接线示例：演示 sz-orm 5 项新 API 的最小可运行调用。
//!
//! 对应 sz-pay 场景：批量对账、慢查询定位、连接池自愈、支付编排、repository 标准化。
//! 可直接复制使用，附 sz-pay 调用点 file:line 证据。

use std::collections::HashMap;
use sz_orm_core::mock::MockConnection;
use sz_orm_core::repository::{EntityAttributes, InMemoryRepository, Repository, SoftDeletable};
use sz_orm_core::{
    Connection, DbType, OrchestrationResult, OrchestratorConfig, RollbackStrategy, StepFn,
    TransactOptions, TransactionOrchestrator, Value,
};

/// sz-pay 对账单实体
#[derive(Debug, Clone, PartialEq)]
struct ReconciliationOrder {
    id: i64,
    order_no: String,
    amount: i64,
    status: String,
}

impl ReconciliationOrder {
    fn new(id: i64, order_no: &str, amount: i64, status: &str) -> Self {
        Self {
            id,
            order_no: order_no.to_string(),
            amount,
            status: status.to_string(),
        }
    }
}

impl EntityAttributes for ReconciliationOrder {
    fn get_attribute(&self, field: &str) -> Option<Value> {
        match field {
            "id" => Some(Value::I64(self.id)),
            "order_no" => Some(Value::String(self.order_no.clone())),
            "amount" => Some(Value::I64(self.amount)),
            "status" => Some(Value::String(self.status.clone())),
            _ => None,
        }
    }
}

impl sz_orm_core::repository::EntityKey<i64> for ReconciliationOrder {
    fn key(&self) -> i64 {
        self.id
    }
}

impl SoftDeletable for ReconciliationOrder {}

/// T7.1 演示 BatchExecutor 批量插入（对应 sz-pay 批量对账场景）
fn demo_batch_executor() {
    let mut mock = MockConnection::new();
    mock.expect_any().with_rows_affected(1);

    let rows: Vec<HashMap<String, Value>> = vec![
        HashMap::from([
            ("order_no".to_string(), Value::String("PAY001".into())),
            ("amount".to_string(), Value::I64(10000)),
        ]),
        HashMap::from([
            ("order_no".to_string(), Value::String("PAY002".into())),
            ("amount".to_string(), Value::I64(20000)),
        ]),
    ];

    let mut executor = sz_orm_sqlx::BatchExecutor::new(&mut mock, DbType::MySQL);
    let rt = tokio::runtime::Runtime::new().unwrap();
    let count = rt
        .block_on(executor.batch_insert("reconciliation_orders", &rows))
        .unwrap();
    println!(
        "[T7.1] batch_insert {} rows, affected={}",
        rows.len(),
        count
    );
}

/// T7.2 演示 SlowQueryGuard 慢查询捕获（对应 sz-pay 慢查询定位场景）
#[cfg(feature = "slow-query-capture")]
fn demo_slow_query_guard() {
    let mut mock = MockConnection::new();
    mock.expect_any().with_rows_affected(0);

    let mut guard = sz_orm_sqlx::SlowQueryGuard::new(mock)
        .with_slow_threshold(std::time::Duration::from_millis(100));
    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on(guard.execute_with_logging(
        "SELECT * FROM orders WHERE status=?",
        &[Value::String("pending".into())],
    ));
    println!("[T7.2] slow_query_guard result={:?}", result);
}

/// T7.3 演示 AutoHealer 连接池自愈配置（对应 sz-pay db.rs 连接池自愈场景）
#[cfg(feature = "auto-healer")]
fn demo_auto_healer() {
    let config = sz_orm_core::HealerConfig {
        heal_interval: std::time::Duration::from_secs(30),
        probe_timeout: std::time::Duration::from_millis(100),
        probe_sql: "SELECT 1".to_string(),
        conservative_mode: true,
        min_idle: 2,
    };
    println!(
        "[T7.3] AutoHealer config: heal_interval={:?} min_idle={}",
        config.heal_interval, config.min_idle
    );
}

/// T7.4 演示 TransactionOrchestrator 多步骤事务编排（对应 sz-pay 支付编排场景）
fn demo_transaction_orchestrator() {
    let mut mock = MockConnection::new();
    mock.expect_any().with_rows_affected(1);
    let conn: Box<dyn Connection> = Box::new(mock);

    let step1: StepFn = Box::new(|tx, _ctx| {
        Box::pin(async move {
            tx.execute("INSERT INTO orders VALUES (1, 'PAY001', 10000)")
                .await?;
            Ok(())
        })
    });
    let step2: StepFn = Box::new(|tx, _ctx| {
        Box::pin(async move {
            tx.execute("UPDATE accounts SET balance = balance - 10000 WHERE id = 1")
                .await?;
            Ok(())
        })
    });

    let config = OrchestratorConfig {
        rollback_strategy: RollbackStrategy::FullRollback,
    };
    let orch = TransactionOrchestrator::new()
        .with_config(config)
        .step("create_order", step1)
        .step("deduct_balance", step2);

    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on(orch.run(conn, TransactOptions::default()));
    match result {
        OrchestrationResult::Success(ctx) => {
            println!("[T7.4] orchestrator success, ctx keys={}", ctx.len());
        }
        OrchestrationResult::Failure {
            failed_step,
            reason,
            compensated,
        } => {
            println!(
                "[T7.4] orchestrator failure at '{}': {} (compensated={})",
                failed_step, reason, compensated
            );
        }
    }
}

/// T7.5 演示 Repository 增强方法（对应 sz-pay repository 层标准化场景）
fn demo_repository_enhanced() {
    let repo = InMemoryRepository::<ReconciliationOrder>::new();

    let orders = vec![
        ReconciliationOrder::new(1, "PAY001", 10000, "paid"),
        ReconciliationOrder::new(2, "PAY002", 20000, "pending"),
        ReconciliationOrder::new(3, "PAY003", 30000, "paid"),
    ];
    let count = repo.batch_create(orders).unwrap();
    println!("[T7.5] batch_create count={}", count);

    let page = repo.paginate_sql(1, 2).unwrap();
    println!(
        "[T7.5] paginate_sql page=1 size=2: items={}, total={}, pages={}",
        page.items.len(),
        page.total,
        page.total_pages()
    );

    let paid_orders = repo
        .query()
        .where_eq("status", Value::String("paid".into()))
        .find_all()
        .unwrap();
    println!(
        "[T7.5] query().where_eq('status','paid').find_all() = {} orders",
        paid_orders.len()
    );

    let q = repo
        .query()
        .with_relation("items")
        .where_eq("id", Value::I64(1));
    println!(
        "[T7.5] with_relation('items') relations={:?}",
        q.relations()
    );
}

fn main() {
    println!("=== v9.5.0 sz-pay 接线示例 ===\n");

    demo_batch_executor();

    #[cfg(feature = "slow-query-capture")]
    demo_slow_query_guard();

    #[cfg(feature = "auto-healer")]
    demo_auto_healer();

    demo_transaction_orchestrator();

    demo_repository_enhanced();

    println!("\n=== 接线示例完成 ===");
}
