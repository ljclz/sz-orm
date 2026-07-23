//! 多因素认证（MFA）框架
//!
//! 实现基于 TOTP（RFC 6238）的 MFA 验证：
//! - 生成密钥和 otpauth URI
//! - 生成当前时间窗口的 6 位 TOTP 码
//! - 验证用户提交的 TOTP 码（允许 ±1 个时间窗口漂移）
//!
//! TOTP 算法基于 HOTP（RFC 4226），使用 HMAC-SHA1 和 30 秒时间步长。

use crate::error::AuthError;
use hmac::{Hmac, Mac};
use sha1::Sha1;

type HmacSha1 = Hmac<Sha1>;

/// TOTP 时间步长（秒），RFC 6239 默认 30 秒
const TIME_STEP: u64 = 30;
/// TOTP 码位数
const CODE_DIGITS: usize = 6;
/// 允许的时间窗口漂移（前后各 1 个窗口）
const ALLOWED_DRIFT: u64 = 1;

/// MFA 密钥（Base32 编码的随机字节）
#[derive(Debug, Clone)]
pub struct MfaSecret {
    /// Base32 编码的密钥
    pub base32_secret: String,
    /// 关联的账户名
    pub account: String,
    /// 发行方名称
    pub issuer: String,
}

impl MfaSecret {
    /// 从原始字节创建 MFA 密钥
    pub fn new(account: impl Into<String>, issuer: impl Into<String>) -> Self {
        let raw = generate_random_bytes(20);
        Self {
            base32_secret: base32_encode(&raw),
            account: account.into(),
            issuer: issuer.into(),
        }
    }

    /// 从已有 Base32 密钥创建
    pub fn from_base32(
        base32_secret: impl Into<String>,
        account: impl Into<String>,
        issuer: impl Into<String>,
    ) -> Self {
        Self {
            base32_secret: base32_secret.into(),
            account: account.into(),
            issuer: issuer.into(),
        }
    }

    /// 生成 otpauth URI（用于二维码扫描）
    pub fn to_uri(&self) -> String {
        format!(
            "otpauth://totp/{}:{}?secret={}&issuer={}",
            self.issuer, self.account, self.base32_secret, self.issuer
        )
    }
}

/// TOTP 验证器
pub struct TotpVerifier {
    time_step: u64,
    digits: usize,
    drift: u64,
}

impl TotpVerifier {
    /// 创建默认 TOTP 验证器（30 秒步长，6 位码，±1 窗口漂移）
    pub fn new() -> Self {
        Self {
            time_step: TIME_STEP,
            digits: CODE_DIGITS,
            drift: ALLOWED_DRIFT,
        }
    }

    /// 配置时间步长
    pub fn with_time_step(mut self, step: u64) -> Self {
        self.time_step = step.max(1);
        self
    }

    /// 配置允许的时间窗口漂移
    pub fn with_drift(mut self, drift: u64) -> Self {
        self.drift = drift;
        self
    }

    /// 生成指定时间戳的 TOTP 码
    pub fn generate_at(&self, base32_secret: &str, timestamp: u64) -> String {
        let counter = timestamp / self.time_step;
        self.generate_hotp(base32_secret, counter)
    }

    /// 生成当前时间的 TOTP 码
    pub fn generate_now(&self, base32_secret: &str) -> String {
        self.generate_at(base32_secret, current_secs())
    }

    /// 验证 TOTP 码（允许 ±drift 个时间窗口漂移）
    pub fn verify(&self, base32_secret: &str, code: &str) -> bool {
        self.verify_at(base32_secret, code, current_secs())
    }

