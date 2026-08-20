//! 密钥管理：密钥生成、轮换、存储、检索与版本管理
//!
//! 本模块提供完整的密钥生命周期管理框架，支持：
//!
//! - **密钥生成**（[`KeyGenerator`]）：按算法（AES-256-GCM、HMAC-SHA256、RSA）生成随机密钥
//! - **密钥存储**（[`KeyStore`]）：内存密钥库，支持按 ID/名称/版本检索
//! - **密钥轮换**（[`RotationPolicy`]）：按时间间隔或使用次数触发轮换
//! - **版本管理**（[`KeyVault`]）：同一密钥名下多版本共存，支持平滑过渡
//! - **审计日志**（[`KeyAuditLog`]）：记录密钥生命周期事件
//! - **密钥状态**（[`KeyStatus`]）：Active/Deprecated/Revoked/Expired 状态机
//!
//! ## 示例
//!
//! ```rust
//! use sz_orm_crypto::key_management::{KeyVault, KeyAlgorithm, KeyPurpose, RotationPolicy};
//! use std::time::Duration;
//!
//! let vault = KeyVault::new(RotationPolicy::TimeInterval(Duration::from_secs(86400)));
//! let key_id = vault.generate_key("db-encryption", KeyAlgorithm::Aes256Gcm, KeyPurpose::Encryption).unwrap();
//! let entry = vault.get_key(&key_id).unwrap();
//! assert_eq!(entry.metadata.algorithm, KeyAlgorithm::Aes256Gcm);
//! ```

use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fmt;
use std::sync::RwLock;
use std::time::{Duration, SystemTime};

// ============================================================================
// 密钥算法与用途
// ============================================================================

/// 密钥算法类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KeyAlgorithm {
    /// AES-256-GCM 对称加密密钥（32 字节）
    Aes256Gcm,
    /// HMAC-SHA256 签名密钥（32 字节）
    HmacSha256,
    /// RSA 2048 位非对称密钥（256 字节私钥材料占位）
    Rsa2048,
    /// RSA 4096 位非对称密钥（512 字节私钥材料占位）
    Rsa4096,
}

impl KeyAlgorithm {
    /// 返回算法的字符串标识
    pub fn as_str(&self) -> &'static str {
        match self {
            KeyAlgorithm::Aes256Gcm => "aes-256-gcm",
            KeyAlgorithm::HmacSha256 => "hmac-sha256",
            KeyAlgorithm::Rsa2048 => "rsa-2048",
            KeyAlgorithm::Rsa4096 => "rsa-4096",
        }
    }

    /// 返回密钥材料的字节长度
    pub fn key_length(&self) -> usize {
        match self {
            KeyAlgorithm::Aes256Gcm => 32,
            KeyAlgorithm::HmacSha256 => 32,
            KeyAlgorithm::Rsa2048 => 256,
            KeyAlgorithm::Rsa4096 => 512,
        }
    }

    /// 是否为对称算法
    pub fn is_symmetric(&self) -> bool {
        matches!(self, KeyAlgorithm::Aes256Gcm | KeyAlgorithm::HmacSha256)
    }
}

impl fmt::Display for KeyAlgorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 密钥用途
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KeyPurpose {
    /// 仅用于加密/解密
    Encryption,
    /// 仅用于签名/验证
    Signing,
    /// 加密与签名均可
    EncryptionAndSigning,
}

impl KeyPurpose {
    /// 返回用途的字符串标识
    pub fn as_str(&self) -> &'static str {
        match self {
            KeyPurpose::Encryption => "encryption",
            KeyPurpose::Signing => "signing",
            KeyPurpose::EncryptionAndSigning => "encryption+signing",
        }
    }

    /// 是否可用于加密
    pub fn can_encrypt(&self) -> bool {
        matches!(
            self,
            KeyPurpose::Encryption | KeyPurpose::EncryptionAndSigning
        )
    }

    /// 是否可用于签名
    pub fn can_sign(&self) -> bool {
        matches!(self, KeyPurpose::Signing | KeyPurpose::EncryptionAndSigning)
    }
}

impl fmt::Display for KeyPurpose {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ============================================================================
// 密钥状态
// ============================================================================

/// 密钥生命周期状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KeyStatus {
    /// 活跃：可用于加密/签名与解密/验证
    Active,
    /// 已弃用：不可用于新操作，但可用于旧数据解密/验证
    Deprecated,
    /// 已吊销：不可用于任何操作
    Revoked,
    /// 已过期：不可用于新操作，需轮换
    Expired,
}

impl KeyStatus {
    /// 返回状态的字符串标识
    pub fn as_str(&self) -> &'static str {
        match self {
            KeyStatus::Active => "active",
            KeyStatus::Deprecated => "deprecated",
            KeyStatus::Revoked => "revoked",
            KeyStatus::Expired => "expired",
        }
    }

    /// 是否可用于新操作（加密/签名）
    pub fn is_usable_for_new(&self) -> bool {
        matches!(self, KeyStatus::Active)
    }

    /// 是否可用于旧操作（解密/验证）
    pub fn is_usable_for_old(&self) -> bool {
        matches!(
            self,
            KeyStatus::Active | KeyStatus::Deprecated | KeyStatus::Expired
        )
    }

    /// 是否已完全不可用
    pub fn is_dead(&self) -> bool {
        matches!(self, KeyStatus::Revoked)
    }
}

impl fmt::Display for KeyStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ============================================================================
// 密钥元数据与条目
// ============================================================================

/// 密钥元数据，描述密钥的属性但不包含密钥材料
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyMetadata {
    /// 密钥唯一标识（UUID 风格）
    pub key_id: String,
    /// 密钥名称（同一名称可有多个版本）
    pub name: String,
    /// 密钥算法
    pub algorithm: KeyAlgorithm,
    /// 密钥用途
    pub purpose: KeyPurpose,
    /// 版本号（从 1 开始递增）
    pub version: u32,
    /// 创建时间
    pub created_at: SystemTime,
    /// 过期时间（None 表示永不过期）
    pub expires_at: Option<SystemTime>,
    /// 密钥状态
    pub status: KeyStatus,
    /// 人类可读的描述
    pub description: String,
    /// 标签列表，便于分类检索
    pub tags: Vec<String>,
}

impl KeyMetadata {
    /// 创建新元数据
    fn new(
        key_id: String,
        name: String,
        algorithm: KeyAlgorithm,
        purpose: KeyPurpose,
        version: u32,
    ) -> Self {
        Self {
            key_id,
            name,
            algorithm,
            purpose,
            version,
            created_at: SystemTime::now(),
            expires_at: None,
            status: KeyStatus::Active,
            description: String::new(),
            tags: Vec::new(),
        }
    }

    /// 设置过期时间
    pub fn with_expiry(mut self, expires_at: SystemTime) -> Self {
        self.expires_at = Some(expires_at);
        self
    }

    /// 设置描述
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// 添加标签
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// 检查密钥是否已过期
    pub fn is_expired(&self) -> bool {
        match self.expires_at {
            Some(expiry) => SystemTime::now() >= expiry,
            None => false,
        }
    }

    /// 返回密钥已存活时长
    pub fn age(&self) -> Duration {
        SystemTime::now()
            .duration_since(self.created_at)
            .unwrap_or_default()
    }
}

/// 密钥条目，包含元数据与密钥材料
#[derive(Debug, Clone)]
pub struct KeyEntry {
    /// 密钥元数据
    pub metadata: KeyMetadata,
    /// 密钥材料（字节）
    pub material: Vec<u8>,
    /// 密钥指纹（SHA-256 十六进制，用于无泄露标识）
    pub fingerprint: String,
}

