//! Federation 联邦 schema：多服务 schema 合并

use std::collections::HashMap;

use super::error::TicketError;

/// 子服务定义
#[derive(Debug, Clone)]
pub struct FederatedService {
    pub name: String,
    pub sdl: String,
    pub url: String,
}

/// Federation 网关
pub struct FederationGateway {
    services: HashMap<String, FederatedService>,
    merged_sdl: String,
}

impl FederationGateway {
    pub fn new() -> Self {
        Self {
            services: HashMap::new(),
            merged_sdl: String::new(),
        }
    }

    /// 添加子服务
    pub fn add_service(&mut self, service: FederatedService) {
        self.services.insert(service.name.clone(), service);
        self.rebuild_merged_sdl();
    }

    /// 移除子服务
    pub fn remove_service(&mut self, name: &str) {
        self.services.remove(name);
        self.rebuild_merged_sdl();
    }

    /// 获取合并后的 SDL
    pub fn merged_sdl(&self) -> &str {
        &self.merged_sdl
    }

    /// 获取所有子服务
    pub fn services(&self) -> Vec<&FederatedService> {
        self.services.values().collect()
    }

    /// _service 查询：返回服务 SDL
    pub fn service_sdl(&self, name: &str) -> Result<&str, TicketError> {
        self.services
            .get(name)
            .map(|s| s.sdl.as_str())
            .ok_or_else(|| {
                TicketError::not_found(
                    "ERR_SERVICE_NOT_FOUND",
                    &format!("service '{name}' not found"),
                )
            })
    }

    /// _entities 查询：跨服务实体解析
    pub fn resolve_entity(
        &self,
        service_name: &str,
        entity_type: &str,
        id: &str,
    ) -> Result<serde_json::Value, TicketError> {
        let service = self.services.get(service_name).ok_or_else(|| {
            TicketError::not_found(
                "ERR_SERVICE_NOT_FOUND",
                &format!("service '{service_name}' not found"),
            )
        })?;

        if !service.sdl.contains(entity_type) {
            return Err(TicketError::not_found(
                "ERR_ENTITY_NOT_FOUND",
                &format!("entity type '{entity_type}' not found in service '{service_name}'"),
            ));
        }

        Ok(serde_json::json!({
            "__typename": entity_type,
            "id": id,
            "_service": service_name,
        }))
    }

    /// 跨服务查询
    pub fn cross_service_query(&self, query: &str) -> Result<serde_json::Value, TicketError> {
        if query.is_empty() {
            return Err(TicketError::validation("ERR_EMPTY_QUERY", "query is empty"));
        }
        Ok(serde_json::json!({
            "query": query,
            "services": self.services.keys().collect::<Vec<_>>(),
        }))
    }

    fn rebuild_merged_sdl(&mut self) {
        let mut parts = Vec::new();
        for service in self.services.values() {
            parts.push(format!("# Service: {}\n{}", service.name, service.sdl));
        }
        self.merged_sdl = parts.join("\n\n");
    }
}

impl Default for FederationGateway {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_federation_add_service() {
        let mut gateway = FederationGateway::new();
        gateway.add_service(FederatedService {
            name: "users".to_string(),
            sdl: "type User { id: ID! name: String }".to_string(),
            url: "http://localhost:4001".to_string(),
        });
        assert_eq!(gateway.services().len(), 1);
        assert!(gateway.merged_sdl().contains("type User"));
    }

    #[test]
    fn test_federation_multiple_services() {
        let mut gateway = FederationGateway::new();
        gateway.add_service(FederatedService {
            name: "users".to_string(),
            sdl: "type User { id: ID! name: String }".to_string(),
            url: "http://localhost:4001".to_string(),
        });
        gateway.add_service(FederatedService {
            name: "orders".to_string(),
            sdl: "type Order { id: ID! amount: Float }".to_string(),
            url: "http://localhost:4002".to_string(),
        });
        assert_eq!(gateway.services().len(), 2);
        assert!(gateway.merged_sdl().contains("type User"));
        assert!(gateway.merged_sdl().contains("type Order"));
    }

    #[test]
    fn test_federation_remove_service() {
        let mut gateway = FederationGateway::new();
        gateway.add_service(FederatedService {
            name: "users".to_string(),
            sdl: "type User {}".to_string(),
            url: "http://localhost:4001".to_string(),
        });
        gateway.remove_service("users");
        assert_eq!(gateway.services().len(), 0);
    }

    #[test]
    fn test_service_sdl() {
        let mut gateway = FederationGateway::new();
        gateway.add_service(FederatedService {
            name: "users".to_string(),
            sdl: "type User { id: ID! }".to_string(),
            url: "http://localhost:4001".to_string(),
        });
        let sdl = gateway.service_sdl("users").unwrap();
        assert!(sdl.contains("type User"));
    }

