//! 端到端测试：PQC 签名验证 ≤ 10ms 延迟达标

use std::time::Instant;

use sz_orm_crypto::pqc::HybridKeyExchange;

#[tokio::test]
async fn e2e_signature_latency_under_10ms() {
    let kex = HybridKeyExchange::with_defaults();
    let msg = b"test message for signature verification";

    let start = Instant::now();
    let result = kex.verify_signature(msg, msg);
    let elapsed = start.elapsed();

    assert!(result.is_ok());
    assert!(
        elapsed.as_millis() <= 10,
        "签名验证延迟 {}ms 应 ≤ 10ms",
        elapsed.as_millis()
    );
}

#[tokio::test]
async fn e2e_signature_invalid_rejected() {
    let kex = HybridKeyExchange::with_defaults();
    assert!(!kex.verify_signature(b"msg1", b"msg2").unwrap());
}
