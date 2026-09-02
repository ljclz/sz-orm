//! NL 查询管线性能基准测试
//!
//! 验证关键路径延迟：
//! - NL→SQL 规则转换：< 1ms
//! - 端到端查询（无执行器）：< 10ms

use criterion::{criterion_group, criterion_main, Criterion};
use sz_orm_nl_query::pipeline::NlQueryPipeline;

async fn run_query(pipeline: &NlQueryPipeline, nl: &str) {
    let _ = pipeline.query(nl).await;
}

fn bench_nl2sql_rule_based(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let pipeline = NlQueryPipeline::new();

    c.bench_function("nl2sql_rule_based", |b| {
        b.iter(|| {
            rt.block_on(run_query(&pipeline, "查询所有用户"));
        });
    });

    c.bench_function("nl2sql_rule_based_complex", |b| {
        b.iter(|| {
            rt.block_on(run_query(&pipeline, "查询支付记录中退款金额大于1000的商户"));
        });
    });
}

fn bench_pipeline_e2e(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let pipeline = NlQueryPipeline::new();

    c.bench_function("pipeline_e2e_no_executor", |b| {
        b.iter(|| {
            rt.block_on(run_query(&pipeline, "查询所有订单"));
        });
    });
}

criterion_group!(benches, bench_nl2sql_rule_based, bench_pipeline_e2e);
criterion_main!(benches);