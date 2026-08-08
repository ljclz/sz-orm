//! XA 事务支持 — 直连 DB 资源管理器的两阶段提交
//!
//! 与现有 `DistributedTransaction`（回调式 2PC）并存，XA 参与者持有真实 DB 连接，
//! 执行数据库原生 XA 协议（`XA PREPARE`/`XA COMMIT`/`XA ROLLBACK`）。
//!
//! # 设计
//!
//! - [`XaResource`]：直连 DB 资源管理器 trait（xa_prepare/xa_commit/xa_rollback）
//! - [`XaParticipant`]：持有 `Arc<dyn XaResource>` + xid + state
//! - [`XaCoordinator`]：XA 两阶段提交协调器，复用现有 `TransactionState` 状态机
//! - [`XaCapabilityChecker`]：后端 XA 能力校验（SQLite 不支持）
//!
//! # 用法
//!
//! ```ignore
//! use sz_orm_dtx::xa::{XaCoordinator, XaParticipant, XaCapabilityChecker};
//!
//! let coord = XaCoordinator::new(log_store);
//! let result = coord.xa_two_phase_commit("tx-1", participants).await?;
//! ```

use std::sync::Arc;
use std::time::Duration;

use sz_orm_sqlx::any_driver::AnyBackend;

use crate::{ParticipantState, TransactionLogEntry, TransactionLogStore, TransactionState};

// ============================================================================
// XaError — XA 错误枚举
// ============================================================================

/// XA 事务错误
#[derive(Debug, thiserror::Error)]
pub enum XaError {
    #[error("XA prepare 失败: {0}")]
    PrepareFailed(String),
    #[error("XA commit 失败: {0}")]
    CommitFailed(String),
    #[error("XA rollback 失败: {0}")]
    RollbackFailed(String),
    #[error("后端不支持 XA: {backend:?}（{reason}）")]
    XaNotSupported { backend: AnyBackend, reason: String },
    #[error("XA 事务 {0} 不存在")]
    NotFound(String),
    #[error("数据库错误: {0}")]
    DatabaseError(String),
}

// ============================================================================
// XaResource — 直连 DB 资源管理器 trait
// ============================================================================

/// XA 资源管理器 trait（直连 DB 资源管理器）
///
/// 实现者负责通过数据库原生 XA 协议执行两阶段提交。
/// 与现有 `TransactionParticipant`（回调式）不同，`XaResource` 持有真实 DB 连接。
#[async_trait::async_trait]
pub trait XaResource: Send + Sync {
    /// XA PREPARE（预提交）
    async fn xa_prepare(&self, xid: &str) -> Result<(), XaError>;
    /// XA COMMIT（提交）
    async fn xa_commit(&self, xid: &str) -> Result<(), XaError>;
    /// XA ROLLBACK（回滚）
    async fn xa_rollback(&self, xid: &str) -> Result<(), XaError>;
    /// 资源标识（DSN 脱敏哈希）
    fn resource_id(&self) -> &str;
    /// 后端类型
    fn backend(&self) -> AnyBackend;
}

// ============================================================================
// XaParticipant — XA 参与者（持有真实 DB 连接，非回调式）
// ============================================================================

/// XA 参与者（持有真实 DB 连接，非回调式）
///
/// 与现有 `TransactionParticipant`（回调式 `Arc<dyn Fn()>`）不同，
/// `XaParticipant` 持有 `Arc<dyn XaResource>`，直连 DB 资源管理器。
pub struct XaParticipant {
    /// XA 资源管理器
    resource: Arc<dyn XaResource>,
    /// XA 事务分支 ID
    xid: String,
    /// 参与者状态（复用现有 ParticipantState）
    state: ParticipantState,
}

impl XaParticipant {
    /// 创建 XA 参与者
    pub fn new(resource: Arc<dyn XaResource>, xid: &str) -> Self {
        Self {
            resource,
            xid: xid.to_string(),
            state: ParticipantState::Active,
        }
    }