impl KeyEntry {
    /// 计算密钥材料的 SHA-256 指纹（十六进制）
    pub fn compute_fingerprint(material: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(material);
        let result = hasher.finalize();
        result.iter().map(|b| format!("{:02x}", b)).collect()
    }

    /// 验证密钥材料与指纹是否匹配
    pub fn verify_fingerprint(&self) -> bool {
        Self::compute_fingerprint(&self.material) == self.fingerprint
    }
}

// ============================================================================
// 密钥生成器
// ============================================================================

/// 密钥生成器，使用操作系统安全随机源（OsRng）
#[derive(Debug, Default)]
pub struct KeyGenerator;

impl KeyGenerator {
    /// 创建新生成器
    pub fn new() -> Self {
        Self
    }

    /// 生成指定算法的随机密钥材料
    pub fn generate_material(&self, algorithm: KeyAlgorithm) -> Vec<u8> {
        let mut material = vec![0u8; algorithm.key_length()];
        OsRng.fill_bytes(&mut material);
        material
    }

    /// 生成完整密钥条目（含元数据与指纹）
    pub fn generate(
        &self,
        name: impl Into<String>,
        algorithm: KeyAlgorithm,
        purpose: KeyPurpose,
        version: u32,
    ) -> KeyEntry {
        let material = self.generate_material(algorithm);
        let fingerprint = KeyEntry::compute_fingerprint(&material);
        let key_id = Self::generate_key_id();
        let metadata = KeyMetadata::new(key_id, name.into(), algorithm, purpose, version);
        KeyEntry {
            metadata,
            material,
            fingerprint,
        }
    }

    /// 生成随机密钥 ID（基于 OsRng 的十六进制字符串）
    pub fn generate_key_id() -> String {
        let mut bytes = [0u8; 16];
        OsRng.fill_bytes(&mut bytes);
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }
}

// ============================================================================
// 轮换策略
// ============================================================================

/// 密钥轮换策略
#[derive(Debug, Clone, Default)]
pub enum RotationPolicy {
    /// 按时间间隔轮换
    TimeInterval(Duration),
    /// 按使用次数轮换
    UsageCount(u64),
    /// 时间间隔或使用次数任一满足即轮换
    TimeIntervalOrUsage(Duration, u64),
    /// 不自动轮换（仅手动轮换）
    #[default]
    Never,
}

impl RotationPolicy {
    /// 判断是否需要轮换
    ///
    /// # 参数
    /// - `age`：密钥已存活时长
    /// - `usage`：密钥已使用次数
    pub fn needs_rotation(&self, age: Duration, usage: u64) -> bool {
        match self {
            RotationPolicy::TimeInterval(interval) => age >= *interval,
            RotationPolicy::UsageCount(count) => usage >= *count,
            RotationPolicy::TimeIntervalOrUsage(interval, count) => {
                age >= *interval || usage >= *count
            }
            RotationPolicy::Never => false,
        }
    }

    /// 返回策略的字符串标识
    pub fn as_str(&self) -> &'static str {
        match self {
            RotationPolicy::TimeInterval(_) => "time-interval",
            RotationPolicy::UsageCount(_) => "usage-count",
            RotationPolicy::TimeIntervalOrUsage(_, _) => "time-or-usage",
            RotationPolicy::Never => "never",
        }
    }
}

// ============================================================================
// 密钥存储
// ============================================================================

/// 密钥存储（内存实现）
///
/// 维护密钥 ID 到条目的映射，以及密钥名称到版本列表的索引。
/// 所有字段通过 `parking_lot::RwLock` 保护，支持并发访问。
#[derive(Debug, Default)]
pub struct KeyStore {
    /// key_id -> KeyEntry
    keys: HashMap<String, KeyEntry>,
    /// name -> Vec<key_id>（按版本排序，最新在前）
    name_index: HashMap<String, Vec<String>>,
}

impl KeyStore {
    /// 创建空存储
    pub fn new() -> Self {
        Self::default()
    }

    /// 存入密钥条目
    pub fn put(&mut self, entry: KeyEntry) -> String {
        let key_id = entry.metadata.key_id.clone();
        let name = entry.metadata.name.clone();
        self.keys.insert(key_id.clone(), entry);
        let list = self.name_index.entry(name).or_default();
        list.push(key_id.clone());
        // 按版本降序排序（最新在前）
        list.sort_by_key(|id| {
            std::cmp::Reverse(self.keys.get(id).map(|e| e.metadata.version).unwrap_or(0))
        });
        key_id
    }

    /// 按 ID 获取密钥
    pub fn get(&self, key_id: &str) -> Option<&KeyEntry> {
        self.keys.get(key_id)
    }

    /// 按 ID 获取密钥（可变引用）
    pub fn get_mut(&mut self, key_id: &str) -> Option<&mut KeyEntry> {
        self.keys.get_mut(key_id)
    }

    /// 按名称获取最新版本密钥
    pub fn get_latest_by_name(&self, name: &str) -> Option<&KeyEntry> {
        let list = self.name_index.get(name)?;
        let latest_id = list.first()?;
        self.keys.get(latest_id)
    }

    /// 按名称与版本获取密钥
    pub fn get_by_name_and_version(&self, name: &str, version: u32) -> Option<&KeyEntry> {
        let list = self.name_index.get(name)?;
        for id in list {
            if let Some(entry) = self.keys.get(id) {
                if entry.metadata.version == version {
                    return Some(entry);
                }
            }
        }
        None
    }

    /// 按名称获取所有版本
    pub fn get_all_versions(&self, name: &str) -> Vec<&KeyEntry> {
        let list = match self.name_index.get(name) {
            Some(l) => l,
            None => return Vec::new(),
        };
        list.iter().filter_map(|id| self.keys.get(id)).collect()
    }

    /// 按状态过滤密钥
    pub fn get_by_status(&self, status: KeyStatus) -> Vec<&KeyEntry> {
        self.keys
            .values()
            .filter(|e| e.metadata.status == status)
            .collect()
    }

    /// 按标签过滤密钥
    pub fn get_by_tag(&self, tag: &str) -> Vec<&KeyEntry> {
        self.keys
            .values()
            .filter(|e| e.metadata.tags.iter().any(|t| t == tag))
            .collect()
    }

    /// 移除密钥（按 ID）
    pub fn remove(&mut self, key_id: &str) -> Option<KeyEntry> {
        let entry = self.keys.remove(key_id)?;
        let list = self.name_index.get_mut(&entry.metadata.name)?;
        list.retain(|id| id != key_id);
        if list.is_empty() {
            self.name_index.remove(&entry.metadata.name);
        }
        Some(entry)
    }

    /// 返回存储的密钥总数
    pub fn count(&self) -> usize {
        self.keys.len()
    }

    /// 返回指定名称的版本数
    pub fn version_count(&self, name: &str) -> usize {
        self.name_index.get(name).map_or(0, |l| l.len())
    }

    /// 返回所有密钥名称
    pub fn names(&self) -> Vec<String> {
        self.name_index.keys().cloned().collect()
    }

    /// 更新密钥状态
    pub fn set_status(&mut self, key_id: &str, status: KeyStatus) -> bool {
        if let Some(entry) = self.keys.get_mut(key_id) {
            entry.metadata.status = status;
            true
        } else {
            false
        }
    }
}

// ============================================================================
// 密钥事件与审计日志
// ============================================================================

