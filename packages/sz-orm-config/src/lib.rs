//! # SZ-ORM Config — 配置中心
//!
//! 提供配置中心抽象（Consul/Nacos 等），支持 get/set/delete/list/watch，
//! 并可在配置变更时通过回调通知订阅者。
//!
//! ## 主要类型
//!
//! - [`ConfigCenter`] trait — 配置中心接口
//! - [`ConfigChangeEvent`] — 配置变更事件

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Callback invoked when a configuration value changes.
/// Arguments: `(key, new_value)`. On delete, `new_value` is empty.
pub type ConfigChangeCallback = Arc<dyn Fn(&str, &str) + Send + Sync>;

/// Configuration center abstraction (Consul/Nacos/etc).
pub trait ConfigCenter: Send + Sync {
    fn get(&self, key: &str) -> Option<String>;
    fn set(&mut self, key: &str, value: &str);
    fn delete(&mut self, key: &str) -> bool;
    fn exists(&self, key: &str) -> bool;
    fn list(&self) -> Vec<String>;
    /// Returns true if a watch was successfully registered.
    /// In this in-memory implementation, registration always succeeds.
    fn watch(&self, key: &str) -> bool;
    /// Registers a callback for changes to `key`.
    fn subscribe(&mut self, key: &str, callback: ConfigChangeCallback);
}

/// Configuration change event record, useful for testing and auditing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigChangeEvent {
    pub key: String,
    pub value: String,
    pub deleted: bool,
}

/// Consul-style in-memory configuration center.
pub struct ConsulConfigCenter {
    data: HashMap<String, String>,
    subscribers: HashMap<String, Vec<ConfigChangeCallback>>,
    events: Mutex<Vec<ConfigChangeEvent>>,
}

impl ConsulConfigCenter {
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
            subscribers: HashMap::new(),
            events: Mutex::new(Vec::new()),
        }
    }

    fn notify(&self, key: &str, value: &str, deleted: bool) {
        if let Some(callbacks) = self.subscribers.get(key) {
            for cb in callbacks {
                cb(key, value);
            }
        }
        if let Ok(mut events) = self.events.lock() {
            events.push(ConfigChangeEvent {
                key: key.to_string(),
                value: value.to_string(),
                deleted,
            });
        }
    }

    /// Returns the ordered list of all change events that have occurred.
    pub fn events(&self) -> Vec<ConfigChangeEvent> {
        self.events.lock().map(|e| e.clone()).unwrap_or_default()
    }

    pub fn subscriber_count(&self, key: &str) -> usize {
        self.subscribers.get(key).map(|cbs| cbs.len()).unwrap_or(0)
    }
}

impl Default for ConsulConfigCenter {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigCenter for ConsulConfigCenter {
    fn get(&self, key: &str) -> Option<String> {
        self.data.get(key).cloned()
    }

    fn set(&mut self, key: &str, value: &str) {
        self.data.insert(key.to_string(), value.to_string());
        self.notify(key, value, false);
    }

    fn delete(&mut self, key: &str) -> bool {
        let removed = self.data.remove(key).is_some();
        if removed {
            self.notify(key, "", true);
        }
        removed
    }

    fn exists(&self, key: &str) -> bool {
        self.data.contains_key(key)
    }

    fn list(&self) -> Vec<String> {
        let mut keys: Vec<String> = self.data.keys().cloned().collect();
        keys.sort();
        keys
    }

    fn watch(&self, _key: &str) -> bool {
        true
    }

    fn subscribe(&mut self, key: &str, callback: ConfigChangeCallback) {
        self.subscribers
            .entry(key.to_string())
            .or_default()
            .push(callback);
    }
}

/// Nacos-style in-memory configuration center.
pub struct NacosConfigCenter {
    data: HashMap<String, String>,
    subscribers: HashMap<String, Vec<ConfigChangeCallback>>,
    events: Mutex<Vec<ConfigChangeEvent>>,
}

impl NacosConfigCenter {
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
            subscribers: HashMap::new(),
            events: Mutex::new(Vec::new()),
        }
    }

    fn notify(&self, key: &str, value: &str, deleted: bool) {
        if let Some(callbacks) = self.subscribers.get(key) {
            for cb in callbacks {
                cb(key, value);
            }
        }
        if let Ok(mut events) = self.events.lock() {
            events.push(ConfigChangeEvent {
                key: key.to_string(),
                value: value.to_string(),
                deleted,
            });
        }
    }

    pub fn events(&self) -> Vec<ConfigChangeEvent> {
        self.events.lock().map(|e| e.clone()).unwrap_or_default()
    }

    pub fn subscriber_count(&self, key: &str) -> usize {
        self.subscribers.get(key).map(|cbs| cbs.len()).unwrap_or(0)
    }
}

impl Default for NacosConfigCenter {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigCenter for NacosConfigCenter {
    fn get(&self, key: &str) -> Option<String> {
        self.data.get(key).cloned()
    }

    fn set(&mut self, key: &str, value: &str) {
        self.data.insert(key.to_string(), value.to_string());
        self.notify(key, value, false);
    }

    fn delete(&mut self, key: &str) -> bool {
        let removed = self.data.remove(key).is_some();
        if removed {
            self.notify(key, "", true);
        }
        removed
    }

    fn exists(&self, key: &str) -> bool {
        self.data.contains_key(key)
    }

    fn list(&self) -> Vec<String> {
        let mut keys: Vec<String> = self.data.keys().cloned().collect();
        keys.sort();
        keys
    }

    fn watch(&self, _key: &str) -> bool {
        true
    }

