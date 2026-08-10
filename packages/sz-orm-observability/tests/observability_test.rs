use sz_orm_observability::*;

#[test]
fn test_metrics_registry_new() {
    let registry = MetricsRegistry::new();
    let _output = registry.render();
}

#[test]
fn test_counter_inc() {
    let registry = MetricsRegistry::new();
    let counter = registry.register_counter("test_counter", "A test counter");
    counter.inc();
    counter.inc();
    counter.inc();
    let output = registry.render();
    assert!(output.contains("test_counter"));
}

#[test]
fn test_gauge_set() {
    let registry = MetricsRegistry::new();
    let gauge = registry.register_gauge("test_gauge", "A test gauge");
    gauge.set(42.0);
    let output = registry.render();
    assert!(output.contains("test_gauge"));
}

#[test]
fn test_histogram_observe() {
    let registry = MetricsRegistry::new();
    let histogram =
        registry.register_histogram("test_histogram", "A test histogram", vec![0.1, 1.0, 10.0]);
    histogram.observe(0.5);
    histogram.observe(5.0);
    let output = registry.render();
    assert!(output.contains("test_histogram"));
}

#[test]
fn test_counter_inc_by() {
    let registry = MetricsRegistry::new();
    let counter = registry.register_counter("add_counter", "test");
    counter.inc_by(10.0);
    let output = registry.render();
    assert!(output.contains("add_counter"));
}

#[test]
fn test_gauge_inc_dec() {
    let registry = MetricsRegistry::new();
    let gauge = registry.register_gauge("inc_gauge", "test");
    gauge.inc();
    gauge.dec_by(1.0);
    let output = registry.render();
    assert!(output.contains("inc_gauge"));
}

#[test]
fn test_multiple_metrics() {
    let registry = MetricsRegistry::new();
    let c1 = registry.register_counter("counter1", "c1");
    let c2 = registry.register_counter("counter2", "c2");
    c1.inc();
    c2.inc();
    let output = registry.render();
    assert!(output.contains("counter1"));
    assert!(output.contains("counter2"));
}

#[test]
fn test_slo_config_default() {
    let config = SloConfig::default();
    let _monitor = SloMonitor::new(config);
}
