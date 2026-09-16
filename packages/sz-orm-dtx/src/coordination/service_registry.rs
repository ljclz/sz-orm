//! 服务注册/发现 + 心跳与租约
//!
//! 服务实例注册、发现、心跳保活和租约管理。

use std::collections::HashMap;

use super::backend::{CoordinationError, SharedBackend};

/// 服务实例
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ServiceInstance {
    /// 服务名
    pub service_name: String,
    /// 实例 ID
    pub instance_id: String,
    /// 主机地址
    pub host: String,
    /// 端口
    pub port: u16,
    /// 元数据
    pub metadata: HashMap<String, String>,
    /// 注册时间戳
    pub registered_at: u64,
}

impl ServiceInstance {
    /// 创建服务实例
    pub fn new(service_name: &str, instance_id: &str, host: &str, port: u16) -> Self {
        Self {
            service_name: service_name.to_string(),
            instance_id: instance_id.to_string(),
            host: host.to_string(),
            port,
            metadata: HashMap::new(),
            registered_at: current_time_ms(),
        }
    }

    /// 添加元数据
    pub fn with_metadata(mut self, key: &str, value: &str) -> Self {
        self.metadata.insert(key.to_string(), value.to_string());
        self
    }

    /// 实例键
    pub fn key(&self) -> String {
        format!("service:{}:{}", self.service_name, self.instance_id)
    }

    /// 服务列表键
    pub fn service_key(&self) -> String {
        format!("services:{}", self.service_name)
    }
}

fn current_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 服务注册中心
pub struct ServiceRegistry {
    backend: SharedBackend,
    lease_ttl_secs: u64,
}

impl ServiceRegistry {
    /// 创建注册中心
    pub fn new(backend: SharedBackend, lease_ttl_secs: u64) -> Self {
        Self {
            backend,
            lease_ttl_secs,
        }
    }

    /// 注册服务实例
    pub async fn register(&self, instance: &ServiceInstance) -> Result<(), CoordinationError> {
        let key = instance.key();
        let value = serde_json::to_string(instance)
            .map_err(|e| CoordinationError::Serialize(e.to_string()))?;
        self.backend.set(&key, &value, self.lease_ttl_secs).await?;
        let list_key = instance.service_key();
        let list_value = serde_json::to_string(&vec![instance.instance_id.clone()])
            .map_err(|e| CoordinationError::Serialize(e.to_string()))?;
        let existing = self.backend.get(&list_key).await?;
        match existing {
            Some(val) => {
                let mut ids: Vec<String> = serde_json::from_str(&val).unwrap_or_default();
                if !ids.contains(&instance.instance_id) {
                    ids.push(instance.instance_id.clone());
                }
                let updated = serde_json::to_string(&ids)
                    .map_err(|e| CoordinationError::Serialize(e.to_string()))?;
                self.backend.set(&list_key, &updated, 0).await?;
            }
            None => {
                self.backend.set(&list_key, &list_value, 0).await?;
            }
        }
        Ok(())
    }

    /// 注销服务实例
    pub async fn deregister(&self, instance: &ServiceInstance) -> Result<bool, CoordinationError> {
        let key = instance.key();
        let result = self.backend.del(&key).await?;
        let list_key = instance.service_key();
        if let Some(val) = self.backend.get(&list_key).await? {
            let mut ids: Vec<String> = serde_json::from_str(&val).unwrap_or_default();
            ids.retain(|id| id != &instance.instance_id);
            let updated = serde_json::to_string(&ids)
                .map_err(|e| CoordinationError::Serialize(e.to_string()))?;
            self.backend.set(&list_key, &updated, 0).await?;
        }
        Ok(result)
    }

