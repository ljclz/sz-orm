//! Real JWT (HS256) implementation with manual SHA-256, HMAC-SHA256, and base64url.
//!
//! Token format: `base64url(header).base64url(payload).base64url(signature)`
//! where signature = HMAC-SHA256(secret, `header.payload`).

use crate::error::AuthError;
use serde::{Deserialize, Serialize};

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

        // Verify signature using constant-time-ish comparison via byte equality.
        let signing_input = format!("{}.{}", header_b64, claims_b64);
        let expected_signature = hmac_sha256(self.secret.as_bytes(), signing_input.as_bytes());
        let expected_signature_b64 = base64_url_encode(&expected_signature);

        if signature_b64 != expected_signature_b64 {
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
// ============================================================================

const B64_URL_CHARS: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn base64_url_encode(input: &[u8]) -> String {
    let mut out = String::with_capacity((input.len() * 4).div_ceil(3));
    let mut i = 0;
    while i + 3 <= input.len() {
        let n = ((input[i] as u32) << 16) | ((input[i + 1] as u32) << 8) | (input[i + 2] as u32);
        out.push(B64_URL_CHARS[((n >> 18) & 0x3f) as usize] as char);
        out.push(B64_URL_CHARS[((n >> 12) & 0x3f) as usize] as char);
        out.push(B64_URL_CHARS[((n >> 6) & 0x3f) as usize] as char);
        out.push(B64_URL_CHARS[(n & 0x3f) as usize] as char);
        i += 3;
    }
    let remaining = input.len() - i;
    if remaining == 1 {
        let n = (input[i] as u32) << 16;
        out.push(B64_URL_CHARS[((n >> 18) & 0x3f) as usize] as char);
        out.push(B64_URL_CHARS[((n >> 12) & 0x3f) as usize] as char);
    } else if remaining == 2 {
        let n = ((input[i] as u32) << 16) | ((input[i + 1] as u32) << 8);
        out.push(B64_URL_CHARS[((n >> 18) & 0x3f) as usize] as char);
        out.push(B64_URL_CHARS[((n >> 12) & 0x3f) as usize] as char);
        out.push(B64_URL_CHARS[((n >> 6) & 0x3f) as usize] as char);
    }
    out
}

fn base64_url_decode(input: &str) -> Result<Vec<u8>, String> {
    fn char_to_val(c: u8) -> Result<u8, String> {
        match c {
            b'A'..=b'Z' => Ok(c - b'A'),
            b'a'..=b'z' => Ok(c - b'a' + 26),
            b'0'..=b'9' => Ok(c - b'0' + 52),
            b'-' => Ok(62),
            b'_' => Ok(63),
            _ => Err(format!("Invalid base64url character: {}", c as char)),
        }
    }

    let bytes = input.as_bytes();
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    // Reject padding; JWT uses unpadded base64url.
    if bytes.contains(&b'=') {
        return Err("base64url must not contain padding '='".to_string());
    }

    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    let mut i = 0;
    while i + 4 <= bytes.len() {
        let n = ((char_to_val(bytes[i])? as u32) << 18)
            | ((char_to_val(bytes[i + 1])? as u32) << 12)
            | ((char_to_val(bytes[i + 2])? as u32) << 6)
            | (char_to_val(bytes[i + 3])? as u32);
        out.push((n >> 16) as u8);
        out.push((n >> 8) as u8);
        out.push(n as u8);
        i += 4;
    }
    let remaining = bytes.len() - i;
    if remaining == 1 {
        return Err("Invalid base64url length (1 trailing char)".to_string());
    } else if remaining == 2 {
        let n =
            ((char_to_val(bytes[i])? as u32) << 18) | ((char_to_val(bytes[i + 1])? as u32) << 12);
        out.push((n >> 16) as u8);
    } else if remaining == 3 {
        let n = ((char_to_val(bytes[i])? as u32) << 18)
            | ((char_to_val(bytes[i + 1])? as u32) << 12)
            | ((char_to_val(bytes[i + 2])? as u32) << 6);
        out.push((n >> 16) as u8);
        out.push((n >> 8) as u8);
    }
    Ok(out)
}

// ============================================================================
// SHA-256 per FIPS 180-4
// ============================================================================

const SHA256_K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

const SHA256_H0: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

fn sha256(data: &[u8]) -> [u8; 32] {
    let mut msg = data.to_vec();
    let bit_len = (data.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    let mut hash = SHA256_H0;

    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let mut a = hash[0];
        let mut b = hash[1];
        let mut c = hash[2];
        let mut d = hash[3];
        let mut e = hash[4];
        let mut f = hash[5];
        let mut g = hash[6];
        let mut h = hash[7];

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(SHA256_K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        hash[0] = hash[0].wrapping_add(a);
        hash[1] = hash[1].wrapping_add(b);
        hash[2] = hash[2].wrapping_add(c);
        hash[3] = hash[3].wrapping_add(d);
        hash[4] = hash[4].wrapping_add(e);
        hash[5] = hash[5].wrapping_add(f);
        hash[6] = hash[6].wrapping_add(g);
        hash[7] = hash[7].wrapping_add(h);
    }

    let mut out = [0u8; 32];
    for (i, h) in hash.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&h.to_be_bytes());
    }
    out
}

// ============================================================================
// HMAC-SHA256 per RFC 2104
// ============================================================================

const HMAC_BLOCK_SIZE: usize = 64;

fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    // Normalize key: if longer than block size, hash it; then pad to block size.
    let mut key_block = [0u8; HMAC_BLOCK_SIZE];
    if key.len() > HMAC_BLOCK_SIZE {
        let hashed = sha256(key);
        key_block[..32].copy_from_slice(&hashed);
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }

    let mut o_key_pad = [0u8; HMAC_BLOCK_SIZE];
    let mut i_key_pad = [0u8; HMAC_BLOCK_SIZE];
    for i in 0..HMAC_BLOCK_SIZE {
        o_key_pad[i] = key_block[i] ^ 0x5c;
        i_key_pad[i] = key_block[i] ^ 0x36;
    }

    // inner = sha256(i_key_pad || message)
    let mut inner_input = Vec::with_capacity(HMAC_BLOCK_SIZE + message.len());
    inner_input.extend_from_slice(&i_key_pad);
    inner_input.extend_from_slice(message);
    let inner_hash = sha256(&inner_input);

    // outer = sha256(o_key_pad || inner_hash)
    let mut outer_input = Vec::with_capacity(HMAC_BLOCK_SIZE + 32);
    outer_input.extend_from_slice(&o_key_pad);
    outer_input.extend_from_slice(&inner_hash);
    sha256(&outer_input)
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
