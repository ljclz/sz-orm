//! 真实 Consul HTTP API 客户端
//!
//! 基于 Consul KV Store API v1：https://developer.hashicorp.com/consul/api-docs/kv
//!
//! 支持：
//! - `get_config` / `set_config` / `delete_config` — KV 读写
//! - `watch` — 长轮询监听配置变更（blocking query via `X-Consul-Index`）
//! - `register_service` — 服务注册（Catalog API）
//! - ACL Token 认证（通过 `X-Consul-Token` header）

#![cfg(feature = "real-consul")]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Consul 客户端错误
#[derive(Debug, thiserror::Error)]
pub enum ConsulError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Consul API error: status={status}, body={body}")]
    Api { status: u16, body: String },
    #[error("Config not found: {0}")]
    NotFound(String),
    #[error("Invalid response: {0}")]
    InvalidResponse(String),
}

/// Consul 客户端配置
#[derive(Debug, Clone)]
pub struct ConsulConfig {
    pub endpoint: String,
    pub acl_token: Option<String>,
    pub datacenter: Option<String>,
    pub timeout_secs: u64,
}

impl Default for ConsulConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://127.0.0.1:8500".to_string(),
            acl_token: None,
            datacenter: None,
            timeout_secs: 10,
        }
    }
}

impl ConsulConfig {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            ..Default::default()
        }
    }

    pub fn with_acl_token(mut self, token: impl Into<String>) -> Self {
        self.acl_token = Some(token.into());
        self
    }

    pub fn with_datacenter(mut self, dc: impl Into<String>) -> Self {
        self.datacenter = Some(dc.into());
        self
    }
}

/// Consul KV 条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsulKvEntry {
    #[serde(rename = "Key")]
    pub key: String,
    #[serde(rename = "Value")]
    pub value: Option<String>,
    #[serde(rename = "CreateIndex")]
    pub create_index: Option<u64>,
    #[serde(rename = "ModifyIndex")]
    pub modify_index: Option<u64>,
}

/// Consul 服务注册请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsulServiceRegistration {
    #[serde(rename = "ID", skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Address", skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    #[serde(rename = "Port", skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(rename = "Tags", skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(rename = "Meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<HashMap<String, String>>,
}

/// 真实 Consul HTTP API 客户端
pub struct ConsulClient {
    config: ConsulConfig,
    http: reqwest::Client,
}

impl ConsulClient {
    pub fn new(config: ConsulConfig) -> Result<Self, ConsulError> {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()?;
        Ok(Self { config, http })
    }

    fn build_url(&self, path: &str) -> String {
        format!("{}{}", self.config.endpoint, path)
    }

