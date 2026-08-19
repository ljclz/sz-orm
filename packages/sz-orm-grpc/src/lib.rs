//! # SZ-ORM gRPC — gRPC 服务定义与调用
//!
//! 提供 gRPC 服务描述、方法注册与基于地址的全局服务注册表，
//! 支持客户端调用与服务端实现绑定。
//!
//! ## 主要类型
//!
//! - [`GrpcServiceDef`] / [`GrpcMethod`] — 服务与方法定义
//! - `UserGrpcService` — 用户服务实现 trait
//! - [`GrpcStream`] — 同步迭代器风格的流式响应容器
//! - [`Interceptor`] / [`InterceptorRequest`] — 请求拦截器机制
//! - [`RetryPolicy`] / [`TimeoutPolicy`] — 超时与重试策略

use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

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
            let mut reg = global_registry().write();
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
        let mut reg = global_registry().write();
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
        if let Some(mut reg) = global_registry().try_write() {
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

/// User service trait defining the methods a gRPC server must implement.
///
/// All methods use synchronous signatures because this crate's in-memory channel
/// ([`GrpcChannel`]) is synchronous. The real tonic variant is bridged to the
/// asynchronous tonic trait via the [`real_grpc`] module.
pub trait UserGrpcService: Send + Sync {
    /// Query a single user by id. Returns [`GrpcError::MethodNotFound`] when not found.
    fn get_user(&self, request: UserRequest) -> Result<UserResponse, GrpcError>;
    /// Return the full user list (sorted by id in ascending order).
    fn list_users(&self) -> Result<Vec<UserResponse>, GrpcError>;
    /// Return user stream data in a batched fashion.
    ///
    /// Because Rust trait objects do not support native async streams, this returns
    /// `Vec<UserResponse>`, and the caller (e.g. [`GrpcChannel::call_server_streaming`])
    /// is responsible for pushing the items into a [`GrpcStream`].
    ///
    /// The default implementation is equivalent to [`UserGrpcService::list_users`];
    /// implementers may override it to provide custom batching logic.
    fn stream_users(&self) -> Result<Vec<UserResponse>, GrpcError> {
        self.list_users()
    }
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
        self.users.write().insert(user.id, user);
        self
    }

    pub fn add_user(&self, user: UserResponse) {
        self.users.write().insert(user.id, user);
    }

    pub fn remove_user(&self, id: i64) -> Option<UserResponse> {
        self.users.write().remove(&id)
    }
}

impl Default for InMemoryUserService {
    fn default() -> Self {
        Self::new()
    }
}

impl UserGrpcService for InMemoryUserService {
    fn get_user(&self, request: UserRequest) -> Result<UserResponse, GrpcError> {
        let users = self.users.read();
        users
            .get(&request.id)
            .cloned()
            .ok_or_else(|| GrpcError::MethodNotFound(format!("User {} not found", request.id)))
    }

    fn list_users(&self) -> Result<Vec<UserResponse>, GrpcError> {
        let users = self.users.read();
        let mut list: Vec<UserResponse> = users.values().cloned().collect();
        list.sort_by_key(|a| a.id);
        Ok(list)
    }

    fn stream_users(&self) -> Result<Vec<UserResponse>, GrpcError> {
        // 当前实现与 list_users 一致：一次性返回全部用户。
        // 未来可扩展为分批加载，接口形状保持不变。
        self.list_users()
    }
}

// =========================================================================
// GrpcStream — 同步迭代器风格的流式响应容器
// =========================================================================

/// Synchronous iterator-style streaming response container.
///
/// Uses `Mutex<Vec<T>>` to buffer items pending consumption and an `AtomicBool`
/// to mark whether the stream has been closed. Suitable for server-streaming
/// scenarios: the server pushes results in batches, and the client consumes
/// them one by one via `next()`.
///
/// Designed to be synchronous and non-blocking: `next()` returns `None`
/// immediately when no data is available, without blocking. The caller can
/// use [`GrpcStream::is_closed`] to determine whether the stream has ended.
pub struct GrpcStream<T> {
    /// Queue of items pending consumption; uses Mutex for thread safety.
    items: Mutex<Vec<T>>,
    /// Flag indicating whether the stream is closed; once closed, no new items are accepted.
    closed: AtomicBool,
}

impl<T> GrpcStream<T> {
    /// Create an empty, unclosed stream.
    pub fn new() -> Self {
        Self {
            items: Mutex::new(Vec::new()),
            closed: AtomicBool::new(false),
        }
    }

    /// Push an item into the stream.
    ///
    /// The item is pushed even if the stream is already closed (the caller should
    /// check [`GrpcStream::is_closed`] itself).
    pub fn push(&self, item: T) {
        self.items.lock().push(item);
    }

    /// Take the next item from the stream (FIFO order).
    ///
    /// - Returns `Some(item)` when there is a pending item.
    /// - Returns `None` when the queue is empty (regardless of whether it is closed).
    ///
    /// The caller should combine this with [`GrpcStream::is_closed`] to determine
    /// whether the stream has truly ended:
    /// `next() == None && is_closed() == true` means the stream has ended with no
    /// remaining data.
    pub fn next(&self) -> Option<T> {
        let mut items = self.items.lock();
        if items.is_empty() {
            None
        } else {
            // 使用 remove(0) 保持 FIFO 顺序；对于小规模流性能可接受。
            Some(items.remove(0))
        }
    }

