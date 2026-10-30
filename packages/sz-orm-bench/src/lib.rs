//! # sz-orm-bench — 基准对标 crate（v6.9.0 起）
//!
//! 提供 sz-orm vs SeaORM vs Diesel vs SQLx 真实 DB 基准对比，
//! 包含 P50/P95/P99 延迟、吞吐量、内存指标、可复现保证。
//!
//! 历史说明：v10.13.0 移除了 v7.3.0 遗留的模拟延迟模型
//! （`run_workload` / `workload_latency_profile` / `framework_latency_factor` /
//! `run_full_benchmark` / `SeededRng` / `SimdComparisonResult::run`）。
//! 全部基准测量一律通过 `real_db::run_workload_real`（`--features real-bench`）
//! 连接真实 DB 执行（ADR-003：禁止回退模拟延迟）。

use serde::{Deserialize, Serialize};

#[cfg(feature = "real-bench")]
pub mod real_db;

pub mod regression_baseline;
pub use regression_baseline::RegressionBaseline;

#[cfg(feature = "stability-matrix")]
pub mod stability_matrix;
#[cfg(feature = "stability-matrix")]
pub use stability_matrix::{MatrixEntry, StabilityMatrixReport, StabilityMatrixRunner};

#[cfg(feature = "real-bench")]
pub use real_db::{
    run_workload_real, DatasetInitializer, RealDbExecutor, SeaOrmWorkload, SqlxWorkload,
    SzOrmWorkload,
};

#[cfg(feature = "real-bench")]
#[cfg(feature = "real-bench-diesel")]
pub use real_db::DieselWorkload;

// v8.1.0 性能基准实测化模块导出（feature gate 门控）
#[cfg(feature = "bench-real-db")]
pub mod prod_db_guard;
#[cfg(feature = "bench-real-db")]
pub use prod_db_guard::BenchProdDbGuard;

#[cfg(feature = "bench-real-db")]
pub mod repeatability_guard;
#[cfg(feature = "bench-real-db")]
pub use repeatability_guard::{BenchRepeatabilityGuard, RepeatabilityReport};

#[cfg(feature = "bench-real-db")]
pub mod result_exporter;
#[cfg(feature = "bench-real-db")]
pub use result_exporter::{
    render_bench_prometheus_metrics, BenchResultExporter, ExportFormat, ExportedBenchRecord,
};

#[cfg(feature = "perf-regression-ci")]
pub mod ci_integration;
#[cfg(feature = "perf-regression-ci")]
pub use ci_integration::{BenchExecutor, CiBenchDecision, CiBenchIntegration, CiBenchReport};

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
    #[serde(rename = "simd_compare")]
    SimdCompare,
    #[serde(rename = "concurrent_read_write")]
    ConcurrentReadWrite,
    #[serde(rename = "pool_stress")]
    PoolStress,
    #[serde(rename = "long_transaction")]
    LongTransaction,
    #[serde(rename = "large_result_set")]
    LargeResultSet,
}

impl WorkloadType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SingleRowQuery => "single_row_query",
            Self::BatchQuery => "batch_query",
            Self::ComplexJoin => "complex_join",
            Self::Transaction => "transaction",
            Self::PoolConcurrency => "pool_concurrency",
            Self::SimdCompare => "simd_compare",
            Self::ConcurrentReadWrite => "concurrent_read_write",
            Self::PoolStress => "pool_stress",
            Self::LongTransaction => "long_transaction",
            Self::LargeResultSet => "large_result_set",
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
    #[serde(rename = "oracle")]
    Oracle,
}

impl DbBackend {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Sqlite => "sqlite",
            Self::Mysql => "mysql",
            Self::Postgres => "postgres",
            Self::Oracle => "oracle",
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
    #[error("生产 DB 被禁止执行基准: {0}")]
    ProdDbForbidden(String),
    #[error("基准配置缺失: {0}")]
    ConfigMissing(String),
    #[error("基准样本不足: 需要 {required} 次，实际 {actual} 次")]
    InsufficientSamples { required: usize, actual: usize },
    #[error("不支持的导出格式: {0}")]
    UnsupportedFormat(String),
    #[error("导出路径不可写: {0}")]
    ExportPathNotWritable(String),
    #[error("基线缺失: {0}")]
    BaselineMissing(String),
    #[error("真实 DB 不可用: {0}")]
    DbUnavailable(String),
}

