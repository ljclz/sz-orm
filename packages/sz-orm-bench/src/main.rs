//! sz-orm-bench 命令行入口 — 运行基准对标并输出 JSON 报告

use std::env;
use sz_orm_bench::{
    report_filename, run_full_benchmark, BenchConfig, BenchReport, FrameworkType,
    SimdComparisonResult, WorkloadType,
};

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() > 1 && (args[1] == "--help" || args[1] == "-h") {
        print_help();
        return;
    }

    let config = parse_config(&args);
    let simulate = args.iter().any(|a| a == "--simulate");

    if args.len() > 1 && args[1] == "simd" {
        run_simd_comparison();
        return;
    }

    if simulate {
        run_benchmark_simulate(&config);
    } else {
        #[cfg(feature = "real-bench")]
        {
            run_benchmark_real(&config);
        }
        #[cfg(not(feature = "real-bench"))]
        {
            run_benchmark_simulate(&config);
        }
    }
}

fn run_benchmark_simulate(config: &BenchConfig) {
    println!(
        "sz-orm-bench: 模拟基准对标（seed={}, rounds={}）— 模拟延迟模型，非真实 DB 测量",
        config.seed, config.measure_rounds
    );

    let results = run_full_benchmark(config);
    let report = BenchReport::new(results);

    let filename = report_filename();
    if let Some(parent) = std::path::Path::new(&filename).parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    match report.write_to_file(&filename) {
        Ok(()) => println!("报告已写入: {}", filename),
        Err(e) => eprintln!("写入报告失败: {}", e),
    }

    println!("\n框架版本:");
    for (fw, ver) in &report.framework_versions {
        println!("  {} = {}", fw.as_str(), ver);
    }
}

#[cfg(feature = "real-bench")]
fn run_benchmark_real(config: &BenchConfig) {
    println!(
        "sz-orm-bench: 真实 DB 基准对标（backend={}, rounds={}）",
        config.db_backend.as_str(),
        config.measure_rounds
    );

    if let Err(e) = config.validate() {
        eprintln!("配置校验失败: {e}");
        std::process::exit(1);
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
        ];

        let mut results = Vec::new();
        for fw in &frameworks {
            for wl in &workloads {
                match sz_orm_bench::run_workload_real(*fw, *wl, config).await {
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

    if results.is_empty() {
        eprintln!("所有基准测试均失败，无结果输出");
        std::process::exit(1);
    }

    let report = BenchReport::new(results);
    let filename = report_filename();
    if let Some(parent) = std::path::Path::new(&filename).parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    match report.write_to_file(&filename) {
        Ok(()) => println!("\n报告已写入: {}", filename),
        Err(e) => eprintln!("写入报告失败: {}", e),
    }

    println!("\n框架版本:");
    for (fw, ver) in &report.framework_versions {
        println!("  {} = {}", fw.as_str(), ver);
    }
}

fn run_simd_comparison() {
    println!("sz-orm-bench: SIMD vs 标量对比（1000 行批量解码）");
    let result = SimdComparisonResult::run(1000);
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
            _ => i += 1,
        }
    }
    config
}

fn print_help() {
    println!("sz-orm-bench — 基准对标工具");
    println!();
    println!("用法:");
    println!("    sz-orm-bench [options]");
    println!("    sz-orm-bench simd");
    println!();
    println!("选项:");
    println!("    --seed <n>              随机种子（默认 42）");
    println!("    --rounds <n>            测量轮数（默认 10）");
    println!("    --pool-size <n>         连接池大小（默认 20）");
    println!("    --dataset-size <n>      数据集大小（默认 10000）");
    println!("    --db-backend <type>     数据库后端: sqlite|mysql（默认 sqlite）");
    println!("    --db-connection <str>   数据库连接串（默认 sqlite::memory:）");
    println!("    --simulate              使用模拟延迟模型（旧路径）");
    println!();
    println!("工作负载:");
    for wl in [
        WorkloadType::SingleRowQuery,
        WorkloadType::BatchQuery,
        WorkloadType::ComplexJoin,
        WorkloadType::Transaction,
        WorkloadType::PoolConcurrency,
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
        println!("    {}", fw.as_str());
    }
}