    fn subscribe(&mut self, key: &str, callback: ConfigChangeCallback) {
        self.subscribers
            .entry(key.to_string())
            .or_default()
            .push(callback);
    }
}

// ====================================================================
// 配置热更新（Watch）— 轮询式监听配置变更
// ====================================================================

/// 配置热更新监听器：以固定间隔轮询配置中心，检测到值变更时通知订阅者
///
/// 由于当前实现为纯内存模型，Watch 通过记录上次快照并与当前值比较来检测变更。
/// 在真实场景中可替换为 Consul/Nacos 的长轮询或 Watch API。
pub struct ConfigWatcher {
    /// 上次快照的配置键值对
    last_snapshot: Mutex<HashMap<String, String>>,
    /// 轮询间隔（毫秒）
    pub poll_interval_ms: u64,
    /// 变更回调列表：(key, callback)
    watchers: Mutex<Vec<(String, ConfigChangeCallback)>>,
}

impl ConfigWatcher {
    pub fn new(poll_interval_ms: u64) -> Self {
        Self {
            last_snapshot: Mutex::new(HashMap::new()),
            poll_interval_ms: poll_interval_ms.max(100),
            watchers: Mutex::new(Vec::new()),
        }
    }

    /// 注册一个 key 的变更监听
    pub fn watch(&self, key: &str, callback: ConfigChangeCallback) {
        if let Ok(mut watchers) = self.watchers.lock() {
            watchers.push((key.to_string(), callback));
        }
    }

    /// 执行一次轮询检测：比较当前配置与上次快照，触发变更回调
    /// 返回本次检测到的变更数量
    pub fn poll<C: ConfigCenter>(&self, center: &C) -> usize {
        let current: HashMap<String, String> = center
            .list()
            .into_iter()
            .filter_map(|k| center.get(&k).map(|v| (k, v)))
            .collect();

        let mut changes = Vec::new();
        {
            let Ok(mut snapshot) = self.last_snapshot.lock() else {
                return 0;
            };
            // 检测新增和修改
            for (k, v) in &current {
                match snapshot.get(k) {
                    Some(old) if old == v => {}
                    _ => changes.push((k.clone(), v.clone(), false)),
                }
            }
            // 检测删除
            for k in snapshot.keys() {
                if !current.contains_key(k) {
                    changes.push((k.clone(), String::new(), true));
                }
            }
            *snapshot = current;
        }

        let watcher_count = changes.len();
        if let Ok(watchers) = self.watchers.lock() {
            for (key, new_value, deleted) in &changes {
                for (watch_key, cb) in watchers.iter() {
                    if watch_key == key {
                        cb(key, new_value);
                    }
                }
                // 标记 deleted 仅用于日志，回调已收到空值
                let _ = deleted;
            }
        }
        watcher_count
    }

    /// 返回当前注册的 watcher 数量
    pub fn watcher_count(&self) -> usize {
        self.watchers
            .lock()
            .map(|w| w.len())
            .unwrap_or(0)
    }
}

impl Default for ConfigWatcher {
    fn default() -> Self {
        Self::new(5000)
    }
}

// ====================================================================
// 多源合并（文件 + 环境变量 + 远程）— 按优先级合并配置
// ====================================================================

/// 配置来源优先级：数值越大优先级越高，高优先级覆盖低优先级
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ConfigSourcePriority {
    /// 文件配置（最低优先级）
    File = 0,
    /// 远程配置中心
    Remote = 1,
    /// 环境变量（最高优先级）
    Env = 2,
}

/// 多源配置合并器：从文件、环境变量、远程配置中心收集配置并按优先级合并
pub struct MultiSourceConfig {
    /// 文件配置
    file_config: HashMap<String, String>,
    /// 环境变量配置
    env_config: HashMap<String, String>,
    /// 远程配置中心配置
    remote_config: HashMap<String, String>,
    /// 合并后的缓存
    merged: Mutex<Option<HashMap<String, String>>>,
}

impl MultiSourceConfig {
    pub fn new() -> Self {
        Self {
            file_config: HashMap::new(),
            env_config: HashMap::new(),
            remote_config: HashMap::new(),
            merged: Mutex::new(None),
        }
    }

    /// 设置文件来源配置
    pub fn set_file_config(&mut self, config: HashMap<String, String>) {
        self.file_config = config;
        self.invalidate_cache();
    }

    /// 设置环境变量来源配置
    pub fn set_env_config(&mut self, config: HashMap<String, String>) {
        self.env_config = config;
        self.invalidate_cache();
    }

    /// 设置远程配置中心来源配置
    pub fn set_remote_config(&mut self, config: HashMap<String, String>) {
        self.remote_config = config;
        self.invalidate_cache();
    }

    /// 从系统环境变量加载配置（前缀过滤）
    pub fn load_env_vars(&mut self, prefix: &str) {
        for (key, value) in std::env::vars() {
            if let Some(stripped) = key.strip_prefix(prefix) {
                // 去掉前缀并转小写
                self.env_config.insert(stripped.to_lowercase(), value);
            }
        }
        self.invalidate_cache();
    }

    /// 从 JSON 文件内容加载配置
    pub fn load_json(&mut self, json: &str) -> Result<(), String> {
        let parsed: HashMap<String, String> =
            serde_json::from_str(json).map_err(|e| format!("parse json error: {}", e))?;
        self.file_config = parsed;
        self.invalidate_cache();
        Ok(())
    }

    /// 合并所有来源并返回结果（高优先级覆盖低优先级）
    pub fn merge(&self) -> HashMap<String, String> {
        if let Ok(cache) = self.merged.lock() {
            if let Some(cached) = cache.as_ref() {
                return cached.clone();
            }
        }
        let mut result = HashMap::new();
        // 按 File -> Remote -> Env 顺序合并
        for (k, v) in &self.file_config {
            result.insert(k.clone(), v.clone());
        }
        for (k, v) in &self.remote_config {
            result.insert(k.clone(), v.clone());
        }
        for (k, v) in &self.env_config {
            result.insert(k.clone(), v.clone());
        }
        if let Ok(mut cache) = self.merged.lock() {
            *cache = Some(result.clone());
        }
        result
    }

    /// 获取合并后的配置值
    pub fn get(&self, key: &str) -> Option<String> {
        self.merge().get(key).cloned()
    }

