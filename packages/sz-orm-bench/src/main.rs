//! sz-orm-bench 命令行入口 — 运行真实 DB 基准对标并输出 JSON 报告
//!
//! v10.13.0 起（2026-10-27 性能评测 ROI-1/ROI-2 整改）：
//! - 移除模拟延迟模型路径（`--simulate`），无 `real-bench` feature 时直接报错退出（ADR-003 fail-closed）
//! - `--save-baseline` / `--compare-baseline` 落地真实基线门禁（退化 ≥10% 阻断，退出码 1）

use std::env;
use sz_orm_bench::{BenchConfig, FrameworkType, WorkloadType};

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() > 1 && (args[1] == "--help" || args[1] == "-h") {
        print_help();
        return;
    }

    if args.iter().any(|a| a == "--simulate") {
        eprintln!(
            "错误: --simulate（模拟延迟模型）已于 v10.13.0 移除。\
             模拟延迟会制造虚假性能数据（ADR-003 禁止回退模拟延迟）。\
             请使用 --features real-bench 运行真实 DB 基准。"
        );
        std::process::exit(1);
    }

    if args.len() > 1 && args[1] == "simd" {
        #[cfg(feature = "real-bench")]
        {
            let config = parse_config(&args);
            run_simd_comparison_real(&config);
            return;
        }
        #[cfg(not(feature = "real-bench"))]
        {
            eprintln!("错误: simd 命令需要 --features real-bench（真实 DB 数据集实测）");
            std::process::exit(1);
        }
    }

    let config = parse_config(&args);

    #[cfg(feature = "real-bench")]
    {
        run_benchmark_real(&config);
    }
    #[cfg(not(feature = "real-bench"))]
    {
        let _ = config;
        eprintln!(
            "错误: sz-orm-bench 需要 --features real-bench 才能运行真实 DB 基准。\
             \n示例: cargo run -p sz-orm-bench --features real-bench -- \
             --db-backend sqlite --db-connection \"sqlite://bench.sqlite?mode=rwc\""
        );
        std::process::exit(1);
    }
}

/// 生成一次性 SQLite 临时文件连接串（跨连接池共享需文件而非 :memory:）
#[cfg(feature = "real-bench")]
fn sqlite_temp_connection(label: &str) -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let path = std::env::temp_dir().join(format!(
        "sz_orm_bench_{label}_{}_{}.sqlite",
        std::process::id(),
        ts
    ));
    format!(
        "sqlite://{}?mode=rwc",
        path.to_string_lossy().replace('\\', "/")
    )
}

/// 清理 SQLite 临时文件（含 -wal/-shm）
#[cfg(feature = "real-bench")]
fn cleanup_sqlite_connection(conn: &str) {
    let path = conn
        .trim_start_matches("sqlite://")
        .trim_end_matches("?mode=rwc");
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(format!("{path}-wal"));
    let _ = std::fs::remove_file(format!("{path}-shm"));
}

/// 基线保存与对比（v10.13.0 ROI-2：真实回归门禁，fail-closed）
///
/// 2026-10-28 P1-4（PERFV2-01 修复）：**先比对后保存**——
/// CI 恢复上一轮基线后以 `--save-baseline X --compare-baseline X` 同名调用时，
/// 比对必须发生在覆盖保存之前，否则本轮会与刚写入的自身基线比较，
/// 回归门禁永远假绿。缺 workload 亦判失败（见 `BaselineComparison`）。
#[cfg(feature = "real-bench")]
fn handle_baseline(config: &BenchConfig, results: &[sz_orm_bench::BenchResult]) {
    use sz_orm_bench::RegressionBaseline;

    if let Some(name) = &config.compare_baseline {
        let path = std::path::Path::new("bench-results").join(format!("baseline_{name}.json"));
        let baseline = match RegressionBaseline::load_from_file(path.to_str().unwrap_or_default()) {
            Ok(b) => b,
            Err(e) => {
                eprintln!(
                    "❌ 基线 `{name}` 加载失败，fail-closed 阻断（BENCH_BASELINE_MISSING）: {e}"
                );
                eprintln!("   请先运行 --save-baseline {name} 建立基线");
                std::process::exit(1);
            }
        };
        let comparison = baseline.compare(results);
        if comparison.has_regression {
            eprintln!("❌ 检测到性能退化（≥10% 或基线 workload 缺失），阻断合入：");
            for wc in comparison
                .workload_comparisons
                .iter()
                .filter(|c| c.is_regression)
            {
                eprintln!(
                    "  - {:?}: 基线 P95={:.1}µs → 当前 {:.1}µs（退化 {:.1}%）",
                    wc.workload, wc.baseline_p95_us, wc.current_p95_us, wc.degradation_pct
                );
            }
            for w in &comparison.missing_workloads {
                eprintln!("  - {w:?}: 基线存在但当前结果缺失（workload 被删除或静默跳过）");
            }
            std::process::exit(1);
        }
        let max_degradation = comparison
            .workload_comparisons
            .iter()
            .map(|c| c.degradation_pct)
            .fold(0.0_f64, f64::max);
        println!("✅ 基线对比通过：最大退化 {max_degradation:.2}% < 10%，无缺失 workload");
    }

    if let Some(name) = &config.save_baseline {
        let baseline = RegressionBaseline::new(
            name,
            config.clone(),
            results.to_vec(),
            config.db_backend.as_str(),
        );
        match baseline.save_to_file("bench-results") {
            Ok(path) => println!("基线已保存: {}", path.display()),
            Err(e) => {
                eprintln!("❌ 基线保存失败: {e}");
                std::process::exit(1);
            }
        }
    }
}

