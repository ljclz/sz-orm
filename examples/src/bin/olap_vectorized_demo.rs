//! OLAP 向量化分析查询 demo（v7.1.0）
//!
//! 演示 `OlapQueryGateway` + `VectorizedExecutor` 完整流程：
//! 1. 列存数据构建
//! 2. 向量化聚合执行
//! 3. 聚合下推
//! 4. Star Schema 优化
//! 5. 物化视图匹配
//! 6. 资源限制
//! 7. EXPLAIN 注解
//! 8. OLAP/OLTP 路由
//!
//! 运行：`cargo run --bin olap_vectorized_demo --features olap-vectorized`

use std::time::Duration;

use sz_orm_core::olap::{
    AggregateColumn, AggregateFunc, AggregatePushdown, DimensionTable, MaterializedView,
    MaterializedViewMatcher, OlapConfig, OlapQueryGateway, ResourceLimit, WorkloadRouter,
    WorkloadType,
};
use sz_orm_parallel::{ColumnarBatch, VectorizedExecutor, VectorizedOp};

fn main() {
    println!("=== sz-orm v7.1.0 OLAP 向量化分析查询 demo ===\n");

    println!("[1] 构造列存数据（sales 表）");
    let mut batch = ColumnarBatch::new(vec!["dept_id".into(), "amount".into(), "qty".into()]);
    batch.push_row(&[1.0, 100.0, 10.0]);
    batch.push_row(&[1.0, 200.0, 20.0]);
    batch.push_row(&[2.0, 300.0, 30.0]);
    batch.push_row(&[2.0, 400.0, 40.0]);
    batch.push_row(&[3.0, 500.0, 50.0]);
    println!(
        "    行数: {}, 列数: {}",
        batch.row_count,
        batch.columns.len()
    );

    println!("\n[2] 向量化聚合执行");
    let exec = VectorizedExecutor::new();
    let sum = exec.execute(
        &VectorizedOp::Sum {
            column: "amount".into(),
        },
        &batch,
    );
    println!("    SUM(amount) = {}", sum.values[0]);
    let count = exec.execute(&VectorizedOp::Count, &batch);
    println!("    COUNT(*) = {}", count.values[0]);
    let min = exec.execute(
        &VectorizedOp::Min {
            column: "amount".into(),
        },
        &batch,
    );
    println!("    MIN(amount) = {}", min.values[0]);
    let max = exec.execute(
        &VectorizedOp::Max {
            column: "amount".into(),
        },
        &batch,
    );
    println!("    MAX(amount) = {}", max.values[0]);

    println!("\n[3] 聚合下推分析");
    let ap = AggregatePushdown::new();
    let result = ap.analyze(
        &[AggregateColumn {
            func: AggregateFunc::Sum,
            column: "amount".into(),
            alias: "total".into(),
        }],
        &["dept_id".into()],
        false,
        false,
        false,
    );
    println!("    可下推: {}", result.can_pushdown);
    if let Some(sql) = ap.generate_pushdown_sql("sales", &result) {
        println!("    下推 SQL: {}", sql);
    }

    println!("\n[4] Star Schema 优化");
    let router = WorkloadRouter::new("primary", vec!["olap_replica".into()]);
    let mut gw = OlapQueryGateway::with_defaults(router);
    let opt_sql = gw.optimize_star(
        "sales",
        vec![
            DimensionTable {
                table: "dept".into(),
                join_on: "sales.dept_id = dept.id".into(),
                filter: Some("dept.region = 'East'".into()),
            },
            DimensionTable {
                table: "time".into(),
                join_on: "sales.time_id = time.id".into(),
                filter: None,
            },
        ],
    );
    println!("    优化后: {}", opt_sql);

    println!("\n[5] 物化视图匹配");
    let mut mv_matcher = MaterializedViewMatcher::with_defaults();
    mv_matcher.register(MaterializedView {
        name: "mv_sales_dept".into(),
        sql: "SELECT dept, SUM(amount) FROM sales GROUP BY dept".into(),
        base_table: "sales".into(),
        aggregate_columns: vec!["SUM(amount)".into()],
        group_by_columns: vec!["dept".into()],
        created_at: std::time::Instant::now(),
    });
    let mv_result = mv_matcher.match_view("sales", &["dept".into()], &["SUM(amount)".into()]);
    if let Some(view) = &mv_result.view {
        println!("    匹配 MV: {}", view.name);
    }

    println!("\n[6] 资源限制检查");
    let result = gw.query(
        "SELECT dept, SUM(amount) FROM sales GROUP BY dept",
        1000,
        800,
        Duration::from_millis(50),
    );
    match &result {
        Ok(r) => println!("    扫描 {} 行, 耗时 {:?}", r.scan_rows, r.elapsed),
        Err(e) => println!("    拒绝: {:?}", e),
    }
    let rejected = gw.query(
        "SELECT * FROM huge_table",
        20_000_000,
        0,
        Duration::from_secs(0),
    );
    println!(
        "    超限查询: {}",
        if rejected.is_err() {
            "拒绝"
        } else {
            "通过"
        }
    );

    println!("\n[7] EXPLAIN 注解");
    let explain = gw.explain("SELECT SUM(amount) FROM sales GROUP BY dept", None);
    println!("{}", explain);

    println!("\n[8] OLAP/OLTP 路由");
    let olap_target = gw.classify_and_route("SELECT dept, SUM(amount) FROM sales GROUP BY dept");
    println!(
        "    OLAP 查询 → {} ({})",
        olap_target.replica,
        olap_target.workload.as_str()
    );
    let oltp_target = gw.classify_and_route("SELECT * FROM users WHERE id = ?");
    println!(
        "    OLTP 查询 → {} ({})",
        oltp_target.replica,
        oltp_target.workload.as_str()
    );

    println!("\n=== demo 完成 ===");
}
