//! Transaction support
//!
//! Provides ACID transaction management

use crate::error::{TransactionState, TxError};
use crate::pool::Connection;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

// TransactionState 定义在 `error` 模块以避免 `transaction` ↔ `error` 循环依赖；
// 通过 `pub use error::*;` 在 crate 根重导出，外部访问路径仍为 `sz_orm_core::TransactionState`。

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AutoCommit {
    #[default]
    On,
    Off,
}

pub struct TransactOptions {
    pub isolation_level: Option<IsolationLevel>,
    pub read_only: bool,
    pub timeout: Option<Duration>,
    /// H-8 修复：嵌套事务（保存点）最大深度限制
    ///
    /// 默认 `DEFAULT_MAX_NESTING_DEPTH`（8），防止递归事务导致数据库连接耗尽或
    /// 保存点栈溢出。设为 0 表示禁用嵌套事务（首次 `savepoint()` 即报错）。
    pub max_nesting_depth: u32,
}

/// H-8 默认最大嵌套深度
pub const DEFAULT_MAX_NESTING_DEPTH: u32 = 8;

impl Default for TransactOptions {
    fn default() -> Self {
        Self {
            isolation_level: None,
            read_only: false,
            timeout: None,
            max_nesting_depth: DEFAULT_MAX_NESTING_DEPTH,
        }
    }
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

    /// H-8 修复：设置最大嵌套深度
    pub fn with_max_nesting_depth(mut self, max_depth: u32) -> Self {
        self.max_nesting_depth = max_depth;
        self
    }
}

/// 校验保存点名称（防止 SQL 注入）
///
/// 保存点名称规则：
/// - 非空
/// - 只能包含 ASCII 字母、数字、下划线
/// - 不能以数字开头
fn validate_savepoint_name(name: &str) -> Result<(), TxError> {
    if name.is_empty() {
        return Err(TxError::InvalidSavepointName(name.to_string()));
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(TxError::InvalidSavepointName(name.to_string()));
    }
    if name.starts_with(|c: char| c.is_ascii_digit()) {
        return Err(TxError::InvalidSavepointName(name.to_string()));
    }
    Ok(())
}

/// 事务对象，封装一个数据库事务
///
/// 内部连接以 `Option<Box<dyn Connection>>` 形式持有：
/// - 事务执行期间，连接存在
/// - 调用 `take_connection()` 可在 commit/rollback 后取回连接归还到连接池
/// - Drop 时若事务仍 Active，会尝试 spawn 后台 rollback 任务
pub struct Transaction {
    conn: Arc<Mutex<Option<Box<dyn Connection>>>>,
    state: TransactionState,
    options: TransactOptions,
    savepoint_counter: u32,
}

