//! XA 事务集成测试
//!
//! 覆盖两库 XA 真实提交、Prepare 失败全局回滚、协调者崩溃恢复、悬挂超时处理。
//! 真实数据库测试标注 `#[ignore]`，通过 `cargo test -- --ignored` 触发。

#![cfg(feature = "xa")]

use std::sync::Arc;

use sz_orm_dtx::suspension::SuspensionDetector;
use sz_orm_dtx::xa::{
    SuspensionConfig, XaCapability, XaCapabilityChecker, XaCoordinator, XaError, XaParticipant,
    XaResource,
};
use sz_orm_dtx::{InMemoryTransactionLog, ParticipantState, TransactionLogStore};
use sz_orm_sqlx::any_driver::AnyBackend;

/// Mock XA 资源管理器
struct MockXaResource {
    id: String,
    backend: AnyBackend,
    prepare_fail: bool,
    commit_fail: bool,
}

#[async_trait::async_trait]
impl XaResource for MockXaResource {
    async fn xa_prepare(&self, _xid: &str) -> Result<(), XaError> {
        if self.prepare_fail {
            Err(XaError::PrepareFailed("mock prepare 失败".to_string()))
        } else {
            Ok(())
        }
    }
    async fn xa_commit(&self, _xid: &str) -> Result<(), XaError> {
        if self.commit_fail {
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

fn mock_resource(id: &str, backend: AnyBackend) -> Arc<dyn XaResource> {
    Arc::new(MockXaResource {
        id: id.to_string(),
        backend,
        prepare_fail: false,
        commit_fail: false,
    })
}

// ============================================================================
// 测试 1：两库 XA 真实提交（Mock）
// ============================================================================

#[tokio::test]
async fn test_xa_two_db_commit_success() {
    let log_store: Arc<dyn TransactionLogStore> = Arc::new(InMemoryTransactionLog::new());
    let coord = XaCoordinator::new(log_store);

    let mut participants = vec![
        XaParticipant::new(mock_resource("mysql-db", AnyBackend::MySql), "tx1"),
        XaParticipant::new(mock_resource("pg-db", AnyBackend::Postgres), "tx1"),
    ];

    let result = coord.xa_two_phase_commit("tx1", &mut participants).await;
    assert!(result.is_ok());
    assert_eq!(*participants[0].state(), ParticipantState::Committed);
    assert_eq!(*participants[1].state(), ParticipantState::Committed);
}

// ============================================================================
// 测试 2：Prepare 失败全局回滚
// ============================================================================

#[tokio::test]
async fn test_xa_prepare_failure_global_rollback() {
    let log_store: Arc<dyn TransactionLogStore> = Arc::new(InMemoryTransactionLog::new());
    let coord = XaCoordinator::new(log_store);

    let fail_resource: Arc<dyn XaResource> = Arc::new(MockXaResource {
        id: "fail-db".to_string(),
        backend: AnyBackend::MySql,
        prepare_fail: true,
        commit_fail: false,
    });

    let mut participants = vec![
        XaParticipant::new(mock_resource("ok-db", AnyBackend::MySql), "tx2"),
        XaParticipant::new(fail_resource, "tx2"),
    ];

    let result = coord.xa_two_phase_commit("tx2", &mut participants).await;
    assert!(result.is_err());
    // 第一个参与者应被回滚
    assert_eq!(*participants[0].state(), ParticipantState::RolledBack);
    // 第二个参与者应失败
    assert_eq!(*participants[1].state(), ParticipantState::Failed);
}

// ============================================================================
// 测试 3：协调者崩溃恢复
// ============================================================================

#[tokio::test]
async fn test_xa_coordinator_crash_recovery() {
    use sz_orm_dtx::recovery::{RecoveryStrategy, XaRecoveryCoordinator};

    let log_store: Arc<dyn TransactionLogStore> = Arc::new(InMemoryTransactionLog::new());

    // 模拟崩溃：写入 Prepared 日志后协调者崩溃
    log_store
        .append(sz_orm_dtx::TransactionLogEntry {
            tx_id: "crash-tx".to_string(),
            state: "Prepared".to_string(),
            participants: vec!["db1".to_string(), "db2".to_string()],
            timestamp: "0".to_string(),
            action: "prepare".to_string(),
        })
        .await
        .unwrap();

    // 重启后恢复
    let coord = Arc::new(XaCoordinator::new(log_store.clone()));
    let recovery = XaRecoveryCoordinator::new(log_store, coord);

    let results = recovery.recover().await;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].tx_id, "crash-tx");
    assert_eq!(results[0].strategy, RecoveryStrategy::CommitPrepared);
    assert!(results[0].success);
}

// ============================================================================
// 测试 4：悬挂超时处理
// ============================================================================

#[tokio::test]
async fn test_xa_suspension_timeout() {
    use std::time::Duration;

    let log_store: Arc<dyn TransactionLogStore> = Arc::new(InMemoryTransactionLog::new());

    // 写入一条很久以前的 Prepared 日志
    log_store
        .append(sz_orm_dtx::TransactionLogEntry {
            tx_id: "suspended-tx".to_string(),
            state: "Prepared".to_string(),
            participants: vec!["db1".to_string()],
            timestamp: "0".to_string(),
            action: "prepare".to_string(),
        })
        .await
        .unwrap();

    let config = SuspensionConfig {
        timeout: Duration::from_secs(1),
        ..Default::default()
    };
    let detector = SuspensionDetector::new(log_store, config);

    let suspended = detector.detect_suspended().await;
    assert_eq!(suspended.len(), 1);
    assert_eq!(suspended[0].tx_id, "suspended-tx");
}

// ============================================================================
// 测试 5：XA 能力校验
// ============================================================================

#[tokio::test]
async fn test_xa_capability_all_backends() {
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
    assert!(matches!(
        XaCapabilityChecker::check(AnyBackend::Sqlite),
        XaCapability::NotSupported { .. }
    ));
}

// ============================================================================
// 测试 6：SQLite 拒绝 XA
// ============================================================================

#[tokio::test]
async fn test_xa_sqlite_rejected() {
    let log_store: Arc<dyn TransactionLogStore> = Arc::new(InMemoryTransactionLog::new());
    let coord = XaCoordinator::new(log_store);

    let mut participants = vec![XaParticipant::new(
        mock_resource("sqlite-db", AnyBackend::Sqlite),
        "tx-sqlite",
    )];

    let result = coord
        .xa_two_phase_commit("tx-sqlite", &mut participants)
        .await;
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

// ============================================================================
// 真实数据库集成测试（#[ignore]）
// ============================================================================

/// 真实 MySQL XA 资源管理器
#[cfg(feature = "real-db")]
struct MySqlXaResource {
    pool: sqlx::MySqlPool,
    id: String,
}

#[cfg(feature = "real-db")]
#[async_trait::async_trait]
impl XaResource for MySqlXaResource {
    async fn xa_prepare(&self, xid: &str) -> Result<(), XaError> {
        sqlx::query(sqlx::AssertSqlSafe(&*format!("XA PREPARE '{}'", xid)))
            .execute(&self.pool)
            .await
            .map_err(|e| XaError::PrepareFailed(e.to_string()))?;
        Ok(())
    }
    async fn xa_commit(&self, xid: &str) -> Result<(), XaError> {
        sqlx::query(sqlx::AssertSqlSafe(&*format!("XA COMMIT '{}'", xid)))
            .execute(&self.pool)
            .await
            .map_err(|e| XaError::CommitFailed(e.to_string()))?;
        Ok(())
    }
    async fn xa_rollback(&self, xid: &str) -> Result<(), XaError> {
        sqlx::query(sqlx::AssertSqlSafe(&*format!("XA ROLLBACK '{}'", xid)))
            .execute(&self.pool)
            .await
            .map_err(|e| XaError::RollbackFailed(e.to_string()))?;
        Ok(())
    }
    fn resource_id(&self) -> &str {
        &self.id
    }
    fn backend(&self) -> AnyBackend {
        AnyBackend::MySql
    }
}

/// 真实 PostgreSQL XA 资源管理器
#[cfg(feature = "real-db")]
struct PostgresXaResource {
    pool: sqlx::PgPool,
    id: String,
}

#[cfg(feature = "real-db")]
#[async_trait::async_trait]
impl XaResource for PostgresXaResource {
    async fn xa_prepare(&self, xid: &str) -> Result<(), XaError> {
        sqlx::query(sqlx::AssertSqlSafe(&*format!(
            "PREPARE TRANSACTION '{}'",
            xid
        )))
        .execute(&self.pool)
        .await
        .map_err(|e| XaError::PrepareFailed(e.to_string()))?;
        Ok(())
    }
    async fn xa_commit(&self, xid: &str) -> Result<(), XaError> {
        sqlx::query(sqlx::AssertSqlSafe(&*format!("COMMIT PREPARED '{}'", xid)))
            .execute(&self.pool)
            .await
            .map_err(|e| XaError::CommitFailed(e.to_string()))?;
        Ok(())
    }
    async fn xa_rollback(&self, xid: &str) -> Result<(), XaError> {
        sqlx::query(sqlx::AssertSqlSafe(&*format!(
            "ROLLBACK PREPARED '{}'",
            xid
        )))
        .execute(&self.pool)
        .await
        .map_err(|e| XaError::RollbackFailed(e.to_string()))?;
        Ok(())
    }
    fn resource_id(&self) -> &str {
        &self.id
    }
    fn backend(&self) -> AnyBackend {
        AnyBackend::Postgres
    }
}

/// 真实两库 XA 提交测试（MySQL + PostgreSQL）
#[cfg(feature = "real-db")]
#[tokio::test]
#[ignore]
async fn test_real_xa_mysql_pg_commit() {
    let mysql_pool = sqlx::MySqlPool::connect("mysql://root:test123@127.0.0.1:3306/sz_orm_test")
        .await
        .unwrap();
    let pg_pool = sqlx::PgPool::connect("postgres://postgres:test123@127.0.0.1:5432/sz_orm_test")
        .await
        .unwrap();

    let mysql_resource: Arc<dyn XaResource> = Arc::new(MySqlXaResource {
        pool: mysql_pool,
        id: "mysql".to_string(),
    });
    let pg_resource: Arc<dyn XaResource> = Arc::new(PostgresXaResource {
        pool: pg_pool,
        id: "pg".to_string(),
    });

    let log_store: Arc<dyn TransactionLogStore> = Arc::new(InMemoryTransactionLog::new());
    let coord = XaCoordinator::new(log_store);

    let mut participants = vec![
        XaParticipant::new(mysql_resource, "real-xa-tx"),
        XaParticipant::new(pg_resource, "real-xa-tx"),
    ];

    let result = coord
        .xa_two_phase_commit("real-xa-tx", &mut participants)
        .await;
    assert!(result.is_ok());
}
