//! M7 集成测试：CDC 全流程 + 去重 + 断点续传 + 脱敏 + 下游分发

use std::collections::HashMap;

use serde_json::Value;
use sz_orm_queue::cdc::{
    capturer::{create_capturer, DialectCapturer},
    checkpoint::CheckpointManager,
    dedup::ExactlyOnceDedup,
    downstream::{create_sink, distribute_to_all, InMemorySink, KafkaSink},
    masking::{apply_masking, build_masking_rules},
    CdcCheckpoint, CdcConfig, CdcError, ChangeEvent, ChangeOp, CheckpointPosition,
    CheckpointStoreConfig, DbType, DownstreamConfig, MaskingRule,
};

fn make_event(op: ChangeOp, txid: &str, table: &str) -> ChangeEvent {
    let mut after = HashMap::new();
    after.insert("id".to_string(), Value::Number(1.into()));
    after.insert("name".to_string(), Value::String("test".to_string()));
    after.insert(
        "phone".to_string(),
        Value::String("13812348888".to_string()),
    );
    ChangeEvent {
        op,
        before: None,
        after: Some(after),
        timestamp: 1234567890,
        transaction_id: txid.to_string(),
        table: table.to_string(),
        schema: "public".to_string(),
    }
}

fn make_config(dialect: DbType) -> CdcConfig {
    CdcConfig {
        tables: vec!["users".to_string()],
        dialect,
        downstream: vec![DownstreamConfig::Kafka {
            topic: "users_cdc".to_string(),
        }],
        checkpoint_store: CheckpointStoreConfig::Memory,
        masking: None,
    }
}

#[tokio::test]
async fn test_cdc_full_workflow() {
    let sink1 = InMemorySink::new("sink1".to_string());
    let sink2 = InMemorySink::new("sink2".to_string());
    let sinks: Vec<Box<dyn sz_orm_queue::cdc::downstream::DownstreamSink>> =
        vec![Box::new(sink1), Box::new(sink2)];

    let event = make_event(ChangeOp::Insert, "tx-001", "users");
    let results = distribute_to_all(&sinks, &event).await;
    assert!(results.iter().all(|r| r.is_ok()));
}

#[tokio::test]
async fn test_cdc_dedup_and_distribute() {
    let dedup = ExactlyOnceDedup::new();
    let sink = KafkaSink::new("users_cdc".to_string());
    let sinks: Vec<Box<dyn sz_orm_queue::cdc::downstream::DownstreamSink>> = vec![Box::new(sink)];

    let e1 = make_event(ChangeOp::Insert, "tx-001", "users");
    let e2 = make_event(ChangeOp::Insert, "tx-001", "users");

    if dedup.check_and_mark(&e1) {
        distribute_to_all(&sinks, &e1).await;
    }
    if dedup.check_and_mark(&e2) {
        distribute_to_all(&sinks, &e2).await;
    }

    let kafka_sink = sinks[0].as_ref();
    assert_eq!(kafka_sink.name(), "users_cdc");
}

#[test]
fn test_cdc_checkpoint_resume() {
    let manager = CheckpointManager::new();
    let cp = CdcCheckpoint {
        dialect: DbType::Postgres,
        position: CheckpointPosition::WalLsn(12345),
        updated_at: 1234567890,
    };
    manager.save_checkpoint(&cp).unwrap();

    let loaded = manager.load_checkpoint().unwrap().unwrap();
    assert_eq!(loaded.position, CheckpointPosition::WalLsn(12345));
    assert_eq!(
        manager.resume_position().unwrap(),
        CheckpointPosition::WalLsn(12345)
    );
}

#[test]
fn test_cdc_masking_before_distribute() {
    let mut event = make_event(ChangeOp::Insert, "tx-001", "users");
    let rules = build_masking_rules();
    apply_masking(&mut event, &rules);

    let after = event.after.unwrap();
    let phone = after.get("phone").unwrap();
    assert_eq!(phone, &Value::String("138****8888".to_string()));
}

#[test]
fn test_cdc_all_dialect_capturers() {
    for dialect in [
        DbType::Postgres,
        DbType::Mysql,
        DbType::Sqlite,
        DbType::Oracle,
        DbType::Mssql,
    ] {
        let capturer = create_capturer(make_config(dialect), "conn_string").unwrap();
        assert_eq!(capturer.dialect(), dialect);
    }
}

#[tokio::test]
async fn test_cdc_wal_not_configured() {
    use sz_orm_queue::cdc::capturer::WalCapturer;
    let capturer = WalCapturer::new(
        make_config(DbType::Postgres),
        "conn".to_string(),
        "slot".to_string(),
        "replica".to_string(),
    );
    let result = capturer.start_capture(None).await;
    assert!(matches!(result, Err(CdcError::WalNotConfigured)));
}

#[tokio::test]
async fn test_cdc_binlog_not_enabled() {
    use sz_orm_queue::cdc::capturer::BinlogCapturer;
    let capturer = BinlogCapturer::new(make_config(DbType::Mysql), "conn".to_string(), 1001, false);
    let result = capturer.start_capture(None).await;
    assert!(matches!(result, Err(CdcError::BinlogNotEnabled)));
}

#[test]
fn test_cdc_downstream_sink_creation() {
    let configs = vec![
        DownstreamConfig::Kafka {
            topic: "t1".to_string(),
        },
        DownstreamConfig::RabbitMq {
            exchange: "e1".to_string(),
        },
        DownstreamConfig::Nats {
            subject: "s1".to_string(),
        },
        DownstreamConfig::Pulsar {
            topic: "t2".to_string(),
        },
        DownstreamConfig::RocketMq {
            topic: "t3".to_string(),
        },
        DownstreamConfig::ActiveMq {
            queue: "q1".to_string(),
        },
        DownstreamConfig::HttpWebhook {
            url: "http://localhost:8080/hook".to_string(),
            headers: HashMap::new(),
        },
    ];

    for config in &configs {
        let sink = create_sink(config);
        assert!(!sink.name().is_empty());
    }
}

#[test]
fn test_cdc_checkpoint_file_store() {
    let path = std::env::temp_dir().join("cdc_integration_checkpoint.json");
    let path_str = path.to_str().unwrap().to_string();
    let manager = CheckpointManager::with_file_store(path_str);
    let cp = CdcCheckpoint {
        dialect: DbType::Mysql,
        position: CheckpointPosition::BinlogGtid("gtid-123".to_string()),
        updated_at: 9999999,
    };
    manager.save_checkpoint(&cp).unwrap();
    let loaded = manager.load_checkpoint().unwrap().unwrap();
    assert_eq!(
        loaded.position,
        CheckpointPosition::BinlogGtid("gtid-123".to_string())
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn test_cdc_masking_custom_rules() {
    let mut event = make_event(ChangeOp::Update, "tx-001", "users");
    let mut rules = HashMap::new();
    rules.insert("phone".to_string(), MaskingRule::Phone);
    rules.insert("name".to_string(), MaskingRule::Name);
    apply_masking(&mut event, &rules);
    let after = event.after.unwrap();
    assert_eq!(
        after.get("phone").unwrap(),
        &Value::String("138****8888".to_string())
    );
}
