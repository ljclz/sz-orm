//! Real JWT (HS256) implementation using RustCrypto audited crates.
//!
//! v0.2.2 重构（P2-5）：将手写 SHA-256 / HMAC-SHA256 / base64url 替换为
//! RustCrypto audited crate（`sha2`、`hmac`、`base64`），降低密码学实现风险。
//!
//! Token format: `base64url(header).base64url(payload).base64url(signature)`
//! where signature = HMAC-SHA256(secret, `header.payload`).

use crate::error::AuthError;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

/// HMAC-SHA256 类型别名（来自 RustCrypto `hmac` crate）
type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtHeader {
    pub alg: String,
    pub typ: String,
}

impl Default for JwtHeader {
    fn default() -> Self {
        Self {
            alg: "HS256".to_string(),
            typ: "JWT".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtClaims {
    pub sub: String,
    pub exp: i64,
    pub iat: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iss: Option<String>,
    #[serde(default)]
    pub roles: Vec<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
    /// 用户 ID（v0.2.1 新增，修复 Critical S-2）
    ///
    /// - `Some(id)`：携带用户 ID，verify_token 可恢复正确 user.id
    /// - `None`：兼容旧 token；verify_token 会回退为 0 并通过 tracing 警告
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<i64>,
}

impl JwtClaims {
    pub fn new(sub: impl Into<String>, exp: i64) -> Self {
        Self {
            sub: sub.into(),
            exp,
            iat: current_timestamp(),
            iss: None,
            roles: Vec::new(),
            permissions: Vec::new(),
            user_id: None,
        }
    }

    pub fn with_issuer(mut self, iss: impl Into<String>) -> Self {
        self.iss = Some(iss.into());
        self
    }

    pub fn with_roles(mut self, roles: Vec<String>) -> Self {
        self.roles = roles;
        self
    }

    pub fn with_permissions(mut self, permissions: Vec<String>) -> Self {
        self.permissions = permissions;
        self
    }

    /// 设置用户 ID（v0.2.1 新增）
    pub fn with_user_id(mut self, user_id: i64) -> Self {
        self.user_id = Some(user_id);
        self
    }

    pub fn is_expired(&self) -> bool {
        current_timestamp() > self.exp
    }
}

pub struct JwtEncoder {
    secret: String,
}

impl JwtEncoder {
    pub fn new(secret: impl Into<String>) -> Self {
        Self {
            secret: secret.into(),
        }
    }

    pub fn secret(&self) -> &str {
        &self.secret
    }

    pub fn encode(&self, claims: &JwtClaims) -> Result<String, AuthError> {
        let header = JwtHeader::default();
        let header_json = serde_json::to_string(&header)
            .map_err(|e| AuthError::TokenInvalid(format!("Header serialization failed: {}", e)))?;
        let claims_json = serde_json::to_string(claims)
            .map_err(|e| AuthError::TokenInvalid(format!("Claims serialization failed: {}", e)))?;

        let header_b64 = base64_url_encode(header_json.as_bytes());
        let claims_b64 = base64_url_encode(claims_json.as_bytes());

        let signing_input = format!("{}.{}", header_b64, claims_b64);
        let signature = hmac_sha256(self.secret.as_bytes(), signing_input.as_bytes());
        let signature_b64 = base64_url_encode(&signature);

        Ok(format!("{}.{}.{}", header_b64, claims_b64, signature_b64))
    }

    pub fn decode(&self, token: &str) -> Result<JwtClaims, AuthError> {
        if token.is_empty() {
            return Err(AuthError::TokenInvalid("Token is empty".to_string()));
        }

        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() != 3 {
            return Err(AuthError::TokenInvalid(
                "Invalid JWT format: expected 3 parts".to_string(),
            ));
        }

        let header_b64 = parts[0];
        let claims_b64 = parts[1];
        let signature_b64 = parts[2];

        // Verify signature using constant-time comparison (v0.2.2 修复 H-3)。
        //
        // 原实现 `signature_b64 != expected_signature_b64` 使用 `String::ne`，
        // 该方法逐字节比较并在第一个不匹配处短路返回，导致比较时间与匹配前缀长度成正比，
        // 攻击者可通过测量响应时间逐字节恢复有效签名（时序攻击）。
        //
        // 修复：使用 `subtle::ConstantTimeEq`（RustCrypto audited crate），
        // 确保无论匹配多少字节，比较时间恒定。
        use subtle::ConstantTimeEq;
        let signing_input = format!("{}.{}", header_b64, claims_b64);
        let expected_signature = hmac_sha256(self.secret.as_bytes(), signing_input.as_bytes());
        let expected_signature_b64 = base64_url_encode(&expected_signature);

        let sig_bytes = signature_b64.as_bytes();
        let expected_bytes = expected_signature_b64.as_bytes();
        // 长度不同直接拒绝（长度信息非敏感，可短路）
        if sig_bytes.len() != expected_bytes.len() {
            return Err(AuthError::TokenInvalid("Invalid signature".to_string()));
        }
        // 常量时间比较字节数组
        let sig_match: bool = sig_bytes.ct_eq(expected_bytes).into();
        if !sig_match {
            return Err(AuthError::TokenInvalid("Invalid signature".to_string()));
        }

        // Decode header
        let header_bytes = base64_url_decode(header_b64)
            .map_err(|e| AuthError::TokenInvalid(format!("Header decode failed: {}", e)))?;
        let header: JwtHeader = serde_json::from_slice(&header_bytes)
            .map_err(|e| AuthError::TokenInvalid(format!("Header parse failed: {}", e)))?;

        if header.alg != "HS256" {
            return Err(AuthError::TokenInvalid(format!(
                "Unsupported algorithm: {}",
                header.alg
            )));
        }
        if header.typ != "JWT" {
            return Err(AuthError::TokenInvalid(format!(
                "Unsupported token type: {}",
                header.typ
            )));
        }

        // Decode claims
        let claims_bytes = base64_url_decode(claims_b64)
            .map_err(|e| AuthError::TokenInvalid(format!("Claims decode failed: {}", e)))?;
        let claims: JwtClaims = serde_json::from_slice(&claims_bytes)
            .map_err(|e| AuthError::TokenInvalid(format!("Claims parse failed: {}", e)))?;

        if claims.is_expired() {
            return Err(AuthError::TokenExpired("Token has expired".to_string()));
        }

        Ok(claims)
    }
}

fn current_timestamp() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

// ============================================================================
// base64 URL-safe encoding (no padding) per RFC 4648 Section 5
//
// v0.2.2 重构（P2-5）：使用 RustCrypto audited `base64` crate 替代手写实现。
// ============================================================================

fn base64_url_encode(input: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(input)
}

fn base64_url_decode(input: &str) -> Result<Vec<u8>, String> {
    // JWT 使用 unpadded base64url，显式拒绝 `=` padding
    if input.contains('=') {
        return Err("base64url must not contain padding '='".to_string());
    }
    URL_SAFE_NO_PAD
        .decode(input)
        .map_err(|e| format!("Invalid base64url: {}", e))
}

// ============================================================================
// SHA-256 per FIPS 180-4
//
// v0.2.2 重构（P2-5）：使用 RustCrypto audited `sha2` crate 替代手写实现。
// ============================================================================

#[cfg(test)]
fn sha256(data: &[u8]) -> [u8; 32] {
    use sha2::Digest;
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&result);
    out
}

// ============================================================================
// HMAC-SHA256 per RFC 2104
//
// v0.2.2 重构（P2-5）：使用 RustCrypto audited `hmac` crate 替代手写实现。
// ============================================================================

fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(message);
    let result = mac.finalize().into_bytes();
    let mut out = [0u8; 32];
    out.copy_from_slice(&result);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> i64 {
        current_timestamp()
    }

    // SHA-256 known answer tests (FIPS 180-2 examples)

    #[test]
    fn test_sha256_empty() {
        // sha256("") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
        let hash = sha256(b"");
        let expected_hex = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        assert_eq!(hex_str(&hash), expected_hex);
    }

    #[test]
    fn test_sha256_abc() {
        // sha256("abc") = ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
        let hash = sha256(b"abc");
        let expected_hex = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert_eq!(hex_str(&hash), expected_hex);
    }

    #[test]
    fn test_sha256_longer_message() {
        // sha256("abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq")
        let input = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";
        let hash = sha256(input);
        let expected_hex = "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1";
        assert_eq!(hex_str(&hash), expected_hex);
    }

    // HMAC-SHA256 known answer test (RFC 4231 Test Case 1)

    #[test]
    fn test_hmac_sha256_rfc4231_case1() {
        // Key = 0x0b repeated 20 times, Data = "Hi There"
        let key = [0x0bu8; 20];
        let message = b"Hi There";
        let mac = hmac_sha256(&key, message);
        let expected_hex = "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7";
        assert_eq!(hex_str(&mac), expected_hex);
    }

    #[test]
    fn test_hmac_sha256_rfc4231_case2() {
        // Key = "Jefe", Data = "what do ya want for nothing?"
        let mac = hmac_sha256(b"Jefe", b"what do ya want for nothing?");
        let expected_hex = "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843";
        assert_eq!(hex_str(&mac), expected_hex);
    }

    #[test]
    fn test_hmac_sha256_long_key() {
        // Key longer than 64 bytes should be hashed first (RFC 4231 Case 6 uses 131 bytes)
        let key = [0xaau8; 131];
        let data = b"Test Using Larger Than Block-Size Key - Hash Key First";
        let mac = hmac_sha256(&key, data);
        let expected_hex = "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54";
        assert_eq!(hex_str(&mac), expected_hex);
    }

    // base64url tests

    #[test]
    fn test_base64_url_encode_known() {
        // RFC 4648 Section 10 (with URL alphabet, no padding):
        // "" -> "", "f" -> "Zg", "fo" -> "Zm8", "foo" -> "Zm9v",
        // "foob" -> "Zm9vYg", "fooba" -> "Zm9vYmE", "foobar" -> "Zm9vYmFy"
        assert_eq!(base64_url_encode(b""), "");
        assert_eq!(base64_url_encode(b"f"), "Zg");
        assert_eq!(base64_url_encode(b"fo"), "Zm8");
        assert_eq!(base64_url_encode(b"foo"), "Zm9v");
        assert_eq!(base64_url_encode(b"foob"), "Zm9vYg");
        assert_eq!(base64_url_encode(b"fooba"), "Zm9vYmE");
        assert_eq!(base64_url_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn test_base64_url_decode_known() {
        assert_eq!(base64_url_decode("").unwrap(), b"");
        assert_eq!(base64_url_decode("Zg").unwrap(), b"f");
        assert_eq!(base64_url_decode("Zm8").unwrap(), b"fo");
        assert_eq!(base64_url_decode("Zm9v").unwrap(), b"foo");
        assert_eq!(base64_url_decode("Zm9vYg").unwrap(), b"foob");
        assert_eq!(base64_url_decode("Zm9vYmE").unwrap(), b"fooba");
        assert_eq!(base64_url_decode("Zm9vYmFy").unwrap(), b"foobar");
    }

    #[test]
    fn test_base64_url_roundtrip() {
        let cases: &[&[u8]] = &[
            b"",
            b"a",
            b"ab",
            b"abc",
            b"abcd",
            b"hello world",
            &[0xffu8; 64],
            &[0x00u8; 64],
            &(0u8..=255).collect::<Vec<u8>>(),
        ];
        for c in cases {
            let encoded = base64_url_encode(c);
            let decoded = base64_url_decode(&encoded).unwrap();
            assert_eq!(decoded.as_slice(), *c, "roundtrip failed for {:?}", c);
        }
    }

    #[test]
    fn test_base64_url_rejects_padding() {
        assert!(base64_url_decode("Zg==").is_err());
    }

    #[test]
    fn test_base64_url_rejects_invalid_char() {
        assert!(base64_url_decode("Zm9v*").is_err());
    }

    // JWT encode/decode tests

    #[test]
    fn test_jwt_encode_decode_roundtrip() {
        let encoder = JwtEncoder::new("my-secret");
        let claims = JwtClaims::new("user123", now() + 3600)
            .with_issuer("test-issuer")
            .with_roles(vec!["user".to_string(), "editor".to_string()])
            .with_permissions(vec!["read:posts".to_string(), "write:posts".to_string()]);

        let token = encoder.encode(&claims).expect("encode");
        assert!(!token.is_empty());

        let parts: Vec<&str> = token.split('.').collect();
        assert_eq!(parts.len(), 3);

        let decoded = encoder.decode(&token).expect("decode");
        assert_eq!(decoded.sub, "user123");
        assert_eq!(decoded.iss, Some("test-issuer".to_string()));
        assert_eq!(
            decoded.roles,
            vec!["user".to_string(), "editor".to_string()]
        );
        assert_eq!(
            decoded.permissions,
            vec!["read:posts".to_string(), "write:posts".to_string()]
        );
    }

    #[test]
    fn test_jwt_format_is_header_payload_signature() {
        let encoder = JwtEncoder::new("secret");
        let claims = JwtClaims::new("alice", now() + 60);
        let token = encoder.encode(&claims).unwrap();
        let parts: Vec<&str> = token.split('.').collect();
        assert_eq!(parts.len(), 3);

        // Header should decode to {"alg":"HS256","typ":"JWT"}
        let header_bytes = base64_url_decode(parts[0]).unwrap();
        let header: JwtHeader = serde_json::from_slice(&header_bytes).unwrap();
        assert_eq!(header.alg, "HS256");
        assert_eq!(header.typ, "JWT");
    }

    #[test]
    fn test_jwt_signature_changes_with_secret() {
        let encoder_a = JwtEncoder::new("secret-a");
        let encoder_b = JwtEncoder::new("secret-b");
        let claims = JwtClaims::new("user", now() + 3600);

        let token_a = encoder_a.encode(&claims).unwrap();
        let token_b = encoder_b.encode(&claims).unwrap();

        // Header.payload should be the same, but signature differs.
        let parts_a: Vec<&str> = token_a.split('.').collect();
        let parts_b: Vec<&str> = token_b.split('.').collect();
        assert_eq!(parts_a[0], parts_b[0]); // header
        assert_eq!(parts_a[1], parts_b[1]); // payload
        assert_ne!(parts_a[2], parts_b[2]); // signature
    }

    #[test]
    fn test_jwt_verify_with_wrong_secret_fails() {
        let encoder_a = JwtEncoder::new("secret-a");
        let encoder_b = JwtEncoder::new("secret-b");
        let claims = JwtClaims::new("user", now() + 3600);

        let token = encoder_a.encode(&claims).unwrap();
        let result = encoder_b.decode(&token);
        assert!(matches!(result, Err(AuthError::TokenInvalid(_))));
    }

    #[test]
    fn test_jwt_expired_token_rejected() {
        let encoder = JwtEncoder::new("secret");
        let claims = JwtClaims::new("user", now() - 100); // expired 100s ago
        let token = encoder.encode(&claims).unwrap();
        let result = encoder.decode(&token);
        assert!(matches!(result, Err(AuthError::TokenExpired(_))));
    }

    #[test]
    fn test_jwt_decode_invalid_format() {
        let encoder = JwtEncoder::new("secret");
        assert!(matches!(
            encoder.decode(""),
            Err(AuthError::TokenInvalid(_))
        ));
        assert!(matches!(
            encoder.decode("not.a.jwt.token"),
            Err(AuthError::TokenInvalid(_))
        ));
        assert!(matches!(
            encoder.decode("only.two"),
            Err(AuthError::TokenInvalid(_))
        ));
    }

    #[test]
    fn test_jwt_tampered_payload_rejected() {
        let encoder = JwtEncoder::new("secret");
        let claims = JwtClaims::new("alice", now() + 3600);
        let token = encoder.encode(&claims).unwrap();

        // Tamper with the payload by replacing it with a different valid base64url string.
        let parts: Vec<&str> = token.split('.').collect();
        let tampered_payload = base64_url_encode(
            br#"{"sub":"mallory","exp":9999999999,"iat":0,"roles":[],"permissions":[]}"#,
        );
        let tampered = format!("{}.{}.{}", parts[0], tampered_payload, parts[2]);
        let result = encoder.decode(&tampered);
        assert!(matches!(result, Err(AuthError::TokenInvalid(_))));
    }

    #[test]
    fn test_jwt_tampered_signature_rejected() {
        let encoder = JwtEncoder::new("secret");
        let claims = JwtClaims::new("alice", now() + 3600);
        let token = encoder.encode(&claims).unwrap();

        let parts: Vec<&str> = token.split('.').collect();
        // Flip the first character of the signature
        let mut sig = parts[2].to_string();
        let first = sig.chars().next().unwrap();
        let replacement = if first == 'A' { 'B' } else { 'A' };
        sig.replace_range(0..first.len_utf8(), &replacement.to_string());
        let tampered = format!("{}.{}.{}", parts[0], parts[1], sig);
        let result = encoder.decode(&tampered);
        assert!(matches!(result, Err(AuthError::TokenInvalid(_))));
    }

    fn hex_str(bytes: &[u8]) -> String {
        let mut s = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            s.push_str(&format!("{:02x}", b));
        }
        s
    }
}