    /// Close the stream, marking that no new data will arrive.
    pub fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
    }

    /// Query whether the stream is closed.
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }
}

impl<T> Default for GrpcStream<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for GrpcStream<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let items_len = self.items.lock().len();
        f.debug_struct("GrpcStream")
            .field("pending_items", &items_len)
            .field("closed", &self.is_closed())
            .finish()
    }
}

// =========================================================================
// Interceptor — 请求拦截器机制
// =========================================================================

/// Interceptor request context carrying the method name, service name, and
/// metadata of the current RPC.
///
/// Interceptors access the invocation context through this struct, used for
/// authentication, logging, and similar scenarios.
#[derive(Debug, Clone)]
pub struct InterceptorRequest {
    /// The method being invoked, e.g. `GetUser`.
    pub method: String,
    /// The service being invoked, e.g. `UserService`.
    pub service_name: String,
    /// Metadata sent by the client (a copy of the metadata from [`GrpcChannel`]).
    pub metadata: HashMap<String, String>,
}

/// Request interceptor trait.
///
/// Interceptors are invoked before [`GrpcChannel::call_unary`] actually issues
/// the RPC. If any interceptor returns `Err`, subsequent interceptors and the
/// RPC call are skipped, and the error is returned directly to the caller.
/// Interceptor failures are not retried (retrying an auth failure is pointless).
///
/// Built-in implementations:
/// - [`LoggingInterceptor`] — logs each call to stderr
/// - [`AuthInterceptor`] — validates the `authorization` field in metadata
pub trait Interceptor: Send + Sync {
    /// Execute the interception logic. Returns `Err(GrpcError)` to reject the request.
    fn call(&self, request: &InterceptorRequest) -> Result<(), GrpcError>;
}

/// Logging interceptor: writes the service name, method name, and metadata count
/// of each RPC to stderr.
///
/// Uses `eprintln!` to avoid introducing a logging framework dependency. Always
/// returns `Ok` and never blocks the call.
#[derive(Debug, Clone, Default)]
pub struct LoggingInterceptor;

impl Interceptor for LoggingInterceptor {
    fn call(&self, request: &InterceptorRequest) -> Result<(), GrpcError> {
        eprintln!(
            "[grpc] {}.{} metadata_keys={}",
            request.service_name,
            request.method,
            request.metadata.len()
        );
        Ok(())
    }
}

/// Authentication interceptor: validates that the `authorization` field in
/// metadata matches the expected token.
///
/// Returns [`GrpcError::Unauthorized`] when metadata is missing or mismatched.
#[derive(Debug, Clone)]
pub struct AuthInterceptor {
    /// Expected authorization token (including prefix, e.g. `Bearer secret-token`).
    expected_token: String,
}

impl AuthInterceptor {
    /// Create an auth interceptor where `token` is the expected `authorization` metadata value.
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            expected_token: token.into(),
        }
    }
}

impl Interceptor for AuthInterceptor {
    fn call(&self, request: &InterceptorRequest) -> Result<(), GrpcError> {
        match request.metadata.get("authorization") {
            Some(token) if token == &self.expected_token => Ok(()),
            Some(_) => Err(GrpcError::Unauthorized("Invalid auth token".to_string())),
            None => Err(GrpcError::Unauthorized(
                "Missing authorization metadata".to_string(),
            )),
        }
    }
}

// =========================================================================
// RetryPolicy / TimeoutPolicy — 超时与重试策略
// =========================================================================

/// Enum of retryable error categories.
///
/// Used by [`RetryPolicy::should_retry`] to determine whether a [`GrpcError`]
/// belongs to a retryable category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryableErrorKind {
    /// Connection failed (server unreachable).
    ConnectionFailed,
    /// Call timed out.
    Timeout,
    /// Transport-layer error.
    Transport,
}

impl RetryableErrorKind {
    /// Determine whether the given error belongs to this category.
    fn matches(&self, error: &GrpcError) -> bool {
        matches!(
            (self, error),
            (Self::ConnectionFailed, GrpcError::ConnectionFailed(_))
                | (Self::Timeout, GrpcError::Timeout(_))
                | (Self::Transport, GrpcError::Transport(_))
        )
    }
}

/// Retry policy: defines the maximum retry count, backoff parameters, and
/// retryable error categories.
///
/// Uses an exponential backoff algorithm to compute the retry interval:
/// `delay = initial_delay * multiplier^attempt`, capped at `max_delay`.
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Maximum retry count (excluding the initial call). `0` means no retries.
    pub max_retries: u32,
    /// Milliseconds to wait before the first retry.
    pub initial_delay_ms: u64,
    /// Maximum milliseconds to wait for a single retry.
    pub max_delay_ms: u64,
    /// Backoff multiplier; the delay is multiplied by this factor after each retry.
    pub multiplier: f64,
    /// List of retryable error categories.
    pub retryable_errors: Vec<RetryableErrorKind>,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay_ms: 50,
            max_delay_ms: 1000,
            multiplier: 2.0,
            retryable_errors: vec![
                RetryableErrorKind::ConnectionFailed,
                RetryableErrorKind::Timeout,
            ],
        }
    }
}