impl Transaction {
    /// 创建新事务（调用方应先通过 connection.begin_transaction() 启动事务）
    ///
    /// L-5 修复：补充示例文档
    ///
    /// 通常通过 `Connection::begin_transaction()` 创建，而不是直接调用此方法。
    ///
    /// # 示例
    ///
    /// ```ignore
    /// use sz_orm_core::transaction::{Transaction, TransactOptions};
    ///
    /// # async fn example(conn: Box<dyn sz_orm_core::pool::Connection>) {
    /// // 通常通过 Connection::begin_transaction 创建
    /// let tx = Transaction::new(conn, TransactOptions::default());
    /// assert!(tx.is_active());
    /// # }
    /// ```
    pub fn new(conn: Box<dyn Connection>, options: TransactOptions) -> Self {
        Self {
            conn: Arc::new(Mutex::new(Some(conn))),
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
    ///
    /// L-5 修复：补充示例文档
    ///
    /// 提交后事务状态变为 `Committed`，不可再次 commit/rollback。
    /// 若未提交就 drop，会自动 rollback。
    ///
    /// # 示例
    ///
    /// ```ignore
    /// # use sz_orm_core::transaction::TransactOptions;
    /// # async fn example(mut tx: sz_orm_core::transaction::Transaction) -> Result<(), Box<dyn std::error::Error>> {
    /// tx.execute("INSERT INTO users (name) VALUES ('Alice')").await?;
    /// tx.execute("INSERT INTO users (name) VALUES ('Bob')").await?;
    /// tx.commit().await?; // 提交两条 INSERT
    /// # Ok(())
    /// # }
    /// ```
    pub async fn commit(&mut self) -> Result<(), TxError> {
        if self.state != TransactionState::Active {
            return Err(TxError::NotActive(self.state));
        }
        let mut conn_guard = self.conn.lock().await;
        let conn = conn_guard.as_mut().ok_or(TxError::ConnectionTaken)?;
        conn.commit()
            .await
            .map_err(|e| TxError::CommitFailed(e.to_string()))?;
        self.state = TransactionState::Committed;
        Ok(())
    }

    /// 回滚事务
    pub async fn rollback(&mut self) -> Result<(), TxError> {
        if self.state != TransactionState::Active {
            return Err(TxError::NotActive(self.state));
        }
        let mut conn_guard = self.conn.lock().await;
        let conn = conn_guard.as_mut().ok_or(TxError::ConnectionTaken)?;
        conn.rollback()
            .await
            .map_err(|e| TxError::RollbackFailed(e.to_string()))?;
        self.state = TransactionState::RolledBack;
        Ok(())
    }

    /// 在事务中执行 SQL（在事务未结束时执行）
    pub async fn execute(&mut self, sql: &str) -> Result<u64, TxError> {
        if self.state != TransactionState::Active {
            return Err(TxError::NotActive(self.state));
        }
        let mut conn_guard = self.conn.lock().await;
        let conn = conn_guard.as_mut().ok_or(TxError::ConnectionTaken)?;
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
            return Err(TxError::NotActive(self.state));
        }
        let mut conn_guard = self.conn.lock().await;
        let conn = conn_guard.as_mut().ok_or(TxError::ConnectionTaken)?;
        let result = conn
            .query(sql)
            .await
            .map_err(|e| TxError::CommitFailed(e.to_string()))?;
        Ok(result)
    }

    /// 创建保存点（用于嵌套事务）
    ///
    /// 返回自动生成的保存点名（格式 `sp_<N>`，N 单调递增）。
    ///
    /// H-8 修复：检查嵌套深度，超过 `options.max_nesting_depth` 时返回
    /// `TxError::MaxNestingDepthExceeded`。
    ///
    /// L-5 修复：补充示例文档
    ///
    /// # 示例
    ///
    /// ```ignore
    /// # async fn example(mut tx: sz_orm_core::transaction::Transaction) -> Result<(), Box<dyn std::error::Error>> {
    /// // 在外层事务中创建保存点
    /// let sp = tx.savepoint().await?;
    /// // 执行一些操作
    /// tx.execute("INSERT INTO orders (id) VALUES (1)").await?;
    /// // 出错时回滚到保存点（不影响外层事务的其他操作）
    /// tx.rollback_to_savepoint(&sp).await?;
    /// // 不再需要保存点时释放
    /// tx.release_savepoint(&sp).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn savepoint(&mut self) -> Result<String, TxError> {
        if self.state != TransactionState::Active {
            return Err(TxError::NotActive(self.state));
        }
        // H-8 修复：嵌套深度检查
        // savepoint_counter 表示已创建的保存点数；新保存点的深度为 counter + 1
        let next_depth = self.savepoint_counter + 1;
        if next_depth > self.options.max_nesting_depth {
            return Err(TxError::MaxNestingDepthExceeded {
                current_depth: next_depth,
                max_depth: self.options.max_nesting_depth,
            });
        }
        self.savepoint_counter += 1;
        let name = format!("sp_{}", self.savepoint_counter);
        // 内部生成的名称已通过命名规则（sp_ + 数字），但为防御性编程仍校验
        validate_savepoint_name(&name)?;
        let sql = format!("SAVEPOINT {}", name);
        let mut conn_guard = self.conn.lock().await;
        let conn = conn_guard.as_mut().ok_or(TxError::ConnectionTaken)?;
        conn.execute(&sql)
            .await
            .map_err(|e| TxError::SavepointError(e.to_string()))?;
        Ok(name)
    }