    /// 验证指定时间戳的 TOTP 码
    pub fn verify_at(&self, base32_secret: &str, code: &str, timestamp: u64) -> bool {
        let counter = timestamp / self.time_step;
        // 检查当前窗口及前后 drift 个窗口
        for offset in 0..=self.drift {
            let test_counter = counter.saturating_sub(offset);
            if constant_time_eq(
                self.generate_hotp(base32_secret, test_counter).as_bytes(),
                code.as_bytes(),
            ) {
                return true;
            }
            if offset > 0 {
                let test_counter = counter.saturating_add(offset);
                if constant_time_eq(
                    self.generate_hotp(base32_secret, test_counter).as_bytes(),
                    code.as_bytes(),
                ) {
                    return true;
                }
            }
        }
        false
    }

    /// HOTP 算法（RFC 4226）
    fn generate_hotp(&self, base32_secret: &str, counter: u64) -> String {
        let key = base32_decode(base32_secret).unwrap_or_default();
        if key.is_empty() {
            return "0".repeat(self.digits);
        }

        let mut mac = match <HmacSha1 as Mac>::new_from_slice(&key) {
            Ok(m) => m,
            Err(_) => return "0".repeat(self.digits),
        };

        let counter_bytes = counter.to_be_bytes();
        mac.update(&counter_bytes);
        let hash = mac.finalize().into_bytes();

        // Dynamic truncation
        let offset = (hash[hash.len() - 1] & 0x0F) as usize;
        let truncated: u32 = (((hash[offset] & 0x7F) as u32) << 24)
            | ((hash[offset + 1] as u32) << 16)
            | ((hash[offset + 2] as u32) << 8)
            | (hash[offset + 3] as u32);

        let code = truncated % (10u32.pow(self.digits as u32));
        format!("{:0width$}", code, width = self.digits)
    }
}

impl Default for TotpVerifier {
    fn default() -> Self {
        Self::new()
    }
}

/// MFA 管理器：管理用户的 MFA 密钥和验证状态
pub struct MfaManager {
    verifier: TotpVerifier,
    secrets: std::sync::Mutex<std::collections::HashMap<String, MfaSecret>>,
}

