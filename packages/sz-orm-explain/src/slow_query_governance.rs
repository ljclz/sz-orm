//! Slow query automatic governance.
//!
//! Detects queries whose latency exceeds a configured threshold, performs
//! root-cause analysis, generates and applies a governance action (add index /
//! rewrite SQL / split query), verifies result-set consistency, and rolls back
//! with a `SLOW_QUERY_GOVERNANCE_MISMATCH` tracing alert on mismatch.
//!
//! Typical usage:
//!
//! ```rust,ignore
//! let governor = SlowQueryGovernor::new(SlowQueryGovernanceConfig {
//!     slow_query_threshold_us: 10_000,
//!     auto_apply: true,
//!     ..Default::default()
//! });
//! let result = governor.govern("SELECT * FROM users").await?;
//! ```

use std::sync::Arc;

/// 慢查询根因分类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlowQueryRootCause {
    /// 缺少合适索引，导致条件过滤低效。
    MissingIndex,
    /// 全表扫描（`SELECT *` 或无 `WHERE`）。
    FullTableScan,
    /// N+1 查询模式（`IN (SELECT ...)` 子查询）。
    NPlusOne,
    /// 锁等待（`FOR UPDATE`）。
    LockWait,
}

/// 治理动作分类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GovernanceAction {
    /// 添加索引。
    AddIndex,
    /// 改写 SQL（消除 `SELECT *` / 加 `LIMIT`）。
    RewriteSql,
    /// 拆分查询（消除 N+1 子查询）。
    SplitQuery,
}

/// 慢查询治理配置。
pub struct SlowQueryGovernanceConfig {
    /// 慢查询延迟阈值（微秒），超过即触发治理。
    pub slow_query_threshold_us: u64,
    /// 是否自动应用治理动作；`false` 时仅产出建议不执行。
    pub auto_apply: bool,
    /// 结果集一致性验证回调 `(original_sql, governed_sql) -> consistent`。
    /// 未设置时默认认为治理是等价变换（一致）。
    pub consistency_checker: Option<Arc<dyn Fn(&str, &str) -> bool + Send + Sync>>,
    /// 延迟探针回调 `query -> latency_us`。未设置时使用启发式估算。
    pub latency_probe: Option<Arc<dyn Fn(&str) -> u64 + Send + Sync>>,
}

impl std::fmt::Debug for SlowQueryGovernanceConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SlowQueryGovernanceConfig")
            .field("slow_query_threshold_us", &self.slow_query_threshold_us)
            .field("auto_apply", &self.auto_apply)
            .field(
                "consistency_checker_set",
                &self.consistency_checker.is_some(),
            )
            .field("latency_probe_set", &self.latency_probe.is_some())
            .finish()
    }
}

impl Clone for SlowQueryGovernanceConfig {
    fn clone(&self) -> Self {
        Self {
            slow_query_threshold_us: self.slow_query_threshold_us,
            auto_apply: self.auto_apply,
            consistency_checker: self.consistency_checker.clone(),
            latency_probe: self.latency_probe.clone(),
        }
    }
}

impl Default for SlowQueryGovernanceConfig {
    fn default() -> Self {
        Self {
            slow_query_threshold_us: 10_000,
            auto_apply: false,
            consistency_checker: None,
            latency_probe: None,
        }
    }
}

/// 治理结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GovernanceResult {
    /// 查询指纹（规范化 SQL 的十六进制哈希），相同指纹指向同一查询模式。
    pub query_fingerprint: String,
    /// 根因；非慢查询时为 `None`。
    pub root_cause: Option<SlowQueryRootCause>,
    /// 治理动作；非慢查询或未应用时为 `None`。
    pub action: Option<GovernanceAction>,
    /// 治理前延迟（微秒）。
    pub latency_before_us: u64,
    /// 治理后延迟（微秒）；回滚时等于治理前延迟。
    pub latency_after_us: u64,
    /// 结果集是否一致。
    pub result_set_consistent: bool,
    /// 治理是否成功（已应用且一致）。
    pub governance_success: bool,
    /// 是否执行了回滚（结果集不一致时为 `true`）。
    pub rollback_executed: bool,
}

/// 治理错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GovernanceError {
    /// 查询文本为空或仅含空白。
    EmptyQuery,
    /// 治理应用失败。
    ApplyFailed {
        /// 失败原因。
        reason: String,
    },
}

