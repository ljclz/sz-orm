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
use benchmark_reporter::{
    BenchmarkRecord, BenchmarkReporter, CriterionConfig, EnvironmentMetadata,
};

use criterion::Criterion;

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

/// Criterion estimates.json 结构（mean/median 点估计，单位 ns）
#[derive(serde::Deserialize)]
struct CriterionEstimates {
    mean: CriterionPoint,
    median: CriterionPoint,
}

#[derive(serde::Deserialize)]
struct CriterionPoint {
    point_estimate: f64,
}

/// Criterion samples.json 结构（原始采样）
#[derive(serde::Deserialize)]
struct CriterionSamples {
    iters: Vec<f64>,
    times: Vec<f64>,
}

impl CriterionSamples {
    /// 每次迭代耗时（ns）的 P95
    fn p95_per_iter_ns(&self) -> Option<f64> {
        let mut per_iter: Vec<f64> = self
            .iters
            .iter()
            .zip(self.times.iter())
            .filter(|(iters, _)| **iters > 0.0)
            .map(|(iters, times)| times / iters)
            .collect();
        if per_iter.is_empty() {
            return None;
        }
        per_iter.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let rank = ((per_iter.len() - 1) as f64 * 0.95).round() as usize;
        Some(per_iter[rank])
    }
}

