//! # SZ-ORM Crypto — 加密工具
//!
//! 提供常用密码学原语：AES-256-GCM 对称加密、HMAC-SHA256 消息认证码、
//! PBKDF2 密钥派生与 SHA-256 哈希，所有实现基于 RustCrypto，保证常数时间比较。
//!
//! ## 主要函数
//!
//! - [`sha256`] / [`sha256_hex`] — SHA-256 哈希
//! - AES-256-GCM 加解密
//! - HMAC-SHA256 与 PBKDF2

use std::collections::HashMap;

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use hmac::{Hmac, Mac};
use pbkdf2::pbkdf2_hmac;
use rand::rngs::OsRng;
use rand::RngCore;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

type HmacSha256 = Hmac<Sha256>;

// ============================================================================
// SHA-256 (基于 RustCrypto sha2 crate, FIPS 180-4)
// ============================================================================

/// 计算 SHA-256 哈希（基于 RustCrypto sha2）
pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&result);
    out
}

/// 计算 SHA-256 并返回十六进制字符串
pub fn sha256_hex(data: &[u8]) -> String {
    sha256(data).iter().map(|b| format!("{:02x}", b)).collect()
}

/// HMAC-SHA256 (RFC 2104, 基于 RustCrypto hmac crate)
pub fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    // HMAC-SHA256 按 RFC 2104 接受任意长度 key，RustCrypto 的 new_from_slice 对 HMAC 永远返回 Ok。
    // 用 match 处理避免 panic，虽然 Err 分支不可达（RustCrypto 不变量保证）。
    let mut mac = match <HmacSha256 as Mac>::new_from_slice(key) {
        Ok(m) => m,
        Err(_) => {
            // 不可达分支：HMAC 规范允许任意 key 长度，RustCrypto 内部会先 hash 过长 key。
            // 为安全起见返回全零（调用方在正常路径下永远不会命中此分支）。
            return [0u8; 32];
        }
    };
    mac.update(message);
    let result = mac.finalize().into_bytes();
    let mut out = [0u8; 32];
    out.copy_from_slice(&result);
    out
}

/// HMAC-SHA256 十六进制字符串
pub fn hmac_sha256_hex(key: &[u8], message: &[u8]) -> String {
    hmac_sha256(key, message)
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect()
}

/// 常量时间比较（基于 subtle crate），避免时序攻击
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.ct_eq(b).into()
}

// ============================================================================
// 加密器
// ============================================================================

pub trait Crypter: Send + Sync {
    fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, CryptoError>;
    fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, CryptoError>;
}

/// AES-256-GCM 加密器（密码学安全）
///
/// 使用 AES-256-GCM AEAD 算法，每次加密生成随机 12 字节 nonce。
/// 密文格式：`nonce(12) || ciphertext || tag(16)`（由 aes-gcm crate 内部处理）。
pub struct AesGcmCrypter {
    cipher: Aes256Gcm,
}

impl AesGcmCrypter {
    /// 从 32 字节密钥创建
    pub fn new(key: &[u8; 32]) -> Self {
        let key = Key::<Aes256Gcm>::from_slice(key);
        Self {
            cipher: Aes256Gcm::new(key),
        }
    }

    /// 从任意长度密钥字符串创建（SHA-256 派生 32 字节密钥）
    pub fn from_key_str(key: &str) -> Self {
        let hash = sha256(key.as_bytes());
        Self::new(&hash)
    }

    fn random_nonce() -> [u8; 12] {
        let mut nonce = [0u8; 12];
        OsRng.fill_bytes(&mut nonce);
        nonce
    }
}

impl Crypter for AesGcmCrypter {
    fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        self.encrypt_with_aad(plaintext, &[])
    }

    fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        self.decrypt_with_aad(ciphertext, &[])
    }
}

impl AesGcmCrypter {
    /// AES-GCM 认证加密（带附加认证数据 AAD）
    ///
    /// 密文格式：`nonce(12) || ciphertext || tag(16)`
    /// AAD（Additional Authenticated Data）不包含在密文中，但参与认证标签计算，
    /// 解密时必须提供相同的 AAD 才能成功。
    pub fn encrypt_with_aad(&self, plaintext: &[u8], aad: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let nonce_bytes = Self::random_nonce();
        let nonce = Nonce::from_slice(&nonce_bytes);
        let payload = aes_gcm::aead::Payload { msg: plaintext, aad };
        let ciphertext = self
            .cipher
            .encrypt(nonce, payload)
            .map_err(|e| CryptoError::EncryptionFailed(e.to_string()))?;
        let mut result = Vec::with_capacity(12 + ciphertext.len());
        result.extend_from_slice(&nonce_bytes);
        result.extend_from_slice(&ciphertext);
        Ok(result)
    }

