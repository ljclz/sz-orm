//! # 生产就绪检查清单执行器（`prod-ready` feature）
//!
//! 聚合 M1-M4 所有 15 项检查（REQ-PROD-001~015）。**v10.13.0（2026-10-28 架构评测
//! P1-1 真实化）起，检查为配置驱动真实验证**：调用方把实际生产配置填入
//! [`ProdReadyCheckerConfig`]，`run()` 逐项执行真实约束校验，汇总
//! PASS/FAIL/SKIPPED + 证据，生成检查报告。
//!
//! ## 检查项映射
//!
//! | REQ | 类别 | 检查项 | 验证方式 |
//! |-----|------|--------|---------|
//! | REQ-PROD-001 | SafetyRedline | 配置脱敏验证 | `config_masking_enabled == Some(true)` |
//! | REQ-PROD-002 | SafetyRedline | Redis TLS 验证 | `redis_tls_enabled == Some(true)` |
//! | REQ-PROD-003 | SafetyRedline | JWT 密钥轮换 | `jwt_key_rotation_days ∈ [1, 90]` |
//! | REQ-PROD-004 | SafetyRedline | 限流配置验证 | `rate_limit_enabled == Some(true)` |
//! | REQ-PROD-005 | SafetyRedline | 熔断器配置验证 | `circuit_breaker_enabled == Some(true)` |
//! | REQ-PROD-006 | ConfigObservability | 日志级别验证 | `log_level` ∈ {error, warn, info} |
//! | REQ-PROD-007 | SafetyRedline | metrics ACL 验证 | `metrics_acl_enabled == Some(true)` |
//! | REQ-PROD-008 | ConfigObservability | 健康端点配置验证 | `health_endpoint_enabled == Some(true)` |
//! | REQ-PROD-009 | ConfigObservability | 优雅关闭超时 | `shutdown_timeout_secs ∈ [1, 3600]` |
//! | REQ-PROD-010 | ConfigObservability | K8s 探针端点配置 | `probe_endpoint_enabled == Some(true)` |
//! | REQ-PROD-011 | SafetyRedline | SQL 注入扫描 | `sql_injection_scan_passed == Some(true)`（由门禁 9 结果喂入） |
//! | REQ-PROD-012 | OrmProtection | 连接泄漏检测配置 | `leak_detection_enabled == Some(true)` |
//! | REQ-PROD-013 | OrmProtection | N+1 检测配置 | `n1_detection_enabled == Some(true)` |
//! | REQ-PROD-014 | ThresholdTuning | 连接池参数验证 | `pool_max_connections >= pool_min_idle > 0` 且 max ≤ 10000 |
//! | REQ-PROD-015 | OrmProtection | 五方言安全验证 | `dialect_security_verified == Some(true)` |
//!
//! ## 诚实声明（需手动接入）
//!
//! 本检查器**不会**自动读取运行环境或连接外部服务：对应配置项为 `None` 时该项
//! 记为 `Skipped`（而非历史版本的硬编码 PASS——2026-10-27 全栈/安全评测均指出
//! 旧实现"15 项全 Pass"是自证合格的幻影，本版本废除）。如需 CI/CD 集成，请在
//! 部署流水线中用真实配置构造 `ProdReadyCheckerConfig` 后调用 `run()`。

use serde::{Deserialize, Serialize};

/// 检查类别
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckCategory {
    /// M1: 安全红线
    SafetyRedline,
    /// M2: 配置可观测
    ConfigObservability,
    /// M3: 阈值调优
    ThresholdTuning,
    /// M4: ORM 防护
    OrmProtection,
}

/// 检查状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckStatus {
    /// 通过
    Pass,
    /// 失败
    Fail,
    /// 跳过（对应配置项未提供——需手动接入，见模块文档"诚实声明"）
    Skipped,
    /// 不适用
    NotApplicable,
}

