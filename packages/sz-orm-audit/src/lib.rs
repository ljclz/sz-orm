//! # SZ-ORM Audit — SQL 审计日志
//!
//! 提供 SQL 执行审计记录，对 password/token/credit_card 等敏感关键词进行
//! 大小写不敏感脱敏，确保审计日志不泄露敏感信息。
//!
//! ## 主要类型
//!
//! - [`SqlAuditContext`] — 审计上下文（SQL/用户/时间戳）
//! - [`SqlAuditor`] — 审计执行器

use serde::{Deserialize, Serialize};
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqlAuditContext {
    pub sql: String,
    pub user: String,
    pub timestamp: i64,
}

/// Sensitive keywords that should be masked in audit logs. Matching is
/// case-insensitive on the ASCII bytes of the SQL string.
const SENSITIVE_KEYWORDS: &[&str] = &[
    "password",
    "pwd",
    "passwd",
    "secret",
    "token",
    "api_key",
    "apikey",
    "access_key",
    "accesskey",
    "session",
    "credit_card",
    "creditcard",
    "cvv",
    "ssn",
];

pub struct SqlAuditor {
    logs: Mutex<Vec<SqlAuditContext>>,
}

impl SqlAuditor {
    pub fn new() -> Self {
        Self {
            logs: Mutex::new(vec![]),
        }
    }

    /// Log an audit entry. The SQL is masked for sensitive keywords before
    /// being stored in the in-memory buffer.
    pub fn log(&self, ctx: &SqlAuditContext) {
        let masked_sql = mask_sensitive(&ctx.sql);
        let entry = SqlAuditContext {
            sql: masked_sql,
            user: ctx.user.clone(),
            timestamp: ctx.timestamp,
        };
        let mut logs = self.logs.lock().unwrap();
        logs.push(entry);
    }

    /// Return a snapshot of all stored audit entries.
    pub fn get_logs(&self) -> Vec<SqlAuditContext> {
        let logs = self.logs.lock().unwrap();
        logs.iter().cloned().collect()
    }

    /// Flush all stored audit entries to a JSON file at `path`.
    /// Returns the number of entries written.
    pub fn flush(&self, path: &str) -> Result<usize, String> {
        let logs = self.logs.lock().unwrap();
        let snapshot: Vec<&SqlAuditContext> = logs.iter().collect();
        let json = serde_json::to_string_pretty(&snapshot).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| e.to_string())?;
        Ok(logs.len())
    }

    /// Mask all sensitive keywords in `sql` with `******`. Matching is
    /// case-insensitive.
    pub fn mask_sensitive(&self, sql: &str) -> String {
        mask_sensitive(sql)
    }
}

impl Default for SqlAuditor {
    fn default() -> Self {
        Self::new()
    }
}

/// Mask all sensitive keywords in `sql` with `******`. Matching is
/// case-insensitive over the ASCII bytes of the string.
fn mask_sensitive(sql: &str) -> String {
    let lower = sql.to_ascii_lowercase();
    let mut result = String::with_capacity(sql.len());
    let mut i = 0;
    let bytes = sql.as_bytes();
    let lower_bytes = lower.as_bytes();
    while i < bytes.len() {
        let mut matched_len: Option<usize> = None;
        for keyword in SENSITIVE_KEYWORDS {
            let kw_bytes = keyword.as_bytes();
            if i + kw_bytes.len() <= bytes.len() && &lower_bytes[i..i + kw_bytes.len()] == kw_bytes
            {
                // Only treat as a keyword match if it's not part of a longer identifier.
                // Boundary check: previous and next char must be non-alphanumeric/underscore
                let prev_ok = i == 0 || !is_ident_char(bytes[i - 1]);
                let next_idx = i + kw_bytes.len();
                let next_ok = next_idx >= bytes.len() || !is_ident_char(bytes[next_idx]);
                if prev_ok && next_ok {
                    matched_len = Some(kw_bytes.len());
                    break;
                }
            }
        }
        if let Some(kw_len) = matched_len {
            result.push_str("******");
            i += kw_len;
        } else {
            // Push one char (handles UTF-8 properly since we step by char).
            let ch = sql[i..].chars().next().unwrap();
            result.push(ch);
            i += ch.len_utf8();
        }
    }
    result
}

/// Returns true if `b` is an ASCII identifier character (alphanumeric or '_').
fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

// ============================================================================
// 审计规则配置（允许/拒绝列表）
// ============================================================================

