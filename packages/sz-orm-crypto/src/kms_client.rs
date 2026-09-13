//! KMS（密钥管理服务）客户端
//!
//! 提供 `KmsClient` async trait + `LocalKmsClient` 本地实现 + `DekCache` TTL 缓存。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use parking_lot::RwLock;

use crate::dek_buffer::DekBuffer;

/// KMS 错误
#[derive(Debug, Clone)]
pub enum KmsError {
    /// KMS 不可用
    KmsUnavailable(String),
    /// 密钥版本不存在
    KeyVersionNotFound(String),
    /// 算法不支持
    AlgoNotSupported(String),
    /// TLS 配置无效
    TlsConfigInvalid(String),
    /// 降级超时（超过 24h）
    DegradeTimeout(String),
}

impl std::fmt::Display for KmsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KmsError::KmsUnavailable(msg) => write!(f, "KMS unavailable: {}", msg),
            KmsError::KeyVersionNotFound(msg) => write!(f, "Key version not found: {}", msg),
            KmsError::AlgoNotSupported(msg) => write!(f, "Algorithm not supported: {}", msg),
            KmsError::TlsConfigInvalid(msg) => write!(f, "TLS config invalid: {}", msg),
            KmsError::DegradeTimeout(msg) => write!(f, "Degrade timeout: {}", msg),
        }
    }
}

impl std::error::Error for KmsError {}

/// KMS 客户端 trait
#[async_trait]
pub trait KmsClient: Send + Sync {
    /// 获取 DEK（数据加密密钥）
    async fn get_dek(&self, column: &str, version: u32) -> Result<DekBuffer, KmsError>;

    /// 解包 DEK（从 wrapped DEK 还原明文 DEK）
    async fn unwrap_dek(&self, wrapped_dek: &[u8], version: u32) -> Result<DekBuffer, KmsError>;

    /// 轮换密钥，返回新版本号
    async fn rotate_key(&self, column: &str) -> Result<u32, KmsError>;
}

/// 本地 KMS 客户端（测试/开发用）
///
/// 在内存中存储 DEK，按 `(column, version)` 索引。
pub struct LocalKmsClient {
    keys: RwLock<HashMap<(String, u32), Vec<u8>>>,
    versions: RwLock<HashMap<String, u32>>,
}

impl Default for LocalKmsClient {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalKmsClient {
    /// 创建空本地 KMS
    pub fn new() -> Self {
        Self {
            keys: RwLock::new(HashMap::new()),
            versions: RwLock::new(HashMap::new()),
        }
    }

    /// 预置一个 DEK
    pub fn with_dek(column: &str, version: u32, dek: Vec<u8>) -> Self {
        let kms = Self::new();
        kms.keys.write().insert((column.to_string(), version), dek);
        kms.versions.write().insert(column.to_string(), version);
        kms
    }

    /// 生成随机 32 字节 DEK 并存储
    pub fn generate_dek(&self, column: &str, version: u32) -> DekBuffer {
        use rand::RngCore;
        let mut key = vec![0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut key);
        self.keys
            .write()
            .insert((column.to_string(), version), key.clone());
        self.versions.write().insert(column.to_string(), version);
        DekBuffer::new(key)
    }
}

#[async_trait]
impl KmsClient for LocalKmsClient {
    async fn get_dek(&self, column: &str, version: u32) -> Result<DekBuffer, KmsError> {
        let key = column.to_string();
        self.keys
            .read()
            .get(&(key, version))
            .map(|k| DekBuffer::new(k.clone()))
            .ok_or_else(|| KmsError::KeyVersionNotFound(format!("{}:v{}", column, version)))
    }

    async fn unwrap_dek(&self, wrapped_dek: &[u8], _version: u32) -> Result<DekBuffer, KmsError> {
        Ok(DekBuffer::new(wrapped_dek.to_vec()))
    }