    /// 回滚到保存点
    ///
    /// `name` 必须是合法的保存点名称（仅 ASCII 字母/数字/下划线，且不以数字开头）。
    /// 通常使用 `savepoint()` 返回的名称。
    pub async fn rollback_to_savepoint(&mut self, name: &str) -> Result<(), TxError> {
        if self.state != TransactionState::Active {
            return Err(TxError::NotActive(self.state));
        }
        validate_savepoint_name(name)?;
        let sql = format!("ROLLBACK TO SAVEPOINT {}", name);
        let mut conn_guard = self.conn.lock().await;
        let conn = conn_guard.as_mut().ok_or(TxError::ConnectionTaken)?;
        conn.execute(&sql)
            .await
            .map_err(|e| TxError::SavepointError(e.to_string()))?;
        Ok(())
    }

    /// 释放保存点
    ///
    /// `name` 必须是合法的保存点名称（仅 ASCII 字母/数字/下划线，且不以数字开头）。
    /// 通常使用 `savepoint()` 返回的名称。
    pub async fn release_savepoint(&mut self, name: &str) -> Result<(), TxError> {
        if self.state != TransactionState::Active {
            return Err(TxError::NotActive(self.state));
        }
        validate_savepoint_name(name)?;
        let sql = format!("RELEASE SAVEPOINT {}", name);
        let mut conn_guard = self.conn.lock().await;
        let conn = conn_guard.as_mut().ok_or(TxError::ConnectionTaken)?;
        conn.execute(&sql)
            .await
            .map_err(|e| TxError::SavepointError(e.to_string()))?;
        Ok(())
    }

    /// 取出底层连接（用于归还到连接池）
    ///
    /// 仅在事务已 commit/rollback 后才能调用，否则返回 `NotActive` 错误。
    /// 重复调用返回 `ConnectionTaken` 错误。
    ///
    /// 典型用法：
    /// ```ignore
    /// tx.commit().await?;
    /// let conn = tx.take_connection().await?;
    /// pool.release(conn).await;
    /// ```
    pub async fn take_connection(&mut self) -> Result<Box<dyn Connection>, TxError> {
        if self.state == TransactionState::Active {
            return Err(TxError::NotActive(self.state));
        }
        let mut conn_guard = self.conn.lock().await;
        conn_guard.take().ok_or(TxError::ConnectionTaken)
    }

    /// 获取事务选项
    pub fn options(&self) -> &TransactOptions {
        &self.options
    }
}

/// M-8 修复：检测错误字符串是否表示死锁
///
/// 各数据库死锁错误码：
/// - MySQL: 1213 (ER_LOCK_DEADLOCK)
/// - PostgreSQL: 40P01 (deadlock_detected)
/// - SQLite: "database is locked" / "database table is locked"
/// - Oracle: ORA-00060
/// - SQL Server: 1205 (Transaction was deadlocked)
pub fn is_deadlock_error(err_msg: &str) -> bool {
    let lower = err_msg.to_lowercase();
    // MySQL: "Deadlock found when trying to get lock" (error 1213)
    if lower.contains("deadlock found when trying to get lock") {
        return true;
    }
    // MySQL error code 1213
    if lower.contains("error 1213") || lower.contains("(1213)") {
        return true;
    }
    // PostgreSQL: "deadlock detected" (SQLSTATE 40P01)
    if lower.contains("deadlock detected") || lower.contains("40p01") {
        return true;
    }
    // SQLite: "database is locked"
    if lower.contains("database is locked") || lower.contains("database table is locked") {
        return true;
    }
    // Oracle: ORA-00060: deadlock detected while waiting for resource
    if lower.contains("ora-00060") {
        return true;
    }
    // SQL Server: 1205 (Transaction was deadlocked on lock resources)
    if lower.contains("transaction (process id") && lower.contains("was deadlocked") {
        return true;
    }
    if lower.contains("error 1205") || lower.contains("(1205)") {
        return true;
    }
    false
}