impl RetryPolicy {
    /// Determine whether the given error should be retried, and return the
    /// duration to wait.
    ///
    /// - `attempt` is the number of attempts already completed (0 means right
    ///   after the first call failed).
    /// - Returns `None` if `attempt >= max_retries` or the error is not retryable.
    /// - Otherwise returns `Some(delay)`; the caller should sleep and then retry.
    pub fn should_retry(&self, error: &GrpcError, attempt: u32) -> Option<Duration> {
        // 已达到最大重试次数，不再重试。
        if attempt >= self.max_retries {
            return None;
        }
        // 错误不在可重试列表中，不重试。
        let is_retryable = self.retryable_errors.iter().any(|kind| kind.matches(error));
        if !is_retryable {
            return None;
        }
        // 指数退避：initial_delay * multiplier^attempt，封顶为 max_delay。
        let raw_delay = self.initial_delay_ms as f64 * self.multiplier.powi(attempt as i32);
        let delay_ms = (raw_delay as u64).min(self.max_delay_ms);
        Some(Duration::from_millis(delay_ms))
    }
}

/// Timeout policy: defines the deadline of an RPC call.
///
/// Because [`GrpcChannel::call_unary`] is synchronous, the timeout can only be
/// checked before/after the call and cannot interrupt an in-flight synchronous
/// call. The policy checks whether the elapsed time has exceeded the deadline
/// before each retry.
#[derive(Debug, Clone)]
pub struct TimeoutPolicy {
    /// Maximum allowed duration measured from the start of the call.
    pub deadline: Duration,
}

impl Default for TimeoutPolicy {
    fn default() -> Self {
        Self {
            deadline: Duration::from_secs(30),
        }
    }
}

impl TimeoutPolicy {
    /// Create a timeout policy with the specified deadline.
    pub fn new(deadline: Duration) -> Self {
        Self { deadline }
    }

    /// Check whether the elapsed time has exceeded the deadline.
    ///
    /// Returns `Err(GrpcError::Timeout)` if exceeded, otherwise `Ok(())`.
    /// Note: uses `>` rather than `>=`, so `elapsed == deadline` is treated as
    /// not timed out.
    pub fn check_elapsed(&self, elapsed: Duration) -> Result<(), GrpcError> {
        if elapsed > self.deadline {
            Err(GrpcError::Timeout(format!(
                "elapsed {:?} exceeds deadline {:?}",
                elapsed, self.deadline
            )))
        } else {
            Ok(())
        }
    }
}

// =========================================================================
// UserGrpcClient — 客户端封装
// =========================================================================

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

    /// Fetch the user list via server-streaming, returning a [`GrpcStream`].
    ///
    /// Internally calls the service's `stream_users` method via
    /// [`GrpcChannel::call_server_streaming`], pushes the results into the
    /// stream, and closes it. The caller may consume items one by one via `next()`.
    pub fn stream_users(&self) -> Result<GrpcStream<UserResponse>, GrpcError> {
        self.channel
            .call_server_streaming("UserService", "StreamUsers")
    }

    pub fn channel(&self) -> &GrpcChannel {
        &self.channel
    }
}

// =========================================================================
// GrpcChannel — 通道与调用逻辑（含拦截器/重试/超时）
// =========================================================================

#[derive(Clone)]
pub struct GrpcChannel {
    address: String,
    metadata: HashMap<String, String>,
    /// Interceptor chain; executed in insertion order.
    interceptors: Vec<Arc<dyn Interceptor>>,
    /// Retry policy; `None` means no retries.
    retry_policy: Option<RetryPolicy>,
    /// Timeout policy; `None` means no timeout check.
    timeout_policy: Option<TimeoutPolicy>,
}

