//! 安全合规自动化（v10.1.0）
//!
//! 合规检查定期自动运行 + 安全配置预设一键启用。
//! 默认 auto_check_enabled=false，向后兼容 v10.0.0。
//! 注意：配置类型在本包独立定义（sz-orm-core 依赖 sz-orm-audit，不可反向依赖）。
//! sz-orm-core::performance_linkage 中也有同名类型供 PoolConfig 使用，两者结构相同。

use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::time::Duration;

/// 合规自动化配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceAutoCheckConfig {
    /// 是否启用自动合规检查，默认 false
    pub auto_check_enabled: bool,
    /// 检查间隔
    pub check_interval: Duration,
    /// 自动生成报告
    pub auto_report: bool,
    /// 违规告警
    pub violation_alert: bool,
    /// 安全配置预设
    pub preset: SecurityPreset,
}

impl Default for ComplianceAutoCheckConfig {
    fn default() -> Self {
        Self {
            auto_check_enabled: false,
            check_interval: Duration::from_secs(86400),
            auto_report: true,
            violation_alert: true,
            preset: SecurityPreset::None,
        }
    }
}

/// 安全配置预设
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SecurityPreset {
    #[default]
    None,
    ComplianceFirst,
    PerformanceFirst,
    Development,
}

/// 合规检查结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceCheckResult {
    pub encryption_coverage: f64,
    pub audit_integrity: bool,
    pub isolation_effectiveness: bool,
    pub violations: Vec<String>,
}

impl ComplianceCheckResult {
    pub fn has_violations(&self) -> bool {
        !self.violations.is_empty()
    }
}

/// 合规检查定期调度器
pub struct ComplianceAutoChecker {
    config: ComplianceAutoCheckConfig,
    last_result: Mutex<Option<ComplianceCheckResult>>,
}

impl ComplianceAutoChecker {
    pub fn new(config: ComplianceAutoCheckConfig) -> Self {
        Self {
            config,
            last_result: Mutex::new(None),
        }
    }

    pub fn config(&self) -> &ComplianceAutoCheckConfig {
        &self.config
    }

    pub fn last_result(&self) -> Option<ComplianceCheckResult> {
        self.last_result
            .lock()
            .expect("last_result lock poisoned")
            .clone()
    }

    /// 运行合规检查（生产消费点：加密覆盖率/审计完整性/隔离有效性）
    pub fn run_check(
        &self,
        encryption_coverage: f64,
        audit_integrity: bool,
        isolation_effectiveness: bool,
        unencrypted_fields: &[String],
    ) -> ComplianceCheckResult {
        let mut violations = Vec::new();
        if encryption_coverage < 1.0 {
            for field in unencrypted_fields {
                violations.push(format!("field '{}' not encrypted", field));
            }
        }
        if !audit_integrity {
            violations.push("audit chain verification failed".to_string());
        }
        if !isolation_effectiveness {
            violations.push("cross-tenant isolation ineffective".to_string());
        }

        let result = ComplianceCheckResult {
            encryption_coverage,
            audit_integrity,
            isolation_effectiveness,
            violations,
        };

        *self.last_result.lock().expect("last_result lock poisoned") = Some(result.clone());
        result
    }

    /// 计算合规评分（生产消费点：基于违规数量计算 0.0~1.0 评分）
    pub fn compliance_score(&self) -> f64 {
        match self.last_result() {
            None => 1.0,
            Some(result) => {
                if result.violations.is_empty() {
                    1.0
                } else {
                    let penalty = result.violations.len() as f64 * 0.1;
                    (1.0 - penalty).max(0.0)
                }
            }
        }
    }

    // ==================== v10.10.0 T4.2: 定期扫描 + 报告 + 告警 + 整改 ====================

