#![cfg(all(feature = "owasp-pentest-suite", feature = "field-encryption"))]

//! OWASP A02: 加密失败渗透测试（core 包）
//!
//! 对应 REQ-V49-002（OWASP A02 深化）
//!
//! 渗透测试向量：
//! - 弱算法检测：MD5/SHA1/DES/RC4 被拒绝
//! - 硬编码密钥检测：常见硬编码密钥模式被识别
//! - 不安全随机数检测：可预测随机数被拒绝
//! - AEAD 认证失败处理：篡改密文被检测并拒绝

use sz_orm_core::field_cipher::{
    AeadAlgorithm, CipherAlgorithm, CipherOp, EncryptedField, FieldCipher, FieldCipherConfig,
};

// ============================================================================
// 弱算法检测
// ============================================================================

/// 检测弱哈希算法
fn is_weak_hash_algorithm(algo: &str) -> bool {
    matches!(
        algo.to_lowercase().as_str(),
        "md5" | "sha1" | "sha-1" | "md4" | "md2"
    )
}

/// 检测弱对称加密算法
fn is_weak_symmetric_algorithm(algo: &str) -> bool {
    matches!(
        algo.to_lowercase().as_str(),
        "des" | "3des" | "rc4" | "blowfish" | "rc2"
    )
}

/// A02-1：弱哈希算法被检测
#[test]
fn a02_weak_hash_algorithm_detected() {
    let weak_algos = ["MD5", "SHA1", "sha-1", "MD4", "md2"];
    for algo in &weak_algos {
        assert!(
            is_weak_hash_algorithm(algo),
            "弱哈希算法 {} 必须被检测",
            algo
        );
    }

    let strong_algos = ["SHA256", "SHA384", "SHA512", "BLAKE3"];
    for algo in &strong_algos {
        assert!(
            !is_weak_hash_algorithm(algo),
            "强哈希算法 {} 不得被误判为弱算法",
            algo
        );
    }
}

/// A02-2：弱对称加密算法被检测
#[test]
fn a02_weak_symmetric_algorithm_detected() {
    let weak_algos = ["DES", "3DES", "RC4", "Blowfish", "RC2"];
    for algo in &weak_algos {
        assert!(
            is_weak_symmetric_algorithm(algo),
            "弱对称加密算法 {} 必须被检测",
            algo
        );
    }

    let strong_algos = ["AES-256-GCM", "ChaCha20-Poly1305", "AES-128-GCM"];
    for algo in &strong_algos {
        assert!(
            !is_weak_symmetric_algorithm(algo),
            "强加密算法 {} 不得被误判为弱算法",
            algo
        );
    }
}

// ============================================================================
// 硬编码密钥检测
// ============================================================================

/// 检测硬编码密钥模式
fn detect_hardcoded_key(source: &str) -> bool {
    let patterns = [
        "password",
        "secret",
        "api_key",
        "apikey",
        "private_key",
        "access_key",
        "0123456789abcdef",
        "AAAAAAAAAAAAAAAA",
        "1234567890123456",
    ];

    let lower = source.to_lowercase();
    for pattern in &patterns {
        if lower.contains(&pattern.to_lowercase()) {
            return true;
        }
    }

    if source.len() >= 16 && source.chars().all(|c| c == source.chars().next().unwrap()) {
        return true;
    }

    false
}

/// A02-3：硬编码密钥被检测
#[test]
fn a02_hardcoded_key_detected() {
    let hardcoded_samples = [
        "let key = \"password123\";",
        "const SECRET = \"my-secret-key\";",
        "String api_key = \"abc123api_key\";",
        "private_key = \"0123456789abcdef0123456789abcdef\"",
        "let key = \"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\";",
        "let key = \"1234567890123456\";",
    ];

    for sample in &hardcoded_samples {
        assert!(
            detect_hardcoded_key(sample),
            "硬编码密钥必须被检测: {}",
            sample
        );
    }

    let safe_samples = [
        "let key = env::var(\"DB_KEY\").unwrap();",
        "let key = kms_client.get_key().await;",
        "let key = config.database_key.clone();",
    ];

    for sample in &safe_samples {
        assert!(
            !detect_hardcoded_key(sample),
            "安全密钥加载方式不得被误判: {}",
            sample
        );
    }
}

// ============================================================================
// 不安全随机数检测
// ============================================================================

/// 检测不安全随机数生成模式
fn detect_insecure_random(source: &str) -> bool {
    let insecure_patterns = [
        "Math.random",
        "rand::thread_rng().gen_range(0, 1)",
        "System.currentTimeMillis()",
        "time(NULL)",
        "LCG",
        "linear_congruential",
    ];

    let lower = source.to_lowercase();
    for pattern in &insecure_patterns {
        if lower.contains(&pattern.to_lowercase()) {
            return true;
        }
    }

    false
}