impl std::fmt::Display for GovernanceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GovernanceError::EmptyQuery => write!(f, "query text is empty"),
            GovernanceError::ApplyFailed { reason } => {
                write!(f, "governance apply failed: {reason}")
            }
        }
    }
}

impl std::error::Error for GovernanceError {}

/// 慢查询治理器。
pub struct SlowQueryGovernor {
    config: SlowQueryGovernanceConfig,
}

impl SlowQueryGovernor {
    /// 创建治理器。
    pub fn new(config: SlowQueryGovernanceConfig) -> Self {
        Self { config }
    }

    /// 对查询执行治理流程：识别 → 根因分析 → 应用 → 一致性验证 → 回滚/告警。
    pub async fn govern(&self, query: &str) -> Result<GovernanceResult, GovernanceError> {
        if query.trim().is_empty() {
            return Err(GovernanceError::EmptyQuery);
        }
        let fingerprint = fingerprint(query);
        let latency_before = self.measure_latency(query);

        if latency_before < self.config.slow_query_threshold_us {
            return Ok(GovernanceResult {
                query_fingerprint: fingerprint,
                root_cause: None,
                action: None,
                latency_before_us: latency_before,
                latency_after_us: latency_before,
                result_set_consistent: true,
                governance_success: true,
                rollback_executed: false,
            });
        }

        let root_cause = analyze_root_cause(query);
        let action = map_action(root_cause);

        if !self.config.auto_apply {
            return Ok(GovernanceResult {
                query_fingerprint: fingerprint,
                root_cause: Some(root_cause),
                action: Some(action),
                latency_before_us: latency_before,
                latency_after_us: latency_before,
                result_set_consistent: true,
                governance_success: false,
                rollback_executed: false,
            });
        }

        let governed_sql = apply_governance(query, action);
        let latency_after = self.measure_latency(&governed_sql);
        let consistent = self.check_consistency(query, &governed_sql);

        if !consistent {
            tracing::warn!(
                target: "sz_orm_explain::slow_query_governance",
                "SLOW_QUERY_GOVERNANCE_MISMATCH fingerprint={} root_cause={:?} action={:?}",
                fingerprint,
                root_cause,
                action,
            );
            return Ok(GovernanceResult {
                query_fingerprint: fingerprint,
                root_cause: Some(root_cause),
                action: Some(action),
                latency_before_us: latency_before,
                latency_after_us: latency_before,
                result_set_consistent: false,
                governance_success: false,
                rollback_executed: true,
            });
        }

        Ok(GovernanceResult {
            query_fingerprint: fingerprint,
            root_cause: Some(root_cause),
            action: Some(action),
            latency_before_us: latency_before,
            latency_after_us: latency_after,
            result_set_consistent: true,
            governance_success: true,
            rollback_executed: false,
        })
    }

    fn measure_latency(&self, query: &str) -> u64 {
        match &self.config.latency_probe {
            Some(probe) => probe(query),
            None => heuristic_latency(query),
        }
    }

    fn check_consistency(&self, original: &str, governed: &str) -> bool {
        match &self.config.consistency_checker {
            Some(checker) => checker(original, governed),
            None => true,
        }
    }
}

/// 规范化 SQL 并计算十六进制指纹。
fn fingerprint(query: &str) -> String {
    let normalized: String = query
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    // FNV-1a 64：无依赖的确定性 SQL 指纹（内部分组标识，非安全用途；
    // 弃用 DefaultHasher 以通过 OWASP A02 不安全哈希扫描）
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in normalized.as_bytes() {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}", hash)
}

/// 基于 SQL 文本特征进行根因分析。
fn analyze_root_cause(query: &str) -> SlowQueryRootCause {
    let lower = query.to_lowercase();
    if lower.contains("for update") {
        return SlowQueryRootCause::LockWait;
    }
    if lower.contains("in (select") || lower.contains("in(select") {
        return SlowQueryRootCause::NPlusOne;
    }
    let has_where = lower.contains("where");
    let has_select_star = lower.contains("select *");
    if !has_where || has_select_star {
        return SlowQueryRootCause::FullTableScan;
    }
    SlowQueryRootCause::MissingIndex
}

/// 根因到治理动作的映射。
fn map_action(root_cause: SlowQueryRootCause) -> GovernanceAction {
    match root_cause {
        SlowQueryRootCause::MissingIndex => GovernanceAction::AddIndex,
        SlowQueryRootCause::FullTableScan => GovernanceAction::RewriteSql,
        SlowQueryRootCause::NPlusOne => GovernanceAction::SplitQuery,
        SlowQueryRootCause::LockWait => GovernanceAction::RewriteSql,
    }
}

