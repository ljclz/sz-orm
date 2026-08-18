//! SQL Server 事务配置
//!
//! 提供 [`TransactionConfig`] 与 [`TransactionIsolation`] 用于配置 SQL Server
//! 事务的隔离级别、超时、锁超时、死锁优先级等。

use std::fmt;
use std::time::Duration;

/// SQL Server 事务隔离级别
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionIsolation {
    /// 读未提交（NOLOCK）
    ReadUncommitted,
    /// 读已提交（默认）
    ReadCommitted,
    /// 可重复读
    RepeatableRead,
    /// 串行化
    Serializable,
    /// 快照（SNAPSHOT）
    Snapshot,
}

impl TransactionIsolation {
    /// 返回 SQL 关键字
    #[must_use]
    pub fn as_sql(&self) -> &'static str {
        match self {
            TransactionIsolation::ReadUncommitted => "READ UNCOMMITTED",
            TransactionIsolation::ReadCommitted => "READ COMMITTED",
            TransactionIsolation::RepeatableRead => "REPEATABLE READ",
            TransactionIsolation::Serializable => "SERIALIZABLE",
            TransactionIsolation::Snapshot => "SNAPSHOT",
        }
    }

    /// 返回描述
    #[must_use]
    pub fn description(&self) -> &'static str {
        match self {
            TransactionIsolation::ReadUncommitted => "read uncommitted (dirty read)",
            TransactionIsolation::ReadCommitted => "read committed (default)",
            TransactionIsolation::RepeatableRead => "repeatable read",
            TransactionIsolation::Serializable => "serializable",
            TransactionIsolation::Snapshot => "snapshot (row versioning)",
        }
    }

    /// 是否允许脏读
    #[must_use]
    pub fn allows_dirty_read(&self) -> bool {
        matches!(self, TransactionIsolation::ReadUncommitted)
    }

    /// 是否允许不可重复读
    #[must_use]
    pub fn allows_non_repeatable_read(&self) -> bool {
        matches!(
            self,
            TransactionIsolation::ReadUncommitted | TransactionIsolation::ReadCommitted
        )
    }

    /// 是否允许幻读
    #[must_use]
    pub fn allows_phantom(&self) -> bool {
        matches!(
            self,
            TransactionIsolation::ReadUncommitted
                | TransactionIsolation::ReadCommitted
                | TransactionIsolation::RepeatableRead
        )
    }

    /// 生成 SET TRANSACTION ISOLATION LEVEL 语句
    #[must_use]
    pub fn set_isolation_sql(&self) -> String {
        format!("SET TRANSACTION ISOLATION LEVEL {}", self.as_sql())
    }

    /// 生成锁提示（表提示）
    #[must_use]
    pub fn lock_hint(&self) -> &'static str {
        match self {
            TransactionIsolation::ReadUncommitted => "WITH (NOLOCK)",
            TransactionIsolation::RepeatableRead => "WITH (REPEATABLEREAD)",
            TransactionIsolation::Serializable => "WITH (SERIALIZABLE)",
            _ => "",
        }
    }
}

impl Default for TransactionIsolation {
    fn default() -> Self {
        TransactionIsolation::ReadCommitted
    }
}

impl fmt::Display for TransactionIsolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_sql())
    }
}

/// 死锁优先级
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeadlockPriority {
    /// 低优先级（被选为牺牲者）
    Low,
    /// 正常优先级
    Normal,
    /// 高优先级
    High,
    /// 自定义数值（-10 到 10）
    Custom(i8),
}

impl DeadlockPriority {
    /// 返回 SET DEADLOCK_PRIORITY 语句
    #[must_use]
    pub fn to_sql(&self) -> String {
        match self {
            DeadlockPriority::Low => "SET DEADLOCK_PRIORITY LOW".to_string(),
            DeadlockPriority::Normal => "SET DEADLOCK_PRIORITY NORMAL".to_string(),
            DeadlockPriority::High => "SET DEADLOCK_PRIORITY HIGH".to_string(),
            DeadlockPriority::Custom(n) => {
                format!("SET DEADLOCK_PRIORITY {n}")
            }
        }
    }
}

impl Default for DeadlockPriority {
    fn default() -> Self {
        DeadlockPriority::Normal
    }
}

/// 事务配置
#[derive(Debug, Clone)]
pub struct TransactionConfig {
    /// 隔离级别
    pub isolation: TransactionIsolation,
    /// 事务超时
    pub timeout: Option<Duration>,
    /// 锁超时（毫秒，-1 表示无限等待）
    pub lock_timeout_ms: Option<i32>,
    /// 死锁优先级
    pub deadlock_priority: DeadlockPriority,
    /// 是否自动提交
    pub auto_commit: bool,
    /// 是否启用 XACT_ABORT（错误时自动回滚）
    pub xact_abort: bool,
    /// 是否启用隐式事务
    pub implicit_transactions: bool,
    /// 事务名称（用于 BEGIN TRAN name）
    pub name: Option<String>,
    /// 事务标记（用于 BEGIN TRAN name WITH MARK）
    pub mark: Option<String>,
}

