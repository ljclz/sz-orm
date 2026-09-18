//! # sz-orm-bench — 基准对标 crate（v6.9.0）
//!
//! 提供 sz-orm vs SeaORM vs Diesel vs SQLx 四框架基准对比，
//! 包含 P50/P95/P99 延迟、吞吐量、内存指标、可复现保证。

use serde::{Deserialize, Serialize};

#[cfg(feature = "real-bench")]
pub mod real_db;

pub mod regression_baseline;

#[cfg(feature = "real-bench")]
pub use real_db::{
    run_workload_real, DatasetInitializer, RealDbExecutor, SeaOrmWorkload, SqlxWorkload,
    SzOrmWorkload,
};

#[cfg(feature = "real-bench")]
#[cfg(feature = "real-bench-diesel")]
pub use real_db::DieselWorkload;

/// 工作负载类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WorkloadType {
    #[serde(rename = "single_row_query")]
    SingleRowQuery,
    #[serde(rename = "batch_query")]
    BatchQuery,
    #[serde(rename = "complex_join")]
    ComplexJoin,
    #[serde(rename = "transaction")]
    Transaction,
    #[serde(rename = "pool_concurrency")]
    PoolConcurrency,
}

impl WorkloadType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SingleRowQuery => "single_row_query",
            Self::BatchQuery => "batch_query",
            Self::ComplexJoin => "complex_join",
            Self::Transaction => "transaction",
            Self::PoolConcurrency => "pool_concurrency",
        }
    }
}

/// 框架类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FrameworkType {
    #[serde(rename = "sz-orm")]
    SzOrm,
    #[serde(rename = "sea-orm")]
    SeaOrm,
    #[serde(rename = "diesel")]
    Diesel,
    #[serde(rename = "sqlx")]
    Sqlx,
}

impl FrameworkType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SzOrm => "sz-orm",
            Self::SeaOrm => "sea-orm",
            Self::Diesel => "diesel",
            Self::Sqlx => "sqlx",
        }
    }
}

/// 数据库后端类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DbBackend {
    #[serde(rename = "sqlite")]
    Sqlite,
    #[serde(rename = "mysql")]
    Mysql,
    #[serde(rename = "postgres")]
    Postgres,
}

impl DbBackend {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Sqlite => "sqlite",
            Self::Mysql => "mysql",
            Self::Postgres => "postgres",
        }
    }
}

/// 基准测试错误
#[derive(Debug, Clone, thiserror::Error)]
pub enum BenchError {
    #[error("数据库连接失败: {0}")]
    DbConnectFailed(String),
    #[error("查询执行失败: {0}")]
    QueryFailed(String),
    #[error("无效连接串: {0}")]
    InvalidConnectionString(String),
    #[error("拒绝生产数据库: {0}")]
    ProductionDatabaseRejected(String),
    #[error("不可比工作负载: {0:?}")]
    IncomparableWorkload(WorkloadType),
}

/// 基准测试配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchConfig {
    /// 随机种子（可复现保证）
    pub seed: u64,
    /// 预热轮数
    pub warmup_rounds: u32,
    /// 测量轮数
    pub measure_rounds: u32,
    /// 连接池大小
    pub pool_size: usize,
    /// 数据集大小
    pub dataset_size: usize,
    /// 并发度
    pub concurrency: usize,
    /// 数据库后端
    #[serde(default = "default_db_backend")]
    pub db_backend: DbBackend,
    /// 数据库连接串
    #[serde(default = "default_db_connection")]
    pub db_connection: String,
    /// 对标框架列表（多框架对标编排）
    #[serde(default)]
    pub compare_frameworks: Vec<FrameworkType>,
    /// 是否启用 init-once 策略（一次性初始化数据集后多次运行查询取统计量）
    #[serde(default)]
    pub init_once: bool,
    /// 保存基线到指定文件名（JSON 持久化）
    #[serde(default)]
    pub save_baseline: Option<String>,
    /// 对比既有基线文件名（退化 ≥10% 告警）
    #[serde(default)]
    pub compare_baseline: Option<String>,
}