/// 基准 DB 连接配置（v8.1.0 新增，用于 CI 基准集成）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchDbConfig {
    /// 数据库连接串
    pub connection: String,
    /// 是否显式标记为生产 DB
    #[serde(default)]
    pub is_production: bool,
    /// 数据库版本（可选，运行时探测）
    #[serde(default)]
    pub db_version: Option<String>,
}

impl BenchDbConfig {
    /// 创建新的基准 DB 配置
    pub fn new(connection: impl Into<String>) -> Self {
        Self {
            connection: connection.into(),
            is_production: false,
            db_version: None,
        }
    }

    /// 标记为生产 DB
    pub fn with_production(mut self, is_prod: bool) -> Self {
        self.is_production = is_prod;
        self
    }

    /// 设置数据库版本
    pub fn with_db_version(mut self, version: impl Into<String>) -> Self {
        self.db_version = Some(version.into());
        self
    }
}

/// 基准 workload 定义（v8.1.0 新增，用于 CI 基准集成）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkloadDef {
    /// 框架类型
    pub framework: FrameworkType,
    /// 工作负载类型
    pub workload: WorkloadType,
    /// 基准配置
    pub config: BenchConfig,
}

impl WorkloadDef {
    /// 创建新的 workload 定义
    pub fn new(framework: FrameworkType, workload: WorkloadType, config: BenchConfig) -> Self {
        Self {
            framework,
            workload,
            config,
        }
    }
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
                FrameworkType::SzOrm
                    | FrameworkType::SeaOrm
                    | FrameworkType::Diesel
                    | FrameworkType::Sqlx
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
            cpu_model: capture_cpu_model(),
            cpu_cores: num_cpus(),
            memory_total_kb: capture_memory_total_kb(),
            os_version: std::env::consts::OS.to_string(),
            rust_version: capture_rust_version(),
            feature_flags: capture_feature_flags(),
            git_commit: capture_git_commit(),
        }
    }
}