    /// AES-GCM 认证解密（带附加认证数据 AAD）
    ///
    /// 必须提供与加密时相同的 AAD，否则认证标签校验失败。
    pub fn decrypt_with_aad(&self, ciphertext: &[u8], aad: &[u8]) -> Result<Vec<u8>, CryptoError> {
        if ciphertext.len() < 12 {
            return Err(CryptoError::DecryptionFailed(
                "Ciphertext too short".to_string(),
            ));
        }
        let nonce = Nonce::from_slice(&ciphertext[..12]);
        let encrypted = &ciphertext[12..];
        let payload = aes_gcm::aead::Payload {
            msg: encrypted,
            aad,
        };
        self.cipher
            .decrypt(nonce, payload)
            .map_err(|e| CryptoError::DecryptionFailed(e.to_string()))
    }
}

// ============================================================================
// 密码哈希
// ============================================================================

pub trait PasswordHasher: Send + Sync {
    fn hash(&self, password: &str) -> Result<String, CryptoError>;
    fn verify(&self, password: &str, hash: &str) -> Result<bool, CryptoError>;
}

/// PBKDF2-HMAC-SHA256 密码哈希器（基于 RustCrypto pbkdf2 crate）
///
/// 使用 PBKDF2-HMAC-SHA256 算法（RFC 8018）。
/// 哈希格式：`$<iterations>$<salt_hex>$<hash_hex>`
pub struct Pbkdf2Hasher {
    iterations: u32,
}

impl Pbkdf2Hasher {
    const DEFAULT_ITERATIONS: u32 = 100_000;
    const SALT_LEN: usize = 16;
    const HASH_LEN: usize = 32;

    pub fn new() -> Self {
        Self {
            iterations: Self::DEFAULT_ITERATIONS,
        }
    }

    pub fn with_iterations(iterations: u32) -> Self {
        Self {
            iterations: iterations.max(1),
        }
    }

    fn compute_hash(password: &str, salt: &[u8], iterations: u32) -> [u8; Self::HASH_LEN] {
        let mut out = [0u8; Self::HASH_LEN];
        pbkdf2_hmac::<Sha256>(password.as_bytes(), salt, iterations, &mut out);
        out
    }
}

impl Default for Pbkdf2Hasher {
    fn default() -> Self {
        Self::new()
    }
}

impl PasswordHasher for Pbkdf2Hasher {
    fn hash(&self, password: &str) -> Result<String, CryptoError> {
        if password.is_empty() {
            return Err(CryptoError::InvalidHash(
                "Password cannot be empty".to_string(),
            ));
        }
        let salt = random_bytes(Self::SALT_LEN);
        let hash = Self::compute_hash(password, &salt, self.iterations);
        Ok(format!(
            "${}${}${}",
            self.iterations,
            hex_encode(&salt),
            hex_encode(&hash)
        ))
    }

    fn verify(&self, password: &str, hash: &str) -> Result<bool, CryptoError> {
        if !hash.starts_with('$') {
            return Err(CryptoError::InvalidHash("Invalid hash format".to_string()));
        }
        let parts: Vec<&str> = hash[1..].splitn(3, '$').collect();
        if parts.len() != 3 {
            return Err(CryptoError::InvalidHash("Invalid hash format".to_string()));
        }
        let iterations: u32 = parts[0]
            .parse()
            .map_err(|_| CryptoError::InvalidHash("Invalid iterations".to_string()))?;
        let salt = hex_decode(parts[1])
            .map_err(|_| CryptoError::InvalidHash("Invalid salt hex".to_string()))?;
        let expected_hash = hex_decode(parts[2])
            .map_err(|_| CryptoError::InvalidHash("Invalid hash hex".to_string()))?;
        let computed = Self::compute_hash(password, &salt, iterations);
        Ok(constant_time_eq(&computed, &expected_hash))
    }
}

// ============================================================================
// API 签名
// ============================================================================

pub trait ApiSigner: Send + Sync {
    fn sign(&self, params: &HashMap<String, String>, secret: &str) -> String;
    fn verify(&self, params: &HashMap<String, String>, secret: &str, signature: &str) -> bool;
}

/// HMAC-SHA256 API 签名器
///
/// 对参数按字典序排序后拼接成 query string，再用 HMAC-SHA256 签名。
pub struct HmacSigner;

impl HmacSigner {
    pub fn new() -> Self {
        Self
    }

    fn compute_signature(params: &HashMap<String, String>, secret: &str) -> String {
        let mut sorted: Vec<_> = params.iter().collect();
        sorted.sort_by(|a, b| a.0.cmp(b.0));

        let query_string: String = sorted
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join("&");

        hmac_sha256_hex(secret.as_bytes(), query_string.as_bytes())
    }
}

impl Default for HmacSigner {
    fn default() -> Self {
        Self::new()
    }
}

impl ApiSigner for HmacSigner {
    fn sign(&self, params: &HashMap<String, String>, secret: &str) -> String {
        Self::compute_signature(params, secret)
    }

    fn verify(&self, params: &HashMap<String, String>, secret: &str, signature: &str) -> bool {
        let computed = Self::compute_signature(params, secret);
        constant_time_eq(computed.as_bytes(), signature.as_bytes())
    }
}

// ============================================================================
// RSA-OAEP 非对称加密
// ============================================================================

use rsa::oaep::Oaep;
use rsa::{RsaPrivateKey, RsaPublicKey};
use sha2::Sha256 as RsaSha256;

