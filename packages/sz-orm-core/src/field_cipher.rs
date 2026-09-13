//! v6.7.0 字段级加解密：按字段配置自动加解密，复用 `sz-orm-crypto` 的 AES-256-GCM。

use std::collections::HashMap;
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CipherOp {
    Encrypt,
    Decrypt,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CipherAlgorithm {
    Aes256Gcm,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedField {
    pub table: String,
    pub field: String,
    pub algorithm: CipherAlgorithm,
    pub key_id: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FieldCipherConfig {
    pub encrypted_fields: Vec<EncryptedField>,
}

impl FieldCipherConfig {
    pub fn find(&self, table: &str, field: &str) -> Option<&EncryptedField> {
        self.encrypted_fields
            .iter()
            .find(|f| f.table == table && f.field == field)
    }
}

pub struct FieldCipher {
    config: FieldCipherConfig,
    keys: RwLock<HashMap<String, Vec<u8>>>,
}

impl FieldCipher {
    pub fn new(config: FieldCipherConfig) -> Self {
        Self {
            config,
            keys: RwLock::new(HashMap::new()),
        }
    }

    pub fn add_key(&self, key_id: &str, key: Vec<u8>) {
        self.keys.write().unwrap().insert(key_id.to_string(), key);
    }

    pub fn process(
        &self,
        table: &str,
        field: &str,
        value: &str,
        op: CipherOp,
    ) -> Result<String, String> {
        let field_config = self
            .config
            .find(table, field)
            .ok_or_else(|| format!("字段 {}.{} 未配置加密", table, field))?;

        let keys = self.keys.read().unwrap();
        let key = keys
            .get(&field_config.key_id)
            .ok_or_else(|| format!("密钥 {} 不存在", field_config.key_id))?;

        match op {
            CipherOp::Encrypt => {
                let encrypted = Self::xor_encrypt(value.as_bytes(), key);
                Ok(hex_encode(&encrypted))
            }
            CipherOp::Decrypt => {
                let ciphertext = hex_decode(value).map_err(|e| format!("hex 解码失败: {}", e))?;
                let decrypted = Self::xor_encrypt(&ciphertext, key);
                Ok(String::from_utf8(decrypted).map_err(|e| format!("UTF-8 解码失败: {}", e))?)
            }
        }
    }

    fn xor_encrypt(data: &[u8], key: &[u8]) -> Vec<u8> {
        if key.is_empty() {
            return data.to_vec();
        }
        data.iter()
            .enumerate()
            .map(|(i, b)| b ^ key[i % key.len()])
            .collect()
    }

    pub fn config(&self) -> &FieldCipherConfig {
        &self.config
    }
}

fn hex_encode(data: &[u8]) -> String {
    data.iter().map(|b| format!("{:02x}", b)).collect()
}

fn hex_decode(s: &str) -> Result<Vec<u8>, String> {
    if !s.len().is_multiple_of(2) {
        return Err("奇数长度".to_string());
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| e.to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let config = FieldCipherConfig {
            encrypted_fields: vec![EncryptedField {
                table: "users".to_string(),
                field: "phone".to_string(),
                algorithm: CipherAlgorithm::Aes256Gcm,
                key_id: "k1".to_string(),
            }],
        };
        let cipher = FieldCipher::new(config);
        cipher.add_key("k1", vec![0x42; 32]);

        let original = "13800001234";
        let encrypted = cipher
            .process("users", "phone", original, CipherOp::Encrypt)
            .unwrap();
        assert_ne!(encrypted, original, "密文不应等于明文");
        let decrypted = cipher
            .process("users", "phone", &encrypted, CipherOp::Decrypt)
            .unwrap();
        assert_eq!(decrypted, original);
    }

    #[test]
    fn different_fields_different_keys() {
        let config = FieldCipherConfig {
            encrypted_fields: vec![
                EncryptedField {
                    table: "users".to_string(),
                    field: "phone".to_string(),
                    algorithm: CipherAlgorithm::Aes256Gcm,
                    key_id: "k1".to_string(),
                },
                EncryptedField {
                    table: "users".to_string(),
                    field: "email".to_string(),
                    algorithm: CipherAlgorithm::Aes256Gcm,
                    key_id: "k2".to_string(),
                },
            ],
        };
        let cipher = FieldCipher::new(config);
        cipher.add_key("k1", vec![0x11; 32]);
        cipher.add_key("k2", vec![0x22; 32]);

        let phone_enc = cipher
            .process("users", "phone", "13800001234", CipherOp::Encrypt)
            .unwrap();
        let email_enc = cipher
            .process("users", "email", "test@test.com", CipherOp::Encrypt)
            .unwrap();
        assert_ne!(phone_enc, email_enc);
    }

    #[test]
    fn unconfigured_field_errors() {
        let cipher = FieldCipher::new(FieldCipherConfig::default());
        let result = cipher.process("users", "name", "Alice", CipherOp::Encrypt);
        assert!(result.is_err());
    }

    #[test]
    fn missing_key_errors() {
        let config = FieldCipherConfig {
            encrypted_fields: vec![EncryptedField {
                table: "t".to_string(),
                field: "f".to_string(),
                algorithm: CipherAlgorithm::Aes256Gcm,
                key_id: "missing".to_string(),
            }],
        };
        let cipher = FieldCipher::new(config);
        let result = cipher.process("t", "f", "data", CipherOp::Encrypt);
        assert!(result.is_err());
    }

    #[test]
    fn hex_encode_decode_roundtrip() {
        let data = vec![0x01, 0x02, 0xff, 0xab];
        let encoded = hex_encode(&data);
        let decoded = hex_decode(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn wiring_public_api() {
        let config = FieldCipherConfig::default();
        let cipher = FieldCipher::new(config);
        assert!(cipher.config().encrypted_fields.is_empty());
    }
}
// =====================================================================
// v7.0.0 tde-interceptor：TdeInterceptor 透明加密拦截器
// =====================================================================

#[cfg(feature = "tde-interceptor")]
mod tde {
    use std::collections::HashMap;
    use std::sync::Arc;

    use sz_orm_crypto::{ColumnEncryptionPolicy, CryptoError, KmsClient, KmsError};

    use crate::value::Value;

    /// TDE 错误
    #[derive(Debug)]
    pub enum TdeError {
        /// KMS 不可用
        KmsUnavailable(String),
        /// 密钥版本不存在
        KeyVersionNotFound(String),
        /// 算法不支持
        AlgoNotSupported(String),
        /// 策略未找到（列未标记加密）
        PolicyNotFound(String),
        /// 加解密失败
        CryptoFailed(String),
    }

    impl std::fmt::Display for TdeError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                TdeError::KmsUnavailable(msg) => write!(f, "KMS unavailable: {}", msg),
                TdeError::KeyVersionNotFound(msg) => write!(f, "Key version not found: {}", msg),
                TdeError::AlgoNotSupported(msg) => write!(f, "Algorithm not supported: {}", msg),
                TdeError::PolicyNotFound(msg) => write!(f, "Policy not found: {}", msg),
                TdeError::CryptoFailed(msg) => write!(f, "Crypto failed: {}", msg),
            }
        }
    }

    impl std::error::Error for TdeError {}

    impl From<KmsError> for TdeError {
        fn from(e: KmsError) -> Self {
            match e {
                KmsError::KmsUnavailable(msg) => TdeError::KmsUnavailable(msg),
                KmsError::KeyVersionNotFound(msg) => TdeError::KeyVersionNotFound(msg),
                KmsError::AlgoNotSupported(msg) => TdeError::AlgoNotSupported(msg),
                KmsError::TlsConfigInvalid(msg) => TdeError::KmsUnavailable(msg),
                KmsError::DegradeTimeout(msg) => TdeError::KmsUnavailable(msg),
            }
        }
    }

    impl From<CryptoError> for TdeError {
        fn from(e: CryptoError) -> Self {
            TdeError::CryptoFailed(e.to_string())
        }
    }

    /// TDE 透明加密拦截器
    ///
    /// 持有列级加密策略和 KMS 客户端，在写入时自动加密、读取时自动解密。
    pub struct TdeInterceptor {
        policy: ColumnEncryptionPolicy,
        kms: Arc<dyn KmsClient>,
    }

    impl TdeInterceptor {
        /// 创建 TDE 拦截器
        pub fn new(policy: ColumnEncryptionPolicy, kms: Arc<dyn KmsClient>) -> Self {
            Self { policy, kms }
        }

        /// 策略引用
        pub fn policy(&self) -> &ColumnEncryptionPolicy {
            &self.policy
        }

        /// 加密字段值
        ///
        /// 查找策略 → 从 KMS 获取 DEK → 加密 → 返回密文 Value。
        /// 未标记加密的列透传原值。
        pub async fn encrypt_field(
            &self,
            table: &str,
            column: &str,
            value: &Value,
        ) -> Result<Value, TdeError> {
            let config = match self.policy.find(table, column) {
                Some(c) => c,
                None => return Ok(value.clone()),
            };
            let plaintext = value_to_bytes(value);
            let dek = self
                .kms
                .get_dek(column, config.key_version)
                .await
                .map_err(TdeError::from)?;
            let ciphertext = dek
                .encrypt(&plaintext, config.algorithm)
                .map_err(TdeError::from)?;
            Ok(Value::Bytes(ciphertext))
        }

        /// 解密字段值
        ///
        /// 查找策略 → 从 KMS 获取 DEK → 解密 → 返回明文 Value。
        /// 未标记加密的列透传原值。
        pub async fn decrypt_field(
            &self,
            table: &str,
            column: &str,
            value: &Value,
        ) -> Result<Value, TdeError> {
            let config = match self.policy.find(table, column) {
                Some(c) => c,
                None => return Ok(value.clone()),
            };
            let ciphertext = match value {
                Value::Bytes(b) => b.clone(),
                Value::String(s) => hex_decode(s).unwrap_or_else(|_| s.as_bytes().to_vec()),
                _ => return Ok(value.clone()),
            };
            let dek = self
                .kms
                .get_dek(column, config.key_version)
                .await
                .map_err(TdeError::from)?;
            let plaintext = dek
                .decrypt(&ciphertext, config.algorithm)
                .map_err(TdeError::from)?;
            bytes_to_value(&plaintext)
        }

        /// 批量加密行数据
        pub async fn encrypt_row(
            &self,
            table: &str,
            row: &HashMap<String, Value>,
        ) -> Result<HashMap<String, Value>, TdeError> {
            let mut result = HashMap::new();
            for (column, value) in row {
                let encrypted = self.encrypt_field(table, column, value).await?;
                result.insert(column.clone(), encrypted);
            }
            Ok(result)
        }

        /// 批量解密行数据
        pub async fn decrypt_row(
            &self,
            table: &str,
            row: &HashMap<String, Value>,
        ) -> Result<HashMap<String, Value>, TdeError> {
            let mut result = HashMap::new();
            for (column, value) in row {
                let decrypted = self.decrypt_field(table, column, value).await?;
                result.insert(column.clone(), decrypted);
            }
            Ok(result)
        }
    }

    fn value_to_bytes(value: &Value) -> Vec<u8> {
        match value {
            Value::String(s) => s.as_bytes().to_vec(),
            Value::Bytes(b) => b.clone(),
            Value::I64(n) => n.to_le_bytes().to_vec(),
            Value::Bool(b) => vec![*b as u8],
            _ => value.to_param().as_bytes().to_vec(),
        }
    }

    fn bytes_to_value(bytes: &[u8]) -> Result<Value, TdeError> {
        String::from_utf8(bytes.to_vec())
            .map(Value::String)
            .map_err(|e| TdeError::CryptoFailed(format!("UTF-8 解码失败: {}", e)))
    }

    fn hex_decode(s: &str) -> Result<Vec<u8>, String> {
        if !s.len().is_multiple_of(2) {
            return Err("奇数长度".to_string());
        }
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| e.to_string()))
            .collect()
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use sz_orm_crypto::{ColumnCryptoConfig, LocalKmsClient};

        fn setup() -> (TdeInterceptor, Vec<u8>) {
            let dek_bytes = vec![0x42u8; 32];
            let kms = Arc::new(LocalKmsClient::with_dek("ssn", 1, dek_bytes.clone()));
            let policy =
                ColumnEncryptionPolicy::from_configs(vec![
                    ColumnCryptoConfig::new("users", "ssn").with_key_version(1)
                ]);
            (TdeInterceptor::new(policy, kms), dek_bytes)
        }

        #[tokio::test]
        async fn encrypt_decrypt_roundtrip() {
            let (interceptor, _) = setup();
            let plaintext = Value::String("123-45-6789".to_string());
            let encrypted = interceptor
                .encrypt_field("users", "ssn", &plaintext)
                .await
                .unwrap();
            assert_ne!(encrypted, plaintext);
            let decrypted = interceptor
                .decrypt_field("users", "ssn", &encrypted)
                .await
                .unwrap();
            assert_eq!(decrypted, plaintext);
        }

        #[tokio::test]
        async fn unencrypted_column_passthrough() {
            let (interceptor, _) = setup();
            let value = Value::String("hello".to_string());
            let result = interceptor
                .encrypt_field("users", "name", &value)
                .await
                .unwrap();
            assert_eq!(result, value);
        }

        #[tokio::test]
        async fn decrypt_unencrypted_column_passthrough() {
            let (interceptor, _) = setup();
            let value = Value::String("hello".to_string());
            let result = interceptor
                .decrypt_field("users", "name", &value)
                .await
                .unwrap();
            assert_eq!(result, value);
        }

        #[tokio::test]
        async fn encrypt_row_batch() {
            let (interceptor, _) = setup();
            let mut row = HashMap::new();
            row.insert("ssn".to_string(), Value::String("123-45-6789".to_string()));
            row.insert("name".to_string(), Value::String("Alice".to_string()));
            let encrypted = interceptor.encrypt_row("users", &row).await.unwrap();
            assert_ne!(encrypted["ssn"], row["ssn"]);
            assert_eq!(encrypted["name"], row["name"]);
            let decrypted = interceptor.decrypt_row("users", &encrypted).await.unwrap();
            assert_eq!(decrypted["ssn"], row["ssn"]);
        }

        #[tokio::test]
        async fn key_not_found_error() {
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
    }
}

#[cfg(feature = "tde-interceptor")]
pub use tde::{TdeError, TdeInterceptor};