    fn add_auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let Some(token) = &self.config.acl_token {
            req.header("X-Consul-Token", token)
        } else {
            req
        }
    }

    fn add_dc(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let Some(dc) = &self.config.datacenter {
            req.query(&[("dc", dc)])
        } else {
            req
        }
    }

    /// 读取 KV 配置值
    pub async fn get_config(&self, key: &str) -> Result<String, ConsulError> {
        let url = self.build_url(&format!("/v1/kv/{}", key));
        let req = self.http.get(&url);
        let req = self.add_auth(req);
        let req = self.add_dc(req);
        let resp = req.send().await?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(ConsulError::NotFound(key.to_string()));
        }
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(ConsulError::Api { status, body });
        }

        let entries: Vec<ConsulKvEntry> = resp.json().await?;
        if entries.is_empty() {
            return Err(ConsulError::NotFound(key.to_string()));
        }
        let encoded = entries[0]
            .value
            .as_ref()
            .ok_or_else(|| ConsulError::InvalidResponse("missing Value field".into()))?;
        let decoded = base64_decode(encoded)?;
        Ok(decoded)
    }

    /// 写入 KV 配置值
    pub async fn set_config(&self, key: &str, value: &str) -> Result<(), ConsulError> {
        let url = self.build_url(&format!("/v1/kv/{}", key));
        let req = self.http.put(&url).body(value.to_string());
        let req = self.add_auth(req);
        let req = self.add_dc(req);
        let resp = req.send().await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(ConsulError::Api { status, body });
        }
        Ok(())
    }

    /// 删除 KV 配置
    pub async fn delete_config(&self, key: &str) -> Result<(), ConsulError> {
        let url = self.build_url(&format!("/v1/kv/{}", key));
        let req = self.http.delete(&url);
        let req = self.add_auth(req);
        let req = self.add_dc(req);
        let resp = req.send().await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(ConsulError::Api { status, body });
        }
        Ok(())
    }

    /// 列出指定前缀下所有 KV 键
    pub async fn list_keys(&self, prefix: &str) -> Result<Vec<String>, ConsulError> {
        let url = self.build_url(&format!("/v1/kv/{}?keys", prefix));
        let req = self.http.get(&url);
        let req = self.add_auth(req);
        let req = self.add_dc(req);
        let resp = req.send().await?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(Vec::new());
        }
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(ConsulError::Api { status, body });
        }
        let keys: Vec<String> = resp.json().await?;
        Ok(keys)
    }

    /// 长轮询监听配置变更
    ///
    /// 返回变更后的值和新的 index。调用方可循环调用此方法实现持续监听。
    /// `wait_secs` 为长轮询等待时间（Consul 默认最大 10 分钟）。
    pub async fn watch(
        &self,
        key: &str,
        last_index: u64,
        wait_secs: u64,
    ) -> Result<(String, u64), ConsulError> {
        let url = self.build_url(&format!("/v1/kv/{}", key));
        let req = self.http.get(&url).query(&[
            ("index", last_index.to_string()),
            ("wait", format!("{}s", wait_secs)),
        ]);
        let req = self.add_auth(req);
        let req = self.add_dc(req);
        let resp = req.send().await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(ConsulError::Api { status, body });
        }

        let new_index = resp
            .headers()
            .get("X-Consul-Index")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(last_index);

        let entries: Vec<ConsulKvEntry> = resp.json().await?;
        if entries.is_empty() {
            return Err(ConsulError::NotFound(key.to_string()));
        }
        let encoded = entries[0]
            .value
            .as_ref()
            .ok_or_else(|| ConsulError::InvalidResponse("missing Value field".into()))?;
        let decoded = base64_decode(encoded)?;
        Ok((decoded, new_index))
    }

    /// 注册服务到 Consul Catalog
    pub async fn register_service(
        &self,
        service: ConsulServiceRegistration,
    ) -> Result<(), ConsulError> {
        let url = self.build_url("/v1/catalog/register");
        let body = serde_json::json!({
            "Datacenter": self.config.datacenter,
            "Node": "sz-orm-auto",
            "Address": service.address.as_deref().unwrap_or("127.0.0.1"),
            "Service": {
                "ID": service.id,
                "Service": service.name,
                "Address": service.address,
                "Port": service.port,
                "Tags": service.tags,
                "Meta": service.meta,
            }
        });
        let req = self.http.put(&url).json(&body);
        let req = self.add_auth(req);
        let resp = req.send().await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(ConsulError::Api { status, body });
        }
        Ok(())
    }

    /// 注销服务
    pub async fn deregister_service(
        &self,
        service_id: &str,
        node: &str,
    ) -> Result<(), ConsulError> {
        let url = self.build_url("/v1/catalog/deregister");
        let body = serde_json::json!({
            "Datacenter": self.config.datacenter,
            "Node": node,
            "ServiceID": service_id,
        });
        let req = self.http.put(&url).json(&body);
        let req = self.add_auth(req);
        let resp = req.send().await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(ConsulError::Api { status, body });
        }
        Ok(())
    }

    /// 发现服务实例（GET /v1/health/service/{name}）
    ///
    /// 返回指定服务名的所有健康实例。
    pub async fn discover_service(
        &self,
        service_name: &str,
    ) -> Result<Vec<ConsulServiceInstance>, ConsulError> {
        let url = self.build_url(&format!("/v1/health/service/{}", service_name));
        let req = self.http.get(&url);
        let req = self.add_auth(req);
        let req = self.add_dc(req);
        let resp = req.send().await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(ConsulError::Api { status, body });
        }

        let instances: Vec<ConsulServiceInstance> = resp.json().await?;
        Ok(instances)
    }
}

/// Consul 服务实例（健康检查结果）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsulServiceInstance {
    #[serde(rename = "Node")]
    pub node: String,
    #[serde(rename = "Address")]
    pub address: String,
    #[serde(rename = "ServiceID")]
    pub service_id: String,
    #[serde(rename = "ServiceName")]
    pub service_name: String,
    #[serde(rename = "ServiceAddress")]
    pub service_address: String,
    #[serde(rename = "ServicePort")]
    pub service_port: u16,
    #[serde(rename = "ServiceTags", default)]
    pub service_tags: Vec<String>,
}

