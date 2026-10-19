//! 安全合规深化接线端到端测试 - 密码学（v10.10.0 T4.3）

#![cfg(feature = "field-encryption")]

use sz_orm_crypto::key_rotation_enhanced::KeyRotationEnhanced;

// ============================================================================
// T4.3: 密钥轮换自动化（KeyRotationEnhanced 自动轮换 + 版本管理）
// ============================================================================

#[test]
fn test_auto_rotate_by_period_rotates_key() {
    let mgr = KeyRotationEnhanced::new("k1", b"key-v1".to_vec());
    let result = mgr.auto_rotate_by_period(
        std::time::Duration::from_secs(0),
        "k2",
        b"key-v2".to_vec(),
    );

    assert!(result.is_ok(), "周期为 0 时应立即轮换");
    assert_eq!(mgr.write_key_id(), "k2");
}

#[test]
fn test_auto_rotate_by_usage_rotates_key() {
    let mgr = KeyRotationEnhanced::new("k1", b"key-v1".to_vec());

    for _ in 0..100 {
        mgr.record_usage();
    }

    let result = mgr.auto_rotate_by_usage(100, "k2", b"key-v2".to_vec());
    assert!(result.is_ok(), "使用次数达到阈值应轮换");
    assert_eq!(mgr.write_key_id(), "k2");
}

#[test]
fn test_key_rollback_restores_previous_key() {
    let mgr = KeyRotationEnhanced::new("k1", b"key-v1".to_vec());
    mgr.rotate_key("k2", b"key-v2".to_vec()).unwrap();
    assert_eq!(mgr.write_key_id(), "k2");

    let result = mgr.key_rollback("k1");
    assert!(result.is_ok(), "回滚到 k1 应成功");
    assert_eq!(mgr.write_key_id(), "k1", "回滚后写密钥应为 k1");
}

#[test]
fn test_version_history_records_rotations() {
    let mgr = KeyRotationEnhanced::new("k1", b"key-v1".to_vec());
    mgr.rotate_key("k2", b"key-v2".to_vec()).unwrap();
    mgr.rotate_key("k3", b"key-v3".to_vec()).unwrap();

    let history = mgr.version_history();
    assert!(
        history.versions.len() >= 3,
        "版本历史应记录至少 3 个版本"
    );
    assert!(
        history.versions.iter().any(|e| e.key_id == "k1"),
        "历史应包含 k1"
    );
    assert!(
        history.versions.iter().any(|e| e.key_id == "k2"),
        "历史应包含 k2"
    );
    assert!(
        history.versions.iter().any(|e| e.key_id == "k3"),
        "历史应包含 k3"
    );
}

#[test]
fn test_old_data_decryptable_after_rotation() {
    let mgr = KeyRotationEnhanced::new("k1", b"key-v1".to_vec());
    let (old_id, old_ct) = mgr.encrypt_with_write_key(b"sensitive-data").unwrap();
    assert_eq!(old_id, "k1");

    mgr.rotate_key("k2", b"key-v2".to_vec()).unwrap();

    let decrypted = mgr.decrypt_with_any_active_key("k1", &old_ct).unwrap();
    assert_eq!(
        decrypted, b"sensitive-data",
        "轮换后旧数据应仍可解密"
    );
}

#[test]
fn test_new_data_encrypted_with_new_key() {
    let mgr = KeyRotationEnhanced::new("k1", b"key-v1".to_vec());
    mgr.rotate_key("k2", b"key-v2".to_vec()).unwrap();

    let (key_id, ct) = mgr.encrypt_with_write_key(b"new-data").unwrap();
    assert_eq!(key_id, "k2", "新数据应使用新密钥 k2 加密");

    let decrypted = mgr.decrypt_with_any_active_key(&key_id, &ct).unwrap();
    assert_eq!(decrypted, b"new-data");
}