//! 真实 Nacos HTTP API 客户端
//!
//! 基于 Nacos Open API：https://nacos.io/zh-cn/docs/open-api.html
//!
//! 支持：
//! - `get_config` / `set_config` / `delete_config` — 配置读写
//! - `watch` — 长轮询监听配置变更
//! - `register_service` — 服务注册
//! - Username + Password 认证（通过 `accessToken`）

#![cfg(feature = "real-nacos")]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Nacos 客户端错误
#[derive(Debug, thiserror::Error)]
pub enum NacosError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Nacos API error: status={status}, body={body}")]
    Api { status: u16, body: String },
    #[error("Config not found: {0}")]
    NotFound(String),
    #[error("Auth error: {0}")]
    Auth(String),
    #[error("Invalid response: {0}")]
    InvalidResponse(String),
}

/// Nacos 客户端配置
#[derive(Debug, Clone)]
pub struct NacosConfig {
    pub endpoint: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub namespace: Option<String>,
    pub timeout_secs: u64,
}

impl Default for NacosConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://127.0.0.1:8848".to_string(),
            username: None,
            password: None,
            namespace: None,
            timeout_secs: 10,
        }
    }
}

impl NacosConfig {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            ..Default::default()
        }
    }

    pub fn with_auth(mut self, username: impl Into<String>, password: impl Into<String>) -> Self {
        self.username = Some(username.into());
        self.password = Some(password.into());
        self
    }

    pub fn with_namespace(mut self, ns: impl Into<String>) -> Self {
        self.namespace = Some(ns.into());
        self
    }
}

/// Nacos 登录响应
#[derive(Debug, Clone, Serialize, Deserialize)]
struct NacosLoginResponse {
    #[serde(rename = "accessToken")]
    access_token: String,
}

/// Nacos 服务注册请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NacosServiceRegistration {
    pub name: String,
    pub ip: String,
    pub port: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weight: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cluster_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ephemeral: Option<bool>,
}

/// 真实 Nacos HTTP API 客户端
pub struct NacosClient {
    config: NacosConfig,
    http: reqwest::Client,
    access_token: Option<String>,
}

impl NacosClient {
    pub fn new(config: NacosConfig) -> Result<Self, NacosError> {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()?;
        Ok(Self {
            config,
            http,
            access_token: None,
        })
    }

    fn build_url(&self, path: &str) -> String {
        format!("{}{}", self.config.endpoint, path)
    }

