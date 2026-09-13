//! v7.0.0 TDE 透明数据加密完整示例
//!
//! 演示：配置列级策略 → KMS 交互 → 透明加解密 → 密钥轮换

#![cfg(feature = "tde-interceptor")]

use std::collections::HashMap;
use std::sync::Arc;

use sz_orm_core::Value;
use sz_orm_core::{ColumnCryptoConfig, ColumnEncryptionPolicy, LocalKmsClient, TdeInterceptor};

#[tokio::main]
async fn main() {
    println!("=== sz-orm v7.0.0 TDE 透明数据加密示例 ===\n");

    let kms = Arc::new(LocalKmsClient::with_dek("ssn", 1, vec![0x42u8; 32]));
    println!("1. KMS 初始化完成（LocalKmsClient）");

    let policy = ColumnEncryptionPolicy::from_configs(vec![
        ColumnCryptoConfig::new("users", "ssn").with_key_version(1),
        ColumnCryptoConfig::new("users", "email").with_key_version(1),
    ]);
    println!("2. 列级策略配置完成（{} 列加密）", policy.len());

    let interceptor = TdeInterceptor::new(policy, kms.clone());
    println!("3. TdeInterceptor 创建完成\n");

    let mut row = HashMap::new();
    row.insert("ssn".to_string(), Value::String("123-45-6789".to_string()));
    row.insert(
        "email".to_string(),
        Value::String("alice@example.com".to_string()),
    );
    row.insert("name".to_string(), Value::String("Alice".to_string()));
    println!("4. 原始数据: {:?}", row);

    let encrypted = interceptor.encrypt_row("users", &row).await.unwrap();
    println!(
        "5. 加密后: ssn={:?} email={:?} name={:?}",
        encrypted["ssn"], encrypted["email"], encrypted["name"]
    );

    let decrypted = interceptor.decrypt_row("users", &encrypted).await.unwrap();
    println!(
        "6. 解密后: ssn={:?} email={:?} name={:?}",
        decrypted["ssn"], decrypted["email"], decrypted["name"]
    );

    assert_eq!(decrypted, row);
    println!("\n=== TDE 示例完成：明文 → 密文 → 明文 往返一致 ===");
}
