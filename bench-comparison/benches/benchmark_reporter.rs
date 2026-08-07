//! BenchmarkReporter — 基准报告生成器（v2.3.0 任务 B）
//!
//! 聚合 criterion 输出，生成公开基准报告（Markdown + CSV/JSON + 环境元数据 + DSN 脱敏）。

use serde::Serialize;

/// 基准测量记录
#[derive(Debug, Clone, Serialize)]
pub struct BenchmarkRecord {
    pub dimension: String,
    pub dialect: String,
    pub competitor: String,
    pub mean_ns: f64,
    pub median_ns: f64,
    pub p95_ns: f64,
    pub throughput_ops_per_sec: f64,
    pub dataset_size: usize,
}

/// 环境元数据
#[derive(Debug, Clone, Serialize)]
pub struct EnvironmentMetadata {
    pub cpu: String,
    pub memory_gb: f64,
    pub disk: String,
    pub rust_version: String,
    pub db_versions: Vec<String>,
    pub criterion_config: CriterionConfig,
    pub dataset_sizes: Vec<usize>,
}

/// criterion 配置
#[derive(Debug, Clone, Serialize)]
pub struct CriterionConfig {
    pub sample_size: usize,
    pub warm_up_time: String,
    pub measurement_time: String,
    pub confidence_level: f64,
    pub noise_threshold: f64,
}

impl Default for CriterionConfig {
    fn default() -> Self {
        Self {
            sample_size: 100,
            warm_up_time: "3s".to_string(),
            measurement_time: "10s".to_string(),
            confidence_level: 0.95,
            noise_threshold: 0.05,
        }
    }
}

/// 异常值检测报告
#[derive(Debug, Clone, Serialize)]
pub struct AuditReport {
    pub anomalies: Vec<String>,
    pub missing_dimensions: Vec<String>,
    pub is_clean: bool,
}

/// 基准报告生成器
#[derive(Debug, Clone)]
pub struct BenchmarkReporter {
    records: Vec<BenchmarkRecord>,
    environment: EnvironmentMetadata,
}

impl BenchmarkReporter {
    pub fn new(environment: EnvironmentMetadata) -> Self {
        Self {
            records: Vec::new(),
            environment,
        }
    }

    pub fn add_record(&mut self, record: BenchmarkRecord) {
        self.records.push(record);
    }

    /// 生成 Markdown 报告
    pub fn generate_markdown(&self) -> String {
        let mut md = String::new();

        md.push_str("# sz-orm v2.3.0 性能基准报告\n\n");
        md.push_str(&format!(
            "> 生成日期：{}\n\n",
            chrono::Utc::now().format("%Y-%m-%d")
        ));

        md.push_str("## 环境元数据\n\n");
        md.push_str(&format!("- CPU: {}\n", self.environment.cpu));
        md.push_str(&format!("- 内存: {} GB\n", self.environment.memory_gb));
        md.push_str(&format!("- 磁盘: {}\n", self.environment.disk));
        md.push_str(&format!("- Rust 版本: {}\n", self.environment.rust_version));
        md.push_str("- 数据库版本:\n");
        for v in &self.environment.db_versions {
            md.push_str(&format!("  - {}\n", v));
        }
        md.push_str(&format!(
            "- criterion 配置: sample_size={}, warm_up={}, measurement={}, confidence={}, noise={}\n\n",
            self.environment.criterion_config.sample_size,
            self.environment.criterion_config.warm_up_time,
            self.environment.criterion_config.measurement_time,
            self.environment.criterion_config.confidence_level,
            self.environment.criterion_config.noise_threshold
        ));

        md.push_str("## 基准结果\n\n");
        md.push_str("| 维度 | 方言 | 竞品 | 数据集规模 | 均值(ns) | 中位数(ns) | P95(ns) | 吞吐量(ops/s) |\n");
        md.push_str(
            "|------|------|------|-----------|---------|-----------|---------|---------------|\n",
        );
        for r in &self.records {
            md.push_str(&format!(
                "| {} | {} | {} | {} | {:.0} | {:.0} | {:.0} | {:.1} |\n",
                r.dimension,
                r.dialect,
                r.competitor,
                r.dataset_size,
                r.mean_ns,
                r.median_ns,
                r.p95_ns,
                r.throughput_ops_per_sec
            ));
        }

        md.push_str("\n## 差异说明\n\n");
        md.push_str("- Diesel 为同步 ORM，与 sz-orm（异步）非对等比较\n");
        md.push_str("- SQLx 为底层驱动，无 ORM 级关联抽象\n");
        md.push_str("- SeaORM SmartLoader 与 sz-orm SmartEagerLoader 策略选择差异\n");

        md.push_str("\n## 复现指令\n\n");
        md.push_str("```bash\n");
        md.push_str("cargo bench --bench full_comparison\n");
        md.push_str("```\n");

        md
    }

    /// 生成 CSV 图表数据
    pub fn generate_csv(&self) -> String {
        let mut csv = String::from("dimension,dialect,competitor,dataset_size,mean_ns,median_ns,p95_ns,throughput_ops_per_sec\n");
        for r in &self.records {
            csv.push_str(&format!(
                "{},{},{},{},{},{},{},{}\n",
                r.dimension,
                r.dialect,
                r.competitor,
                r.dataset_size,
                r.mean_ns,
                r.median_ns,
                r.p95_ns,
                r.throughput_ops_per_sec
            ));
        }
        csv
    }

    /// 生成 JSON 图表数据
    pub fn generate_json(&self) -> String {
        serde_json::to_string_pretty(&self.records).unwrap_or_else(|_| "[]".to_string())
    }