    fn add_auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let Some(token) = &self.access_token {
            req.query(&[("accessToken", token)])
        } else {
            req
        }
    }

    fn add_namespace(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let Some(ns) = &self.config.namespace {
            req.query(&[("tenant", ns)])
        } else {
            req
        }
    }

    /// 登录获取 accessToken
    pub async fn login(&mut self) -> Result<(), NacosError> {
        let (username, password) = match (&self.config.username, &self.config.password) {
            (Some(u), Some(p)) => (u.clone(), p.clone()),
            _ => return Err(NacosError::Auth("username and password required".into())),
        };

        let url = self.build_url("/nacos/v1/auth/login");
        let resp = self
            .http
            .post(&url)
            .form(&[
                ("username", username.as_str()),
                ("password", password.as_str()),
            ])
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(NacosError::Api { status, body });
        }

        let login_resp: NacosLoginResponse = resp.json().await?;
        self.access_token = Some(login_resp.access_token);
        Ok(())
    }

    /// 读取配置
    pub async fn get_config(&self, data_id: &str, group: &str) -> Result<String, NacosError> {
        let url = self.build_url("/nacos/v1/cs/configs");
        let req = self
            .http
            .get(&url)
            .query(&[("dataId", data_id), ("group", group)]);
        let req = self.add_auth(req);
        let req = self.add_namespace(req);
        let resp = req.send().await?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND
            || resp.status() == reqwest::StatusCode::CONFLICT
        {
            return Err(NacosError::NotFound(format!("{}/{}", group, data_id)));
        }
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(NacosError::Api { status, body });
        }

        let config = resp.text().await?;
        if config.is_empty() {
            return Err(NacosError::NotFound(format!("{}/{}", group, data_id)));
        }
        Ok(config)
    }

    /// 写入配置
    pub async fn set_config(
        &self,
        data_id: &str,
        group: &str,
        content: &str,
    ) -> Result<(), NacosError> {
        let url = self.build_url("/nacos/v1/cs/configs");
        let mut form: Vec<(&str, &str)> =
            vec![("dataId", data_id), ("group", group), ("content", content)];
        let ns_val: String;
        if let Some(ns) = &self.config.namespace {
            ns_val = ns.clone();
            form.push(("tenant", ns_val.as_str()));
        }

        let req = self.http.post(&url).form(&form);
        let req = self.add_auth(req);
        let resp = req.send().await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(NacosError::Api { status, body });
        }
        Ok(())
    }

    /// 删除配置
    pub async fn delete_config(&self, data_id: &str, group: &str) -> Result<(), NacosError> {
        let url = self.build_url("/nacos/v1/cs/configs");
        let req = self
            .http
            .delete(&url)
            .query(&[("dataId", data_id), ("group", group)]);
        let req = self.add_auth(req);
        let req = self.add_namespace(req);
        let resp = req.send().await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(NacosError::Api { status, body });
        }
        Ok(())
    }

    /// 长轮询监听配置变更
    ///
    /// 返回变更后的配置内容。`timeout_millis` 为长轮询超时时间。
    pub async fn watch(
        &self,
        data_id: &str,
        group: &str,
        timeout_millis: u64,
    ) -> Result<String, NacosError> {
        let url = self.build_url("/nacos/v1/cs/configs/listener");
        let listening_str = format!(
            "{}^2{}^1{}",
            data_id,
            group,
            self.config.namespace.as_deref().unwrap_or("")
        );
        let req = self.http.post(&url).query(&[
            ("Listening-Configs", listening_str.as_str()),
            ("timeout", &timeout_millis.to_string()),
        ]);
        let req = self.add_auth(req);
        let resp = req.send().await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(NacosError::Api { status, body });
        }

        let content = resp.text().await?;
        if content.is_empty() {
            return Err(NacosError::NotFound(format!("{}/{}", group, data_id)));
        }
        Ok(content)
    }

    /// 注册服务实例
    pub async fn register_service(
        &self,
        service: NacosServiceRegistration,
    ) -> Result<(), NacosError> {
        let url = self.build_url("/nacos/v1/ns/instance");
        let mut form: Vec<(&str, String)> = vec![
            ("serviceName", service.name.clone()),
            ("ip", service.ip.clone()),
            ("port", service.port.to_string()),
        ];
        if let Some(w) = service.weight {
            form.push(("weight", w.to_string()));
        }
        if let Some(cluster) = &service.cluster_name {
            form.push(("clusterName", cluster.clone()));
        }
        if let Some(group) = &service.group_name {
            form.push(("groupName", group.clone()));
        }
        if let Some(enabled) = service.enabled {
            form.push(("enabled", enabled.to_string()));
        }
        if let Some(ephemeral) = service.ephemeral {
            form.push(("ephemeral", ephemeral.to_string()));
        }
        if let Some(metadata) = &service.metadata {
            let meta_str = metadata
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>()
                .join(",");
            form.push(("metadata", meta_str));
        }

        let form_refs: Vec<(&str, &str)> = form.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let req = self.http.post(&url).form(&form_refs);
        let req = self.add_auth(req);
        let resp = req.send().await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(NacosError::Api { status, body });
        }
        Ok(())
    }

    /// 注销服务实例
    pub async fn deregister_service(
        &self,
        name: &str,
        ip: &str,
        port: u16,
    ) -> Result<(), NacosError> {
        let url = self.build_url("/nacos/v1/ns/instance");
        let req = self.http.delete(&url).query(&[
            ("serviceName", name),
            ("ip", ip),
            ("port", &port.to_string()),
        ]);
        let req = self.add_auth(req);
        let resp = req.send().await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(NacosError::Api { status, body });
        }
        Ok(())
    }

    /// 发现服务实例（GET /nacos/v1/ns/instance/list）
    ///
    /// 返回指定服务名的所有实例。
    pub async fn discover_service(
        &self,
        service_name: &str,
        group: &str,
    ) -> Result<Vec<NacosServiceInstance>, NacosError> {
        let url = self.build_url("/nacos/v1/ns/instance/list");
        let req = self
            .http
            .get(&url)
            .query(&[("serviceName", service_name), ("groupName", group)]);
        let req = self.add_auth(req);
        let resp = req.send().await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(NacosError::Api { status, body });
        }

        let resp_json: serde_json::Value = resp.json().await?;
        let hosts = resp_json["hosts"].as_array().cloned().unwrap_or_default();

        let instances: Vec<NacosServiceInstance> = hosts
            .iter()
            .map(|h| NacosServiceInstance {
                instance_id: h["instanceId"].as_str().unwrap_or("").to_string(),
                service_name: h["serviceName"].as_str().unwrap_or("").to_string(),
                ip: h["ip"].as_str().unwrap_or("").to_string(),
                port: h["port"].as_u64().unwrap_or(0) as u16,
                healthy: h["healthy"].as_bool().unwrap_or(true),
                enabled: h["enabled"].as_bool().unwrap_or(true),
                weight: h["weight"].as_f64().unwrap_or(1.0),
            })
            .collect();

        Ok(instances)
    }
}