/// 密钥生命周期事件类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyEvent {
    /// 密钥创建
    Created,
    /// 密钥轮换
    Rotated,
    /// 密钥弃用
    Deprecated,
    /// 密钥吊销
    Revoked,
    /// 密钥过期
    Expired,
    /// 密钥检索
    Retrieved,
    /// 密钥删除
    Deleted,
}

impl KeyEvent {
    /// 返回事件的字符串标识
    pub fn as_str(&self) -> &'static str {
        match self {
            KeyEvent::Created => "created",
            KeyEvent::Rotated => "rotated",
            KeyEvent::Deprecated => "deprecated",
            KeyEvent::Revoked => "revoked",
            KeyEvent::Expired => "expired",
            KeyEvent::Retrieved => "retrieved",
            KeyEvent::Deleted => "deleted",
        }
    }
}

/// 密钥审计日志条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyAuditEntry {
    /// 事件时间戳
    pub timestamp: SystemTime,
    /// 事件类型
    pub event: KeyEvent,
    /// 相关密钥 ID
    pub key_id: String,
    /// 密钥名称
    pub key_name: String,
    /// 事件详情
    pub details: String,
}

impl KeyAuditEntry {
    /// 创建审计条目
    fn new(event: KeyEvent, key_id: String, key_name: String, details: impl Into<String>) -> Self {
        Self {
            timestamp: SystemTime::now(),
            event,
            key_id,
            key_name,
            details: details.into(),
        }
    }
}

/// 密钥审计日志
#[derive(Debug, Clone, Default)]
pub struct KeyAuditLog {
    entries: Vec<KeyAuditEntry>,
}

impl KeyAuditLog {
    /// 创建空审计日志
    pub fn new() -> Self {
        Self::default()
    }

    /// 记录事件
    pub fn record(
        &mut self,
        event: KeyEvent,
        key_id: String,
        key_name: String,
        details: impl Into<String>,
    ) {
        self.entries
            .push(KeyAuditEntry::new(event, key_id, key_name, details));
    }

    /// 返回所有日志条目
    pub fn entries(&self) -> &[KeyAuditEntry] {
        &self.entries
    }

    /// 按事件类型过滤
    pub fn by_event(&self, event: KeyEvent) -> Vec<&KeyAuditEntry> {
        self.entries.iter().filter(|e| e.event == event).collect()
    }

    /// 按密钥名称过滤
    pub fn by_key_name(&self, name: &str) -> Vec<&KeyAuditEntry> {
        self.entries.iter().filter(|e| e.key_name == name).collect()
    }

    /// 返回日志条目数
    pub fn count(&self) -> usize {
        self.entries.len()
    }
}

// ============================================================================
// 密钥保险库（高级管理器）
// ============================================================================

/// 密钥保险库，集成生成、轮换、存储、检索与版本管理
///
/// 所有操作线程安全，通过 `parking_lot::RwLock` 保护内部状态。
pub struct KeyVault {
    store: RwLock<KeyStore>,
    rotation_policy: RotationPolicy,
    usage_counters: RwLock<HashMap<String, u64>>,
    audit_log: RwLock<KeyAuditLog>,
    generator: KeyGenerator,
}

impl KeyVault {
    /// 创建密钥保险库，指定轮换策略
    pub fn new(rotation_policy: RotationPolicy) -> Self {
        Self {
            store: RwLock::new(KeyStore::new()),
            rotation_policy,
            usage_counters: RwLock::new(HashMap::new()),
            audit_log: RwLock::new(KeyAuditLog::new()),
            generator: KeyGenerator::new(),
        }
    }

    /// 生成新密钥并存入存储，返回密钥 ID
    pub fn generate_key(
        &self,
        name: &str,
        algorithm: KeyAlgorithm,
        purpose: KeyPurpose,
    ) -> Result<String, KeyError> {
        let store = self.store.read().expect("store lock");
        let version = store.version_count(name) as u32 + 1;
        drop(store);

        let entry = self.generator.generate(name, algorithm, purpose, version);
        let key_id = entry.metadata.key_id.clone();
        let fingerprint = entry.fingerprint.clone();

        let mut store = self.store.write().expect("store lock");
        store.put(entry);

        self.audit_log.write().expect("audit lock").record(
            KeyEvent::Created,
            key_id.clone(),
            name.to_string(),
            format!(
                "algorithm={}, version={}, fingerprint={}",
                algorithm, version, fingerprint
            ),
        );

        Ok(key_id)
    }

    /// 轮换密钥：弃用当前版本并生成新版本
    pub fn rotate_key(&self, name: &str) -> Result<String, KeyError> {
        let mut store = self.store.write().expect("store lock");
        let current = store
            .get_latest_by_name(name)
            .ok_or(KeyError::KeyNotFound {
                name: name.to_string(),
            })?;

        if !current.metadata.status.is_usable_for_new() {
            return Err(KeyError::KeyNotActive {
                name: name.to_string(),
                status: current.metadata.status,
            });
        }

        let algorithm = current.metadata.algorithm;
        let purpose = current.metadata.purpose;
        let old_id = current.metadata.key_id.clone();
        let old_version = current.metadata.version;

        // 弃用旧密钥
        store.set_status(&old_id, KeyStatus::Deprecated);
        drop(store);

        // 生成新密钥
        let new_id = self.generate_key(name, algorithm, purpose)?;

        self.audit_log.write().expect("audit lock").record(
            KeyEvent::Rotated,
            new_id.clone(),
            name.to_string(),
            format!(
                "rotated from version {} to {}",
                old_version,
                old_version + 1
            ),
        );
        self.audit_log.write().expect("audit lock").record(
            KeyEvent::Deprecated,
            old_id,
            name.to_string(),
            format!("deprecated by rotation to version {}", old_version + 1),
        );

        Ok(new_id)
    }

    /// 获取密钥（按 ID），同时递增使用计数
    pub fn get_key(&self, key_id: &str) -> Option<KeyEntry> {
        let entry = self
            .store
            .read()
            .expect("store lock")
            .get(key_id)
            .cloned()?;
        *self
            .usage_counters
            .write()
            .expect("counters lock")
            .entry(key_id.to_string())
            .or_insert(0) += 1;
        self.audit_log.write().expect("audit lock").record(
            KeyEvent::Retrieved,
            key_id.to_string(),
            entry.metadata.name.clone(),
            "retrieved by id",
        );
        Some(entry)
    }

    /// 获取最新版本密钥（按名称）
    pub fn get_latest_key(&self, name: &str) -> Option<KeyEntry> {
        let entry = self
            .store
            .read()
            .expect("store lock")
            .get_latest_by_name(name)
            .cloned()?;
        let key_id = entry.metadata.key_id.clone();
        *self
            .usage_counters
            .write()
            .expect("counters lock")
            .entry(key_id)
            .or_insert(0) += 1;
        self.audit_log.write().expect("audit lock").record(
            KeyEvent::Retrieved,
            entry.metadata.key_id.clone(),
            name.to_string(),
            "retrieved latest by name",
        );
        Some(entry)
    }

    /// 按名称与版本获取密钥
    pub fn get_key_by_version(&self, name: &str, version: u32) -> Option<KeyEntry> {
        self.store
            .read()
            .expect("store lock")
            .get_by_name_and_version(name, version)
            .cloned()
    }

    /// 获取指定名称的所有版本
    pub fn get_all_versions(&self, name: &str) -> Vec<KeyEntry> {
        self.store
            .read()
            .expect("store lock")
            .get_all_versions(name)
            .into_iter()
            .cloned()
            .collect()
    }