/// 审计规则：允许/拒绝列表，用于决定是否记录某条 SQL 审计日志。
///
/// 规则评估顺序：
/// 1. 若 SQL 命中拒绝列表中的任一模式 → 不记录
/// 2. 若允许列表为空 → 记录所有未命中拒绝列表的 SQL
/// 3. 若允许列表非空 → 仅记录命中允许列表的 SQL
#[derive(Debug, Clone, Default)]
pub struct AuditRules {
    /// 允许列表模式（大小写不敏感子串匹配），为空表示允许所有
    allow_patterns: Vec<String>,
    /// 拒绝列表模式（大小写不敏感子串匹配），命中则拒绝
    deny_patterns: Vec<String>,
}

impl AuditRules {
    pub fn new() -> Self {
        Self::default()
    }

    /// 添加允许模式（大小写不敏感子串匹配）
    pub fn allow(mut self, pattern: impl Into<String>) -> Self {
        self.allow_patterns.push(pattern.into().to_ascii_lowercase());
        self
    }

    /// 添加拒绝模式（大小写不敏感子串匹配）
    pub fn deny(mut self, pattern: impl Into<String>) -> Self {
        self.deny_patterns.push(pattern.into().to_ascii_lowercase());
        self
    }

    /// 判断给定 SQL 是否应该被审计记录
    pub fn should_audit(&self, sql: &str) -> bool {
        let lower = sql.to_ascii_lowercase();
        // 拒绝列表优先
        for pat in &self.deny_patterns {
            if lower.contains(pat) {
                return false;
            }
        }
        // 允许列表为空 → 允许所有
        if self.allow_patterns.is_empty() {
            return true;
        }
        // 允许列表非空 → 仅允许命中项
        self.allow_patterns.iter().any(|pat| lower.contains(pat))
    }

    /// 返回允许模式数量
    pub fn allow_count(&self) -> usize {
        self.allow_patterns.len()
    }

    /// 返回拒绝模式数量
    pub fn deny_count(&self) -> usize {
        self.deny_patterns.len()
    }
}

// ============================================================================
// 审计日志轮转策略（按大小/时间）
// ============================================================================

/// 审计日志轮转策略配置。
///
/// - `max_entries`：内存中最多保留的日志条数，超过后自动轮转（旧日志清空或落盘）
/// - `max_age_ms`：日志最大存活时间（毫秒），超过后触发轮转
#[derive(Debug, Clone)]
pub struct RotationPolicy {
    /// 最大条目数（0 表示不限制）
    pub max_entries: usize,
    /// 最大存活时间毫秒（0 表示不限制）
    pub max_age_ms: i64,
}

impl RotationPolicy {
    /// 创建不限制的轮转策略
    pub fn none() -> Self {
        Self {
            max_entries: 0,
            max_age_ms: 0,
        }
    }

    /// 按条目数轮转
    pub fn by_size(max_entries: usize) -> Self {
        Self {
            max_entries,
            max_age_ms: 0,
        }
    }

    /// 按时间轮转（毫秒）
    pub fn by_age(max_age_ms: i64) -> Self {
        Self {
            max_entries: 0,
            max_age_ms,
        }
    }

    /// 同时按条目数和时间轮转
    pub fn by_size_and_age(max_entries: usize, max_age_ms: i64) -> Self {
        Self {
            max_entries,
            max_age_ms,
        }
    }

    /// 判断是否需要轮转
    fn needs_rotation(&self, entry_count: usize, oldest_ts: i64, now_ts: i64) -> bool {
        if self.max_entries > 0 && entry_count >= self.max_entries {
            return true;
        }
        if self.max_age_ms > 0 && oldest_ts > 0 && (now_ts - oldest_ts) > self.max_age_ms {
            return true;
        }
        false
    }
}

impl Default for RotationPolicy {
    fn default() -> Self {
        Self::none()
    }
}

// ============================================================================
// 带轮转和规则的审计器
// ============================================================================

/// 带轮转策略和审计规则的增强审计器。
///
/// 在 `SqlAuditor` 基础上增加：
/// - 日志轮转（按大小/时间自动清理旧日志）
/// - 审计规则（允许/拒绝列表过滤）
pub struct RotatingAuditor {
    logs: Mutex<Vec<SqlAuditContext>>,
    rules: AuditRules,
    policy: RotationPolicy,
    /// 已轮转（清理）的次数
    rotations: Mutex<usize>,
}