/// 单项检查结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckItemResult {
    /// 检查项 ID（如 "REQ-PROD-001"）
    pub id: String,
    /// 检查项名称
    pub name: String,
    /// 检查类别
    pub category: CheckCategory,
    /// 检查状态
    pub status: CheckStatus,
    /// 证据列表（配置值 / 约束表达式）
    pub evidence: Vec<String>,
    /// 时间戳（ISO 8601）
    pub timestamp: String,
    /// 失败原因（status=Fail 时填充）
    pub failure_reason: Option<String>,
}

/// 报告摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportSummary {
    /// 总检查数
    pub total: u32,
    /// 通过数
    pub pass: u32,
    /// 失败数
    pub fail: u32,
    /// 跳过数
    pub skipped: u32,
}

/// 生产就绪检查报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProdReadyReport {
    /// 各检查项结果
    pub items: Vec<CheckItemResult>,
    /// 报告摘要
    pub summary: ReportSummary,
}

impl ProdReadyReport {
    /// 所有检查是否全部通过（Skipped/NotApplicable 视为非失败）
    ///
    /// 注意：`all_pass() == true` **不等于**生产就绪——若存在 Skipped 项，说明
    /// 对应配置未接入检查。严格判定请用 [`ProdReadyReport::all_pass_strict`]。
    pub fn all_pass(&self) -> bool {
        self.items
            .iter()
            .all(|item| item.status != CheckStatus::Fail)
    }

    /// 严格判定：无 FAIL 且无 SKIPPED（所有检查均真实执行且通过）
    pub fn all_pass_strict(&self) -> bool {
        self.items.iter().all(|item| {
            item.status == CheckStatus::Pass || item.status == CheckStatus::NotApplicable
        })
    }

    /// 序列化为 JSON 字符串（供 CI/CD 集成）
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

/// 检查项 trait（扩展性：新增检查项仅需实现此 trait）
pub trait CheckItem: Send + Sync {
    /// 检查项 ID
    fn id(&self) -> &str;
    /// 检查项名称
    fn name(&self) -> &str;
    /// 检查类别
    fn category(&self) -> CheckCategory;
    /// 执行检查（配置驱动：从 config 读取对应字段做真实约束校验）
    fn run(&self, config: &ProdReadyCheckerConfig) -> CheckItemResult;
}

/// 生产就绪检查器配置
///
/// 除启用/跳过清单外，其余字段为各检查项的**真实配置输入**：
/// 填 `Some(实际值)` 才会执行该项校验；`None` 该项记 `Skipped`。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProdReadyCheckerConfig {
    /// 启用的检查项 ID 列表（空表示全部启用）
    #[serde(default)]
    pub enabled_checks: Vec<String>,
    /// 跳过的检查项 ID 列表
    #[serde(default)]
    pub skipped_checks: Vec<String>,
    /// REQ-PROD-001：配置脱敏已启用（sz-orm-config masker）
    #[serde(default)]
    pub config_masking_enabled: Option<bool>,
    /// REQ-PROD-002：Redis TLS 已启用
    #[serde(default)]
    pub redis_tls_enabled: Option<bool>,
    /// REQ-PROD-003：JWT 密钥轮换周期（天）；生产建议 1~90
    #[serde(default)]
    pub jwt_key_rotation_days: Option<u32>,
    /// REQ-PROD-004：限流器已启用
    #[serde(default)]
    pub rate_limit_enabled: Option<bool>,
    /// REQ-PROD-005：熔断器已启用
    #[serde(default)]
    pub circuit_breaker_enabled: Option<bool>,
    /// REQ-PROD-006：运行时日志级别（error/warn/info 合格；debug/trace 生产不合格）
    #[serde(default)]
    pub log_level: Option<String>,
    /// REQ-PROD-007：metrics 端点 ACL 已启用
    #[serde(default)]
    pub metrics_acl_enabled: Option<bool>,
    /// REQ-PROD-008：健康端点已启用
    #[serde(default)]
    pub health_endpoint_enabled: Option<bool>,
    /// REQ-PROD-009：优雅关闭超时（秒）；生产建议 1~3600
    #[serde(default)]
    pub shutdown_timeout_secs: Option<u64>,
    /// REQ-PROD-010：K8s liveness/readiness 探针端点已启用
    #[serde(default)]
    pub probe_endpoint_enabled: Option<bool>,
    /// REQ-PROD-011：SQL 注入扫描已通过（门禁 9 `check-sql-injection-auto.py --strict` 结果）
    #[serde(default)]
    pub sql_injection_scan_passed: Option<bool>,
    /// REQ-PROD-012：连接泄漏检测已启用
    #[serde(default)]
    pub leak_detection_enabled: Option<bool>,
    /// REQ-PROD-013：N+1 检测已接入（N1QueryDetector 或 `#[detect_n_plus_one]`）
    #[serde(default)]
    pub n1_detection_enabled: Option<bool>,
    /// REQ-PROD-014：连接池最大连接数
    #[serde(default)]
    pub pool_max_connections: Option<u32>,
    /// REQ-PROD-014：连接池最小空闲连接数
    #[serde(default)]
    pub pool_min_idle: Option<u32>,
    /// REQ-PROD-015：五方言安全验证（TLS/鉴权/脱敏）已通过
    #[serde(default)]
    pub dialect_security_verified: Option<bool>,
}