/// M-8 修复：在死锁时自动重试事务
///
/// `operation` 是一个异步闭包，返回 `Result<T, TxError>`。如果返回的错误包含
/// 死锁信息（通过 `is_deadlock_error` 判断），则等待 `backoff` 后重试。
///
/// # 参数
///
/// - `max_attempts`: 最大重试次数（含首次执行），默认 3
/// - `backoff`: 每次重试前的等待时间，默认 50ms
/// - `operation`: 异步闭包，接受当前尝试次数（从 1 开始），返回 `Result<T, TxError>`
///
/// # 返回值
///
/// - 成功时返回 `Ok(T)`
/// - 所有重试都失败时返回最后一次的 `Err(TxError::DeadlockDetected)`
///
/// # 示例
///
/// ```ignore
/// let result = retry_on_deadlock(3, Duration::from_millis(50), |attempt| async move {
///     // 执行事务操作
///     tx.execute("UPDATE accounts SET balance = balance - 100 WHERE id = 1").await
/// }).await;
/// ```
pub async fn retry_on_deadlock<F, Fut, T>(
    max_attempts: u32,
    backoff: Duration,
    operation: F,
) -> Result<T, TxError>
where
    F: Fn(u32) -> Fut,
    Fut: std::future::Future<Output = Result<T, TxError>>,
{
    let mut last_err: Option<TxError> = None;
    for attempt in 1..=max_attempts {
        match operation(attempt).await {
            Ok(v) => return Ok(v),
            Err(e) => {
                // 检查是否为死锁错误
                let err_msg = format!("{}", e);
                if is_deadlock_error(&err_msg) && attempt < max_attempts {
                    tokio::time::sleep(backoff).await;
                    last_err = Some(TxError::DeadlockDetected {
                        attempt,
                        max_attempts,
                    });
                    continue;
                }
                // 非死锁错误或已达最大重试次数，直接返回
                return Err(e);
            }
        }
    }
    // 理论上不会到达（循环内所有路径都会 return），但为类型安全保留
    Err(last_err.unwrap_or(TxError::DeadlockDetected {
        attempt: max_attempts,
        max_attempts,
    }))
}

impl Drop for Transaction {
    fn drop(&mut self) {
        // 如果事务未被显式提交或回滚，在 drop 时尝试回滚
        // 注意：无法在 Drop 中 await，所以这里 spawn 一个后台任务执行 rollback
        if self.state == TransactionState::Active {
            let conn = self.conn.clone();
            // 尝试获取当前 tokio 运行时句柄；若不存在（如非 async 上下文）则跳过
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                // spawn 后台任务：锁连接 → rollback → 连接随 Arc 释放而 Drop
                // 若任务因 runtime 关闭未执行，连接 Drop 时由驱动/池策略兜底
                handle.spawn(async move {
                    let mut conn_guard = conn.lock().await;
                    if let Some(ref mut conn) = *conn_guard {
                        let _ = conn.rollback().await;
                    }
                });
            }
            // 标记为已回滚（即使后台任务未完成，状态机也需要前进）
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
            max_nesting_depth: DEFAULT_MAX_NESTING_DEPTH,
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

        // 再次 commit 应该失败（NotActive）
        let result = tx.commit().await;
        assert!(result.is_err());
        match result {
            Err(TxError::NotActive(state)) => {
                assert_eq!(state, TransactionState::Committed);
            }
            _ => panic!("Expected NotActive error"),
        }
    }

    #[tokio::test]
    async fn test_transaction_rollback() {
        let conn = Box::new(MockConnection::new());
        let mut tx = Transaction::new(conn, TransactOptions::default());

        tx.rollback().await.unwrap();
        assert_eq!(tx.state(), TransactionState::RolledBack);

        // 再次 rollback 应该失败（NotActive）
        let result = tx.rollback().await;
        assert!(result.is_err());
        match result {
            Err(TxError::NotActive(state)) => {
                assert_eq!(state, TransactionState::RolledBack);
            }
            _ => panic!("Expected NotActive error"),
        }
    }

    #[tokio::test]
    async fn test_transaction_execute_after_commit() {
        let conn = Box::new(MockConnection::new());
        let mut tx = Transaction::new(conn, TransactOptions::default());
        tx.commit().await.unwrap();

        let result = tx.execute("SELECT 1").await;
        assert!(result.is_err());
        match result {
            Err(TxError::NotActive(_)) => {}
            _ => panic!("Expected NotActive error"),
        }
    }

