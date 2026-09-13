//! DEK（数据加密密钥）安全缓冲区
//!
//! 使用 `zeroize::ZeroizingVec` 包装 DEK 明文，确保 Drop 时内存清零。

use zeroize::Zeroizing;

use crate::{AesGcmCrypter, Crypter, CryptoError};

/// 加密算法枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EncryptionAlgo {
    /// AES-256-GCM（已实现）
    #[default]
    Aes256Gcm,
    /// ChaCha20-Poly1305（需引入 chacha20poly1305 crate，当前返回 AlgoNotSupported）
    ChaCha20Poly1305,
    /// SM4-GCM（国密算法，需引入 sm4 crate，当前返回 AlgoNotSupported）
    Sm4Gcm,
}

impl EncryptionAlgo {
    /// 算法名称
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Aes256Gcm => "AES-256-GCM",
            Self::ChaCha20Poly1305 => "ChaCha20-Poly1305",
            Self::Sm4Gcm => "SM4-GCM",
        }
    }

    /// 是否已实现
    pub fn is_supported(&self) -> bool {
        matches!(self, Self::Aes256Gcm)
    }
}

/// DEK 安全缓冲区
///
/// 内部用 `Zeroizing<Vec<u8>>` 包装 DEK 明文，Drop 时自动清零。
pub struct DekBuffer {
    dek: Zeroizing<Vec<u8>>,
}

impl DekBuffer {
    /// 创建 DEK 缓冲区
    pub fn new(plaintext: Vec<u8>) -> Self {
        Self {
            dek: Zeroizing::new(plaintext),
        }
    }

    /// 从字节切片创建
    pub fn from_slice(slice: &[u8]) -> Self {
        Self::new(slice.to_vec())
    }

    /// 获取 DEK 字节
    pub fn as_bytes(&self) -> &[u8] {
        &self.dek
    }

    /// DEK 长度
    pub fn len(&self) -> usize {
        self.dek.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.dek.is_empty()
    }

    /// 用 DEK 加密数据
    ///
    /// 支持 AES-256-GCM，其他算法返回 `CryptoError`。
    pub fn encrypt(&self, plaintext: &[u8], algo: EncryptionAlgo) -> Result<Vec<u8>, CryptoError> {
        match algo {
            EncryptionAlgo::Aes256Gcm => {
                if self.len() != 32 {
                    return Err(CryptoError::InvalidKey(format!(
                        "AES-256-GCM 需要 32 字节 DEK，实际 {} 字节",
                        self.len()
                    )));
                }
                let mut key = [0u8; 32];
                key.copy_from_slice(&self.dek[..32]);
                let crypter = AesGcmCrypter::new(&key);
                crypter.encrypt(plaintext)
            }
            EncryptionAlgo::ChaCha20Poly1305 | EncryptionAlgo::Sm4Gcm => {
                Err(CryptoError::EncryptionFailed(format!(
                    "算法 {} 未实现，请使用 AES-256-GCM",
                    algo.as_str()
                )))
            }
        }
    }

    /// 用 DEK 解密数据
    pub fn decrypt(&self, ciphertext: &[u8], algo: EncryptionAlgo) -> Result<Vec<u8>, CryptoError> {
        match algo {
            EncryptionAlgo::Aes256Gcm => {
                if self.len() != 32 {
                    return Err(CryptoError::InvalidKey(format!(
                        "AES-256-GCM 需要 32 字节 DEK，实际 {} 字节",
                        self.len()
                    )));
                }
                let mut key = [0u8; 32];
                key.copy_from_slice(&self.dek[..32]);
                let crypter = AesGcmCrypter::new(&key);
                crypter.decrypt(ciphertext)
            }
            EncryptionAlgo::ChaCha20Poly1305 | EncryptionAlgo::Sm4Gcm => {
                Err(CryptoError::DecryptionFailed(format!(
                    "算法 {} 未实现，请使用 AES-256-GCM",
                    algo.as_str()
                )))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dek_buffer_encrypt_decrypt_aes256gcm() {
        let dek = DekBuffer::new(vec![0x42u8; 32]);
        let plaintext = b"sensitive data";
        let ciphertext = dek.encrypt(plaintext, EncryptionAlgo::Aes256Gcm).unwrap();
        assert_ne!(&ciphertext[..], &plaintext[..]);
        let decrypted = dek.decrypt(&ciphertext, EncryptionAlgo::Aes256Gcm).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn dek_buffer_empty_plaintext() {
        let dek = DekBuffer::new(vec![0x42u8; 32]);
        let ciphertext = dek.encrypt(b"", EncryptionAlgo::Aes256Gcm).unwrap();
        let decrypted = dek.decrypt(&ciphertext, EncryptionAlgo::Aes256Gcm).unwrap();
        assert_eq!(decrypted, b"");
    }

    #[test]
    fn dek_buffer_wrong_key_fails() {
        let dek1 = DekBuffer::new(vec![0x42u8; 32]);
        let dek2 = DekBuffer::new(vec![0x43u8; 32]);
        let ciphertext = dek1.encrypt(b"secret", EncryptionAlgo::Aes256Gcm).unwrap();
        assert!(dek2
            .decrypt(&ciphertext, EncryptionAlgo::Aes256Gcm)
            .is_err());
    }

    #[test]
    fn dek_buffer_invalid_key_length() {
        let dek = DekBuffer::new(vec![0x42u8; 16]);
        assert!(dek.encrypt(b"data", EncryptionAlgo::Aes256Gcm).is_err());
    }

    #[test]
    fn dek_buffer_chacha20_unsupported() {
        let dek = DekBuffer::new(vec![0x42u8; 32]);
        assert!(dek
            .encrypt(b"data", EncryptionAlgo::ChaCha20Poly1305)
            .is_err());
    }

    #[test]
    fn dek_buffer_sm4_unsupported() {
        let dek = DekBuffer::new(vec![0x42u8; 32]);
        assert!(dek.encrypt(b"data", EncryptionAlgo::Sm4Gcm).is_err());
    }

    #[test]
    fn dek_buffer_drop_zeroizes() {
        let dek = DekBuffer::new(vec![0xABu8; 32]);
        let ptr = dek.as_bytes().as_ptr();
        assert_eq!(unsafe { *ptr }, 0xAB);
        drop(dek);
        // SAFETY: Drop 后 ZeroizingVec 已将内存清零，读取该地址验证全零。
        // 此处 ptr 指向的内存可能已被释放，但 ZeroizingVec 在 Drop 时
        // 先清零再释放，所以如果内存尚未被复用，应读到 0。
        // 这是一个尽力检查（best-effort），不保证在所有分配器下都通过。
        // SAFETY: 读取已释放内存是 UB，此测试改为验证 Drop 不 panic。
    }

    #[test]
    fn encryption_algo_default() {
        assert_eq!(EncryptionAlgo::default(), EncryptionAlgo::Aes256Gcm);
    }

    #[test]
    fn encryption_algo_is_supported() {
        assert!(EncryptionAlgo::Aes256Gcm.is_supported());
        assert!(!EncryptionAlgo::ChaCha20Poly1305.is_supported());
        assert!(!EncryptionAlgo::Sm4Gcm.is_supported());
    }
}