#[cfg(feature = "real-bench")]
fn run_benchmark_real(config: &BenchConfig) {
    use sz_orm_bench::{report_filename, run_workload_real, BenchReport, DbBackend};

    println!(
        "sz-orm-bench: 真实 DB 基准对标（backend={}, rounds={}）",
        config.db_backend.as_str(),
        config.measure_rounds
    );

    if let Err(e) = config.validate() {
        eprintln!("配置校验失败: {e}");
        std::process::exit(1);
    }

    let conn = sqlite_temp_connection("bench");
    let mut effective_config = config.clone();
    if config.db_connection == "sqlite::memory:" && config.db_backend == DbBackend::Sqlite {
        // :memory: 每连接独立，跨池不可见（DatasetInitializer 与 Workload 是两个池）；
        // 默认 sqlite 后端改用临时文件保证数据集可见
        effective_config.db_connection = conn.clone();
    }

    let rt = tokio::runtime::Runtime::new().expect("创建 tokio runtime 失败");
    let results = rt.block_on(async {
        let frameworks = [
            FrameworkType::SzOrm,
            FrameworkType::SeaOrm,
            FrameworkType::Diesel,
            FrameworkType::Sqlx,
        ];
        let workloads = [
            WorkloadType::SingleRowQuery,
            WorkloadType::BatchQuery,
            WorkloadType::ComplexJoin,
            WorkloadType::Transaction,
            WorkloadType::PoolConcurrency,
            WorkloadType::SimdCompare,
        ];

        let mut results = Vec::new();
        for fw in &frameworks {
            for wl in &workloads {
                match run_workload_real(*fw, *wl, &effective_config).await {
                    Ok(r) => {
                        println!(
                            "  {} / {} → P50={:.0}μs P95={:.0}μs P99={:.0}μs",
                            fw.as_str(),
                            wl.as_str(),
                            r.p50_us,
                            r.p95_us,
                            r.p99_us
                        );
                        results.push(r);
                    }
                    Err(e) => {
                        eprintln!("  {} / {} 失败: {e}", fw.as_str(), wl.as_str());
                    }
                }
            }
        }
        results
    });

    cleanup_sqlite_connection(&conn);

    if results.is_empty() {
        eprintln!("所有基准测试均失败，无结果输出");
        std::process::exit(1);
    }

    let report = BenchReport::new(results.clone());
    let filename = report_filename();
    if let Some(parent) = std::path::Path::new(&filename).parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    match report.write_to_file(&filename) {
        Ok(()) => println!("\n报告已写入: {}", filename),
        Err(e) => eprintln!("写入报告失败: {}", e),
    }

    handle_baseline(&effective_config, &results);

    println!("\n框架版本:");
    for (fw, ver) in &report.framework_versions {
        println!("  {} = {}", fw.as_str(), ver);
    }
}

/// SIMD vs 标量对比（v10.13.0 ROI-1：移除模拟吞吐模型，改为真实 DB 数据集实测）
#[cfg(feature = "real-bench")]
fn run_simd_comparison_real(config: &BenchConfig) {
    use sz_orm_bench::real_db::DatasetInitializer;
    use sz_orm_bench::{DbBackend, SimdComparisonResult};

    println!(
        "sz-orm-bench: SIMD vs 标量对比（真实 SQLite 数据集，dataset_size={}）",
        config.dataset_size
    );
    let conn = sqlite_temp_connection("simd");
    let rt = tokio::runtime::Runtime::new().expect("创建 tokio runtime 失败");
    let result = rt.block_on(async {
        DatasetInitializer::init(DbBackend::Sqlite, &conn, config.dataset_size)
            .await
            .expect("数据集初始化失败");
        SimdComparisonResult::run_real(&conn, config.dataset_size)
            .await
            .expect("SIMD 真实实测失败")
    });
    cleanup_sqlite_connection(&conn);

    println!("  SIMD 吞吐: {:.0} ops/s", result.simd_throughput_ops);
    println!("  标量吞吐: {:.0} ops/s", result.scalar_throughput_ops);
    println!("  加速比: {:.2}x", result.speedup);
    println!("  SIMD 可用: {}", result.simd_available);
    if result.meets_threshold() {
        println!("  ✅ 满足 ≥1.5x 阈值");
    } else {
        println!("  ⚠ 未满足 ≥1.5x 阈值");
    }
}

