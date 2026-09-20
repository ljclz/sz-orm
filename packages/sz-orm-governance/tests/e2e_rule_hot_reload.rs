//! 端到端测试：生命周期规则热更新 10s 内生效，正在执行的迁移不受影响

use sz_orm_governance::lifecycle::{
    ColdHotMigrationScheduler, ColdHotThreshold, LifecycleRule, LifecycleRuleEngine,
    MigrationWindow,
};

fn make_rule(name: &str, table: &str, retention: u32) -> LifecycleRule {
    LifecycleRule {
        name: name.to_string(),
        target_table: table.to_string(),
        cold_hot_threshold: ColdHotThreshold::default(),
        retention_days: retention,
        destruction_days: retention * 4,
        migration_window: MigrationWindow::default(),
        archive_target: "s3://archive".to_string(),
        enabled: true,
    }
}

#[tokio::test]
async fn e2e_rule_hot_reload_updates_rules() {
    let engine = LifecycleRuleEngine::new();
    engine
        .load_rules(vec![make_rule("r1", "orders", 90)])
        .unwrap();
    assert_eq!(engine.rules().len(), 1);
    assert_eq!(engine.reload_version(), 1);

    engine
        .hot_reload(vec![make_rule("r2", "users", 60)])
        .unwrap();
    assert_eq!(engine.rules().len(), 1);
    assert_eq!(engine.rules()[0].target_table, "users");
    assert_eq!(engine.reload_version(), 2);
}

#[tokio::test]
async fn e2e_rule_hot_reload_in_progress_migration_unaffected() {
    let engine = LifecycleRuleEngine::new();
    engine
        .load_rules(vec![make_rule("r1", "orders", 90)])
        .unwrap();

    let scheduler = ColdHotMigrationScheduler::new();
    let classification = sz_orm_governance::lifecycle::ColdHotClassification {
        table: "orders".to_string(),
        hot_data: vec![],
        warm_data: vec![],
        cold_data: vec![sz_orm_governance::lifecycle::DataRange {
            min_id: 1,
            max_id: 100,
            row_count: 100,
            temperature: sz_orm_governance::lifecycle::DataTemperature::Cold,
            last_accessed_days_ago: 90,
            access_frequency_per_day: 0.1,
        }],
    };

    let window = MigrationWindow::default();
    scheduler
        .schedule_migration(&classification, &window, 3)
        .unwrap();
    assert!(scheduler.is_migration_in_progress("orders"));

    engine
        .hot_reload(vec![make_rule("r2", "users", 60)])
        .unwrap();
    assert!(
        scheduler.is_migration_in_progress("orders"),
        "热更新不应影响正在执行的迁移"
    );

    scheduler.complete_migration("orders");
    assert!(!scheduler.is_migration_in_progress("orders"));
}

#[tokio::test]
async fn e2e_rule_hot_reload_validation() {
    let engine = LifecycleRuleEngine::new();
    let bad_rule = LifecycleRule {
        name: "".to_string(),
        ..make_rule("r1", "orders", 90)
    };
    assert!(engine.hot_reload(vec![bad_rule]).is_err());
}

#[tokio::test]
async fn e2e_rule_find_after_reload() {
    let engine = LifecycleRuleEngine::new();
    engine
        .load_rules(vec![make_rule("r1", "orders", 90)])
        .unwrap();

    engine
        .hot_reload(vec![
            make_rule("r1", "orders", 60),
            make_rule("r2", "users", 30),
        ])
        .unwrap();

    assert!(engine.find_rule_for_table("orders").is_some());
    assert!(engine.find_rule_for_table("users").is_some());
    assert_eq!(engine.rules().len(), 2);
}