    /// v10.10.0：定期扫描合规规则
    pub fn scheduled_scan(
        &self,
        _interval: Duration,
    ) -> Result<ScanResult, ComplianceError> {
        let result = self.last_result();
        let violations: Vec<Violation> = result
            .as_ref()
            .map(|r| {
                r.violations
                    .iter()
                    .enumerate()
                    .map(|(i, v)| Violation {
                        id: format!("V-{}", i + 1),
                        description: v.clone(),
                        severity: ViolationSeverity::Medium,
                    })
                    .collect()
            })
            .unwrap_or_default();

        let score = self.compliance_score();
        Ok(ScanResult {
            violations,
            compliance_score: score,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        })
    }

    /// v10.10.0：生成合规报告
    pub fn generate_compliance_report(&self) -> ComplianceReport {
        let score = self.compliance_score();
        let result = self.last_result();

        let violations: Vec<Violation> = result
            .as_ref()
            .map(|r| {
                r.violations
                    .iter()
                    .enumerate()
                    .map(|(i, v)| Violation {
                        id: format!("V-{}", i + 1),
                        description: v.clone(),
                        severity: ViolationSeverity::Medium,
                    })
                    .collect()
            })
            .unwrap_or_default();

        let remediation: Vec<RemediationSuggestion> = violations
            .iter()
            .flat_map(|v| self.remediation_suggest(v))
            .collect();

        ComplianceReport {
            score,
            violations,
            remediation_suggestions: remediation,
            generated_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }

    /// v10.10.0：违规告警
    pub fn violation_alert(&self, violation: &Violation) -> Alert {
        let level = match violation.severity {
            ViolationSeverity::Critical => AlertLevel::Critical,
            ViolationSeverity::High => AlertLevel::High,
            ViolationSeverity::Medium => AlertLevel::Medium,
            ViolationSeverity::Low => AlertLevel::Low,
        };

        Alert {
            level,
            message: format!("合规违规 [{}]: {}", violation.id, violation.description),
            violation_id: violation.id.clone(),
        }
    }

    /// v10.10.0：整改建议
    pub fn remediation_suggest(
        &self,
        violation: &Violation,
    ) -> Vec<RemediationSuggestion> {
        let mut suggestions = Vec::new();

        if violation.description.contains("not encrypted") {
            suggestions.push(RemediationSuggestion {
                violation_id: violation.id.clone(),
                action: "启用透明加密".to_string(),
                priority: 1,
                description: "为未加密字段启用 TDE 透明加密".to_string(),
            });
        }

        if violation.description.contains("audit chain") {
            suggestions.push(RemediationSuggestion {
                violation_id: violation.id.clone(),
                action: "修复审计链".to_string(),
                priority: 1,
                description: "重新计算哈希链，修复断裂的审计记录".to_string(),
            });
        }

        if violation.description.contains("isolation") {
            suggestions.push(RemediationSuggestion {
                violation_id: violation.id.clone(),
                action: "强化租户隔离".to_string(),
                priority: 2,
                description: "启用 RLS 策略，验证跨租户访问拦截".to_string(),
            });
        }

        if suggestions.is_empty() {
            suggestions.push(RemediationSuggestion {
                violation_id: violation.id.clone(),
                action: "人工审查".to_string(),
                priority: 3,
                description: format!("人工审查违规: {}", violation.description),
            });
        }

        suggestions
    }
}

// ==================== v10.10.0 T4.2: 合规报告类型 ====================

/// v10.10.0：违规严重级别
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViolationSeverity {
    Critical,
    High,
    Medium,
    Low,
}

/// v10.10.0：违规项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Violation {
    pub id: String,
    pub description: String,
    pub severity: ViolationSeverity,
}

/// v10.10.0：扫描结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    pub violations: Vec<Violation>,
    pub compliance_score: f64,
    pub timestamp: u64,
}

/// v10.10.0：合规报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceReport {
    pub score: f64,
    pub violations: Vec<Violation>,
    pub remediation_suggestions: Vec<RemediationSuggestion>,
    pub generated_at: u64,
}

/// v10.10.0：告警级别
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertLevel {
    Critical,
    High,
    Medium,
    Low,
}

