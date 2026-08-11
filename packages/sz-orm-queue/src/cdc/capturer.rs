//! DialectCapturer trait + 各方言捕获器

use std::pin::Pin;

use async_trait::async_trait;
use futures::Stream;

use super::{CdcCheckpoint, CdcConfig, CdcError, ChangeEvent, DbType};

/// 方言捕获器 trait
#[async_trait]
pub trait DialectCapturer: Send + Sync {
    /// 启动捕获，返回变更事件流
    async fn start_capture(
        &self,
        checkpoint: Option<CdcCheckpoint>,
    ) -> Result<Pin<Box<dyn Stream<Item = ChangeEvent> + Send>>, CdcError>;

    /// 方言类型
    fn dialect(&self) -> DbType;
}

/// PostgreSQL WAL 逻辑复制捕获器
pub struct WalCapturer {
    config: CdcConfig,
    conn_string: String,
    slot_name: String,
    wal_level: String,
}

impl WalCapturer {
    pub fn new(
        config: CdcConfig,
        conn_string: String,
        slot_name: String,
        wal_level: String,
    ) -> Self {
        Self {
            config,
            conn_string,
            slot_name,
            wal_level,
        }
    }

    pub fn config(&self) -> &CdcConfig {
        &self.config
    }

    pub fn wal_level(&self) -> &str {
        &self.wal_level
    }
}

#[async_trait]
impl DialectCapturer for WalCapturer {
    async fn start_capture(
        &self,
        checkpoint: Option<CdcCheckpoint>,
    ) -> Result<Pin<Box<dyn Stream<Item = ChangeEvent> + Send>>, CdcError> {
        if self.wal_level != "logical" {
            return Err(CdcError::WalNotConfigured);
        }
        let _ = (&self.conn_string, &self.slot_name, checkpoint);
        Err(CdcError::CaptureError {
            reason: "WAL capture requires live PostgreSQL connection".to_string(),
        })
    }

    fn dialect(&self) -> DbType {
        DbType::Postgres
    }
}

/// MySQL binlog 捕获器
pub struct BinlogCapturer {
    config: CdcConfig,
    conn_string: String,
    server_id: u32,
    binlog_enabled: bool,
}

impl BinlogCapturer {
    pub fn new(
        config: CdcConfig,
        conn_string: String,
        server_id: u32,
        binlog_enabled: bool,
    ) -> Self {
        Self {
            config,
            conn_string,
            server_id,
            binlog_enabled,
        }
    }

    pub fn binlog_enabled(&self) -> bool {
        self.binlog_enabled
    }

    pub fn config(&self) -> &CdcConfig {
        &self.config
    }
}

#[async_trait]
impl DialectCapturer for BinlogCapturer {
    async fn start_capture(
        &self,
        checkpoint: Option<CdcCheckpoint>,
    ) -> Result<Pin<Box<dyn Stream<Item = ChangeEvent> + Send>>, CdcError> {
        if !self.binlog_enabled {
            return Err(CdcError::BinlogNotEnabled);
        }
        let _ = (&self.conn_string, self.server_id, checkpoint);
        Err(CdcError::CaptureError {
            reason: "binlog capture requires live MySQL connection".to_string(),
        })
    }

    fn dialect(&self) -> DbType {
        DbType::Mysql
    }
}

/// SQLite 触发器捕获器
pub struct TriggerCapturer {
    config: CdcConfig,
    db_path: String,
}

impl TriggerCapturer {
    pub fn new(config: CdcConfig, db_path: String) -> Self {
        Self { config, db_path }
    }

    pub fn config(&self) -> &CdcConfig {
        &self.config
    }
}

#[async_trait]
impl DialectCapturer for TriggerCapturer {
    async fn start_capture(
        &self,
        _checkpoint: Option<CdcCheckpoint>,
    ) -> Result<Pin<Box<dyn Stream<Item = ChangeEvent> + Send>>, CdcError> {
        let _ = &self.db_path;
        Err(CdcError::CaptureError {
            reason: "trigger capture requires live SQLite connection".to_string(),
        })
    }

    fn dialect(&self) -> DbType {
        DbType::Sqlite
    }
}

/// Oracle LogMiner 捕获器
pub struct LogMinerCapturer {
    config: CdcConfig,
    conn_string: String,
}

impl LogMinerCapturer {
    pub fn new(config: CdcConfig, conn_string: String) -> Self {
        Self {
            config,
            conn_string,
        }
    }

    pub fn config(&self) -> &CdcConfig {
        &self.config
    }
}

#[async_trait]
impl DialectCapturer for LogMinerCapturer {
    async fn start_capture(
        &self,
        _checkpoint: Option<CdcCheckpoint>,
    ) -> Result<Pin<Box<dyn Stream<Item = ChangeEvent> + Send>>, CdcError> {
        let _ = &self.conn_string;
        Err(CdcError::CaptureError {
            reason: "LogMiner capture requires live Oracle connection".to_string(),
        })
    }

    fn dialect(&self) -> DbType {
        DbType::Oracle
    }
}