impl Default for TransactionConfig {
    fn default() -> Self {
        Self {
            isolation: TransactionIsolation::ReadCommitted,
            timeout: None,
            lock_timeout_ms: None,
            deadlock_priority: DeadlockPriority::Normal,
            auto_commit: true,
            xact_abort: false,
            implicit_transactions: false,
            name: None,
            mark: None,
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
        self
    }

    /// 设置超时
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// 设置锁超时
    #[must_use]
    pub fn with_lock_timeout(mut self, timeout_ms: i32) -> Self {
        self.lock_timeout_ms = Some(timeout_ms);
        self
    }

    /// 设置死锁优先级
    #[must_use]
    pub fn with_deadlock_priority(mut self, priority: DeadlockPriority) -> Self {
        self.deadlock_priority = priority;
        self
    }

    /// 禁用自动提交
    #[must_use]
    pub fn without_auto_commit(mut self) -> Self {
        self.auto_commit = false;
        self
    }

    /// 启用 XACT_ABORT
    #[must_use]
    pub fn with_xact_abort(mut self) -> Self {
        self.xact_abort = true;
        self
    }

    /// 启用隐式事务
    #[must_use]
    pub fn with_implicit_transactions(mut self) -> Self {
        self.implicit_transactions = true;
        self
    }

    /// 设置事务名称
    #[must_use]
    pub fn with_name(mut self, name: &str) -> Self {
        self.name = Some(name.to_string());
        self
    }

    /// 设置事务标记
    #[must_use]
    pub fn with_mark(mut self, mark: &str) -> Self {
        self.mark = Some(mark.to_string());
        self
    }

    /// 生成 BEGIN TRANSACTION 语句
    #[must_use]
    pub fn begin_sql(&self) -> String {
        let mut sql = String::from("BEGIN TRANSACTION");
        if let Some(ref name) = self.name {
            sql.push(' ');
            sql.push_str(name);
            if let Some(ref mark) = self.mark {
                sql.push_str(&format!(" WITH MARK '{mark}'"));
            }
        }
        sql.push(';');
        sql
    }

    /// 生成 COMMIT 语句
    #[must_use]
    pub fn commit_sql(&self) -> String {
        let mut sql = String::from("COMMIT TRANSACTION");
        if let Some(ref name) = self.name {
            sql.push(' ');
            sql.push_str(name);
        }
        sql.push(';');
        sql
    }

    /// 生成 ROLLBACK 语句
    #[must_use]
    pub fn rollback_sql(&self) -> String {
        let mut sql = String::from("ROLLBACK TRANSACTION");
        if let Some(ref name) = self.name {
            sql.push(' ');
            sql.push_str(name);
        }
        sql.push(';');
        sql
    }

    /// 生成 SAVE TRANSACTION 语句
    #[must_use]
    pub fn save_sql(&self, savepoint: &str) -> String {
        format!("SAVE TRANSACTION {savepoint};")
    }

    /// 生成所有 SET 语句
    #[must_use]
    pub fn set_statements(&self) -> Vec<String> {
        let mut stmts = Vec::new();
        stmts.push(self.isolation.set_isolation_sql() + ";");
        if let Some(ms) = self.lock_timeout_ms {
            stmts.push(format!("SET LOCK_TIMEOUT {ms};"));
        }
        stmts.push(self.deadlock_priority.to_sql() + ";");
        if self.xact_abort {
            stmts.push("SET XACT_ABORT ON;".to_string());
        }
        if self.implicit_transactions {
            stmts.push("SET IMPLICIT_TRANSACTIONS ON;".to_string());
        }
        stmts
    }

    /// 生成完整事务初始化脚本
    #[must_use]
    pub fn init_script(&self) -> String {
        let mut parts = self.set_statements();
        if !self.auto_commit {
            parts.push(self.begin_sql());
        }
        parts.join("\n")
    }

    /// 验证配置
    ///
    /// # Errors
    ///
    /// 若配置无效返回 `Err`。
    pub fn validate(&self) -> Result<(), String> {
        if let Some(t) = self.timeout {
            if t.as_secs() == 0 {
                return Err("timeout must be greater than 0".to_string());
            }
        }
        if let Some(ref mark) = self.mark {
            if mark.is_empty() {
                return Err("mark cannot be empty".to_string());
            }
        }
        if self.mark.is_some() && self.name.is_none() {
            return Err("mark requires a transaction name".to_string());
        }
        Ok(())
    }
}

impl fmt::Display for TransactionConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "TransactionConfig(isolation={}, auto_commit={})",
            self.isolation, self.auto_commit
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_isolation_as_sql() {
        assert_eq!(
            TransactionIsolation::ReadUncommitted.as_sql(),
            "READ UNCOMMITTED"
        );
        assert_eq!(TransactionIsolation::Snapshot.as_sql(), "SNAPSHOT");
    }

    #[test]
    fn test_isolation_allows_dirty_read() {
        assert!(TransactionIsolation::ReadUncommitted.allows_dirty_read());
        assert!(!TransactionIsolation::ReadCommitted.allows_dirty_read());
    }