/// 生产就绪检查器
pub struct ProdReadyChecker {
    config: ProdReadyCheckerConfig,
    items: Vec<Box<dyn CheckItem>>,
}

impl ProdReadyChecker {
    /// 创建检查器，注册所有 15 项检查
    pub fn new(config: ProdReadyCheckerConfig) -> Self {
        let items: Vec<Box<dyn CheckItem>> = vec![
            Box::new(ReqProd001),
            Box::new(ReqProd002),
            Box::new(ReqProd003),
            Box::new(ReqProd004),
            Box::new(ReqProd005),
            Box::new(ReqProd006),
            Box::new(ReqProd007),
            Box::new(ReqProd008),
            Box::new(ReqProd009),
            Box::new(ReqProd010),
            Box::new(ReqProd011),
            Box::new(ReqProd012),
            Box::new(ReqProd013),
            Box::new(ReqProd014),
            Box::new(ReqProd015),
        ];
        Self { config, items }
    }

    /// 执行所有检查，生成报告
    pub fn run(&self) -> ProdReadyReport {
        let mut results = Vec::new();
        let timestamp = chrono::Utc::now().to_rfc3339();

        for item in &self.items {
            let id = item.id();
            let mut result = item.run(&self.config);
            result.timestamp = timestamp.clone();

            if self.config.skipped_checks.iter().any(|s| s == id) {
                result.status = CheckStatus::Skipped;
                result.failure_reason = Some("skipped by config".to_string());
            }

            if !self.config.enabled_checks.is_empty()
                && !self.config.enabled_checks.iter().any(|s| s == id)
            {
                result.status = CheckStatus::Skipped;
                result.failure_reason = Some("not in enabled_checks".to_string());
            }

            results.push(result);
        }

        let summary = ReportSummary {
            total: results.len() as u32,
            pass: results
                .iter()
                .filter(|r| r.status == CheckStatus::Pass)
                .count() as u32,
            fail: results
                .iter()
                .filter(|r| r.status == CheckStatus::Fail)
                .count() as u32,
            skipped: results
                .iter()
                .filter(|r| {
                    r.status == CheckStatus::Skipped || r.status == CheckStatus::NotApplicable
                })
                .count() as u32,
        };

        ProdReadyReport {
            items: results,
            summary,
        }
    }
}

fn pass_result(
    id: &str,
    name: &str,
    category: CheckCategory,
    evidence: Vec<String>,
) -> CheckItemResult {
    CheckItemResult {
        id: id.to_string(),
        name: name.to_string(),
        category,
        status: CheckStatus::Pass,
        evidence,
        timestamp: String::new(),
        failure_reason: None,
    }
}