    #[tokio::test]
    async fn test_transaction_query_after_commit_returns_not_active() {
        let conn = Box::new(MockConnection::new());
        let mut tx = Transaction::new(conn, TransactOptions::default());
        tx.commit().await.unwrap();

        let result = tx.query("SELECT 1").await;
        assert!(result.is_err());
        match result {
            Err(TxError::NotActive(_)) => {}
            _ => panic!("Expected NotActive error"),
        }
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
    async fn test_transaction_savepoint_name_validation() {
        let conn = Box::new(MockConnection::new());
        let mut tx = Transaction::new(conn, TransactOptions::default());

        // 非法名称：包含单引号（SQL 注入尝试）
        let result = tx.rollback_to_savepoint("sp'; DROP TABLE--").await;
        assert!(result.is_err());
        match result {
            Err(TxError::InvalidSavepointName(_)) => {}
            _ => panic!("Expected InvalidSavepointName error"),
        }

        // 非法名称：以数字开头
        let result = tx.release_savepoint("1sp").await;
        assert!(result.is_err());
        match result {
            Err(TxError::InvalidSavepointName(_)) => {}
            _ => panic!("Expected InvalidSavepointName error"),
        }

        // 非法名称：空字符串
        let result = tx.rollback_to_savepoint("").await;
        assert!(result.is_err());
        match result {
            Err(TxError::InvalidSavepointName(_)) => {}
            _ => panic!("Expected InvalidSavepointName error"),
        }

        // 合法名称：字母+下划线+数字
        let result = tx.rollback_to_savepoint("sp_test_1").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_transaction_take_connection() {
        let conn = Box::new(MockConnection::new());
        let mut tx = Transaction::new(conn, TransactOptions::default());

        // Active 状态下不能取连接
        let result = tx.take_connection().await;
        assert!(result.is_err());
        match result {
            Err(TxError::NotActive(_)) => {}
            _ => panic!("Expected NotActive error"),
        }

        // commit 后可以取连接
        tx.commit().await.unwrap();
        let conn = tx.take_connection().await;
        assert!(conn.is_ok());

        // 重复取连接应失败
        let result = tx.take_connection().await;
        assert!(result.is_err());
        match result {
            Err(TxError::ConnectionTaken) => {}
            _ => panic!("Expected ConnectionTaken error"),
        }
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

    /// 验证 Drop 时若事务仍 Active，会 spawn 后台 rollback 任务
    #[tokio::test]
    async fn test_transaction_drop_rolls_back_when_active() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc as StdArc;

        struct TrackingConnection {
            rollback_called: StdArc<AtomicBool>,
        }

        impl Connection for TrackingConnection {
            fn execute<'a>(
                &'a mut self,
                _sql: &'a str,
            ) -> Pin<Box<dyn Future<Output = Result<u64, crate::DbError>> + Send + 'a>>
            {
                Box::pin(async { Ok(1) })
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
                Box::pin(async { Ok(vec![]) })
            }
            fn begin_transaction<'a>(
                &'a mut self,
            ) -> Pin<Box<dyn Future<Output = Result<(), crate::DbError>> + Send + 'a>> {
                Box::pin(async { Ok(()) })
            }
            fn commit<'a>(
                &'a mut self,
            ) -> Pin<Box<dyn Future<Output = Result<(), crate::DbError>> + Send + 'a>> {
                Box::pin(async { Ok(()) })
            }
            fn rollback<'a>(
                &'a mut self,
            ) -> Pin<Box<dyn Future<Output = Result<(), crate::DbError>> + Send + 'a>> {
                let flag = self.rollback_called.clone();
                Box::pin(async move {
                    flag.store(true, Ordering::SeqCst);
                    Ok(())
                })
            }
            fn is_connected(&self) -> bool {
                true
            }
            fn ping<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
                Box::pin(async { true })
            }
            fn close<'a>(
                &'a mut self,
            ) -> Pin<Box<dyn Future<Output = Result<(), crate::DbError>> + Send + 'a>> {
                Box::pin(async { Ok(()) })
            }
        }

        let rollback_flag = StdArc::new(AtomicBool::new(false));
        let conn = Box::new(TrackingConnection {
            rollback_called: rollback_flag.clone(),
        });
        {
            let _tx = Transaction::new(conn, TransactOptions::default());
            // _tx 在块结束时 drop，状态为 Active，应触发后台 rollback
        }
        // 给 spawn 的任务一点时间执行
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(
            rollback_flag.load(Ordering::SeqCst),
            "Drop should have triggered rollback"
        );
    }

    // ==================== H-8 事务嵌套深度限制测试 ====================

    #[test]
    fn test_h8_default_max_nesting_depth_is_8() {
        let opts = TransactOptions::default();
        assert_eq!(opts.max_nesting_depth, DEFAULT_MAX_NESTING_DEPTH);
        assert_eq!(opts.max_nesting_depth, 8);
    }

    #[test]
    fn test_h8_with_max_nesting_depth_builder() {
        let opts = TransactOptions::default().with_max_nesting_depth(3);
        assert_eq!(opts.max_nesting_depth, 3);
    }

    #[tokio::test]
    async fn test_h8_savepoint_within_default_depth_succeeds() {
        let conn = Box::new(MockConnection::new());
        let mut tx = Transaction::new(conn, TransactOptions::default());

        // 默认深度 8，创建 8 个保存点应全部成功
        for i in 1..=8 {
            let sp = tx.savepoint().await.unwrap();
            assert_eq!(sp, format!("sp_{}", i));
        }
    }

    #[tokio::test]
    async fn test_h8_savepoint_exceeding_default_depth_fails() {
        let conn = Box::new(MockConnection::new());
        let mut tx = Transaction::new(conn, TransactOptions::default());

        // 创建 8 个保存点（达到上限）
        for _ in 0..8 {
            tx.savepoint().await.unwrap();
        }

        // 第 9 个保存点应失败
        let result = tx.savepoint().await;
        assert!(result.is_err());
        match result {
            Err(TxError::MaxNestingDepthExceeded {
                current_depth,
                max_depth,
            }) => {
                assert_eq!(current_depth, 9);
                assert_eq!(max_depth, 8);
            }
            _ => panic!("Expected MaxNestingDepthExceeded error"),
        }
    }

    #[tokio::test]
    async fn test_h8_savepoint_with_custom_depth_3() {
        let conn = Box::new(MockConnection::new());
        let mut tx = Transaction::new(conn, TransactOptions::default().with_max_nesting_depth(3));

        // 3 个保存点应成功
        for i in 1..=3 {
            let sp = tx.savepoint().await.unwrap();
            assert_eq!(sp, format!("sp_{}", i));
        }

        // 第 4 个应失败
        let result = tx.savepoint().await;
        assert!(result.is_err());
        match result {
            Err(TxError::MaxNestingDepthExceeded {
                current_depth,
                max_depth,
            }) => {
                assert_eq!(current_depth, 4);
                assert_eq!(max_depth, 3);
            }
            _ => panic!("Expected MaxNestingDepthExceeded error"),
        }
    }

    #[tokio::test]
    async fn test_h8_savepoint_depth_zero_disables_nesting() {
        let conn = Box::new(MockConnection::new());
        let mut tx = Transaction::new(conn, TransactOptions::default().with_max_nesting_depth(0));

        // 深度 0 表示禁用嵌套，首次 savepoint 即失败
        let result = tx.savepoint().await;
        assert!(result.is_err());
        match result {
            Err(TxError::MaxNestingDepthExceeded {
                current_depth,
                max_depth,
            }) => {
                assert_eq!(current_depth, 1);
                assert_eq!(max_depth, 0);
            }
            _ => panic!("Expected MaxNestingDepthExceeded error"),
        }
    }

    #[tokio::test]
    async fn test_h8_savepoint_after_rollback_to_still_respects_depth() {
        // 即使回滚到保存点，savepoint_counter 不减少（保存点栈可能仍存在），
        // 因此深度检查仍以 savepoint_counter 为准
        let conn = Box::new(MockConnection::new());
        let mut tx = Transaction::new(conn, TransactOptions::default().with_max_nesting_depth(2));

        let sp1 = tx.savepoint().await.unwrap();
        let sp2 = tx.savepoint().await.unwrap();

        // 回滚到 sp1（不重置计数器）
        tx.rollback_to_savepoint(&sp1).await.unwrap();
        tx.release_savepoint(&sp2).await.unwrap();

        // 第 3 个保存点仍应失败（计数器不回退）
        let result = tx.savepoint().await;
        assert!(result.is_err());
        match result {
            Err(TxError::MaxNestingDepthExceeded {
                current_depth,
                max_depth,
            }) => {
                assert_eq!(current_depth, 3);
                assert_eq!(max_depth, 2);
            }
            _ => panic!("Expected MaxNestingDepthExceeded error"),
        }
    }

    #[tokio::test]
    async fn test_h8_max_nesting_depth_error_display() {
        let err = TxError::MaxNestingDepthExceeded {
            current_depth: 10,
            max_depth: 8,
        };
        let msg = format!("{}", err);
        assert!(msg.contains("10"));
        assert!(msg.contains("8"));
        assert!(msg.contains("exceeds"));
    }

    // ==================== M-8 死锁检测重试测试 ====================

    #[test]
    fn test_m8_is_deadlock_error_mysql() {
        assert!(is_deadlock_error(
            "Deadlock found when trying to get lock; try restarting transaction"
        ));
        assert!(is_deadlock_error("Error 1213: Deadlock found"));
        assert!(is_deadlock_error("MySQL error (1213)"));
    }

    #[test]
    fn test_m8_is_deadlock_error_postgresql() {
        assert!(is_deadlock_error("deadlock detected"));
        assert!(is_deadlock_error("ERROR: deadlock detected (40P01)"));
        assert!(is_deadlock_error("SQLSTATE 40P01"));
    }

    #[test]
    fn test_m8_is_deadlock_error_sqlite() {
        assert!(is_deadlock_error("database is locked"));
        assert!(is_deadlock_error("database table is locked"));
    }

    #[test]
    fn test_m8_is_deadlock_error_oracle() {
        assert!(is_deadlock_error(
            "ORA-00060: deadlock detected while waiting for resource"
        ));
    }

    #[test]
    fn test_m8_is_deadlock_error_sql_server() {
        assert!(is_deadlock_error(
            "Transaction (Process ID 52) was deadlocked on lock resources"
        ));
        assert!(is_deadlock_error("Error 1205: Transaction was deadlocked"));
    }

    #[test]
    fn test_m8_is_deadlock_error_non_deadlock() {
        assert!(!is_deadlock_error("connection refused"));
        assert!(!is_deadlock_error("syntax error near SELECT"));
        assert!(!is_deadlock_error("permission denied for table users"));
        assert!(!is_deadlock_error(""));
    }

    #[tokio::test]
    async fn test_m8_retry_on_deadlock_succeeds_first_attempt() {
        use std::sync::atomic::{AtomicU32, Ordering};

        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result: Result<u32, TxError> =
            retry_on_deadlock(3, Duration::from_millis(1), |_attempt| {
                let c = counter_clone.clone();
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    Ok(42u32)
                }
            })
            .await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_m8_retry_on_deadlock_retries_on_deadlock_error() {
        use std::sync::atomic::{AtomicU32, Ordering};

        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result: Result<u32, TxError> =
            retry_on_deadlock(3, Duration::from_millis(1), |_attempt| {
                let c = counter_clone.clone();
                async move {
                    let n = c.fetch_add(1, Ordering::SeqCst);
                    if n < 2 {
                        // 前两次返回死锁错误
                        Err(TxError::CommitFailed(
                            "Deadlock found when trying to get lock".to_string(),
                        ))
                    } else {
                        Ok(42u32)
                    }
                }
            })
            .await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_m8_retry_on_deadlock_returns_error_after_max_attempts() {
        use std::sync::atomic::{AtomicU32, Ordering};

        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result: Result<u32, TxError> =
            retry_on_deadlock(2, Duration::from_millis(1), |_attempt| {
                let c = counter_clone.clone();
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    Err(TxError::CommitFailed(
                        "Deadlock found when trying to get lock".to_string(),
                    ))
                }
            })
            .await;

        // 应返回最后一次的死锁错误
        assert!(result.is_err());
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn test_m8_retry_on_deadlock_does_not_retry_non_deadlock_errors() {
        use std::sync::atomic::{AtomicU32, Ordering};

        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result: Result<u32, TxError> =
            retry_on_deadlock(3, Duration::from_millis(1), |_attempt| {
                let c = counter_clone.clone();
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    Err(TxError::CommitFailed("syntax error".to_string()))
                }
            })
            .await;

        // 非死锁错误应立即返回，不重试
        assert!(result.is_err());
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_m8_deadlock_error_display() {
        let err = TxError::DeadlockDetected {
            attempt: 2,
            max_attempts: 3,
        };
        let msg = format!("{}", err);
        assert!(msg.contains("2"));
        assert!(msg.contains("3"));
        assert!(msg.contains("Deadlock"));
    }
}
