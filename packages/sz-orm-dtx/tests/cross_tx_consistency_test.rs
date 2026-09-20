//! v7.7.0 任务 3.6：CrossTxConsistencyVerifier 端到端测试

#![cfg(feature = "cross-tx-consistency")]

use sz_orm_dtx::{ConsistencyLevel, CrossTxConsistencyVerifier, TxProtocol};

#[tokio::test]
async fn e2e_saga_consistency() {
    let verifier = CrossTxConsistencyVerifier::new();
    let tx_ids = vec!["saga_tx_001".to_string(), "saga_tx_002".to_string()];
    let result = verifier.verify(TxProtocol::Saga, &tx_ids).await.unwrap();
    assert_eq!(result.consistency_level, ConsistencyLevel::Eventual);
    assert!(result.verified);
    assert!(result.traceable);
    assert!(result.violations.is_empty());
}

#[tokio::test]
async fn e2e_two_pc_consistency() {
    let verifier = CrossTxConsistencyVerifier::new();
    let tx_ids = vec!["2pc_tx_001".to_string(), "2pc_tx_002".to_string()];
    let result = verifier.verify(TxProtocol::TwoPc, &tx_ids).await.unwrap();
    assert_eq!(result.consistency_level, ConsistencyLevel::Strong);
    assert!(result.verified);
}

#[tokio::test]
async fn e2e_three_pc_consistency() {
    let verifier = CrossTxConsistencyVerifier::new();
    let tx_ids = vec!["3pc_tx_001".to_string()];
    let result = verifier.verify(TxProtocol::ThreePc, &tx_ids).await.unwrap();
    assert_eq!(result.consistency_level, ConsistencyLevel::Strong);
    assert!(result.verified);
}