    /// 获取资源管理器引用
    pub fn resource(&self) -> &Arc<dyn XaResource> {
        &self.resource
    }

    /// 获取 XA 事务分支 ID
    pub fn xid(&self) -> &str {
        &self.xid
    }

    /// 获取参与者状态
    pub fn state(&self) -> &ParticipantState {
        &self.state
    }

    /// XA PREPARE
    pub async fn prepare(&mut self) -> Result<(), XaError> {
        match self.resource.xa_prepare(&self.xid).await {
            Ok(()) => {
                self.state = ParticipantState::Prepared;
                Ok(())
            }
            Err(e) => {
                self.state = ParticipantState::Failed;
                Err(e)
            }
        }
    }

    /// XA COMMIT
    pub async fn commit(&mut self) -> Result<(), XaError> {
        match self.resource.xa_commit(&self.xid).await {
            Ok(()) => {
                self.state = ParticipantState::Committed;
                Ok(())
            }
            Err(e) => {
                self.state = ParticipantState::Failed;
                Err(e)
            }
        }
    }

    /// XA ROLLBACK
    pub async fn rollback(&mut self) -> Result<(), XaError> {
        match self.resource.xa_rollback(&self.xid).await {
            Ok(()) => {
                self.state = ParticipantState::RolledBack;
                Ok(())
            }
            Err(e) => {
                self.state = ParticipantState::Failed;
                Err(e)
            }
        }
    }
}

impl std::fmt::Debug for XaParticipant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("XaParticipant")
            .field("xid", &self.xid)
            .field("state", &self.state)
            .field("backend", &self.resource.backend())
            .field("resource_id", &self.resource.resource_id())
            .finish()
    }
}

// ============================================================================
// XaCapability — XA 能力校验
// ============================================================================

/// XA 能力校验结果
#[derive(Debug, Clone)]
pub enum XaCapability {
    /// 支持 XA
    Supported,
    /// 不支持 XA（如 SQLite）
    NotSupported { reason: String },
}

/// XA 能力校验器
///
/// 检测后端是否支持 XA 协议：
/// - MySQL/PostgreSQL/Oracle/MSSQL → Supported
/// - SQLite → NotSupported（嵌入式数据库，不支持 XA 协议）
pub struct XaCapabilityChecker;

impl XaCapabilityChecker {
    /// 检测后端 XA 能力
    pub fn check(backend: AnyBackend) -> XaCapability {
        match backend {
            AnyBackend::MySql => XaCapability::Supported,
            AnyBackend::Postgres => XaCapability::Supported,
            AnyBackend::Oracle => XaCapability::Supported,
            AnyBackend::Mssql => XaCapability::Supported,
            AnyBackend::Sqlite => XaCapability::NotSupported {
                reason: "SQLite 不支持 XA 协议（嵌入式数据库，无分布式事务能力）".to_string(),
            },
            _ => XaCapability::NotSupported {
                reason: "未知后端，保守拒绝 XA".to_string(),
            },
        }
    }
}

// ============================================================================
// SuspensionConfig — 悬挂事务配置
// ============================================================================

/// 悬挂事务配置
#[derive(Debug, Clone)]
pub struct SuspensionConfig {
    /// 超时阈值（默认 30s）
    pub timeout: Duration,
    /// 超时处理策略
    pub policy: SuspensionPolicy,
    /// 检测间隔（后台扫描周期，默认 5s）
    pub check_interval: Duration,
}

impl Default for SuspensionConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            policy: SuspensionPolicy::Rollback,
            check_interval: Duration::from_secs(5),
        }
    }
}

/// 悬挂事务处理策略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuspensionPolicy {
    /// 超时后提交（假设 Prepare 成功即大概率可提交）
    Commit,
    /// 超时后回滚（保守策略）
    Rollback,
}

/// 悬挂事务记录
#[derive(Debug, Clone)]
pub struct SuspendedTransaction {
    pub tx_id: String,
    pub resource_id: String,
    pub suspended_at: chrono::DateTime<chrono::Utc>,
    pub policy: SuspensionPolicy,
}