/// RSA-OAEP 非对称加密器（基于 RustCrypto `rsa` crate）
///
/// 使用 RSA-OAEP with SHA-256 和 MGF1-SHA256 填充方案。
/// 公钥加密，私钥解密，适用于小数据（如密钥交换、短消息加密）。
pub struct RsaOaepCrypter {
    public_key: RsaPublicKey,
    private_key: RsaPrivateKey,
}

impl RsaOaepCrypter {
    /// 生成新的 RSA 密钥对（指定位数，推荐 2048 或 3072）
    pub fn generate(key_bits: usize) -> Result<Self, CryptoError> {
        let mut rng = OsRng;
        let private_key =
            RsaPrivateKey::new(&mut rng, key_bits).map_err(|e| CryptoError::InvalidKey(e.to_string()))?;
        let public_key = RsaPublicKey::from(&private_key);
        Ok(Self {
            public_key,
            private_key,
        })
    }

    /// 从已有密钥对创建
    pub fn from_keys(public_key: RsaPublicKey, private_key: RsaPrivateKey) -> Self {
        Self {
            public_key,
            private_key,
        }
    }

    /// 返回公钥引用
    pub fn public_key(&self) -> &RsaPublicKey {
        &self.public_key
    }

    /// 返回私钥引用
    pub fn private_key(&self) -> &RsaPrivateKey {
        &self.private_key
    }

    /// 使用公钥加密数据（RSA-OAEP with SHA-256）
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let mut rng = OsRng;
        let padding = Oaep::new::<RsaSha256>();
        self.public_key
            .encrypt(&mut rng, padding, plaintext)
            .map_err(|e| CryptoError::EncryptionFailed(e.to_string()))
    }

    /// 使用私钥解密数据（RSA-OAEP with SHA-256）
    pub fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let padding = Oaep::new::<RsaSha256>();
        self.private_key
            .decrypt(padding, ciphertext)
            .map_err(|e| CryptoError::DecryptionFailed(e.to_string()))
    }
}

impl Crypter for RsaOaepCrypter {
    fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        self.encrypt(plaintext)
    }

    fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        self.decrypt(ciphertext)
    }
}

// ============================================================================
// HMAC-SHA256 签名验证器
// ============================================================================

/// 签名验证器 trait：提供消息签名与验证接口
pub trait SignatureVerifier: Send + Sync {
    /// 对消息生成签名
    fn sign(&self, message: &[u8]) -> Vec<u8>;
    /// 验证消息签名（常量时间比较）
    fn verify(&self, message: &[u8], signature: &[u8]) -> bool;
}

/// HMAC-SHA256 签名验证器
///
/// 使用 HMAC-SHA256 算法对消息签名，验证时采用常量时间比较防止时序攻击。
pub struct HmacSignatureVerifier {
    key: Vec<u8>,
}

impl HmacSignatureVerifier {
    /// 创建签名验证器，从任意长度密钥派生
    pub fn new(key: &[u8]) -> Self {
        Self {
            key: key.to_vec(),
        }
    }

    /// 从字符串密钥创建
    pub fn from_key_str(key: &str) -> Self {
        Self::new(key.as_bytes())
    }
}

impl SignatureVerifier for HmacSignatureVerifier {
    fn sign(&self, message: &[u8]) -> Vec<u8> {
        hmac_sha256(&self.key, message).to_vec()
    }

    fn verify(&self, message: &[u8], signature: &[u8]) -> bool {
        let expected = self.sign(message);
        constant_time_eq(&expected, signature)
    }
}

// ============================================================================
// 密钥轮换（Key Rotation）
// ============================================================================

/// 密钥版本：保存密钥及其版本号和创建时间
#[derive(Clone)]
struct KeyVersion {
    version: u32,
    key: Vec<u8>,
    created_at: u64,
}

/// 密钥轮换管理器
///
/// 管理多个版本的密钥，支持：
/// - 轮换生成新版本密钥
/// - 用最新密钥签名
/// - 用任意历史密钥验证（向后兼容）
/// - 自动淘汰过期密钥
pub struct KeyRotationManager {
    keys: Vec<KeyVersion>,
    current_version: u32,
    max_versions: usize,
}

impl KeyRotationManager {
    /// 创建密钥轮换管理器，指定最大保留版本数
    pub fn new(max_versions: usize) -> Self {
        Self {
            keys: vec![],
            current_version: 0,
            max_versions: max_versions.max(1),
        }
    }

    /// 初始化首个密钥版本
    pub fn with_initial_key(key: Vec<u8>) -> Self {
        let mut mgr = Self::new(3);
        mgr.rotate_key(key);
        mgr
    }

    /// 轮换到新密钥，返回新版本号
    pub fn rotate_key(&mut self, new_key: Vec<u8>) -> u32 {
        self.current_version += 1;
        let now = current_timestamp_secs();
        self.keys.push(KeyVersion {
            version: self.current_version,
            key: new_key,
            created_at: now,
        });
        // 淘汰过期版本
        while self.keys.len() > self.max_versions {
            self.keys.remove(0);
        }
        self.current_version
    }