/// 真实 Consul 配置中心，实现 [`crate::ConfigCenter`] trait。
///
/// 内部持有 [`ConsulClient`] 和独立 tokio runtime，通过阻塞调用异步 HTTP API 实现 trait 同步方法。
///
/// # 设计说明
///
/// `ConfigCenter` trait 的方法是同步的，而 `ConsulClient` 的方法是异步的。
/// 此结构体内部创建独立的 current_thread tokio runtime，用 `block_on` 调用异步方法。
/// 因此，此实现**不能在 tokio runtime 上下文内使用**（会 panic）。
///
/// # 示例
///
/// ```ignore
/// use sz_orm_config::consul_client::{RealConsulConfigCenter, ConsulConfig};
/// use sz_orm_config::ConfigCenter;
///
/// let mut cc = RealConsulConfigCenter::new(ConsulConfig::new("http://localhost:8500")).unwrap();
/// cc.set("key", "value");
/// assert_eq!(cc.get("key"), Some("value".to_string()));
/// ```
pub struct RealConsulConfigCenter {
    /// Consul HTTP 客户端
    client: ConsulClient,
    /// 独立 tokio runtime（用于阻塞调用异步 HTTP）
    runtime: tokio::runtime::Runtime,
    /// 本地缓存（用于 list/exists 等不直接映射到单个 API 的方法）
    cache: std::collections::HashMap<String, String>,
    /// 订阅者列表
    subscribers: std::collections::HashMap<String, Vec<crate::ConfigChangeCallback>>,
    /// 事件记录
    events: std::sync::Mutex<Vec<crate::ConfigChangeEvent>>,
}

impl RealConsulConfigCenter {
    /// 创建真实 Consul 配置中心。
    pub fn new(config: ConsulConfig) -> Result<Self, ConsulError> {
        let client = ConsulClient::new(config)?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| ConsulError::InvalidResponse(format!("build runtime: {}", e)))?;
        Ok(Self {
            client,
            runtime,
            cache: std::collections::HashMap::new(),
            subscribers: std::collections::HashMap::new(),
            events: std::sync::Mutex::new(Vec::new()),
        })
    }

    fn notify(&self, key: &str, value: &str, deleted: bool) {
        if let Some(callbacks) = self.subscribers.get(key) {
            for cb in callbacks {
                cb(key, value);
            }
        }
        if let Ok(mut events) = self.events.lock() {
            events.push(crate::ConfigChangeEvent {
                key: key.to_string(),
                value: value.to_string(),
                deleted,
            });
        }
    }

    /// 返回所有变更事件记录
    pub fn events(&self) -> Vec<crate::ConfigChangeEvent> {
        self.events.lock().map(|e| e.clone()).unwrap_or_default()
    }
}

impl crate::ConfigCenter for RealConsulConfigCenter {
    fn get(&self, key: &str) -> Option<String> {
        self.runtime.block_on(self.client.get_config(key)).ok()
    }

    fn set(&mut self, key: &str, value: &str) {
        if let Err(e) = self.runtime.block_on(self.client.set_config(key, value)) {
            eprintln!("RealConsulConfigCenter::set error: {}", e);
            return;
        }
        self.cache.insert(key.to_string(), value.to_string());
        self.notify(key, value, false);
    }

    fn delete(&mut self, key: &str) -> bool {
        match self.runtime.block_on(self.client.delete_config(key)) {
            Ok(()) => {
                self.cache.remove(key);
                self.notify(key, "", true);
                true
            }
            Err(_) => false,
        }
    }

    fn exists(&self, key: &str) -> bool {
        self.runtime.block_on(self.client.get_config(key)).is_ok()
    }

    fn list(&self) -> Vec<String> {
        // 尝试列出所有键（使用空前缀）
        match self.runtime.block_on(self.client.list_keys("")) {
            Ok(keys) => {
                let mut sorted = keys;
                sorted.sort();
                sorted
            }
            Err(_) => {
                let mut keys: Vec<String> = self.cache.keys().cloned().collect();
                keys.sort();
                keys
            }
        }
    }

    fn watch(&self, _key: &str) -> bool {
        // Consul 长轮询通过 watch 方法实现，此处返回 true 表示支持
        true
    }

    fn subscribe(&mut self, key: &str, callback: crate::ConfigChangeCallback) {
        self.subscribers
            .entry(key.to_string())
            .or_default()
            .push(callback);
    }
}

fn base64_decode(s: &str) -> Result<String, ConsulError> {
    use base64::Engine;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(s)
        .map_err(|e| ConsulError::InvalidResponse(format!("base64 decode error: {}", e)))?;
    String::from_utf8(decoded)
        .map_err(|e| ConsulError::InvalidResponse(format!("UTF-8 decode error: {}", e)))
}
