//! 端到端接线测试：PQC 迁移执行 + 密钥轮换完整链路
//!
//! 验证生产入口可达：评估 → 执行 → 性能追踪 → 算法切换 → 密钥轮换。

use std::collections::HashMap;

use sz_orm_crypto::pqc::{
    CryptoScenario, KeyRotationManager, PerformanceBaselineTracker, PqcAlgorithm, PqcExecError,
    PqcMigrationAssessor, PqcMigrationExecutor,
};

#[tokio::test]
async fn e2e_pqc_migration_full_pipeline() {
    let assessor = PqcMigrationAssessor::new();
    let scenarios = vec![
        CryptoScenario {
            name: "tls_handshake".to_string(),
            current_algo: "RSA-2048".to_string(),
            pqc_recommended: PqcAlgorithm::MlKem768,
            compatible: true,
        },
        CryptoScenario {
            name: "document_signing".to_string(),
            current_algo: "ECDSA-P256".to_string(),
            pqc_recommended: PqcAlgorithm::MlDsa65,
            compatible: true,
        },
        CryptoScenario {
            name: "legacy_blockchain".to_string(),
            current_algo: "RSA-1024".to_string(),
            pqc_recommended: PqcAlgorithm::SlhDsa128s,
            compatible: false,
        },
    ];
    let report = assessor.assess(&scenarios).unwrap();
    assert_eq!(report.scan_status, "SCAN_COMPLETE");

    let executor = PqcMigrationExecutor::new("e2e-asm-001");
    let results = executor.execute(&report).unwrap();
    assert_eq!(results.len(), 3);
    let failed_count = results.iter().filter(|r| !r.success).count();
    assert_eq!(failed_count, 1);
    let progress = executor.progress();
    assert_eq!(progress.total_scenes, 3);
    assert_eq!(progress.completed_count, 2);
    assert_eq!(progress.failed_count, 1);
    assert!(progress.finished);

    let checkpoint = executor.checkpoint();
    assert_eq!(checkpoint.completed_scenes.len(), 2);
    assert_eq!(checkpoint.failed_scenes.len(), 1);
    assert_eq!(checkpoint.failed_scenes[0], "legacy_blockchain");

    let tracker = PerformanceBaselineTracker::new();
    tracker.set_baseline(sz_orm_crypto::pqc::SceneMetrics {
        scene_name: "tls_handshake".to_string(),
        latency_micros: 1000,
        throughput_ops: 10000,
        memory_kib: 256,
    });
    let alert = tracker
        .track(sz_orm_crypto::pqc::SceneMetrics {
            scene_name: "tls_handshake".to_string(),
            latency_micros: 2500,
            throughput_ops: 4000,
            memory_kib: 512,
        })
        .unwrap();
    assert!(alert.is_some());
    let alert_text = alert.unwrap();
    assert!(alert_text.contains("PQC_PERFORMANCE_REGRESSION"));
    let perf_report = tracker.report().unwrap();
    assert_eq!(perf_report.regression_count, 1);

    let switcher = sz_orm_crypto::pqc::AlgorithmAgilitySwitcher::new(PqcAlgorithm::MlKem768);
    let switched = switcher.switch(PqcAlgorithm::MlKem1024).unwrap();
    assert_eq!(switched, PqcAlgorithm::MlKem1024);
    let logs = switcher.audit_logs();
    assert_eq!(logs.len(), 1);
    assert!(logs[0].success);
}

#[tokio::test]
async fn e2e_key_rotation_with_pause_resume() {
    let manager = KeyRotationManager::new("e2e-key-v1");
    let record = manager.rotate("e2e-key-v2").unwrap();
    assert_eq!(record.old_key_id, "e2e-key-v1");
    assert_eq!(record.new_key_id, "e2e-key-v2");
    assert!(record.overlap_until - record.rotated_at >= 86_400);

    let status = manager.status();
    assert_eq!(status.active_key_id, "e2e-key-v2");
    assert!(status.overlap_active);
    assert!(status.previous_key_id.is_some());

    manager.mark_interrupted();
    let err = manager.rotate("e2e-key-v3").unwrap_err();
    assert!(matches!(err, PqcExecError::KeyRotationInterrupted(_)));
    let status_after_interrupt = manager.status();
    assert_eq!(status_after_interrupt.active_key_id, "e2e-key-v2");
    assert_eq!(status_after_interrupt.rotation_count, 1);

    manager.clear_interrupted();
    let record2 = manager.rotate("e2e-key-v3").unwrap();
    assert_eq!(record2.new_key_id, "e2e-key-v3");
    assert_eq!(record2.old_key_id, "e2e-key-v2");

    manager.complete_overlap().unwrap();
    let final_status = manager.status();
    assert!(!final_status.overlap_active);
    assert!(final_status.previous_key_id.is_none());
    assert_eq!(final_status.rotation_count, 2);

    let records = manager.records();
    assert_eq!(records.len(), 2);
    assert!(!records[0].completed);
    assert!(records[1].completed);

    let executor = PqcMigrationExecutor::new("e2e-asm-002");
    executor.pause().unwrap();
    let assessor = PqcMigrationAssessor::new();
    let scenarios = vec![CryptoScenario {
        name: "paused_scene".to_string(),
        current_algo: "RSA-2048".to_string(),
        pqc_recommended: PqcAlgorithm::MlKem768,
        compatible: true,
    }];
    let report = assessor.assess(&scenarios).unwrap();
    let results = executor.execute(&report).unwrap();
    assert!(results.is_empty());
    let progress = executor.progress();
    assert!(progress.paused);
    assert!(!progress.finished);

    executor.resume().unwrap();
    let results2 = executor.execute(&report).unwrap();
    assert_eq!(results2.len(), 1);
    let progress2 = executor.progress();
    assert!(!progress2.paused);
    assert!(progress2.finished);

    let _ = HashMap::<String, String>::new();
}