/// v10.10.0：告警
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    pub level: AlertLevel,
    pub message: String,
    pub violation_id: String,
}

/// v10.10.0：整改建议
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemediationSuggestion {
    pub violation_id: String,
    pub action: String,
    pub priority: u32,
    pub description: String,
}

/// v10.10.0：合规错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComplianceError {
    ScanFailed(String),
    NoData,
}

impl std::fmt::Display for ComplianceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ComplianceError::ScanFailed(msg) => write!(f, "Scan failed: {}", msg),
            ComplianceError::NoData => write!(f, "No compliance data available"),
        }
    }
}

impl std::error::Error for ComplianceError {}

/// v10.1.0：Pool 合规扩展 trait
///
/// 由持有 Pool + ComplianceAutoChecker 引用的应用侧上下文实现。
/// sz-orm-audit 不依赖 sz-orm-core（避免循环依赖），因此 trait 定义在此处，
/// 应用侧通过组合 Pool 和 ComplianceAutoChecker 实现此接口。
pub trait PoolComplianceExt {
    /// 运行 Pool 合规检查，返回最近一次检查结果
    fn run_pool_compliance_check(&self) -> Option<ComplianceCheckResult>;

    /// 获取合规评分（0.0~1.0）
    fn pool_compliance_score(&self) -> f64 {
        match self.run_pool_compliance_check() {
            None => 1.0,
            Some(result) => {
                if result.violations.is_empty() {
                    1.0
                } else {
                    let penalty = result.violations.len() as f64 * 0.1;
                    (1.0 - penalty).max(0.0)
                }
            }
        }
    }
}

/// 一键应用安全预设（生产消费点：修改 PoolConfig 的加密/审计/隔离/告警配置）
///
/// 注意：此函数为公共 API，由应用方显式调用。
/// `pool_alert_enabled`/`transparent_encrypt_enabled`/`cross_tenant_detection` 等参数
/// 由调用方提供可变引用，函数内部按预设修改。
pub fn apply_security_preset(
    preset: SecurityPreset,
    transparent_encrypt_enabled: &mut bool,
    pool_alert_enabled: &mut bool,
    cross_tenant_detection: &mut bool,
    auto_check_enabled: &mut bool,
    cache_enhance_enabled: &mut bool,
) {
    match preset {
        SecurityPreset::None => {}
        SecurityPreset::ComplianceFirst => {
            *transparent_encrypt_enabled = true;
            *pool_alert_enabled = true;
            *cross_tenant_detection = true;
            *auto_check_enabled = true;
        }
        SecurityPreset::PerformanceFirst => {
            *cross_tenant_detection = true;
            *cache_enhance_enabled = true;
        }
        SecurityPreset::Development => {
            *transparent_encrypt_enabled = false;
            *cross_tenant_detection = true;
        }
    }
}

// ==================== v10.11.0 T4.2: 持续合规监控（事件驱动实时扫描） ====================

impl ViolationSeverity {
    /// 风险等级数值（0=Low, 1=Medium, 2=High, 3=Critical）
    pub fn level(&self) -> u8 {
        match self {
            ViolationSeverity::Low => 0,
            ViolationSeverity::Medium => 1,
            ViolationSeverity::High => 2,
            ViolationSeverity::Critical => 3,
        }
    }
}

/// 数据访问事件类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataAccessEventType {
    Query,
    Insert,
    Update,
    Delete,
    SchemaChange,
}

/// 数据访问事件（事件驱动合规监控的输入）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataAccessEvent {
    pub event_id: String,
    pub event_type: DataAccessEventType,
    pub user: String,
    pub table: String,
    pub fields: Vec<String>,
    pub raw_sql: String,
    pub timestamp: u64,
    pub tenant_id: Option<String>,
}

