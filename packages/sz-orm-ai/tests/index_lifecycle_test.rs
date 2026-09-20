//! v7.7.0 任务 2.7：IndexLifecycleManager 端到端测试
//!
//! 验证索引全生命周期管理的完整流程：
//! - 推荐索引（基于负载模型）
//! - 创建索引（DDL 生成）
//! - 维护索引（监控使用率/重建/碎片整理）
//! - 淘汰索引（低使用率 + 备份 + 可恢复）
//! - 决策延迟 ≤ 500ms
//! - 可解释性（decision_basis 非空）

#![cfg(feature = "ai-index-lifecycle")]

use std::time::Duration;

use sz_orm_ai::{
    AiError, IndexEvictor, IndexLifecycleManager, IndexMaintainer, QueryPattern, WorkloadModel,
};

#[tokio::test]
async fn e2e_index_lifecycle_manage() {
    let manager = IndexLifecycleManager::new();
    let mut table_stats = std::collections::HashMap::new();
    table_stats.insert(
        "users".to_string(),
        sz_orm_ai::TableStats {
            row_count: 100000,
            existing_indexes: vec![],
        },
    );
    let workload = WorkloadModel {
        patterns: vec![QueryPattern {
            sql_template: "SELECT * FROM users WHERE email = ?".to_string(),
            frequency: 100,
            columns_accessed: vec!["email".to_string()],
        }],
        table_stats,
    };
    let result = manager.manage_lifecycle(&workload).await.unwrap();
    assert!(result.decision_latency_ms <= 500.0);
    assert!(!result.decision_basis.is_empty());
}

#[tokio::test]
async fn e2e_index_maintainer_rebuild() {
    let maintainer = IndexMaintainer::default();
    assert!(maintainer.rebuild("idx_test").await.is_ok());
    assert!(maintainer.rebuild("").await.is_err());
}

#[tokio::test]
async fn e2e_index_maintainer_defragment() {
    let maintainer = IndexMaintainer::default();
    assert!(maintainer.defragment("idx_test").await.is_ok());
    let result = maintainer
        .defragment_with_stats("idx_test", 0.4)
        .await
        .unwrap();
    assert!(result.fragmentation_after < result.fragmentation_before);
}

#[tokio::test]
async fn e2e_index_evictor_evict() {
    let evictor = IndexEvictor::default();
    let result = evictor
        .evict("idx_low_usage", Duration::from_secs(86400))
        .await
        .unwrap();
    assert!(result.usage_rate < 0.05);
    assert!(!result.definition_backup.is_empty());
    assert!(result.recoverable);
    assert!(result.decision_latency_ms <= 500.0);
}

#[tokio::test]
async fn e2e_index_evictor_premature_rejection() {
    let evictor = IndexEvictor::default();
    let result = evictor
        .evict_with_usage("idx_test", 0.1, "CREATE INDEX idx_test ON users (email)")
        .await;
    assert!(result.is_err());
    assert!(matches!(result, Err(AiError::NotSupported(_))));
}
