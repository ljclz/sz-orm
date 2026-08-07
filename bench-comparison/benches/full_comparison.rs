//! full_comparison — 全维度 × 多方言 × 竞品基准主入口（v2.3.0 任务 B）
//!
//! 聚合 5 个维度（CRUD/关联/事务/连接池/分页）× 4 档规模 × 4 竞品。
//!
//! # 运行方式
//!
//! ```bash
//! # SQLite only（默认）
//! cargo bench --bench full_comparison
//!
//! # MySQL + PostgreSQL + SQLite
//! export DATABASE_URL_MYSQL=mysql://root:***@127.0.0.1:3306/bench
//! export DATABASE_URL_POSTGRES=postgres://postgres:***@127.0.0.1:5432/bench
//! cargo bench --bench full_comparison
//!
//! # 单独运行某维度
//! cargo bench --bench bench_crud
//! cargo bench --bench bench_relation
//! cargo bench --bench bench_transaction
//! cargo bench --bench bench_pool
//! cargo bench --bench bench_pagination
//! ```

#[path = "competitor_adapter.rs"]
mod competitor_adapter;
use competitor_adapter::*;

#[path = "benchmark_reporter.rs"]
mod benchmark_reporter;
use benchmark_reporter::{BenchmarkRecord, BenchmarkReporter, CriterionConfig, EnvironmentMetadata};

use criterion::{criterion_group, criterion_main, Criterion};

/// Drop guard：程序退出时生成基准报告文件
struct ReportGuard;
impl Drop for ReportGuard {
    fn drop(&mut self) {
        let dialects = detect_dialects();
        let records: Vec<BenchmarkRecord> = Vec::new();
        generate_report_files(&records, &dialects);
        eprintln!("基准报告已生成（方言: {dialects:?}）");
    }
}

// ============================================================================
// 多方言检测（T-B-009）
// ============================================================================

/// 检测可用的数据库方言（通过环境变量触发）
fn detect_dialects() -> Vec<String> {
    let mut dialects = vec!["sqlite".to_string()];
    if std::env::var("DATABASE_URL_MYSQL").is_ok() {
        dialects.push("mysql".to_string());
    }
    if std::env::var("DATABASE_URL_POSTGRES").is_ok() {
        dialects.push("postgres".to_string());
    }
    if std::env::var("DATABASE_URL_ORACLE").is_ok() {
        dialects.push("oracle".to_string());
    }
    if std::env::var("DATABASE_URL_MSSQL").is_ok() {
        dialects.push("mssql".to_string());
    }
    dialects
}

/// 当前活跃方言（用于 bench 命名）
fn active_dialect() -> &'static str {
    if std::env::var("DATABASE_URL_MYSQL").is_ok() {
        "mysql"
    } else if std::env::var("DATABASE_URL_POSTGRES").is_ok() {
        "postgres"
    } else if std::env::var("DATABASE_URL_ORACLE").is_ok() {
        "oracle"
    } else if std::env::var("DATABASE_URL_MSSQL").is_ok() {
        "mssql"
    } else {
        "sqlite"
    }
}

/// 生成基准报告文件（T-B-009：benchmark-report.md + benchmark-data.csv + benchmark-data.json）
fn generate_report_files(records: &[BenchmarkRecord], dialects: &[String]) {
    let env = EnvironmentMetadata {
        cpu: std::env::var("BENCH_CPU").unwrap_or_else(|_| "unknown".to_string()),
        memory_gb: std::env::var("BENCH_MEMORY_GB")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.0),
        disk: std::env::var("BENCH_DISK").unwrap_or_else(|_| "unknown".to_string()),
        rust_version: env!("CARGO_PKG_RUST_VERSION").to_string(),
        db_versions: dialects.iter().map(|d| format!("{d} (active)")).collect(),
        criterion_config: CriterionConfig::default(),
        dataset_sizes: DATASET_SIZES.to_vec(),
    };
    let mut reporter = BenchmarkReporter::new(env);
    for record in records {
        reporter.add_record(record.clone());
    }

    let report_dir = std::env::var("BENCH_REPORT_DIR").unwrap_or_else(|_| ".".to_string());
    let md_path = format!("{report_dir}/benchmark-report.md");
    let csv_path = format!("{report_dir}/benchmark-data.csv");
    let json_path = format!("{report_dir}/benchmark-data.json");

    let mut md = reporter.generate_markdown();
    md.push_str("\n\n");
    md.push_str(&reporter.generate_repro_instructions());
    std::fs::write(&md_path, md).ok();
    std::fs::write(&csv_path, reporter.generate_csv()).ok();
    std::fs::write(&json_path, reporter.generate_json()).ok();

    for dsn_var in &["DATABASE_URL_MYSQL", "DATABASE_URL_POSTGRES", "DATABASE_URL_ORACLE", "DATABASE_URL_MSSQL"] {
        if let Ok(dsn) = std::env::var(dsn_var) {
            eprintln!("{dsn_var}: {}", BenchmarkReporter::mask_dsn(&dsn));
        }
    }

    let audit = reporter.audit();
    if !audit.is_clean {
        eprintln!("⚠ 基准报告审查发现异常: {:?}", audit);
    }
}