fn fail_result(
    id: &str,
    name: &str,
    category: CheckCategory,
    evidence: Vec<String>,
    reason: String,
) -> CheckItemResult {
    CheckItemResult {
        id: id.to_string(),
        name: name.to_string(),
        category,
        status: CheckStatus::Fail,
        evidence,
        timestamp: String::new(),
        failure_reason: Some(reason),
    }
}

fn skipped_result(id: &str, name: &str, category: CheckCategory) -> CheckItemResult {
    CheckItemResult {
        id: id.to_string(),
        name: name.to_string(),
        category,
        status: CheckStatus::Skipped,
        evidence: vec![
            "对应配置项为 None（需手动接入：填充 ProdReadyCheckerConfig 后重跑）".to_string(),
        ],
        timestamp: String::new(),
        failure_reason: None,
    }
}

/// bool 型开关检查：None → Skipped；Some(true) → Pass；Some(false) → Fail
fn check_bool_flag(
    id: &str,
    name: &str,
    category: CheckCategory,
    value: Option<bool>,
    field: &str,
) -> CheckItemResult {
    match value {
        None => skipped_result(id, name, category),
        Some(true) => pass_result(id, name, category, vec![format!("{} = true", field)]),
        Some(false) => fail_result(
            id,
            name,
            category,
            vec![format!("{} = false", field)],
            format!("{} 必须为 true 才能通过生产就绪检查", field),
        ),
    }
}

// ===== REQ-PROD-001 ~ 015（v10.13.0 配置驱动真实实现） =====

struct ReqProd001;
impl CheckItem for ReqProd001 {
    fn id(&self) -> &str {
        "REQ-PROD-001"
    }
    fn name(&self) -> &str {
        "配置脱敏验证"
    }
    fn category(&self) -> CheckCategory {
        CheckCategory::SafetyRedline
    }
    fn run(&self, config: &ProdReadyCheckerConfig) -> CheckItemResult {
        check_bool_flag(
            self.id(),
            self.name(),
            self.category(),
            config.config_masking_enabled,
            "config_masking_enabled",
        )
    }
}

struct ReqProd002;
impl CheckItem for ReqProd002 {
    fn id(&self) -> &str {
        "REQ-PROD-002"
    }
    fn name(&self) -> &str {
        "Redis TLS 验证"
    }
    fn category(&self) -> CheckCategory {
        CheckCategory::SafetyRedline
    }
    fn run(&self, config: &ProdReadyCheckerConfig) -> CheckItemResult {
        check_bool_flag(
            self.id(),
            self.name(),
            self.category(),
            config.redis_tls_enabled,
            "redis_tls_enabled",
        )
    }
}

struct ReqProd003;
impl CheckItem for ReqProd003 {
    fn id(&self) -> &str {
        "REQ-PROD-003"
    }
    fn name(&self) -> &str {
        "JWT 密钥轮换"
    }
    fn category(&self) -> CheckCategory {
        CheckCategory::SafetyRedline
    }
    fn run(&self, config: &ProdReadyCheckerConfig) -> CheckItemResult {
        match config.jwt_key_rotation_days {
            None => skipped_result(self.id(), self.name(), self.category()),
            Some(days) if (1..=90).contains(&days) => pass_result(
                self.id(),
                self.name(),
                self.category(),
                vec![format!("jwt_key_rotation_days = {}（约束 1..=90）", days)],
            ),
            Some(days) => fail_result(
                self.id(),
                self.name(),
                self.category(),
                vec![format!("jwt_key_rotation_days = {}", days)],
                format!(
                    "JWT 密钥轮换周期 {} 天超出生产约束 1..=90（过长轮换窗口扩大密钥泄露影响面）",
                    days
                ),
            ),
        }
    }
}

struct ReqProd004;
impl CheckItem for ReqProd004 {
    fn id(&self) -> &str {
        "REQ-PROD-004"
    }
    fn name(&self) -> &str {
        "限流配置验证"
    }
    fn category(&self) -> CheckCategory {
        CheckCategory::SafetyRedline
    }
    fn run(&self, config: &ProdReadyCheckerConfig) -> CheckItemResult {
        check_bool_flag(
            self.id(),
            self.name(),
            self.category(),
            config.rate_limit_enabled,
            "rate_limit_enabled",
        )
    }
}

