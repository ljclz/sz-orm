use sz_orm_rw::*;

#[test]
fn test_load_balance_strategy_variants() {
    assert_eq!(
        LoadBalanceStrategy::RoundRobin,
        LoadBalanceStrategy::RoundRobin
    );
    assert_ne!(LoadBalanceStrategy::RoundRobin, LoadBalanceStrategy::Random);
    assert_ne!(
        LoadBalanceStrategy::LeastConnections,
        LoadBalanceStrategy::WeightedRoundRobin
    );
}

#[test]
fn test_slave_health_variants() {
    assert_eq!(SlaveHealth::Healthy, SlaveHealth::Healthy);
    assert_ne!(SlaveHealth::Healthy, SlaveHealth::Unhealthy);
    assert_ne!(SlaveHealth::Unhealthy, SlaveHealth::Drained);
}

#[test]
fn test_weighted_slave_new() {
    let slave = WeightedSlave::new("127.0.0.1:3306", 5);
    assert_eq!(slave.addr, "127.0.0.1:3306");
    assert_eq!(slave.weight, 5);
    assert_eq!(slave.health, SlaveHealth::Healthy);
}

#[test]
fn test_weighted_slave_min_weight() {
    let slave = WeightedSlave::new("host", 0);
    assert_eq!(slave.weight, 1);
}

#[test]
fn test_weighted_slave_with_health() {
    let slave = WeightedSlave::new("host", 1).with_health(SlaveHealth::Unhealthy);
    assert_eq!(slave.health, SlaveHealth::Unhealthy);
}

#[test]
fn test_weighted_slave_drained() {
    let slave = WeightedSlave::new("host", 1).with_health(SlaveHealth::Drained);
    assert_eq!(slave.health, SlaveHealth::Drained);
}

#[test]
fn test_read_write_router_new() {
    let router = ReadWriteRouter::new("master:3306", vec!["slave1:3306"]);
    assert_eq!(router.master(), "master:3306");
}

#[test]
fn test_read_write_router_slaves() {
    let router = ReadWriteRouter::new("master:3306", vec!["slave1:3306", "slave2:3306"]);
    let slaves = router.slaves();
    assert_eq!(slaves.len(), 2);
    assert_eq!(slaves[0], "slave1:3306");
    assert_eq!(slaves[1], "slave2:3306");
}

#[test]
fn test_read_write_router_slave() {
    let router = ReadWriteRouter::new("master:3306", vec!["slave1:3306", "slave2:3306"]);
    let first = router.slave();
    let second = router.slave();
    let third = router.slave();
    assert!(first == "slave1:3306" || first == "slave2:3306");
    assert!(second == "slave1:3306" || second == "slave2:3306");
    assert!(third == "slave1:3306" || third == "slave2:3306");
}

#[test]
fn test_read_write_router_no_slaves() {
    let router = ReadWriteRouter::new("master:3306", vec![]);
    assert_eq!(router.slaves().len(), 0);
}

#[test]
fn test_read_write_router_single_slave() {
    let router = ReadWriteRouter::new("m", vec!["s1"]);
    assert_eq!(router.slave(), "s1");
    assert_eq!(router.slave(), "s1");
}

#[test]
fn test_latency_snapshot_default() {
    let snapshot = LatencySnapshot::default();
    assert_eq!(snapshot.samples, 0);
}
