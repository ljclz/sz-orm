//! 查询指标接线端到端测试（2026-10-28 架构评测 P1-3）
//!
//! 验证链路：`install_query_metrics()` 安装 core 钩子 → 工厂创建的 MetricsConn
//! 在语句执行后上报 → 全局 `AlertMetricHandles` 的
//! `sz_orm_ops_total` / `sz_orm_latency_us` / `sz_orm_errors_total` 产生真实数据。
//!
//! 运行：`cargo test -p sz-orm-sqlx --test metrics_wiring_test`

use std::sync::Arc;

use sz_orm_core::ConnectionFactory;
use sz_orm_sqlx::{SqlitePoolHandle, SqlxSqliteConnectionFactory};

/// 从全局 registry 的渲染输出中提取 `name value` 行的值
fn rendered_counter_value(metrics_text: &str, name: &str) -> Option<f64> {
    for line in metrics_text.lines() {
        if let Some(rest) = line.strip_prefix(name) {
            if let Some(v) = rest.split_whitespace().next() {
                if let Ok(n) = v.parse::<f64>() {
                    return Some(n);
                }
            }
        }
    }
    None
}

#[tokio::test]
async fn query_metrics_flow_to_global_handles() {
    // 安装全局告警指标 + 查询路径钩子（幂等）
    sz_orm_observability::alert_metrics::install_global_alert_metrics();
    let installed = sz_orm_observability::alert_metrics::install_query_metrics();
    // 若其他测试已抢先安装钩子，installed == false 也是合法状态
    let _ = installed;
    assert!(sz_orm_core::metrics_hook::query_metrics_hook_installed());

    let ops_before = sz_orm_observability::alert_metrics::global_alert_handles()
        .map(|h| h.ops_total.value())
        .unwrap_or(0.0);

    // 通过工厂创建连接（内部为 MetricsConn 包装）并执行语句
    let pool = SqlitePoolHandle::connect("sqlite::memory:")
        .await
        .expect("sqlite pool");
    let factory = SqlxSqliteConnectionFactory::new(Arc::new(pool));
    let mut conn = factory.create().await.expect("create conn");

    conn.execute("CREATE TABLE t_mw (id INTEGER PRIMARY KEY, name TEXT)")
        .await
        .expect("create table");
    conn.execute("INSERT INTO t_mw (name) VALUES ('a'), ('b')")
        .await
        .expect("insert");
    let rows = conn
        .query("SELECT id, name FROM t_mw ORDER BY id")
        .await
        .expect("select");
    assert_eq!(rows.len(), 2);
    // 失败语句也必须计入 errors
    let _ = conn.query("SELECT * FROM no_such_table_mw").await;

    let metrics = sz_orm_observability::alert_metrics::global_registry()
        .expect("global registry installed")
        .render();

    let ops_after =
        rendered_counter_value(&metrics, "sz_orm_ops_total").expect("sz_orm_ops_total rendered");
    assert!(
        ops_after >= ops_before + 4.0,
        "4 条语句（3 成功 + 1 失败）都应计入 ops_total: before={ops_before}, after={ops_after}"
    );

    let errors = rendered_counter_value(&metrics, "sz_orm_errors_total")
        .expect("sz_orm_errors_total rendered");
    assert!(
        errors >= 1.0,
        "失败查询应计入 errors_total（实际 {errors}）"
    );

    // 延迟直方图产生观测样本（_count 渲染行）
    let latency_count = rendered_counter_value(&metrics, "sz_orm_latency_us_count")
        .expect("sz_orm_latency_us_count rendered");
    assert!(
        latency_count >= 4.0,
        "4 条语句都应产生延迟观测样本（实际 {latency_count}）"
    );
}