impl RotatingAuditor {
    pub fn new(policy: RotationPolicy, rules: AuditRules) -> Self {
        Self {
            logs: Mutex::new(vec![]),
            rules,
            policy,
            rotations: Mutex::new(0),
        }
    }

    /// 创建仅按大小轮转的审计器，无规则过滤
    pub fn with_max_entries(max_entries: usize) -> Self {
        Self::new(RotationPolicy::by_size(max_entries), AuditRules::new())
    }

    /// 创建仅按时间轮转的审计器，无规则过滤
    pub fn with_max_age(max_age_ms: i64) -> Self {
        Self::new(RotationPolicy::by_age(max_age_ms), AuditRules::new())
    }

    /// 记录审计日志，自动应用规则过滤和轮转策略
    pub fn log(&self, ctx: &SqlAuditContext) -> bool {
        // 规则过滤
        if !self.rules.should_audit(&ctx.sql) {
            return false;
        }
        let masked_sql = mask_sensitive(&ctx.sql);
        let entry = SqlAuditContext {
            sql: masked_sql,
            user: ctx.user.clone(),
            timestamp: ctx.timestamp,
        };
        let mut logs = self.logs.lock().unwrap();

        // 在添加新条目前检查轮转（确保新条目不被立即清除）
        let now = ctx.timestamp;
        let oldest = logs.first().map(|e| e.timestamp).unwrap_or(now);
        if self.policy.needs_rotation(logs.len(), oldest, now) {
            logs.clear();
            *self.rotations.lock().unwrap() += 1;
        }

        logs.push(entry);
        true
    }

    /// 返回当前日志快照
    pub fn get_logs(&self) -> Vec<SqlAuditContext> {
        self.logs.lock().unwrap().clone()
    }

    /// 返回已轮转次数
    pub fn rotation_count(&self) -> usize {
        *self.rotations.lock().unwrap()
    }

    /// 手动触发轮转（清空当前日志）
    pub fn rotate(&self) -> usize {
        let mut logs = self.logs.lock().unwrap();
        let count = logs.len();
        logs.clear();
        *self.rotations.lock().unwrap() += 1;
        count
    }

    /// 返回当前日志条数
    pub fn len(&self) -> usize {
        self.logs.lock().unwrap().len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.logs.lock().unwrap().is_empty()
    }
}

// ============================================================================
// 异步审计写入器
// ============================================================================

/// 异步审计写入器：通过后台线程异步写入审计日志，避免阻塞主线程。
///
/// 使用 `std::sync::mpsc` 通道将日志发送到后台线程，后台线程负责存储。
/// 关闭时调用 `shutdown` 等待后台线程退出并返回所有已写入的日志。
pub struct AsyncAuditWriter {
    sender: std::sync::mpsc::Sender<AsyncCommand>,
    handle: Mutex<Option<std::thread::JoinHandle<Vec<SqlAuditContext>>>>,
}

enum AsyncCommand {
    Log(SqlAuditContext),
    Shutdown,
}

impl AsyncAuditWriter {
    /// 创建异步写入器，启动后台线程
    pub fn new() -> Self {
        let (sender, receiver) = std::sync::mpsc::channel::<AsyncCommand>();
        let handle = std::thread::spawn(move || {
            let mut logs: Vec<SqlAuditContext> = Vec::new();
            for cmd in receiver {
                match cmd {
                    AsyncCommand::Log(ctx) => {
                        let masked_sql = mask_sensitive(&ctx.sql);
                        logs.push(SqlAuditContext {
                            sql: masked_sql,
                            user: ctx.user,
                            timestamp: ctx.timestamp,
                        });
                    }
                    AsyncCommand::Shutdown => break,
                }
            }
            logs
        });
        Self {
            sender,
            handle: Mutex::new(Some(handle)),
        }
    }

    /// 异步记录审计日志（非阻塞）
    pub fn log(&self, ctx: &SqlAuditContext) -> Result<(), String> {
        self.sender
            .send(AsyncCommand::Log(ctx.clone()))
            .map_err(|e| format!("AsyncAuditWriter channel closed: {}", e))
    }

    /// 关闭后台线程并返回所有已写入的日志
    pub fn shutdown(&self) -> Result<Vec<SqlAuditContext>, String> {
        let _ = self.sender.send(AsyncCommand::Shutdown);
        let mut handle_guard = self.handle.lock().unwrap();
        if let Some(handle) = handle_guard.take() {
            handle
                .join()
                .map_err(|e| format!("Thread panicked: {:?}", e))
        } else {
            Err("Already shut down".to_string())
        }
    }
}

