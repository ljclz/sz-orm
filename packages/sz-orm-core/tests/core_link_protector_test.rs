#![cfg(feature = "circuit-breaker")]

use sz_orm_core::degradation::*;

#[test]
fn e2e_core_link_protector_core_request() {
    let protector = CoreLinkProtector::new();
    assert!(protector.is_core_link("SELECT * FROM payment WHERE id = 1"));
    assert!(protector.is_core_link("SELECT * FROM orders WHERE order_id = 1"));
    assert!(protector.is_core_link("SELECT * FROM auth_tokens WHERE user_id = 1"));
}

#[test]
fn e2e_core_link_protector_non_core_request() {
    let protector = CoreLinkProtector::new();
    assert!(!protector.is_core_link("SELECT * FROM users WHERE id = 1"));
    assert!(!protector.is_core_link("SELECT * FROM logs WHERE level = 'debug'"));
}

#[test]
fn e2e_degradation_strategy_variants() {
    assert_eq!(format!("{:?}", DegradationStrategy::Cache), "Cache");
    assert_eq!(
        format!("{:?}", DegradationStrategy::DefaultValue),
        "DefaultValue"
    );
    assert_eq!(
        format!("{:?}", DegradationStrategy::SimplifiedResult),
        "SimplifiedResult"
    );
}

#[tokio::test]
async fn e2e_degrade_non_core_request() {
    let protector = CoreLinkProtector::new();
    let result = protector
        .degrade_non_core("SELECT * FROM logs", DegradationStrategy::DefaultValue)
        .await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn e2e_degrade_core_link_rejected() {
    let protector = CoreLinkProtector::new();
    let result = protector
        .degrade_non_core("SELECT * FROM payment", DegradationStrategy::DefaultValue)
        .await;
    assert!(result.is_err());
}