struct ReqProd005;
impl CheckItem for ReqProd005 {
    fn id(&self) -> &str {
        "REQ-PROD-005"
    }
    fn name(&self) -> &str {
        "熔断器配置验证"
    }
    fn category(&self) -> CheckCategory {
        CheckCategory::SafetyRedline
    }
    fn run(&self, config: &ProdReadyCheckerConfig) -> CheckItemResult {
        check_bool_flag(
            self.id(),
            self.name(),
            self.category(),
            config.circuit_breaker_enabled,
            "circuit_breaker_enabled",
        )
    }
}

struct ReqProd006;
impl CheckItem for ReqProd006 {
    fn id(&self) -> &str {
        "REQ-PROD-006"
    }
    fn name(&self) -> &str {
        "日志级别验证"
    }
    fn category(&self) -> CheckCategory {
        CheckCategory::ConfigObservability
    }
    fn run(&self, config: &ProdReadyCheckerConfig) -> CheckItemResult {
        match config.log_level.as_deref() {
            None => skipped_result(self.id(), self.name(), self.category()),
            Some(level) => {
                let normalized = level.trim().to_ascii_lowercase();
                if matches!(normalized.as_str(), "error" | "warn" | "info") {
                    pass_result(
                        self.id(),
                        self.name(),
                        self.category(),
                        vec![format!("log_level = \"{}\"（生产合格级别）", normalized)],
                    )
                } else {
                    fail_result(
                        self.id(),
                        self.name(),
                        self.category(),
                        vec![format!("log_level = \"{}\"", level)],
                        format!(
                            "日志级别 \"{}\" 不适合生产（debug/trace 泄露 SQL 与敏感数据；允许 error/warn/info）",
                            level
                        ),
                    )
                }
            }
        }
    }
}

struct ReqProd007;
impl CheckItem for ReqProd007 {
    fn id(&self) -> &str {
        "REQ-PROD-007"
    }
    fn name(&self) -> &str {
        "metrics ACL 验证"
    }
    fn category(&self) -> CheckCategory {
        CheckCategory::SafetyRedline
    }
    fn run(&self, config: &ProdReadyCheckerConfig) -> CheckItemResult {
        check_bool_flag(
            self.id(),
            self.name(),
            self.category(),
            config.metrics_acl_enabled,
            "metrics_acl_enabled",
        )
    }
}

struct ReqProd008;
impl CheckItem for ReqProd008 {
    fn id(&self) -> &str {
        "REQ-PROD-008"
    }
    fn name(&self) -> &str {
        "健康端点配置验证"
    }
    fn category(&self) -> CheckCategory {
        CheckCategory::ConfigObservability
    }
    fn run(&self, config: &ProdReadyCheckerConfig) -> CheckItemResult {
        check_bool_flag(
            self.id(),
            self.name(),
            self.category(),
            config.health_endpoint_enabled,
            "health_endpoint_enabled",
        )
    }
}

struct ReqProd009;
impl CheckItem for ReqProd009 {
    fn id(&self) -> &str {
        "REQ-PROD-009"
    }
    fn name(&self) -> &str {
        "优雅关闭超时"
    }
    fn category(&self) -> CheckCategory {
        CheckCategory::ConfigObservability
    }
    fn run(&self, config: &ProdReadyCheckerConfig) -> CheckItemResult {
        match config.shutdown_timeout_secs {
            None => skipped_result(self.id(), self.name(), self.category()),
            Some(secs) if (1..=3600).contains(&secs) => pass_result(
                self.id(),
                self.name(),
                self.category(),
                vec![format!("shutdown_timeout_secs = {}（约束 1..=3600）", secs)],
            ),
            Some(secs) => fail_result(
                self.id(),
                self.name(),
                self.category(),
                vec![format!("shutdown_timeout_secs = {}", secs)],
                format!(
                    "优雅关闭超时 {} 秒超出生产约束 1..=3600（0 表示不等待在途请求，过长阻塞发布）",
                    secs
                ),
            ),
        }
    }
}