// ============================================================================
// CRUD 维度
// ============================================================================

fn bench_crud_single(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("rt");
    for &size in DATASET_SIZES {
        for mut adapter in create_all_adapters() {
            if rt.block_on(adapter.setup(size)).is_err() { continue; }
            let name = format!("crud_single/{}/{}/{}", adapter.name(), active_dialect(), size);
            c.bench_function(&name, |b| {
                b.iter_custom(|iters| {
                    let start = std::time::Instant::now();
                    rt.block_on(async { for i in 0..iters as i64 { std::hint::black_box(adapter.insert_one(&BenchRecord::new(i + 1)).await); } });
                    start.elapsed()
                });
            });
            let _ = rt.block_on(adapter.teardown());
        }
    }
}

fn bench_crud_find(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("rt");
    for &size in DATASET_SIZES {
        for mut adapter in create_all_adapters() {
            if rt.block_on(adapter.setup(size)).is_err() { continue; }
            let name = format!("crud_find/{}/{}/{}", adapter.name(), active_dialect(), size);
            c.bench_function(&name, |b| {
                b.iter_custom(|iters| {
                    let start = std::time::Instant::now();
                    rt.block_on(async { for i in 0..iters as i64 { std::hint::black_box(adapter.find_one((i % size as i64) + 1).await); } });
                    start.elapsed()
                });
            });
            let _ = rt.block_on(adapter.teardown());
        }
    }
}

fn bench_crud_batch(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("rt");
    for &size in DATASET_SIZES {
        for mut adapter in create_all_adapters() {
            if rt.block_on(adapter.setup(size)).is_err() { continue; }
            let name = format!("crud_batch/{}/{}/{}", adapter.name(), active_dialect(), size);
            let records: Vec<BenchRecord> = (1..=100).map(BenchRecord::new).collect();
            c.bench_function(&name, |b| {
                b.iter_custom(|iters| {
                    let start = std::time::Instant::now();
                    rt.block_on(async { for _ in 0..iters { std::hint::black_box(adapter.insert_batch(&records).await); } });
                    start.elapsed()
                });
            });
            let _ = rt.block_on(adapter.teardown());
        }
    }
}

// ============================================================================
// 关联查询维度
// ============================================================================

fn bench_relation_has_one(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("rt");
    for &size in DATASET_SIZES {
        for mut adapter in create_all_adapters() {
            if rt.block_on(adapter.setup(size)).is_err() { continue; }
            let name = format!("relation_has_one/{}/{}/{}", adapter.name(), active_dialect(), size);
            c.bench_function(&name, |b| {
                b.iter_custom(|iters| {
                    let start = std::time::Instant::now();
                    rt.block_on(async { for i in 0..iters as i64 { std::hint::black_box(adapter.find_with_has_one((i % size as i64) + 1).await); } });
                    start.elapsed()
                });
            });
            let _ = rt.block_on(adapter.teardown());
        }
    }
}

