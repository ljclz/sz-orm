use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrpcServiceDef {
    pub name: String,
    pub methods: Vec<GrpcMethod>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrpcMethod {
    pub name: String,
    pub input_type: String,
    pub output_type: String,
    pub client_streaming: bool,
    pub server_streaming: bool,
}

/// Global in-memory service registry keyed by `address`.
/// Maps `address -> service_name -> service implementation`.
type ServiceRef = Arc<dyn UserGrpcService>;

#[derive(Default)]
struct AddressRegistry {
    services: HashMap<String, HashMap<String, ServiceRef>>,
    definitions: HashMap<String, Vec<GrpcServiceDef>>,
}

static REGISTRY: OnceLock<RwLock<AddressRegistry>> = OnceLock::new();

fn global_registry() -> &'static RwLock<AddressRegistry> {
    REGISTRY.get_or_init(|| RwLock::new(AddressRegistry::default()))
}

pub struct GrpcServer {
    address: String,
    port: u16,
    pub services: Vec<GrpcServiceDef>,
    user_service: Option<ServiceRef>,
}

impl GrpcServer {
    pub fn new(address: impl Into<String>, port: u16) -> Self {
        Self {
            address: address.into(),
            port,
            services: Vec::new(),
            user_service: None,
        }
    }

    pub fn register_service(mut self, service: GrpcServiceDef) -> Self {
        self.services.push(service);
        self
    }

    pub fn register_user_service(mut self, service: ServiceRef) -> Self {
        self.user_service = Some(service);
        self
    }

    pub fn start(&self) -> Result<GrpcServerHandle, GrpcError> {
        if self.services.is_empty() && self.user_service.is_none() {
            return Err(GrpcError::NoServices("No services registered".to_string()));
        }
        let addr = format!("{}:{}", self.address, self.port);
        {
            let mut reg = global_registry().write().unwrap();
            reg.definitions.insert(addr.clone(), self.services.clone());
            // Always create the services map entry so clients can distinguish
            // "server up but service missing" from "no server at all".
            let entry = reg.services.entry(addr.clone()).or_default();
            if let Some(user_svc) = &self.user_service {
                entry.insert("UserService".to_string(), user_svc.clone());
            }
        }
        Ok(GrpcServerHandle {
            address: addr,
            registered: self.user_service.is_some(),
        })
    }
}

pub struct GrpcServerHandle {
    address: String,
    registered: bool,
}

impl GrpcServerHandle {
    pub fn address(&self) -> &str {
        &self.address
    }

    /// Removes this address's user service entry from the global registry.
    pub fn stop(&self) -> Result<(), GrpcError> {
        let mut reg = global_registry().write().unwrap();
        if let Some(services) = reg.services.get_mut(&self.address) {
            services.remove("UserService");
        }
        reg.definitions.remove(&self.address);
        Ok(())
    }

    pub fn has_user_service(&self) -> bool {
        self.registered
    }
}

