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

    if args.len() > 1 && args[1] == "simd" {
        run_simd_comparison();
        return;
    }

    run_benchmark(&config);
}

fn run_benchmark(config: &BenchConfig) {
    println!(
        "sz-orm-bench: 运行基准对标（seed={}, rounds={}）",
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
    println!("    --seed <n>         随机种子（默认 42）");
    println!("    --rounds <n>       测量轮数（默认 10）");
    println!("    --pool-size <n>    连接池大小（默认 20）");
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
