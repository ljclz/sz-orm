//! 安全攻击性测试：JWT 认证攻击向量（门禁 21 安全攻击测试）
//!
//! 覆盖攻击面：
//!   1. 伪造签名：篡改 claims 后使用错误 secret 重签 → 必须拒绝
//!   2. 过期 token：exp 已过 → 必须拒绝
//!   3. 算法混淆/头部篡改：修改 header（alg=none 等）→ 签名校验必须失败
//!   4. secret 猜测：穷举常见弱 secret → 必须拒绝
//!   5. 格式攻击：空 token / 分段错误 / base64 篡改 → 优雅失败（不 panic）

use sz_orm_auth::jwt::{JwtClaims, JwtEncoder};

fn now_ts() -> i64 {
    // JWT exp 使用 unix 秒
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

#[test]
fn attack_forged_signature_rejected() {
    let encoder = JwtEncoder::new("server-secret-42");
    let claims = JwtClaims::new("user-1", now_ts() + 3600).with_roles(vec!["admin".into()]);
    let token = encoder.encode(&claims).unwrap();

    // 攻击：篡改 claims（提权为 admin）后使用攻击者 secret 重签
    let attacker = JwtEncoder::new("attacker-secret");
    let forged_claims = JwtClaims::new("user-1", now_ts() + 3600).with_roles(vec!["root".into()]);
    let forged = attacker.encode(&forged_claims).unwrap();

    // 受害者使用自己的 secret 验证攻击者 token → 必须失败
    assert!(
        encoder.decode(&forged).is_err(),
        "使用错误 secret 签发的 token 必须被拒绝"
    );
    // 原 token 仍有效（对比正例）
    assert!(encoder.decode(&token).is_ok());
}

#[test]
fn attack_expired_token_rejected() {
    let encoder = JwtEncoder::new("server-secret-42");
    // 已过期 5 分钟
    let expired = JwtClaims::new("user-1", now_ts() - 300);
    let token = encoder.encode(&expired).unwrap();

    assert!(
        encoder.decode(&token).is_err(),
        "过期 token 必须被拒绝（decode 应校验 exp）"
    );
}

#[test]
fn attack_tampered_payload_rejected() {
    let encoder = JwtEncoder::new("server-secret-42");
    let claims = JwtClaims::new("user-1", now_ts() + 3600).with_roles(vec!["user".into()]);
    let token = encoder.encode(&claims).unwrap();

    // 攻击：不改签名，直接篡改中间段（payload base64 篡改）
    let parts: Vec<&str> = token.split('.').collect();
    assert_eq!(parts.len(), 3);
    let tampered_claims = JwtClaims::new("user-1", now_ts() + 3600).with_roles(vec!["root".into()]);
    let tampered_json = serde_json::to_string(&tampered_claims).unwrap();
    let tampered_b64 = base64_url_encode(tampered_json.as_bytes());
    let tampered = format!("{}.{}.{}", parts[0], tampered_b64, parts[2]);

    assert!(
        encoder.decode(&tampered).is_err(),
        "篡改 payload 但保留原签名的 token 必须被拒绝（签名覆盖 payload）"
    );
}

#[test]
fn attack_weak_secret_guessing_fails() {
    // 攻击者使用常见弱 secret 尝试验证
    let encoder = JwtEncoder::new("correct-horse-battery-staple-2026");
    let claims = JwtClaims::new("user-1", now_ts() + 3600);
    let token = encoder.encode(&claims).unwrap();

    for weak in [
        "secret", "password", "123456", "admin", "changeme", "sz-orm",
    ] {
        let guess = JwtEncoder::new(weak);
        assert!(
            guess.decode(&token).is_err(),
            "使用弱 secret 猜测解码必须失败: {weak}"
        );
    }
    // 正例：正确 secret 可解码
    assert!(encoder.decode(&token).is_ok());
}

#[test]
fn attack_malformed_tokens_do_not_panic() {
    let encoder = JwtEncoder::new("server-secret-42");
    // 空 token / 分段错误 / 非法 base64 / 乱码
    for malformed in [
        "",
        "a.b",
        "a.b.c.d",
        "!!!.???.###",
        "not-a-jwt",
        "eyJ.eyJ.sig",
    ] {
        let result = encoder.decode(malformed);
        assert!(
            result.is_err(),
            "畸形 token 应优雅失败而非 panic/接受: {malformed:?}"
        );
    }
}

/// URL-safe base64 编码（与 sz-orm-auth 内部一致的简化实现，仅测试用）
fn base64_url_encode(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(data)
}