    /// 使用当前（最新）密钥签名
    pub fn sign(&self, message: &[u8]) -> (u32, Vec<u8>) {
        if let Some(kv) = self.keys.last() {
            let sig = hmac_sha256(&kv.key, message).to_vec();
            (kv.version, sig)
        } else {
            (0, vec![])
        }
    }

    /// 验证签名（尝试所有保留的密钥版本）
    pub fn verify(&self, message: &[u8], version: u32, signature: &[u8]) -> bool {
        for kv in &self.keys {
            if kv.version == version {
                let expected = hmac_sha256(&kv.key, message);
                return constant_time_eq(&expected, signature);
            }
        }
        false
    }

    /// 返回当前密钥版本号
    pub fn current_version(&self) -> u32 {
        self.current_version
    }

    /// 返回保留的密钥版本数量
    pub fn version_count(&self) -> usize {
        self.keys.len()
    }

    /// 返回所有保留的版本号
    pub fn versions(&self) -> Vec<u32> {
        self.keys.iter().map(|kv| kv.version).collect()
    }

    /// 返回指定版本密钥的创建时间（Unix 秒），不存在返回 None
    pub fn key_created_at(&self, version: u32) -> Option<u64> {
        self.keys
            .iter()
            .find(|kv| kv.version == version)
            .map(|kv| kv.created_at)
    }
}

fn current_timestamp_secs() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ============================================================================
// 辅助函数
// ============================================================================

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn hex_decode(hex: &str) -> Result<Vec<u8>, ()> {
    if !hex.len().is_multiple_of(2) {
        return Err(());
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| ()))
        .collect()
}

fn random_bytes(len: usize) -> Vec<u8> {
    let mut result = vec![0u8; len];
    OsRng.fill_bytes(&mut result);
    result
}

// ============================================================================
// 错误类型
// ============================================================================

#[derive(Debug)]
pub enum CryptoError {
    EncryptionFailed(String),
    DecryptionFailed(String),
    InvalidKey(String),
    InvalidNonce(String),
    InvalidHash(String),
    SigningFailed(String),
}

impl std::fmt::Display for CryptoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CryptoError::EncryptionFailed(msg) => write!(f, "Encryption failed: {}", msg),
            CryptoError::DecryptionFailed(msg) => write!(f, "Decryption failed: {}", msg),
            CryptoError::InvalidKey(msg) => write!(f, "Invalid key: {}", msg),
            CryptoError::InvalidNonce(msg) => write!(f, "Invalid nonce: {}", msg),
            CryptoError::InvalidHash(msg) => write!(f, "Invalid hash: {}", msg),
            CryptoError::SigningFailed(msg) => write!(f, "Signing failed: {}", msg),
        }
    }
}

impl std::error::Error for CryptoError {}