/// MSSQL CDC 捕获器
pub struct MssqlCdcCapturer {
    config: CdcConfig,
    conn_string: String,
}

impl MssqlCdcCapturer {
    pub fn new(config: CdcConfig, conn_string: String) -> Self {
        Self {
            config,
            conn_string,
        }
    }

    pub fn config(&self) -> &CdcConfig {
        &self.config
    }
}

#[async_trait]
impl DialectCapturer for MssqlCdcCapturer {
    async fn start_capture(
        &self,
        _checkpoint: Option<CdcCheckpoint>,
    ) -> Result<Pin<Box<dyn Stream<Item = ChangeEvent> + Send>>, CdcError> {
        let _ = &self.conn_string;
        Err(CdcError::CaptureError {
            reason: "MSSQL CDC capture requires live MSSQL connection".to_string(),
        })
    }

    fn dialect(&self) -> DbType {
        DbType::Mssql
    }
}

/// 按方言构造捕获器
pub fn create_capturer(
    config: CdcConfig,
    conn_string: &str,
) -> Result<Box<dyn DialectCapturer>, CdcError> {
    match config.dialect {
        DbType::Postgres => Ok(Box::new(WalCapturer::new(
            config,
            conn_string.to_string(),
            "sz_orm_cdc_slot".to_string(),
            "logical".to_string(),
        ))),
        DbType::Mysql => Ok(Box::new(BinlogCapturer::new(
            config,
            conn_string.to_string(),
            1001,
            true,
        ))),
        DbType::Sqlite => Ok(Box::new(TriggerCapturer::new(
            config,
            conn_string.to_string(),
        ))),
        DbType::Oracle => Ok(Box::new(LogMinerCapturer::new(
            config,
            conn_string.to_string(),
        ))),
        DbType::Mssql => Ok(Box::new(MssqlCdcCapturer::new(
            config,
            conn_string.to_string(),
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cdc::{CheckpointStoreConfig, DownstreamConfig};

    fn test_config(dialect: DbType) -> CdcConfig {
        CdcConfig {
            tables: vec!["users".to_string()],
            dialect,
            downstream: vec![DownstreamConfig::Kafka {
                topic: "cdc".to_string(),
            }],
            checkpoint_store: CheckpointStoreConfig::Memory,
            masking: None,
        }
    }

    #[test]
    fn test_wal_capturer_dialect() {
        let capturer = WalCapturer::new(
            test_config(DbType::Postgres),
            "host=localhost".to_string(),
            "slot1".to_string(),
            "logical".to_string(),
        );
        assert_eq!(capturer.dialect(), DbType::Postgres);
    }

    #[test]
    fn test_binlog_capturer_dialect() {
        let capturer = BinlogCapturer::new(
            test_config(DbType::Mysql),
            "host=localhost".to_string(),
            1001,
            true,
        );
        assert_eq!(capturer.dialect(), DbType::Mysql);
    }

    #[test]
    fn test_trigger_capturer_dialect() {
        let capturer = TriggerCapturer::new(test_config(DbType::Sqlite), "test.db".to_string());
        assert_eq!(capturer.dialect(), DbType::Sqlite);
    }

    #[test]
    fn test_logminer_capturer_dialect() {
        let capturer =
            LogMinerCapturer::new(test_config(DbType::Oracle), "oracle_conn".to_string());
        assert_eq!(capturer.dialect(), DbType::Oracle);
    }

    #[test]
    fn test_mssql_cdc_capturer_dialect() {
        let capturer = MssqlCdcCapturer::new(test_config(DbType::Mssql), "mssql_conn".to_string());
        assert_eq!(capturer.dialect(), DbType::Mssql);
    }

    #[tokio::test]
    async fn test_wal_not_configured_error() {
        let capturer = WalCapturer::new(
            test_config(DbType::Postgres),
            "host=localhost".to_string(),
            "slot1".to_string(),
            "replica".to_string(),
        );
        let result = capturer.start_capture(None).await;
        assert!(matches!(result, Err(CdcError::WalNotConfigured)));
    }

    #[tokio::test]
    async fn test_binlog_not_enabled_error() {
        let capturer = BinlogCapturer::new(
            test_config(DbType::Mysql),
            "host=localhost".to_string(),
            1001,
            false,
        );
        let result = capturer.start_capture(None).await;
        assert!(matches!(result, Err(CdcError::BinlogNotEnabled)));
    }

    #[test]
    fn test_create_capturer_postgres() {
        let capturer = create_capturer(test_config(DbType::Postgres), "conn").unwrap();
        assert_eq!(capturer.dialect(), DbType::Postgres);
    }

    #[test]
    fn test_create_capturer_all_dialects() {
        for dialect in [
            DbType::Postgres,
            DbType::Mysql,
            DbType::Sqlite,
            DbType::Oracle,
            DbType::Mssql,
        ] {
            let capturer = create_capturer(test_config(dialect), "conn").unwrap();
            assert_eq!(capturer.dialect(), dialect);
        }
    }
}