impl MfaManager {
    pub fn new() -> Self {
        Self {
            verifier: TotpVerifier::new(),
            secrets: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// 为用户生成新的 MFA 密钥
    pub fn generate_secret(
        &self,
        user_id: &str,
        account: impl Into<String>,
        issuer: impl Into<String>,
    ) -> MfaSecret {
        let secret = MfaSecret::new(account, issuer);
        self.secrets
            .lock()
            .unwrap()
            .insert(user_id.to_string(), secret.clone());
        secret
    }

    /// 为用户绑定已有密钥
    pub fn bind_secret(&self, user_id: &str, secret: MfaSecret) {
        self.secrets
            .lock()
            .unwrap()
            .insert(user_id.to_string(), secret);
    }

    /// 验证用户的 TOTP 码
    pub fn verify(&self, user_id: &str, code: &str) -> Result<bool, AuthError> {
        let secrets = self.secrets.lock().unwrap();
        let secret = secrets
            .get(user_id)
            .ok_or_else(|| AuthError::Config(format!("No MFA secret for user: {}", user_id)))?;
        Ok(self.verifier.verify(&secret.base32_secret, code))
    }

    /// 生成用户当前的 TOTP 码（用于测试或重置流程）
    pub fn generate_code(&self, user_id: &str) -> Result<String, AuthError> {
        let secrets = self.secrets.lock().unwrap();
        let secret = secrets
            .get(user_id)
            .ok_or_else(|| AuthError::Config(format!("No MFA secret for user: {}", user_id)))?;
        Ok(self.verifier.generate_now(&secret.base32_secret))
    }

    /// 移除用户的 MFA 密钥
    pub fn remove_secret(&self, user_id: &str) -> bool {
        self.secrets.lock().unwrap().remove(user_id).is_some()
    }

    /// 检查用户是否已绑定 MFA
    pub fn has_mfa(&self, user_id: &str) -> bool {
        self.secrets.lock().unwrap().contains_key(user_id)
    }

    /// 获取用户 MFA 密钥的 otpauth URI
    pub fn get_uri(&self, user_id: &str) -> Result<String, AuthError> {
        let secrets = self.secrets.lock().unwrap();
        let secret = secrets
            .get(user_id)
            .ok_or_else(|| AuthError::Config(format!("No MFA secret for user: {}", user_id)))?;
        Ok(secret.to_uri())
    }
}

impl Default for MfaManager {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// 辅助函数
// ============================================================================

fn current_secs() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn generate_random_bytes(len: usize) -> Vec<u8> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut result = Vec::with_capacity(len);
    let mut seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    for i in 0..len {
        let mut hasher = DefaultHasher::new();
        seed.wrapping_add(i as u128).hash(&mut hasher);
        let h = hasher.finish();
        result.push((h & 0xFF) as u8);
        seed = seed.wrapping_add(h as u128);
    }
    result
}

/// Base32 编码（RFC 4648，无填充）
fn base32_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut result = String::new();
    let mut buffer: u32 = 0;
    let mut bits_left = 0;
    for &byte in data {
        buffer = (buffer << 8) | (byte as u32);
        bits_left += 8;
        while bits_left >= 5 {
            bits_left -= 5;
            let idx = ((buffer >> bits_left) & 0x1F) as usize;
            result.push(ALPHABET[idx] as char);
        }
    }
    if bits_left > 0 {
        let idx = ((buffer << (5 - bits_left)) & 0x1F) as usize;
        result.push(ALPHABET[idx] as char);
    }
    result
}

/// Base32 解码（RFC 4648，无填充）
fn base32_decode(data: &str) -> Result<Vec<u8>, ()> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut result = Vec::new();
    let mut buffer: u32 = 0;
    let mut bits_left: u32 = 0;
    for ch in data.chars() {
        let upper = ch.to_ascii_uppercase();
        let idx = ALPHABET.iter().position(|&c| c == upper as u8).ok_or(())?;
        buffer = (buffer << 5) | (idx as u32);
        bits_left += 5;
        if bits_left >= 8 {
            bits_left -= 8;
            let byte = ((buffer >> bits_left) & 0xFF) as u8;
            result.push(byte);
        }
    }
    Ok(result)
}