impl Default for AsyncAuditWriter {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// 审计日志查询过滤
// ============================================================================

/// 审计日志查询过滤器
#[derive(Debug, Clone, Default)]
pub struct AuditQuery {
    /// 按用户名过滤（精确匹配，None 表示不过滤）
    pub user: Option<String>,
    /// 时间范围起始（毫秒时间戳，None 表示不限制下限）
    pub from_ts: Option<i64>,
    /// 时间范围结束（毫秒时间戳，None 表示不限制上限）
    pub to_ts: Option<i64>,
    /// SQL 关键词过滤（大小写不敏感子串匹配，None 表示不过滤）
    pub sql_contains: Option<String>,
    /// 限制返回条数（0 表示不限制）
    pub limit: usize,
}

impl AuditQuery {
    pub fn new() -> Self {
        Self::default()
    }

    /// 按用户名过滤
    pub fn by_user(mut self, user: impl Into<String>) -> Self {
        self.user = Some(user.into());
        self
    }

    /// 按时间范围过滤（毫秒时间戳）
    pub fn by_time_range(mut self, from: i64, to: i64) -> Self {
        self.from_ts = Some(from);
        self.to_ts = Some(to);
        self
    }

    /// 按 SQL 关键词过滤（大小写不敏感）
    pub fn by_sql_contains(mut self, keyword: impl Into<String>) -> Self {
        self.sql_contains = Some(keyword.into());
        self
    }

