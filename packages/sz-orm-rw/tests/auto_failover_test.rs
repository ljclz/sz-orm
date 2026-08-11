//! M6 集成测试：自动 failover 全流程 + 脑裂检测 + 与既有 ReadWriteRouter 兼容性

use std::time::Duration;

use sz_orm_rw::auto_failover::{
    AutoFailoverManager, DemotionStrategy, FailoverConfig, FailoverOperator, SplitBrainStatus,
};

#[test]
fn test_full_failover_workflow() {
    let config = FailoverConfig {
        check_interval: Duration::from_millis(100),
        failure_threshold: 3,
        lag_threshold: Duration::from_secs(1),
        switch_timeout: Duration::from_secs(5),
    };
    let manager = AutoFailoverManager::new(config, "master".to_string());
    manager.add_slave("slave1", Duration::from_millis(50), 10000);
    manager.add_slave("slave2", Duration::from_millis(100), 8000);
    manager.add_slave("slave3", Duration::from_millis(200), 5000);

    assert!(manager.record_master_failure().is_none());
    assert!(manager.record_master_failure().is_none());
    assert_eq!(manager.current_master(), "master");

    let result = manager.record_master_failure().unwrap();
    assert_eq!(result.new_master, "slave1");
    assert_eq!(manager.current_master(), "slave1");
    assert_eq!(result.event.old_master, "master");
    assert_eq!(result.event.promoted_slave, "slave1");
    assert_eq!(result.event.operator, FailoverOperator::Auto);
    assert!(result.event.data_loss_assessment.is_safe);
}

#[test]
fn test_failover_with_unhealthy_slaves_skipped() {
    let manager = AutoFailoverManager::new(FailoverConfig::default(), "master".to_string());
    manager.add_slave("slave1", Duration::from_millis(50), 1000);
    manager.add_slave("slave2", Duration::from_millis(100), 1000);

    manager.update_slave_health("slave1", false, Duration::from_millis(50));

    let result = manager.manual_failover().unwrap();
    assert_eq!(result.new_master, "slave2");
}

#[test]
fn test_failover_no_healthy_slave_returns_error() {
    let manager = AutoFailoverManager::new(FailoverConfig::default(), "master".to_string());
    manager.add_slave("slave1", Duration::from_millis(50), 1000);
    manager.update_slave_health("slave1", false, Duration::from_millis(50));

    assert!(manager.manual_failover().is_err());
}

#[test]
fn test_split_brain_full_scenario() {
    let manager = AutoFailoverManager::new(FailoverConfig::default(), "master".to_string());
    manager.add_slave("slave1", Duration::from_millis(100), 1000);

    manager.manual_failover().unwrap();
    assert_eq!(manager.current_master(), "slave1");

    let status = manager.check_split_brain("master", true);
    assert!(matches!(status, SplitBrainStatus::Detected { .. }));

    let alert = manager
        .resolve_split_brain("master", DemotionStrategy::DemoteToSlave)
        .unwrap();
    assert_eq!(alert.old_master, "master");
    assert_eq!(alert.new_master, "slave1");
    assert!(alert.message.contains("demoted to slave"));

    assert_eq!(manager.split_brain_status(), SplitBrainStatus::Resolved);

    let status_after = manager.check_split_brain("master", true);
    assert_eq!(status_after, SplitBrainStatus::NoSplitBrain);
}

#[test]
fn test_split_brain_isolate_strategy() {
    let manager = AutoFailoverManager::new(FailoverConfig::default(), "master".to_string());
    manager.add_slave("slave1", Duration::from_millis(100), 1000);

    manager.manual_failover().unwrap();

    let alert = manager
        .resolve_split_brain("master", DemotionStrategy::Isolate)
        .unwrap();
    assert!(alert.message.contains("isolated"));
}

#[test]
fn test_periodic_split_brain_check_multiple_old_masters() {
    let manager = AutoFailoverManager::new(FailoverConfig::default(), "master".to_string());
    manager.add_slave("slave1", Duration::from_millis(100), 1000);

    manager.manual_failover().unwrap();
    assert_eq!(manager.current_master(), "slave1");

    manager.manual_failover().unwrap();
    assert_eq!(manager.current_master(), "slave1");

    let results = manager.periodic_split_brain_check(|name| name == "master");
    assert!(!results.is_empty());
}

#[test]
fn test_failover_events_history() {
    let manager = AutoFailoverManager::new(FailoverConfig::default(), "master".to_string());
    manager.add_slave("slave1", Duration::from_millis(100), 1000);

    manager.manual_failover().unwrap();
    manager.add_slave("slave2", Duration::from_millis(50), 1000);
    manager.manual_failover().unwrap();

    let events = manager.events();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].promoted_slave, "slave1");
    assert_eq!(events[1].promoted_slave, "slave2");
}

#[test]
fn test_data_loss_assessment_high_lag() {
    let manager = AutoFailoverManager::new(FailoverConfig::default(), "master".to_string());
    manager.add_slave("slave1", Duration::from_secs(10), 10000);

    let result = manager.manual_failover();
    assert!(result.is_err());
}

#[test]
fn test_auto_failover_resets_after_success() {
    let manager = AutoFailoverManager::new(FailoverConfig::default(), "master".to_string());
    manager.add_slave("slave1", Duration::from_millis(100), 1000);

    manager.record_master_failure();
    manager.record_master_failure();
    manager.record_master_success();
    manager.record_master_failure();
    manager.record_master_failure();

    assert_eq!(manager.current_master(), "master");

    let result = manager.record_master_failure();
    assert!(result.is_some());
    assert_eq!(manager.current_master(), "slave1");
}

#[test]
fn test_config_access() {
    let config = FailoverConfig {
        check_interval: Duration::from_secs(2),
        failure_threshold: 5,
        lag_threshold: Duration::from_secs(3),
        switch_timeout: Duration::from_secs(10),
    };
    let manager = AutoFailoverManager::new(config, "master".to_string());

    assert_eq!(manager.config().failure_threshold, 5);
    assert_eq!(manager.config().lag_threshold, Duration::from_secs(3));
}
