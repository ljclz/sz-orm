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
}