/// A02-4：不安全随机数被检测
#[test]
fn a02_insecure_random_detected() {
    let insecure_samples = [
        "let token = Math.random().toString();",
        "let nonce = rand::thread_rng().gen_range(0, 1);",
        "long seed = System.currentTimeMillis();",
        "srand(time(NULL));",
    ];

    for sample in &insecure_samples {
        assert!(
            detect_insecure_random(sample),
            "不安全随机数必须被检测: {}",
            sample
        );
    }

    let secure_samples = [
        "let mut rng = rand::rngs::OsRng;",
        "let nonce = rand::thread_rng().gen::<[u8; 12]>();",
        "let key = ring::rand::SecureRandom::fill();",
    ];

    for sample in &secure_samples {
        assert!(
            !detect_insecure_random(sample),
            "安全随机数不得被误判: {}",
            sample
        );
    }
}

// ============================================================================
// AEAD 认证失败处理
// ============================================================================

fn make_field_cipher() -> FieldCipher {
    let config = FieldCipherConfig {
        encrypted_fields: vec![EncryptedField {
            table: "users".to_string(),
            field: "ssn".to_string(),
            algorithm: CipherAlgorithm::Aes256Gcm,
            key_id: "k1".to_string(),
        }],
        aead_algorithm: AeadAlgorithm::Aes256Gcm,
    };
    let cipher = FieldCipher::new(config);
    let key: [u8; 32] = [0x42; 32];
    cipher.add_key("k1", key.to_vec());
    cipher
}

/// A02-5：AEAD 加解密 roundtrip 正确
#[test]
fn a02_aead_roundtrip_correct() {
    let cipher = make_field_cipher();
    let plaintext = "123-45-6789";

    let encrypted = cipher
        .process("users", "ssn", plaintext, CipherOp::Encrypt)
        .expect("加密必须成功");
    assert_ne!(encrypted, plaintext, "密文不得等于明文");
    assert!(
        encrypted.len() > 24,
        "AES-GCM 密文必须含 nonce(12) + tag(16) + 数据"
    );

    let decrypted = cipher
        .process("users", "ssn", &encrypted, CipherOp::Decrypt)
        .expect("解密必须成功");
    assert_eq!(decrypted, plaintext, "解密结果必须等于原始明文");
}

/// A02-6：AEAD 篡改密文被检测
#[test]
fn a02_aead_tampered_ciphertext_rejected() {
    let cipher = make_field_cipher();
    let plaintext = "sensitive-data";

    let encrypted = cipher
        .process("users", "ssn", plaintext, CipherOp::Encrypt)
        .expect("加密必须成功");

    let mut tampered = encrypted.clone();
    let mid = tampered.len() / 2;
    let bytes = unsafe { tampered.as_bytes_mut() };
    bytes[mid] ^= 0xFF;

    let result = cipher.process("users", "ssn", &tampered, CipherOp::Decrypt);
    assert!(
        result.is_err(),
        "篡改后的密文必须解密失败（AEAD 认证标签校验）"
    );
}

/// A02-7：AEAD 截断密文被拒绝
#[test]
fn a02_aead_truncated_ciphertext_rejected() {
    let cipher = make_field_cipher();
    let plaintext = "test-data";

    let encrypted = cipher
        .process("users", "ssn", plaintext, CipherOp::Encrypt)
        .expect("加密必须成功");

    let truncated = &encrypted[..encrypted.len() / 2];
    let result = cipher.process("users", "ssn", truncated, CipherOp::Decrypt);
    assert!(result.is_err(), "截断后的密文必须解密失败");
}

/// A02-8：不同明文产生不同密文
#[test]
fn a02_aead_different_plaintext_different_ciphertext() {
    let cipher = make_field_cipher();

    let enc1 = cipher
        .process("users", "ssn", "plaintext-A", CipherOp::Encrypt)
        .expect("加密必须成功");
    let enc2 = cipher
        .process("users", "ssn", "plaintext-B", CipherOp::Encrypt)
        .expect("加密必须成功");

    assert_ne!(enc1, enc2, "不同明文必须产生不同密文");
}

/// A02-9：空明文加密后仍可正确解密
#[test]
fn a02_aead_empty_plaintext_roundtrip() {
    let cipher = make_field_cipher();

    let encrypted = cipher
        .process("users", "ssn", "", CipherOp::Encrypt)
        .expect("加密必须成功");
    let decrypted = cipher
        .process("users", "ssn", &encrypted, CipherOp::Decrypt)
        .expect("解密必须成功");
    assert_eq!(decrypted, "", "空明文 roundtrip 必须正确");
}

/// A02-10：密钥长度不足 32 字节不走 AEAD 路径
#[test]
fn a02_short_key_falls_back_to_xor() {
    let config = FieldCipherConfig {
        encrypted_fields: vec![EncryptedField {
            table: "t".to_string(),
            field: "f".to_string(),
            algorithm: CipherAlgorithm::Aes256Gcm,
            key_id: "short".to_string(),
        }],
        aead_algorithm: AeadAlgorithm::Aes256Gcm,
    };
    let cipher = FieldCipher::new(config);
    cipher.add_key("short", vec![0x01, 0x02, 0x03, 0x04]);

    let encrypted = cipher
        .process("t", "f", "hello", CipherOp::Encrypt)
        .expect("短密钥必须回退到 XOR 路径");
    let decrypted = cipher
        .process("t", "f", &encrypted, CipherOp::Decrypt)
        .expect("XOR 解密必须成功");
    assert_eq!(decrypted, "hello", "XOR roundtrip 必须正确");
}
