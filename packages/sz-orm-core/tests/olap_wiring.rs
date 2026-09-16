//! OLAP 分析查询接线测试（`olap-vectorized` feature）
//!
//! 验证 `OlapQueryGateway` 端到端编排：
//! 资源检查 → 工作负载路由 → 聚合下推 → Star Schema 优化 → EXPLAIN 注解。
//!
//! 生产调用点证据：
//! - `OlapQueryGateway::query` → `packages/sz-orm-core/src/olap/gateway.rs:118`
//! - `OlapQueryGateway::explain` → `packages/sz-orm-core/src/olap/gateway.rs:107`

use std::time::Duration;

use sz_orm_core::olap::{
    AggregateColumn, AggregateFunc, AggregatePushdown, DimensionTable, MaterializedView,
    MaterializedViewMatcher, OlapConfig, OlapQueryGateway, ResourceLimit, StarSchemaOptimizer,
    WorkloadRouter, WorkloadType,
};

fn gateway() -> OlapQueryGateway {
    let router = WorkloadRouter::new("primary", vec!["olap_replica".into()]);
    OlapQueryGateway::with_defaults(router)
}

/// W1: OLAP 查询路由到读副本
#[test]
fn wiring_olap_routes_to_replica() {
    let mut gw = gateway();
    let result = gw.query(
        "SELECT dept, SUM(amount) FROM sales GROUP BY dept",
        1000,
        800,
        Duration::from_millis(50),
    );
    assert!(result.is_ok());
    let r = result.unwrap();
    assert_eq!(r.route.workload, WorkloadType::Olap);
    assert_eq!(r.route.replica, "olap_replica");
}

/// W2: OLTP 查询路由到主库
#[test]
fn wiring_oltp_routes_to_primary() {
    let mut gw = gateway();
    let result = gw.query(
        "SELECT * FROM users WHERE id = ?",
        1,
        1,
        Duration::from_millis(1),
    );
    assert!(result.is_ok());
    let r = result.unwrap();
    assert_eq!(r.route.workload, WorkloadType::Oltp);
    assert_eq!(r.route.replica, "primary");
}

/// W3: 资源超限拒绝执行
#[test]
fn wiring_resource_limit_rejects() {
    let mut gw = gateway();
    let result = gw.query(
        "SELECT * FROM huge_table",
        20_000_000,
        0,
        Duration::from_secs(0),
    );
    assert!(result.is_err());
}

/// W4: 聚合下推分析
#[test]
fn wiring_aggregate_pushdown() {
    let ap = AggregatePushdown::new();
    let result = ap.analyze(
        &[AggregateColumn {
            func: AggregateFunc::Sum,
            column: "amount".into(),
            alias: "total".into(),
        }],
        &["dept".into()],
        false,
        false,
        false,
    );
    assert!(result.can_pushdown);
    let sql = ap.generate_pushdown_sql("sales", &result).unwrap();
    assert!(sql.contains("SUM(amount) AS total"));
    assert!(sql.contains("GROUP BY dept"));
}

/// W5: Star Schema 优化
#[test]
fn wiring_star_schema_optimization() {
    let gw = gateway();
    let sql = gw.optimize_star(
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
    assert!(sql.contains("JOIN dept"));
    assert!(sql.contains("JOIN time"));
    let dept_pos = sql.find("JOIN dept").unwrap();
    let time_pos = sql.find("JOIN time").unwrap();
    assert!(dept_pos < time_pos, "filtered dimension first");
}

/// W6: 物化视图匹配
#[test]
fn wiring_materialized_view_match() {
    let mut matcher = MaterializedViewMatcher::with_defaults();
    matcher.register(MaterializedView {
        name: "mv_sales_dept".into(),
        sql: "SELECT dept, SUM(amount) FROM sales GROUP BY dept".into(),
        base_table: "sales".into(),
        aggregate_columns: vec!["SUM(amount)".into()],
        group_by_columns: vec!["dept".into()],
        created_at: std::time::Instant::now(),
    });
    let result = matcher.match_view("sales", &["dept".into()], &["SUM(amount)".into()]);
    assert!(result.view.is_some());
    assert_eq!(result.view.unwrap().name, "mv_sales_dept");
}

/// W7: EXPLAIN 包含 OLAP 注解
#[test]
fn wiring_explain_annotations() {
    let mut gw = gateway();
    let explain = gw.explain("SELECT SUM(x) FROM t GROUP BY y", None);
    assert!(explain.contains("vectorized"));
    assert!(explain.contains("OLAP Annotations"));
}

/// W8: Star Schema 识别
#[test]
fn wiring_star_schema_identify() {
    let opt = StarSchemaOptimizer::new();
    let schema = opt.identify(
        "sales",
        vec![
            DimensionTable {
                table: "dept".into(),
                join_on: "1".into(),
                filter: None,
            },
            DimensionTable {
                table: "region".into(),
                join_on: "2".into(),
                filter: None,
            },
        ],
    );
    assert!(schema.is_star);
    assert_eq!(schema.dimensions.len(), 2);
}
