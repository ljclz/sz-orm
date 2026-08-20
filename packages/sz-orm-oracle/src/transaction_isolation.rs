//! Oracle 事务隔离级别配置
//!
//! 提供 [`TransactionIsolation`] 与 [`TransactionConfig`] 用于配置 Oracle
//! 事务的隔离级别、只读、超时、保存点等行为。

use std::fmt;
use std::time::Duration;

/// Oracle 事务隔离级别
///
/// Oracle 实际只支持 READ COMMITTED 与 SERIALIZABLE（以及只读事务）。
/// REPEATABLE READ 在 Oracle 中无直接对应，映射到 SERIALIZABLE。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[derive(Default)]
pub enum TransactionIsolation {
    /// 读已提交（Oracle 默认）
    #[default]
    ReadCommitted,
    /// 串行化
    Serializable,
    /// 只读事务（等效于 SERIALIZABLE 但不允许 DML）
    ReadCommittedReadOnly,
}

impl TransactionIsolation {
    /// 返回 SQL 语句片段
    #[must_use]
    pub fn as_sql(&self) -> &'static str {
        match self {
            TransactionIsolation::ReadCommitted => "READ COMMITTED",
            TransactionIsolation::Serializable => "SERIALIZABLE",
            TransactionIsolation::ReadCommittedReadOnly => "READ ONLY",
        }
    }

    /// 返回描述
    #[must_use]
    pub fn description(&self) -> &'static str {
        match self {
            TransactionIsolation::ReadCommitted => "read committed (default)",
            TransactionIsolation::Serializable => "serializable",
            TransactionIsolation::ReadCommittedReadOnly => "read only",
        }
    }

    /// 是否允许写操作
    #[must_use]
    pub fn allows_writes(&self) -> bool {
        matches!(
            self,
            TransactionIsolation::ReadCommitted | TransactionIsolation::Serializable
        )
    }

    /// 是否为最高隔离级别
    #[must_use]
    pub fn is_strongest(&self) -> bool {
        matches!(self, TransactionIsolation::Serializable)
    }

    /// 生成 SET TRANSACTION 语句
    #[must_use]
    pub fn set_transaction_sql(&self) -> String {
        match self {
            TransactionIsolation::ReadCommittedReadOnly => "SET TRANSACTION READ ONLY".to_string(),
            _ => format!("SET TRANSACTION ISOLATION LEVEL {}", self.as_sql()),
        }
    }

    /// 生成 ALTER SESSION 语句
    #[must_use]
    pub fn alter_session_sql(&self) -> String {
        match self {
            TransactionIsolation::ReadCommitted => {
                "ALTER SESSION SET ISOLATION_LEVEL = READ COMMITTED".to_string()
            }
            TransactionIsolation::Serializable => {
                "ALTER SESSION SET ISOLATION_LEVEL = SERIALIZABLE".to_string()
            }
            TransactionIsolation::ReadCommittedReadOnly => "SET TRANSACTION READ ONLY".to_string(),
        }
    }
}

impl fmt::Display for TransactionIsolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_sql())
    }
}


/// 事务访问模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[derive(Default)]
pub enum AccessMode {
    /// 读写模式
    #[default]
    ReadWrite,
    /// 只读模式
    ReadOnly,
}

impl AccessMode {
    /// 返回 SQL 关键字
    #[must_use]
    pub fn as_sql(&self) -> &'static str {
        match self {
            AccessMode::ReadWrite => "READ WRITE",
            AccessMode::ReadOnly => "READ ONLY",
        }
    }
}


/// 事务配置
#[derive(Debug, Clone)]
pub struct TransactionConfig {
    /// 隔离级别
    pub isolation: TransactionIsolation,
    /// 访问模式
    pub access_mode: AccessMode,
    /// 事务超时（秒，None 表示无超时）
    pub timeout: Option<Duration>,
    /// 是否自动提交
    pub auto_commit: bool,
    /// 保存点列表
    pub savepoints: Vec<String>,
    /// DDL 模式（是否允许事务内 DDL）
    pub allow_ddl: bool,
    /// 批量失败时的行为
    pub on_batch_error: BatchErrorAction,
}