/// 常量时间比较
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    use subtle::ConstantTimeEq;
    a.ct_eq(b).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base32_encode_decode_roundtrip() {
        let original = b"Hello, MFA!";
        let encoded = base32_encode(original);
        let decoded = base32_decode(&encoded).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_base32_encode_known() {
        // RFC 4648 测试向量（去除填充）
        assert_eq!(base32_encode(b""), "");
        assert_eq!(base32_encode(b"f"), "MY");
        assert_eq!(base32_encode(b"fo"), "MZXQ");
        assert_eq!(base32_encode(b"foo"), "MZXW6");
        assert_eq!(base32_encode(b"foob"), "MZXW6YQ");
        assert_eq!(base32_encode(b"fooba"), "MZXW6YTB");
        assert_eq!(base32_encode(b"foobar"), "MZXW6YTBOI");
    }

    #[test]
    fn test_base32_decode_known() {
        assert_eq!(base32_decode("").unwrap(), b"");
        assert_eq!(base32_decode("MY").unwrap(), b"f");
        assert_eq!(base32_decode("MZXQ").unwrap(), b"fo");
        assert_eq!(base32_decode("MZXW6").unwrap(), b"foo");
    }

    #[test]
    fn test_base32_decode_lowercase() {
        assert_eq!(base32_decode("mzxw6").unwrap(), b"foo");
    }

    #[test]
    fn test_base32_decode_invalid_char() {
        assert!(base32_decode("INVALID!").is_err());
        assert!(base32_decode("1").is_err());
    }

    #[test]
    fn test_mfa_secret_new() {
        let secret = MfaSecret::new("user@test.com", "TestApp");
        assert!(!secret.base32_secret.is_empty());
        assert_eq!(secret.account, "user@test.com");
        assert_eq!(secret.issuer, "TestApp");
    }

    #[test]
    fn test_mfa_secret_from_base32() {
        let secret = MfaSecret::from_base32("JBSWY3DPEHPK3PXP", "alice", "MyApp");
        assert_eq!(secret.base32_secret, "JBSWY3DPEHPK3PXP");
        assert_eq!(secret.account, "alice");
    }

    #[test]
    fn test_mfa_secret_to_uri() {
        let secret = MfaSecret::from_base32("JBSWY3DPEHPK3PXP", "alice", "MyApp");
        let uri = secret.to_uri();
        assert!(uri.starts_with("otpauth://totp/MyApp:alice?"));
        assert!(uri.contains("secret=JBSWY3DPEHPK3PXP"));
        assert!(uri.contains("issuer=MyApp"));
    }

    #[test]
    fn test_totp_verifier_generate_format() {
        let verifier = TotpVerifier::new();
        let code = verifier.generate_now("JBSWY3DPEHPK3PXP");
        assert_eq!(code.len(), 6);
        assert!(code.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn test_totp_verifier_generate_at_deterministic() {
        let verifier = TotpVerifier::new();
        let code1 = verifier.generate_at("JBSWY3DPEHPK3PXP", 1000000);
        let code2 = verifier.generate_at("JBSWY3DPEHPK3PXP", 1000000);
        assert_eq!(code1, code2);
    }

    #[test]
    fn test_totp_verifier_generate_different_timestamps() {
        let verifier = TotpVerifier::new();
        // 1000000 / 30 = 33333, 1000030 / 30 = 33334（不同计数器）
        let code1 = verifier.generate_at("JBSWY3DPEHPK3PXP", 1000000);
        let code2 = verifier.generate_at("JBSWY3DPEHPK3PXP", 1000030);
        assert_ne!(code1, code2);
    }

    #[test]
    fn test_totp_verifier_verify_correct() {
        let verifier = TotpVerifier::new();
        let timestamp = 1000000u64;
        let code = verifier.generate_at("JBSWY3DPEHPK3PXP", timestamp);
        assert!(verifier.verify_at("JBSWY3DPEHPK3PXP", &code, timestamp));
    }

    #[test]
    fn test_totp_verifier_verify_wrong_code() {
        let verifier = TotpVerifier::new();
        assert!(!verifier.verify_at("JBSWY3DPEHPK3PXP", "000000", 1000000));
    }

    #[test]
    fn test_totp_verifier_verify_within_drift() {
        let verifier = TotpVerifier::new();
        let timestamp = 1000000u64;
        let code = verifier.generate_at("JBSWY3DPEHPK3PXP", timestamp);
        // 前后 1 个窗口（30 秒）内应验证通过
        assert!(verifier.verify_at("JBSWY3DPEHPK3PXP", &code, timestamp + 30));
        assert!(verifier.verify_at("JBSWY3DPEHPK3PXP", &code, timestamp - 30));
    }

    #[test]
    fn test_totp_verifier_verify_outside_drift() {
        let verifier = TotpVerifier::new();
        let timestamp = 1000000u64;
        let code = verifier.generate_at("JBSWY3DPEHPK3PXP", timestamp);
        // 超出漂移范围（2 个窗口 = 60 秒）
        assert!(!verifier.verify_at("JBSWY3DPEHPK3PXP", &code, timestamp + 60));
        assert!(!verifier.verify_at("JBSWY3DPEHPK3PXP", &code, timestamp - 60));
    }

    #[test]
    fn test_totp_verifier_different_secrets_different_codes() {
        let verifier = TotpVerifier::new();
        let code1 = verifier.generate_at("JBSWY3DPEHPK3PXP", 1000000);
        let code2 = verifier.generate_at("JBSWY3DPEHPK3PXP", 1000000);
        let code3 = verifier.generate_at("GEZDGNBVGY3TQOJQ", 1000000);
        assert_eq!(code1, code2);
        assert_ne!(code1, code3);
    }

    #[test]
    fn test_totp_verifier_with_drift_zero() {
        let verifier = TotpVerifier::new().with_drift(0);
        let timestamp = 1000000u64;
        let code = verifier.generate_at("JBSWY3DPEHPK3PXP", timestamp);
        assert!(verifier.verify_at("JBSWY3DPEHPK3PXP", &code, timestamp));
        // 0 漂移：前后 30 秒应失败
        assert!(!verifier.verify_at("JBSWY3DPEHPK3PXP", &code, timestamp + 30));
    }

    #[test]
    fn test_totp_verifier_with_custom_time_step() {
        let verifier = TotpVerifier::new().with_time_step(60);
        let timestamp = 1000000u64;
        let code = verifier.generate_at("JBSWY3DPEHPK3PXP", timestamp);
        assert_eq!(code.len(), 6);
        assert!(verifier.verify_at("JBSWY3DPEHPK3PXP", &code, timestamp));
        // 60 秒步长：30 秒后仍在同一窗口
        assert!(verifier.verify_at("JBSWY3DPEHPK3PXP", &code, timestamp + 30));
    }

    #[test]
    fn test_totp_verifier_empty_secret() {
        let verifier = TotpVerifier::new();
        let code = verifier.generate_at("", 1000000);
        assert_eq!(code, "000000");
    }

    #[test]
    fn test_mfa_manager_generate_secret() {
        let mgr = MfaManager::new();
        let secret = mgr.generate_secret("user1", "user1@test.com", "TestApp");
        assert!(!secret.base32_secret.is_empty());
        assert!(mgr.has_mfa("user1"));
    }

    #[test]
    fn test_mfa_manager_verify_correct() {
        let mgr = MfaManager::new();
        mgr.generate_secret("user1", "user1@test.com", "TestApp");
        let code = mgr.generate_code("user1").unwrap();
        assert!(mgr.verify("user1", &code).unwrap());
    }

    #[test]
    fn test_mfa_manager_verify_wrong_code() {
        let mgr = MfaManager::new();
        mgr.generate_secret("user1", "user1@test.com", "TestApp");
        assert!(!mgr.verify("user1", "000000").unwrap());
    }

    #[test]
    fn test_mfa_manager_no_secret_errors() {
        let mgr = MfaManager::new();
        assert!(mgr.verify("unknown", "123456").is_err());
        assert!(mgr.generate_code("unknown").is_err());
        assert!(mgr.get_uri("unknown").is_err());
    }

    #[test]
    fn test_mfa_manager_remove_secret() {
        let mgr = MfaManager::new();
        mgr.generate_secret("user1", "user1@test.com", "TestApp");
        assert!(mgr.has_mfa("user1"));
        assert!(mgr.remove_secret("user1"));
        assert!(!mgr.has_mfa("user1"));
    }

    #[test]
    fn test_mfa_manager_remove_nonexistent() {
        let mgr = MfaManager::new();
        assert!(!mgr.remove_secret("unknown"));
    }

    #[test]
    fn test_mfa_manager_bind_secret() {
        let mgr = MfaManager::new();
        let secret = MfaSecret::from_base32("JBSWY3DPEHPK3PXP", "bob", "App");
        mgr.bind_secret("bob", secret);
        assert!(mgr.has_mfa("bob"));
        let code = mgr.generate_code("bob").unwrap();
        assert!(mgr.verify("bob", &code).unwrap());
    }

    #[test]
    fn test_mfa_manager_get_uri() {
        let mgr = MfaManager::new();
        mgr.generate_secret("user1", "user1@test.com", "TestApp");
        let uri = mgr.get_uri("user1").unwrap();
        assert!(uri.starts_with("otpauth://totp/"));
        assert!(uri.contains("user1@test.com"));
    }

    #[test]
    fn test_mfa_manager_default() {
        let mgr = MfaManager::default();
        assert!(!mgr.has_mfa("anyone"));
    }
}