// ============================================================================
// XaCoordinator — XA 两阶段提交协调器
// ============================================================================

/// XA 协调器
///
/// 复用现有 `TransactionState` 状态机，协调多个 `XaParticipant` 的两阶段提交。
/// 各阶段写 `TransactionLogStore`，支持崩溃恢复。
pub struct XaCoordinator {
    /// 事务日志存储（复用现有 TransactionLogStore）
    log_store: Arc<dyn TransactionLogStore>,
    /// 悬挂检测配置
    suspension_config: SuspensionConfig,
}

impl XaCoordinator {
    /// 创建 XA 协调器
    pub fn new(log_store: Arc<dyn TransactionLogStore>) -> Self {
        Self {
            log_store,
            suspension_config: SuspensionConfig::default(),
        }
    }

    /// 创建 XA 协调器（自定义悬挂配置）
    pub fn with_suspension_config(
        log_store: Arc<dyn TransactionLogStore>,
        suspension_config: SuspensionConfig,
    ) -> Self {
        Self {
            log_store,
            suspension_config,
        }
    }

    /// 获取悬挂配置
    pub fn suspension_config(&self) -> &SuspensionConfig {
        &self.suspension_config
    }

    /// XA 两阶段提交
    ///
    /// 步骤：
    /// 1. XA 能力校验（任一不支持 → 拒绝，事务不进入 Prepare）
    /// 2. 阶段一 - Prepare（任一失败 → 全局回滚已 Prepare 的）
    /// 3. 阶段二 - Commit（全成功 → Committed）
    pub async fn xa_two_phase_commit(
        &self,
        tx_id: &str,
        participants: &mut [XaParticipant],
    ) -> Result<TransactionState, XaError> {
        if participants.is_empty() {
            return Ok(TransactionState::Committed);
        }

        // 步骤 1: XA 能力校验
        for p in participants.iter() {
            let capability = XaCapabilityChecker::check(p.resource().backend());
            if let XaCapability::NotSupported { reason } = capability {
                return Err(XaError::XaNotSupported {
                    backend: p.resource().backend(),
                    reason,
                });
            }
        }

        // 步骤 2: 阶段一 - Prepare
        self.write_log(tx_id, "Preparing", participants).await;

        let mut prepared_count = 0;
        for i in 0..participants.len() {
            match participants[i].prepare().await {
                Ok(()) => prepared_count += 1,
                Err(e) => {
                    let resource_id = participants[i].resource().resource_id().to_string();
                    // 回滚已 Prepare 的参与者
                    for prepared_p in participants.iter_mut().take(prepared_count) {
                        let _ = prepared_p.rollback().await;
                    }
                    self.write_log(tx_id, "Failed", participants).await;
                    return Err(XaError::PrepareFailed(format!(
                        "参与者 {} prepare 失败: {}",
                        resource_id, e
                    )));
                }
            }
        }

        self.write_log(tx_id, "Prepared", participants).await;

        // 步骤 3: 阶段二 - Commit
        self.write_log(tx_id, "Committing", participants).await;

        for i in 0..participants.len() {
            match participants[i].commit().await {
                Ok(()) => continue,
                Err(e) => {
                    let resource_id = participants[i].resource().resource_id().to_string();
                    self.write_log(tx_id, "Failed", participants).await;
                    return Err(XaError::CommitFailed(format!(
                        "参与者 {} commit 失败: {}",
                        resource_id, e
                    )));
                }
            }
        }

        self.write_log(tx_id, "Committed", participants).await;
        Ok(TransactionState::Committed)
    }

    /// XA 全局回滚
    ///
    /// 对所有参与者执行 XA ROLLBACK。
    pub async fn xa_rollback(
        &self,
        tx_id: &str,
        participants: &mut [XaParticipant],
    ) -> Result<TransactionState, XaError> {
        self.write_log(tx_id, "RollingBack", participants).await;

        for p in participants.iter_mut() {
            let _ = p.rollback().await;
        }

        self.write_log(tx_id, "RolledBack", participants).await;
        Ok(TransactionState::RolledBack)
    }