    /// 发送心跳
    pub async fn heartbeat(&self, instance: &ServiceInstance) -> Result<bool, CoordinationError> {
        let key = instance.key();
        let value = serde_json::to_string(instance)
            .map_err(|e| CoordinationError::Serialize(e.to_string()))?;
        let existing = self.backend.get(&key).await?;
        match existing {
            Some(_) => {
                self.backend.set(&key, &value, self.lease_ttl_secs).await?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// 发现服务实例
    pub async fn discover(
        &self,
        service_name: &str,
    ) -> Result<Vec<ServiceInstance>, CoordinationError> {
        let list_key = format!("services:{}", service_name);
        let list_val = self.backend.get(&list_key).await?;
        let ids: Vec<String> = match list_val {
            Some(val) => serde_json::from_str(&val).unwrap_or_default(),
            None => Vec::new(),
        };
        let mut instances = Vec::new();
        for id in &ids {
            let key = format!("service:{}:{}", service_name, id);
            if let Some(val) = self.backend.get(&key).await? {
                if let Ok(instance) = serde_json::from_str::<ServiceInstance>(&val) {
                    instances.push(instance);
                }
            }
        }
        Ok(instances)
    }

    /// 检查租约是否有效
    pub async fn is_lease_valid(
        &self,
        instance: &ServiceInstance,
    ) -> Result<bool, CoordinationError> {
        let key = instance.key();
        let existing = self.backend.get(&key).await?;
        Ok(existing.is_some())
    }

    /// 租约 TTL
    pub fn lease_ttl_secs(&self) -> u64 {
        self.lease_ttl_secs
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use super::super::backend::InMemoryBackend;
    use super::*;

    fn make_registry() -> ServiceRegistry {
        ServiceRegistry::new(Arc::new(InMemoryBackend::new()), 30)
    }

    #[tokio::test]
    async fn test_register_and_discover() {
        let registry = make_registry();
        let instance = ServiceInstance::new("user-service", "inst-1", "10.0.0.1", 8080);
        registry.register(&instance).await.unwrap();
        let found = registry.discover("user-service").await.unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].instance_id, "inst-1");
    }

    #[tokio::test]
    async fn test_deregister() {
        let registry = make_registry();
        let instance = ServiceInstance::new("svc", "inst-1", "10.0.0.1", 8080);
        registry.register(&instance).await.unwrap();
        assert!(registry.deregister(&instance).await.unwrap());
        let found = registry.discover("svc").await.unwrap();
        assert!(found.is_empty());
    }

    #[tokio::test]
    async fn test_heartbeat() {
        let registry = make_registry();
        let instance = ServiceInstance::new("svc", "inst-1", "10.0.0.1", 8080);
        registry.register(&instance).await.unwrap();
        assert!(registry.heartbeat(&instance).await.unwrap());
    }

    #[tokio::test]
    async fn test_heartbeat_unregistered() {
        let registry = make_registry();
        let instance = ServiceInstance::new("svc", "inst-1", "10.0.0.1", 8080);
        assert!(!registry.heartbeat(&instance).await.unwrap());
    }

    #[tokio::test]
    async fn test_multiple_instances() {
        let registry = make_registry();
        let i1 = ServiceInstance::new("svc", "inst-1", "10.0.0.1", 8080);
        let i2 = ServiceInstance::new("svc", "inst-2", "10.0.0.2", 8080);
        registry.register(&i1).await.unwrap();
        registry.register(&i2).await.unwrap();
        let found = registry.discover("svc").await.unwrap();
        assert_eq!(found.len(), 2);
    }

    #[tokio::test]
    async fn test_lease_validity() {
        let registry = make_registry();
        let instance = ServiceInstance::new("svc", "inst-1", "10.0.0.1", 8080);
        registry.register(&instance).await.unwrap();
        assert!(registry.is_lease_valid(&instance).await.unwrap());
        registry.deregister(&instance).await.unwrap();
        assert!(!registry.is_lease_valid(&instance).await.unwrap());
    }

    #[tokio::test]
    async fn test_lease_expiration() {
        let registry = ServiceRegistry::new(Arc::new(InMemoryBackend::new()), 1);
        let instance = ServiceInstance::new("svc", "inst-1", "10.0.0.1", 8080);
        registry.register(&instance).await.unwrap();
        assert!(registry.is_lease_valid(&instance).await.unwrap());
        tokio::time::sleep(Duration::from_secs(2)).await;
        assert!(!registry.is_lease_valid(&instance).await.unwrap());
    }

    #[tokio::test]
    async fn test_metadata() {
        let registry = make_registry();
        let instance = ServiceInstance::new("svc", "inst-1", "10.0.0.1", 8080)
            .with_metadata("version", "1.0.0")
            .with_metadata("region", "us-east");
        registry.register(&instance).await.unwrap();
        let found = registry.discover("svc").await.unwrap();
        assert_eq!(found[0].metadata.get("version"), Some(&"1.0.0".to_string()));
        assert_eq!(
            found[0].metadata.get("region"),
            Some(&"us-east".to_string())
        );
    }

    #[tokio::test]
    async fn test_lease_ttl() {
        let registry = ServiceRegistry::new(Arc::new(InMemoryBackend::new()), 60);
        assert_eq!(registry.lease_ttl_secs(), 60);
    }

    #[tokio::test]
    async fn test_discover_empty_service() {
        let registry = make_registry();
        let found = registry.discover("nonexistent").await.unwrap();
        assert!(found.is_empty());
    }

    #[tokio::test]
    async fn test_deregister_unknown_instance() {
        let registry = make_registry();
        let instance = ServiceInstance::new("svc", "unknown", "10.0.0.1", 8080);
        assert!(!registry.deregister(&instance).await.unwrap());
    }
}