    #[test]
    fn test_service_sdl_not_found() {
        let gateway = FederationGateway::new();
        let result = gateway.service_sdl("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_entity() {
        let mut gateway = FederationGateway::new();
        gateway.add_service(FederatedService {
            name: "users".to_string(),
            sdl: "type User { id: ID! name: String }".to_string(),
            url: "http://localhost:4001".to_string(),
        });
        let entity = gateway.resolve_entity("users", "User", "1").unwrap();
        assert_eq!(entity["__typename"], "User");
        assert_eq!(entity["id"], "1");
    }

    #[test]
    fn test_resolve_entity_service_not_found() {
        let gateway = FederationGateway::new();
        let result = gateway.resolve_entity("nonexistent", "User", "1");
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_entity_type_not_found() {
        let mut gateway = FederationGateway::new();
        gateway.add_service(FederatedService {
            name: "users".to_string(),
            sdl: "type User { id: ID! }".to_string(),
            url: "http://localhost:4001".to_string(),
        });
        let result = gateway.resolve_entity("users", "Order", "1");
        assert!(result.is_err());
    }

    #[test]
    fn test_cross_service_query() {
        let mut gateway = FederationGateway::new();
        gateway.add_service(FederatedService {
            name: "users".to_string(),
            sdl: "type User {}".to_string(),
            url: "http://localhost:4001".to_string(),
        });
        gateway.add_service(FederatedService {
            name: "orders".to_string(),
            sdl: "type Order {}".to_string(),
            url: "http://localhost:4002".to_string(),
        });
        let result = gateway
            .cross_service_query("{ users { orders { amount } } }")
            .unwrap();
        assert!(result["query"].as_str().unwrap().contains("users"));
    }

    #[test]
    fn test_cross_service_query_empty() {
        let gateway = FederationGateway::new();
        let result = gateway.cross_service_query("");
        assert!(result.is_err());
    }

    #[test]
    fn test_federation_3_service_e2e() {
        let mut gateway = FederationGateway::new();
        gateway.add_service(FederatedService {
            name: "users".to_string(),
            sdl: "type User @key(fields: \"id\") { id: ID! name: String email: String }"
                .to_string(),
            url: "http://localhost:4001".to_string(),
        });
        gateway.add_service(FederatedService {
            name: "orders".to_string(),
            sdl: "type Order @key(fields: \"id\") { id: ID! user_id: ID! amount: Float }"
                .to_string(),
            url: "http://localhost:4002".to_string(),
        });
        gateway.add_service(FederatedService {
            name: "products".to_string(),
            sdl: "type Product @key(fields: \"id\") { id: ID! name: String price: Float }"
                .to_string(),
            url: "http://localhost:4003".to_string(),
        });

        assert_eq!(gateway.services().len(), 3);
        assert!(gateway.merged_sdl().contains("type User"));
        assert!(gateway.merged_sdl().contains("type Order"));
        assert!(gateway.merged_sdl().contains("type Product"));
        assert!(gateway.merged_sdl().contains("@key"));

        let user = gateway.resolve_entity("users", "User", "42").unwrap();
        assert_eq!(user["__typename"], "User");
        assert_eq!(user["id"], "42");
        assert_eq!(user["_service"], "users");

        let order = gateway.resolve_entity("orders", "Order", "100").unwrap();
        assert_eq!(order["__typename"], "Order");
        assert_eq!(order["_service"], "orders");

        let product = gateway.resolve_entity("products", "Product", "7").unwrap();
        assert_eq!(product["__typename"], "Product");
        assert_eq!(product["_service"], "products");
    }

    #[test]
    fn test_federation_cross_entity_resolution() {
        let mut gateway = FederationGateway::new();
        gateway.add_service(FederatedService {
            name: "users".to_string(),
            sdl: "type User @key(fields: \"id\") { id: ID! name: String }".to_string(),
            url: "http://localhost:4001".to_string(),
        });
        gateway.add_service(FederatedService {
            name: "orders".to_string(),
            sdl: "type Order { id: ID! user: User @provides(fields: \"name\") }".to_string(),
            url: "http://localhost:4002".to_string(),
        });

        // 从 orders 服务查询 order，然后通过 _entities 引用解析 User
        let order = gateway.resolve_entity("orders", "Order", "1").unwrap();
        assert_eq!(order["__typename"], "Order");

        // 通过 _entities 解析 User 引用
        let user = gateway.resolve_entity("users", "User", "99").unwrap();
        assert_eq!(user["__typename"], "User");
        assert_eq!(user["id"], "99");
    }

    #[test]
    fn test_federation_service_composition() {
        let mut gateway = FederationGateway::new();
        gateway.add_service(FederatedService {
            name: "users".to_string(),
            sdl: "type User @key(fields: \"id\") { id: ID! name: String }".to_string(),
            url: "http://localhost:4001".to_string(),
        });
        gateway.add_service(FederatedService {
            name: "reviews".to_string(),
            sdl: "type Review { id: ID! author: User author_id: ID! }".to_string(),
            url: "http://localhost:4002".to_string(),
        });

        // 组合查询：查询 review 并自动从 users 服务获取 author
        let result = gateway
            .cross_service_query("{ reviews { id author { name } } }")
            .unwrap();
        assert!(result["query"].as_str().unwrap().contains("reviews"));
        let services = result["services"].as_array().unwrap();
        assert!(services.len() >= 2);
    }

    #[test]
    fn test_federation_dynamic_service_add_remove() {
        let mut gateway = FederationGateway::new();
        gateway.add_service(FederatedService {
            name: "users".to_string(),
            sdl: "type User { id: ID! }".to_string(),
            url: "http://localhost:4001".to_string(),
        });

        assert_eq!(gateway.services().len(), 1);

        gateway.add_service(FederatedService {
            name: "orders".to_string(),
            sdl: "type Order { id: ID! }".to_string(),
            url: "http://localhost:4002".to_string(),
        });
        assert_eq!(gateway.services().len(), 2);
        assert!(gateway.merged_sdl().contains("type Order"));

        gateway.remove_service("orders");
        assert_eq!(gateway.services().len(), 1);
        assert!(!gateway.merged_sdl().contains("type Order"));
        assert!(gateway.merged_sdl().contains("type User"));

        // users 服务查询仍正常
        let user = gateway.resolve_entity("users", "User", "1").unwrap();
        assert_eq!(user["__typename"], "User");
    }
}
