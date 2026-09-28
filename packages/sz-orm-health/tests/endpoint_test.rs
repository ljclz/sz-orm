use std::time::Duration;
use sz_orm_health::endpoint::HealthEndpointConfig;

#[test]
fn test_health_endpoint_config_new() {
    let config = HealthEndpointConfig::new(
        "/health",
        8080,
        vec!["pool1".to_string()],
        Duration::from_secs(10),
    );
    assert_eq!(config.path, "/health");
    assert_eq!(config.port, 8080);
    assert_eq!(config.resources.len(), 1);
    assert_eq!(config.cache_ttl, Duration::from_secs(10));
}

#[test]
fn test_health_endpoint_config_default_for_port() {
    let config = HealthEndpointConfig::default_for_port(9090);
    assert_eq!(config.path, "/health");
    assert_eq!(config.port, 9090);
    assert!(config.resources.is_empty());
    assert_eq!(config.cache_ttl, Duration::from_secs(5));
}

#[test]
fn test_health_endpoint_config_with_multiple_resources() {
    let config = HealthEndpointConfig::new(
        "/api/health",
        3000,
        vec!["mysql".to_string(), "pg".to_string(), "redis".to_string()],
        Duration::from_millis(500),
    );
    assert_eq!(config.resources.len(), 3);
    assert_eq!(config.cache_ttl, Duration::from_millis(500));
}

#[test]
fn test_health_endpoint_config_empty_resources() {
    let config = HealthEndpointConfig::new("/health", 8080, vec![], Duration::from_secs(5));
    assert!(config.resources.is_empty());
}

#[test]
fn test_health_endpoint_config_clone() {
    let config = HealthEndpointConfig::new("/health", 8080, vec!["pool1".to_string()], Duration::from_secs(5));
    let cloned = config.clone();
    assert_eq!(cloned.path, config.path);
    assert_eq!(cloned.port, config.port);
}