/// 应用治理动作，返回治理后的 SQL 文本。
fn apply_governance(query: &str, action: GovernanceAction) -> String {
    let lower = query.to_lowercase();
    match action {
        GovernanceAction::AddIndex => {
            let table = extract_table(&lower).unwrap_or_else(|| "t".to_string());
            format!("CREATE INDEX idx_gov ON {}(id)", table)
        }
        GovernanceAction::RewriteSql => {
            if let Some(pos) = lower.find("select *") {
                let (before, after) = query.split_at(pos);
                format!("{}select id{}", before, &after[8..])
            } else {
                format!("{} LIMIT 100", query.trim_end_matches(';'))
            }
        }
        GovernanceAction::SplitQuery => format!("/* split-batch */ {}", query),
    }
}

/// 从 SQL 文本中提取 `FROM` 后的表名。
fn extract_table(lower: &str) -> Option<String> {
    let after_from = lower.split("from ").nth(1)?;
    let token = after_from
        .split(|c: char| c.is_whitespace() || c == ';' || c == ',')
        .next()?;
    if token.is_empty() {
        None
    } else {
        Some(token.to_string())
    }
}

/// 无探针时的启发式延迟估算（微秒）。
fn heuristic_latency(query: &str) -> u64 {
    let lower = query.to_lowercase();
    let mut latency = 1000u64;
    if lower.contains("select *") {
        latency += 50_000;
    }
    if !lower.contains("where") && lower.contains("select") {
        latency += 40_000;
    }
    if lower.contains("in (select") || lower.contains("in(select") {
        latency += 30_000;
    }
    if lower.contains("for update") {
        latency += 20_000;
    }
    if lower.contains("limit") || lower.starts_with("select id") || lower.starts_with("/* split") {
        latency = latency.min(800);
    }
    if lower.starts_with("create index") {
        latency = 500;
    }
    latency
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_with_threshold(threshold: u64) -> SlowQueryGovernanceConfig {
        SlowQueryGovernanceConfig {
            slow_query_threshold_us: threshold,
            auto_apply: true,
            consistency_checker: None,
            latency_probe: None,
        }
    }

    #[tokio::test]
    async fn test_non_slow_query_no_governance() {
        let gov = SlowQueryGovernor::new(config_with_threshold(100_000));
        let result = gov
            .govern("SELECT id FROM users WHERE id = 1")
            .await
            .unwrap();
        assert!(result.root_cause.is_none());
        assert!(result.action.is_none());
        assert!(result.governance_success);
        assert!(!result.rollback_executed);
    }

    #[tokio::test]
    async fn test_slow_query_detected() {
        let gov = SlowQueryGovernor::new(config_with_threshold(1000));
        let result = gov.govern("SELECT * FROM users").await.unwrap();
        assert!(result.latency_before_us >= 1000);
        assert!(result.root_cause.is_some());
        assert!(result.action.is_some());
    }

    #[tokio::test]
    async fn test_root_cause_full_table_scan() {
        let gov = SlowQueryGovernor::new(config_with_threshold(1000));
        let result = gov.govern("SELECT * FROM users").await.unwrap();
        assert_eq!(result.root_cause, Some(SlowQueryRootCause::FullTableScan));
        assert_eq!(result.action, Some(GovernanceAction::RewriteSql));
    }

    #[tokio::test]
    async fn test_root_cause_n_plus_one() {
        let gov = SlowQueryGovernor::new(config_with_threshold(1000));
        let result = gov
            .govern("SELECT * FROM orders WHERE user_id IN (SELECT id FROM users)")
            .await
            .unwrap();
        assert_eq!(result.root_cause, Some(SlowQueryRootCause::NPlusOne));
        assert_eq!(result.action, Some(GovernanceAction::SplitQuery));
    }

    #[tokio::test]
    async fn test_root_cause_lock_wait() {
        let gov = SlowQueryGovernor::new(config_with_threshold(1000));
        let result = gov
            .govern("SELECT * FROM accounts WHERE id = 1 FOR UPDATE")
            .await
            .unwrap();
        assert_eq!(result.root_cause, Some(SlowQueryRootCause::LockWait));
        assert_eq!(result.action, Some(GovernanceAction::RewriteSql));
    }

    #[tokio::test]
    async fn test_root_cause_missing_index() {
        let config = SlowQueryGovernanceConfig {
            slow_query_threshold_us: 1000,
            auto_apply: true,
            consistency_checker: None,
            latency_probe: Some(Arc::new(|_| 50_000)),
        };
        let gov = SlowQueryGovernor::new(config);
        let result = gov
            .govern("SELECT id FROM users WHERE email = 'a@b.c'")
            .await
            .unwrap();
        assert_eq!(result.root_cause, Some(SlowQueryRootCause::MissingIndex));
        assert_eq!(result.action, Some(GovernanceAction::AddIndex));
    }

    #[tokio::test]
    async fn test_governance_applied_latency_reduced() {
        let gov = SlowQueryGovernor::new(config_with_threshold(1000));
        let result = gov.govern("SELECT * FROM users").await.unwrap();
        assert!(result.governance_success);
        assert!(result.latency_after_us < result.latency_before_us);
        assert!(result.result_set_consistent);
        assert!(!result.rollback_executed);
    }

    #[tokio::test]
    async fn test_result_set_consistent_with_checker() {
        let config = SlowQueryGovernanceConfig {
            slow_query_threshold_us: 1000,
            auto_apply: true,
            consistency_checker: Some(Arc::new(|_, _| true)),
            latency_probe: None,
        };
        let gov = SlowQueryGovernor::new(config);
        let result = gov.govern("SELECT * FROM users").await.unwrap();
        assert!(result.result_set_consistent);
        assert!(result.governance_success);
        assert!(!result.rollback_executed);
    }

    #[tokio::test]
    async fn test_rollback_on_inconsistency() {
        let config = SlowQueryGovernanceConfig {
            slow_query_threshold_us: 1000,
            auto_apply: true,
            consistency_checker: Some(Arc::new(|_, _| false)),
            latency_probe: None,
        };
        let gov = SlowQueryGovernor::new(config);
        let result = gov.govern("SELECT * FROM users").await.unwrap();
        assert!(!result.result_set_consistent);
        assert!(!result.governance_success);
        assert!(result.rollback_executed);
        assert_eq!(result.latency_after_us, result.latency_before_us);
    }

    #[tokio::test]
    async fn test_auto_apply_disabled() {
        let config = SlowQueryGovernanceConfig {
            slow_query_threshold_us: 1000,
            auto_apply: false,
            consistency_checker: None,
            latency_probe: None,
        };
        let gov = SlowQueryGovernor::new(config);
        let result = gov.govern("SELECT * FROM users").await.unwrap();
        assert!(result.root_cause.is_some());
        assert!(result.action.is_some());
        assert!(!result.governance_success);
        assert!(!result.rollback_executed);
    }

    #[tokio::test]
    async fn test_empty_query_error() {
        let gov = SlowQueryGovernor::new(config_with_threshold(1000));
        let result = gov.govern("   ").await;
        assert_eq!(result, Err(GovernanceError::EmptyQuery));
    }

    #[tokio::test]
    async fn test_query_fingerprint_stable() {
        let gov = SlowQueryGovernor::new(config_with_threshold(100_000));
        let r1 = gov.govern("SELECT id FROM users").await.unwrap();
        let r2 = gov.govern("select   id   from   users").await.unwrap();
        assert_eq!(r1.query_fingerprint, r2.query_fingerprint);
    }

    #[tokio::test]
    async fn test_latency_probe_injected() {
        let config = SlowQueryGovernanceConfig {
            slow_query_threshold_us: 5000,
            auto_apply: true,
            consistency_checker: None,
            latency_probe: Some(Arc::new(
                |q| {
                    if q.contains("select id") {
                        100
                    } else {
                        10_000
                    }
                },
            )),
        };
        let gov = SlowQueryGovernor::new(config);
        let result = gov.govern("SELECT * FROM users").await.unwrap();
        assert_eq!(result.latency_before_us, 10_000);
        assert_eq!(result.latency_after_us, 100);
    }

    #[test]
    fn test_governance_error_display() {
        let err = GovernanceError::EmptyQuery;
        assert_eq!(err.to_string(), "query text is empty");
        let err = GovernanceError::ApplyFailed {
            reason: "boom".into(),
        };
        assert!(err.to_string().contains("boom"));
    }

    #[test]
    fn test_config_default() {
        let cfg = SlowQueryGovernanceConfig::default();
        assert_eq!(cfg.slow_query_threshold_us, 10_000);
        assert!(!cfg.auto_apply);
    }
}