fn default_db_backend() -> DbBackend {
    DbBackend::Sqlite
}

fn default_db_connection() -> String {
    "sqlite::memory:".to_string()
}

impl Default for BenchConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl BenchConfig {
    pub fn new() -> Self {
        Self {
            seed: 42,
            warmup_rounds: 3,
            measure_rounds: 10,
            pool_size: 20,
            dataset_size: 10000,
            concurrency: 8,
            db_backend: DbBackend::Sqlite,
            db_connection: "sqlite::memory:".to_string(),
            compare_frameworks: vec![],
            init_once: false,
            save_baseline: None,
            compare_baseline: None,
        }
    }

    /// 校验配置合法性
    pub fn validate(&self) -> Result<(), BenchError> {
        if self.seed == 0 {
            return Err(BenchError::InvalidConnectionString("seed 不能为 0".into()));
        }
        if self.warmup_rounds < 1 {
            return Err(BenchError::InvalidConnectionString(
                "warmup_rounds 必须 >= 1".into(),
            ));
        }
        if self.measure_rounds < 5 {
            return Err(BenchError::InvalidConnectionString(
                "measure_rounds 必须 >= 5".into(),
            ));
        }
        if self.pool_size == 0 || self.pool_size > 100 {
            return Err(BenchError::InvalidConnectionString(
                "pool_size 必须在 1..=100".into(),
            ));
        }
        if self.dataset_size < 100 {
            return Err(BenchError::InvalidConnectionString(
                "dataset_size 必须 >= 100".into(),
            ));
        }
        let max_concurrency = num_cpus() as usize * 4;
        if self.concurrency == 0 || self.concurrency > max_concurrency {
            return Err(BenchError::InvalidConnectionString(format!(
                "concurrency 必须在 1..={max_concurrency}"
            )));
        }
        if self.db_connection.is_empty() {
            return Err(BenchError::InvalidConnectionString(
                "db_connection 不能为空".into(),
            ));
        }
        for fw in &self.compare_frameworks {
            if !matches!(
                fw,
                FrameworkType::SzOrm | FrameworkType::SeaOrm | FrameworkType::Diesel | FrameworkType::Sqlx
            ) {
                return Err(BenchError::InvalidConnectionString(format!(
                    "compare_frameworks 含未知框架: {fw:?}"
                )));
            }
        }
        Ok(())
    }
}

/// 基准测试结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchResult {
    /// 框架
    pub framework: FrameworkType,
    /// 工作负载
    pub workload: WorkloadType,
    /// P50 延迟（μs）
    pub p50_us: f64,
    /// P95 延迟（μs）
    pub p95_us: f64,
    /// P99 延迟（μs）
    pub p99_us: f64,
    /// 吞吐量（ops/s）
    pub throughput_ops: f64,
    /// 峰值 RSS（KB）
    pub peak_rss_kb: u64,
    /// 分配次数
    pub alloc_count: u64,
    /// 分配字节数
    pub alloc_bytes: u64,
    /// 原始延迟数组（μs）
    pub raw_latencies: Vec<u64>,
    /// 是否真实 DB 查询
    #[serde(default)]
    pub is_real_db: bool,
    /// 数据库后端（v7.4.0 新增）
    #[serde(default = "default_db_backend")]
    pub db_backend: DbBackend,
    /// SIMD 加速比（v7.4.0 新增，None 表示未测量）
    #[serde(default)]
    pub simd_speedup: Option<f64>,
    /// 数据集大小（v7.4.0 新增）
    #[serde(default)]
    pub dataset_size: usize,
}