impl Default for TransactionConfig {
    fn default() -> Self {
        Self {
            isolation: TransactionIsolation::ReadCommitted,
            access_mode: AccessMode::ReadWrite,
            timeout: None,
            auto_commit: false,
            savepoints: Vec::new(),
            allow_ddl: false,
            on_batch_error: BatchErrorAction::Rollback,
        }
    }
}

impl TransactionConfig {
    /// 创建默认配置
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置隔离级别
    #[must_use]
    pub fn with_isolation(mut self, isolation: TransactionIsolation) -> Self {
        self.isolation = isolation;
        if matches!(isolation, TransactionIsolation::ReadCommittedReadOnly) {
            self.access_mode = AccessMode::ReadOnly;
        }
        self
    }

    /// 设置访问模式
    #[must_use]
    pub fn with_access_mode(mut self, mode: AccessMode) -> Self {
        self.access_mode = mode;
        self
    }

    /// 设置超时
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// 启用自动提交
    #[must_use]
    pub fn with_auto_commit(mut self) -> Self {
        self.auto_commit = true;
        self
    }

    /// 允许事务内 DDL
    #[must_use]
    pub fn with_ddl(mut self) -> Self {
        self.allow_ddl = true;
        self
    }

    /// 设置批量错误行为
    #[must_use]
    pub fn with_batch_error_action(mut self, action: BatchErrorAction) -> Self {
        self.on_batch_error = action;
        self
    }

    /// 添加保存点
    #[must_use]
    pub fn add_savepoint(mut self, name: &str) -> Self {
        self.savepoints.push(name.to_string());
        self
    }

    /// 生成 SET TRANSACTION 语句
    #[must_use]
    pub fn begin_sql(&self) -> String {
        let mut parts = Vec::new();
        if matches!(self.isolation, TransactionIsolation::ReadCommittedReadOnly) {
            return "SET TRANSACTION READ ONLY".to_string();
        }
        parts.push(format!("ISOLATION LEVEL {}", self.isolation.as_sql()));
        if matches!(self.access_mode, AccessMode::ReadOnly) {
            parts.push("READ ONLY".to_string());
        } else {
            parts.push("READ WRITE".to_string());
        }
        format!("SET TRANSACTION {}", parts.join(" "))
    }

    /// 生成 COMMIT 语句
    #[must_use]
    pub fn commit_sql(&self) -> String {
        let mut sql = String::from("COMMIT");
        if let Some(comment) = self.commit_comment() {
            sql.push_str(&format!(" COMMENT '{comment}'"));
        }
        sql.push(';');
        sql
    }

    /// 生成 ROLLBACK 语句
    #[must_use]
    pub fn rollback_sql(&self) -> String {
        "ROLLBACK;".to_string()
    }

    /// 生成 ROLLBACK TO SAVEPOINT 语句
    #[must_use]
    pub fn rollback_to_savepoint_sql(&self, name: &str) -> Option<String> {
        if self.savepoints.iter().any(|s| s == name) {
            Some(format!("ROLLBACK TO SAVEPOINT {name};"))
        } else {
            None
        }
    }

    /// 生成 SAVEPOINT 语句
    #[must_use]
    pub fn savepoint_sql(&self, name: &str) -> String {
        format!("SAVEPOINT {name};")
    }

    /// 生成 RELEASE SAVEPOINT 语句
    #[must_use]
    pub fn release_savepoint_sql(&self, name: &str) -> String {
        format!("RELEASE SAVEPOINT {name};")
    }

    /// 提交注释（用于事务标识）
    fn commit_comment(&self) -> Option<String> {
        if self.savepoints.is_empty() {
            None
        } else {
            Some(format!("tx with {} savepoints", self.savepoints.len()))
        }
    }

