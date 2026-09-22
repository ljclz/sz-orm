//! 基准结果导出器（v8.1.0 任务 2.3，feature gate: `bench-real-db`）
//!
//! 结果序列化 → 导出 JSON/CSV → 含时间戳/DB 版本/workload/指标（QPS/延迟/资源占用）→ 支持历史对比。

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{BenchError, BenchResult, FrameworkType, WorkloadType};

/// 导出格式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExportFormat {
    Json,
    Csv,
}

impl ExportFormat {
    /// 文件扩展名
    pub fn extension(&self) -> &'static str {
        match self {
            ExportFormat::Json => "json",
            ExportFormat::Csv => "csv",
        }
    }
}

impl std::str::FromStr for ExportFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "json" => Ok(ExportFormat::Json),
            "csv" => Ok(ExportFormat::Csv),
            _ => Err(format!("不支持的导出格式: {s}")),
        }
    }
}

/// 导出的基准结果记录（含元数据）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportedBenchRecord {
    /// 导出时间戳（Unix 毫秒）
    pub exported_at_ms: u64,
    /// 数据库版本
    pub db_version: String,
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
    pub throughput_qps: f64,
    /// 峰值 RSS（KB）
    pub peak_rss_kb: u64,
    /// 是否真实 DB
    pub is_real_db: bool,
}

impl ExportedBenchRecord {
    /// 从基准结果构建导出记录
    pub fn from_bench_result(result: &BenchResult, db_version: &str) -> Self {
        Self {
            exported_at_ms: now_ms(),
            db_version: db_version.to_string(),
            framework: result.framework,
            workload: result.workload,
            p50_us: result.p50_us,
            p95_us: result.p95_us,
            p99_us: result.p99_us,
            throughput_qps: result.throughput_ops,
            peak_rss_kb: result.peak_rss_kb,
            is_real_db: result.is_real_db,
        }
    }
}

/// 基准结果导出器
#[derive(Debug, Clone)]
pub struct BenchResultExporter {
    /// 导出根目录
    export_dir: PathBuf,
    /// 数据库版本
    db_version: String,
}

impl BenchResultExporter {
    /// 创建导出器：指定导出目录和 DB 版本
    pub fn new(export_dir: impl Into<PathBuf>, db_version: impl Into<String>) -> Self {
        Self {
            export_dir: export_dir.into(),
            db_version: db_version.into(),
        }
    }

    /// 导出单个基准结果到指定格式
    pub async fn export(
        &self,
        result: &BenchResult,
        format: ExportFormat,
    ) -> Result<PathBuf, BenchError> {
        let record = ExportedBenchRecord::from_bench_result(result, &self.db_version);
        let content = match format {
            ExportFormat::Json => serde_json::to_string_pretty(&record)
                .map_err(|e| BenchError::QueryFailed(e.to_string()))?,
            ExportFormat::Csv => record_to_csv(&record),
        };
        let file_path = self.make_path(result, format);
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| BenchError::ExportPathNotWritable(e.to_string()))?;
        }
        std::fs::write(&file_path, content)
            .map_err(|e| BenchError::ExportPathNotWritable(e.to_string()))?;
        Ok(file_path)
    }

    /// 导出多个基准结果（合并为一个文件）
    pub async fn export_batch(
        &self,
        results: &[BenchResult],
        format: ExportFormat,
    ) -> Result<PathBuf, BenchError> {
        if results.is_empty() {
            return Err(BenchError::ConfigMissing("results 为空".into()));
        }
        let records: Vec<ExportedBenchRecord> = results
            .iter()
            .map(|r| ExportedBenchRecord::from_bench_result(r, &self.db_version))
            .collect();
        let content = match format {
            ExportFormat::Json => serde_json::to_string_pretty(&records)
                .map_err(|e| BenchError::QueryFailed(e.to_string()))?,
            ExportFormat::Csv => records_to_csv(&records),
        };
        let file_path = self.make_batch_path(format);
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| BenchError::ExportPathNotWritable(e.to_string()))?;
        }
        std::fs::write(&file_path, content)
            .map_err(|e| BenchError::ExportPathNotWritable(e.to_string()))?;
        Ok(file_path)
    }

    /// 与历史导出文件对比：返回 QPS 退化百分比（正值=退化，负值=改善）
    pub fn compare_with_history(
        &self,
        current: &BenchResult,
        history_path: &PathBuf,
    ) -> Result<f64, BenchError> {
        let content = std::fs::read_to_string(history_path)
            .map_err(|e| BenchError::QueryFailed(e.to_string()))?;
        let history: ExportedBenchRecord =
            serde_json::from_str(&content).map_err(|e| BenchError::QueryFailed(e.to_string()))?;
        if history.workload != current.workload || history.framework != current.framework {
            return Err(BenchError::IncomparableWorkload(current.workload));
        }
        if history.throughput_qps > 0.0 {
            Ok(
                ((history.throughput_qps - current.throughput_ops) / history.throughput_qps)
                    * 100.0,
            )
        } else {
            Ok(0.0)
        }
    }

    fn make_path(&self, result: &BenchResult, format: ExportFormat) -> PathBuf {
        let filename = format!(
            "bench_{}_{}_{}.{}",
            result.framework.as_str(),
            result.workload.as_str(),
            now_ms(),
            format.extension()
        );
        self.export_dir.join(filename)
    }

    fn make_batch_path(&self, format: ExportFormat) -> PathBuf {
        let filename = format!("bench_batch_{}.{}", now_ms(), format.extension());
        self.export_dir.join(filename)
    }
}

