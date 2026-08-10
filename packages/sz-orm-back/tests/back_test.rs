use sz_orm_back::*;

#[test]
fn test_drill_scenario_variants() {
    assert_ne!(
        DrillScenario::FullBackupRestore,
        DrillScenario::IncrementalMerge
    );
    assert_ne!(DrillScenario::IncrementalMerge, DrillScenario::CorruptFile);
    assert_ne!(DrillScenario::FullBackupRestore, DrillScenario::CorruptFile);
}

#[test]
fn test_drill_report_fields() {
    let report = DrillReport {
        rto_ms: 5000,
        rpo_ms: 0,
        data_loss_count: 0,
        success: true,
    };
    assert_eq!(report.rto_ms, 5000);
    assert_eq!(report.rpo_ms, 0);
    assert!(report.success);
}

#[test]
fn test_drill_report_failed() {
    let report = DrillReport {
        rto_ms: 0,
        rpo_ms: 1000,
        data_loss_count: 10,
        success: false,
    };
    assert!(!report.success);
    assert_eq!(report.data_loss_count, 10);
}

#[test]
fn test_drill_report_equality() {
    let r1 = DrillReport {
        rto_ms: 100,
        rpo_ms: 0,
        data_loss_count: 0,
        success: true,
    };
    let r2 = DrillReport {
        rto_ms: 100,
        rpo_ms: 0,
        data_loss_count: 0,
        success: true,
    };
    assert_eq!(r1, r2);
}
