//! Transaction support
//!
//! Provides ACID transaction management

use crate::error::TxError;
use crate::pool::Connection;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

#[derive(Debug, Clone, PartialEq, Default)]
pub enum IsolationLevel {
    ReadUncommitted,
    ReadCommitted,
    #[default]
    RepeatableRead,
    Serializable,
    Snapshot,
}

impl std::fmt::Display for IsolationLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IsolationLevel::ReadUncommitted => write!(f, "READ UNCOMMITTED"),
            IsolationLevel::ReadCommitted => write!(f, "READ COMMITTED"),
            IsolationLevel::RepeatableRead => write!(f, "REPEATABLE READ"),
            IsolationLevel::Serializable => write!(f, "SERIALIZABLE"),
            IsolationLevel::Snapshot => write!(f, "SNAPSHOT"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum TransactionState {
    #[default]
    Active,
    Committed,
    RolledBack,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum AutoCommit {
    #[default]
    On,
    Off,
}

#[derive(Default)]
pub struct TransactOptions {
    pub isolation_level: Option<IsolationLevel>,
    pub read_only: bool,
    pub timeout: Option<Duration>,
}

impl TransactOptions {
    pub fn with_isolation(mut self, level: IsolationLevel) -> Self {
        self.isolation_level = Some(level);
        self
    }

    pub fn read_only(mut self) -> Self {
        self.read_only = true;
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }
}

/// 事务对象，封装一个数据库事务
pub struct Transaction {
    conn: Arc<Mutex<Box<dyn Connection>>>,
    state: TransactionState,
    options: TransactOptions,
    savepoint_counter: u32,
}

impl Transaction {
    /// 创建新事务（调用方应先通过 connection.begin_transaction() 启动事务）
    pub fn new(conn: Box<dyn Connection>, options: TransactOptions) -> Self {
        Self {
            conn: Arc::new(Mutex::new(conn)),
            state: TransactionState::Active,
            options,
            savepoint_counter: 0,
        }
    }

    /// 获取当前事务状态
    pub fn state(&self) -> TransactionState {
        self.state
    }

    /// 检查事务是否仍然活跃
    pub fn is_active(&self) -> bool {
        self.state == TransactionState::Active
    }

    /// 提交事务
    pub async fn commit(&mut self) -> Result<(), TxError> {
        if self.state != TransactionState::Active {
            return Err(TxError::CommitFailed(format!(
                "Transaction already finished: {:?}",
                self.state
            )));
        }
        let mut conn = self.conn.lock().await;
        conn.commit()
            .await
            .map_err(|e| TxError::CommitFailed(e.to_string()))?;
        self.state = TransactionState::Committed;
        Ok(())
    }

    /// 回滚事务
    pub async fn rollback(&mut self) -> Result<(), TxError> {
        if self.state != TransactionState::Active {
            return Err(TxError::RollbackFailed(format!(
                "Transaction already finished: {:?}",
                self.state
            )));
        }
        let mut conn = self.conn.lock().await;
        conn.rollback()
            .await
            .map_err(|e| TxError::RollbackFailed(e.to_string()))?;
        self.state = TransactionState::RolledBack;
        Ok(())
    }

    /// 在事务中执行 SQL（在事务未结束时执行）
    pub async fn execute(&mut self, sql: &str) -> Result<u64, TxError> {
        if self.state != TransactionState::Active {
            return Err(TxError::CommitFailed(format!(
                "Transaction not active: {:?}",
                self.state
            )));
        }
        let mut conn = self.conn.lock().await;
        let result = conn
            .execute(sql)
            .await
            .map_err(|e| TxError::CommitFailed(e.to_string()))?;
        Ok(result)
    }

    /// 在事务中执行查询
    pub async fn query(
        &mut self,
        sql: &str,
    ) -> Result<Vec<std::collections::HashMap<String, crate::value::Value>>, TxError> {
        if self.state != TransactionState::Active {
            return Err(TxError::CommitFailed(format!(
                "Transaction not active: {:?}",
                self.state
            )));
        }
        let mut conn = self.conn.lock().await;
        let result = conn
            .query(sql)
            .await
            .map_err(|e| TxError::CommitFailed(e.to_string()))?;
        Ok(result)
    }

    /// 创建保存点（用于嵌套事务）
    pub async fn savepoint(&mut self) -> Result<String, TxError> {
        if self.state != TransactionState::Active {
            return Err(TxError::SavepointError(format!(
                "Transaction not active: {:?}",
                self.state
            )));
        }
        self.savepoint_counter += 1;
        let name = format!("sp_{}", self.savepoint_counter);
        let sql = format!("SAVEPOINT {}", name);
        let mut conn = self.conn.lock().await;
        conn.execute(&sql)
            .await
            .map_err(|e| TxError::SavepointError(e.to_string()))?;
        Ok(name)
    }

    /// 回滚到保存点
    pub async fn rollback_to_savepoint(&mut self, name: &str) -> Result<(), TxError> {
        if self.state != TransactionState::Active {
            return Err(TxError::SavepointError(format!(
                "Transaction not active: {:?}",
                self.state
            )));
        }
        let sql = format!("ROLLBACK TO SAVEPOINT {}", name);
        let mut conn = self.conn.lock().await;
        conn.execute(&sql)
            .await
            .map_err(|e| TxError::SavepointError(e.to_string()))?;
        Ok(())
    }

    /// 释放保存点
    pub async fn release_savepoint(&mut self, name: &str) -> Result<(), TxError> {
        if self.state != TransactionState::Active {
            return Err(TxError::SavepointError(format!(
                "Transaction not active: {:?}",
                self.state
            )));
        }
        let sql = format!("RELEASE SAVEPOINT {}", name);
        let mut conn = self.conn.lock().await;
        conn.execute(&sql)
            .await
            .map_err(|e| TxError::SavepointError(e.to_string()))?;
        Ok(())
    }

    /// 获取事务选项
    pub fn options(&self) -> &TransactOptions {
        &self.options
    }
}

impl Drop for Transaction {
    fn drop(&mut self) {
        // 如果事务未被显式提交或回滚，在 drop 时尝试回滚
        // 注意：无法在 Drop 中 await，所以这里只能标记状态
        // 真正的回滚应由调用方负责
        if self.state == TransactionState::Active {
            // 标记为已回滚（实际回滚应由数据库自动处理）
            self.state = TransactionState::RolledBack;
        }
    }
}

/// 事务管理器，管理多个事务
pub struct TransactionManager {
    transactions: Arc<Mutex<std::collections::HashMap<String, Transaction>>>,
}

impl TransactionManager {
    pub fn new() -> Self {
        Self {
            transactions: Arc::new(Mutex::new(std::collections::HashMap::new())),
        }
    }

    /// 开始新事务
    pub async fn begin(
        &self,
        id: String,
        conn: Box<dyn Connection>,
        options: TransactOptions,
    ) -> Result<(), TxError> {
        let mut conn = conn;
        conn.begin_transaction()
            .await
            .map_err(|e| TxError::CommitFailed(e.to_string()))?;
        let tx = Transaction::new(conn, options);
        let mut txs = self.transactions.lock().await;
        txs.insert(id, tx);
        Ok(())
    }

    /// 提交事务
    pub async fn commit(&self, id: &str) -> Result<(), TxError> {
        let mut txs = self.transactions.lock().await;
        let tx = txs
            .get_mut(id)
            .ok_or_else(|| TxError::SavepointError(format!("Transaction {} not found", id)))?;
        tx.commit().await
    }

    /// 回滚事务
    pub async fn rollback(&self, id: &str) -> Result<(), TxError> {
        let mut txs = self.transactions.lock().await;
        let tx = txs
            .get_mut(id)
            .ok_or_else(|| TxError::SavepointError(format!("Transaction {} not found", id)))?;
        tx.rollback().await
    }

    /// 获取事务状态
    pub async fn state(&self, id: &str) -> Option<TransactionState> {
        let txs = self.transactions.lock().await;
        txs.get(id).map(|tx| tx.state())
    }

    /// 列出所有事务 ID
    pub async fn list(&self) -> Vec<String> {
        let txs = self.transactions.lock().await;
        txs.keys().cloned().collect()
    }

    /// 移除已完成的事务
    pub async fn remove(&self, id: &str) -> Option<Transaction> {
        let mut txs = self.transactions.lock().await;
        txs.remove(id)
    }
}

impl Default for TransactionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::pin::Pin;

    /// 测试用的模拟连接
    struct MockConnection {
        begin_called: bool,
        commit_called: bool,
        rollback_called: bool,
        executed_sql: Vec<String>,
    }

    impl MockConnection {
        fn new() -> Self {
            Self {
                begin_called: false,
                commit_called: false,
                rollback_called: false,
                executed_sql: Vec::new(),
            }
        }
    }

    impl Connection for MockConnection {
        fn execute<'a>(
            &'a mut self,
            sql: &'a str,
        ) -> Pin<Box<dyn Future<Output = Result<u64, crate::DbError>> + Send + 'a>> {
            Box::pin(async move {
                self.executed_sql.push(sql.to_string());
                Ok(1)
            })
        }

        fn query<'a>(
            &'a mut self,
            _sql: &'a str,
        ) -> Pin<
            Box<
                dyn Future<
                        Output = Result<
                            Vec<std::collections::HashMap<String, crate::value::Value>>,
                            crate::DbError,
                        >,
                    > + Send
                    + 'a,
            >,
        > {
            Box::pin(async move { Ok(vec![]) })
        }

        fn begin_transaction<'a>(
            &'a mut self,
        ) -> Pin<Box<dyn Future<Output = Result<(), crate::DbError>> + Send + 'a>> {
            Box::pin(async move {
                self.begin_called = true;
                Ok(())
            })
        }

        fn commit<'a>(
            &'a mut self,
        ) -> Pin<Box<dyn Future<Output = Result<(), crate::DbError>> + Send + 'a>> {
            Box::pin(async move {
                self.commit_called = true;
                Ok(())
            })
        }

        fn rollback<'a>(
            &'a mut self,
        ) -> Pin<Box<dyn Future<Output = Result<(), crate::DbError>> + Send + 'a>> {
            Box::pin(async move {
                self.rollback_called = true;
                Ok(())
            })
        }

        fn is_connected(&self) -> bool {
            true
        }

        fn ping<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
            Box::pin(async move { true })
        }

        fn close<'a>(
            &'a mut self,
        ) -> Pin<Box<dyn Future<Output = Result<(), crate::DbError>> + Send + 'a>> {
            Box::pin(async move { Ok(()) })
        }
    }

    #[test]
    fn test_isolation_level_display() {
        assert_eq!(IsolationLevel::ReadCommitted.to_string(), "READ COMMITTED");
        assert_eq!(IsolationLevel::Serializable.to_string(), "SERIALIZABLE");
    }

    #[test]
    fn test_transaction_state_default() {
        let opts = TransactOptions::default();
        assert!(opts.isolation_level.is_none());
        assert!(!opts.read_only);
    }

    #[test]
    fn test_transact_options_builder() {
        let opts = TransactOptions {
            isolation_level: Some(IsolationLevel::Serializable),
            read_only: true,
            timeout: Some(Duration::from_secs(30)),
        };

        assert_eq!(opts.isolation_level, Some(IsolationLevel::Serializable));
        assert!(opts.read_only);
        assert_eq!(opts.timeout, Some(Duration::from_secs(30)));
    }

    #[test]
    fn test_auto_commit_default() {
        assert_eq!(AutoCommit::default(), AutoCommit::On);
    }

    #[test]
    fn test_transaction_state() {
        assert_eq!(TransactionState::Active, TransactionState::Active);
        assert_ne!(TransactionState::Active, TransactionState::Committed);
    }

    #[test]
    fn test_transact_options_chaining() {
        let opts = TransactOptions::default()
            .with_isolation(IsolationLevel::Serializable)
            .read_only()
            .with_timeout(Duration::from_secs(60));
        assert_eq!(opts.isolation_level, Some(IsolationLevel::Serializable));
        assert!(opts.read_only);
        assert_eq!(opts.timeout, Some(Duration::from_secs(60)));
    }

    #[tokio::test]
    async fn test_transaction_commit() {
        let conn = Box::new(MockConnection::new());
        let mut tx = Transaction::new(conn, TransactOptions::default());
        assert!(tx.is_active());

        let result = tx.execute("INSERT INTO users VALUES (1)").await;
        assert!(result.is_ok());

        tx.commit().await.unwrap();
        assert_eq!(tx.state(), TransactionState::Committed);

        // 再次 commit 应该失败
        let result = tx.commit().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_transaction_rollback() {
        let conn = Box::new(MockConnection::new());
        let mut tx = Transaction::new(conn, TransactOptions::default());

        tx.rollback().await.unwrap();
        assert_eq!(tx.state(), TransactionState::RolledBack);

        // 再次 rollback 应该失败
        let result = tx.rollback().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_transaction_execute_after_commit() {
        let conn = Box::new(MockConnection::new());
        let mut tx = Transaction::new(conn, TransactOptions::default());
        tx.commit().await.unwrap();

        let result = tx.execute("SELECT 1").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_transaction_savepoint() {
        let conn = Box::new(MockConnection::new());
        let mut tx = Transaction::new(conn, TransactOptions::default());

        let sp1 = tx.savepoint().await.unwrap();
        assert_eq!(sp1, "sp_1");

        let sp2 = tx.savepoint().await.unwrap();
        assert_eq!(sp2, "sp_2");

        tx.rollback_to_savepoint(&sp1).await.unwrap();
        tx.release_savepoint(&sp2).await.unwrap();
    }

    #[tokio::test]
    async fn test_transaction_manager() {
        let mgr = TransactionManager::new();
        let conn = Box::new(MockConnection::new());

        mgr.begin("tx1".to_string(), conn, TransactOptions::default())
            .await
            .unwrap();

        let state = mgr.state("tx1").await;
        assert_eq!(state, Some(TransactionState::Active));

        mgr.commit("tx1").await.unwrap();
        let state = mgr.state("tx1").await;
        assert_eq!(state, Some(TransactionState::Committed));

        let list = mgr.list().await;
        assert!(list.contains(&"tx1".to_string()));
    }

    #[tokio::test]
    async fn test_transaction_manager_rollback() {
        let mgr = TransactionManager::new();
        let conn = Box::new(MockConnection::new());

        mgr.begin("tx2".to_string(), conn, TransactOptions::default())
            .await
            .unwrap();

        mgr.rollback("tx2").await.unwrap();
        let state = mgr.state("tx2").await;
        assert_eq!(state, Some(TransactionState::RolledBack));
    }

    #[tokio::test]
    async fn test_transaction_manager_not_found() {
        let mgr = TransactionManager::new();
        let result = mgr.commit("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_transaction_manager_remove() {
        let mgr = TransactionManager::new();
        let conn = Box::new(MockConnection::new());

        mgr.begin("tx3".to_string(), conn, TransactOptions::default())
            .await
            .unwrap();

        let removed = mgr.remove("tx3").await;
        assert!(removed.is_some());

        let state = mgr.state("tx3").await;
        assert_eq!(state, None);
    }
}
