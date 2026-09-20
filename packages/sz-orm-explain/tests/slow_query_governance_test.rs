#![cfg(feature = "slow-query-governance")]

use sz_orm_explain::slow_query_governance::*;

#[tokio::test]
async fn e2e_slow_query_governance_missing_index() {
    let config = SlowQueryGovernanceConfig::default();
    let governor = SlowQueryGovernor::new(config);
    let result = governor
        .govern("SELECT * FROM big_table WHERE non_indexed_col = 1")
        .await;
    assert!(result.is_ok());
    let r = result.unwrap();
    assert!(r.result_set_consistent);
}

#[tokio::test]
async fn e2e_slow_query_governance_full_table_scan() {
    let config = SlowQueryGovernanceConfig::default();
    let governor = SlowQueryGovernor::new(config);
    let result = governor.govern("SELECT * FROM huge_table").await;
    assert!(result.is_ok());
    let r = result.unwrap();
    assert!(r.result_set_consistent);
}

#[tokio::test]
async fn e2e_slow_query_governance_n_plus_one() {
    let config = SlowQueryGovernanceConfig::default();
    let governor = SlowQueryGovernor::new(config);
    let result = governor
        .govern("SELECT * FROM orders WHERE user_id = 1")
        .await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn e2e_slow_query_governance_lock_wait() {
    let config = SlowQueryGovernanceConfig::default();
    let governor = SlowQueryGovernor::new(config);
    let result = governor
        .govern("UPDATE accounts SET balance = balance + 100 WHERE id = 1")
        .await;
    assert!(result.is_ok());
}
