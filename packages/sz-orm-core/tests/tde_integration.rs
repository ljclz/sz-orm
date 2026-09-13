//! v7.0.0 TDE 透明数据加密端到端集成测试

#![cfg(feature = "tde-interceptor")]

use std::collections::HashMap;
use std::sync::Arc;

use sz_orm_core::KmsClient;
use sz_orm_core::Value;
use sz_orm_core::{ColumnCryptoConfig, ColumnEncryptionPolicy, LocalKmsClient, TdeInterceptor};

#[tokio::test]
async fn tde_full_workflow() {
    let kms = Arc::new(LocalKmsClient::new());
    kms.generate_dek("ssn", 1);
    kms.generate_dek("email", 1);

    let policy = ColumnEncryptionPolicy::from_configs(vec![
        ColumnCryptoConfig::new("users", "ssn").with_key_version(1),
        ColumnCryptoConfig::new("users", "email").with_key_version(1),
    ]);
    let interceptor = TdeInterceptor::new(policy, kms.clone());

    let mut row = HashMap::new();
    row.insert("ssn".to_string(), Value::String("123-45-6789".to_string()));
    row.insert(
        "email".to_string(),
        Value::String("alice@example.com".to_string()),
    );
    row.insert("name".to_string(), Value::String("Alice".to_string()));

    let encrypted = interceptor.encrypt_row("users", &row).await.unwrap();
    assert_ne!(encrypted["ssn"], row["ssn"]);
    assert_ne!(encrypted["email"], row["email"]);
    assert_eq!(encrypted["name"], row["name"]);

    let decrypted = interceptor.decrypt_row("users", &encrypted).await.unwrap();
    assert_eq!(decrypted["ssn"], row["ssn"]);
    assert_eq!(decrypted["email"], row["email"]);
    assert_eq!(decrypted["name"], row["name"]);
}

#[tokio::test]
async fn tde_key_rotation_old_data_decryptable() {
    let kms = Arc::new(LocalKmsClient::new());
    kms.generate_dek("ssn", 1);

    let policy_v1 =
        ColumnEncryptionPolicy::from_configs(vec![
            ColumnCryptoConfig::new("users", "ssn").with_key_version(1)
        ]);
    let interceptor_v1 = TdeInterceptor::new(policy_v1, kms.clone());

    let plaintext = Value::String("123-45-6789".to_string());
    let encrypted = interceptor_v1
        .encrypt_field("users", "ssn", &plaintext)
        .await
        .unwrap();

    let new_version = kms.rotate_key("ssn").await.unwrap();
    assert_eq!(new_version, 2);

    let decrypted = interceptor_v1
        .decrypt_field("users", "ssn", &encrypted)
        .await
        .unwrap();
    assert_eq!(decrypted, plaintext);
}

#[tokio::test]
async fn tde_unencrypted_passthrough() {
    let kms = Arc::new(LocalKmsClient::new());
    let policy = ColumnEncryptionPolicy::new();
    let interceptor = TdeInterceptor::new(policy, kms);

    let value = Value::String("hello".to_string());
    let encrypted = interceptor
        .encrypt_field("users", "name", &value)
        .await
        .unwrap();
    assert_eq!(encrypted, value);
}

#[tokio::test]
async fn tde_kms_unavailable() {
    let kms = Arc::new(LocalKmsClient::new());
    let policy =
        ColumnEncryptionPolicy::from_configs(vec![
            ColumnCryptoConfig::new("users", "ssn").with_key_version(99)
        ]);
    let interceptor = TdeInterceptor::new(policy, kms);

    let result = interceptor
        .encrypt_field("users", "ssn", &Value::String("test".to_string()))
        .await;
    assert!(result.is_err());
}