    /// 返回某个 key 的来源优先级
    pub fn source_of(&self, key: &str) -> Option<ConfigSourcePriority> {
        if self.env_config.contains_key(key) {
            Some(ConfigSourcePriority::Env)
        } else if self.remote_config.contains_key(key) {
            Some(ConfigSourcePriority::Remote)
        } else if self.file_config.contains_key(key) {
            Some(ConfigSourcePriority::File)
        } else {
            None
        }
    }

    fn invalidate_cache(&self) {
        if let Ok(mut cache) = self.merged.lock() {
            *cache = None;
        }
    }
}

impl Default for MultiSourceConfig {
    fn default() -> Self {
        Self::new()
    }
}

// ====================================================================
// 配置验证（Schema Validation）
// ====================================================================

/// 配置字段类型约束
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConfigFieldType {
    String,
    Integer,
    Float,
    Boolean,
    Url,
    Email,
}

/// 配置字段 schema 定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigFieldSchema {
    /// 字段名
    pub name: String,
    /// 字段类型
    pub field_type: ConfigFieldType,
    /// 是否必填
    pub required: bool,
    /// 最小值（数值类型）
    pub min: Option<f64>,
    /// 最大值（数值类型）
    pub max: Option<f64>,
    /// 最小长度（字符串类型）
    pub min_length: Option<usize>,
    /// 最大长度（字符串类型）
    pub max_length: Option<usize>,
    /// 枚举允许值
    pub allowed_values: Option<Vec<String>>,
}

impl ConfigFieldSchema {
    pub fn new(name: impl Into<String>, field_type: ConfigFieldType) -> Self {
        Self {
            name: name.into(),
            field_type,
            required: false,
            min: None,
            max: None,
            min_length: None,
            max_length: None,
            allowed_values: None,
        }
    }

    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    pub fn with_range(mut self, min: f64, max: f64) -> Self {
        self.min = Some(min);
        self.max = Some(max);
        self
    }

    pub fn with_length(mut self, min: usize, max: usize) -> Self {
        self.min_length = Some(min);
        self.max_length = Some(max);
        self
    }

    pub fn with_allowed_values(mut self, values: Vec<String>) -> Self {
        self.allowed_values = Some(values);
        self
    }
}

/// 配置 schema 验证器：根据字段定义验证配置值合法性
pub struct SchemaValidator {
    fields: Vec<ConfigFieldSchema>,
}

/// 验证错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    /// 必填字段缺失
    MissingRequired(String),
    /// 类型不匹配
    TypeMismatch(String, String),
    /// 数值超出范围
    OutOfRange(String, String),
    /// 长度超出限制
    LengthExceeded(String, String),
    /// 值不在允许枚举内
    NotAllowed(String, String),
    /// 格式无效
    InvalidFormat(String, String),
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationError::MissingRequired(k) => write!(f, "missing required field: {}", k),
            ValidationError::TypeMismatch(k, v) => write!(f, "type mismatch for {}: {}", k, v),
            ValidationError::OutOfRange(k, v) => write!(f, "value out of range for {}: {}", k, v),
            ValidationError::LengthExceeded(k, v) => {
                write!(f, "length exceeded for {}: {}", k, v)
            }
            ValidationError::NotAllowed(k, v) => write!(f, "value not allowed for {}: {}", k, v),
            ValidationError::InvalidFormat(k, v) => write!(f, "invalid format for {}: {}", k, v),
        }
    }
}

impl SchemaValidator {
    pub fn new() -> Self {
        Self { fields: Vec::new() }
    }

    pub fn add_field(&mut self, schema: ConfigFieldSchema) {
        self.fields.push(schema);
    }

    /// 验证配置 map，返回所有验证错误（空 Vec 表示通过）
    pub fn validate(&self, config: &HashMap<String, String>) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        for field in &self.fields {
            match config.get(&field.name) {
                None => {
                    if field.required {
                        errors.push(ValidationError::MissingRequired(field.name.clone()));
                    }
                }
                Some(value) => {
                    if let Err(e) = self.validate_field(field, value) {
                        errors.push(e);
                    }
                }
            }
        }
        errors
    }

    fn validate_field(
        &self,
        schema: &ConfigFieldSchema,
        value: &str,
    ) -> Result<(), ValidationError> {
        // 枚举值检查
        if let Some(allowed) = &schema.allowed_values {
            if !allowed.iter().any(|v| v == value) {
                return Err(ValidationError::NotAllowed(
                    schema.name.clone(),
                    value.to_string(),
                ));
            }
        }

        match schema.field_type {
            ConfigFieldType::String => {
                if let Some(max_len) = schema.max_length {
                    if value.len() > max_len {
                        return Err(ValidationError::LengthExceeded(
                            schema.name.clone(),
                            format!("len={} > {}", value.len(), max_len),
                        ));
                    }
                }
                if let Some(min_len) = schema.min_length {
                    if value.len() < min_len {
                        return Err(ValidationError::LengthExceeded(
                            schema.name.clone(),
                            format!("len={} < {}", value.len(), min_len),
                        ));
                    }
                }
            }
            ConfigFieldType::Integer => {
                let n: i64 = value.parse().map_err(|_| {
                    ValidationError::TypeMismatch(schema.name.clone(), value.to_string())
                })?;
                if let Some(min) = schema.min {
                    if (n as f64) < min {
                        return Err(ValidationError::OutOfRange(
                            schema.name.clone(),
                            format!("{} < {}", n, min),
                        ));
                    }
                }
                if let Some(max) = schema.max {
                    if (n as f64) > max {
                        return Err(ValidationError::OutOfRange(
                            schema.name.clone(),
                            format!("{} > {}", n, max),
                        ));
                    }
                }
            }
            ConfigFieldType::Float => {
                let n: f64 = value.parse().map_err(|_| {
                    ValidationError::TypeMismatch(schema.name.clone(), value.to_string())
                })?;
                if let Some(min) = schema.min {
                    if n < min {
                        return Err(ValidationError::OutOfRange(
                            schema.name.clone(),
                            format!("{} < {}", n, min),
                        ));
                    }
                }
                if let Some(max) = schema.max {
                    if n > max {
                        return Err(ValidationError::OutOfRange(
                            schema.name.clone(),
                            format!("{} > {}", n, max),
                        ));
                    }
                }
            }
            ConfigFieldType::Boolean => {
                if value != "true" && value != "false" {
                    return Err(ValidationError::TypeMismatch(
                        schema.name.clone(),
                        value.to_string(),
                    ));
                }
            }
            ConfigFieldType::Url => {
                if !value.starts_with("http://") && !value.starts_with("https://") {
                    return Err(ValidationError::InvalidFormat(
                        schema.name.clone(),
                        "not a valid URL".to_string(),
                    ));
                }
            }
            ConfigFieldType::Email => {
                if !value.contains('@') || !value.contains('.') {
                    return Err(ValidationError::InvalidFormat(
                        schema.name.clone(),
                        "not a valid email".to_string(),
                    ));
                }
            }
        }
        Ok(())
    }
}