/// Nacos 服务实例
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NacosServiceInstance {
    pub instance_id: String,
    pub service_name: String,
    pub ip: String,
    pub port: u16,
    pub healthy: bool,
    pub enabled: bool,
    pub weight: f64,
}

/// 真实 Nacos 配置中心，实现 [`crate::ConfigCenter`] trait。
///
/// 内部持有 [`NacosClient`] 和独立 tokio runtime，通过阻塞调用异步 HTTP API 实现 trait 同步方法。
///
/// # 设计说明
///
/// `ConfigCenter` trait 的方法是同步的，而 `NacosClient` 的方法是异步的。
/// 此结构体内部创建独立的 current_thread tokio runtime，用 `block_on` 调用异步方法。
/// 因此，此实现**不能在 tokio runtime 上下文内使用**（会 panic）。
///
/// Nacos 的配置需要 `dataId` + `group` 二维定位，而 `ConfigCenter` trait 的 key 是一维的。
/// 此实现将 key 映射为 `dataId`，group 在构造时固定。
///
/// # 示例
///
/// ```ignore
/// use sz_orm_config::nacos_client::{RealNacosConfigCenter, NacosConfig};
/// use sz_orm_config::ConfigCenter;
///
/// let config = NacosConfig::new("http://localhost:8848").with_auth("nacos", "nacos");
/// let mut nc = RealNacosConfigCenter::new(config, "DEFAULT_GROUP".to_string()).unwrap();
/// nc.set("dataId", "content");
/// assert_eq!(nc.get("dataId"), Some("content".to_string()));
/// ```
pub struct RealNacosConfigCenter {
    /// Nacos HTTP 客户端
    client: NacosClient,
    /// 独立 tokio runtime
    runtime: tokio::runtime::Runtime,
    /// 配置组名（Nacos 用 dataId + group 定位配置）
    group: String,
    /// 本地缓存
    cache: std::collections::HashMap<String, String>,
    /// 订阅者列表
    subscribers: std::collections::HashMap<String, Vec<crate::ConfigChangeCallback>>,
    /// 事件记录
    events: std::sync::Mutex<Vec<crate::ConfigChangeEvent>>,
}

impl RealNacosConfigCenter {
    /// 创建真实 Nacos 配置中心。
    ///
    /// `group` 为 Nacos 配置组名（如 `DEFAULT_GROUP`）。
    pub fn new(config: NacosConfig, group: String) -> Result<Self, NacosError> {
        let client = NacosClient::new(config)?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| NacosError::InvalidResponse(format!("build runtime: {}", e)))?;
        Ok(Self {
            client,
            runtime,
            group,
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

impl crate::ConfigCenter for RealNacosConfigCenter {
    fn get(&self, key: &str) -> Option<String> {
        self.runtime
            .block_on(self.client.get_config(key, &self.group))
            .ok()
    }

    fn set(&mut self, key: &str, value: &str) {
        if let Err(e) = self
            .runtime
            .block_on(self.client.set_config(key, &self.group, value))
        {
            eprintln!("RealNacosConfigCenter::set error: {}", e);
            return;
        }
        self.cache.insert(key.to_string(), value.to_string());
        self.notify(key, value, false);
    }

    fn delete(&mut self, key: &str) -> bool {
        match self
            .runtime
            .block_on(self.client.delete_config(key, &self.group))
        {
            Ok(()) => {
                self.cache.remove(key);
                self.notify(key, "", true);
                true
            }
            Err(_) => false,
        }
    }

    fn exists(&self, key: &str) -> bool {
        self.runtime
            .block_on(self.client.get_config(key, &self.group))
            .is_ok()
    }

    fn list(&self) -> Vec<String> {
        let mut keys: Vec<String> = self.cache.keys().cloned().collect();
        keys.sort();
        keys
    }

    fn watch(&self, _key: &str) -> bool {
        true
    }

    fn subscribe(&mut self, key: &str, callback: crate::ConfigChangeCallback) {
        self.subscribers
            .entry(key.to_string())
            .or_default()
            .push(callback);
    }
}