    /// 验证配置有效性
    ///
    /// # Errors
    ///
    /// 若配置无效返回 `Err(String)`。
    pub fn validate(&self) -> Result<(), String> {
        if matches!(self.access_mode, AccessMode::ReadOnly)
            && !matches!(
                self.isolation,
                TransactionIsolation::ReadCommittedReadOnly | TransactionIsolation::ReadCommitted
            )
        {
            return Err(
                "read-only access mode requires read committed or read-only isolation".to_string(),
            );
        }
        if let Some(t) = self.timeout {
            if t.as_secs() == 0 {
                return Err("timeout must be greater than 0".to_string());
            }
        }
        Ok(())
    }

    /// 保存点数量
    #[must_use]
    pub fn savepoint_count(&self) -> usize {
        self.savepoints.len()
    }
}

impl fmt::Display for TransactionConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "TransactionConfig(isolation={}, auto_commit={}, savepoints={})",
            self.isolation,
            self.auto_commit,
            self.savepoints.len()
        )
    }
}

/// 批量错误处理动作
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchErrorAction {
    /// 回滚整个事务
    Rollback,
    /// 回滚到上一个保存点
    RollbackToSavepoint,
    /// 提交已成功的行
    CommitPartial,
    /// 忽略错误继续
    Continue,
}