impl Default for SchemaValidator {
    fn default() -> Self {
        Self::new()
    }
}

// ====================================================================
// 配置加密 — 对敏感配置值进行加解密
// ====================================================================

/// 配置加密器：使用 XOR + Base64 对敏感配置值进行对称加密
///
/// 适用场景：数据库密码、API Key 等敏感配置不应明文存储在配置文件中。
/// 加密格式：`ENC(base64(xor(plaintext, key)))`
pub struct ConfigEncryption {
    /// 加密密钥
    key: Vec<u8>,
}

/// 加密值前缀，用于标识已加密的配置项
pub const ENCRYPTED_PREFIX: &str = "ENC(";
pub const ENCRYPTED_SUFFIX: &str = ")";

impl ConfigEncryption {
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into().into_bytes(),
        }
    }

    /// 加密明文，返回 `ENC(base64)` 格式字符串
    pub fn encrypt(&self, plaintext: &str) -> String {
        let bytes = plaintext.as_bytes();
        let encrypted: Vec<u8> = bytes
            .iter()
            .enumerate()
            .map(|(i, &b)| b ^ self.key[i % self.key.len()])
            .collect();
        let encoded = base64_encode(&encrypted);
        format!("{}{}{}", ENCRYPTED_PREFIX, encoded, ENCRYPTED_SUFFIX)
    }

    /// 解密 `ENC(base64)` 格式字符串，返回明文
    pub fn decrypt(&self, ciphertext: &str) -> Result<String, String> {
        let inner = ciphertext
            .strip_prefix(ENCRYPTED_PREFIX)
            .and_then(|s| s.strip_suffix(ENCRYPTED_SUFFIX))
            .ok_or_else(|| "invalid encrypted format, expected ENC(...)".to_string())?;
        let decoded = base64_decode(inner)?;
        let decrypted: Vec<u8> = decoded
            .iter()
            .enumerate()
            .map(|(i, &b)| b ^ self.key[i % self.key.len()])
            .collect();
        String::from_utf8(decrypted).map_err(|e| format!("decrypt utf8 error: {}", e))
    }

    /// 判断值是否已加密（以 ENC( 开头）
    pub fn is_encrypted(value: &str) -> bool {
        value.starts_with(ENCRYPTED_PREFIX) && value.ends_with(ENCRYPTED_SUFFIX)
    }

    /// 如果值已加密则解密，否则原样返回
    pub fn decrypt_if_needed(&self, value: &str) -> Result<String, String> {
        if Self::is_encrypted(value) {
            self.decrypt(value)
        } else {
            Ok(value.to_string())
        }
    }

    /// 批量解密配置 map 中所有已加密的值
    pub fn decrypt_config(
        &self,
        config: &HashMap<String, String>,
    ) -> Result<HashMap<String, String>, String> {
        let mut result = HashMap::new();
        for (k, v) in config {
            let decrypted = self.decrypt_if_needed(v)?;
            result.insert(k.clone(), decrypted);
        }
        Ok(result)
    }
}