fn record_to_csv(record: &ExportedBenchRecord) -> String {
    format!(
        "exported_at_ms,db_version,framework,workload,p50_us,p95_us,p99_us,throughput_qps,peak_rss_kb,is_real_db\n\
         {exported_at_ms},{db_version},{framework},{workload},{p50_us},{p95_us},{p99_us},{throughput_qps},{peak_rss_kb},{is_real_db}",
        exported_at_ms = record.exported_at_ms,
        db_version = record.db_version,
        framework = record.framework.as_str(),
        workload = record.workload.as_str(),
        p50_us = record.p50_us,
        p95_us = record.p95_us,
        p99_us = record.p99_us,
        throughput_qps = record.throughput_qps,
        peak_rss_kb = record.peak_rss_kb,
        is_real_db = record.is_real_db,
    )
}

fn records_to_csv(records: &[ExportedBenchRecord]) -> String {
    let mut output =
        String::from("exported_at_ms,db_version,framework,workload,p50_us,p95_us,p99_us,throughput_qps,peak_rss_kb,is_real_db\n");
    for r in records {
        output.push_str(&format!(
            "{exported_at_ms},{db_version},{framework},{workload},{p50_us},{p95_us},{p99_us},{throughput_qps},{peak_rss_kb},{is_real_db}\n",
            exported_at_ms = r.exported_at_ms,
            db_version = r.db_version,
            framework = r.framework.as_str(),
            workload = r.workload.as_str(),
            p50_us = r.p50_us,
            p95_us = r.p95_us,
            p99_us = r.p99_us,
            throughput_qps = r.throughput_qps,
            peak_rss_kb = r.peak_rss_kb,
            is_real_db = r.is_real_db,
        ));
    }
    output
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 渲染基准 Prometheus 指标（任务 2.6）
///
/// 暴露指标：
/// - `sz_orm_bench_latency_ms`（histogram，基准执行延迟分桶）
/// - `sz_orm_bench_qps`（gauge，基准 QPS）
/// - `sz_orm_bench_deviation_pct`（gauge，可重复性偏差）
/// - `sz_orm_perf_regression_degradation_pct`（gauge，CI 回归退化率）
pub fn render_bench_prometheus_metrics(
    results: &[BenchResult],
    deviation_pct: Option<f64>,
    degradation_pct: Option<f64>,
) -> String {
    let mut output = String::new();

    // sz_orm_bench_latency_ms histogram
    output.push_str("# HELP sz_orm_bench_latency_ms Benchmark execution latency in ms\n");
    output.push_str("# TYPE sz_orm_bench_latency_ms histogram\n");
    let buckets = [0.1, 0.5, 1.0, 5.0, 10.0, 50.0, 100.0, 500.0, 1000.0];
    for bucket in &buckets {
        let count = results
            .iter()
            .filter(|r| r.p95_us / 1000.0 <= *bucket)
            .count();
        output.push_str(&format!(
            "sz_orm_bench_latency_ms_bucket{{le=\"{bucket}\"}} {count}\n"
        ));
    }
    output.push_str(&format!(
        "sz_orm_bench_latency_ms_bucket{{le=\"+Inf\"}} {}\n",
        results.len()
    ));
    let sum: f64 = results.iter().map(|r| r.p95_us / 1000.0).sum();
    output.push_str(&format!("sz_orm_bench_latency_ms_sum {sum}\n"));
    output.push_str(&format!(
        "sz_orm_bench_latency_ms_count {}\n",
        results.len()
    ));

    // sz_orm_bench_qps gauge
    output.push_str("# HELP sz_orm_bench_qps Benchmark throughput in QPS\n");
    output.push_str("# TYPE sz_orm_bench_qps gauge\n");
    for r in results {
        output.push_str(&format!(
            "sz_orm_bench_qps{{framework=\"{}\",workload=\"{}\"}} {}\n",
            r.framework.as_str(),
            r.workload.as_str(),
            r.throughput_ops
        ));
    }

    // sz_orm_bench_deviation_pct gauge
    if let Some(dev) = deviation_pct {
        output.push_str(
            "# HELP sz_orm_bench_deviation_pct Benchmark repeatability deviation percentage\n",
        );
        output.push_str("# TYPE sz_orm_bench_deviation_pct gauge\n");
        output.push_str(&format!("sz_orm_bench_deviation_pct {dev}\n"));
    }

    // sz_orm_perf_regression_degradation_pct gauge
    if let Some(deg) = degradation_pct {
        output.push_str(
            "# HELP sz_orm_perf_regression_degradation_pct CI regression degradation percentage\n",
        );
        output.push_str("# TYPE sz_orm_perf_regression_degradation_pct gauge\n");
        output.push_str(&format!("sz_orm_perf_regression_degradation_pct {deg}\n"));
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BenchResult, FrameworkType, WorkloadType};

    fn make_result() -> BenchResult {
        BenchResult::from_latencies(
            FrameworkType::SzOrm,
            WorkloadType::SingleRowQuery,
            vec![100, 200, 300, 400, 500],
            1024,
            10,
            4096,
            true,
        )
    }

    fn temp_export_dir() -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "sz_orm_bench_export_test_{}_{}_{}",
            std::process::id(),
            now_ms(),
            id
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn cleanup(dir: &PathBuf) {
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn test_export_json() {
        let dir = temp_export_dir();
        let exporter = BenchResultExporter::new(&dir, "mysql 9.6");
        let result = make_result();
        let path = exporter.export(&result, ExportFormat::Json).await.unwrap();
        assert!(path.extension().unwrap() == "json");
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("mysql 9.6"));
        assert!(content.contains("sz-orm"));
        assert!(content.contains("single_row_query"));
        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_export_csv() {
        let dir = temp_export_dir();
        let exporter = BenchResultExporter::new(&dir, "mysql 9.6");
        let result = make_result();
        let path = exporter.export(&result, ExportFormat::Csv).await.unwrap();
        assert!(path.extension().unwrap() == "csv");
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("exported_at_ms,db_version"));
        assert!(content.contains("mysql 9.6"));
        assert!(content.contains("sz-orm"));
        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_export_batch_json() {
        let dir = temp_export_dir();
        let exporter = BenchResultExporter::new(&dir, "mysql 9.6");
        let results = vec![make_result(); 3];
        let path = exporter
            .export_batch(&results, ExportFormat::Json)
            .await
            .unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.starts_with('['));
        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_export_batch_csv() {
        let dir = temp_export_dir();
        let exporter = BenchResultExporter::new(&dir, "mysql 9.6");
        let results = vec![make_result(); 3];
        let path = exporter
            .export_batch(&results, ExportFormat::Csv)
            .await
            .unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        let line_count = content.lines().count();
        assert_eq!(line_count, 4); // header + 3 rows
        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_export_batch_empty_rejected() {
        let dir = temp_export_dir();
        let exporter = BenchResultExporter::new(&dir, "mysql 9.6");
        let results: Vec<BenchResult> = vec![];
        let err = exporter
            .export_batch(&results, ExportFormat::Json)
            .await
            .unwrap_err();
        assert!(matches!(err, BenchError::ConfigMissing(_)));
        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_compare_with_history() {
        let dir = temp_export_dir();
        let exporter = BenchResultExporter::new(&dir, "mysql 9.6");
        let mut history = make_result();
        history.throughput_ops = 10000.0;
        let history_path = exporter.export(&history, ExportFormat::Json).await.unwrap();

        let mut current = make_result();
        current.throughput_ops = 8000.0;
        let degradation = exporter
            .compare_with_history(&current, &history_path)
            .unwrap();
        assert!((degradation - 20.0).abs() < 0.01);
        cleanup(&dir);
    }

    #[tokio::test]
    async fn test_compare_with_history_improvement() {
        let dir = temp_export_dir();
        let exporter = BenchResultExporter::new(&dir, "mysql 9.6");
        let mut history = make_result();
        history.throughput_ops = 8000.0;
        let history_path = exporter.export(&history, ExportFormat::Json).await.unwrap();

        let mut current = make_result();
        current.throughput_ops = 10000.0;
        let degradation = exporter
            .compare_with_history(&current, &history_path)
            .unwrap();
        assert!(degradation < 0.0); // 负值=改善
        cleanup(&dir);
    }

    #[test]
    fn test_export_format_extension() {
        assert_eq!(ExportFormat::Json.extension(), "json");
        assert_eq!(ExportFormat::Csv.extension(), "csv");
    }

    #[test]
    fn test_export_format_from_str() {
        use std::str::FromStr;
        assert_eq!(ExportFormat::from_str("json").unwrap(), ExportFormat::Json);
        assert_eq!(ExportFormat::from_str("CSV").unwrap(), ExportFormat::Csv);
        assert!(ExportFormat::from_str("xml").is_err());
    }

    #[test]
    fn test_render_bench_prometheus_metrics() {
        let results = vec![make_result(); 2];
        let output = render_bench_prometheus_metrics(&results, Some(3.5), Some(15.0));
        assert!(output.contains("sz_orm_bench_latency_ms_bucket"));
        assert!(output.contains("sz_orm_bench_qps{framework=\"sz-orm\""));
        assert!(output.contains("sz_orm_bench_deviation_pct 3.5"));
        assert!(output.contains("sz_orm_perf_regression_degradation_pct 15"));
    }

    #[test]
    fn test_render_bench_prometheus_metrics_empty() {
        let output = render_bench_prometheus_metrics(&[], None, None);
        assert!(output.contains("sz_orm_bench_latency_ms_count 0"));
        assert!(!output.contains("sz_orm_bench_deviation_pct"));
    }
}