    /// 限制返回条数
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    /// 对日志列表执行查询过滤
    pub fn filter(&self, logs: &[SqlAuditContext]) -> Vec<SqlAuditContext> {
        let keyword_lower = self.sql_contains.as_ref().map(|s| s.to_ascii_lowercase());
        let mut result: Vec<SqlAuditContext> = logs
            .iter()
            .filter(|entry| {
                if let Some(u) = &self.user {
                    if entry.user != *u {
                        return false;
                    }
                }
                if let Some(from) = self.from_ts {
                    if entry.timestamp < from {
                        return false;
                    }
                }
                if let Some(to) = self.to_ts {
                    if entry.timestamp > to {
                        return false;
                    }
                }
                if let Some(kw) = &keyword_lower {
                    if !entry.sql.to_ascii_lowercase().contains(kw) {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect();
        if self.limit > 0 && result.len() > self.limit {
            result.truncate(self.limit);
        }
        result
    }
}

/// 从 `SqlAuditor` 的日志中按条件查询
pub fn query_logs(auditor: &SqlAuditor, query: &AuditQuery) -> Vec<SqlAuditContext> {
    let logs = auditor.get_logs();
    query.filter(&logs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(sql: &str, user: &str, ts: i64) -> SqlAuditContext {
        SqlAuditContext {
            sql: sql.to_string(),
            user: user.to_string(),
            timestamp: ts,
        }
    }

    #[test]
    fn test_log_stores_in_memory() {
        let a = SqlAuditor::new();
        a.log(&ctx("SELECT * FROM users", "admin", 1000));
        a.log(&ctx("INSERT INTO logs VALUES(1)", "admin", 1001));
        let logs = a.get_logs();
        assert_eq!(logs.len(), 2);
        assert_eq!(logs[0].sql, "SELECT * FROM users");
        assert_eq!(logs[0].user, "admin");
        assert_eq!(logs[0].timestamp, 1000);
        assert_eq!(logs[1].timestamp, 1001);
    }

    #[test]
    fn test_log_masks_sensitive_in_storage() {
        let a = SqlAuditor::new();
        a.log(&ctx(
            "SELECT * FROM users WHERE password='secret'",
            "admin",
            1000,
        ));
        let logs = a.get_logs();
        assert_eq!(logs.len(), 1);
        let stored_sql = &logs[0].sql;
        assert!(!stored_sql.contains("password"));
        assert!(!stored_sql.contains("secret"));
        assert!(stored_sql.contains("******"));
    }

    #[test]
    fn test_mask_sensitive_password() {
        let a = SqlAuditor::new();
        let masked = a.mask_sensitive("SELECT * FROM users WHERE password='secret'");
        assert!(!masked.contains("password"));
        assert!(!masked.contains("secret"));
        assert!(masked.contains("******"));
    }

    #[test]
    fn test_mask_sensitive_case_insensitive() {
        let a = SqlAuditor::new();
        let masked = a.mask_sensitive("UPDATE users SET PASSWORD='abc', Token='x'");
        let lower = masked.to_lowercase();
        assert!(!lower.contains("password"));
        assert!(!lower.contains("token"));
        assert!(masked.contains("******"));
    }

    #[test]
    fn test_mask_sensitive_extended_keywords() {
        let a = SqlAuditor::new();
        let inputs = [
            "pwd",
            "passwd",
            "secret",
            "api_key",
            "access_key",
            "session",
            "credit_card",
            "cvv",
            "ssn",
        ];
        for kw in inputs {
            let sql = format!("SELECT * FROM t WHERE k = '{}'", kw);
            let masked = a.mask_sensitive(&sql);
            let lower = masked.to_lowercase();
            assert!(
                !lower.contains(kw),
                "keyword '{}' should be masked in: {}",
                kw,
                masked
            );
            assert!(masked.contains("******"));
        }
    }

    #[test]
    fn test_mask_sensitive_preserves_non_sensitive() {
        let a = SqlAuditor::new();
        let masked = a.mask_sensitive("SELECT id, name FROM users WHERE active = 1");
        assert_eq!(masked, "SELECT id, name FROM users WHERE active = 1");
    }

    #[test]
    fn test_mask_sensitive_does_not_match_substrings() {
        // "passworded" should not be partially matched as "password"
        // because we require word boundaries.
        let a = SqlAuditor::new();
        let masked = a.mask_sensitive("SELECT * FROM users WHERE note='passworded'");
        // The 'passworded' word should remain intact because of boundary check
        assert!(masked.contains("passworded"));
        // Ensure we did NOT replace anything (no ****** from this substring)
        // Actually, "passworded" still has 'password' as a prefix but our
        // boundary check requires the char AFTER the keyword to be non-ident.
        // In "passworded", after "password" comes "e" which IS an ident char,
        // so it should NOT be matched.
        assert_eq!(masked, "SELECT * FROM users WHERE note='passworded'");
    }

    #[test]
    fn test_mask_sensitive_multiple_occurrences() {
        let a = SqlAuditor::new();
        let masked = a.mask_sensitive("INSERT INTO t (password, token) VALUES ('p1', 't1')");
        // Both keywords should be masked
        let lower = masked.to_lowercase();
        assert!(!lower.contains("password"));
        assert!(!lower.contains("token"));
        // Verify there are at least 2 mask replacements
        let count = masked.matches("******").count();
        assert!(count >= 2, "expected at least 2 masks, got: {}", masked);
    }

    #[test]
    fn test_get_logs_empty_initially() {
        let a = SqlAuditor::new();
        assert!(a.get_logs().is_empty());
    }

    #[test]
    fn test_get_logs_returns_snapshot_independent_of_changes() {
        let a = SqlAuditor::new();
        a.log(&ctx("SELECT 1", "u", 1));
        let snap = a.get_logs();
        a.log(&ctx("SELECT 2", "u", 2));
        assert_eq!(snap.len(), 1, "snapshot should not change after new log");
        assert_eq!(a.get_logs().len(), 2);
    }

    #[test]
    fn test_flush_writes_json_file() {
        let a = SqlAuditor::new();
        a.log(&ctx("SELECT * FROM users WHERE password='p'", "admin", 123));
        a.log(&ctx("INSERT INTO logs VALUES(1)", "user2", 456));
        let path = std::env::temp_dir().join("sz_orm_audit_flush_test.json");
        let path_str = path.to_str().unwrap();
        let count = a.flush(path_str).expect("flush should succeed");
        assert_eq!(count, 2);
        // Read back the file and verify it contains valid JSON
        let content = std::fs::read_to_string(path_str).expect("file should be readable");
        let parsed: Vec<SqlAuditContext> =
            serde_json::from_str(&content).expect("should parse as JSON array");
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].user, "admin");
        assert_eq!(parsed[1].timestamp, 456);
        // Verify masking was applied during log()
        assert!(!parsed[0].sql.contains("password"));
        // Cleanup
        let _ = std::fs::remove_file(path_str);
    }

    #[test]
    fn test_flush_empty_writes_empty_array() {
        let a = SqlAuditor::new();
        let path = std::env::temp_dir().join("sz_orm_audit_flush_empty_test.json");
        let path_str = path.to_str().unwrap();
        let count = a.flush(path_str).expect("flush should succeed");
        assert_eq!(count, 0);
        let content = std::fs::read_to_string(path_str).expect("file should be readable");
        assert_eq!(content.trim(), "[]");
        let _ = std::fs::remove_file(path_str);
    }

    #[test]
    fn test_default_creates_new_auditor() {
        let a = SqlAuditor::default();
        assert!(a.get_logs().is_empty());
    }

    #[test]
    fn test_original_test_compatibility() {
        // Backward compatibility: the original test asserts that masking
        // "SELECT * FROM users WHERE password='secret'" removes "password".
        let a = SqlAuditor::new();
        let masked = a.mask_sensitive("SELECT * FROM users WHERE password='secret'");
        assert!(!masked.contains("password"));
    }

    // ===== 审计规则配置测试 =====

    #[test]
    fn test_audit_rules_empty_allows_all() {
        let rules = AuditRules::new();
        assert!(rules.should_audit("SELECT * FROM users"));
        assert!(rules.should_audit("DELETE FROM orders"));
        assert_eq!(rules.allow_count(), 0);
        assert_eq!(rules.deny_count(), 0);
    }

    #[test]
    fn test_audit_rules_deny_blocks() {
        let rules = AuditRules::new().deny("pg_catalog");
        assert!(!rules.should_audit("SELECT * FROM pg_catalog.tables"));
        assert!(rules.should_audit("SELECT * FROM users"));
    }

    #[test]
    fn test_audit_rules_allow_filters() {
        let rules = AuditRules::new().allow("select").allow("insert");
        assert!(rules.should_audit("SELECT * FROM users"));
        assert!(rules.should_audit("INSERT INTO logs VALUES(1)"));
        assert!(!rules.should_audit("DELETE FROM users"));
    }

    #[test]
    fn test_audit_rules_deny_overrides_allow() {
        let rules = AuditRules::new()
            .allow("select")
            .deny("password");
        // 包含 password 的 SELECT 应被拒绝
        assert!(!rules.should_audit("SELECT * FROM users WHERE password='x'"));
        // 不含 password 的 SELECT 应被允许
        assert!(rules.should_audit("SELECT * FROM users"));
    }

    #[test]
    fn test_audit_rules_case_insensitive() {
        let rules = AuditRules::new().deny("DROP");
        assert!(!rules.should_audit("drop table users"));
        assert!(!rules.should_audit("DROP TABLE users"));
        assert!(rules.should_audit("SELECT * FROM users"));
    }

    #[test]
    fn test_audit_rules_multiple_deny() {
        let rules = AuditRules::new()
            .deny("drop")
            .deny("truncate")
            .deny("shutdown");
        assert!(!rules.should_audit("DROP TABLE x"));
        assert!(!rules.should_audit("TRUNCATE TABLE y"));
        assert!(!rules.should_audit("SHUTDOWN"));
        assert!(rules.should_audit("SELECT 1"));
    }

    // ===== 轮转策略测试 =====

    #[test]
    fn test_rotation_policy_none_never_rotates() {
        let policy = RotationPolicy::none();
        assert!(!policy.needs_rotation(1_000_000, 0, 1_000_000));
        assert!(!policy.needs_rotation(0, 0, 0));
    }

    #[test]
    fn test_rotation_policy_by_size() {
        let policy = RotationPolicy::by_size(100);
        assert!(!policy.needs_rotation(99, 0, 1000));
        assert!(policy.needs_rotation(100, 0, 1000));
        assert!(policy.needs_rotation(200, 0, 1000));
    }

    #[test]
    fn test_rotation_policy_by_age() {
        let policy = RotationPolicy::by_age(5000);
        // 旧日志 4000ms 前，未超时
        assert!(!policy.needs_rotation(10, 5000, 9000));
        // 旧日志 6000ms 前，已超时
        assert!(policy.needs_rotation(10, 5000, 11000));
    }

    #[test]
    fn test_rotation_policy_by_size_and_age() {
        let policy = RotationPolicy::by_size_and_age(100, 5000);
        // 大小未达上限且时间未超 → 不轮转
        assert!(!policy.needs_rotation(50, 5000, 9000));
        // 大小达上限 → 轮转
        assert!(policy.needs_rotation(100, 5000, 5000));
        // 时间超 → 轮转
        assert!(policy.needs_rotation(10, 5000, 11000));
    }

    // ===== RotatingAuditor 测试 =====

    #[test]
    fn test_rotating_auditor_no_rotation_stores_all() {
        let auditor = RotatingAuditor::new(RotationPolicy::none(), AuditRules::new());
        for i in 0..100 {
            auditor.log(&ctx(&format!("SELECT {}", i), "user", i));
        }
        assert_eq!(auditor.len(), 100);
        assert_eq!(auditor.rotation_count(), 0);
    }

    #[test]
    fn test_rotating_auditor_rotates_by_size() {
        let auditor = RotatingAuditor::with_max_entries(5);
        for i in 0..5 {
            auditor.log(&ctx(&format!("SELECT {}", i), "user", i));
        }
        assert_eq!(auditor.len(), 5);
        assert_eq!(auditor.rotation_count(), 0);
        // 第 6 条触发轮转
        auditor.log(&ctx("SELECT 6", "user", 100));
        assert_eq!(auditor.len(), 1);
        assert_eq!(auditor.rotation_count(), 1);
    }

    #[test]
    fn test_rotating_auditor_rotates_by_age() {
        let auditor = RotatingAuditor::with_max_age(1000);
        auditor.log(&ctx("SELECT 1", "user", 100));
        auditor.log(&ctx("SELECT 2", "user", 200));
        assert_eq!(auditor.len(), 2);
        assert_eq!(auditor.rotation_count(), 0);
        // 时间差超过 1000ms → 轮转
        auditor.log(&ctx("SELECT 3", "user", 1500));
        assert_eq!(auditor.len(), 1);
        assert_eq!(auditor.rotation_count(), 1);
    }

    #[test]
    fn test_rotating_auditor_rules_filter() {
        let rules = AuditRules::new().deny("drop").allow("select");
        let auditor = RotatingAuditor::new(RotationPolicy::none(), rules);
        let logged1 = auditor.log(&ctx("SELECT * FROM users", "u", 1));
        let logged2 = auditor.log(&ctx("DROP TABLE users", "u", 2));
        let logged3 = auditor.log(&ctx("DELETE FROM users", "u", 3));
        assert!(logged1);
        assert!(!logged2);
        assert!(!logged3);
        assert_eq!(auditor.len(), 1);
    }

    #[test]
    fn test_rotating_auditor_manual_rotate() {
        let auditor = RotatingAuditor::with_max_entries(100);
        auditor.log(&ctx("SELECT 1", "u", 1));
        auditor.log(&ctx("SELECT 2", "u", 2));
        let cleared = auditor.rotate();
        assert_eq!(cleared, 2);
        assert!(auditor.is_empty());
        assert_eq!(auditor.rotation_count(), 1);
    }

    #[test]
    fn test_rotating_auditor_masks_sensitive() {
        let auditor = RotatingAuditor::with_max_entries(100);
        auditor.log(&ctx("SELECT * FROM users WHERE password='x'", "u", 1));
        let logs = auditor.get_logs();
        assert_eq!(logs.len(), 1);
        assert!(!logs[0].sql.contains("password"));
        assert!(logs[0].sql.contains("******"));
    }

    #[test]
    fn test_rotating_auditor_get_logs_snapshot() {
        let auditor = RotatingAuditor::with_max_entries(100);
        auditor.log(&ctx("SELECT 1", "u", 1));
        let snap = auditor.get_logs();
        auditor.log(&ctx("SELECT 2", "u", 2));
        assert_eq!(snap.len(), 1, "snapshot should be independent");
        assert_eq!(auditor.len(), 2);
    }

    // ===== 异步审计写入器测试 =====

    #[test]
    fn test_async_writer_log_and_shutdown() {
        let writer = AsyncAuditWriter::new();
        writer.log(&ctx("SELECT * FROM users", "admin", 1000)).unwrap();
        writer.log(&ctx("INSERT INTO logs VALUES(1)", "user2", 2000)).unwrap();
        let logs = writer.shutdown().expect("shutdown should succeed");
        assert_eq!(logs.len(), 2);
        assert_eq!(logs[0].user, "admin");
        assert_eq!(logs[1].timestamp, 2000);
    }

    #[test]
    fn test_async_writer_masks_sensitive() {
        let writer = AsyncAuditWriter::new();
        writer.log(&ctx("SELECT * FROM users WHERE password='secret'", "u", 1)).unwrap();
        let logs = writer.shutdown().unwrap();
        assert_eq!(logs.len(), 1);
        assert!(!logs[0].sql.contains("password"));
    }

    #[test]
    fn test_async_writer_empty_shutdown() {
        let writer = AsyncAuditWriter::new();
        let logs = writer.shutdown().expect("shutdown should succeed");
        assert!(logs.is_empty());
    }

    #[test]
    fn test_async_writer_double_shutdown_errors() {
        let writer = AsyncAuditWriter::new();
        let _ = writer.shutdown().unwrap();
        let result = writer.shutdown();
        assert!(result.is_err(), "double shutdown should error");
    }

    #[test]
    fn test_async_writer_default() {
        let writer = AsyncAuditWriter::default();
        writer.log(&ctx("SELECT 1", "u", 1)).unwrap();
        let logs = writer.shutdown().unwrap();
        assert_eq!(logs.len(), 1);
    }

    // ===== 审计日志查询过滤测试 =====

    #[test]
    fn test_audit_query_by_user() {
        let auditor = SqlAuditor::new();
        auditor.log(&ctx("SELECT 1", "alice", 100));
        auditor.log(&ctx("SELECT 2", "bob", 200));
        auditor.log(&ctx("SELECT 3", "alice", 300));
        let query = AuditQuery::new().by_user("alice");
        let results = query_logs(&auditor, &query);
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.user == "alice"));
    }

    #[test]
    fn test_audit_query_by_time_range() {
        let auditor = SqlAuditor::new();
        auditor.log(&ctx("SELECT 1", "u", 100));
        auditor.log(&ctx("SELECT 2", "u", 200));
        auditor.log(&ctx("SELECT 3", "u", 300));
        auditor.log(&ctx("SELECT 4", "u", 400));
        let query = AuditQuery::new().by_time_range(150, 350);
        let results = query_logs(&auditor, &query);
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.timestamp >= 150 && r.timestamp <= 350));
    }