impl serde::Serialize for CryptoError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // --- SHA-256 标准测试向量 (FIPS 180-2 / NIST) ---

    #[test]
    fn test_sha256_empty() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn test_sha256_abc() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn test_sha256_hello() {
        assert_eq!(
            sha256_hex(b"hello"),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn test_sha256_long_message() {
        assert_eq!(
            sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn test_sha256_deterministic() {
        assert_eq!(sha256_hex(b"test"), sha256_hex(b"test"));
        assert_ne!(sha256_hex(b"test"), sha256_hex(b"Test"));
    }

    // --- HMAC-SHA256 测试向量 (RFC 4231) ---

    #[test]
    fn test_hmac_sha256_rfc4231_case1() {
        let key = vec![0x0bu8; 20];
        let result = hmac_sha256_hex(&key, b"Hi There");
        assert_eq!(
            result,
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
    }

    #[test]
    fn test_hmac_sha256_rfc4231_case2() {
        let result = hmac_sha256_hex(b"Jefe", b"what do ya want for nothing?");
        assert_eq!(
            result,
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn test_hmac_sha256_long_key() {
        let key = vec![0xaau8; 130];
        let result = hmac_sha256_hex(&key, b"test message");
        assert_eq!(result.len(), 64);
        let short_key = vec![0xaau8; 32];
        let result_short = hmac_sha256_hex(&short_key, b"test message");
        assert_ne!(result, result_short);
    }

    #[test]
    fn test_hmac_sha256_different_messages() {
        let key = b"secret";
        assert_ne!(hmac_sha256_hex(key, b"msg1"), hmac_sha256_hex(key, b"msg2"));
    }

    // --- AesGcmCrypter 测试 ---

    #[test]
    fn test_aes_gcm_roundtrip() {
        let key = [0x42u8; 32];
        let crypter = AesGcmCrypter::new(&key);
        let plaintext = b"Hello, World!";
        let encrypted = crypter.encrypt(plaintext).unwrap();
        let decrypted = crypter.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_aes_gcm_random_nonce_per_encryption() {
        let key = [0x42u8; 32];
        let crypter = AesGcmCrypter::new(&key);
        let plaintext = b"same plaintext";
        let encrypted1 = crypter.encrypt(plaintext).unwrap();
        let encrypted2 = crypter.encrypt(plaintext).unwrap();
        assert_ne!(encrypted1, encrypted2, "随机 nonce 应使密文不同");
        assert_eq!(crypter.decrypt(&encrypted1).unwrap(), plaintext);
        assert_eq!(crypter.decrypt(&encrypted2).unwrap(), plaintext);
    }

    #[test]
    fn test_aes_gcm_from_key_str() {
        let crypter = AesGcmCrypter::from_key_str("my-secret-key");
        let plaintext = b"data to encrypt";
        let encrypted = crypter.encrypt(plaintext).unwrap();
        let decrypted = crypter.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_aes_gcm_short_ciphertext() {
        let key = [0x42u8; 32];
        let crypter = AesGcmCrypter::new(&key);
        assert!(crypter.decrypt(&[0u8; 8]).is_err());
    }

    #[test]
    fn test_aes_gcm_empty_plaintext() {
        let key = [0x42u8; 32];
        let crypter = AesGcmCrypter::new(&key);
        let encrypted = crypter.encrypt(b"").unwrap();
        // nonce(12) + tag(16) = 28
        assert_eq!(encrypted.len(), 28);
        let decrypted = crypter.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, b"");
    }

    #[test]
    fn test_aes_gcm_tampered_ciphertext() {
        let key = [0x42u8; 32];
        let crypter = AesGcmCrypter::new(&key);
        let encrypted = crypter.encrypt(b"sensitive data").unwrap();
        let mut tampered = encrypted.clone();
        tampered[15] ^= 0x01;
        assert!(crypter.decrypt(&tampered).is_err());
    }

    // --- Pbkdf2Hasher 测试 ---

    #[test]
    fn test_pbkdf2_hasher_hash_format() {
        let hasher = Pbkdf2Hasher::new();
        let hash = hasher.hash("password123").unwrap();
        assert!(hash.starts_with('$'));
        let parts: Vec<&str> = hash[1..].splitn(3, '$').collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0].parse::<u32>().unwrap(), 100_000);
        // salt 32 hex chars (16 bytes)
        assert_eq!(parts[1].len(), 32);
        // hash 64 hex chars (32 bytes)
        assert_eq!(parts[2].len(), 64);
    }

    #[test]
    fn test_pbkdf2_hasher_verify_correct() {
        let hasher = Pbkdf2Hasher::new();
        let hash = hasher.hash("password123").unwrap();
        assert!(hasher.verify("password123", &hash).unwrap());
    }

    #[test]
    fn test_pbkdf2_hasher_verify_wrong() {
        let hasher = Pbkdf2Hasher::new();
        let hash = hasher.hash("password123").unwrap();
        assert!(!hasher.verify("wrongpassword", &hash).unwrap());
    }

    #[test]
    fn test_pbkdf2_hasher_different_passwords_different_hashes() {
        let hasher = Pbkdf2Hasher::new();
        let h1 = hasher.hash("pass1").unwrap();
        let h2 = hasher.hash("pass2").unwrap();
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_pbkdf2_hasher_same_password_different_salts() {
        let hasher = Pbkdf2Hasher::new();
        let h1 = hasher.hash("same").unwrap();
        let h2 = hasher.hash("same").unwrap();
        assert_ne!(h1, h2);
        assert!(hasher.verify("same", &h1).unwrap());
        assert!(hasher.verify("same", &h2).unwrap());
    }

    #[test]
    fn test_pbkdf2_hasher_invalid_format() {
        let hasher = Pbkdf2Hasher::new();
        assert!(hasher.verify("password", "invalid-hash").is_err());
        assert!(hasher.verify("password", "$abc").is_err());
        assert!(hasher.verify("password", "$abc$def").is_err());
    }

    #[test]
    fn test_pbkdf2_hasher_with_iterations() {
        let hasher = Pbkdf2Hasher::with_iterations(1000);
        let hash = hasher.hash("secret").unwrap();
        let parts: Vec<&str> = hash[1..].splitn(3, '$').collect();
        assert_eq!(parts[0], "1000");
        assert!(hasher.verify("secret", &hash).unwrap());
    }

    #[test]
    fn test_pbkdf2_hasher_empty_password() {
        let hasher = Pbkdf2Hasher::new();
        assert!(hasher.hash("").is_err());
    }

    // --- HmacSigner 测试 ---

    #[test]
    fn test_hmac_signer_sign_not_empty() {
        let signer = HmacSigner::new();
        let mut params = HashMap::new();
        params.insert("name".to_string(), "test".to_string());
        let signature = signer.sign(&params, "secret123");
        assert_eq!(signature.len(), 64);
    }

    #[test]
    fn test_hmac_signer_verify_correct() {
        let signer = HmacSigner::new();
        let mut params = HashMap::new();
        params.insert("name".to_string(), "test".to_string());
        params.insert("age".to_string(), "25".to_string());

        let signature = signer.sign(&params, "mysecret");
        assert!(signer.verify(&params, "mysecret", &signature));
    }

    #[test]
    fn test_hmac_signer_verify_wrong_secret() {
        let signer = HmacSigner::new();
        let mut params = HashMap::new();
        params.insert("name".to_string(), "test".to_string());
        let signature = signer.sign(&params, "correctsecret");
        assert!(!signer.verify(&params, "wrongsecret", &signature));
    }

    #[test]
    fn test_hmac_signer_verify_wrong_signature() {
        let signer = HmacSigner::new();
        let mut params = HashMap::new();
        params.insert("name".to_string(), "test".to_string());
        let valid_sig = signer.sign(&params, "secret");
        let tampered = if let Some(stripped) = valid_sig.strip_prefix('0') {
            format!("1{}", stripped)
        } else {
            format!("0{}", &valid_sig[1..])
        };
        assert!(!signer.verify(&params, "secret", &tampered));
    }

    #[test]
    fn test_hmac_signer_different_params_different_signatures() {
        let signer = HmacSigner::new();
        let mut params1 = HashMap::new();
        params1.insert("a".to_string(), "1".to_string());

        let mut params2 = HashMap::new();
        params2.insert("b".to_string(), "2".to_string());

        let sig1 = signer.sign(&params1, "secret");
        let sig2 = signer.sign(&params2, "secret");
        assert_ne!(sig1, sig2);
    }

    #[test]
    fn test_hmac_signer_param_order_independent() {
        let signer = HmacSigner::new();
        let mut params1 = HashMap::new();
        params1.insert("b".to_string(), "2".to_string());
        params1.insert("a".to_string(), "1".to_string());

        let mut params2 = HashMap::new();
        params2.insert("a".to_string(), "1".to_string());
        params2.insert("b".to_string(), "2".to_string());

        let sig1 = signer.sign(&params1, "secret");
        let sig2 = signer.sign(&params2, "secret");
        assert_eq!(sig1, sig2);
    }

    #[test]
    fn test_hmac_signer_empty_params() {
        let signer = HmacSigner::new();
        let params = HashMap::new();
        let sig = signer.sign(&params, "secret");
        assert_eq!(sig.len(), 64);
        assert!(signer.verify(&params, "secret", &sig));
    }

    // --- 辅助函数测试 ---

    #[test]
    fn test_random_bytes_length() {
        assert_eq!(random_bytes(0).len(), 0);
        assert_eq!(random_bytes(16).len(), 16);
        assert_eq!(random_bytes(100).len(), 100);
    }

    #[test]
    fn test_random_bytes_random() {
        let a = random_bytes(32);
        let b = random_bytes(32);
        assert_ne!(a, b, "随机字节序列应不同");
    }

    #[test]
    fn test_constant_time_eq() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"ab"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
        assert!(constant_time_eq(b"", b""));
    }

    #[test]
    fn test_hex_encode_decode_roundtrip() {
        let original = vec![0x00, 0xff, 0xab, 0x42];
        let encoded = hex_encode(&original);
        let decoded = hex_decode(&encoded).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_hex_decode_invalid() {
        assert!(hex_decode("abc").is_err());
        assert!(hex_decode("xy").is_err());
    }

    // ===== AES-GCM AAD 测试 =====

    #[test]
    fn test_aes_gcm_aad_roundtrip() {
        let key = [0x42u8; 32];
        let crypter = AesGcmCrypter::new(&key);
        let plaintext = b"sensitive data";
        let aad = b"associated metadata";
        let encrypted = crypter.encrypt_with_aad(plaintext, aad).unwrap();
        let decrypted = crypter.decrypt_with_aad(&encrypted, aad).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_aes_gcm_aad_wrong_aad_fails() {
        let key = [0x42u8; 32];
        let crypter = AesGcmCrypter::new(&key);
        let plaintext = b"sensitive data";
        let aad = b"correct aad";
        let encrypted = crypter.encrypt_with_aad(plaintext, aad).unwrap();
        // 使用错误的 AAD 解密应失败
        let result = crypter.decrypt_with_aad(&encrypted, b"wrong aad");
        assert!(result.is_err());
    }

    #[test]
    fn test_aes_gcm_aad_empty_aad_equivalent_to_no_aad() {
        let key = [0x42u8; 32];
        let crypter = AesGcmCrypter::new(&key);
        let plaintext = b"test data";
        // 空 AAD 等价于无 AAD
        let encrypted_no_aad = crypter.encrypt(plaintext).unwrap();
        let encrypted_empty_aad = crypter.encrypt_with_aad(plaintext, b"").unwrap();
        // 两者都应能解密
        assert_eq!(crypter.decrypt(&encrypted_no_aad).unwrap(), plaintext);
        assert_eq!(
            crypter.decrypt_with_aad(&encrypted_empty_aad, b"").unwrap(),
            plaintext
        );
    }

    #[test]
    fn test_aes_gcm_aad_tampered_ciphertext_fails() {
        let key = [0x42u8; 32];
        let crypter = AesGcmCrypter::new(&key);
        let encrypted = crypter.encrypt_with_aad(b"data", b"aad").unwrap();
        let mut tampered = encrypted.clone();
        tampered[15] ^= 0x01;
        assert!(crypter.decrypt_with_aad(&tampered, b"aad").is_err());
    }

    #[test]
    fn test_aes_gcm_aad_empty_plaintext() {
        let key = [0x42u8; 32];
        let crypter = AesGcmCrypter::new(&key);
        let encrypted = crypter.encrypt_with_aad(b"", b"aad").unwrap();
        // nonce(12) + tag(16) = 28
        assert_eq!(encrypted.len(), 28);
        let decrypted = crypter.decrypt_with_aad(&encrypted, b"aad").unwrap();
        assert_eq!(decrypted, b"");
    }

    // ===== RSA-OAEP 测试 =====

    #[test]
    fn test_rsa_oaep_roundtrip() {
        let crypter = RsaOaepCrypter::generate(2048).expect("RSA key generation");
        let plaintext = b"Hello, RSA-OAEP!";
        let encrypted = crypter.encrypt(plaintext).unwrap();
        let decrypted = crypter.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_rsa_oaep_different_ciphertexts_same_plaintext() {
        let crypter = RsaOaepCrypter::generate(2048).unwrap();
        let plaintext = b"same message";
        let enc1 = crypter.encrypt(plaintext).unwrap();
        let enc2 = crypter.encrypt(plaintext).unwrap();
        // OAEP 使用随机填充，相同明文应产生不同密文
        assert_ne!(enc1, enc2);
        // 但两者都能正确解密
        assert_eq!(crypter.decrypt(&enc1).unwrap(), plaintext);
        assert_eq!(crypter.decrypt(&enc2).unwrap(), plaintext);
    }

    #[test]
    fn test_rsa_oaep_empty_plaintext() {
        let crypter = RsaOaepCrypter::generate(2048).unwrap();
        let encrypted = crypter.encrypt(b"").unwrap();
        let decrypted = crypter.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, b"");
    }

    #[test]
    fn test_rsa_oaep_tampered_ciphertext_fails() {
        let crypter = RsaOaepCrypter::generate(2048).unwrap();
        let encrypted = crypter.encrypt(b"secret").unwrap();
        let mut tampered = encrypted.clone();
        tampered[0] ^= 0x01;
        assert!(crypter.decrypt(&tampered).is_err());
    }

    #[test]
    fn test_rsa_oaep_max_message_length() {
        // 2048-bit RSA-OAEP with SHA-256: max message = 2048/8 - 2*32 - 2 = 190 bytes
        let crypter = RsaOaepCrypter::generate(2048).unwrap();
        let plaintext = vec![0xABu8; 190];
        let encrypted = crypter.encrypt(&plaintext).unwrap();
        let decrypted = crypter.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_rsa_oaep_oversized_message_fails() {
        let crypter = RsaOaepCrypter::generate(2048).unwrap();
        // 超过最大消息长度（190 字节 + 1）
        let plaintext = vec![0xABu8; 191];
        assert!(crypter.encrypt(&plaintext).is_err());
    }

    #[test]
    fn test_rsa_oaep_from_keys() {
        let crypter1 = RsaOaepCrypter::generate(2048).unwrap();
        let crypter2 = RsaOaepCrypter::from_keys(
            crypter1.public_key().clone(),
            crypter1.private_key().clone(),
        );
        let plaintext = b"test from_keys";
        let encrypted = crypter2.encrypt(plaintext).unwrap();
        let decrypted = crypter2.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_rsa_oaep_crypter_trait() {
        let crypter = RsaOaepCrypter::generate(2048).unwrap();
        let plaintext = b"trait test";
        let encrypted = Crypter::encrypt(&crypter, plaintext).unwrap();
        let decrypted = Crypter::decrypt(&crypter, &encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    // ===== HMAC 签名验证器测试 =====

    #[test]
    fn test_hmac_signature_verifier_sign_verify() {
        let verifier = HmacSignatureVerifier::new(b"my-secret-key");
        let message = b"important message";
        let signature = verifier.sign(message);
        assert_eq!(signature.len(), 32);
        assert!(verifier.verify(message, &signature));
    }

    #[test]
    fn test_hmac_signature_verifier_wrong_message() {
        let verifier = HmacSignatureVerifier::new(b"key");
        let signature = verifier.sign(b"message1");
        assert!(!verifier.verify(b"message2", &signature));
    }

    #[test]
    fn test_hmac_signature_verifier_wrong_signature() {
        let verifier = HmacSignatureVerifier::new(b"key");
        let signature = verifier.sign(b"message");
        let mut tampered = signature.clone();
        tampered[0] ^= 0x01;
        assert!(!verifier.verify(b"message", &tampered));
    }

    #[test]
    fn test_hmac_signature_verifier_from_key_str() {
        let verifier = HmacSignatureVerifier::from_key_str("string-key");
        let message = b"test";
        let sig = verifier.sign(message);
        assert!(verifier.verify(message, &sig));
    }

    #[test]
    fn test_hmac_signature_verifier_different_keys_different_signatures() {
        let v1 = HmacSignatureVerifier::new(b"key1");
        let v2 = HmacSignatureVerifier::new(b"key2");
        let message = b"same message";
        let sig1 = v1.sign(message);
        let sig2 = v2.sign(message);
        assert_ne!(sig1, sig2);
    }

    #[test]
    fn test_hmac_signature_verifier_empty_message() {
        let verifier = HmacSignatureVerifier::new(b"key");
        let sig = verifier.sign(b"");
        assert_eq!(sig.len(), 32);
        assert!(verifier.verify(b"", &sig));
    }

    #[test]
    fn test_hmac_signature_verifier_wrong_length_signature() {
        let verifier = HmacSignatureVerifier::new(b"key");
        // 长度不对的签名应验证失败
        assert!(!verifier.verify(b"message", b"short"));
        assert!(!verifier.verify(b"message", &[]));
    }

    // ===== 密钥轮换测试 =====

    #[test]
    fn test_key_rotation_initial_key() {
        let mgr = KeyRotationManager::with_initial_key(b"key-v1".to_vec());
        assert_eq!(mgr.current_version(), 1);
        assert_eq!(mgr.version_count(), 1);
        assert_eq!(mgr.versions(), vec![1]);
    }

    #[test]
    fn test_key_rotation_sign_verify_current() {
        let mgr = KeyRotationManager::with_initial_key(b"secret-key".to_vec());
        let message = b"test message";
        let (version, signature) = mgr.sign(message);
        assert_eq!(version, 1);
        assert!(mgr.verify(message, version, &signature));
    }

    #[test]
    fn test_key_rotation_old_version_still_valid() {
        let mut mgr = KeyRotationManager::with_initial_key(b"key-v1".to_vec());
        let message = b"persistent message";
        let (v1, sig1) = mgr.sign(message);
        // 轮换到新密钥
        mgr.rotate_key(b"key-v2".to_vec());
        let (v2, sig2) = mgr.sign(message);
        assert_eq!(v1, 1);
        assert_eq!(v2, 2);
        // 旧版本签名仍应验证通过
        assert!(mgr.verify(message, v1, &sig1));
        // 新版本签名也应验证通过
        assert!(mgr.verify(message, v2, &sig2));
    }

    #[test]
    fn test_key_rotation_max_versions_evicts_oldest() {
        let mut mgr = KeyRotationManager::new(2);
        mgr.rotate_key(b"key-v1".to_vec());
        mgr.rotate_key(b"key-v2".to_vec());
        assert_eq!(mgr.version_count(), 2);
        // 第三次轮换应淘汰 v1
        mgr.rotate_key(b"key-v3".to_vec());
        assert_eq!(mgr.version_count(), 2);
        assert_eq!(mgr.versions(), vec![2, 3]);
        assert!(!mgr.versions().contains(&1));
    }

    #[test]
    fn test_key_rotation_old_version_evicted_fails_verify() {
        let mut mgr = KeyRotationManager::new(2);
        mgr.rotate_key(b"key-v1".to_vec());
        let message = b"test";
        let (v1, sig1) = mgr.sign(message);
        mgr.rotate_key(b"key-v2".to_vec());
        mgr.rotate_key(b"key-v3".to_vec());
        // v1 已被淘汰，验证应失败
        assert!(!mgr.verify(message, v1, &sig1));
    }

    #[test]
    fn test_key_rotation_wrong_version_fails() {
        let mgr = KeyRotationManager::with_initial_key(b"key".to_vec());
        let message = b"test";
        let (_, signature) = mgr.sign(message);
        // 使用不存在的版本号验证应失败
        assert!(!mgr.verify(message, 999, &signature));
    }

    #[test]
    fn test_key_rotation_multiple_rotations() {
        let mut mgr = KeyRotationManager::new(5);
        for i in 1..=4 {
            let key = format!("key-v{}", i);
            let version = mgr.rotate_key(key.as_bytes().to_vec());
            assert_eq!(version, i as u32);
        }
        assert_eq!(mgr.current_version(), 4);
        assert_eq!(mgr.version_count(), 4);
        assert_eq!(mgr.versions(), vec![1, 2, 3, 4]);
    }

    #[test]
    fn test_key_rotation_empty_manager_sign_returns_zero() {
        let mgr = KeyRotationManager::new(3);
        let (version, sig) = mgr.sign(b"message");
        assert_eq!(version, 0);
        assert!(sig.is_empty());
    }

    #[test]
    fn test_key_rotation_verify_with_wrong_signature() {
        let mgr = KeyRotationManager::with_initial_key(b"key".to_vec());
        let message = b"test";
        let (version, _) = mgr.sign(message);
        let wrong_sig = vec![0u8; 32];
        assert!(!mgr.verify(message, version, &wrong_sig));
    }

    #[test]
    fn test_key_rotation_max_versions_min_one() {
        // max_versions = 0 应被提升为 1
        let mut mgr = KeyRotationManager::new(0);
        mgr.rotate_key(b"k1".to_vec());
        mgr.rotate_key(b"k2".to_vec());
        assert_eq!(mgr.version_count(), 1);
        assert_eq!(mgr.versions(), vec![2]);
    }
}
