//! 实时流处理示例
//!
//! 演示物化视图 + 窗口分配 + 背压 + 流批一体作业 + Flink 适配。

use std::time::Duration;

use sz_orm_stream::*;

fn main() {
    let engine = ViewRefreshEngine::new();
    let def = MaterializedViewDef::new(
        "user_stats",
        "public.users",
        "SELECT count(*), avg(age) FROM users",
    );
    let view_id = engine.define_view(def).unwrap();
    println!("定义物化视图: {}", view_id);

    let stats = engine.start_incremental_refresh(&view_id).unwrap();
    println!("增量刷新: 耗时={:?}", stats.elapsed);

    let result = engine.query_view(&view_id).unwrap();
    println!("查询结果: {}", result);

    let assigner = WindowAssigner::new(WindowConfig::tumbling(Duration::from_secs(60)));
    let event = Event::new(120_000, serde_json::json!({"user": "alice"}));
    let windows = assigner.assign(&event);
    println!(
        "窗口分配: {} 个窗口 [{}, {})",
        windows.len(),
        windows[0].start_ms,
        windows[0].end_ms
    );

    let strategy = BackpressureStrategy::DropOldest;
    let remaining = strategy.apply(150, 100);
    println!(
        "背压处理: 积压150 阈值100 策略={:?} 剩余={}",
        strategy, remaining
    );

    let mut job = StreamBatchJob::new("etl_job", JobMode::StreamBatchUnified);
    job.dag.add_node("source", vec![]);
    job.dag.add_node("transform", vec!["source".to_string()]);
    job.dag.add_node("sink", vec!["transform".to_string()]);
    let order = job.execution_order();
    println!("作业 DAG 拓扑序: {:?}", order);
    let handle = job.submit_stream().unwrap();
    println!("流模式提交: {} {:?}", handle.job_id, handle.mode);
    job.save_watermark(JobWatermark {
        source: "cdc".into(),
        position: 1000,
    });
    let wm = job.load_watermark("cdc").unwrap();
    println!("水位点: {} -> {}", wm.source, wm.position);

    let flink = FlinkClient::new("http://localhost:8081");
    println!(
        "Flink 端点: {} 降级={}",
        flink.endpoint(),
        flink.is_degraded()
    );
}