    #[test]
    fn test_audit_query_by_sql_contains() {
        let auditor = SqlAuditor::new();
        auditor.log(&ctx("SELECT * FROM users", "u", 1));
        auditor.log(&ctx("INSERT INTO orders", "u", 2));
        auditor.log(&ctx("SELECT * FROM orders", "u", 3));
        let query = AuditQuery::new().by_sql_contains("orders");
        let results = query_logs(&auditor, &query);
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.sql.to_lowercase().contains("orders")));
    }

    #[test]
    fn test_audit_query_sql_contains_case_insensitive() {
        let auditor = SqlAuditor::new();
        auditor.log(&ctx("select * from Users", "u", 1));
        let query = AuditQuery::new().by_sql_contains("USERS");
        let results = query_logs(&auditor, &query);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_audit_query_with_limit() {
        let auditor = SqlAuditor::new();
        for i in 0..10 {
            auditor.log(&ctx(&format!("SELECT {}", i), "u", i));
        }
        let query = AuditQuery::new().with_limit(3);
        let results = query_logs(&auditor, &query);
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_audit_query_combined_filters() {
        let auditor = SqlAuditor::new();
        auditor.log(&ctx("SELECT * FROM users", "alice", 100));
        auditor.log(&ctx("INSERT INTO users", "alice", 200));
        auditor.log(&ctx("SELECT * FROM orders", "alice", 300));
        auditor.log(&ctx("SELECT * FROM users", "bob", 400));
        let query = AuditQuery::new()
            .by_user("alice")
            .by_sql_contains("select")
            .with_limit(10);
        let results = query_logs(&auditor, &query);
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.user == "alice"));
    }

    #[test]
    fn test_audit_query_empty_returns_all() {
        let auditor = SqlAuditor::new();
        auditor.log(&ctx("SELECT 1", "u", 1));
        auditor.log(&ctx("SELECT 2", "u", 2));
        let query = AuditQuery::new();
        let results = query_logs(&auditor, &query);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_audit_query_no_match_returns_empty() {
        let auditor = SqlAuditor::new();
        auditor.log(&ctx("SELECT 1", "u", 1));
        let query = AuditQuery::new().by_user("nonexistent");
        let results = query_logs(&auditor, &query);
        assert!(results.is_empty());
    }

    #[test]
    fn test_audit_query_filter_directly() {
        let logs = vec![
            ctx("SELECT 1", "a", 10),
            ctx("SELECT 2", "b", 20),
            ctx("SELECT 3", "a", 30),
        ];
        let query = AuditQuery::new().by_user("a");
        let results = query.filter(&logs);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_audit_query_limit_zero_means_no_limit() {
        let logs = vec![
            ctx("SELECT 1", "a", 10),
            ctx("SELECT 2", "a", 20),
        ];
        let query = AuditQuery::new().with_limit(0);
        let results = query.filter(&logs);
        assert_eq!(results.len(), 2);
    }
}