/// 简易 Base64 编码（不依赖外部 crate）
fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity(data.len().div_ceil(3) * 4);
    let mut i = 0;
    while i + 3 <= data.len() {
        let n = ((data[i] as u32) << 16) | ((data[i + 1] as u32) << 8) | (data[i + 2] as u32);
        result.push(CHARS[((n >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((n >> 12) & 0x3F) as usize] as char);
        result.push(CHARS[((n >> 6) & 0x3F) as usize] as char);
        result.push(CHARS[(n & 0x3F) as usize] as char);
        i += 3;
    }
    let remaining = data.len() - i;
    if remaining == 1 {
        let n = (data[i] as u32) << 16;
        result.push(CHARS[((n >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((n >> 12) & 0x3F) as usize] as char);
        result.push('=');
        result.push('=');
    } else if remaining == 2 {
        let n = ((data[i] as u32) << 16) | ((data[i + 1] as u32) << 8);
        result.push(CHARS[((n >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((n >> 12) & 0x3F) as usize] as char);
        result.push(CHARS[((n >> 6) & 0x3F) as usize] as char);
        result.push('=');
    }
    result
}

/// 简易 Base64 解码
fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    fn char_to_val(c: u8) -> Result<u8, String> {
        CHARS
            .iter()
            .position(|&ch| ch == c)
            .map(|p| p as u8)
            .ok_or_else(|| format!("invalid base64 char: {}", c as char))
    }
    let trimmed = s.trim_end_matches('=');
    let bytes = trimmed.as_bytes();
    let mut result = Vec::with_capacity(bytes.len() * 3 / 4);
    let mut i = 0;
    while i + 4 <= bytes.len() {
        let a = char_to_val(bytes[i])? as u32;
        let b = char_to_val(bytes[i + 1])? as u32;
        let c = char_to_val(bytes[i + 2])? as u32;
        let d = char_to_val(bytes[i + 3])? as u32;
        let n = (a << 18) | (b << 12) | (c << 6) | d;
        result.push((n >> 16) as u8);
        result.push((n >> 8) as u8);
        result.push(n as u8);
        i += 4;
    }
    let remaining = bytes.len() - i;
    if remaining == 2 {
        let a = char_to_val(bytes[i])? as u32;
        let b = char_to_val(bytes[i + 1])? as u32;
        let n = (a << 18) | (b << 12);
        result.push((n >> 16) as u8);
    } else if remaining == 3 {
        let a = char_to_val(bytes[i])? as u32;
        let b = char_to_val(bytes[i + 1])? as u32;
        let c = char_to_val(bytes[i + 2])? as u32;
        let n = (a << 18) | (b << 12) | (c << 6);
        result.push((n >> 16) as u8);
        result.push((n >> 8) as u8);
    } else if remaining != 0 {
        return Err(format!("invalid base64 length, remainder: {}", remaining));
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[test]
    fn test_consul_set_and_get() {
        let mut c = ConsulConfigCenter::new();
        c.set("k", "v");
        assert_eq!(c.get("k"), Some("v".to_string()));
        assert!(c.exists("k"));
        assert!(!c.exists("missing"));
    }

    #[test]
    fn test_consul_get_missing() {
        let c = ConsulConfigCenter::new();
        assert_eq!(c.get("missing"), None);
    }

    #[test]
    fn test_consul_delete() {
        let mut c = ConsulConfigCenter::new();
        c.set("k", "v");
        assert!(c.delete("k"));
        assert!(!c.exists("k"));
        assert_eq!(c.get("k"), None);
        // Deleting a missing key returns false
        assert!(!c.delete("missing"));
    }

    #[test]
    fn test_consul_list_sorted() {
        let mut c = ConsulConfigCenter::new();
        c.set("z", "1");
        c.set("a", "2");
        c.set("m", "3");
        assert_eq!(c.list(), vec!["a", "m", "z"]);
    }

    #[test]
    fn test_consul_watch_returns_true() {
        let c = ConsulConfigCenter::new();
        assert!(c.watch("any-key"));
    }

    #[test]
    fn test_nacos_set_and_get() {
        let mut c = NacosConfigCenter::new();
        c.set("k", "v");
        assert_eq!(c.get("k"), Some("v".to_string()));
    }

    #[test]
    fn test_nacos_watch_returns_true() {
        let c = NacosConfigCenter::new();
        assert!(c.watch("k"));
    }

    #[test]
    fn test_nacos_delete() {
        let mut c = NacosConfigCenter::new();
        c.set("k", "v");
        assert!(c.delete("k"));
        assert_eq!(c.get("k"), None);
    }

    #[test]
    fn test_nacos_list_sorted() {
        let mut c = NacosConfigCenter::new();
        c.set("b", "1");
        c.set("a", "2");
        assert_eq!(c.list(), vec!["a", "b"]);
    }

    // ---- Subscribe / notify tests ----

    #[test]
    fn test_consul_subscribe_receives_set_events() {
        let mut c = ConsulConfigCenter::new();
        let count = Arc::new(AtomicU32::new(0));
        let last_value = Arc::new(Mutex::new(String::new()));

        let cb_count = count.clone();
        let cb_value = last_value.clone();
        c.subscribe(
            "app.config",
            Arc::new(move |_key, value| {
                cb_count.fetch_add(1, Ordering::SeqCst);
                *cb_value.lock().unwrap() = value.to_string();
            }),
        );

        c.set("app.config", "v1");
        assert_eq!(count.load(Ordering::SeqCst), 1);
        assert_eq!(*last_value.lock().unwrap(), "v1");

        c.set("app.config", "v2");
        assert_eq!(count.load(Ordering::SeqCst), 2);
        assert_eq!(*last_value.lock().unwrap(), "v2");
    }

    #[test]
    fn test_consul_subscribe_receives_delete_events() {
        let mut c = ConsulConfigCenter::new();
        let deleted = Arc::new(AtomicU32::new(0));
        let last_value = Arc::new(Mutex::new(String::new()));

        let d_count = deleted.clone();
        let d_value = last_value.clone();
        c.subscribe(
            "k",
            Arc::new(move |_key, value| {
                d_count.fetch_add(1, Ordering::SeqCst);
                *d_value.lock().unwrap() = value.to_string();
            }),
        );

        c.set("k", "v");
        c.delete("k");
        assert_eq!(deleted.load(Ordering::SeqCst), 2); // set + delete
        assert_eq!(*last_value.lock().unwrap(), ""); // delete sends empty
    }

    #[test]
    fn test_consul_multiple_subscribers() {
        let mut c = ConsulConfigCenter::new();
        let c1 = Arc::new(AtomicU32::new(0));
        let c2 = Arc::new(AtomicU32::new(0));

        let c1_clone = c1.clone();
        c.subscribe(
            "k",
            Arc::new(move |_key, _value| {
                c1_clone.fetch_add(1, Ordering::SeqCst);
            }),
        );

        let c2_clone = c2.clone();
        c.subscribe(
            "k",
            Arc::new(move |_key, _value| {
                c2_clone.fetch_add(1, Ordering::SeqCst);
            }),
        );

        assert_eq!(c.subscriber_count("k"), 2);
        c.set("k", "v");
        assert_eq!(c1.load(Ordering::SeqCst), 1);
        assert_eq!(c2.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_consul_subscribers_are_keyed() {
        let mut c = ConsulConfigCenter::new();
        let other_count = Arc::new(AtomicU32::new(0));
        let oc = other_count.clone();
        c.subscribe(
            "other",
            Arc::new(move |_key, _value| {
                oc.fetch_add(1, Ordering::SeqCst);
            }),
        );

        c.set("this", "v");
        // Should not notify subscribers of "other"
        assert_eq!(other_count.load(Ordering::SeqCst), 0);

        c.set("other", "v");
        assert_eq!(other_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_consul_events_record() {
        let mut c = ConsulConfigCenter::new();
        c.set("a", "1");
        c.set("b", "2");
        c.delete("a");

        let events = c.events();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].key, "a");
        assert_eq!(events[0].value, "1");
        assert!(!events[0].deleted);
        assert_eq!(events[2].key, "a");
        assert!(events[2].deleted);
    }

    #[test]
    fn test_nacos_subscribe_receives_events() {
        let mut c = NacosConfigCenter::new();
        let received = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
        let r = received.clone();
        c.subscribe(
            "cfg",
            Arc::new(move |key, value| {
                r.lock().unwrap().push((key.to_string(), value.to_string()));
            }),
        );
        c.set("cfg", "v1");
        c.set("cfg", "v2");
        let received = received.lock().unwrap();
        assert_eq!(
            *received,
            vec![
                ("cfg".to_string(), "v1".to_string()),
                ("cfg".to_string(), "v2".to_string()),
            ]
        );
    }

    #[test]
    fn test_nacos_events_record() {
        let mut c = NacosConfigCenter::new();
        c.set("k", "v");
        let events = c.events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].key, "k");
        assert_eq!(events[0].value, "v");
    }

    #[test]
    fn test_subscribe_via_trait_object() {
        // Verify subscribe works through a boxed trait object.
        let mut boxed: Box<dyn ConfigCenter> = Box::new(ConsulConfigCenter::new());
        let count = Arc::new(AtomicU32::new(0));
        let c = count.clone();
        boxed.subscribe(
            "k",
            Arc::new(move |_key, _value| {
                c.fetch_add(1, Ordering::SeqCst);
            }),
        );
        boxed.set("k", "v");
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_overwrite_existing_value_notifies() {
        let mut c = ConsulConfigCenter::new();
        let count = Arc::new(AtomicU32::new(0));
        let c1 = count.clone();
        c.subscribe(
            "k",
            Arc::new(move |_key, _value| {
                c1.fetch_add(1, Ordering::SeqCst);
            }),
        );
        c.set("k", "v1");
        c.set("k", "v2");
        c.set("k", "v3");
        assert_eq!(count.load(Ordering::SeqCst), 3);
    }

    // ====================================================================
    // ConfigWatcher 测试
    // ====================================================================

    #[test]
    fn test_config_watcher_detects_new_key() {
        let mut center = ConsulConfigCenter::new();
        let watcher = ConfigWatcher::new(1000);
        let received = Arc::new(Mutex::new(String::new()));
        let r = received.clone();
        watcher.watch("app.port", Arc::new(move |_k, v| {
            *r.lock().unwrap() = v.to_string();
        }));

        center.set("app.port", "8080");
        let changes = watcher.poll(&center);
        assert_eq!(changes, 1);
        assert_eq!(*received.lock().unwrap(), "8080");
    }

    #[test]
    fn test_config_watcher_detects_value_change() {
        let mut center = ConsulConfigCenter::new();
        center.set("k", "v1");
        let watcher = ConfigWatcher::new(1000);
        // 第一次 poll 建立基线
        watcher.poll(&center);

        let received = Arc::new(AtomicU32::new(0));
        let r = received.clone();
        watcher.watch("k", Arc::new(move |_, _| {
            r.fetch_add(1, Ordering::SeqCst);
        }));

        center.set("k", "v2");
        let changes = watcher.poll(&center);
        assert_eq!(changes, 1);
        assert_eq!(received.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_config_watcher_detects_deletion() {
        let mut center = ConsulConfigCenter::new();
        center.set("k", "v");
        let watcher = ConfigWatcher::new(1000);
        watcher.poll(&center);

        let deleted_value = Arc::new(Mutex::new("not_empty".to_string()));
        let d = deleted_value.clone();
        watcher.watch("k", Arc::new(move |_, v| {
            *d.lock().unwrap() = v.to_string();
        }));

        center.delete("k");
        let changes = watcher.poll(&center);
        assert_eq!(changes, 1);
        assert_eq!(*deleted_value.lock().unwrap(), "");
    }

    #[test]
    fn test_config_watcher_no_change_returns_zero() {
        let mut center = ConsulConfigCenter::new();
        center.set("k", "v");
        let watcher = ConfigWatcher::new(1000);
        watcher.poll(&center);
        // 再次 poll 无变更
        let changes = watcher.poll(&center);
        assert_eq!(changes, 0);
    }

    #[test]
    fn test_config_watcher_watcher_count() {
        let watcher = ConfigWatcher::new(1000);
        assert_eq!(watcher.watcher_count(), 0);
        watcher.watch("a", Arc::new(|_, _| {}));
        watcher.watch("b", Arc::new(|_, _| {}));
        assert_eq!(watcher.watcher_count(), 2);
    }

    #[test]
    fn test_config_watcher_default_interval() {
        let watcher = ConfigWatcher::default();
        assert_eq!(watcher.poll_interval_ms, 5000);
    }

    #[test]
    fn test_config_watcher_min_interval_clamped() {
        let watcher = ConfigWatcher::new(10);
        assert_eq!(watcher.poll_interval_ms, 100);
    }

    // ====================================================================
    // MultiSourceConfig 测试
    // ====================================================================

    #[test]
    fn test_multi_source_env_overrides_remote_overrides_file() {
        let mut ms = MultiSourceConfig::new();
        let mut file = HashMap::new();
        file.insert("k".to_string(), "file_val".to_string());
        ms.set_file_config(file);

        let mut remote = HashMap::new();
        remote.insert("k".to_string(), "remote_val".to_string());
        ms.set_remote_config(remote);

        let mut env = HashMap::new();
        env.insert("k".to_string(), "env_val".to_string());
        ms.set_env_config(env);

        assert_eq!(ms.get("k"), Some("env_val".to_string()));
        assert_eq!(ms.source_of("k"), Some(ConfigSourcePriority::Env));
    }

    #[test]
    fn test_multi_source_remote_overrides_file() {
        let mut ms = MultiSourceConfig::new();
        let mut file = HashMap::new();
        file.insert("k".to_string(), "file_val".to_string());
        ms.set_file_config(file);

        let mut remote = HashMap::new();
        remote.insert("k".to_string(), "remote_val".to_string());
        ms.set_remote_config(remote);

        assert_eq!(ms.get("k"), Some("remote_val".to_string()));
        assert_eq!(ms.source_of("k"), Some(ConfigSourcePriority::Remote));
    }

    #[test]
    fn test_multi_source_file_only() {
        let mut ms = MultiSourceConfig::new();
        let mut file = HashMap::new();
        file.insert("k".to_string(), "file_val".to_string());
        ms.set_file_config(file);

        assert_eq!(ms.get("k"), Some("file_val".to_string()));
        assert_eq!(ms.source_of("k"), Some(ConfigSourcePriority::File));
    }

    #[test]
    fn test_multi_source_missing_key_returns_none() {
        let ms = MultiSourceConfig::new();
        assert_eq!(ms.get("missing"), None);
        assert_eq!(ms.source_of("missing"), None);
    }

    #[test]
    fn test_multi_source_merge_combines_all_keys() {
        let mut ms = MultiSourceConfig::new();
        let mut file = HashMap::new();
        file.insert("file_key".to_string(), "fv".to_string());
        ms.set_file_config(file);

        let mut remote = HashMap::new();
        remote.insert("remote_key".to_string(), "rv".to_string());
        ms.set_remote_config(remote);

        let mut env = HashMap::new();
        env.insert("env_key".to_string(), "ev".to_string());
        ms.set_env_config(env);

        let merged = ms.merge();
        assert_eq!(merged.len(), 3);
        assert_eq!(merged.get("file_key"), Some(&"fv".to_string()));
        assert_eq!(merged.get("remote_key"), Some(&"rv".to_string()));
        assert_eq!(merged.get("env_key"), Some(&"ev".to_string()));
    }

    #[test]
    fn test_multi_source_load_json() {
        let mut ms = MultiSourceConfig::new();
        ms.load_json(r#"{"host":"localhost","port":"3306"}"#)
            .unwrap();
        assert_eq!(ms.get("host"), Some("localhost".to_string()));
        assert_eq!(ms.get("port"), Some("3306".to_string()));
    }

    #[test]
    fn test_multi_source_load_json_invalid() {
        let mut ms = MultiSourceConfig::new();
        assert!(ms.load_json("not json").is_err());
    }

    #[test]
    fn test_multi_source_cache_invalidated_on_update() {
        let mut ms = MultiSourceConfig::new();
        let mut file = HashMap::new();
        file.insert("k".to_string(), "v1".to_string());
        ms.set_file_config(file);
        assert_eq!(ms.get("k"), Some("v1".to_string()));

        let mut file2 = HashMap::new();
        file2.insert("k".to_string(), "v2".to_string());
        ms.set_file_config(file2);
        assert_eq!(ms.get("k"), Some("v2".to_string()));
    }

    // ====================================================================
    // SchemaValidator 测试
    // ====================================================================

    #[test]
    fn test_schema_validates_integer_in_range() {
        let mut validator = SchemaValidator::new();
        validator.add_field(
            ConfigFieldSchema::new("port", ConfigFieldType::Integer)
                .required()
                .with_range(1.0, 65535.0),
        );
        let mut config = HashMap::new();
        config.insert("port".to_string(), "3306".to_string());
        let errors = validator.validate(&config);
        assert!(errors.is_empty(), "expected no errors, got: {:?}", errors);
    }

    #[test]
    fn test_schema_missing_required_field() {
        let mut validator = SchemaValidator::new();
        validator.add_field(
            ConfigFieldSchema::new("port", ConfigFieldType::Integer).required(),
        );
        let config = HashMap::new();
        let errors = validator.validate(&config);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0], ValidationError::MissingRequired("port".to_string()));
    }

    #[test]
    fn test_schema_integer_out_of_range() {
        let mut validator = SchemaValidator::new();
        validator.add_field(
            ConfigFieldSchema::new("port", ConfigFieldType::Integer).with_range(1.0, 65535.0),
        );
        let mut config = HashMap::new();
        config.insert("port".to_string(), "99999".to_string());
        let errors = validator.validate(&config);
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], ValidationError::OutOfRange(_, _)));
    }

    #[test]
    fn test_schema_type_mismatch() {
        let mut validator = SchemaValidator::new();
        validator.add_field(ConfigFieldSchema::new("port", ConfigFieldType::Integer));
        let mut config = HashMap::new();
        config.insert("port".to_string(), "not_a_number".to_string());
        let errors = validator.validate(&config);
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], ValidationError::TypeMismatch(_, _)));
    }

    #[test]
    fn test_schema_boolean_valid() {
        let mut validator = SchemaValidator::new();
        validator.add_field(ConfigFieldSchema::new("debug", ConfigFieldType::Boolean));
        let mut config = HashMap::new();
        config.insert("debug".to_string(), "true".to_string());
        assert!(validator.validate(&config).is_empty());

        config.insert("debug".to_string(), "false".to_string());
        assert!(validator.validate(&config).is_empty());
    }

    #[test]
    fn test_schema_boolean_invalid() {
        let mut validator = SchemaValidator::new();
        validator.add_field(ConfigFieldSchema::new("debug", ConfigFieldType::Boolean));
        let mut config = HashMap::new();
        config.insert("debug".to_string(), "yes".to_string());
        let errors = validator.validate(&config);
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], ValidationError::TypeMismatch(_, _)));
    }

    #[test]
    fn test_schema_string_length() {
        let mut validator = SchemaValidator::new();
        validator.add_field(
            ConfigFieldSchema::new("name", ConfigFieldType::String).with_length(2, 10),
        );
        let mut config = HashMap::new();
        config.insert("name".to_string(), "ab".to_string());
        assert!(validator.validate(&config).is_empty());

        config.insert("name".to_string(), "a".to_string());
        assert_eq!(validator.validate(&config).len(), 1);

        config.insert("name".to_string(), "this_is_way_too_long".to_string());
        assert_eq!(validator.validate(&config).len(), 1);
    }

    #[test]
    fn test_schema_allowed_values() {
        let mut validator = SchemaValidator::new();
        validator.add_field(
            ConfigFieldSchema::new("level", ConfigFieldType::String)
                .with_allowed_values(vec!["info".into(), "warn".into(), "error".into()]),
        );
        let mut config = HashMap::new();
        config.insert("level".to_string(), "info".to_string());
        assert!(validator.validate(&config).is_empty());

        config.insert("level".to_string(), "debug".to_string());
        let errors = validator.validate(&config);
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], ValidationError::NotAllowed(_, _)));
    }

    #[test]
    fn test_schema_url_format() {
        let mut validator = SchemaValidator::new();
        validator.add_field(ConfigFieldSchema::new("endpoint", ConfigFieldType::Url));
        let mut config = HashMap::new();
        config.insert("endpoint".to_string(), "https://example.com".to_string());
        assert!(validator.validate(&config).is_empty());

        config.insert("endpoint".to_string(), "ftp://bad".to_string());
        assert_eq!(validator.validate(&config).len(), 1);
    }

    #[test]
    fn test_schema_email_format() {
        let mut validator = SchemaValidator::new();
        validator.add_field(ConfigFieldSchema::new("email", ConfigFieldType::Email));
        let mut config = HashMap::new();
        config.insert("email".to_string(), "user@example.com".to_string());
        assert!(validator.validate(&config).is_empty());

        config.insert("email".to_string(), "not_an_email".to_string());
        assert_eq!(validator.validate(&config).len(), 1);
    }

    #[test]
    fn test_schema_float_range() {
        let mut validator = SchemaValidator::new();
        validator.add_field(
            ConfigFieldSchema::new("ratio", ConfigFieldType::Float).with_range(0.0, 1.0),
        );
        let mut config = HashMap::new();
        config.insert("ratio".to_string(), "0.5".to_string());
        assert!(validator.validate(&config).is_empty());

        config.insert("ratio".to_string(), "1.5".to_string());
        assert_eq!(validator.validate(&config).len(), 1);
    }

    #[test]
    fn test_schema_multiple_errors() {
        let mut validator = SchemaValidator::new();
        validator.add_field(ConfigFieldSchema::new("a", ConfigFieldType::Integer).required());
        validator.add_field(ConfigFieldSchema::new("b", ConfigFieldType::Boolean).required());
        let config = HashMap::new();
        let errors = validator.validate(&config);
        assert_eq!(errors.len(), 2);
    }

    #[test]
    fn test_schema_optional_field_missing_ok() {
        let mut validator = SchemaValidator::new();
        validator.add_field(ConfigFieldSchema::new("optional", ConfigFieldType::String));
        let config = HashMap::new();
        assert!(validator.validate(&config).is_empty());
    }

    // ====================================================================
    // ConfigEncryption 测试
    // ====================================================================

    #[test]
    fn test_encryption_roundtrip() {
        let enc = ConfigEncryption::new("my_secret_key");
        let plaintext = "database_password_123";
        let ciphertext = enc.encrypt(plaintext);
        assert!(ConfigEncryption::is_encrypted(&ciphertext));
        let decrypted = enc.decrypt(&ciphertext).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encryption_is_encrypted_detection() {
        assert!(ConfigEncryption::is_encrypted("ENC(abc123)"));
        assert!(!ConfigEncryption::is_encrypted("plaintext"));
        assert!(!ConfigEncryption::is_encrypted("ENC(incomplete"));
    }

    #[test]
    fn test_encryption_decrypt_if_needed_for_plaintext() {
        let enc = ConfigEncryption::new("key");
        let result = enc.decrypt_if_needed("plain_value").unwrap();
        assert_eq!(result, "plain_value");
    }

    #[test]
    fn test_encryption_decrypt_if_needed_for_ciphertext() {
        let enc = ConfigEncryption::new("key");
        let ciphertext = enc.encrypt("secret");
        let result = enc.decrypt_if_needed(&ciphertext).unwrap();
        assert_eq!(result, "secret");
    }

    #[test]
    fn test_encryption_decrypt_invalid_format_errors() {
        let enc = ConfigEncryption::new("key");
        assert!(enc.decrypt("not_encrypted").is_err());
        assert!(enc.decrypt("ENC(incomplete").is_err());
    }

    #[test]
    fn test_encryption_decrypt_config_batch() {
        let enc = ConfigEncryption::new("master_key");
        let mut config = HashMap::new();
        config.insert("host".to_string(), "localhost".to_string());
        config.insert("password".to_string(), enc.encrypt("s3cr3t"));
        config.insert("api_key".to_string(), enc.encrypt("abc123"));

        let decrypted = enc.decrypt_config(&config).unwrap();
        assert_eq!(decrypted.get("host"), Some(&"localhost".to_string()));
        assert_eq!(decrypted.get("password"), Some(&"s3cr3t".to_string()));
        assert_eq!(decrypted.get("api_key"), Some(&"abc123".to_string()));
    }

    #[test]
    fn test_encryption_different_keys_produce_different_output() {
        let enc1 = ConfigEncryption::new("key1");
        let enc2 = ConfigEncryption::new("key2");
        let plaintext = "same_secret";
        let c1 = enc1.encrypt(plaintext);
        let c2 = enc2.encrypt(plaintext);
        assert_ne!(c1, c2);
    }

    #[test]
    fn test_encryption_same_key_same_plaintext_deterministic() {
        let enc = ConfigEncryption::new("key");
        let c1 = enc.encrypt("secret");
        let c2 = enc.encrypt("secret");
        assert_eq!(c1, c2);
    }

    #[test]
    fn test_base64_encode_decode_roundtrip() {
        let data = b"hello world";
        let encoded = base64_encode(data);
        let decoded = base64_decode(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn test_base64_encode_known_value() {
        // "Man" -> "TWFu"
        assert_eq!(base64_encode(b"Man"), "TWFu");
        // "Ma" -> "TWE="
        assert_eq!(base64_encode(b"Ma"), "TWE=");
        // "M" -> "TQ=="
        assert_eq!(base64_encode(b"M"), "TQ==");
    }

    #[test]
    fn test_base64_decode_invalid_char_errors() {
        assert!(base64_decode("!!!").is_err());
    }

    #[test]
    fn test_base64_decode_empty_string() {
        let result = base64_decode("").unwrap();
        assert!(result.is_empty());
    }
}
