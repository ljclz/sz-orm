//! # CDC 变更数据捕获
//!
//! 提供 `ChangeEvent`/`CdcConfig`/`CdcCheckpoint` 数据结构，`DialectCapturer` trait，
//! `ExactlyOnceDedup` 去重，`CheckpointManager` 断点续传，`DownstreamSink` 下游分发，
//! `CdcCapturer` 编排捕获 + 去重 + 脱敏 + 分发 + 位点管理。

pub mod capturer;
pub mod checkpoint;
pub mod dedup;
pub mod downstream;
pub mod masking;

use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 变更操作类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChangeOp {
    Insert,
    Update,
    Delete,
}

/// 行数据（字段名 → 值）
pub type Row = HashMap<String, Value>;

/// 变更事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeEvent {
    pub op: ChangeOp,
    pub before: Option<Row>,
    pub after: Option<Row>,
    pub timestamp: u64,
    pub transaction_id: String,
    pub table: String,
    pub schema: String,
}

/// 数据库类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DbType {
    Postgres,
    Mysql,
    Sqlite,
    Oracle,
    Mssql,
}

/// 检查点位置（各方言不同）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckpointPosition {
    /// PostgreSQL WAL LSN
    WalLsn(u64),
    /// MySQL binlog GTID
    BinlogGtid(String),
    /// SQLite 触发器序列号
    TriggerSeq(u64),
    /// Oracle LogMiner SCN
    LogMinerScn(u64),
    /// MSSQL CDC LSN
    CdcLsn(u64),
}

/// CDC 检查点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdcCheckpoint {
    pub dialect: DbType,
    pub position: CheckpointPosition,
    pub updated_at: u64,
}

/// 下游配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DownstreamConfig {
    Kafka {
        topic: String,
    },
    RabbitMq {
        exchange: String,
    },
    Nats {
        subject: String,
    },
    Pulsar {
        topic: String,
    },
    RocketMq {
        topic: String,
    },
    ActiveMq {
        queue: String,
    },
    HttpWebhook {
        url: String,
        headers: HashMap<String, String>,
    },
}

/// 检查点存储配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CheckpointStoreConfig {
    File { path: String },
    Memory,
}

/// 脱敏规则映射（字段名 → 脱敏规则）
pub type MaskingRuleMap = HashMap<String, MaskingRule>;

/// CDC 配置
#[derive(Debug, Clone)]
pub struct CdcConfig {
    pub tables: Vec<String>,
    pub dialect: DbType,
    pub downstream: Vec<DownstreamConfig>,
    pub checkpoint_store: CheckpointStoreConfig,
    pub masking: Option<MaskingRuleMap>,
}

/// CDC 错误
#[derive(Debug, Clone)]
pub enum CdcError {
    WalNotConfigured,
    BinlogNotEnabled,
    DownstreamUnavailable { downstream: String },
    CheckpointFailed,
    CaptureError { reason: String },
}

impl fmt::Display for CdcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CdcError::WalNotConfigured => {
                write!(
                    f,
                    "WAL logical replication not configured (wal_level != logical)"
                )
            }
            CdcError::BinlogNotEnabled => {
                write!(f, "MySQL binlog not enabled (binlog_format != ROW)")
            }
            CdcError::DownstreamUnavailable { downstream } => {
                write!(f, "downstream unavailable: {downstream}")
            }
            CdcError::CheckpointFailed => write!(f, "checkpoint persistence failed"),
            CdcError::CaptureError { reason } => write!(f, "capture error: {reason}"),
        }
    }
}

impl std::error::Error for CdcError {}

pub use sz_orm_masking::MaskingRule;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_change_event_serde() {
        let mut before = HashMap::new();
        before.insert("name".to_string(), Value::String("old".to_string()));
        let mut after = HashMap::new();
        after.insert("name".to_string(), Value::String("new".to_string()));

        let event = ChangeEvent {
            op: ChangeOp::Update,
            before: Some(before),
            after: Some(after),
            timestamp: 1234567890,
            transaction_id: "tx-001".to_string(),
            table: "users".to_string(),
            schema: "public".to_string(),
        };

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: ChangeEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.op, ChangeOp::Update);
        assert_eq!(deserialized.transaction_id, "tx-001");
        assert_eq!(deserialized.table, "users");
    }

    #[test]
    fn test_change_op_serde() {
        let json = serde_json::to_string(&ChangeOp::Insert).unwrap();
        assert_eq!(json, "\"Insert\"");
        let op: ChangeOp = serde_json::from_str(&json).unwrap();
        assert_eq!(op, ChangeOp::Insert);
    }

    #[test]
    fn test_checkpoint_with_wal_lsn() {
        let checkpoint = CdcCheckpoint {
            dialect: DbType::Postgres,
            position: CheckpointPosition::WalLsn(123456),
            updated_at: 1234567890,
        };
        let json = serde_json::to_string(&checkpoint).unwrap();
        let deserialized: CdcCheckpoint = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.dialect, DbType::Postgres);
        assert_eq!(deserialized.position, CheckpointPosition::WalLsn(123456));
    }

    #[test]
    fn test_checkpoint_with_binlog_gtid() {
        let checkpoint = CdcCheckpoint {
            dialect: DbType::Mysql,
            position: CheckpointPosition::BinlogGtid("gtid-001".to_string()),
            updated_at: 1234567890,
        };
        let json = serde_json::to_string(&checkpoint).unwrap();
        let deserialized: CdcCheckpoint = serde_json::from_str(&json).unwrap();
        assert_eq!(
            deserialized.position,
            CheckpointPosition::BinlogGtid("gtid-001".to_string())
        );
    }

    #[test]
    fn test_cdc_error_display() {
        assert!(CdcError::WalNotConfigured.to_string().contains("WAL"));
        assert!(CdcError::BinlogNotEnabled.to_string().contains("binlog"));
        assert!(CdcError::CheckpointFailed
            .to_string()
            .contains("checkpoint"));
    }

    #[test]
    fn test_downstream_config_serde() {
        let config = DownstreamConfig::Kafka {
            topic: "users_cdc".to_string(),
        };
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: DownstreamConfig = serde_json::from_str(&json).unwrap();
        match deserialized {
            DownstreamConfig::Kafka { topic } => assert_eq!(topic, "users_cdc"),
            _ => panic!("expected Kafka"),
        }
    }

    #[test]
    fn test_cdc_config() {
        let config = CdcConfig {
            tables: vec!["users".to_string(), "orders".to_string()],
            dialect: DbType::Postgres,
            downstream: vec![DownstreamConfig::Kafka {
                topic: "cdc_topic".to_string(),
            }],
            checkpoint_store: CheckpointStoreConfig::Memory,
            masking: None,
        };
        assert_eq!(config.tables.len(), 2);
        assert_eq!(config.dialect, DbType::Postgres);
    }
}