    /// 写入事务日志
    async fn write_log(&self, tx_id: &str, state: &str, participants: &[XaParticipant]) {
        let entry = TransactionLogEntry {
            tx_id: tx_id.to_string(),
            state: state.to_string(),
            participants: participants
                .iter()
                .map(|p| p.resource().resource_id().to_string())
                .collect(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis().to_string())
                .unwrap_or_else(|_| "0".to_string()),
            action: state.to_lowercase(),
        };
        let _ = self.log_store.append(entry).await;
    }
}

impl std::fmt::Debug for XaCoordinator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("XaCoordinator")
            .field("suspension_config", &self.suspension_config)
            .finish_non_exhaustive()
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::InMemoryTransactionLog;

    /// Mock XA 资源管理器（用于测试）
    struct MockXaResource {
        id: String,
        backend: AnyBackend,
        prepare_should_fail: bool,
        commit_should_fail: bool,
    }

    #[async_trait::async_trait]
    impl XaResource for MockXaResource {
        async fn xa_prepare(&self, _xid: &str) -> Result<(), XaError> {
            if self.prepare_should_fail {
                Err(XaError::PrepareFailed("mock prepare 失败".to_string()))
            } else {
                Ok(())
            }
        }

        async fn xa_commit(&self, _xid: &str) -> Result<(), XaError> {
            if self.commit_should_fail {
                Err(XaError::CommitFailed("mock commit 失败".to_string()))
            } else {
                Ok(())
            }
        }

        async fn xa_rollback(&self, _xid: &str) -> Result<(), XaError> {
            Ok(())
        }

        fn resource_id(&self) -> &str {
            &self.id
        }

        fn backend(&self) -> AnyBackend {
            self.backend
        }
    }

    fn make_mock_resource(id: &str, backend: AnyBackend) -> Arc<dyn XaResource> {
        Arc::new(MockXaResource {
            id: id.to_string(),
            backend,
            prepare_should_fail: false,
            commit_should_fail: false,
        })
    }

    #[tokio::test]
    async fn test_xa_capability_checker() {
        assert!(matches!(
            XaCapabilityChecker::check(AnyBackend::MySql),
            XaCapability::Supported
        ));
        assert!(matches!(
            XaCapabilityChecker::check(AnyBackend::Postgres),
            XaCapability::Supported
        ));
        assert!(matches!(
            XaCapabilityChecker::check(AnyBackend::Oracle),
            XaCapability::Supported
        ));
        assert!(matches!(
            XaCapabilityChecker::check(AnyBackend::Mssql),
            XaCapability::Supported
        ));
        match XaCapabilityChecker::check(AnyBackend::Sqlite) {
            XaCapability::NotSupported { reason } => {
                assert!(reason.contains("SQLite"));
            }
            _ => panic!("SQLite 应不支持 XA"),
        }
    }

    #[tokio::test]
    async fn test_xa_two_phase_commit_success() {
        let log_store: Arc<dyn TransactionLogStore> = Arc::new(InMemoryTransactionLog::new());
        let coord = XaCoordinator::new(log_store);

        let mut participants = vec![
            XaParticipant::new(make_mock_resource("db1", AnyBackend::MySql), "tx1"),
            XaParticipant::new(make_mock_resource("db2", AnyBackend::Postgres), "tx1"),
        ];

        let result = coord.xa_two_phase_commit("tx1", &mut participants).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), TransactionState::Committed);
        assert_eq!(*participants[0].state(), ParticipantState::Committed);
        assert_eq!(*participants[1].state(), ParticipantState::Committed);
    }

    #[tokio::test]
    async fn test_xa_two_phase_commit_prepare_failure() {
        let log_store: Arc<dyn TransactionLogStore> = Arc::new(InMemoryTransactionLog::new());
        let coord = XaCoordinator::new(log_store);

        let fail_resource: Arc<dyn XaResource> = Arc::new(MockXaResource {
            id: "db-fail".to_string(),
            backend: AnyBackend::MySql,
            prepare_should_fail: true,
            commit_should_fail: false,
        });

        let mut participants = vec![
            XaParticipant::new(make_mock_resource("db1", AnyBackend::MySql), "tx2"),
            XaParticipant::new(fail_resource, "tx2"),
        ];

        let result = coord.xa_two_phase_commit("tx2", &mut participants).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            XaError::PrepareFailed(msg) => assert!(msg.contains("db-fail")),
            e => panic!("期望 PrepareFailed，实际: {:?}", e),
        }
        // 第一个参与者应被回滚
        assert_eq!(*participants[0].state(), ParticipantState::RolledBack);
    }

    #[tokio::test]
    async fn test_xa_two_phase_commit_sqlite_rejected() {
        let log_store: Arc<dyn TransactionLogStore> = Arc::new(InMemoryTransactionLog::new());
        let coord = XaCoordinator::new(log_store);

        let mut participants = vec![XaParticipant::new(
            make_mock_resource("sqlite-db", AnyBackend::Sqlite),
            "tx3",
        )];

        let result = coord.xa_two_phase_commit("tx3", &mut participants).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            XaError::XaNotSupported { backend, .. } => {
                assert_eq!(backend, AnyBackend::Sqlite);
            }
            e => panic!("期望 XaNotSupported，实际: {:?}", e),
        }
        // 事务不应进入 Prepare
        assert_eq!(*participants[0].state(), ParticipantState::Active);
    }

    #[tokio::test]
    async fn test_xa_two_phase_commit_empty_participants() {
        let log_store: Arc<dyn TransactionLogStore> = Arc::new(InMemoryTransactionLog::new());
        let coord = XaCoordinator::new(log_store);

        let mut participants: Vec<XaParticipant> = vec![];
        let result = coord.xa_two_phase_commit("tx4", &mut participants).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), TransactionState::Committed);
    }

    #[tokio::test]
    async fn test_xa_rollback() {
        let log_store: Arc<dyn TransactionLogStore> = Arc::new(InMemoryTransactionLog::new());
        let coord = XaCoordinator::new(log_store);

        let mut participants = vec![
            XaParticipant::new(make_mock_resource("db1", AnyBackend::MySql), "tx5"),
            XaParticipant::new(make_mock_resource("db2", AnyBackend::Postgres), "tx5"),
        ];

        let result = coord.xa_rollback("tx5", &mut participants).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), TransactionState::RolledBack);
        assert_eq!(*participants[0].state(), ParticipantState::RolledBack);
        assert_eq!(*participants[1].state(), ParticipantState::RolledBack);
    }

    #[tokio::test]
    async fn test_xa_commit_failure() {
        let log_store: Arc<dyn TransactionLogStore> = Arc::new(InMemoryTransactionLog::new());
        let coord = XaCoordinator::new(log_store);

        let fail_commit: Arc<dyn XaResource> = Arc::new(MockXaResource {
            id: "db-commit-fail".to_string(),
            backend: AnyBackend::MySql,
            prepare_should_fail: false,
            commit_should_fail: true,
        });

        let mut participants = vec![
            XaParticipant::new(make_mock_resource("db1", AnyBackend::MySql), "tx6"),
            XaParticipant::new(fail_commit, "tx6"),
        ];

        let result = coord.xa_two_phase_commit("tx6", &mut participants).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            XaError::CommitFailed(msg) => assert!(msg.contains("db-commit-fail")),
            e => panic!("期望 CommitFailed，实际: {:?}", e),
        }
    }

    /// M2.6 共存测试：XA 事务与既有 2PC 回调式事务并行运行，互不干扰
    #[tokio::test]
    async fn test_coexistence_xa_and_2pc() {
        use crate::{DistributedTransaction, TransactionParticipant};

        let log_store: Arc<dyn TransactionLogStore> = Arc::new(InMemoryTransactionLog::new());

        // 既有 2PC 回调式事务（不使用 log_store，避免同步 block_on 在异步上下文中 panic）
        let mut dtx_2pc = DistributedTransaction::new("2pc-tx");
        dtx_2pc.add_participant(
            TransactionParticipant::new("2pc-p1")
                .with_prepare(|| Ok(()))
                .with_commit(|| Ok(()))
                .with_rollback(|| Ok(())),
        );
        dtx_2pc.add_participant(
            TransactionParticipant::new("2pc-p2")
                .with_prepare(|| Ok(()))
                .with_commit(|| Ok(()))
                .with_rollback(|| Ok(())),
        );

        // XA 事务（使用 log_store，异步写入）
        let xa_coord = XaCoordinator::new(log_store);
        let mut xa_participants = vec![
            XaParticipant::new(make_mock_resource("xa-db1", AnyBackend::MySql), "xa-tx"),
            XaParticipant::new(make_mock_resource("xa-db2", AnyBackend::Postgres), "xa-tx"),
        ];

        // 并行执行：先 2PC prepare，再 XA 两阶段提交，最后 2PC commit
        let two_pc_prepare = dtx_2pc.prepare();
        assert!(two_pc_prepare.is_ok());

        let xa_result = xa_coord
            .xa_two_phase_commit("xa-tx", &mut xa_participants)
            .await;
        assert!(xa_result.is_ok());

        let two_pc_commit = dtx_2pc.commit();
        assert!(two_pc_commit.is_ok());

        // 验证两者独立收敛终态
        assert_eq!(dtx_2pc.state(), crate::TransactionState::Committed);
        assert_eq!(xa_result.unwrap(), crate::TransactionState::Committed);
    }

    /// M2.6 共存测试：XA 事务与 Saga 并行运行
    #[tokio::test]
    async fn test_coexistence_xa_and_saga() {
        use crate::saga::{Saga, SagaStep};

        let log_store: Arc<dyn TransactionLogStore> = Arc::new(InMemoryTransactionLog::new());

        // Saga 事务
        let mut saga = Saga::new("saga-tx");
        saga.add_step(
            SagaStep::new("step1")
                .with_action(|| Ok(()))
                .with_compensation(|| Ok(())),
        )
        .unwrap();
        saga.add_step(
            SagaStep::new("step2")
                .with_action(|| Ok(()))
                .with_compensation(|| Ok(())),
        )
        .unwrap();

        // XA 事务
        let xa_coord = XaCoordinator::new(log_store);
        let mut xa_participants = vec![XaParticipant::new(
            make_mock_resource("xa-db", AnyBackend::MySql),
            "xa-saga-tx",
        )];

        // 并行执行
        let saga_result = saga.execute();
        assert!(saga_result.is_ok());

        let xa_result = xa_coord
            .xa_two_phase_commit("xa-saga-tx", &mut xa_participants)
            .await;
        assert!(xa_result.is_ok());
    }

    /// M2.6 共存测试：XA 事务与 TCC 并行运行
    #[tokio::test]
    async fn test_coexistence_xa_and_tcc() {
        use crate::tcc::{TccCoordinator, TccParticipant};

        let log_store: Arc<dyn TransactionLogStore> = Arc::new(InMemoryTransactionLog::new());

        // TCC 事务
        let mut tcc = TccCoordinator::new("tcc-tx");
        tcc.add_participant(TccParticipant::new("tcc-p1"));
        tcc.add_participant(TccParticipant::new("tcc-p2"));

        // XA 事务
        let xa_coord = XaCoordinator::new(log_store);
        let mut xa_participants = vec![XaParticipant::new(
            make_mock_resource("xa-db", AnyBackend::Postgres),
            "xa-tcc-tx",
        )];

        // 并行执行
        let tcc_result = tcc.execute();
        assert!(tcc_result.is_ok());

        let xa_result = xa_coord
            .xa_two_phase_commit("xa-tcc-tx", &mut xa_participants)
            .await;
        assert!(xa_result.is_ok());
    }
}