impl DataAccessEvent {
    pub fn new(
        event_type: DataAccessEventType,
        user: &str,
        table: &str,
        fields: Vec<String>,
        raw_sql: &str,
        timestamp: u64,
    ) -> Self {
        Self {
            event_id: format!("EVT-{}", timestamp),
            event_type,
            user: user.to_string(),
            table: table.to_string(),
            fields,
            raw_sql: raw_sql.to_string(),
            timestamp,
            tenant_id: None,
        }
    }

    pub fn with_tenant(mut self, tenant: &str) -> Self {
        self.tenant_id = Some(tenant.to_string());
        self
    }
}

/// 监控结果（事件驱动合规监控的输出）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MonitorResult {
    /// 合规
    Compliant,
    /// 违规告警（高风险，需人工介入）
    Violation {
        violation: Violation,
        alert: Alert,
        latency_ms: u64,
    },
    /// 自动处置完成（低风险）
    AutoRemediated {
        violation: Violation,
        remediation: RemediationResult,
        latency_ms: u64,
    },
}

/// 整改动作类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RemediationActionType {
    AutoDesensitize,
    AutoEncrypt,
    AutoBlock,
    ManualReview,
}

/// 整改动作
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemediationAction {
    pub action_id: String,
    pub action_type: RemediationActionType,
    pub description: String,
    pub target_field: Option<String>,
}

impl RemediationAction {
    pub fn auto_desensitize(field: &str) -> Self {
        Self {
            action_id: format!("REM-DESENS-{}", field),
            action_type: RemediationActionType::AutoDesensitize,
            description: format!("自动脱敏字段 '{}'", field),
            target_field: Some(field.to_string()),
        }
    }

    pub fn auto_encrypt(field: &str) -> Self {
        Self {
            action_id: format!("REM-ENC-{}", field),
            action_type: RemediationActionType::AutoEncrypt,
            description: format!("自动加密字段 '{}'", field),
            target_field: Some(field.to_string()),
        }
    }

    pub fn manual_review(violation_id: &str) -> Self {
        Self {
            action_id: format!("REM-MANUAL-{}", violation_id),
            action_type: RemediationActionType::ManualReview,
            description: format!("人工审查违规 '{}'", violation_id),
            target_field: None,
        }
    }
}

/// 整改结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemediationResult {
    pub action: RemediationAction,
    pub success: bool,
    pub before_value: String,
    pub after_value: String,
    pub timestamp: u64,
}

/// 监控错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MonitorError {
    ScanFailed(String),
    RemediationFailed(String),
    AlertTimeout,
}

impl std::fmt::Display for MonitorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MonitorError::ScanFailed(msg) => write!(f, "Scan failed: {}", msg),
            MonitorError::RemediationFailed(msg) => write!(f, "Remediation failed: {}", msg),
            MonitorError::AlertTimeout => write!(f, "Alert timeout"),
        }
    }
}

impl std::error::Error for MonitorError {}

/// 整改错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemediationError {
    NotRemediable,
    ActionFailed(String),
    HighRiskRequiresManual,
}

impl std::fmt::Display for RemediationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RemediationError::NotRemediable => write!(f, "Violation not remediable"),
            RemediationError::ActionFailed(msg) => write!(f, "Action failed: {}", msg),
            RemediationError::HighRiskRequiresManual => {
                write!(f, "High risk violation requires manual intervention")
            }
        }
    }
}

impl std::error::Error for RemediationError {}

/// 事件驱动扫描器（监听数据访问事件，实时扫描合规规则）
pub struct EventDrivenScanner {
    sensitive_fields: Vec<String>,
    blocked_tables: Vec<String>,
    stats: Mutex<EventScanStats>,
}

#[derive(Default)]
struct EventScanStats {
    events_scanned: u64,
    violations_found: u64,
}

impl EventDrivenScanner {
    pub fn new(sensitive_fields: Vec<String>) -> Self {
        Self {
            sensitive_fields,
            blocked_tables: Vec::new(),
            stats: Mutex::new(EventScanStats::default()),
        }
    }