struct ReqProd010;
impl CheckItem for ReqProd010 {
    fn id(&self) -> &str {
        "REQ-PROD-010"
    }
    fn name(&self) -> &str {
        "K8s 探针端点配置"
    }
    fn category(&self) -> CheckCategory {
        CheckCategory::ConfigObservability
    }
    fn run(&self, config: &ProdReadyCheckerConfig) -> CheckItemResult {
        check_bool_flag(
            self.id(),
            self.name(),
            self.category(),
            config.probe_endpoint_enabled,
            "probe_endpoint_enabled",
        )
    }
}

struct ReqProd011;
impl CheckItem for ReqProd011 {
    fn id(&self) -> &str {
        "REQ-PROD-011"
    }
    fn name(&self) -> &str {
        "SQL 注入扫描"
    }
    fn category(&self) -> CheckCategory {
        CheckCategory::SafetyRedline
    }
    fn run(&self, config: &ProdReadyCheckerConfig) -> CheckItemResult {
        check_bool_flag(
            self.id(),
            self.name(),
            self.category(),
            config.sql_injection_scan_passed,
            "sql_injection_scan_passed",
        )
    }
}

struct ReqProd012;
impl CheckItem for ReqProd012 {
    fn id(&self) -> &str {
        "REQ-PROD-012"
    }
    fn name(&self) -> &str {
        "连接泄漏检测配置"
    }
    fn category(&self) -> CheckCategory {
        CheckCategory::OrmProtection
    }
    fn run(&self, config: &ProdReadyCheckerConfig) -> CheckItemResult {
        check_bool_flag(
            self.id(),
            self.name(),
            self.category(),
            config.leak_detection_enabled,
            "leak_detection_enabled",
        )
    }
}

struct ReqProd013;
impl CheckItem for ReqProd013 {
    fn id(&self) -> &str {
        "REQ-PROD-013"
    }
    fn name(&self) -> &str {
        "N+1 检测配置"
    }
    fn category(&self) -> CheckCategory {
        CheckCategory::OrmProtection
    }
    fn run(&self, config: &ProdReadyCheckerConfig) -> CheckItemResult {
        check_bool_flag(
            self.id(),
            self.name(),
            self.category(),
            config.n1_detection_enabled,
            "n1_detection_enabled",
        )
    }
}

struct ReqProd014;
impl CheckItem for ReqProd014 {
    fn id(&self) -> &str {
        "REQ-PROD-014"
    }
    fn name(&self) -> &str {
        "连接池参数验证"
    }
    fn category(&self) -> CheckCategory {
        CheckCategory::ThresholdTuning
    }
    fn run(&self, config: &ProdReadyCheckerConfig) -> CheckItemResult {
        match (config.pool_max_connections, config.pool_min_idle) {
            (None, _) | (_, None) => skipped_result(self.id(), self.name(), self.category()),
            (Some(max), Some(min)) if max > 0 && min > 0 && min <= max && max <= 10_000 => {
                pass_result(
                    self.id(),
                    self.name(),
                    self.category(),
                    vec![format!(
                        "pool_max_connections = {}, pool_min_idle = {}（约束 0 < min <= max <= 10000）",
                        max, min
                    )],
                )
            }
            (Some(max), Some(min)) => fail_result(
                self.id(),
                self.name(),
                self.category(),
                vec![format!(
                    "pool_max_connections = {}, pool_min_idle = {}",
                    max, min
                )],
                format!(
                    "连接池参数不满足生产约束 0 < min_idle <= max_connections <= 10000（当前 max={}, min={}）",
                    max, min
                ),
            ),
        }
    }
}

