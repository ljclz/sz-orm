//! bench_smart_eager — SmartEagerLoader 性能基准（v2.4.0 任务 3.2~3.4）
//!
//! 三类基准：
//! 1. 决策延迟：StrategyResolver::resolve() P99 ≤ 100μs
//! 2. 智能 vs 手动：SmartEagerLoader / EagerLoader 耗时比 ≤ 1.10
//! 3. N+1 消除：批量查询 vs 逐条查询

#[path = "smart_eager_harness.rs"]
mod smart_eager_harness;
use smart_eager_harness::*;

use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};
use sz_orm_core::eager_loader::EagerLoader;
use sz_orm_core::smart_eager_loader::{SmartEagerLoader, StrategyResolver};
use sz_orm_core::n1_eliminator::{N1Eliminator, PendingQuery};
use sz_orm_core::relation_trait::{RelationDef, RelationKind};
use sz_orm_core::{Connection, Value};
use std::time::{Duration, Instant};

/// 任务 3.2：决策延迟基准 — P99 ≤ 100μs
fn bench_decision_latency(c: &mut Criterion) {
    let relations: Vec<RelationDef> = vec![
        RelationDef::new("profile", "users", "profiles", "id", "user_id", RelationKind::HasOne),
        RelationDef::new("orders", "users", "orders", "id", "user_id", RelationKind::HasMany),
        RelationDef::new_many_to_many("roles", "users", "roles", "id", "id", "user_roles", "user_id", "role_id"),
    ];

    let resolver = StrategyResolver::new();

    let mut group = c.benchmark_group("decision_latency");
    group.sample_size(100);
    group.measurement_time(Duration::from_secs(3));

    for (i, rel) in relations.iter().enumerate() {
        group.bench_with_input(BenchmarkId::new("resolve", i), &rel, |b, rel| {
            b.iter_custom(|iters| {
                let start = Instant::now();
                for _ in 0..iters {
                    std::hint::black_box(resolver.resolve(rel));
                }
                start.elapsed()
            });
        });
    }
    group.finish();
}

/// 任务 3.3：智能 vs 手动对比基准 — 退化 ≤ 10%
fn bench_smart_vs_manual(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("rt");
    let relation = RelationDef::new("orders", "users", "orders", "id", "user_id", RelationKind::HasMany);

    let mut group = c.benchmark_group("smart_vs_manual");
    group.sample_size(30);
    group.measurement_time(Duration::from_secs(5));

    for &size in BENCH_SIZES {
        let mut harness = SmartEagerBenchHarness::new();
        harness.setup(size);

        group.bench_with_input(BenchmarkId::new("smart", size), &size, |b, _| {
            b.iter_custom(|iters| {
                let start = Instant::now();
                rt.block_on(async {
                    for _ in 0..iters {
                        let loader = SmartEagerLoader::new(relation.clone());
                        std::hint::black_box(loader.load(harness.conn(), "SELECT * FROM users").await.unwrap());
                    }
                });
                start.elapsed()
            });
        });

        group.bench_with_input(BenchmarkId::new("manual", size), &size, |b, _| {
            b.iter_custom(|iters| {
                let start = Instant::now();
                rt.block_on(async {
                    for _ in 0..iters {
                        let loader = EagerLoader::new(relation.clone());
                        std::hint::black_box(loader.load_many(harness.conn(), "SELECT * FROM users").await.unwrap());
                    }
                });
                start.elapsed()
            });
        });

        harness.teardown(size);
    }
    group.finish();
}

/// 任务 3.4：N+1 消除对比基准
fn bench_n1_elimination(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("rt");
    let relation = RelationDef::new("orders", "users", "orders", "id", "user_id", RelationKind::HasMany);

    let mut group = c.benchmark_group("n1_elimination");
    group.sample_size(30);
    group.measurement_time(Duration::from_secs(5));

    for &size in BENCH_SIZES {
        let mut harness = SmartEagerBenchHarness::new();
        harness.setup(size);

        // N+1 逐条查询
        group.bench_with_input(BenchmarkId::new("n_plus_1", size), &size, |b, _| {
            b.iter_custom(|iters| {
                let start = Instant::now();
                rt.block_on(async {
                    for _ in 0..iters {
                        let users = harness.conn().query("SELECT * FROM users").await.unwrap();
                        for user in &users {
                            let uid = user.get("id").cloned().unwrap_or(sz_orm_core::Value::Null);
                            let _ = harness.conn().query_with_params(
                                "SELECT * FROM orders WHERE user_id = ?",
                                &[uid],
                            ).await.unwrap();
                        }
                        std::hint::black_box(users.len());
                    }
                });
                start.elapsed()
            });
        });

        // 批量查询（2 次）
        group.bench_with_input(BenchmarkId::new("batch", size), &size, |b, _| {
            b.iter_custom(|iters| {
                let start = Instant::now();
                rt.block_on(async {
                    for _ in 0..iters {
                        let loader = SmartEagerLoader::new(relation.clone());
                        std::hint::black_box(loader.load(harness.conn(), "SELECT * FROM users").await.unwrap());
                    }
                });
                start.elapsed()
            });
        });

        harness.teardown(size);
    }
    group.finish();
}

/// 任务 3.4 辅助：验证 N1Eliminator 检测能力
fn bench_n1_detector(c: &mut Criterion) {
    let mut group = c.benchmark_group("n1_detector");
    group.sample_size(100);

    group.bench_function("detect_n1_pattern", |b| {
        b.iter(|| {
            let mut elim = N1Eliminator::with_threshold(5);
            for i in 0..20 {
                elim.record_query(PendingQuery {
                    table: "orders".to_string(),
                    where_column: "user_id".to_string(),
                    where_value: Value::I64(i),
                    select_columns: vec!["*".to_string()],
                    in_standalone_transaction: false,
                    trigger_location: format!("bench:{}", i),
                });
            }
            std::hint::black_box(elim.pending_count());
        });
    });

    group.finish();
}

criterion_group!(benches, bench_decision_latency, bench_smart_vs_manual, bench_n1_elimination, bench_n1_detector);
criterion_main!(benches);