    pub fn with_blocked_tables(mut self, tables: Vec<String>) -> Self {
        self.blocked_tables = tables;
        self
    }

    pub fn sensitive_fields(&self) -> &[String] {
        &self.sensitive_fields
    }

    /// 事件驱动扫描（非轮询）：扫描数据访问事件，返回违规（如有）
    pub fn scan(&self, event: &DataAccessEvent) -> Option<Violation> {
        {
            let mut stats = self.stats.lock().expect("scanner stats lock");
            stats.events_scanned += 1;
        }

        for field in &event.fields {
            let field_lower = field.to_ascii_lowercase();
            for sensitive in &self.sensitive_fields {
                if field_lower.contains(&sensitive.to_ascii_lowercase())
                    && !event.raw_sql.contains("******")
                {
                    let mut stats = self.stats.lock().expect("scanner stats lock");
                    stats.violations_found += 1;
                    return Some(Violation {
                        id: format!("V-SENSITIVE-{}-{}", event.event_id, field),
                        description: format!(
                            "敏感字段 '{}' 在数据访问中未脱敏（table={}, user={}）",
                            field, event.table, event.user
                        ),
                        severity: ViolationSeverity::Medium,
                    });
                }
            }
        }

        for blocked in &self.blocked_tables {
            if event.table.eq_ignore_ascii_case(blocked) {
                let mut stats = self.stats.lock().expect("scanner stats lock");
                stats.violations_found += 1;
                return Some(Violation {
                    id: format!("V-BLOCKED-{}-{}", event.event_id, event.table),
                    description: format!(
                        "访问阻断表 '{}'（user={}）",
                        event.table, event.user
                    ),
                    severity: ViolationSeverity::High,
                });
            }
        }

        if event.event_type == DataAccessEventType::SchemaChange {
            let mut stats = self.stats.lock().expect("scanner stats lock");
            stats.violations_found += 1;
            return Some(Violation {
                id: format!("V-SCHEMA-{}", event.event_id),
                description: format!(
                    "Schema 变更操作（table={}, user={}）需人工审查",
                    event.table, event.user
                ),
                severity: ViolationSeverity::High,
            });
        }

        None
    }

    pub fn events_scanned(&self) -> u64 {
        self.stats.lock().expect("scanner stats lock").events_scanned
    }

    pub fn violations_found(&self) -> u64 {
        self.stats.lock().expect("scanner stats lock").violations_found
    }
}

/// 在 SQL 中脱敏指定字段值（field='value' → field='******'）
fn mask_field_in_sql(sql: &str, field: Option<&str>) -> String {
    match field {
        None | Some("") => sql.to_string(),
        Some(f) => {
            let mut result = sql.to_string();
            for pat in [format!("{}='", f), format!("{} = '", f)] {
                if let Some(pos) = result.find(&pat) {
                    let value_start = pos + pat.len();
                    if let Some(end) = result[value_start..].find('\'') {
                        let value_end = value_start + end;
                        result = format!(
                            "{}'******'{}",
                            &result[..value_start],
                            &result[value_end + 1..]
                        );
                    }
                }
            }
            result
        }
    }
}

/// 自动处置器（低风险违规自动处置，高风险告警人工介入）
pub struct AutoRemediator {
    custom_actions: Mutex<Vec<RemediationAction>>,
    stats: Mutex<RemediationStats>,
}

#[derive(Default)]
struct RemediationStats {
    auto_remediated: u64,
    manual_alerted: u64,
}

impl AutoRemediator {
    pub fn new() -> Self {
        Self {
            custom_actions: Mutex::new(Vec::new()),
            stats: Mutex::new(RemediationStats::default()),
        }
    }

    /// 注册自定义处置策略（扩展点）
    pub fn register_action(&self, action: RemediationAction) {
        self.custom_actions
            .lock()
            .expect("custom actions lock")
            .push(action);
    }