    #[test]
    fn test_isolation_allows_non_repeatable_read() {
        assert!(TransactionIsolation::ReadCommitted.allows_non_repeatable_read());
        assert!(!TransactionIsolation::RepeatableRead.allows_non_repeatable_read());
    }

    #[test]
    fn test_isolation_allows_phantom() {
        assert!(TransactionIsolation::RepeatableRead.allows_phantom());
        assert!(!TransactionIsolation::Serializable.allows_phantom());
    }

    #[test]
    fn test_isolation_set_sql() {
        let sql = TransactionIsolation::Serializable.set_isolation_sql();
        assert_eq!(sql, "SET TRANSACTION ISOLATION LEVEL SERIALIZABLE");
    }

    #[test]
    fn test_isolation_lock_hint() {
        assert_eq!(
            TransactionIsolation::ReadUncommitted.lock_hint(),
            "WITH (NOLOCK)"
        );
        assert_eq!(TransactionIsolation::ReadCommitted.lock_hint(), "");
    }

    #[test]
    fn test_isolation_default() {
        assert_eq!(
            TransactionIsolation::default(),
            TransactionIsolation::ReadCommitted
        );
    }

    #[test]
    fn test_deadlock_priority_to_sql() {
        assert_eq!(DeadlockPriority::Low.to_sql(), "SET DEADLOCK_PRIORITY LOW");
        assert_eq!(
            DeadlockPriority::High.to_sql(),
            "SET DEADLOCK_PRIORITY HIGH"
        );
        assert_eq!(
            DeadlockPriority::Custom(5).to_sql(),
            "SET DEADLOCK_PRIORITY 5"
        );
    }

    #[test]
    fn test_transaction_config_default() {
        let cfg = TransactionConfig::default();
        assert_eq!(cfg.isolation, TransactionIsolation::ReadCommitted);
        assert!(cfg.auto_commit);
    }

    #[test]
    fn test_transaction_config_builder() {
        let cfg = TransactionConfig::new()
            .with_isolation(TransactionIsolation::Serializable)
            .with_timeout(Duration::from_secs(30))
            .with_lock_timeout(5000)
            .with_deadlock_priority(DeadlockPriority::Low)
            .without_auto_commit()
            .with_xact_abort()
            .with_name("my_tx");
        assert_eq!(cfg.isolation, TransactionIsolation::Serializable);
        assert!(!cfg.auto_commit);
        assert!(cfg.xact_abort);
        assert_eq!(cfg.name.as_deref(), Some("my_tx"));
    }

    #[test]
    fn test_transaction_config_begin_sql() {
        let cfg = TransactionConfig::new().with_name("tx1");
        assert_eq!(cfg.begin_sql(), "BEGIN TRANSACTION tx1;");
    }

    #[test]
    fn test_transaction_config_begin_sql_with_mark() {
        let cfg = TransactionConfig::new()
            .with_name("tx1")
            .with_mark("checkpoint");
        let sql = cfg.begin_sql();
        assert!(sql.contains("WITH MARK 'checkpoint'"));
    }

    #[test]
    fn test_transaction_config_commit_sql() {
        let cfg = TransactionConfig::new().with_name("tx1");
        assert_eq!(cfg.commit_sql(), "COMMIT TRANSACTION tx1;");
    }

    #[test]
    fn test_transaction_config_rollback_sql() {
        let cfg = TransactionConfig::new().with_name("tx1");
        assert_eq!(cfg.rollback_sql(), "ROLLBACK TRANSACTION tx1;");
    }

    #[test]
    fn test_transaction_config_save_sql() {
        let cfg = TransactionConfig::new();
        assert_eq!(cfg.save_sql("sp1"), "SAVE TRANSACTION sp1;");
    }

    #[test]
    fn test_transaction_config_set_statements() {
        let cfg = TransactionConfig::new()
            .with_isolation(TransactionIsolation::Serializable)
            .with_lock_timeout(5000)
            .with_xact_abort();
        let stmts = cfg.set_statements();
        assert!(stmts.iter().any(|s| s.contains("SERIALIZABLE")));
        assert!(stmts.iter().any(|s| s.contains("LOCK_TIMEOUT 5000")));
        assert!(stmts.iter().any(|s| s.contains("XACT_ABORT ON")));
    }

    #[test]
    fn test_transaction_config_init_script_no_auto_commit() {
        let cfg = TransactionConfig::new()
            .without_auto_commit()
            .with_name("tx1");
        let script = cfg.init_script();
        assert!(script.contains("BEGIN TRANSACTION"));
    }

    #[test]
    fn test_transaction_config_init_script_auto_commit() {
        let cfg = TransactionConfig::new();
        let script = cfg.init_script();
        assert!(!script.contains("BEGIN TRANSACTION"));
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
    fn test_transaction_config_validate_mark_without_name() {
        let cfg = TransactionConfig::new().with_mark("m1");
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_transaction_config_display() {
        let cfg = TransactionConfig::new().with_isolation(TransactionIsolation::Snapshot);
        let s = format!("{}", cfg);
        assert!(s.contains("SNAPSHOT"));
    }
}