    async fn rotate_key(&self, column: &str) -> Result<u32, KmsError> {
        let mut versions = self.versions.write();
        let new_version = versions.get(column).copied().unwrap_or(0) + 1;
        versions.insert(column.to_string(), new_version);
        drop(versions);
        self.generate_dek(column, new_version);
        Ok(new_version)
    }
}

/// DEK 缓存
///
/// TTL 默认 3600s，缓存键为 `(column, version)`。
pub struct DekCache {
    cache: RwLock<HashMap<(String, u32), (DekBuffer, Instant)>>,
    ttl: Duration,
}

impl Default for DekCache {
    fn default() -> Self {
        Self::new(Duration::from_secs(3600))
    }
}

impl DekCache {
    /// 创建指定 TTL 的缓存
    pub fn new(ttl: Duration) -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
            ttl,
        }
    }

    /// 获取缓存的 DEK（未过期时返回）
    pub fn get(&self, column: &str, version: u32) -> Option<DekBuffer> {
        let cache = self.cache.read();
        if let Some((dek, inserted_at)) = cache.get(&(column.to_string(), version)) {
            if inserted_at.elapsed() < self.ttl {
                return Some(DekBuffer::new(dek.as_bytes().to_vec()));
            }
        }
        None
    }

    /// 缓存 DEK
    pub fn put(&self, column: &str, version: u32, dek: DekBuffer) {
        self.cache
            .write()
            .insert((column.to_string(), version), (dek, Instant::now()));
    }

    /// 清除过期条目
    pub fn evict_expired(&self) {
        let mut cache = self.cache.write();
        cache.retain(|_, (_, inserted_at)| inserted_at.elapsed() < self.ttl);
    }

    /// 缓存条目数
    pub fn len(&self) -> usize {
        self.cache.read().len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// KMS 降级管理器
///
/// KMS 不可达时使用缓存 DEK 继续服务，超过容忍期后拒绝新密钥请求。
pub struct KmsDegradeManager {
    degrade_since: RwLock<Option<Instant>>,
    tolerance: Duration,
}

impl KmsDegradeManager {
    /// 创建降级管理器，默认容忍 24h
    pub fn new(tolerance: Duration) -> Self {
        Self {
            degrade_since: RwLock::new(None),
            tolerance,
        }
    }

    /// 标记 KMS 进入降级模式
    pub fn enter_degrade(&self) {
        *self.degrade_since.write() = Some(Instant::now());
    }

    /// 退出降级模式（KMS 恢复）
    pub fn exit_degrade(&self) {
        *self.degrade_since.write() = None;
    }

    /// 是否允许新密钥请求
    ///
    /// 降级超过容忍期后返回 false。
    pub fn allow_new_key(&self) -> Result<(), KmsError> {
        let degrade_since = self.degrade_since.read();
        if let Some(since) = *degrade_since {
            if since.elapsed() >= self.tolerance {
                return Err(KmsError::DegradeTimeout(format!(
                    "KMS 降级超过 {:?}，拒绝新密钥请求",
                    self.tolerance
                )));
            }
        }
        Ok(())
    }

    /// 是否处于降级模式
    pub fn is_degraded(&self) -> bool {
        self.degrade_since.read().is_some()
    }
}

/// 带缓存的 KMS 客户端
///
/// 包装 `KmsClient` + `DekCache`，优先命中缓存。
pub struct CachedKmsClient {
    inner: Arc<dyn KmsClient>,
    cache: DekCache,
}

impl CachedKmsClient {
    /// 创建带缓存的 KMS 客户端
    pub fn new(inner: Arc<dyn KmsClient>, cache_ttl: Duration) -> Self {
        Self {
            inner,
            cache: DekCache::new(cache_ttl),
        }
    }

    /// 获取 DEK（优先缓存）
    pub async fn get_dek(&self, column: &str, version: u32) -> Result<DekBuffer, KmsError> {
        if let Some(dek) = self.cache.get(column, version) {
            return Ok(dek);
        }
        let dek = self.inner.get_dek(column, version).await?;
        self.cache
            .put(column, version, DekBuffer::new(dek.as_bytes().to_vec()));
        Ok(dek)
    }

    /// 缓存命中数
    pub fn cache_len(&self) -> usize {
        self.cache.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn local_kms_get_dek() {
        let dek_bytes = vec![0x42u8; 32];
        let kms = LocalKmsClient::with_dek("users.ssn", 1, dek_bytes.clone());
        let dek = kms.get_dek("users.ssn", 1).await.unwrap();
        assert_eq!(dek.as_bytes(), &dek_bytes[..]);
    }

    #[tokio::test]
    async fn local_kms_key_not_found() {
        let kms = LocalKmsClient::new();
        let result = kms.get_dek("users.ssn", 1).await;
        assert!(matches!(result, Err(KmsError::KeyVersionNotFound(_))));
    }

    #[tokio::test]
    async fn local_kms_rotate_key() {
        let kms = LocalKmsClient::with_dek("users.ssn", 1, vec![0x42u8; 32]);
        let new_version = kms.rotate_key("users.ssn").await.unwrap();
        assert_eq!(new_version, 2);
        let dek = kms.get_dek("users.ssn", 2).await.unwrap();
        assert_eq!(dek.len(), 32);
    }

    #[tokio::test]
    async fn local_kms_unwrap_dek() {
        let kms = LocalKmsClient::new();
        let wrapped = vec![0xABu8; 32];
        let dek = kms.unwrap_dek(&wrapped, 1).await.unwrap();
        assert_eq!(dek.as_bytes(), &wrapped[..]);
    }

    #[test]
    fn dek_cache_put_get() {
        let cache = DekCache::new(Duration::from_secs(3600));
        let dek = DekBuffer::new(vec![0x42u8; 32]);
        cache.put("users.ssn", 1, dek);
        assert_eq!(cache.len(), 1);
        let retrieved = cache.get("users.ssn", 1).unwrap();
        assert_eq!(retrieved.as_bytes(), &[0x42u8; 32]);
    }

    #[test]
    fn dek_cache_miss() {
        let cache = DekCache::new(Duration::from_secs(3600));
        assert!(cache.get("users.ssn", 1).is_none());
    }

    #[test]
    fn dek_cache_expired() {
        let cache = DekCache::new(Duration::from_millis(1));
        cache.put("users.ssn", 1, DekBuffer::new(vec![0x42u8; 32]));
        std::thread::sleep(Duration::from_millis(10));
        assert!(cache.get("users.ssn", 1).is_none());
    }

    #[test]
    fn dek_cache_evict() {
        let cache = DekCache::new(Duration::from_millis(1));
        cache.put("users.ssn", 1, DekBuffer::new(vec![0x42u8; 32]));
        assert_eq!(cache.len(), 1);
        std::thread::sleep(Duration::from_millis(10));
        cache.evict_expired();
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn kms_degrade_allow_before_timeout() {
        let mgr = KmsDegradeManager::new(Duration::from_secs(60));
        mgr.enter_degrade();
        assert!(mgr.is_degraded());
        assert!(mgr.allow_new_key().is_ok());
    }

    #[test]
    fn kms_degrade_reject_after_timeout() {
        let mgr = KmsDegradeManager::new(Duration::from_millis(1));
        mgr.enter_degrade();
        std::thread::sleep(Duration::from_millis(10));
        assert!(mgr.allow_new_key().is_err());
    }

    #[test]
    fn kms_degrade_exit() {
        let mgr = KmsDegradeManager::new(Duration::from_secs(60));
        mgr.enter_degrade();
        assert!(mgr.is_degraded());
        mgr.exit_degrade();
        assert!(!mgr.is_degraded());
        assert!(mgr.allow_new_key().is_ok());
    }

    #[tokio::test]
    async fn cached_kms_hits_cache() {
        let kms = Arc::new(LocalKmsClient::with_dek("col", 1, vec![0x42u8; 32]));
        let cached = CachedKmsClient::new(kms, Duration::from_secs(3600));
        let _dek1 = cached.get_dek("col", 1).await.unwrap();
        assert_eq!(cached.cache_len(), 1);
        let _dek2 = cached.get_dek("col", 1).await.unwrap();
        assert_eq!(cached.cache_len(), 1);
    }
}