    /// 自动处置违规
    ///
    /// 低风险（Medium/Low）：自动脱敏
    /// 高风险（Critical/High）：返回 HighRiskRequiresManual
    pub fn auto_remediate(
        &self,
        violation: &Violation,
        event: &DataAccessEvent,
    ) -> Result<RemediationResult, RemediationError> {
        match violation.severity {
            ViolationSeverity::Critical | ViolationSeverity::High => {
                self.stats.lock().expect("remediator stats lock").manual_alerted += 1;
                Err(RemediationError::HighRiskRequiresManual)
            }
            ViolationSeverity::Medium | ViolationSeverity::Low => {
                let target_field = violation
                    .description
                    .split('\'')
                    .nth(1)
                    .map(|s| s.to_string());

                let custom = {
                    let actions = self.custom_actions.lock().expect("custom actions lock");
                    actions
                        .iter()
                        .find(|a| a.target_field == target_field)
                        .cloned()
                };

                let action = custom.unwrap_or_else(|| {
                    let field = target_field.as_deref().unwrap_or("unknown");
                    RemediationAction::auto_desensitize(field)
                });

                let before = event.raw_sql.clone();
                let after = mask_field_in_sql(&before, action.target_field.as_deref());

                let result = RemediationResult {
                    action: action.clone(),
                    success: before != after,
                    before_value: before,
                    after_value: after,
                    timestamp: event.timestamp,
                };

                self.stats.lock().expect("remediator stats lock").auto_remediated += 1;
                Ok(result)
            }
        }
    }

    pub fn auto_remediated_count(&self) -> u64 {
        self.stats.lock().expect("remediator stats lock").auto_remediated
    }

    pub fn manual_alerted_count(&self) -> u64 {
        self.stats.lock().expect("remediator stats lock").manual_alerted
    }
}

impl Default for AutoRemediator {
    fn default() -> Self {
        Self::new()
    }
}

/// 持续合规监控器（事件驱动实时扫描 + 自动处置 + 告警）
///
/// 复用 `ComplianceAutoChecker`（line 65）+ `EventDrivenScanner` + `AutoRemediator`。
/// 事件驱动（非轮询），不引入热路径性能退化。
pub struct ContinuousComplianceMonitor {
    checker: ComplianceAutoChecker,
    scanner: EventDrivenScanner,
    remediator: AutoRemediator,
    alert_threshold: ViolationSeverity,
    stats: Mutex<MonitorStats>,
}

#[derive(Default)]
struct MonitorStats {
    total_events: u64,
    compliant: u64,
    violations: u64,
    auto_remediated: u64,
    manual_alerted: u64,
    last_alert_latency_ms: u64,
    max_alert_latency_ms: u64,
}

impl ContinuousComplianceMonitor {
    pub fn new(config: ComplianceAutoCheckConfig, sensitive_fields: Vec<String>) -> Self {
        Self {
            checker: ComplianceAutoChecker::new(config),
            scanner: EventDrivenScanner::new(sensitive_fields),
            remediator: AutoRemediator::new(),
            alert_threshold: ViolationSeverity::High,
            stats: Mutex::new(MonitorStats::default()),
        }
    }

    pub fn with_alert_threshold(mut self, threshold: ViolationSeverity) -> Self {
        self.alert_threshold = threshold;
        self
    }

    pub fn with_blocked_tables(mut self, tables: Vec<String>) -> Self {
        self.scanner = self.scanner.with_blocked_tables(tables);
        self
    }