fn bench_relation_has_many(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("rt");
    for &size in DATASET_SIZES {
        for mut adapter in create_all_adapters() {
            if rt.block_on(adapter.setup(size)).is_err() { continue; }
            let name = format!("relation_has_many/{}/{}/{}", adapter.name(), active_dialect(), size);
            c.bench_function(&name, |b| {
                b.iter_custom(|iters| {
                    let start = std::time::Instant::now();
                    rt.block_on(async { for i in 0..iters as i64 { std::hint::black_box(adapter.find_with_has_many((i % size as i64) + 1).await); } });
                    start.elapsed()
                });
            });
            let _ = rt.block_on(adapter.teardown());
        }
    }
}

fn bench_relation_m2m(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("rt");
    for &size in DATASET_SIZES {
        for mut adapter in create_all_adapters() {
            if rt.block_on(adapter.setup(size)).is_err() { continue; }
            let name = format!("relation_m2m/{}/{}/{}", adapter.name(), active_dialect(), size);
            c.bench_function(&name, |b| {
                b.iter_custom(|iters| {
                    let start = std::time::Instant::now();
                    rt.block_on(async { for _ in 0..iters { std::hint::black_box(adapter.find_with_many_to_many(1).await); } });
                    start.elapsed()
                });
            });
            let _ = rt.block_on(adapter.teardown());
        }
    }
}

// ============================================================================
// 事务维度
// ============================================================================

fn bench_transaction(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("rt");
    for &size in DATASET_SIZES {
        for mut adapter in create_all_adapters() {
            if rt.block_on(adapter.setup(size)).is_err() { continue; }
            let name = format!("transaction/{}/{}/{}", adapter.name(), active_dialect(), size);
            c.bench_function(&name, |b| {
                b.iter_custom(|iters| {
                    let start = std::time::Instant::now();
                    rt.block_on(async { for _ in 0..iters { std::hint::black_box(adapter.transaction_commit().await); } });
                    start.elapsed()
                });
            });
            let _ = rt.block_on(adapter.teardown());
        }
    }
}

// ============================================================================
// 连接池维度
// ============================================================================

fn bench_pool(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("rt");
    for &size in DATASET_SIZES {
        for mut adapter in create_all_adapters() {
            if rt.block_on(adapter.setup(size)).is_err() { continue; }
            let name = format!("pool/{}/{}/{}", adapter.name(), active_dialect(), size);
            c.bench_function(&name, |b| {
                b.iter_custom(|iters| {
                    let start = std::time::Instant::now();
                    rt.block_on(async { for _ in 0..iters { std::hint::black_box(adapter.pool_acquire().await); } });
                    start.elapsed()
                });
            });
            let _ = rt.block_on(adapter.teardown());
        }
    }
}

// ============================================================================
// 分页维度
// ============================================================================

fn bench_pagination(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("rt");
    for &size in DATASET_SIZES {
        for mut adapter in create_all_adapters() {
            if rt.block_on(adapter.setup(size)).is_err() { continue; }
            let name = format!("pagination/{}/{}/{}", adapter.name(), active_dialect(), size);
            c.bench_function(&name, |b| {
                b.iter_custom(|iters| {
                    let start = std::time::Instant::now();
                    rt.block_on(async { for i in 0..iters as i64 { std::hint::black_box(adapter.paginate_offset(((i as usize) % size) / 2, 20).await); } });
                    start.elapsed()
                });
            });
            let _ = rt.block_on(adapter.teardown());
        }
    }
}

// ============================================================================
// criterion 配置 + 主入口
// ============================================================================

fn configure_criterion() -> Criterion {
    let dialects = detect_dialects();
    eprintln!("基准运行方言: {dialects:?}");
    let _guard = Box::leak(Box::new(ReportGuard));
    Criterion::default()
        .sample_size(100)
        .warm_up_time(std::time::Duration::from_secs(3))
        .measurement_time(std::time::Duration::from_secs(10))
        .confidence_level(0.95)
        .noise_threshold(0.05)
}

criterion_group! {
    name = benches;
    config = configure_criterion();
    targets =
        bench_crud_single,
        bench_crud_find,
        bench_crud_batch,
        bench_relation_has_one,
        bench_relation_has_many,
        bench_relation_m2m,
        bench_transaction,
        bench_pool,
        bench_pagination
}

criterion_main!(benches);