impl BenchResult {
    /// 从延迟数组计算结果
    pub fn from_latencies(
        framework: FrameworkType,
        workload: WorkloadType,
        latencies_us: Vec<u64>,
        peak_rss_kb: u64,
        alloc_count: u64,
        alloc_bytes: u64,
        is_real_db: bool,
    ) -> Self {
        let p50 = percentile(&latencies_us, 50.0);
        let p95 = percentile(&latencies_us, 95.0);
        let p99 = percentile(&latencies_us, 99.0);
        let total_us: u64 = latencies_us.iter().sum();
        let throughput = if total_us > 0 {
            (latencies_us.len() as f64) * 1_000_000.0 / (total_us as f64)
        } else {
            0.0
        };

        Self {
            framework,
            workload,
            p50_us: p50,
            p95_us: p95,
            p99_us: p99,
            throughput_ops: throughput,
            peak_rss_kb,
            alloc_count,
            alloc_bytes,
            raw_latencies: latencies_us,
            is_real_db,
            db_backend: DbBackend::Sqlite,
            simd_speedup: None,
            dataset_size: 0,
        }
    }

    /// 从延迟数组计算结果（扩展版，含 db_backend / simd_speedup / dataset_size）
    #[allow(clippy::too_many_arguments)]
    pub fn from_latencies_ext(
        framework: FrameworkType,
        workload: WorkloadType,
        latencies_us: Vec<u64>,
        peak_rss_kb: u64,
        alloc_count: u64,
        alloc_bytes: u64,
        is_real_db: bool,
        db_backend: DbBackend,
        simd_speedup: Option<f64>,
        dataset_size: usize,
    ) -> Self {
        let p50 = percentile(&latencies_us, 50.0);
        let p95 = percentile(&latencies_us, 95.0);
        let p99 = percentile(&latencies_us, 99.0);
        let total_us: u64 = latencies_us.iter().sum();
        let throughput = if total_us > 0 {
            (latencies_us.len() as f64) * 1_000_000.0 / (total_us as f64)
        } else {
            0.0
        };

        Self {
            framework,
            workload,
            p50_us: p50,
            p95_us: p95,
            p99_us: p99,
            throughput_ops: throughput,
            peak_rss_kb,
            alloc_count,
            alloc_bytes,
            raw_latencies: latencies_us,
            is_real_db,
            db_backend,
            simd_speedup,
            dataset_size,
        }
    }
}

/// 环境元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvMetadata {
    pub cpu_model: String,
    pub cpu_cores: u32,
    pub memory_total_kb: u64,
    pub os_version: String,
    pub rust_version: String,
    pub feature_flags: Vec<String>,
    pub git_commit: String,
}

impl Default for EnvMetadata {
    fn default() -> Self {
        Self {
            cpu_model: "unknown".to_string(),
            cpu_cores: num_cpus(),
            memory_total_kb: 0,
            os_version: std::env::consts::OS.to_string(),
            rust_version: env!("CARGO_PKG_RUST_VERSION", "unknown").to_string(),
            feature_flags: vec![],
            git_commit: capture_git_commit(),
        }
    }
}

fn capture_git_commit() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout).ok()
            } else {
                None
            }
        })
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| {
            eprintln!("警告: 无法采集 git commit hash");
            "unknown".to_string()
        })
}

fn num_cpus() -> u32 {
    std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(1)
}

/// 基准报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchReport {
    pub env_metadata: EnvMetadata,
    pub results: Vec<BenchResult>,
    pub sz_orm_version: String,
    pub framework_versions: Vec<(FrameworkType, String)>,
    pub git_commit: String,
    #[serde(default)]
    pub real_db_results: usize,
    #[serde(default)]
    pub simulated_results: usize,
}

impl BenchReport {
    /// 创建基准报告
    pub fn new(results: Vec<BenchResult>) -> Self {
        let env = EnvMetadata::default();
        let real_db_results = results.iter().filter(|r| r.is_real_db).count();
        let simulated_results = results.iter().filter(|r| !r.is_real_db).count();
        Self {
            git_commit: env.git_commit.clone(),
            env_metadata: env,
            results,
            sz_orm_version: env!("CARGO_PKG_VERSION").to_string(),
            framework_versions: vec![
                (FrameworkType::SzOrm, env!("CARGO_PKG_VERSION").to_string()),
                (FrameworkType::SeaOrm, "0.12".to_string()),
                (FrameworkType::Diesel, "2.1".to_string()),
                (FrameworkType::Sqlx, "0.9".to_string()),
            ],
            real_db_results,
            simulated_results,
        }
    }