struct ReqProd015;
impl CheckItem for ReqProd015 {
    fn id(&self) -> &str {
        "REQ-PROD-015"
    }
    fn name(&self) -> &str {
        "五方言安全验证"
    }
    fn category(&self) -> CheckCategory {
        CheckCategory::OrmProtection
    }
    fn run(&self, config: &ProdReadyCheckerConfig) -> CheckItemResult {
        check_bool_flag(
            self.id(),
            self.name(),
            self.category(),
            config.dialect_security_verified,
            "dialect_security_verified",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checker_runs_all_15_checks() {
        let checker = ProdReadyChecker::new(ProdReadyCheckerConfig::default());
        let report = checker.run();
        assert_eq!(report.items.len(), 15);
        assert_eq!(report.summary.total, 15);
    }

    #[test]
    fn test_default_config_all_skipped_not_pass() {
        // v10.13.0 真实化：默认配置（无任何配置输入）必须 Skipped 而非历史版本的硬编码 PASS
        let checker = ProdReadyChecker::new(ProdReadyCheckerConfig::default());
        let report = checker.run();
        assert_eq!(report.summary.pass, 0);
        assert_eq!(report.summary.fail, 0);
        assert_eq!(report.summary.skipped, 15);
        assert!(report.all_pass()); // 非 Fail 即"无失败"
        assert!(!report.all_pass_strict()); // 但严格判定不通过（未真实执行）
    }

    #[test]
    fn test_good_config_all_pass_strict() {
        let config = ProdReadyCheckerConfig {
            config_masking_enabled: Some(true),
            redis_tls_enabled: Some(true),
            jwt_key_rotation_days: Some(30),
            rate_limit_enabled: Some(true),
            circuit_breaker_enabled: Some(true),
            log_level: Some("warn".to_string()),
            metrics_acl_enabled: Some(true),
            health_endpoint_enabled: Some(true),
            shutdown_timeout_secs: Some(30),
            probe_endpoint_enabled: Some(true),
            sql_injection_scan_passed: Some(true),
            leak_detection_enabled: Some(true),
            n1_detection_enabled: Some(true),
            pool_max_connections: Some(50),
            pool_min_idle: Some(5),
            dialect_security_verified: Some(true),
            ..Default::default()
        };
        let checker = ProdReadyChecker::new(config);
        let report = checker.run();
        assert_eq!(report.summary.pass, 15, "好配置应 15 项全 Pass");
        assert_eq!(report.summary.fail, 0);
        assert_eq!(report.summary.skipped, 0);
        assert!(report.all_pass_strict());
    }

    #[test]
    fn test_bad_config_fails() {
        let config = ProdReadyCheckerConfig {
            config_masking_enabled: Some(false),
            redis_tls_enabled: Some(false),
            jwt_key_rotation_days: Some(365),
            log_level: Some("debug".to_string()),
            shutdown_timeout_secs: Some(0),
            pool_max_connections: Some(10),
            pool_min_idle: Some(20), // min > max 非法
            ..Default::default()
        };
        let checker = ProdReadyChecker::new(config);
        let report = checker.run();
        assert_eq!(report.summary.fail, 6, "坏配置应 6 项 Fail");
        assert!(!report.all_pass());
        assert!(!report.all_pass_strict());
        // 逐项核对失败 ID
        let failed: Vec<&str> = report
            .items
            .iter()
            .filter(|i| i.status == CheckStatus::Fail)
            .map(|i| i.id.as_str())
            .collect();
        for id in [
            "REQ-PROD-001",
            "REQ-PROD-002",
            "REQ-PROD-003",
            "REQ-PROD-006",
            "REQ-PROD-009",
            "REQ-PROD-014",
        ] {
            assert!(failed.contains(&id), "应包含失败项 {}", id);
        }
    }

    #[test]
    fn test_partial_config_mixed_statuses() {
        let config = ProdReadyCheckerConfig {
            config_masking_enabled: Some(true),
            ..Default::default()
        };
        let checker = ProdReadyChecker::new(config);
        let report = checker.run();
        assert_eq!(report.summary.pass, 1);
        assert_eq!(report.summary.fail, 0);
        assert_eq!(report.summary.skipped, 14);
    }

    #[test]
    fn test_check_ids_are_sequential() {
        let checker = ProdReadyChecker::new(ProdReadyCheckerConfig::default());
        let report = checker.run();
        for (i, item) in report.items.iter().enumerate() {
            let expected_id = format!("REQ-PROD-{:03}", i + 1);
            assert_eq!(item.id, expected_id, "item {} has wrong id", i);
        }
    }

    #[test]
    fn test_categories_are_correct() {
        let checker = ProdReadyChecker::new(ProdReadyCheckerConfig::default());
        let report = checker.run();
        assert_eq!(report.items[0].category, CheckCategory::SafetyRedline);
        assert_eq!(report.items[5].category, CheckCategory::ConfigObservability);
        assert_eq!(report.items[13].category, CheckCategory::ThresholdTuning);
        assert_eq!(report.items[14].category, CheckCategory::OrmProtection);
    }

    #[test]
    fn test_evidence_is_non_empty() {
        let checker = ProdReadyChecker::new(ProdReadyCheckerConfig::default());
        let report = checker.run();
        for item in &report.items {
            assert!(!item.evidence.is_empty(), "{} has no evidence", item.id);
        }
    }

    #[test]
    fn test_timestamp_is_set() {
        let checker = ProdReadyChecker::new(ProdReadyCheckerConfig::default());
        let report = checker.run();
        for item in &report.items {
            assert!(!item.timestamp.is_empty(), "{} has no timestamp", item.id);
        }
    }

    #[test]
    fn test_skipped_checks() {
        let config = ProdReadyCheckerConfig {
            skipped_checks: vec!["REQ-PROD-001".to_string()],
            ..Default::default()
        };
        let checker = ProdReadyChecker::new(config);
        let report = checker.run();
        assert_eq!(report.items[0].status, CheckStatus::Skipped);
        assert_eq!(report.summary.skipped, 15);
    }

    #[test]
    fn test_enabled_checks_filter() {
        let config = ProdReadyCheckerConfig {
            enabled_checks: vec!["REQ-PROD-001".to_string(), "REQ-PROD-002".to_string()],
            config_masking_enabled: Some(true),
            redis_tls_enabled: Some(true),
            ..Default::default()
        };
        let checker = ProdReadyChecker::new(config);
        let report = checker.run();
        assert_eq!(report.summary.pass, 2);
        assert_eq!(report.summary.skipped, 13);
    }

    #[test]
    fn test_report_to_json() {
        let checker = ProdReadyChecker::new(ProdReadyCheckerConfig::default());
        let report = checker.run();
        let json = report.to_json().unwrap();
        assert!(json.contains("REQ-PROD-001"));
        assert!(json.contains("total"));
        assert!(json.contains("pass"));
        assert!(json.contains("15"));
    }

    #[test]
    fn test_fail_result_has_reason() {
        let result = fail_result(
            "REQ-PROD-001",
            "test",
            CheckCategory::SafetyRedline,
            vec!["config_masking_enabled = false".to_string()],
            "config invalid".to_string(),
        );
        assert_eq!(result.status, CheckStatus::Fail);
        assert_eq!(result.failure_reason.as_deref(), Some("config invalid"));
    }

    #[test]
    fn test_check_item_trait_extensibility() {
        struct CustomCheck;
        impl CheckItem for CustomCheck {
            fn id(&self) -> &str {
                "REQ-PROD-CUSTOM"
            }
            fn name(&self) -> &str {
                "custom check"
            }
            fn category(&self) -> CheckCategory {
                CheckCategory::OrmProtection
            }
            fn run(&self, _config: &ProdReadyCheckerConfig) -> CheckItemResult {
                pass_result(
                    "REQ-PROD-CUSTOM",
                    "custom check",
                    CheckCategory::OrmProtection,
                    vec!["custom evidence".to_string()],
                )
            }
        }

        let custom = CustomCheck;
        let result = custom.run(&ProdReadyCheckerConfig::default());
        assert_eq!(result.id, "REQ-PROD-CUSTOM");
        assert_eq!(result.status, CheckStatus::Pass);
        assert_eq!(result.evidence[0], "custom evidence");
    }
}