    /// 吊销密钥
    pub fn revoke_key(&self, key_id: &str) -> Result<(), KeyError> {
        let mut store = self.store.write().expect("store lock");
        let entry = store.get(key_id).ok_or(KeyError::KeyIdNotFound {
            key_id: key_id.to_string(),
        })?;
        let name = entry.metadata.name.clone();
        store.set_status(key_id, KeyStatus::Revoked);
        drop(store);

        self.audit_log.write().expect("audit lock").record(
            KeyEvent::Revoked,
            key_id.to_string(),
            name,
            "key revoked",
        );
        Ok(())
    }

    /// 弃用密钥
    pub fn deprecate_key(&self, key_id: &str) -> Result<(), KeyError> {
        let mut store = self.store.write().expect("store lock");
        let entry = store.get(key_id).ok_or(KeyError::KeyIdNotFound {
            key_id: key_id.to_string(),
        })?;
        let name = entry.metadata.name.clone();
        store.set_status(key_id, KeyStatus::Deprecated);
        drop(store);

        self.audit_log.write().expect("audit lock").record(
            KeyEvent::Deprecated,
            key_id.to_string(),
            name,
            "key deprecated",
        );
        Ok(())
    }

    /// 检查密钥是否需要轮换
    pub fn needs_rotation(&self, name: &str) -> bool {
        let store = self.store.read().expect("store lock");
        let entry = match store.get_latest_by_name(name) {
            Some(e) => e,
            None => return false,
        };
        let key_id = &entry.metadata.key_id;
        let age = entry.metadata.age();
        let usage = self
            .usage_counters
            .read()
            .expect("counters lock")
            .get(key_id)
            .copied()
            .unwrap_or(0);
        self.rotation_policy.needs_rotation(age, usage)
    }

    /// 自动轮换检查：若需要轮换则执行
    pub fn auto_rotate(&self, name: &str) -> Option<String> {
        if self.needs_rotation(name) {
            self.rotate_key(name).ok()
        } else {
            None
        }
    }

    /// 返回密钥总数
    pub fn key_count(&self) -> usize {
        self.store.read().expect("store lock").count()
    }

    /// 返回指定名称的版本数
    pub fn version_count(&self, name: &str) -> usize {
        self.store.read().expect("store lock").version_count(name)
    }

    /// 返回所有密钥名称
    pub fn key_names(&self) -> Vec<String> {
        self.store.read().expect("store lock").names()
    }

    /// 返回审计日志的只读快照
    pub fn audit_log(&self) -> KeyAuditLog {
        self.audit_log.read().expect("audit lock").clone()
    }

    /// 返回指定密钥的使用次数
    pub fn usage_count(&self, key_id: &str) -> u64 {
        self.usage_counters
            .read()
            .expect("counters lock")
            .get(key_id)
            .copied()
            .unwrap_or(0)
    }

    /// 按标签检索密钥
    pub fn get_by_tag(&self, tag: &str) -> Vec<KeyEntry> {
        self.store
            .read()
            .expect("store lock")
            .get_by_tag(tag)
            .into_iter()
            .cloned()
            .collect()
    }

    /// 按状态检索密钥
    pub fn get_by_status(&self, status: KeyStatus) -> Vec<KeyEntry> {
        self.store
            .read()
            .expect("store lock")
            .get_by_status(status)
            .into_iter()
            .cloned()
            .collect()
    }

    /// 删除密钥（永久移除）
    pub fn delete_key(&self, key_id: &str) -> Result<KeyEntry, KeyError> {
        let mut store = self.store.write().expect("store lock");
        let entry = store.remove(key_id).ok_or(KeyError::KeyIdNotFound {
            key_id: key_id.to_string(),
        })?;
        let name = entry.metadata.name.clone();
        drop(store);

        self.audit_log.write().expect("audit lock").record(
            KeyEvent::Deleted,
            key_id.to_string(),
            name,
            "key deleted",
        );
        Ok(entry)
    }
}

impl fmt::Debug for KeyVault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KeyVault")
            .field("key_count", &self.key_count())
            .field("rotation_policy", &self.rotation_policy.as_str())
            .finish()
    }
}

// ============================================================================
// 错误类型
// ============================================================================

/// 密钥管理错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyError {
    /// 密钥名称不存在
    KeyNotFound { name: String },
    /// 密钥 ID 不存在
    KeyIdNotFound { key_id: String },
    /// 密钥状态不允许操作
    KeyNotActive { name: String, status: KeyStatus },
    /// 密钥用途不匹配
    PurposeMismatch {
        expected: KeyPurpose,
        actual: KeyPurpose,
    },
    /// 密钥已过期
    KeyExpired { key_id: String },
    /// 密钥已吊销
    KeyRevoked { key_id: String },
}

impl fmt::Display for KeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KeyError::KeyNotFound { name } => write!(f, "key '{}' not found", name),
            KeyError::KeyIdNotFound { key_id } => write!(f, "key id '{}' not found", key_id),
            KeyError::KeyNotActive { name, status } => {
                write!(f, "key '{}' is not active (status: {})", name, status)
            }
            KeyError::PurposeMismatch { expected, actual } => {
                write!(
                    f,
                    "key purpose mismatch: expected {}, actual {}",
                    expected, actual
                )
            }
            KeyError::KeyExpired { key_id } => write!(f, "key '{}' is expired", key_id),
            KeyError::KeyRevoked { key_id } => write!(f, "key '{}' is revoked", key_id),
        }
    }
}

impl std::error::Error for KeyError {}

// ============================================================================
// 密钥派生配置
// ============================================================================

/// 密钥派生函数参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyDerivationConfig {
    /// 派生函数名称（pbkdf2 / argon2id / scrypt）
    pub algorithm: String,
    /// 迭代次数
    pub iterations: u32,
    /// 盐长度（字节）
    pub salt_len: usize,
    /// 派生密钥长度（字节）
    pub key_len: usize,
    /// 内存参数（Argon2 专用，单位 KB）
    pub memory_kb: Option<u32>,
    /// 并行度（Argon2 专用）
    pub parallelism: Option<u32>,
}

impl Default for KeyDerivationConfig {
    fn default() -> Self {
        Self {
            algorithm: "pbkdf2".to_string(),
            iterations: 600_000,
            salt_len: 32,
            key_len: 32,
            memory_kb: None,
            parallelism: None,
        }
    }
}

impl KeyDerivationConfig {
    /// PBKDF2-HMAC-SHA256 配置
    pub fn pbkdf2(iterations: u32) -> Self {
        Self {
            algorithm: "pbkdf2".to_string(),
            iterations,
            ..Default::default()
        }
    }

    /// Argon2id 配置
    pub fn argon2id(iterations: u32, memory_kb: u32, parallelism: u32) -> Self {
        Self {
            algorithm: "argon2id".to_string(),
            iterations,
            memory_kb: Some(memory_kb),
            parallelism: Some(parallelism),
            ..Default::default()
        }
    }

    /// scrypt 配置
    pub fn scrypt(iterations: u32) -> Self {
        Self {
            algorithm: "scrypt".to_string(),
            iterations,
            ..Default::default()
        }
    }

    /// 是否为 Argon2
    pub fn is_argon2(&self) -> bool {
        self.algorithm == "argon2id"
    }

    /// 是否为 PBKDF2
    pub fn is_pbkdf2(&self) -> bool {
        self.algorithm == "pbkdf2"
    }

