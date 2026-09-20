//! 跨模块联动 e2e 测试：生命周期 + 灰度发布
//!
//! 验证归档迁移与灰度发布不冲突。
//! 运行：cargo test --workspace --features sz-orm-governance/data-lifecycle-mgmt,sz-orm-mig/gray-release --test e2e_lifecycle_gray -- --ignored

#![cfg(feature = "data-lifecycle-mgmt")]

use sz_orm_governance::lifecycle::{
    ArchiveExecutor, DataRange, DataTemperature, LifecycleError, SourceStatus,
};
use sz_orm_mig::gray_release::{GrayReleaseConfig, GrayReleaseOrchestrator, ReleaseStatus};

fn make_cold_range(min_id: i64, max_id: i64) -> DataRange {
    DataRange {
        min_id,
        max_id,
        row_count: (max_id - min_id + 1) as u64,
        temperature: DataTemperature::Cold,
        last_accessed_days_ago: 90,
        access_frequency_per_day: 0.01,
    }
}

#[test]
#[ignore = "跨模块联动 e2e 测试：需 --features data-lifecycle-mgmt + gray-release"]
fn e2e_archive_and_gray_release_coexist() {
    let archive_executor = ArchiveExecutor::new();
    let mut orch = GrayReleaseOrchestrator::new(GrayReleaseConfig::default());

    let progress = orch.start_release("rel-lifecycle-1");
    assert_eq!(progress.status, ReleaseStatus::Running);

    let range = make_cold_range(1, 1000);
    let archive_result = archive_executor.archive("orders", &range, "s3://archive-bucket");
    assert!(archive_result.is_ok(), "归档应成功");
    let record = archive_result.unwrap();
    assert_eq!(record.table, "orders");
    assert_eq!(record.source_status, SourceStatus::Archived);

    let advance = orch.advance(true).unwrap();
    assert_eq!(advance.current_percentage, 20, "灰度发布应正常推进");
    assert_eq!(advance.status, ReleaseStatus::Running);
}

#[test]
#[ignore = "跨模块联动 e2e 测试：需 --features data-lifecycle-mgmt + gray-release"]
fn e2e_batch_archive_during_gray_release() {
    let archive_executor = ArchiveExecutor::new();
    let mut orch = GrayReleaseOrchestrator::new(GrayReleaseConfig::default());
    orch.start_release("rel-lifecycle-2");

    let ranges = vec![
        make_cold_range(1, 500),
        make_cold_range(501, 1000),
        make_cold_range(1001, 1500),
    ];
    let report = archive_executor
        .batch_archive("orders", &ranges, "s3://archive")
        .unwrap();
    assert_eq!(report.total_archived, 1500);
    assert_eq!(report.failed, 0);
    assert_eq!(report.records.len(), 3);

    for _ in 0..9 {
        let progress = orch.advance(true).unwrap();
        if progress.status == ReleaseStatus::Completed {
            return;
        }
    }
    panic!("灰度发布应在归档后仍能完成");
}

#[test]
#[ignore = "跨模块联动 e2e 测试：需 --features data-lifecycle-mgmt + gray-release"]
fn e2e_gray_rollback_preserves_archived_data() {
    let archive_executor = ArchiveExecutor::new();
    let mut orch = GrayReleaseOrchestrator::new(GrayReleaseConfig::default());
    orch.start_release("rel-lifecycle-3");

    let range = make_cold_range(1, 200);
    let archive_record = archive_executor
        .archive("orders", &range, "s3://archive")
        .unwrap();

    let rollback = orch.force_rollback("health check failed");
    assert!(rollback.success);
    assert_eq!(orch.query_progress().status, ReleaseStatus::RolledBack);

    let archives = archive_executor.get_archives();
    assert_eq!(archives.len(), 1, "归档数据应在回滚后保留");
    assert_eq!(archives[0].record_id, archive_record.record_id);
    assert_eq!(archives[0].source_status, SourceStatus::Archived);
}

#[test]
#[ignore = "跨模块联动 e2e 测试：需 --features data-lifecycle-mgmt + gray-release"]
fn e2e_archive_failure_does_not_block_gray_release() {
    let archive_executor = ArchiveExecutor::new();
    let mut orch = GrayReleaseOrchestrator::new(GrayReleaseConfig::default());
    orch.start_release("rel-lifecycle-4");

    let invalid_range = DataRange {
        min_id: 100,
        max_id: 1,
        row_count: 10,
        temperature: DataTemperature::Cold,
        last_accessed_days_ago: 90,
        access_frequency_per_day: 0.01,
    };
    let archive_result = archive_executor.archive("orders", &invalid_range, "s3://archive");
    assert!(matches!(
        archive_result,
        Err(LifecycleError::IntegrityCheckFailed(_))
    ));

    let advance = orch.advance(true).unwrap();
    assert_eq!(
        advance.current_percentage, 20,
        "归档失败不应阻塞灰度发布推进"
    );
    assert_eq!(advance.status, ReleaseStatus::Running);
}

#[test]
#[ignore = "跨模块联动 e2e 测试：需 --features data-lifecycle-mgmt + gray-release"]
fn e2e_gray_pause_resume_with_archive_in_between() {
    let archive_executor = ArchiveExecutor::new();
    let mut orch = GrayReleaseOrchestrator::new(GrayReleaseConfig::default());
    orch.start_release("rel-lifecycle-5");

    orch.advance(true).unwrap();
    orch.pause().unwrap();
    assert_eq!(orch.query_progress().status, ReleaseStatus::Paused);

    let range = make_cold_range(1, 100);
    let _ = archive_executor
        .archive("orders", &range, "s3://archive")
        .unwrap();

    orch.resume().unwrap();
    assert_eq!(orch.query_progress().status, ReleaseStatus::Running);

    let advance = orch.advance(true).unwrap();
    assert!(advance.current_percentage > 20, "恢复后应继续推进");
}
