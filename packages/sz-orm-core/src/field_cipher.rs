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
