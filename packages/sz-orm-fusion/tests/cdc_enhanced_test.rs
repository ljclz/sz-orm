//! v7.7.0 任务 3.6：CDC 增强端到端测试

#![cfg(feature = "cdc-enhanced")]

use sz_orm_fusion::{CdcIncrementalSyncer, CdcResumeCoordinator, CdcSchemaSyncer};

#[tokio::test]
async fn e2e_cdc_incremental_sync() {
    let syncer = CdcIncrementalSyncer::default();
    let result = syncer.sync_incremental().await.unwrap();
    assert!(result.sync_latency_ms <= 500.0);
    assert!(result.data_intact);
    assert!(result.checkpoint_preserved);
}

#[tokio::test]
async fn e2e_cdc_resume() {
    let coordinator = CdcResumeCoordinator::new();
    let checkpoint = sz_orm_queue::cdc::CdcCheckpoint {
        dialect: sz_orm_queue::cdc::DbType::Mysql,
        position: sz_orm_queue::cdc::CheckpointPosition::BinlogGtid("gtid-1".to_string()),
        updated_at: 0,
    };
    let result = coordinator.resume(&checkpoint).await.unwrap();
    assert!(result.resume_time_ms <= 2000.0);
    assert!(result.checkpoint_recovered);
}

#[tokio::test]
async fn e2e_cdc_schema_sync() {
    let syncer = CdcSchemaSyncer::new();
    let result = syncer
        .sync_schema("ALTER TABLE users ADD COLUMN age INT")
        .await
        .unwrap();
    assert!(result.schema_compatible);
}