impl BatchErrorAction {
    /// 返回描述
    #[must_use]
    pub fn description(&self) -> &'static str {
        match self {
            BatchErrorAction::Rollback => "rollback entire transaction",
            BatchErrorAction::RollbackToSavepoint => "rollback to last savepoint",
            BatchErrorAction::CommitPartial => "commit successful rows",
            BatchErrorAction::Continue => "ignore errors and continue",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_isolation_as_sql() {
        assert_eq!(
            TransactionIsolation::ReadCommitted.as_sql(),
            "READ COMMITTED"
        );
        assert_eq!(TransactionIsolation::Serializable.as_sql(), "SERIALIZABLE");
        assert_eq!(
            TransactionIsolation::ReadCommittedReadOnly.as_sql(),
            "READ ONLY"
        );
    }

    #[test]
    fn test_isolation_allows_writes() {
        assert!(TransactionIsolation::ReadCommitted.allows_writes());
        assert!(TransactionIsolation::Serializable.allows_writes());
        assert!(!TransactionIsolation::ReadCommittedReadOnly.allows_writes());
    }

    #[test]
    fn test_isolation_is_strongest() {
        assert!(TransactionIsolation::Serializable.is_strongest());
        assert!(!TransactionIsolation::ReadCommitted.is_strongest());
    }

    #[test]
    fn test_isolation_set_transaction_sql() {
        let sql = TransactionIsolation::Serializable.set_transaction_sql();
        assert_eq!(sql, "SET TRANSACTION ISOLATION LEVEL SERIALIZABLE");
    }

    #[test]
    fn test_isolation_set_transaction_read_only_sql() {
        let sql = TransactionIsolation::ReadCommittedReadOnly.set_transaction_sql();
        assert_eq!(sql, "SET TRANSACTION READ ONLY");
    }

    #[test]
    fn test_isolation_alter_session_sql() {
        let sql = TransactionIsolation::Serializable.alter_session_sql();
        assert!(sql.contains("ALTER SESSION"));
        assert!(sql.contains("SERIALIZABLE"));
    }

    #[test]
    fn test_isolation_display() {
        let s = format!("{}", TransactionIsolation::ReadCommitted);
        assert_eq!(s, "READ COMMITTED");
    }

    #[test]
    fn test_isolation_default() {
        let iso = TransactionIsolation::default();
        assert_eq!(iso, TransactionIsolation::ReadCommitted);
    }

    #[test]
    fn test_access_mode_as_sql() {
        assert_eq!(AccessMode::ReadWrite.as_sql(), "READ WRITE");
        assert_eq!(AccessMode::ReadOnly.as_sql(), "READ ONLY");
    }

    #[test]
    fn test_transaction_config_default() {
        let cfg = TransactionConfig::default();
        assert_eq!(cfg.isolation, TransactionIsolation::ReadCommitted);
        assert!(!cfg.auto_commit);
    }

    #[test]
    fn test_transaction_config_builder() {
        let cfg = TransactionConfig::new()
            .with_isolation(TransactionIsolation::Serializable)
            .with_auto_commit()
            .with_timeout(Duration::from_secs(30))
            .add_savepoint("sp1");
        assert_eq!(cfg.isolation, TransactionIsolation::Serializable);
        assert!(cfg.auto_commit);
        assert_eq!(cfg.savepoint_count(), 1);
    }

    #[test]
    fn test_transaction_config_read_only_isolation() {
        let cfg =
            TransactionConfig::new().with_isolation(TransactionIsolation::ReadCommittedReadOnly);
        assert_eq!(cfg.access_mode, AccessMode::ReadOnly);
    }

    #[test]
    fn test_transaction_config_begin_sql() {
        let cfg = TransactionConfig::new().with_isolation(TransactionIsolation::Serializable);
        let sql = cfg.begin_sql();
        assert!(sql.contains("ISOLATION LEVEL SERIALIZABLE"));
        assert!(sql.contains("READ WRITE"));
    }

    #[test]
    fn test_transaction_config_begin_sql_read_only() {
        let cfg =
            TransactionConfig::new().with_isolation(TransactionIsolation::ReadCommittedReadOnly);
        let sql = cfg.begin_sql();
        assert_eq!(sql, "SET TRANSACTION READ ONLY");
    }

    #[test]
    fn test_transaction_config_commit_sql() {
        let cfg = TransactionConfig::new();
        let sql = cfg.commit_sql();
        assert_eq!(sql, "COMMIT;");
    }

    #[test]
    fn test_transaction_config_rollback_sql() {
        let cfg = TransactionConfig::new();
        assert_eq!(cfg.rollback_sql(), "ROLLBACK;");
    }

    #[test]
    fn test_transaction_config_savepoint_sql() {
        let cfg = TransactionConfig::new();
        assert_eq!(cfg.savepoint_sql("sp1"), "SAVEPOINT sp1;");
    }

    #[test]
    fn test_transaction_config_release_savepoint_sql() {
        let cfg = TransactionConfig::new();
        assert_eq!(cfg.release_savepoint_sql("sp1"), "RELEASE SAVEPOINT sp1;");
    }

    #[test]
    fn test_transaction_config_rollback_to_savepoint() {
        let cfg = TransactionConfig::new().add_savepoint("sp1");
        let sql = cfg.rollback_to_savepoint_sql("sp1").unwrap();
        assert_eq!(sql, "ROLLBACK TO SAVEPOINT sp1;");
    }

    #[test]
    fn test_transaction_config_rollback_to_nonexistent_savepoint() {
        let cfg = TransactionConfig::new().add_savepoint("sp1");
        assert!(cfg.rollback_to_savepoint_sql("sp2").is_none());
    }

    #[test]
    fn test_transaction_config_validate_ok() {
        let cfg = TransactionConfig::new();
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn test_transaction_config_validate_zero_timeout() {
        let cfg = TransactionConfig::new().with_timeout(Duration::from_secs(0));
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_transaction_config_display() {
        let cfg = TransactionConfig::new()
            .with_isolation(TransactionIsolation::Serializable)
            .add_savepoint("sp1");
        let s = format!("{}", cfg);
        assert!(s.contains("SERIALIZABLE"));
        assert!(s.contains("savepoints=1"));
    }

    #[test]
    fn test_batch_error_action_description() {
        assert_eq!(
            BatchErrorAction::Rollback.description(),
            "rollback entire transaction"
        );
        assert_eq!(
            BatchErrorAction::Continue.description(),
            "ignore errors and continue"
        );
    }

    #[test]
    fn test_transaction_config_with_ddl() {
        let cfg = TransactionConfig::new().with_ddl();
        assert!(cfg.allow_ddl);
    }

    #[test]
    fn test_transaction_config_with_access_mode() {
        let cfg = TransactionConfig::new().with_access_mode(AccessMode::ReadOnly);
        assert_eq!(cfg.access_mode, AccessMode::ReadOnly);
    }

    #[test]
    fn test_transaction_config_with_batch_error_action() {
        let cfg = TransactionConfig::new().with_batch_error_action(BatchErrorAction::Continue);
        assert_eq!(cfg.on_batch_error, BatchErrorAction::Continue);
    }
}