impl Drop for GrpcServerHandle {
    fn drop(&mut self) {
        // Best-effort cleanup so tests do not leak state into each other.
        if let Ok(mut reg) = global_registry().write() {
            reg.services.remove(&self.address);
            reg.definitions.remove(&self.address);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRequest {
    pub id: i64,
    pub username: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserResponse {
    pub id: i64,
    pub username: String,
    pub email: String,
}

pub trait UserGrpcService: Send + Sync {
    fn get_user(&self, request: UserRequest) -> Result<UserResponse, GrpcError>;
    fn list_users(&self) -> Result<Vec<UserResponse>, GrpcError>;
}

/// In-memory user service backed by a HashMap, useful for tests and demos.
pub struct InMemoryUserService {
    users: RwLock<HashMap<i64, UserResponse>>,
}

impl InMemoryUserService {
    pub fn new() -> Self {
        Self {
            users: RwLock::new(HashMap::new()),
        }
    }

    pub fn with_user(self, user: UserResponse) -> Self {
        self.users.write().unwrap().insert(user.id, user);
        self
    }

    pub fn add_user(&self, user: UserResponse) {
        self.users.write().unwrap().insert(user.id, user);
    }

    pub fn remove_user(&self, id: i64) -> Option<UserResponse> {
        self.users.write().unwrap().remove(&id)
    }
}

impl Default for InMemoryUserService {
    fn default() -> Self {
        Self::new()
    }
}

impl UserGrpcService for InMemoryUserService {
    fn get_user(&self, request: UserRequest) -> Result<UserResponse, GrpcError> {
        let users = self.users.read().unwrap();
        users
            .get(&request.id)
            .cloned()
            .ok_or_else(|| GrpcError::MethodNotFound(format!("User {} not found", request.id)))
    }

    fn list_users(&self) -> Result<Vec<UserResponse>, GrpcError> {
        let users = self.users.read().unwrap();
        let mut list: Vec<UserResponse> = users.values().cloned().collect();
        list.sort_by_key(|a| a.id);
        Ok(list)
    }
}

pub struct UserGrpcClient {
    channel: GrpcChannel,
}

impl UserGrpcClient {
    pub fn connect(address: impl Into<String>) -> Result<Self, GrpcError> {
        let addr_str = address.into();
        if addr_str.is_empty() {
            return Err(GrpcError::ConnectionFailed(
                "Address cannot be empty".to_string(),
            ));
        }
        Ok(Self {
            channel: GrpcChannel::new(addr_str),
        })
    }

    pub fn get_user(&self, id: i64) -> Result<UserResponse, GrpcError> {
        let request = UserRequest {
            id,
            username: String::new(),
        };
        self.channel
            .call_unary("UserService", "GetUser", move |svc| {
                svc.get_user(request.clone())
            })
    }

    pub fn list_users(&self) -> Result<Vec<UserResponse>, GrpcError> {
        self.channel
            .call_unary("UserService", "ListUsers", |svc| svc.list_users())
    }

    pub fn channel(&self) -> &GrpcChannel {
        &self.channel
    }
}

#[derive(Clone)]
pub struct GrpcChannel {
    address: String,
    metadata: HashMap<String, String>,
}

impl GrpcChannel {
    pub fn new(address: impl Into<String>) -> Self {
        Self {
            address: address.into(),
            metadata: HashMap::new(),
        }
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    pub fn address(&self) -> &str {
        &self.address
    }

    pub fn metadata(&self) -> &HashMap<String, String> {
        &self.metadata
    }

    /// Looks up the service implementation in the global registry and invokes `f`.
    pub fn call_unary<F, T>(
        &self,
        service_name: &str,
        _method_name: &str,
        f: F,
    ) -> Result<T, GrpcError>
    where
        F: FnOnce(&dyn UserGrpcService) -> Result<T, GrpcError>,
    {
        let reg = global_registry().read().unwrap();
        let services = reg.services.get(&self.address).ok_or_else(|| {
            GrpcError::ConnectionFailed(format!("No server listening at {}", self.address))
        })?;
        let svc = services
            .get(service_name)
            .ok_or_else(|| GrpcError::ServiceNotFound(service_name.to_string()))?;
        f(svc.as_ref())
    }

    /// Returns the registered service definitions for this address (metadata only).
    pub fn list_services(&self) -> Vec<GrpcServiceDef> {
        let reg = global_registry().read().unwrap();
        reg.definitions
            .get(&self.address)
            .cloned()
            .unwrap_or_default()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GrpcError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    #[error("Service not found: {0}")]
    ServiceNotFound(String),
    #[error("Method not found: {0}")]
    MethodNotFound(String),
    #[error("No services: {0}")]
    NoServices(String),
    #[error("Timeout: {0}")]
    Timeout(String),
    #[error("Transport error: {0}")]
    Transport(String),
}

impl serde::Serialize for GrpcError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

#[cfg(feature = "real")]
mod real_grpc;

#[cfg(feature = "real")]
pub use real_grpc::{RealGrpcClient, RealGrpcServer, RealGrpcServerHandle};

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_addr(tag: &str) -> String {
        // Use a unique port per test to avoid collisions in the global registry.
        let port = 50000u16
            + (tag.len() as u16 * 7)
            + (tag.bytes().fold(0u16, |a, b| a.wrapping_add(b as u16)) % 500);
        format!("localhost:{}", port)
    }

    fn build_user(id: i64, username: &str, email: &str) -> UserResponse {
        UserResponse {
            id,
            username: username.to_string(),
            email: email.to_string(),
        }
    }

    #[test]
    fn test_grpc_server_new() {
        let server = GrpcServer::new("localhost", 50051);
        assert_eq!(server.port, 50051);
    }

    #[test]
    fn test_grpc_server_register_service() {
        let server = GrpcServer::new("localhost", 50051);
        let service = GrpcServiceDef {
            name: "UserService".to_string(),
            methods: vec![GrpcMethod {
                name: "GetUser".to_string(),
                input_type: "UserRequest".to_string(),
                output_type: "UserResponse".to_string(),
                client_streaming: false,
                server_streaming: false,
            }],
        };
        let server = server.register_service(service);
        assert_eq!(server.services.len(), 1);
    }

    #[test]
    fn test_grpc_server_start() {
        let service = GrpcServiceDef {
            name: "UserService".to_string(),
            methods: vec![],
        };
        let server = GrpcServer::new("localhost", 50051).register_service(service);
        let handle = server.start();
        assert!(handle.is_ok());
        assert_eq!(handle.unwrap().address(), "localhost:50051");
    }

    #[test]
    fn test_grpc_server_start_no_services() {
        let server = GrpcServer::new("localhost", 50051);
        let handle = server.start();
        assert!(handle.is_err());
    }

    #[test]
    fn test_grpc_channel_new() {
        let channel = GrpcChannel::new("localhost:50051");
        assert_eq!(channel.address(), "localhost:50051");
    }

    #[test]
    fn test_grpc_channel_with_metadata() {
        let channel =
            GrpcChannel::new("localhost:50051").with_metadata("Authorization", "Bearer token");
        assert_eq!(
            channel.metadata().get("Authorization"),
            Some(&"Bearer token".to_string())
        );
    }

    #[test]
    fn test_user_grpc_client_connect() {
        let client = UserGrpcClient::connect("localhost:50051");
        assert!(client.is_ok());
    }

    #[test]
    fn test_user_grpc_client_connect_empty() {
        let client = UserGrpcClient::connect("");
        assert!(client.is_err());
    }

    #[test]
    fn test_grpc_service_def_new() {
        let service = GrpcServiceDef {
            name: "TestService".to_string(),
            methods: vec![],
        };
        assert_eq!(service.name, "TestService");
        assert!(service.methods.is_empty());
    }

    #[test]
    fn test_grpc_method_new() {
        let method = GrpcMethod {
            name: "TestMethod".to_string(),
            input_type: "Input".to_string(),
            output_type: "Output".to_string(),
            client_streaming: false,
            server_streaming: false,
        };
        assert_eq!(method.name, "TestMethod");
    }

    #[test]
    fn test_user_request_new() {
        let request = UserRequest {
            id: 123,
            username: "testuser".to_string(),
        };
        assert_eq!(request.id, 123);
        assert_eq!(request.username, "testuser");
    }

    #[test]
    fn test_user_response_new() {
        let response = UserResponse {
            id: 1,
            username: "john".to_string(),
            email: "john@example.com".to_string(),
        };
        assert_eq!(response.username, "john");
    }

    // ---- New tests verifying real in-memory RPC behavior ----

    #[test]
    fn test_client_get_user_from_server() {
        let addr = unique_addr("get_user_from_server");
        let svc = Arc::new(
            InMemoryUserService::new()
                .with_user(build_user(42, "alice", "alice@example.com"))
                .with_user(build_user(43, "bob", "bob@example.com")),
        );
        let server = GrpcServer::new(host_of(&addr), port_of(&addr))
            .register_user_service(svc)
            .register_service(GrpcServiceDef {
                name: "UserService".to_string(),
                methods: vec![],
            });
        let _handle = server.start().expect("server start");

        let client = UserGrpcClient::connect(&addr).expect("connect");
        let user = client.get_user(42).expect("get_user");
        assert_eq!(user.id, 42);
        assert_eq!(user.username, "alice");
        assert_eq!(user.email, "alice@example.com");

        let bob = client.get_user(43).expect("get_user bob");
        assert_eq!(bob.id, 43);
        assert_eq!(bob.username, "bob");
    }

    #[test]
    fn test_client_get_user_missing() {
        let addr = unique_addr("get_user_missing");
        let svc = Arc::new(InMemoryUserService::new().with_user(build_user(1, "a", "a@x")));
        let server = GrpcServer::new(host_of(&addr), port_of(&addr)).register_user_service(svc);
        let _handle = server.start().expect("server start");

        let client = UserGrpcClient::connect(&addr).expect("connect");
        let result = client.get_user(999);
        assert!(result.is_err());
    }

    #[test]
    fn test_client_list_users() {
        let addr = unique_addr("list_users");
        let svc = Arc::new(
            InMemoryUserService::new()
                .with_user(build_user(3, "c", "c@x"))
                .with_user(build_user(1, "a", "a@x"))
                .with_user(build_user(2, "b", "b@x")),
        );
        let server = GrpcServer::new(host_of(&addr), port_of(&addr)).register_user_service(svc);
        let _handle = server.start().expect("server start");

        let client = UserGrpcClient::connect(&addr).expect("connect");
        let users = client.list_users().expect("list_users");
        assert_eq!(users.len(), 3);
        // list_users returns sorted by id
        assert_eq!(users[0].id, 1);
        assert_eq!(users[1].id, 2);
        assert_eq!(users[2].id, 3);
    }

    #[test]
    fn test_client_connect_to_no_server() {
        let addr = unique_addr("no_server");
        let client = UserGrpcClient::connect(&addr).expect("connect");
        let result = client.get_user(1);
        assert!(matches!(result, Err(GrpcError::ConnectionFailed(_))));
    }

    #[test]
    fn test_client_connect_to_server_without_user_service() {
        let addr = unique_addr("without_user_service");
        // Server only registers a service definition, no UserGrpcService impl
        let server =
            GrpcServer::new(host_of(&addr), port_of(&addr)).register_service(GrpcServiceDef {
                name: "OtherService".to_string(),
                methods: vec![],
            });
        let _handle = server.start().expect("server start");

        let client = UserGrpcClient::connect(&addr).expect("connect");
        let result = client.get_user(1);
        assert!(matches!(result, Err(GrpcError::ServiceNotFound(_))));
    }

    #[test]
    fn test_grpc_channel_call_unary_directly() {
        let addr = unique_addr("channel_call_unary");
        let svc = Arc::new(InMemoryUserService::new().with_user(build_user(7, "g", "g@x")));
        let server = GrpcServer::new(host_of(&addr), port_of(&addr)).register_user_service(svc);
        let _handle = server.start().expect("server start");

        let channel = GrpcChannel::new(&addr);
        let response: UserResponse = channel
            .call_unary("UserService", "GetUser", |svc| {
                svc.get_user(UserRequest {
                    id: 7,
                    username: String::new(),
                })
            })
            .expect("call_unary");
        assert_eq!(response.id, 7);
        assert_eq!(response.username, "g");
    }

    #[test]
    fn test_in_memory_user_service_add_and_remove() {
        let svc = InMemoryUserService::new();
        svc.add_user(build_user(1, "a", "a@x"));
        assert_eq!(svc.list_users().unwrap().len(), 1);
        let removed = svc.remove_user(1);
        assert!(removed.is_some());
        assert_eq!(svc.list_users().unwrap().len(), 0);
    }

    #[test]
    fn test_handle_stop_removes_registry_entry() {
        let addr = unique_addr("handle_stop");
        let svc = Arc::new(InMemoryUserService::new().with_user(build_user(1, "a", "a@x")));
        let server = GrpcServer::new(host_of(&addr), port_of(&addr)).register_user_service(svc);
        let handle = server.start().expect("server start");

        let client = UserGrpcClient::connect(&addr).expect("connect");
        assert!(client.get_user(1).is_ok());

        handle.stop().expect("stop");
        let result = client.get_user(1);
        assert!(matches!(result, Err(GrpcError::ServiceNotFound(_))));
    }

    fn host_of(addr: &str) -> &str {
        addr.split(':').next().unwrap_or("localhost")
    }

    fn port_of(addr: &str) -> u16 {
        addr.split(':')
            .nth(1)
            .and_then(|p| p.parse().ok())
            .unwrap_or(0)
    }

    // ---- 真实 tonic gRPC 集成测试 ----
    // 仅在启用 `real` feature 时编译；标记为 #[ignore] 因为它们会真实绑定 TCP 端口
    // 并在后台运行 tokio task。通过 `cargo test --features real -- --ignored` 运行。

    #[cfg(feature = "real")]
    fn bind_addr() -> std::net::SocketAddr {
        // 0 端口让操作系统随机分配，避免并行测试时的端口冲突。
        "127.0.0.1:0".parse().expect("valid socket addr")
    }

    #[cfg(feature = "real")]
    #[tokio::test]
    #[ignore = "requires the real gRPC server (feature `real`)"]
    async fn test_real_grpc_get_user() {
        use crate::real_grpc::{RealGrpcClient, RealGrpcServer};

        let server = RealGrpcServer::new();
        server
            .backend()
            .add_user(build_user(42, "alice", "alice@example.com"));
        let mut handle = server.start(bind_addr()).await.expect("server start");
        let addr = handle.local_addr();

        let mut client = RealGrpcClient::connect(addr.to_string())
            .await
            .expect("connect");
        let user = client.get_user(42).await.expect("get_user");
        assert_eq!(user.id, 42);
        assert_eq!(user.username, "alice");
        assert_eq!(user.email, "alice@example.com");

        handle.stop().await.expect("stop");
    }

    #[cfg(feature = "real")]
    #[tokio::test]
    #[ignore = "requires the real gRPC server (feature `real`)"]
    async fn test_real_grpc_list_users() {
        use crate::real_grpc::{RealGrpcClient, RealGrpcServer};

        let server = RealGrpcServer::new();
        // 故意乱序插入，验证 list_users 返回按 id 升序。
        server.backend().add_user(build_user(3, "c", "c@x"));
        server.backend().add_user(build_user(1, "a", "a@x"));
        server.backend().add_user(build_user(2, "b", "b@x"));
        let mut handle = server.start(bind_addr()).await.expect("server start");
        let addr = handle.local_addr();

        let mut client = RealGrpcClient::connect(addr.to_string())
            .await
            .expect("connect");
        let users = client.list_users().await.expect("list_users");
        assert_eq!(users.len(), 3);
        assert_eq!(users[0].id, 1);
        assert_eq!(users[0].username, "a");
        assert_eq!(users[1].id, 2);
        assert_eq!(users[2].id, 3);

        handle.stop().await.expect("stop");
    }

    #[cfg(feature = "real")]
    #[tokio::test]
    #[ignore = "requires the real gRPC server (feature `real`)"]
    async fn test_real_grpc_user_not_found() {
        use crate::real_grpc::{RealGrpcClient, RealGrpcServer};

        let server = RealGrpcServer::new();
        server.backend().add_user(build_user(1, "a", "a@x"));
        let mut handle = server.start(bind_addr()).await.expect("server start");
        let addr = handle.local_addr();

        let mut client = RealGrpcClient::connect(addr.to_string())
            .await
            .expect("connect");
        let result = client.get_user(999).await;
        // 服务器把 GrpcError::MethodNotFound 映射为 tonic::Status::not_found，
        // 客户端再还原为 GrpcError::MethodNotFound。
        assert!(
            matches!(result, Err(GrpcError::MethodNotFound(_))),
            "expected MethodNotFound, got {:?}",
            result
        );

        handle.stop().await.expect("stop");
    }

    #[cfg(feature = "real")]
    #[tokio::test]
    #[ignore = "requires the real gRPC server (feature `real`)"]
    async fn test_real_grpc_multiple_clients() {
        use crate::real_grpc::{RealGrpcClient, RealGrpcServer};

        let server = RealGrpcServer::new();
        server
            .backend()
            .add_user(build_user(42, "shared", "shared@example.com"));
        let mut handle = server.start(bind_addr()).await.expect("server start");
        let addr = handle.local_addr();
        let addr_str = addr.to_string();

        // 同一服务器应支持多个并发客户端。
        let mut client1 = RealGrpcClient::connect(addr_str.clone())
            .await
            .expect("connect1");
        let mut client2 = RealGrpcClient::connect(addr_str).await.expect("connect2");

        let u1 = client1.get_user(42).await.expect("get_user via client1");
        let u2 = client2.get_user(42).await.expect("get_user via client2");
        assert_eq!(u1.id, 42);
        assert_eq!(u1.username, "shared");
        assert_eq!(u1.email, "shared@example.com");
        assert_eq!(u2.id, 42);
        assert_eq!(u2.username, "shared");

        // 第二个客户端应也能正确感知后端数据。
        let list = client2.list_users().await.expect("list_users via client2");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, 42);

        handle.stop().await.expect("stop");
    }
}