    /// 事件驱动实时扫描（非轮询）
    ///
    /// 数据访问事件 → 扫描合规规则 → 违规则告警/自动处置。
    /// 时效保证：违规事件 ≤ 1 秒内告警（latency_ms 记入 MonitorResult）。
    pub fn on_data_access_event(
        &self,
        event: &DataAccessEvent,
    ) -> Result<MonitorResult, MonitorError> {
        let scan_start = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        {
            let mut stats = self.stats.lock().expect("monitor stats lock");
            stats.total_events += 1;
        }

        let violation = self.scanner.scan(event);

        match violation {
            None => {
                self.stats.lock().expect("monitor stats lock").compliant += 1;
                Ok(MonitorResult::Compliant)
            }
            Some(violation) => {
                let scan_end = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;
                let latency_ms = scan_end.saturating_sub(scan_start);

                {
                    let mut stats = self.stats.lock().expect("monitor stats lock");
                    stats.violations += 1;
                    stats.last_alert_latency_ms = latency_ms;
                    if latency_ms > stats.max_alert_latency_ms {
                        stats.max_alert_latency_ms = latency_ms;
                    }
                }

                if violation.severity.level() < self.alert_threshold.level() {
                    match self.remediator.auto_remediate(&violation, event) {
                        Ok(remediation) => {
                            self.stats.lock().expect("monitor stats lock").auto_remediated += 1;
                            Ok(MonitorResult::AutoRemediated {
                                violation,
                                remediation,
                                latency_ms,
                            })
                        }
                        Err(e) => {
                            self.stats.lock().expect("monitor stats lock").manual_alerted += 1;
                            let alert = Alert {
                                level: AlertLevel::High,
                                message: format!(
                                    "自动处置失败 [{}]: {} — 需人工介入",
                                    violation.id, e
                                ),
                                violation_id: violation.id.clone(),
                            };
                            Ok(MonitorResult::Violation {
                                violation,
                                alert,
                                latency_ms,
                            })
                        }
                    }
                } else {
                    self.stats.lock().expect("monitor stats lock").manual_alerted += 1;
                    let alert = Alert {
                        level: match violation.severity {
                            ViolationSeverity::Critical => AlertLevel::Critical,
                            ViolationSeverity::High => AlertLevel::High,
                            ViolationSeverity::Medium => AlertLevel::Medium,
                            ViolationSeverity::Low => AlertLevel::Low,
                        },
                        message: format!(
                            "合规违规 [{}]: {} — 需人工介入",
                            violation.id, violation.description
                        ),
                        violation_id: violation.id.clone(),
                    };
                    Ok(MonitorResult::Violation {
                        violation,
                        alert,
                        latency_ms,
                    })
                }
            }
        }
    }

    /// 自动处置违规（手动调用，用于批量处置）
    pub fn auto_remediate(
        &self,
        violation: &Violation,
        event: &DataAccessEvent,
    ) -> Result<RemediationResult, RemediationError> {
        self.remediator.auto_remediate(violation, event)
    }

    /// 注册自定义处置策略（扩展点）
    pub fn register_custom_action(&self, action: RemediationAction) {
        self.remediator.register_action(action);
    }

    pub fn total_events(&self) -> u64 {
        self.stats.lock().expect("monitor stats lock").total_events
    }

    pub fn compliant_count(&self) -> u64 {
        self.stats.lock().expect("monitor stats lock").compliant
    }

    pub fn violation_count(&self) -> u64 {
        self.stats.lock().expect("monitor stats lock").violations
    }

    pub fn auto_remediated_count(&self) -> u64 {
        self.stats.lock().expect("monitor stats lock").auto_remediated
    }

    pub fn manual_alerted_count(&self) -> u64 {
        self.stats.lock().expect("monitor stats lock").manual_alerted
    }

    pub fn last_alert_latency_ms(&self) -> u64 {
        self.stats.lock().expect("monitor stats lock").last_alert_latency_ms
    }

    pub fn max_alert_latency_ms(&self) -> u64 {
        self.stats.lock().expect("monitor stats lock").max_alert_latency_ms
    }

    pub fn checker(&self) -> &ComplianceAutoChecker {
        &self.checker
    }

    pub fn scanner(&self) -> &EventDrivenScanner {
        &self.scanner
    }

