//! 密码学已知答案测试（KAT）——标准向量验证（门禁 21 安全攻击测试）
//!
//! 向量来源：
//!   - SHA-256：NIST FIPS 180-4 示例
//!   - HMAC-SHA256：RFC 4231 测试向量 1/2
//!   - PBKDF2-HMAC-SHA256：Python hashlib 官方测试向量（dkLen=32）
//!   - AES-256-GCM：往返 + 篡改拒绝 + AAD 校验（nonce 随机，无法固定向量，
//!     以认证解密往返 + 篡改检测代替）

use sz_orm_crypto::{hmac_sha256, AesGcmCrypter, PasswordHasher, Pbkdf2Hasher};

fn hex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

#[test]
fn kat_sha256_nist_vectors() {
    // NIST FIPS 180-4：SHA-256("abc")
    assert_eq!(
        sz_orm_crypto::sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    // SHA-256("")（空串）
    assert_eq!(
        sz_orm_crypto::sha256_hex(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    // SHA-256 长消息（NIST 示例：'a' × 1,000,000）
    let long = vec![b'a'; 1_000_000];
    assert_eq!(
        sz_orm_crypto::sha256_hex(&long),
        "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
    );
}

#[test]
fn kat_hmac_sha256_rfc4231() {
    // RFC 4231 测试向量 1：key = 0x0b×20, data = "Hi There"
    let key1 = vec![0x0b; 20];
    assert_eq!(
        hmac_sha256(&key1, b"Hi There").to_vec(),
        hex("b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7")
    );
    // RFC 4231 测试向量 2：key = "Jefe", data = "what do ya want for nothing?"
    assert_eq!(
        hmac_sha256(b"Jefe", b"what do ya want for nothing?").to_vec(),
        hex("5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843")
    );
}

#[test]
fn kat_pbkdf2_sha256_python_vectors() {
    // Python hashlib.pbkdf2_hmac 官方向量（dkLen=32）：
    //   P="password", S="salt", c=1 → 120fb6cf...
    //   P="password", S="salt", c=2 → ae4d0c95...
    // 通过 verify 验证：构造 "$iterations$salt_hex$hash_hex"（"salt" 的 hex = 73616c74）
    let hasher = Pbkdf2Hasher::new();

    // c=1
    let c1 = "120fb6cffcf8b32c43e7225256c4f837a86548c92ccc35480805987cb70be17b";
    let hash_c1 = format!("$1$73616c74${c1}");
    assert!(
        hasher.verify("password", &hash_c1).unwrap_or(false),
        "PBKDF2 c=1 官方向量验证应通过"
    );
    assert!(
        !hasher.verify("wrong-password", &hash_c1).unwrap_or(true),
        "错误密码验证必须失败"
    );

    // c=2
    let c2 = "ae4d0c95af6b46d32d0adff928f06dd02a303f8ef3c251dfd6e2d85a95474c43";
    let hash_c2 = format!("$2$73616c74${c2}");
    assert!(
        hasher.verify("password", &hash_c2).unwrap_or(false),
        "PBKDF2 c=2 官方向量验证应通过"
    );
}

#[test]
fn kat_aes256gcm_roundtrip_and_tamper() {
    let key: [u8; 32] = *b"0123456789abcdef0123456789abcdef";
    let cipher = AesGcmCrypter::new(&key);
    let plaintext = b"attack-at-dawn-2026";
    let aad = b"header-v1";

    // 往返：加密 → 解密一致
    let ct = cipher.encrypt_with_aad(plaintext, aad).unwrap();
    let pt = cipher.decrypt_with_aad(&ct, aad).unwrap();
    assert_eq!(pt, plaintext);

    // 篡改检测：密文改 1 字节 → 认证失败
    let mut tampered = ct.clone();
    let last = tampered.len() - 1;
    tampered[last] ^= 0x01;
    assert!(
        cipher.decrypt_with_aad(&tampered, aad).is_err(),
        "篡改密文必须认证失败"
    );

    // AAD 不匹配 → 认证失败
    assert!(
        cipher.decrypt_with_aad(&ct, b"wrong-aad").is_err(),
        "AAD 不匹配必须认证失败"
    );

    // 短密文 → 优雅失败
    assert!(cipher.decrypt_with_aad(&[0u8; 4], aad).is_err());
}