/// v10.13.0 ROI-5：从 Criterion 落盘的真实测量数据生成基准报告。
///
/// 原 ReportGuard 用空 records 生成空报告且因 `Box::leak` 从未执行（双重幻影）。
/// 现改为在全部 bench 完成后解析 `target/criterion/<id>/new/{estimates,samples}.json`
/// ——数据 100% 来自本次运行的真实测量，无测量数据时明确跳过并告警。
fn generate_report_from_criterion(dialects: &[String]) {
    let root = std::path::Path::new("target/criterion");
    let Ok(entries) = std::fs::read_dir(root) else {
        eprintln!("⚠ 未找到 target/criterion 测量数据，报告生成跳过");
        return;
    };

    let mut records: Vec<BenchmarkRecord> = Vec::new();
    for entry in entries.flatten() {
        let id = entry.file_name().to_string_lossy().to_string();
        // bench id 格式：dimension/competitor/dialect/size
        let parts: Vec<&str> = id.split('/').collect();
        if parts.len() < 4 {
            continue;
        }
        let estimates_path = entry.path().join("new").join("estimates.json");
        let samples_path = entry.path().join("new").join("samples.json");
        let Ok(est_raw) = std::fs::read_to_string(&estimates_path) else {
            continue;
        };
        let Ok(estimates) = serde_json::from_str::<CriterionEstimates>(&est_raw) else {
            continue;
        };
        let p95_ns = std::fs::read_to_string(&samples_path)
            .ok()
            .and_then(|raw| serde_json::from_str::<CriterionSamples>(&raw).ok())
            .and_then(|s| s.p95_per_iter_ns())
            .unwrap_or(estimates.median.point_estimate);

        records.push(BenchmarkRecord {
            dimension: parts[0].to_string(),
            dialect: parts[2].to_string(),
            competitor: parts[1].to_string(),
            mean_ns: estimates.mean.point_estimate,
            median_ns: estimates.median.point_estimate,
            p95_ns,
            throughput_ops_per_sec: 1e9 / estimates.mean.point_estimate.max(1.0),
            dataset_size: parts[3].parse().unwrap_or(0),
        });
    }

    if records.is_empty() {
        eprintln!("⚠ Criterion 数据为空，跳过基准报告生成");
        return;
    }
    generate_report_files(&records, dialects);
    eprintln!("基准报告已生成（{} 条真实测量记录，方言: {dialects:?}）", records.len());
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

    for dsn_var in &[
        "DATABASE_URL_MYSQL",
        "DATABASE_URL_POSTGRES",
        "DATABASE_URL_ORACLE",
        "DATABASE_URL_MSSQL",
    ] {
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
            if rt.block_on(adapter.setup(size)).is_err() {
                continue;
            }
            let name = format!(
                "crud_single/{}/{}/{}",
                adapter.name(),
                active_dialect(),
                size
            );
            c.bench_function(&name, |b| {
                b.iter_custom(|iters| {
                    let start = std::time::Instant::now();
                    rt.block_on(async {
                        for i in 0..iters as i64 {
                            std::hint::black_box(
                                adapter.insert_one(&BenchRecord::new(i + 1)).await,
                            );
                        }
                    });
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
            if rt.block_on(adapter.setup(size)).is_err() {
                continue;
            }
            let name = format!("crud_find/{}/{}/{}", adapter.name(), active_dialect(), size);
            c.bench_function(&name, |b| {
                b.iter_custom(|iters| {
                    let start = std::time::Instant::now();
                    rt.block_on(async {
                        for i in 0..iters as i64 {
                            std::hint::black_box(adapter.find_one((i % size as i64) + 1).await);
                        }
                    });
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
            if rt.block_on(adapter.setup(size)).is_err() {
                continue;
            }
            let name = format!(
                "crud_batch/{}/{}/{}",
                adapter.name(),
                active_dialect(),
                size
            );
            let records: Vec<BenchRecord> = (1..=100).map(BenchRecord::new).collect();
            c.bench_function(&name, |b| {
                b.iter_custom(|iters| {
                    let start = std::time::Instant::now();
                    rt.block_on(async {
                        for _ in 0..iters {
                            std::hint::black_box(adapter.insert_batch(&records).await);
                        }
                    });
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
            if rt.block_on(adapter.setup(size)).is_err() {
                continue;
            }
            let name = format!(
                "relation_has_one/{}/{}/{}",
                adapter.name(),
                active_dialect(),
                size
            );
            c.bench_function(&name, |b| {
                b.iter_custom(|iters| {
                    let start = std::time::Instant::now();
                    rt.block_on(async {
                        for i in 0..iters as i64 {
                            std::hint::black_box(
                                adapter.find_with_has_one((i % size as i64) + 1).await,
                            );
                        }
                    });
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
            if rt.block_on(adapter.setup(size)).is_err() {
                continue;
            }
            let name = format!(
                "relation_has_many/{}/{}/{}",
                adapter.name(),
                active_dialect(),
                size
            );
            c.bench_function(&name, |b| {
                b.iter_custom(|iters| {
                    let start = std::time::Instant::now();
                    rt.block_on(async {
                        for i in 0..iters as i64 {
                            std::hint::black_box(
                                adapter.find_with_has_many((i % size as i64) + 1).await,
                            );
                        }
                    });
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
            if rt.block_on(adapter.setup(size)).is_err() {
                continue;
            }
            let name = format!(
                "relation_m2m/{}/{}/{}",
                adapter.name(),
                active_dialect(),
                size
            );
            c.bench_function(&name, |b| {
                b.iter_custom(|iters| {
                    let start = std::time::Instant::now();
                    rt.block_on(async {
                        for _ in 0..iters {
                            std::hint::black_box(adapter.find_with_many_to_many(1).await);
                        }
                    });
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
            if rt.block_on(adapter.setup(size)).is_err() {
                continue;
            }
            let name = format!(
                "transaction/{}/{}/{}",
                adapter.name(),
                active_dialect(),
                size
            );
            c.bench_function(&name, |b| {
                b.iter_custom(|iters| {
                    let start = std::time::Instant::now();
                    rt.block_on(async {
                        for _ in 0..iters {
                            std::hint::black_box(adapter.transaction_commit().await);
                        }
                    });
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
            if rt.block_on(adapter.setup(size)).is_err() {
                continue;
            }
            let name = format!("pool/{}/{}/{}", adapter.name(), active_dialect(), size);
            c.bench_function(&name, |b| {
                b.iter_custom(|iters| {
                    let start = std::time::Instant::now();
                    rt.block_on(async {
                        for _ in 0..iters {
                            std::hint::black_box(adapter.pool_acquire().await);
                        }
                    });
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
            if rt.block_on(adapter.setup(size)).is_err() {
                continue;
            }
            let name = format!(
                "pagination/{}/{}/{}",
                adapter.name(),
                active_dialect(),
                size
            );
            c.bench_function(&name, |b| {
                b.iter_custom(|iters| {
                    let start = std::time::Instant::now();
                    rt.block_on(async {
                        for i in 0..iters as i64 {
                            std::hint::black_box(
                                adapter.paginate_offset(((i as usize) % size) / 2, 20).await,
                            );
                        }
                    });
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
    Criterion::default()
        .sample_size(100)
        .warm_up_time(std::time::Duration::from_secs(3))
        .measurement_time(std::time::Duration::from_secs(10))
        .confidence_level(0.95)
        .noise_threshold(0.05)
}

fn main() {
    let mut c = configure_criterion();
    bench_crud_single(&mut c);
    bench_crud_find(&mut c);
    bench_crud_batch(&mut c);
    bench_relation_has_one(&mut c);
    bench_relation_has_many(&mut c);
    bench_relation_m2m(&mut c);
    bench_transaction(&mut c);
    bench_pool(&mut c);
    bench_pagination(&mut c);
    c.final_summary();
    // 全部 bench 完成后，Criterion 已将真实测量写入 target/criterion，
    // 此时基于落盘数据生成报告（v10.13.0 ROI-5）
    generate_report_from_criterion(&detect_dialects());
}