    /// 生成随机盐
    pub fn generate_salt(&self) -> Vec<u8> {
        let mut salt = vec![0u8; self.salt_len];
        OsRng.fill_bytes(&mut salt);
        salt
    }
}

// ============================================================================
// 随机数生成器
// ============================================================================

/// 加密安全随机数生成器
pub struct NonceGenerator {
    /// 随机数长度（字节）
    nonce_len: usize,
}

impl NonceGenerator {
    /// 创建 12 字节 GCM nonce 生成器
    pub fn for_gcm() -> Self {
        Self { nonce_len: 12 }
    }

    /// 创建 16 字节 ChaCha20 nonce 生成器
    pub fn for_chacha20() -> Self {
        Self { nonce_len: 16 }
    }

    /// 自定义长度
    pub fn new(nonce_len: usize) -> Self {
        Self { nonce_len }
    }

    /// 生成随机 nonce
    pub fn generate(&self) -> Vec<u8> {
        let mut nonce = vec![0u8; self.nonce_len];
        OsRng.fill_bytes(&mut nonce);
        nonce
    }

    /// nonce 长度
    pub fn len(&self) -> usize {
        self.nonce_len
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.nonce_len == 0
    }
}

// ============================================================================
// 加密上下文
// ============================================================================

/// 加密上下文：携带关联数据（AEAD）和元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct EncryptionContext {
    /// 关联数据（Additional Authenticated Data）
    pub aad: Vec<u8>,
    /// 上下文标签（用于区分不同加密场景）
    pub label: String,
    /// 租户 ID（多租户场景）
    pub tenant_id: Option<String>,
}


impl EncryptionContext {
    /// 创建空上下文
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置关联数据
    pub fn with_aad(mut self, aad: Vec<u8>) -> Self {
        self.aad = aad;
        self
    }

    /// 设置标签
    pub fn with_label(mut self, label: &str) -> Self {
        self.label = label.to_string();
        self
    }

    /// 设置租户 ID
    pub fn with_tenant(mut self, tenant_id: &str) -> Self {
        self.tenant_id = Some(tenant_id.to_string());
        self
    }

    /// 是否有关联数据
    pub fn has_aad(&self) -> bool {
        !self.aad.is_empty()
    }

    /// 是否有租户
    pub fn has_tenant(&self) -> bool {
        self.tenant_id.is_some()
    }
}

// ============================================================================
// 密钥指纹
// ============================================================================

/// 密钥指纹：用于安全识别密钥（不暴露密钥本身）
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KeyFingerprint {
    /// SHA-256 指纹（十六进制）
    pub sha256_hex: String,
    /// 算法
    pub algorithm: KeyAlgorithm,
}

impl KeyFingerprint {
    /// 从密钥材料计算指纹
    pub fn from_key_material(key: &[u8], algorithm: KeyAlgorithm) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(key);
        let hash = hasher.finalize();
        let hex: String = hash.iter().map(|b| format!("{:02x}", b)).collect();
        Self {
            sha256_hex: hex,
            algorithm,
        }
    }

    /// 指纹前 8 个字符（用于日志显示）
    pub fn short(&self) -> &str {
        &self.sha256_hex[..8.min(self.sha256_hex.len())]
    }

    /// 指纹是否匹配
    pub fn matches(&self, other: &Self) -> bool {
        self.sha256_hex == other.sha256_hex && self.algorithm == other.algorithm
    }
}

impl fmt::Display for KeyFingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.algorithm.as_str(), self.short())
    }
}

// ============================================================================
// 安全策略
// ============================================================================

/// 安全策略配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityPolicy {
    /// 最小密钥长度
    pub min_key_length: usize,
    /// 密钥最大使用次数（0 = 无限）
    pub max_key_usage: u64,
    /// 密钥最大年龄（秒，0 = 无限）
    pub max_key_age_secs: u64,
    /// 是否禁止明文存储
    pub require_encryption_at_rest: bool,
    /// 是否要求密钥轮换
    pub require_key_rotation: bool,
    /// 允许的算法列表
    pub allowed_algorithms: Vec<KeyAlgorithm>,
}

impl Default for SecurityPolicy {
    fn default() -> Self {
        Self {
            min_key_length: 32,
            max_key_usage: 1_000_000,
            max_key_age_secs: 86400 * 90,
            require_encryption_at_rest: true,
            require_key_rotation: true,
            allowed_algorithms: vec![KeyAlgorithm::Aes256Gcm, KeyAlgorithm::HmacSha256],
        }
    }
}

impl SecurityPolicy {
    /// 创建严格策略
    pub fn strict() -> Self {
        Self {
            min_key_length: 32,
            max_key_usage: 100_000,
            max_key_age_secs: 86400 * 30,
            require_encryption_at_rest: true,
            require_key_rotation: true,
            allowed_algorithms: vec![KeyAlgorithm::Aes256Gcm],
        }
    }

    /// 检查算法是否允许
    pub fn is_algorithm_allowed(&self, alg: KeyAlgorithm) -> bool {
        self.allowed_algorithms.contains(&alg)
    }