/// 采集 CPU 型号（Windows: PROCESSOR_IDENTIFIER；Linux: /proc/cpuinfo）。
fn capture_cpu_model() -> String {
    #[cfg(windows)]
    {
        if let Ok(v) = std::env::var("PROCESSOR_IDENTIFIER") {
            let v = v.trim().to_string();
            if !v.is_empty() {
                return v;
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(text) = std::fs::read_to_string("/proc/cpuinfo") {
            for line in text.lines() {
                if let Some(v) = line.strip_prefix("model name") {
                    return v.split(':').nth(1).map(str::trim).unwrap_or("").to_string();
                }
            }
        }
    }
    "unknown".to_string()
}

/// 采集物理内存总量（KB）。Linux: /proc/meminfo；Windows: wmic。
fn capture_memory_total_kb() -> u64 {
    #[cfg(target_os = "linux")]
    {
        if let Ok(text) = std::fs::read_to_string("/proc/meminfo") {
            for line in text.lines() {
                if let Some(v) = line.strip_prefix("MemTotal:") {
                    if let Some(kb) = v.trim().split_whitespace().next() {
                        return kb.parse().unwrap_or(0);
                    }
                }
            }
        }
    }
    #[cfg(windows)]
    {
        if let Ok(out) = std::process::Command::new("wmic")
            .args(["ComputerSystem", "get", "TotalPhysicalMemory", "/value"])
            .output()
        {
            if out.status.success() {
                let text = String::from_utf8_lossy(&out.stdout);
                for line in text.lines() {
                    if let Some(v) = line.trim().strip_prefix("TotalPhysicalMemory=") {
                        if let Ok(bytes) = v.trim().parse::<u64>() {
                            return bytes / 1024;
                        }
                    }
                }
            }
        }
    }
    0
}

/// 采集 rustc 版本（如 "1.99.1"）。
fn capture_rust_version() -> String {
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    if let Ok(out) = std::process::Command::new(&rustc).arg("--version").output() {
        if out.status.success() {
            let text = String::from_utf8_lossy(&out.stdout);
            let parts: Vec<&str> = text.split_whitespace().collect();
            if parts.len() >= 2 {
                return parts[1].to_string();
            }
        }
    }
    "unknown".to_string()
}

/// 收集 cargo 构建时启用的 feature 标志（CARGO_FEATURE_* 环境变量）。
fn capture_feature_flags() -> Vec<String> {
    let mut flags: Vec<String> = std::env::vars()
        .filter(|(k, _)| k.starts_with("CARGO_FEATURE_"))
        .map(|(k, _)| k.trim_start_matches("CARGO_FEATURE_").to_lowercase())
        .collect();
    flags.sort();
    flags.dedup();
    flags
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
                    if let Some(n) = rest.split_whitespace().next() {
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
    if connection.starts_with("oracle://") || connection.starts_with("oracle://") {
        let lower = connection.to_lowercase();
        if lower.contains("prod") || lower.contains("production") {
            return Err(BenchError::ProductionDatabaseRejected(
                "连接串含 prod/production".into(),
            ));
        }
        return Ok(DbBackend::Oracle);
    }
    Err(BenchError::InvalidConnectionString(format!(
        "不支持的连接串: {connection}"
    )))
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
    /// 是否满足加速比阈值（≥ 1.5）
    pub fn meets_threshold(&self) -> bool {
        self.simd_available && self.speedup >= 1.5
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
        let results = vec![BenchResult::from_latencies(
            FrameworkType::SzOrm,
            WorkloadType::SingleRowQuery,
            vec![100, 200, 300],
            1024,
            10,
            4096,
            true,
        )];
        let report = BenchReport::new(results);
        let json = report.to_json().unwrap();
        assert!(json.contains("sz-orm"));
        assert!(json.contains("single_row_query"));
        assert!(json.contains("\"is_real_db\": true"));
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BottleneckPoint {
    ConnectionPool,
    Cache,
    Simd,
    ZeroCopy,
    PlanCache,
}

impl BottleneckPoint {
    pub fn as_str(&self) -> &'static str {
        match self {
            BottleneckPoint::ConnectionPool => "connection_pool",
            BottleneckPoint::Cache => "cache",
            BottleneckPoint::Simd => "simd",
            BottleneckPoint::ZeroCopy => "zero_copy",
            BottleneckPoint::PlanCache => "plan_cache",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BottleneckReport {
    pub bottleneck_workloads: Vec<WorkloadType>,
    pub bottleneck_points: Vec<BottleneckPoint>,
    pub optimization_directions: Vec<String>,
}

pub struct BottleneckAnalyzer;

impl BottleneckAnalyzer {
    pub fn analyze(baseline: &crate::regression_baseline::RegressionBaseline) -> BottleneckReport {
        let mut bottleneck_workloads = Vec::new();
        let mut max_p95 = 0.0_f64;
        let mut bottleneck_points = vec![
            BottleneckPoint::ConnectionPool,
            BottleneckPoint::Cache,
            BottleneckPoint::PlanCache,
        ];
        let mut optimization_directions = Vec::new();

        for result in &baseline.results {
            if result.p95_us > max_p95 {
                max_p95 = result.p95_us;
                bottleneck_workloads = vec![result.workload];
            } else if result.p95_us == max_p95
                && max_p95 > 0.0
                && !bottleneck_workloads.contains(&result.workload)
            {
                bottleneck_workloads.push(result.workload);
            }
        }

        if max_p95 > 1000.0 {
            optimization_directions.push("连接池预热 + 连接复用率提升".into());
            optimization_directions.push("查询计划缓存命中率提升至 ≥92%".into());
        }
        if max_p95 > 500.0 {
            bottleneck_points.push(BottleneckPoint::Simd);
            optimization_directions.push("SIMD 向量化加速（目标 ≥1.8x）".into());
        }
        if max_p95 > 200.0 {
            bottleneck_points.push(BottleneckPoint::ZeroCopy);
            optimization_directions.push("零拷贝覆盖扩展（目标 ≥70% 堆分配降低）".into());
        }
        if optimization_directions.is_empty() {
            optimization_directions.push("当前性能良好，无显著瓶颈".into());
        }

        BottleneckReport {
            bottleneck_workloads,
            bottleneck_points,
            optimization_directions,
        }
    }
}
