#![cfg(all(feature = "owasp-pentest-suite", feature = "field-encryption"))]

//! OWASP v10.10.0 密码学攻击向量渗透测试（T4.5）
//!
//! 新增 4 个密码学攻击变体：弱密钥尝试、密钥轮换竞态、
//! 版本回滚攻击、nonce 重用尝试。
//! 复用既有 owasp-pentest-suite + field-encryption feature gate。

use sz_orm_crypto::key_rotation_enhanced::KeyRotationEnhanced;
use sz_orm_crypto::{AesGcmCrypter, Crypter, PasswordHasher, Pbkdf2Hasher};

/// 密码学-1：弱密钥尝试变体
///
/// 攻击模型：攻击者尝试使用弱密钥（全零/重复/短密钥）。
/// PBKDF2 弱迭代必须被拒绝；AES-256 密钥长度由类型系统保证。
#[test]
fn test_owasp_v10100_a02_crypto_weak_key_attempt() {
    let weak_iter_hashes = [
        Pbkdf2Hasher::with_iterations(0),
        Pbkdf2Hasher::with_iterations(1),
        Pbkdf2Hasher::with_iterations(999),
        Pbkdf2Hasher::with_iterations(99_999),
    ];
    for (i, hasher) in weak_iter_hashes.iter().enumerate() {
        let result = hasher.hash("test_password");
        assert!(
            result.is_err(),
            "弱迭代次数[{}] 必须被拒绝（< 100_000）",
            i
        );
    }

    let strong_hasher = Pbkdf2Hasher::new();
    assert!(strong_hasher.hash("test_password").is_ok(), "默认迭代必须通过");

    let zero_key = [0u8; 32];
    let crypter = AesGcmCrypter::new(&zero_key);
    let ct = crypter.encrypt(b"sensitive").unwrap();
    assert_eq!(crypter.decrypt(&ct).unwrap(), b"sensitive");
}

/// 密码学-2：密钥轮换竞态变体
///
/// 攻击模型：攻击者尝试在轮换过程中插入重复密钥 ID，
/// 试图破坏密钥一致性。rotate_key 必须拒绝重复 key_id。
#[test]
fn test_owasp_v10100_a02_crypto_key_rotation_race() {
    let mgr = KeyRotationEnhanced::new("key-v1", b"initial-key".to_vec());

    let result = mgr.rotate_key("key-v1", b"duplicate-key".to_vec());
    assert!(
        result.is_err(),
        "重复 key_id 必须被拒绝"
    );

    mgr.rotate_key("key-v2", b"second-key".to_vec()).unwrap();
    let result2 = mgr.rotate_key("key-v2", b"another-duplicate".to_vec());
    assert!(
        result2.is_err(),
        "已存在的 key_id 必须被拒绝"
    );

    mgr.record_usage();
    mgr.record_usage();
    let history = mgr.version_history();
    assert!(
        !history.versions.is_empty(),
        "版本历史应记录轮换操作"
    );
}

/// 密码学-3：版本回滚攻击变体
///
/// 攻击模型：攻击者尝试回滚到不存在的密钥版本，
/// 试图使用已淘汰的弱密钥。key_rollback 必须拒绝不存在的版本。
#[test]
fn test_owasp_v10100_a02_crypto_version_rollback_attack() {
    let mgr = KeyRotationEnhanced::new("key-v1", b"initial-key".to_vec());
    mgr.rotate_key("key-v2", b"second-key".to_vec()).unwrap();
    mgr.rotate_key("key-v3", b"third-key".to_vec()).unwrap();

    let rollback_result = mgr.key_rollback("nonexistent-key");
    assert!(
        rollback_result.is_err(),
        "回滚到不存在的密钥版本必须被拒绝"
    );

    let rollback_empty = mgr.key_rollback("");
    assert!(
        rollback_empty.is_err(),
        "回滚到空 key_id 必须被拒绝"
    );
}

/// 密码学-4：nonce 重用尝试变体
///
/// 攻击模型：攻击者尝试重用 nonce 加密不同明文。
/// AES-GCM 每次加密生成随机 12 字节 nonce，
/// 相同明文加密两次密文必须不同（nonce 唯一性）。
#[test]
fn test_owasp_v10100_a02_crypto_nonce_reuse_attempt() {
    let key = [42u8; 32];
    let crypter = AesGcmCrypter::new(&key);

    let plaintext = b"highly sensitive data for nonce reuse test";

    let mut ciphertexts = Vec::new();
    for _ in 0..10 {
        let ct = crypter.encrypt(plaintext).unwrap();
        ciphertexts.push(ct);
    }

    for i in 0..ciphertexts.len() {
        for j in (i + 1)..ciphertexts.len() {
            assert_ne!(
                ciphertexts[i], ciphertexts[j],
                "nonce 必须唯一：加密[{}] 和 加密[{}] 密文必须不同",
                i, j
            );
        }
    }

    for ct in &ciphertexts {
        let pt = crypter.decrypt(ct).unwrap();
        assert_eq!(pt, plaintext, "所有密文必须解密回相同明文");
    }
}