    /// 检查密钥长度是否满足要求
    pub fn is_key_length_ok(&self, key_len: usize) -> bool {
        key_len >= self.min_key_length
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ---- KeyAlgorithm 测试 ----

    #[test]
    fn test_key_algorithm_as_str() {
        assert_eq!(KeyAlgorithm::Aes256Gcm.as_str(), "aes-256-gcm");
        assert_eq!(KeyAlgorithm::HmacSha256.as_str(), "hmac-sha256");
        assert_eq!(KeyAlgorithm::Rsa2048.as_str(), "rsa-2048");
        assert_eq!(KeyAlgorithm::Rsa4096.as_str(), "rsa-4096");
    }

    #[test]
    fn test_key_algorithm_key_length() {
        assert_eq!(KeyAlgorithm::Aes256Gcm.key_length(), 32);
        assert_eq!(KeyAlgorithm::HmacSha256.key_length(), 32);
        assert_eq!(KeyAlgorithm::Rsa2048.key_length(), 256);
        assert_eq!(KeyAlgorithm::Rsa4096.key_length(), 512);
    }

    #[test]
    fn test_key_algorithm_is_symmetric() {
        assert!(KeyAlgorithm::Aes256Gcm.is_symmetric());
        assert!(KeyAlgorithm::HmacSha256.is_symmetric());
        assert!(!KeyAlgorithm::Rsa2048.is_symmetric());
        assert!(!KeyAlgorithm::Rsa4096.is_symmetric());
    }

    // ---- KeyPurpose 测试 ----

    #[test]
    fn test_key_purpose_can_encrypt() {
        assert!(KeyPurpose::Encryption.can_encrypt());
        assert!(!KeyPurpose::Signing.can_encrypt());
        assert!(KeyPurpose::EncryptionAndSigning.can_encrypt());
    }

    #[test]
    fn test_key_purpose_can_sign() {
        assert!(!KeyPurpose::Encryption.can_sign());
        assert!(KeyPurpose::Signing.can_sign());
        assert!(KeyPurpose::EncryptionAndSigning.can_sign());
    }

    // ---- KeyStatus 测试 ----

    #[test]
    fn test_key_status_is_usable_for_new() {
        assert!(KeyStatus::Active.is_usable_for_new());
        assert!(!KeyStatus::Deprecated.is_usable_for_new());
        assert!(!KeyStatus::Revoked.is_usable_for_new());
        assert!(!KeyStatus::Expired.is_usable_for_new());
    }

    #[test]
    fn test_key_status_is_usable_for_old() {
        assert!(KeyStatus::Active.is_usable_for_old());
        assert!(KeyStatus::Deprecated.is_usable_for_old());
        assert!(!KeyStatus::Revoked.is_usable_for_old());
        assert!(KeyStatus::Expired.is_usable_for_old());
    }

    #[test]
    fn test_key_status_is_dead() {
        assert!(!KeyStatus::Active.is_dead());
        assert!(KeyStatus::Revoked.is_dead());
    }

    // ---- KeyMetadata 测试 ----

    #[test]
    fn test_key_metadata_with_expiry() {
        let now = SystemTime::now();
        let meta = KeyMetadata::new(
            "id1".to_string(),
            "test".to_string(),
            KeyAlgorithm::Aes256Gcm,
            KeyPurpose::Encryption,
            1,
        )
        .with_expiry(now + Duration::from_secs(3600));
        assert!(!meta.is_expired());
        assert_eq!(meta.expires_at, Some(now + Duration::from_secs(3600)));
    }

    #[test]
    fn test_key_metadata_is_expired() {
        let now = SystemTime::now();
        let meta = KeyMetadata::new(
            "id1".to_string(),
            "test".to_string(),
            KeyAlgorithm::Aes256Gcm,
            KeyPurpose::Encryption,
            1,
        )
        .with_expiry(now - Duration::from_secs(1));
        assert!(meta.is_expired());
    }

    #[test]
    fn test_key_metadata_with_tag_and_description() {
        let meta = KeyMetadata::new(
            "id1".to_string(),
            "test".to_string(),
            KeyAlgorithm::Aes256Gcm,
            KeyPurpose::Encryption,
            1,
        )
        .with_description("test key")
        .with_tag("production")
        .with_tag("critical");
        assert_eq!(meta.description, "test key");
        assert_eq!(meta.tags, vec!["production", "critical"]);
    }

    // ---- KeyEntry 测试 ----

    #[test]
    fn test_key_entry_compute_fingerprint() {
        let material = b"test-key-material";
        let fp = KeyEntry::compute_fingerprint(material);
        assert_eq!(fp.len(), 64); // SHA-256 hex = 64 chars
    }

    #[test]
    fn test_key_entry_verify_fingerprint() {
        let material = b"test-key-material";
        let entry = KeyEntry {
            metadata: KeyMetadata::new(
                "id1".to_string(),
                "test".to_string(),
                KeyAlgorithm::Aes256Gcm,
                KeyPurpose::Encryption,
                1,
            ),
            material: material.to_vec(),
            fingerprint: KeyEntry::compute_fingerprint(material),
        };
        assert!(entry.verify_fingerprint());
    }

    #[test]
    fn test_key_entry_verify_fingerprint_mismatch() {
        let entry = KeyEntry {
            metadata: KeyMetadata::new(
                "id1".to_string(),
                "test".to_string(),
                KeyAlgorithm::Aes256Gcm,
                KeyPurpose::Encryption,
                1,
            ),
            material: b"actual-material".to_vec(),
            fingerprint: "0000000000000000000000000000000000000000000000000000000000000000"
                .to_string(),
        };
        assert!(!entry.verify_fingerprint());
    }

    // ---- KeyGenerator 测试 ----

    #[test]
    fn test_key_generator_generate_material_length() {
        let gen = KeyGenerator::new();
        let material = gen.generate_material(KeyAlgorithm::Aes256Gcm);
        assert_eq!(material.len(), 32);
    }

    #[test]
    fn test_key_generator_generate_material_random() {
        let gen = KeyGenerator::new();
        let a = gen.generate_material(KeyAlgorithm::Aes256Gcm);
        let b = gen.generate_material(KeyAlgorithm::Aes256Gcm);
        assert_ne!(a, b, "随机生成的密钥应不同");
    }

    #[test]
    fn test_key_generator_generate_key_id() {
        let id1 = KeyGenerator::generate_key_id();
        let id2 = KeyGenerator::generate_key_id();
        assert_eq!(id1.len(), 32); // 16 bytes hex = 32 chars
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_key_generator_generate_full_entry() {
        let gen = KeyGenerator::new();
        let entry = gen.generate("test-key", KeyAlgorithm::HmacSha256, KeyPurpose::Signing, 1);
        assert_eq!(entry.metadata.name, "test-key");
        assert_eq!(entry.metadata.algorithm, KeyAlgorithm::HmacSha256);
        assert_eq!(entry.metadata.version, 1);
        assert_eq!(entry.material.len(), 32);
        assert!(entry.verify_fingerprint());
    }

    // ---- RotationPolicy 测试 ----

    #[test]
    fn test_rotation_policy_time_interval() {
        let policy = RotationPolicy::TimeInterval(Duration::from_secs(100));
        assert!(!policy.needs_rotation(Duration::from_secs(50), 0));
        assert!(policy.needs_rotation(Duration::from_secs(100), 0));
        assert!(policy.needs_rotation(Duration::from_secs(150), 0));
    }

    #[test]
    fn test_rotation_policy_usage_count() {
        let policy = RotationPolicy::UsageCount(1000);
        assert!(!policy.needs_rotation(Duration::ZERO, 500));
        assert!(policy.needs_rotation(Duration::ZERO, 1000));
        assert!(policy.needs_rotation(Duration::ZERO, 1500));
    }

    #[test]
    fn test_rotation_policy_time_or_usage() {
        let policy = RotationPolicy::TimeIntervalOrUsage(Duration::from_secs(100), 1000);
        assert!(!policy.needs_rotation(Duration::from_secs(50), 500));
        assert!(policy.needs_rotation(Duration::from_secs(100), 500));
        assert!(policy.needs_rotation(Duration::from_secs(50), 1000));
    }

    #[test]
    fn test_rotation_policy_never() {
        let policy = RotationPolicy::Never;
        assert!(!policy.needs_rotation(Duration::from_secs(999999), 999999));
    }

    // ---- KeyStore 测试 ----

    #[test]
    fn test_key_store_put_and_get() {
        let mut store = KeyStore::new();
        let gen = KeyGenerator::new();
        let entry = gen.generate("test", KeyAlgorithm::Aes256Gcm, KeyPurpose::Encryption, 1);
        let key_id = store.put(entry);
        assert!(store.get(&key_id).is_some());
        assert_eq!(store.count(), 1);
    }

    #[test]
    fn test_key_store_get_latest_by_name() {
        let mut store = KeyStore::new();
        let gen = KeyGenerator::new();
        let e1 = gen.generate("key", KeyAlgorithm::Aes256Gcm, KeyPurpose::Encryption, 1);
        let e2 = gen.generate("key", KeyAlgorithm::Aes256Gcm, KeyPurpose::Encryption, 2);
        store.put(e1);
        store.put(e2);
        let latest = store.get_latest_by_name("key").unwrap();
        assert_eq!(latest.metadata.version, 2);
    }

    #[test]
    fn test_key_store_get_by_name_and_version() {
        let mut store = KeyStore::new();
        let gen = KeyGenerator::new();
        let e1 = gen.generate("key", KeyAlgorithm::Aes256Gcm, KeyPurpose::Encryption, 1);
        let e2 = gen.generate("key", KeyAlgorithm::Aes256Gcm, KeyPurpose::Encryption, 2);
        store.put(e1);
        store.put(e2);
        assert!(store.get_by_name_and_version("key", 1).is_some());
        assert!(store.get_by_name_and_version("key", 2).is_some());
        assert!(store.get_by_name_and_version("key", 3).is_none());
    }

    #[test]
    fn test_key_store_get_all_versions() {
        let mut store = KeyStore::new();
        let gen = KeyGenerator::new();
        for v in 1..=3 {
            let entry = gen.generate("key", KeyAlgorithm::Aes256Gcm, KeyPurpose::Encryption, v);
            store.put(entry);
        }
        let versions = store.get_all_versions("key");
        assert_eq!(versions.len(), 3);
    }

    #[test]
    fn test_key_store_remove() {
        let mut store = KeyStore::new();
        let gen = KeyGenerator::new();
        let entry = gen.generate("test", KeyAlgorithm::Aes256Gcm, KeyPurpose::Encryption, 1);
        let key_id = store.put(entry);
        assert_eq!(store.count(), 1);
        let removed = store.remove(&key_id);
        assert!(removed.is_some());
        assert_eq!(store.count(), 0);
    }

    #[test]
    fn test_key_store_set_status() {
        let mut store = KeyStore::new();
        let gen = KeyGenerator::new();
        let entry = gen.generate("test", KeyAlgorithm::Aes256Gcm, KeyPurpose::Encryption, 1);
        let key_id = store.put(entry);
        assert!(store.set_status(&key_id, KeyStatus::Revoked));
        assert_eq!(
            store.get(&key_id).unwrap().metadata.status,
            KeyStatus::Revoked
        );
    }

    #[test]
    fn test_key_store_get_by_tag() {
        let mut store = KeyStore::new();
        let gen = KeyGenerator::new();
        let mut entry = gen.generate("test", KeyAlgorithm::Aes256Gcm, KeyPurpose::Encryption, 1);
        entry.metadata.tags = vec!["production".to_string()];
        store.put(entry);
        let results = store.get_by_tag("production");
        assert_eq!(results.len(), 1);
        assert_eq!(store.get_by_tag("staging").len(), 0);
    }

    // ---- KeyAuditLog 测试 ----

    #[test]
    fn test_key_audit_log_record_and_query() {
        let mut log = KeyAuditLog::new();
        log.record(
            KeyEvent::Created,
            "id1".to_string(),
            "key1".to_string(),
            "created",
        );
        log.record(
            KeyEvent::Rotated,
            "id2".to_string(),
            "key1".to_string(),
            "rotated",
        );
        assert_eq!(log.count(), 2);
        assert_eq!(log.by_event(KeyEvent::Created).len(), 1);
        assert_eq!(log.by_key_name("key1").len(), 2);
    }

    // ---- KeyVault 测试 ----

    #[test]
    fn test_key_vault_generate_key() {
        let vault = KeyVault::new(RotationPolicy::Never);
        let key_id = vault
            .generate_key("test", KeyAlgorithm::Aes256Gcm, KeyPurpose::Encryption)
            .unwrap();
        assert!(vault.get_key(&key_id).is_some());
        assert_eq!(vault.key_count(), 1);
        assert_eq!(vault.version_count("test"), 1);
    }

    #[test]
    fn test_key_vault_rotate_key() {
        let vault = KeyVault::new(RotationPolicy::Never);
        let old_id = vault
            .generate_key("test", KeyAlgorithm::Aes256Gcm, KeyPurpose::Encryption)
            .unwrap();
        let new_id = vault.rotate_key("test").unwrap();
        assert_ne!(old_id, new_id);
        assert_eq!(vault.version_count("test"), 2);
        // 旧密钥应被弃用
        let old = vault.get_key(&old_id).unwrap();
        assert_eq!(old.metadata.status, KeyStatus::Deprecated);
        // 新密钥应为活跃
        let new_key = vault.get_key(&new_id).unwrap();
        assert_eq!(new_key.metadata.status, KeyStatus::Active);
    }

    #[test]
    fn test_key_vault_rotate_nonexistent_key() {
        let vault = KeyVault::new(RotationPolicy::Never);
        let result = vault.rotate_key("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_key_vault_get_latest_key() {
        let vault = KeyVault::new(RotationPolicy::Never);
        vault
            .generate_key("test", KeyAlgorithm::Aes256Gcm, KeyPurpose::Encryption)
            .unwrap();
        vault.rotate_key("test").unwrap();
        let latest = vault.get_latest_key("test").unwrap();
        assert_eq!(latest.metadata.version, 2);
    }

    #[test]
    fn test_key_vault_revoke_key() {
        let vault = KeyVault::new(RotationPolicy::Never);
        let key_id = vault
            .generate_key("test", KeyAlgorithm::Aes256Gcm, KeyPurpose::Encryption)
            .unwrap();
        vault.revoke_key(&key_id).unwrap();
        let entry = vault.get_key(&key_id).unwrap();
        assert_eq!(entry.metadata.status, KeyStatus::Revoked);
    }

    #[test]
    fn test_key_vault_deprecate_key() {
        let vault = KeyVault::new(RotationPolicy::Never);
        let key_id = vault
            .generate_key("test", KeyAlgorithm::Aes256Gcm, KeyPurpose::Encryption)
            .unwrap();
        vault.deprecate_key(&key_id).unwrap();
        let entry = vault.get_key(&key_id).unwrap();
        assert_eq!(entry.metadata.status, KeyStatus::Deprecated);
    }

    #[test]
    fn test_key_vault_audit_log() {
        let vault = KeyVault::new(RotationPolicy::Never);
        vault
            .generate_key("test", KeyAlgorithm::Aes256Gcm, KeyPurpose::Encryption)
            .unwrap();
        let log = vault.audit_log();
        assert!(log.count() >= 1);
        assert_eq!(log.by_event(KeyEvent::Created).len(), 1);
    }

    #[test]
    fn test_key_vault_usage_count() {
        let vault = KeyVault::new(RotationPolicy::Never);
        let key_id = vault
            .generate_key("test", KeyAlgorithm::Aes256Gcm, KeyPurpose::Encryption)
            .unwrap();
        assert_eq!(vault.usage_count(&key_id), 0);
        vault.get_key(&key_id);
        vault.get_key(&key_id);
        assert_eq!(vault.usage_count(&key_id), 2);
    }

    #[test]
    fn test_key_vault_needs_rotation_time() {
        let vault = KeyVault::new(RotationPolicy::TimeInterval(Duration::from_millis(0)));
        vault
            .generate_key("test", KeyAlgorithm::Aes256Gcm, KeyPurpose::Encryption)
            .unwrap();
        std::thread::sleep(Duration::from_millis(1));
        assert!(vault.needs_rotation("test"));
    }

    #[test]
    fn test_key_vault_needs_rotation_never() {
        let vault = KeyVault::new(RotationPolicy::Never);
        vault
            .generate_key("test", KeyAlgorithm::Aes256Gcm, KeyPurpose::Encryption)
            .unwrap();
        assert!(!vault.needs_rotation("test"));
    }

    #[test]
    fn test_key_vault_auto_rotate() {
        let vault = KeyVault::new(RotationPolicy::TimeInterval(Duration::from_millis(0)));
        vault
            .generate_key("test", KeyAlgorithm::Aes256Gcm, KeyPurpose::Encryption)
            .unwrap();
        std::thread::sleep(Duration::from_millis(1));
        let new_id = vault.auto_rotate("test");
        assert!(new_id.is_some());
        assert_eq!(vault.version_count("test"), 2);
    }

    #[test]
    fn test_key_vault_auto_rotate_no_rotation_needed() {
        let vault = KeyVault::new(RotationPolicy::Never);
        vault
            .generate_key("test", KeyAlgorithm::Aes256Gcm, KeyPurpose::Encryption)
            .unwrap();
        let result = vault.auto_rotate("test");
        assert!(result.is_none());
    }

    #[test]
    fn test_key_vault_delete_key() {
        let vault = KeyVault::new(RotationPolicy::Never);
        let key_id = vault
            .generate_key("test", KeyAlgorithm::Aes256Gcm, KeyPurpose::Encryption)
            .unwrap();
        assert_eq!(vault.key_count(), 1);
        vault.delete_key(&key_id).unwrap();
        assert_eq!(vault.key_count(), 0);
    }

    #[test]
    fn test_key_vault_get_by_status() {
        let vault = KeyVault::new(RotationPolicy::Never);
        let id1 = vault
            .generate_key("k1", KeyAlgorithm::Aes256Gcm, KeyPurpose::Encryption)
            .unwrap();
        vault
            .generate_key("k2", KeyAlgorithm::Aes256Gcm, KeyPurpose::Encryption)
            .unwrap();
        vault.revoke_key(&id1).unwrap();
        let active = vault.get_by_status(KeyStatus::Active);
        let revoked = vault.get_by_status(KeyStatus::Revoked);
        assert_eq!(active.len(), 1);
        assert_eq!(revoked.len(), 1);
    }

    #[test]
    fn test_key_vault_concurrent_access() {
        use std::sync::Arc;
        use std::thread;
        let vault = Arc::new(KeyVault::new(RotationPolicy::Never));
        let id = vault
            .generate_key("test", KeyAlgorithm::Aes256Gcm, KeyPurpose::Encryption)
            .unwrap();
        let mut handles = vec![];
        for _ in 0..4 {
            let v = vault.clone();
            let kid = id.clone();
            handles.push(thread::spawn(move || {
                v.get_key(&kid);
            }));
        }
        for h in handles {
            h.join().expect("thread panicked");
        }
        assert_eq!(vault.usage_count(&id), 4);
    }

    // ---- KeyError 测试 ----

    #[test]
    fn test_key_error_display() {
        let err = KeyError::KeyNotFound {
            name: "test".to_string(),
        };
        assert!(err.to_string().contains("test"));
        let err2 = KeyError::KeyRevoked {
            key_id: "id1".to_string(),
        };
        assert!(err2.to_string().contains("id1"));
    }

    // ---- KeyDerivationConfig 测试 ----

    #[test]
    fn test_kdf_default() {
        let cfg = KeyDerivationConfig::default();
        assert_eq!(cfg.algorithm, "pbkdf2");
        assert_eq!(cfg.key_len, 32);
    }

    #[test]
    fn test_kdf_pbkdf2() {
        let cfg = KeyDerivationConfig::pbkdf2(100_000);
        assert!(cfg.is_pbkdf2());
        assert!(!cfg.is_argon2());
        assert_eq!(cfg.iterations, 100_000);
    }

    #[test]
    fn test_kdf_argon2id() {
        let cfg = KeyDerivationConfig::argon2id(3, 65536, 4);
        assert!(cfg.is_argon2());
        assert!(!cfg.is_pbkdf2());
        assert_eq!(cfg.memory_kb, Some(65536));
        assert_eq!(cfg.parallelism, Some(4));
    }

    #[test]
    fn test_kdf_scrypt() {
        let cfg = KeyDerivationConfig::scrypt(1024);
        assert_eq!(cfg.algorithm, "scrypt");
        assert_eq!(cfg.iterations, 1024);
    }

    #[test]
    fn test_kdf_generate_salt() {
        let cfg = KeyDerivationConfig::default();
        let salt = cfg.generate_salt();
        assert_eq!(salt.len(), cfg.salt_len);
    }

    // ---- NonceGenerator 测试 ----

    #[test]
    fn test_nonce_generator_gcm() {
        let gen = NonceGenerator::for_gcm();
        let nonce = gen.generate();
        assert_eq!(nonce.len(), 12);
    }

    #[test]
    fn test_nonce_generator_chacha20() {
        let gen = NonceGenerator::for_chacha20();
        let nonce = gen.generate();
        assert_eq!(nonce.len(), 16);
    }

    #[test]
    fn test_nonce_generator_custom() {
        let gen = NonceGenerator::new(32);
        assert_eq!(gen.len(), 32);
        let nonce = gen.generate();
        assert_eq!(nonce.len(), 32);
    }

    #[test]
    fn test_nonce_generator_unique() {
        let gen = NonceGenerator::for_gcm();
        let n1 = gen.generate();
        let n2 = gen.generate();
        assert_ne!(n1, n2);
    }

    // ---- EncryptionContext 测试 ----

    #[test]
    fn test_encryption_context_default() {
        let ctx = EncryptionContext::default();
        assert!(!ctx.has_aad());
        assert!(!ctx.has_tenant());
    }

    #[test]
    fn test_encryption_context_builder() {
        let ctx = EncryptionContext::new()
            .with_aad(b"associated".to_vec())
            .with_label("db")
            .with_tenant("tenant1");
        assert!(ctx.has_aad());
        assert!(ctx.has_tenant());
        assert_eq!(ctx.label, "db");
        assert_eq!(ctx.tenant_id, Some("tenant1".to_string()));
    }

    // ---- KeyFingerprint 测试 ----

    #[test]
    fn test_key_fingerprint() {
        let key = b"my-secret-key-1234567890123456";
        let fp = KeyFingerprint::from_key_material(key, KeyAlgorithm::Aes256Gcm);
        assert_eq!(fp.sha256_hex.len(), 64);
        assert_eq!(fp.short().len(), 8);
    }

    #[test]
    fn test_key_fingerprint_matches() {
        let key = b"my-secret-key-1234567890123456";
        let fp1 = KeyFingerprint::from_key_material(key, KeyAlgorithm::Aes256Gcm);
        let fp2 = KeyFingerprint::from_key_material(key, KeyAlgorithm::Aes256Gcm);
        assert!(fp1.matches(&fp2));
    }

    #[test]
    fn test_key_fingerprint_not_matches() {
        let fp1 = KeyFingerprint::from_key_material(b"key1", KeyAlgorithm::Aes256Gcm);
        let fp2 = KeyFingerprint::from_key_material(b"key2", KeyAlgorithm::Aes256Gcm);
        assert!(!fp1.matches(&fp2));
    }

    #[test]
    fn test_key_fingerprint_display() {
        let fp = KeyFingerprint::from_key_material(b"key", KeyAlgorithm::Aes256Gcm);
        let s = format!("{}", fp);
        assert!(s.contains("aes-256-gcm"));
    }

    // ---- SecurityPolicy 测试 ----

    #[test]
    fn test_security_policy_default() {
        let policy = SecurityPolicy::default();
        assert!(policy.require_encryption_at_rest);
        assert!(policy.require_key_rotation);
        assert!(policy.is_algorithm_allowed(KeyAlgorithm::Aes256Gcm));
    }

    #[test]
    fn test_security_policy_strict() {
        let policy = SecurityPolicy::strict();
        assert!(policy.is_algorithm_allowed(KeyAlgorithm::Aes256Gcm));
        assert!(!policy.is_algorithm_allowed(KeyAlgorithm::HmacSha256));
    }

    #[test]
    fn test_security_policy_key_length() {
        let policy = SecurityPolicy::default();
        assert!(policy.is_key_length_ok(32));
        assert!(!policy.is_key_length_ok(16));
    }
}