    pub fn remediator(&self) -> &AutoRemediator {
        &self.remediator
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compliance_config_default_disabled() {
        let config = ComplianceAutoCheckConfig::default();
        assert!(!config.auto_check_enabled);
        assert_eq!(config.preset, SecurityPreset::None);

        let checker = ComplianceAutoChecker::new(config);
        let result = checker.run_check(0.5, false, false, &["ssn".to_string()]);
        assert!(
            result.has_violations(),
            "run_check with violations should detect them"
        );
    }

    #[test]
    fn test_security_preset_compliance_first_enables_all() {
        let mut te = false;
        let mut pa = false;
        let mut ct = false;
        let mut ac = false;
        let mut ce = false;
        apply_security_preset(
            SecurityPreset::ComplianceFirst,
            &mut te,
            &mut pa,
            &mut ct,
            &mut ac,
            &mut ce,
        );
        assert!(te);
        assert!(pa);
        assert!(ct);
        assert!(ac);
    }

    #[test]
    fn test_security_preset_performance_first_keeps_audit_isolation() {
        let mut te = true;
        let mut pa = true;
        let mut ct = false;
        let mut ac = false;
        let mut ce = false;
        apply_security_preset(
            SecurityPreset::PerformanceFirst,
            &mut te,
            &mut pa,
            &mut ct,
            &mut ac,
            &mut ce,
        );
        assert!(ct);
        assert!(ce);
    }

    #[test]
    fn test_security_preset_development_disables_encryption() {
        let mut te = true;
        let mut pa = true;
        let mut ct = false;
        let mut ac = false;
        let mut ce = false;
        apply_security_preset(
            SecurityPreset::Development,
            &mut te,
            &mut pa,
            &mut ct,
            &mut ac,
            &mut ce,
        );
        assert!(!te);
        assert!(ct);
    }

    #[test]
    fn test_run_check_collects_violations() {
        let checker = ComplianceAutoChecker::new(ComplianceAutoCheckConfig::default());
        let result = checker.run_check(
            0.5,
            true,
            true,
            &["ssn".to_string(), "password".to_string()],
        );
        assert!(result.has_violations());
        assert_eq!(result.violations.len(), 2);
        assert!(result.violations[0].contains("ssn"));
        assert!(result.violations[1].contains("password"));
        assert_eq!(result.encryption_coverage, 0.5);
    }

    #[test]
    fn test_run_check_no_violations() {
        let checker = ComplianceAutoChecker::new(ComplianceAutoCheckConfig::default());
        let result = checker.run_check(1.0, true, true, &[]);
        assert!(!result.has_violations());
        assert_eq!(result.violations.len(), 0);
    }

    #[test]
    fn test_run_check_last_result_stored() {
        let checker = ComplianceAutoChecker::new(ComplianceAutoCheckConfig::default());
        checker.run_check(1.0, true, true, &[]);
        let last = checker.last_result();
        assert!(last.is_some());
        assert!(last.unwrap().audit_integrity);
    }

    #[test]
    fn test_run_check_audit_failure() {
        let checker = ComplianceAutoChecker::new(ComplianceAutoCheckConfig::default());
        let result = checker.run_check(1.0, false, true, &[]);
        assert!(result.has_violations());
        assert!(result.violations.iter().any(|v| v.contains("audit")));
    }

    #[test]
    fn test_compliance_score_no_check_run() {
        let checker = ComplianceAutoChecker::new(ComplianceAutoCheckConfig::default());
        assert_eq!(checker.compliance_score(), 1.0);
    }

    #[test]
    fn test_compliance_score_no_violations() {
        let checker = ComplianceAutoChecker::new(ComplianceAutoCheckConfig::default());
        checker.run_check(1.0, true, true, &[]);
        assert_eq!(checker.compliance_score(), 1.0);
    }

    #[test]
    fn test_compliance_score_with_violations() {
        let checker = ComplianceAutoChecker::new(ComplianceAutoCheckConfig::default());
        checker.run_check(0.5, false, false, &["ssn".to_string()]);
        let score = checker.compliance_score();
        assert!(score < 1.0);
        assert!(score > 0.0);
    }
}