fn parse_config(args: &[String]) -> BenchConfig {
    let mut config = BenchConfig::new();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--seed" if i + 1 < args.len() => {
                config.seed = args[i + 1].parse().unwrap_or(42);
                i += 2;
            }
            "--rounds" if i + 1 < args.len() => {
                config.measure_rounds = args[i + 1].parse().unwrap_or(10);
                i += 2;
            }
            "--pool-size" if i + 1 < args.len() => {
                config.pool_size = args[i + 1].parse().unwrap_or(20);
                i += 2;
            }
            "--db-backend" if i + 1 < args.len() => {
                match args[i + 1].as_str() {
                    "sqlite" => config.db_backend = sz_orm_bench::DbBackend::Sqlite,
                    "mysql" => config.db_backend = sz_orm_bench::DbBackend::Mysql,
                    "postgres" => config.db_backend = sz_orm_bench::DbBackend::Postgres,
                    "oracle" => config.db_backend = sz_orm_bench::DbBackend::Oracle,
                    other => eprintln!("未知 db-backend: {other}，使用默认 sqlite"),
                }
                i += 2;
            }
            "--db-connection" if i + 1 < args.len() => {
                config.db_connection = args[i + 1].clone();
                i += 2;
            }
            "--dataset-size" if i + 1 < args.len() => {
                config.dataset_size = args[i + 1].parse().unwrap_or(10000);
                i += 2;
            }
            "--compare" if i + 1 < args.len() => {
                let frameworks: Vec<FrameworkType> = args[i + 1]
                    .split(',')
                    .filter_map(|s| match s.trim() {
                        "sz-orm" => Some(FrameworkType::SzOrm),
                        "sea-orm" => Some(FrameworkType::SeaOrm),
                        "diesel" => Some(FrameworkType::Diesel),
                        "sqlx" => Some(FrameworkType::Sqlx),
                        _ => None,
                    })
                    .collect();
                config.compare_frameworks = frameworks;
                i += 2;
            }
            "--init-once" => {
                config.init_once = true;
                i += 1;
            }
            "--save-baseline" if i + 1 < args.len() => {
                config.save_baseline = Some(args[i + 1].clone());
                i += 2;
            }
            "--compare-baseline" if i + 1 < args.len() => {
                config.compare_baseline = Some(args[i + 1].clone());
                i += 2;
            }
            _ => i += 1,
        }
    }
    config
}

fn print_help() {
    println!("sz-orm-bench — 真实 DB 基准对标工具");
    println!();
    println!("用法:");
    println!("    sz-orm-bench [options]        （需 --features real-bench）");
    println!(
        "    sz-orm-bench simd             （真实 DB 数据集 SIMD 对比，需 --features real-bench）"
    );
    println!();
    println!("选项:");
    println!("    --seed <n>              随机种子（默认 42）");
    println!("    --rounds <n>            测量轮数（默认 10，>=5）");
    println!("    --pool-size <n>         连接池大小（默认 20）");
    println!("    --dataset-size <n>      数据集大小（默认 10000，>=100）");
    println!("    --db-backend <type>     数据库后端: sqlite|mysql|postgres（默认 sqlite）");
    println!("    --db-connection <str>   数据库连接串（默认 sqlite 临时文件）");
    println!("    --compare <fw1,fw2,...> 对标框架列表（逗号分隔: sz-orm,sea-orm,diesel,sqlx）");
    println!("    --init-once             启用 init-once 策略（一次性初始化后多次运行查询）");
    println!("    --save-baseline <name>  保存基线到 bench-results/baseline_<name>.json");
    println!("    --compare-baseline <name> 对比既有基线（退化 ≥10% 阻断，退出码 1）");
    println!();
    println!("工作负载:");
    for wl in [
        WorkloadType::SingleRowQuery,
        WorkloadType::BatchQuery,
        WorkloadType::ComplexJoin,
        WorkloadType::Transaction,
        WorkloadType::PoolConcurrency,
        WorkloadType::SimdCompare,
    ] {
        println!("    {}", wl.as_str());
    }
    println!();
    println!("框架:");
    for fw in [
        FrameworkType::SzOrm,
        FrameworkType::SeaOrm,
        FrameworkType::Diesel,
        FrameworkType::Sqlx,
    ] {
        println!("  {}", fw.as_str());
    }
    println!();
    println!("说明: 模拟延迟模型（--simulate）已于 v10.13.0 移除（ADR-003 禁止模拟延迟）。");
}