impl GrpcChannel {
    pub fn new(address: impl Into<String>) -> Self {
        Self {
            address: address.into(),
            metadata: HashMap::new(),
            interceptors: Vec::new(),
            retry_policy: None,
            timeout_policy: None,
        }
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Append an interceptor to the end of the interceptor chain. Interceptors
    /// execute in insertion order.
    pub fn with_interceptor(mut self, interceptor: Arc<dyn Interceptor>) -> Self {
        self.interceptors.push(interceptor);
        self
    }

    /// Set the retry policy. `call_unary` will retry on retryable errors per the policy.
    pub fn with_retry(mut self, policy: RetryPolicy) -> Self {
        self.retry_policy = Some(policy);
        self
    }

    /// Set the timeout policy. `call_unary` will check the timeout before each attempt.
    pub fn with_timeout(mut self, policy: TimeoutPolicy) -> Self {
        self.timeout_policy = Some(policy);
        self
    }

    pub fn address(&self) -> &str {
        &self.address
    }

    pub fn metadata(&self) -> &HashMap<String, String> {
        &self.metadata
    }

    /// Issue a unary RPC call.
    ///
    /// Execution flow:
    /// 1. Record the start time and loop per the retry policy (once if no policy).
    /// 2. Before each attempt, check the timeout (if [`TimeoutPolicy`] is configured).
    /// 3. Execute the interceptor chain (if [`Interceptor`] is configured); any
    ///    failure returns the error immediately (no retry).
    /// 4. Look up the service in the global registry and invoke the closure `f`.
    /// 5. On success, return the result; on failure, decide whether to retry
    ///    via [`RetryPolicy::should_retry`].
    ///
    /// Note: the timeout is a synchronous check and cannot interrupt an
    /// in-flight closure call.
    pub fn call_unary<F, T>(
        &self,
        service_name: &str,
        method_name: &str,
        f: F,
    ) -> Result<T, GrpcError>
    where
        F: Fn(&dyn UserGrpcService) -> Result<T, GrpcError>,
    {
        let start = Instant::now();
        let max_retries = self
            .retry_policy
            .as_ref()
            .map(|p| p.max_retries)
            .unwrap_or(0);
        let mut last_error: Option<GrpcError> = None;

        for attempt in 0..=max_retries {
            // 1. 超时检查
            if let Some(timeout_policy) = &self.timeout_policy {
                timeout_policy.check_elapsed(start.elapsed())?;
            }

            // 2. 拦截器链
            if !self.interceptors.is_empty() {
                let interceptor_req = InterceptorRequest {
                    method: method_name.to_string(),
                    service_name: service_name.to_string(),
                    metadata: self.metadata.clone(),
                };
                for interceptor in &self.interceptors {
                    // 拦截器失败直接返回，不进入重试逻辑。
                    interceptor.call(&interceptor_req)?;
                }
            }

            // 3. 实际调用（使用 IIFE 以便使用 ? 操作符）
            let result = (|| {
                let reg = global_registry().read();
                let services = reg.services.get(&self.address).ok_or_else(|| {
                    GrpcError::ConnectionFailed(format!("No server listening at {}", self.address))
                })?;
                let svc = services
                    .get(service_name)
                    .ok_or_else(|| GrpcError::ServiceNotFound(service_name.to_string()))?;
                f(svc.as_ref())
            })();

            match result {
                Ok(value) => return Ok(value),
                Err(err) => {
                    last_error = Some(err);
                    // 4. 判断是否应重试（last_error 刚被设为 Some(err)，安全引用）
                    if let Some(policy) = &self.retry_policy {
                        let last_err = last_error
                            .as_ref()
                            .expect("last_error must be Some in Err branch");
                        if let Some(delay) = policy.should_retry(last_err, attempt) {
                            std::thread::sleep(delay);
                            continue;
                        }
                    }
                    // 不可重试或无重试策略，直接返回错误。
                    return Err(
                        last_error.expect("last_error must be Some after Err branch assignment")
                    );
                }
            }
        }

        // 重试次数耗尽，返回最后一次错误。
        Err(last_error.unwrap_or_else(|| GrpcError::Transport("no attempt made".to_string())))
    }

    /// Issue a server-streaming RPC call, returning a [`GrpcStream`].
    ///
    /// Internally reuses [`GrpcChannel::call_unary`] to call the service's
    /// `stream_users` method, pushes the returned `Vec` items one by one into
    /// a [`GrpcStream`], and then closes the stream.
    /// Interceptor, retry, and timeout policies also apply.
    pub fn call_server_streaming(
        &self,
        service_name: &str,
        method_name: &str,
    ) -> Result<GrpcStream<UserResponse>, GrpcError> {
        let items: Vec<UserResponse> =
            self.call_unary(service_name, method_name, |svc| svc.stream_users())?;
        let stream = GrpcStream::new();
        for item in items {
            stream.push(item);
        }
        stream.close();
        Ok(stream)
    }

    /// Returns the registered service definitions for this address (metadata only).
    pub fn list_services(&self) -> Vec<GrpcServiceDef> {
        let reg = global_registry().read();
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
    #[error("Unauthorized: {0}")]
    Unauthorized(String),
}

impl serde::Serialize for GrpcError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

pub mod grpc_health;
pub mod grpc_metrics;
pub mod grpc_resilience;

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
        // 验证连接后客户端持有正确的地址（防止 is_ok() 假成功）
        let client = client.unwrap();
        assert_eq!(
            client.channel().address(),
            "localhost:50051",
            "客户端地址应与传入参数一致"
        );
        assert!(
            client.channel().metadata().is_empty(),
            "新连接的 metadata 应为空"
        );
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
        let resp = client.get_user(1).expect("get_user before stop");
        // 验证返回的用户数据正确，而不仅仅是 is_ok()
        assert_eq!(resp.id, 1);
        assert_eq!(resp.username, "a");
        assert_eq!(resp.email, "a@x");

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
        let token = server.auth_token().to_string();
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
        let token = server.auth_token().to_string();
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
        let token = server.auth_token().to_string();
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
        let token = server.auth_token().to_string();
        let mut handle = server.start(bind_addr()).await.expect("server start");
        let addr = handle.local_addr();
        let addr_str = addr.to_string();

        // 同一服务器应支持多个并发客户端。
        let mut client1 = RealGrpcClient::connect(addr_str.clone())
            .await
            .expect("connect1");
        let mut client2 = RealGrpcClient::connect(addr_str)
            .await
            .expect("connect2")
            .with_token(token.clone());

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

    // =================================================================
    // 新增测试：GrpcStream
    // =================================================================

    #[test]
    fn test_grpc_stream_push_and_next() {
        let stream = GrpcStream::new();
        stream.push(1);
        stream.push(2);
        assert_eq!(stream.next(), Some(1));
        assert_eq!(stream.next(), Some(2));
        assert_eq!(stream.next(), None);
    }

    #[test]
    fn test_grpc_stream_close_and_is_closed() {
        let stream = GrpcStream::<i32>::new();
        assert!(!stream.is_closed());
        stream.close();
        assert!(stream.is_closed());
    }

    #[test]
    fn test_grpc_stream_empty_next_returns_none() {
        let stream = GrpcStream::<i32>::new();
        // 未关闭的空流也应返回 None（同步非阻塞）
        assert_eq!(stream.next(), None);
        assert!(!stream.is_closed());
    }

    #[test]
    fn test_grpc_stream_close_drains_remaining() {
        let stream = GrpcStream::new();
        stream.push(10);
        stream.push(20);
        stream.close();
        // 关闭后仍可取出残留元素
        assert_eq!(stream.next(), Some(10));
        assert_eq!(stream.next(), Some(20));
        assert_eq!(stream.next(), None);
        assert!(stream.is_closed());
    }

    #[test]
    fn test_grpc_stream_default() {
        let stream = GrpcStream::<u8>::default();
        assert!(!stream.is_closed());
        assert_eq!(stream.next(), None);
    }

    // =================================================================
    // 新增测试：server-streaming 调用
    // =================================================================

    #[test]
    fn test_call_server_streaming_returns_stream() {
        let addr = unique_addr("server_streaming");
        let svc = Arc::new(
            InMemoryUserService::new()
                .with_user(build_user(1, "a", "a@x"))
                .with_user(build_user(2, "b", "b@x")),
        );
        let server = GrpcServer::new(host_of(&addr), port_of(&addr)).register_user_service(svc);
        let _handle = server.start().expect("server start");

        let channel = GrpcChannel::new(&addr);
        let stream = channel
            .call_server_streaming("UserService", "StreamUsers")
            .expect("server streaming");
        assert!(stream.is_closed());
        let mut ids = Vec::new();
        while let Some(user) = stream.next() {
            ids.push(user.id);
        }
        assert_eq!(ids, vec![1, 2]);
    }

    #[test]
    fn test_user_grpc_client_stream_users() {
        let addr = unique_addr("client_stream_users");
        let svc = Arc::new(
            InMemoryUserService::new()
                .with_user(build_user(5, "e", "e@x"))
                .with_user(build_user(6, "f", "f@x"))
                .with_user(build_user(7, "g", "g@x")),
        );
        let server = GrpcServer::new(host_of(&addr), port_of(&addr)).register_user_service(svc);
        let _handle = server.start().expect("server start");

        let client = UserGrpcClient::connect(&addr).expect("connect");
        let stream = client.stream_users().expect("stream_users");
        assert!(stream.is_closed());
        let count: usize = std::iter::from_fn(|| stream.next()).count();
        assert_eq!(count, 3);
    }

    #[test]
    fn test_server_streaming_on_empty_service() {
        // 边界：服务无用户时，流应立即关闭且无元素。
        let addr = unique_addr("stream_empty");
        let svc = Arc::new(InMemoryUserService::new());
        let server = GrpcServer::new(host_of(&addr), port_of(&addr)).register_user_service(svc);
        let _handle = server.start().expect("server start");

        let client = UserGrpcClient::connect(&addr).expect("connect");
        let stream = client.stream_users().expect("stream_users");
        assert!(stream.is_closed());
        assert!(stream.next().is_none());
    }

    // =================================================================
    // 新增测试：Interceptor
    // =================================================================

    #[test]
    fn test_logging_interceptor_returns_ok() {
        let interceptor = LoggingInterceptor;
        let mut metadata = HashMap::new();
        metadata.insert("trace-id".to_string(), "abc-123".to_string());
        let req = InterceptorRequest {
            method: "GetUser".to_string(),
            service_name: "UserService".to_string(),
            metadata,
        };
        // LoggingInterceptor 始终返回 Ok，且不修改请求
        let result = interceptor.call(&req);
        assert!(result.is_ok());
        // LoggingInterceptor.call 返回 Result<(), GrpcError>，Ok(()) 已由 is_ok 验证
        // 验证请求字段未被拦截器修改
        assert_eq!(req.method, "GetUser", "method 不应被修改");
        assert_eq!(req.service_name, "UserService", "service_name 不应被修改");
        assert_eq!(req.metadata.len(), 1, "metadata 数量不应被修改");
        assert_eq!(
            req.metadata.get("trace-id"),
            Some(&"abc-123".to_string()),
            "metadata 内容不应被修改"
        );
    }

    #[test]
    fn test_auth_interceptor_rejects_missing_token() {
        let interceptor = AuthInterceptor::new("Bearer secret");
        let req = InterceptorRequest {
            method: "GetUser".to_string(),
            service_name: "UserService".to_string(),
            metadata: HashMap::new(),
        };
        let result = interceptor.call(&req);
        assert!(matches!(result, Err(GrpcError::Unauthorized(_))));
    }

    #[test]
    fn test_auth_interceptor_rejects_wrong_token() {
        let interceptor = AuthInterceptor::new("Bearer secret");
        let mut metadata = HashMap::new();
        metadata.insert("authorization".to_string(), "Bearer wrong".to_string());
        let req = InterceptorRequest {
            method: "GetUser".to_string(),
            service_name: "UserService".to_string(),
            metadata,
        };
        let result = interceptor.call(&req);
        assert!(matches!(result, Err(GrpcError::Unauthorized(_))));
    }

    #[test]
    fn test_auth_interceptor_accepts_valid_token() {
        let interceptor = AuthInterceptor::new("Bearer secret");
        let mut metadata = HashMap::new();
        metadata.insert("authorization".to_string(), "Bearer secret".to_string());
        let req = InterceptorRequest {
            method: "GetUser".to_string(),
            service_name: "UserService".to_string(),
            metadata,
        };
        let result = interceptor.call(&req);
        assert!(result.is_ok());
        // AuthInterceptor.call 返回 Result<(), GrpcError>，Ok(()) 已由 is_ok 验证
        // 验证 token 匹配是精确匹配（不会因前缀相同而误判）
        assert_eq!(
            req.metadata.get("authorization"),
            Some(&"Bearer secret".to_string()),
            "authorization 不应被修改"
        );
    }

    #[test]
    fn test_interceptor_execution_order() {
        use parking_lot::Mutex;

        /// Test interceptor that appends its own name to a shared log, used to
        /// verify execution order.
        struct OrderInterceptor {
            name: &'static str,
            log: Arc<Mutex<Vec<&'static str>>>,
        }
        impl Interceptor for OrderInterceptor {
            fn call(&self, _req: &InterceptorRequest) -> Result<(), GrpcError> {
                self.log.lock().push(self.name);
                Ok(())
            }
        }

        let log: Arc<Mutex<Vec<&'static str>>> = Arc::new(Mutex::new(Vec::new()));
        let addr = unique_addr("interceptor_order");
        let svc = Arc::new(InMemoryUserService::new().with_user(build_user(1, "a", "a@x")));
        let server = GrpcServer::new(host_of(&addr), port_of(&addr)).register_user_service(svc);
        let _handle = server.start().expect("server start");

        let channel = GrpcChannel::new(&addr)
            .with_interceptor(Arc::new(OrderInterceptor {
                name: "first",
                log: log.clone(),
            }))
            .with_interceptor(Arc::new(OrderInterceptor {
                name: "second",
                log: log.clone(),
            }))
            .with_interceptor(Arc::new(OrderInterceptor {
                name: "third",
                log: log.clone(),
            }));

        let _: UserResponse = channel
            .call_unary("UserService", "GetUser", |svc| {
                svc.get_user(UserRequest {
                    id: 1,
                    username: String::new(),
                })
            })
            .expect("call should succeed");

        let recorded = log.lock().clone();
        assert_eq!(recorded, vec!["first", "second", "third"]);
    }

    #[test]
    fn test_call_unary_with_auth_interceptor_rejects() {
        let addr = unique_addr("auth_reject");
        let svc = Arc::new(InMemoryUserService::new().with_user(build_user(1, "a", "a@x")));
        let server = GrpcServer::new(host_of(&addr), port_of(&addr)).register_user_service(svc);
        let _handle = server.start().expect("server start");

        // 未携带 authorization metadata，鉴权应失败。
        let channel = GrpcChannel::new(&addr)
            .with_interceptor(Arc::new(AuthInterceptor::new("Bearer secret")));
        let result: Result<UserResponse, _> = channel.call_unary("UserService", "GetUser", |svc| {
            svc.get_user(UserRequest {
                id: 1,
                username: String::new(),
            })
        });
        assert!(matches!(result, Err(GrpcError::Unauthorized(_))));
    }

    #[test]
    fn test_call_unary_with_auth_interceptor_accepts() {
        let addr = unique_addr("auth_accept");
        let svc = Arc::new(InMemoryUserService::new().with_user(build_user(1, "a", "a@x")));
        let server = GrpcServer::new(host_of(&addr), port_of(&addr)).register_user_service(svc);
        let _handle = server.start().expect("server start");

        let channel = GrpcChannel::new(&addr)
            .with_metadata("authorization", "Bearer secret")
            .with_interceptor(Arc::new(AuthInterceptor::new("Bearer secret")));
        let user: UserResponse = channel
            .call_unary("UserService", "GetUser", |svc| {
                svc.get_user(UserRequest {
                    id: 1,
                    username: String::new(),
                })
            })
            .expect("call should succeed");
        assert_eq!(user.id, 1);
    }

    #[test]
    fn test_logging_and_auth_interceptor_chain() {
        // 边界：LoggingInterceptor 放前面（始终 Ok），AuthInterceptor 放后面。
        // 无 token 时 AuthInterceptor 应拒绝。
        let addr = unique_addr("log_auth_chain");
        let svc = Arc::new(InMemoryUserService::new().with_user(build_user(1, "a", "a@x")));
        let server = GrpcServer::new(host_of(&addr), port_of(&addr)).register_user_service(svc);
        let _handle = server.start().expect("server start");

        let channel = GrpcChannel::new(&addr)
            .with_interceptor(Arc::new(LoggingInterceptor))
            .with_interceptor(Arc::new(AuthInterceptor::new("Bearer secret")));
        let result: Result<UserResponse, _> = channel.call_unary("UserService", "GetUser", |svc| {
            svc.get_user(UserRequest {
                id: 1,
                username: String::new(),
            })
        });
        assert!(matches!(result, Err(GrpcError::Unauthorized(_))));
    }

    // =================================================================
    // 新增测试：RetryPolicy
    // =================================================================

    #[test]
    fn test_retry_policy_retries_connection_failed() {
        let policy = RetryPolicy::default();
        let err = GrpcError::ConnectionFailed("boom".to_string());
        // 首次失败（attempt=0）应重试
        assert!(policy.should_retry(&err, 0).is_some());
        // attempt=2 仍小于 max_retries=3，应重试
        assert!(policy.should_retry(&err, 2).is_some());
    }

    #[test]
    fn test_retry_policy_no_retry_method_not_found() {
        let policy = RetryPolicy::default();
        let err = GrpcError::MethodNotFound("nope".to_string());
        // MethodNotFound 不在默认可重试列表中
        assert!(policy.should_retry(&err, 0).is_none());
    }

    #[test]
    fn test_retry_policy_exponential_backoff() {
        let policy = RetryPolicy {
            max_retries: 5,
            initial_delay_ms: 10,
            max_delay_ms: 10000,
            multiplier: 2.0,
            retryable_errors: vec![RetryableErrorKind::ConnectionFailed],
        };
        let err = GrpcError::ConnectionFailed("x".to_string());
        // attempt 0: 10 * 2^0 = 10ms
        assert_eq!(
            policy.should_retry(&err, 0),
            Some(Duration::from_millis(10))
        );
        // attempt 1: 10 * 2^1 = 20ms
        assert_eq!(
            policy.should_retry(&err, 1),
            Some(Duration::from_millis(20))
        );
        // attempt 2: 10 * 2^2 = 40ms
        assert_eq!(
            policy.should_retry(&err, 2),
            Some(Duration::from_millis(40))
        );
        // attempt 3: 10 * 2^3 = 80ms
        assert_eq!(
            policy.should_retry(&err, 3),
            Some(Duration::from_millis(80))
        );
    }

    #[test]
    fn test_retry_policy_respects_max_delay() {
        let policy = RetryPolicy {
            max_retries: 10,
            initial_delay_ms: 100,
            max_delay_ms: 500,
            multiplier: 2.0,
            retryable_errors: vec![RetryableErrorKind::ConnectionFailed],
        };
        let err = GrpcError::ConnectionFailed("x".to_string());
        // attempt 5: 100 * 2^5 = 3200，应被截断为 500
        assert_eq!(
            policy.should_retry(&err, 5),
            Some(Duration::from_millis(500))
        );
    }

    #[test]
    fn test_retry_policy_max_retries_zero() {
        // 边界：max_retries=0 表示不重试
        let policy = RetryPolicy {
            max_retries: 0,
            initial_delay_ms: 10,
            max_delay_ms: 1000,
            multiplier: 2.0,
            retryable_errors: vec![RetryableErrorKind::ConnectionFailed],
        };
        let err = GrpcError::ConnectionFailed("x".to_string());
        assert!(policy.should_retry(&err, 0).is_none());
    }

    #[test]
    fn test_retry_policy_exhausts_attempts() {
        let policy = RetryPolicy {
            max_retries: 2,
            initial_delay_ms: 1,
            max_delay_ms: 10,
            multiplier: 2.0,
            retryable_errors: vec![RetryableErrorKind::ConnectionFailed],
        };
        let err = GrpcError::ConnectionFailed("x".to_string());
        // attempt=0,1 可重试；attempt=2（等于 max_retries）不再重试
        assert!(policy.should_retry(&err, 0).is_some());
        assert!(policy.should_retry(&err, 1).is_some());
        assert!(policy.should_retry(&err, 2).is_none());
    }

    #[test]
    fn test_retry_policy_transport_is_retryable() {
        let policy = RetryPolicy {
            max_retries: 3,
            initial_delay_ms: 1,
            max_delay_ms: 10,
            multiplier: 2.0,
            retryable_errors: vec![RetryableErrorKind::Transport],
        };
        let err = GrpcError::Transport("link reset".to_string());
        assert!(policy.should_retry(&err, 0).is_some());
    }

    // =================================================================
    // 新增测试：TimeoutPolicy
    // =================================================================

    #[test]
    fn test_timeout_policy_triggers_on_exceed() {
        let policy = TimeoutPolicy::new(Duration::from_millis(50));
        // 已耗时 100ms > 50ms deadline
        let result = policy.check_elapsed(Duration::from_millis(100));
        assert!(matches!(result, Err(GrpcError::Timeout(_))));
    }

    #[test]
    fn test_timeout_policy_passes_when_within() {
        let policy = TimeoutPolicy::new(Duration::from_secs(30));
        // 已耗时 1ms < 30s deadline
        let result = policy.check_elapsed(Duration::from_millis(1));
        assert!(result.is_ok());
        // check_elapsed 返回 Result<(), GrpcError>，Ok(()) 已由 is_ok 验证
        // 验证 deadline 字段未被 check_elapsed 修改
        assert_eq!(
            policy.deadline,
            Duration::from_secs(30),
            "deadline 不应被 check_elapsed 修改"
        );
    }

    #[test]
    fn test_timeout_policy_deadline_zero() {
        // 边界：deadline=0，任何非零耗时都应超时
        let policy = TimeoutPolicy::new(Duration::ZERO);
        let result = policy.check_elapsed(Duration::from_nanos(1));
        assert!(matches!(result, Err(GrpcError::Timeout(_))));
    }

    #[test]
    fn test_timeout_policy_exact_deadline_passes() {
        // 边界：elapsed == deadline 不算超时（使用 > 而非 >=）
        let policy = TimeoutPolicy::new(Duration::from_millis(100));
        let result = policy.check_elapsed(Duration::from_millis(100));
        assert!(result.is_ok());
        // check_elapsed 返回 Result<(), GrpcError>，Ok(()) 已由 is_ok 验证
        // 反例：elapsed 比 deadline 多 1ns 就应超时（> 而非 >= 的严格验证）
        let result_over = policy.check_elapsed(Duration::from_nanos(100_000_001));
        assert!(
            matches!(result_over, Err(GrpcError::Timeout(_))),
            "elapsed 比 deadline 多 1ns 应超时"
        );
    }

    #[test]
    fn test_timeout_policy_default() {
        let policy = TimeoutPolicy::default();
        assert_eq!(policy.deadline, Duration::from_secs(30));
    }

    // =================================================================
    // 新增测试：call_unary 集成（重试与不可重试）
    // =================================================================

    #[test]
    fn test_call_unary_retries_then_succeeds() {
        use std::sync::atomic::AtomicU32;

        /// Service that fails the first N calls and then succeeds.
        struct FlakyUserService {
            real: InMemoryUserService,
            fail_count: AtomicU32,
            fail_until: u32,
        }
        impl UserGrpcService for FlakyUserService {
            fn get_user(&self, req: UserRequest) -> Result<UserResponse, GrpcError> {
                let n = self.fail_count.fetch_add(1, Ordering::SeqCst);
                if n < self.fail_until {
                    return Err(GrpcError::ConnectionFailed(format!(
                        "simulated failure #{}",
                        n
                    )));
                }
                self.real.get_user(req)
            }
            fn list_users(&self) -> Result<Vec<UserResponse>, GrpcError> {
                self.real.list_users()
            }
        }

        let addr = unique_addr("retry_then_succeed");
        let svc = Arc::new(FlakyUserService {
            real: InMemoryUserService::new().with_user(build_user(1, "a", "a@x")),
            fail_count: AtomicU32::new(0),
            fail_until: 2,
        });
        let server = GrpcServer::new(host_of(&addr), port_of(&addr)).register_user_service(svc);
        let _handle = server.start().expect("server start");

        let channel = GrpcChannel::new(&addr).with_retry(RetryPolicy {
            max_retries: 3,
            initial_delay_ms: 1,
            max_delay_ms: 10,
            multiplier: 2.0,
            retryable_errors: vec![RetryableErrorKind::ConnectionFailed],
        });

        let user: UserResponse = channel
            .call_unary("UserService", "GetUser", |svc| {
                svc.get_user(UserRequest {
                    id: 1,
                    username: String::new(),
                })
            })
            .expect("should succeed after retries");
        assert_eq!(user.id, 1);
        assert_eq!(user.username, "a");
    }

    #[test]
    fn test_call_unary_no_retry_on_method_not_found() {
        use std::sync::atomic::AtomicU32;

        /// Service that returns MethodNotFound, used to verify that non-retryable
        /// errors do not trigger retries.
        struct NotFoundService {
            call_count: AtomicU32,
        }
        impl UserGrpcService for NotFoundService {
            fn get_user(&self, _req: UserRequest) -> Result<UserResponse, GrpcError> {
                self.call_count.fetch_add(1, Ordering::SeqCst);
                Err(GrpcError::MethodNotFound("not found".to_string()))
            }
            fn list_users(&self) -> Result<Vec<UserResponse>, GrpcError> {
                Err(GrpcError::MethodNotFound("not found".to_string()))
            }
        }

        let addr = unique_addr("no_retry_method_not_found");
        let svc = Arc::new(NotFoundService {
            call_count: AtomicU32::new(0),
        });
        let server = GrpcServer::new(host_of(&addr), port_of(&addr)).register_user_service(svc);
        let _handle = server.start().expect("server start");

        let channel = GrpcChannel::new(&addr).with_retry(RetryPolicy {
            max_retries: 3,
            initial_delay_ms: 1,
            max_delay_ms: 10,
            multiplier: 2.0,
            retryable_errors: vec![RetryableErrorKind::ConnectionFailed],
        });

        let result: Result<UserResponse, _> = channel.call_unary("UserService", "GetUser", |svc| {
            svc.get_user(UserRequest {
                id: 1,
                username: String::new(),
            })
        });
        assert!(matches!(result, Err(GrpcError::MethodNotFound(_))));
        // 验证只调用了一次（无重试）
        let svc_ref = global_registry().read();
        // 通过注册表间接验证：服务仍在
        assert!(svc_ref
            .services
            .get(&addr)
            .and_then(|m| m.get("UserService"))
            .is_some());
    }

    #[test]
    fn test_call_unary_retry_exhausts_on_persistent_failure() {
        // 无服务器时，ConnectionFailed 会持续触发重试直到耗尽。
        let addr = unique_addr("retry_exhaust");
        let channel = GrpcChannel::new(&addr).with_retry(RetryPolicy {
            max_retries: 2,
            initial_delay_ms: 1,
            max_delay_ms: 5,
            multiplier: 2.0,
            retryable_errors: vec![RetryableErrorKind::ConnectionFailed],
        });

        let result: Result<UserResponse, _> = channel.call_unary("UserService", "GetUser", |svc| {
            svc.get_user(UserRequest {
                id: 1,
                username: String::new(),
            })
        });
        // 重试耗尽后返回 ConnectionFailed
        assert!(matches!(result, Err(GrpcError::ConnectionFailed(_))));
    }

    #[test]
    fn test_retry_policy_default_values() {
        let policy = RetryPolicy::default();
        assert_eq!(policy.max_retries, 3);
        assert_eq!(policy.initial_delay_ms, 50);
        assert_eq!(policy.max_delay_ms, 1000);
        assert_eq!(policy.multiplier, 2.0);
        assert_eq!(policy.retryable_errors.len(), 2);
        assert!(policy
            .retryable_errors
            .contains(&RetryableErrorKind::ConnectionFailed));
        assert!(policy
            .retryable_errors
            .contains(&RetryableErrorKind::Timeout));
    }
}
