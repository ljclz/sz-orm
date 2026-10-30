//! 查询路径指标接入示例（v10.13.0，2026-10-28）
//!
//! 演示如何把 `AlertMetricHandles` 接入查询路径：
//! 1. 创建 `MetricsRegistry` 并注册告警/仪表盘指标；
//! 2. 用 `QueryPathMetrics` 封装延迟/操作/错误写入；
//! 3. 在真实查询循环中记录指标（本示例以 `std::thread::sleep` 模拟查询耗时；
//!    实际使用时替换为真实 DB 查询并传入 `QueryPathMetrics::start()`）；
//! 4. 渲染 Prometheus 文本格式。
//!
//! 运行：`cargo run -p sz-orm-observability --example query_path_metrics`

use std::time::Duration;

use sz_orm_observability::{register_alert_metrics, MetricsRegistry, QueryPathMetrics};

fn main() {
    // 1. 创建注册表并注册告警/仪表盘指标
    let registry = MetricsRegistry::new();
    let handles = register_alert_metrics(&registry);
    let metrics = QueryPathMetrics::new(handles);

    println!("=== sz-orm 查询路径指标接入示例 ===");
    println!("注册告警指标后，执行 5 次查询（2 次失败）……\n");

    // 2. 模拟查询循环（实际使用中替换为真实 DB 查询）
    for i in 0..5 {
        let start = QueryPathMetrics::start();
        std::thread::sleep(Duration::from_micros((i + 1) * 200));
        // 模拟第 2、4 次查询失败
        let result = if i % 2 == 1 { Err(()) } else { Ok(()) };
        metrics.observe_query(start, result);
    }

    // 3. 渲染 Prometheus 文本格式
    let text = registry.render();
    println!("--- /metrics 输出（节选）---");
    for line in text.lines().filter(|l| {
        l.contains("sz_orm_ops_total")
            || l.contains("sz_orm_errors_total")
            || l.contains("sz_orm_latency_us_bucket{le=\"100")
    }) {
        println!("{line}");
    }

    println!("\n=== 完成 ===");
    println!(
        "说明: sz_orm_ops_total 应为 5，sz_orm_errors_total 应为 2；延迟直方图记录 5 次采样。"
    );
    println!("生产接入: 将 sleep 替换为真实查询，并把 registry 交给 start_metrics_server 暴露 HTTP /metrics。");
}