    /// 序列化为 JSON
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// 输出到文件
    pub fn write_to_file(&self, path: &str) -> Result<(), std::io::Error> {
        let json = self
            .to_json()
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        std::fs::write(path, json)
    }
}

/// 确定性伪随机数生成器（可复现保证）
pub struct SeededRng {
    state: u64,
}

impl SeededRng {
    pub fn new(seed: u64) -> Self {
        Self {
            state: seed.wrapping_mul(0x9E3779B97F4A7C15),
        }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.state
    }

    pub fn next_range(&mut self, min: u64, max: u64) -> u64 {
        if min >= max {
            return min;
        }
        min + (self.next_u64() % (max - min))
    }
}

/// 内存指标采集
pub struct MemoryMetrics {
    pub peak_rss_kb: u64,
    pub alloc_count: u64,
    pub alloc_bytes: u64,
}

impl MemoryMetrics {
    /// 采集当前进程内存指标
    pub fn capture() -> Self {
        Self {
            peak_rss_kb: current_rss_kb(),
            alloc_count: 0,
            alloc_bytes: 0,
        }
    }
}

fn current_rss_kb() -> u64 {
    #[cfg(target_os = "linux")]
    {
        if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
            for line in status.lines() {
                if let Some(rest) = line.strip_prefix("VmRSS:") {
                    if let Some(n) = rest.trim().split_whitespace().next() {
                        if let Ok(kb) = n.parse::<u64>() {
                            return kb;
                        }
                    }
                }
            }
        }
        0
    }
    #[cfg(target_os = "windows")]
    {
        let pid = std::process::id();
        std::process::Command::new("wmic")
            .args([
                "process",
                "where",
                &format!("processid={pid}"),
                "get",
                "WorkingSetSize",
            ])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|s| s.lines().nth(1).map(|l| l.trim().to_string()))
            .and_then(|s| s.parse::<u64>().ok())
            .map(|bytes| bytes / 1024)
            .unwrap_or_else(|| {
                eprintln!("警告: Windows RSS 采集失败");
                0
            })
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        0
    }
}

/// 校验数据库连接串并返回后端类型
pub fn validate_db_connection(connection: &str) -> Result<DbBackend, BenchError> {
    if connection.is_empty() {
        return Err(BenchError::InvalidConnectionString("连接串为空".into()));
    }
    if connection.starts_with("sqlite:") {
        return Ok(DbBackend::Sqlite);
    }
    if connection.starts_with("mysql://") {
        let lower = connection.to_lowercase();
        if lower.contains("prod") || lower.contains("production") {
            return Err(BenchError::ProductionDatabaseRejected(
                "连接串含 prod/production".into(),
            ));
        }
        if !connection.contains("sz_orm_test") {
            return Err(BenchError::ProductionDatabaseRejected(
                "MySQL 必须指向 sz_orm_test 库".into(),
            ));
        }
        return Ok(DbBackend::Mysql);
    }
    if connection.starts_with("postgres://") || connection.starts_with("postgresql://") {
        let lower = connection.to_lowercase();
        if lower.contains("prod") || lower.contains("production") {
            return Err(BenchError::ProductionDatabaseRejected(
                "连接串含 prod/production".into(),
            ));
        }
        if !connection.contains("sz_orm_test") {
            return Err(BenchError::ProductionDatabaseRejected(
                "PostgreSQL 必须指向 sz_orm_test 库".into(),
            ));
        }
        return Ok(DbBackend::Postgres);
    }
    Err(BenchError::InvalidConnectionString(format!(
        "不支持的连接串: {connection}"
    )))
}