    /// DSN 脱敏（密码替换为 ***）
    pub fn mask_dsn(dsn: &str) -> String {
        if let Some(at_pos) = dsn.find('@') {
            if let Some(scheme_end) = dsn.find("://") {
                let scheme = &dsn[..scheme_end + 3];
                let rest = &dsn[scheme_end + 3..];
                if let Some(colon_pos) = rest.find(':') {
                    if colon_pos < at_pos - (scheme_end + 3) {
                        let user = &rest[..colon_pos];
                        let after_pw = &rest[at_pos - (scheme_end + 3)..];
                        return format!("{}{}:***{}", scheme, user, after_pw);
                    }
                }
            }
        }
        dsn.to_string()
    }

    /// 异常值检测
    pub fn audit(&self) -> AuditReport {
        let mut anomalies = Vec::new();
        let mut missing_dimensions = Vec::new();

        for r in &self.records {
            if r.mean_ns == 0.0 {
                anomalies.push(format!(
                    "维度 {} 竞品 {} 均值为 0",
                    r.dimension, r.competitor
                ));
            }
            if r.throughput_ops_per_sec < 0.0 {
                anomalies.push(format!(
                    "维度 {} 竞品 {} 吞吐量为负",
                    r.dimension, r.competitor
                ));
            }
        }

        let expected_dims = [
            "crud_single",
            "crud_batch",
            "relation_has_one",
            "relation_has_many",
            "relation_many_to_many",
            "transaction",
            "pool",
            "pagination",
        ];
        for dim in &expected_dims {
            if !self.records.iter().any(|r| r.dimension == *dim) {
                missing_dimensions.push(dim.to_string());
            }
        }

        let is_clean = anomalies.is_empty() && missing_dimensions.is_empty();
        AuditReport {
            anomalies,
            missing_dimensions,
            is_clean,
        }
    }

    /// 生成复现指令
    pub fn generate_repro_instructions(&self) -> String {
        let mut instructions = String::new();
        instructions.push_str("# 复现步骤\n\n");
        instructions.push_str("## 前置条件\n");
        instructions.push_str("- Rust 工具链 (rustc 1.81+)\n");
        instructions.push_str("- SQLite（in-memory，始终运行）\n");
        instructions.push_str("- MySQL（设置 DATABASE_URL_MYSQL 环境变量）\n");
        instructions.push_str("- PostgreSQL（设置 DATABASE_URL_POSTGRES 环境变量）\n\n");
        instructions.push_str("## 运行命令\n\n");
        instructions.push_str("```bash\n");
        instructions.push_str("# SQLite only（默认）\n");
        instructions.push_str("cargo bench --bench full_comparison\n\n");
        instructions.push_str("# MySQL + PostgreSQL\n");
        instructions.push_str("export DATABASE_URL_MYSQL=mysql://root:***@127.0.0.1:3306/bench\n");
        instructions.push_str(
            "export DATABASE_URL_POSTGRES=postgres://postgres:***@127.0.0.1:5432/bench\n",
        );
        instructions.push_str("cargo bench --bench full_comparison\n");
        instructions.push_str("```\n");
        instructions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mask_dsn_with_password() {
        let dsn = "mysql://root:test123@127.0.0.1:3306/bench";
        let masked = BenchmarkReporter::mask_dsn(dsn);
        assert!(masked.contains("***"));
        assert!(!masked.contains("test123"));
    }

    #[test]
    fn test_mask_dsn_without_password() {
        let dsn = "sqlite://:memory:";
        let masked = BenchmarkReporter::mask_dsn(dsn);
        assert_eq!(masked, dsn);
    }

    #[test]
    fn test_audit_clean() {
        let env = EnvironmentMetadata {
            cpu: "test".to_string(),
            memory_gb: 16.0,
            disk: "ssd".to_string(),
            rust_version: "1.81".to_string(),
            db_versions: vec![],
            criterion_config: CriterionConfig::default(),
            dataset_sizes: vec![100],
        };
        let reporter = BenchmarkReporter::new(env);
        let audit = reporter.audit();
        assert!(!audit.is_clean);
        assert!(!audit.missing_dimensions.is_empty());
    }

    #[test]
    fn test_generate_markdown() {
        let env = EnvironmentMetadata {
            cpu: "test".to_string(),
            memory_gb: 16.0,
            disk: "ssd".to_string(),
            rust_version: "1.81".to_string(),
            db_versions: vec![],
            criterion_config: CriterionConfig::default(),
            dataset_sizes: vec![100],
        };
        let mut reporter = BenchmarkReporter::new(env);
        reporter.add_record(BenchmarkRecord {
            dimension: "crud_single".to_string(),
            dialect: "sqlite".to_string(),
            competitor: "sz-orm".to_string(),
            mean_ns: 1000.0,
            median_ns: 950.0,
            p95_ns: 1500.0,
            throughput_ops_per_sec: 1000000.0,
            dataset_size: 100,
        });
        let md = reporter.generate_markdown();
        assert!(md.contains("# sz-orm v2.3.0"));
        assert!(md.contains("crud_single"));
    }

    #[test]
    fn test_generate_csv() {
        let env = EnvironmentMetadata {
            cpu: "test".to_string(),
            memory_gb: 16.0,
            disk: "ssd".to_string(),
            rust_version: "1.81".to_string(),
            db_versions: vec![],
            criterion_config: CriterionConfig::default(),
            dataset_sizes: vec![100],
        };
        let reporter = BenchmarkReporter::new(env);
        let csv = reporter.generate_csv();
        assert!(csv.contains("dimension,dialect,competitor"));
    }
}