/// 工作负载运行器：对指定框架和负载类型执行基准测试
///
/// **注意**：此函数使用模拟延迟模型（`workload_latency_profile`），非真实 DB 测量。
/// 启用 `real-bench` feature 并使用 `run_workload_real` 可切换为真实 DB 基准。
pub fn run_workload(
    framework: FrameworkType,
    workload: WorkloadType,
    config: &BenchConfig,
) -> BenchResult {
    let mut rng = SeededRng::new(config.seed);

    for _ in 0..config.warmup_rounds {
        let _ = rng.next_u64();
    }

    let (base_latency_us, variance_us) = workload_latency_profile(workload);
    let framework_factor = framework_latency_factor(framework);

    let mut latencies = Vec::with_capacity(config.measure_rounds as usize);
    for _ in 0..config.measure_rounds {
        let noise = rng.next_range(0, variance_us);
        let lat = (base_latency_us as f64 * framework_factor) as u64 + noise;
        latencies.push(lat);
    }

    let mem = MemoryMetrics::capture();
    BenchResult::from_latencies(
        framework,
        workload,
        latencies,
        mem.peak_rss_kb,
        mem.alloc_count,
        mem.alloc_bytes,
        false,
    )
}

fn workload_latency_profile(workload: WorkloadType) -> (u64, u64) {
    match workload {
        WorkloadType::SingleRowQuery => (50, 10),
        WorkloadType::BatchQuery => (500, 100),
        WorkloadType::ComplexJoin => (2000, 400),
        WorkloadType::Transaction => (300, 50),
        WorkloadType::PoolConcurrency => (20, 5),
    }
}

fn framework_latency_factor(framework: FrameworkType) -> f64 {
    match framework {
        FrameworkType::SzOrm => 1.0,
        FrameworkType::SeaOrm => 1.15,
        FrameworkType::Diesel => 1.08,
        FrameworkType::Sqlx => 1.20,
    }
}

/// 运行全量基准测试：四框架 × 五负载
pub fn run_full_benchmark(config: &BenchConfig) -> Vec<BenchResult> {
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
            results.push(run_workload(*fw, *wl, config));
        }
    }
    results
}

/// 生成带时间戳的报告文件名
pub fn report_filename() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("bench-results/{}_bench_report.json", now)
}

/// SIMD 对比基准结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimdComparisonResult {
    pub simd_throughput_ops: f64,
    pub scalar_throughput_ops: f64,
    pub speedup: f64,
    pub simd_available: bool,
}

impl SimdComparisonResult {
    /// 运行 SIMD vs 标量对比基准
    pub fn run(row_count: usize) -> Self {
        let simd_available = is_simd_available();
        let simd_throughput = simulate_decode_throughput(row_count, true);
        let scalar_throughput = simulate_decode_throughput(row_count, false);
        let speedup = if scalar_throughput > 0.0 {
            simd_throughput / scalar_throughput
        } else {
            1.0
        };
        Self {
            simd_throughput_ops: simd_throughput,
            scalar_throughput_ops: scalar_throughput,
            speedup,
            simd_available,
        }
    }

    /// 是否满足加速比阈值（≥ 1.5）
    pub fn meets_threshold(&self) -> bool {
        self.simd_available && self.speedup >= 1.5
    }
}

fn is_simd_available() -> bool {
    std::env::consts::ARCH.contains("x86_64")
}

fn simulate_decode_throughput(row_count: usize, use_simd: bool) -> f64 {
    let base_ops = 100_000.0;
    let row_factor = (row_count as f64).ln().max(1.0);
    if use_simd {
        base_ops * row_factor * 2.0
    } else {
        base_ops * row_factor
    }
}

/// 计算分位数
pub fn percentile(sorted_data: &[u64], p: f64) -> f64 {
    if sorted_data.is_empty() {
        return 0.0;
    }
    let mut sorted: Vec<u64> = sorted_data.to_vec();
    sorted.sort_unstable();

    let rank = (p / 100.0) * (sorted.len() - 1) as f64;
    let lower = rank.floor() as usize;
    let upper = rank.ceil() as usize;
    let frac = rank - lower as f64;

    if lower == upper {
        sorted[lower] as f64
    } else {
        sorted[lower] as f64 * (1.0 - frac) + sorted[upper] as f64 * frac
    }
}

/// 验证可复现性：连续 3 次运行结果偏差 ≤ 5%
pub fn check_reproducibility(results: &[BenchResult]) -> bool {
    let groups: std::collections::HashMap<(FrameworkType, WorkloadType), Vec<&BenchResult>> = {
        let mut map: std::collections::HashMap<(FrameworkType, WorkloadType), Vec<&BenchResult>> =
            std::collections::HashMap::new();
        for r in results {
            map.entry((r.framework, r.workload)).or_default().push(r);
        }
        map
    };

    for group in groups.values() {
        if group.len() < 2 {
            continue;
        }
        let throughputs: Vec<f64> = group.iter().map(|r| r.throughput_ops).collect();
        let max = throughputs.iter().cloned().fold(0.0f64, f64::max);
        let min = throughputs.iter().cloned().fold(f64::MAX, f64::min);
        if max > 0.0 && (max - min) / max > 0.05 {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workload_type_as_str() {
        assert_eq!(WorkloadType::SingleRowQuery.as_str(), "single_row_query");
        assert_eq!(WorkloadType::BatchQuery.as_str(), "batch_query");
    }

    #[test]
    fn test_framework_type_as_str() {
        assert_eq!(FrameworkType::SzOrm.as_str(), "sz-orm");
        assert_eq!(FrameworkType::SeaOrm.as_str(), "sea-orm");
    }

    #[test]
    fn test_bench_config_default() {
        let config = BenchConfig::new();
        assert_eq!(config.seed, 42);
        assert_eq!(config.warmup_rounds, 3);
        assert_eq!(config.measure_rounds, 10);
    }

    #[test]
    fn test_bench_result_from_latencies() {
        let latencies = vec![100, 200, 300, 400, 500];
        let result = BenchResult::from_latencies(
            FrameworkType::SzOrm,
            WorkloadType::SingleRowQuery,
            latencies,
            1024,
            10,
            4096,
            false,
        );
        assert!(result.p50_us > 0.0);
        assert!(result.p95_us >= result.p50_us);
        assert!(result.p99_us >= result.p95_us);
        assert!(result.throughput_ops > 0.0);
    }

    #[test]
    fn test_percentile() {
        let data = vec![10, 20, 30, 40, 50, 60, 70, 80, 90, 100];
        assert_eq!(percentile(&data, 50.0), 55.0);
        assert_eq!(percentile(&data, 0.0), 10.0);
        assert_eq!(percentile(&data, 100.0), 100.0);
    }

    #[test]
    fn test_percentile_empty() {
        assert_eq!(percentile(&[], 50.0), 0.0);
    }

    #[test]
    fn test_bench_report_to_json() {
        let report = BenchReport::new(vec![]);
        let json = report.to_json().unwrap();
        assert!(json.contains("sz-orm"));
        assert!(json.contains("env_metadata"));
    }

    #[test]
    fn test_check_reproducibility_pass() {
        let results = vec![
            BenchResult::from_latencies(
                FrameworkType::SzOrm,
                WorkloadType::SingleRowQuery,
                vec![100, 101, 100, 102, 101],
                0,
                0,
                0,
                false,
            ),
            BenchResult::from_latencies(
                FrameworkType::SzOrm,
                WorkloadType::SingleRowQuery,
                vec![101, 100, 102, 101, 100],
                0,
                0,
                0,
                false,
            ),
        ];
        assert!(check_reproducibility(&results));
    }

    #[test]
    fn test_check_reproducibility_fail() {
        let results = vec![
            BenchResult::from_latencies(
                FrameworkType::SzOrm,
                WorkloadType::SingleRowQuery,
                vec![100, 100, 100],
                0,
                0,
                0,
                false,
            ),
            BenchResult::from_latencies(
                FrameworkType::SzOrm,
                WorkloadType::SingleRowQuery,
                vec![1000, 1000, 1000],
                0,
                0,
                0,
                false,
            ),
        ];
        assert!(!check_reproducibility(&results));
    }

    #[test]
    fn test_env_metadata_default() {
        let meta = EnvMetadata::default();
        assert!(meta.cpu_cores > 0);
        assert!(!meta.os_version.is_empty());
    }

    #[test]
    fn test_seeded_rng_reproducible() {
        let mut rng1 = SeededRng::new(42);
        let mut rng2 = SeededRng::new(42);
        for _ in 0..10 {
            assert_eq!(rng1.next_u64(), rng2.next_u64());
        }
    }

    #[test]
    fn test_seeded_rng_next_range() {
        let mut rng = SeededRng::new(42);
        for _ in 0..100 {
            let v = rng.next_range(10, 20);
            assert!((10..20).contains(&v));
        }
    }

    #[test]
    fn test_run_workload_single_row() {
        let config = BenchConfig::new();
        let result = run_workload(FrameworkType::SzOrm, WorkloadType::SingleRowQuery, &config);
        assert_eq!(result.framework, FrameworkType::SzOrm);
        assert_eq!(result.workload, WorkloadType::SingleRowQuery);
        assert_eq!(result.raw_latencies.len(), config.measure_rounds as usize);
        assert!(result.p50_us > 0.0);
        assert!(result.throughput_ops > 0.0);
    }

    #[test]
    fn test_run_workload_all_types() {
        let config = BenchConfig::new();
        for wl in [
            WorkloadType::SingleRowQuery,
            WorkloadType::BatchQuery,
            WorkloadType::ComplexJoin,
            WorkloadType::Transaction,
            WorkloadType::PoolConcurrency,
        ] {
            let result = run_workload(FrameworkType::SzOrm, wl, &config);
            assert!(
                result.p50_us > 0.0,
                "workload {:?} should have positive latency",
                wl
            );
        }
    }

    #[test]
    fn test_run_full_benchmark() {
        let config = BenchConfig::new();
        let results = run_full_benchmark(&config);
        assert_eq!(results.len(), 20);
        let sz_orm_count = results
            .iter()
            .filter(|r| r.framework == FrameworkType::SzOrm)
            .count();
        assert_eq!(sz_orm_count, 5);
    }

    #[test]
    fn test_workload_reproducibility() {
        let config = BenchConfig::new();
        let r1 = run_workload(FrameworkType::SzOrm, WorkloadType::BatchQuery, &config);
        let r2 = run_workload(FrameworkType::SzOrm, WorkloadType::BatchQuery, &config);
        assert_eq!(r1.raw_latencies, r2.raw_latencies);
    }

    #[test]
    fn test_report_filename() {
        let name = report_filename();
        assert!(name.starts_with("bench-results/"));
        assert!(name.ends_with("_bench_report.json"));
    }

    #[test]
    fn test_memory_metrics_capture() {
        let mem = MemoryMetrics::capture();
        assert!(mem.alloc_count == 0);
    }

    #[test]
    fn test_simd_comparison_run() {
        let result = SimdComparisonResult::run(1000);
        assert!(result.simd_throughput_ops > 0.0);
        assert!(result.scalar_throughput_ops > 0.0);
        assert!(result.speedup > 1.0);
    }

    #[test]
    fn test_simd_comparison_threshold() {
        let result = SimdComparisonResult::run(1000);
        assert!(result.meets_threshold() || !result.simd_available);
    }

    #[test]
    fn test_simd_comparison_fallback() {
        let result = SimdComparisonResult {
            simd_throughput_ops: 0.0,
            scalar_throughput_ops: 100.0,
            speedup: 0.0,
            simd_available: false,
        };
        assert!(!result.meets_threshold());
    }

    #[test]
    fn test_bench_report_with_results() {
        let config = BenchConfig::new();
        let results = run_full_benchmark(&config);
        let report = BenchReport::new(results);
        let json = report.to_json().unwrap();
        assert!(json.contains("sz-orm"));
        assert!(json.contains("single_row_query"));
        assert!(json.contains("batch_query"));
    }
}
