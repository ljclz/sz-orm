//! 真实 tonic gRPC 服务器/客户端实现。
//!
//! 本模块仅在启用 `real` feature 时编译。提供 [`RealGrpcServer`] 和
//! [`RealGrpcClient`]，与内存版 API 形状一致，但通过真实 TCP 交换 protobuf。
//! 业务后端复用 [`crate::InMemoryUserService`]，因此同一份
//! [`crate::UserGrpcService`] trait 实现可被内存版与真实版无感切换使用。

use crate::{GrpcError, InMemoryUserService, UserGrpcService, UserRequest, UserResponse};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::task::JoinHandle;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::Server;

// 引入 build.rs 中 tonic_prost_build 生成的代码。
// 文件位于 `OUT_DIR/szorm.grpc.rs`（按 proto package 命名）。
// 包进 `proto` 子模块避免与 crate::UserRequest/UserResponse 同名冲突。
mod proto {
    tonic::include_proto!("szorm.grpc");
}

use proto::{
    user_service_client::UserServiceClient,
    user_service_server::{UserService, UserServiceServer},
    Empty as ProtoEmpty, UserListResponse, UserRequest as ProtoUserRequest,
    UserResponse as ProtoUserResponse,
};

/// 把 [`InMemoryUserService`] 适配为 tonic 生成的 [`UserService`] trait 实现。
///
/// 这是一个内部类型，外部代码不应直接使用。它只是把 `UserGrpcService` trait
/// 的同步方法桥接到 tonic 的异步 RPC 接口。
struct UserServiceImpl {
    backend: Arc<InMemoryUserService>,
}

impl UserServiceImpl {
    fn new(backend: Arc<InMemoryUserService>) -> Self {
        Self { backend }
    }
}

#[tonic::async_trait]
impl UserService for UserServiceImpl {
    async fn get_user(
        &self,
        request: tonic::Request<ProtoUserRequest>,
    ) -> Result<tonic::Response<ProtoUserResponse>, tonic::Status> {
        let req = request.into_inner();
        // 复用同一份 trait 实现，保证业务逻辑与内存版完全一致。
        let result = self.backend.get_user(UserRequest {
            id: req.id,
            username: req.username,
        });
        match result {
            Ok(user) => Ok(tonic::Response::new(ProtoUserResponse {
                id: user.id,
                username: user.username,
                email: user.email,
            })),
            // 用户未找到映射到 gRPC NotFound，客户端可据此识别。
            Err(GrpcError::MethodNotFound(msg)) => Err(tonic::Status::not_found(msg)),
            Err(e) => Err(tonic::Status::internal(e.to_string())),
        }
    }

    async fn list_users(
        &self,
        _request: tonic::Request<ProtoEmpty>,
    ) -> Result<tonic::Response<UserListResponse>, tonic::Status> {
        match self.backend.list_users() {
            Ok(users) => {
                let users_proto = users
                    .into_iter()
                    .map(|u| ProtoUserResponse {
                        id: u.id,
                        username: u.username,
                        email: u.email,
                    })
                    .collect();
                Ok(tonic::Response::new(UserListResponse {
                    users: users_proto,
                }))
            }
            Err(e) => Err(tonic::Status::internal(e.to_string())),
        }
    }
}

/// 真实 tonic gRPC 服务器，承载 [`InMemoryUserService`] 作为后端。
///
/// 构造后通过 [`RealGrpcServer::start`] 绑定到 TCP 端口并启动后台 task。
/// 返回的 [`RealGrpcServerHandle`] 可用于查询监听地址与停止服务。
pub struct RealGrpcServer {
    backend: Arc<InMemoryUserService>,
}

impl RealGrpcServer {
    /// 构造一个空的真实服务器，后端为空的 [`InMemoryUserService`]。
    pub fn new() -> Self {
        Self {
            backend: Arc::new(InMemoryUserService::new()),
        }
    }

    /// 用已有后端构造真实服务器，便于在多个客户端间共享同一份数据。
    pub fn with_backend(backend: Arc<InMemoryUserService>) -> Self {
        Self { backend }
    }

    /// 获取后端引用，方便测试预置数据。
    pub fn backend(&self) -> &Arc<InMemoryUserService> {
        &self.backend
    }

    /// 绑定到 `addr` 并在独立 tokio task 中启动 tonic 服务器。
    ///
    /// `addr` 中端口为 `0` 时由操作系统随机分配，可通过
    /// [`RealGrpcServerHandle::local_addr`] 查询实际端口。
    /// 必须在 tokio 运行时上下文中调用。
    pub async fn start(self, addr: SocketAddr) -> Result<RealGrpcServerHandle, GrpcError> {
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| GrpcError::ConnectionFailed(format!("bind {addr} failed: {e}")))?;
        let local_addr = listener
            .local_addr()
            .map_err(|e| GrpcError::ConnectionFailed(format!("local_addr: {e}")))?;
        // tonic 0.14 的 TcpIncoming::from_listener 已移除，改用
        // tokio_stream::wrappers::TcpListenerStream 将 tokio TcpListener 转为
        // tonic 可接受的 incoming Stream。
        let incoming = TcpListenerStream::new(listener);

        // 用 oneshot 通道作为优雅停机信号：handle 持有 sender，drop 或
        // 显式 send 都会让 server future 退出。
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
        let serve = Server::builder()
            .add_service(UserServiceServer::new(UserServiceImpl::new(self.backend)))
            .serve_with_incoming_shutdown(incoming, async move {
                let _ = shutdown_rx.await;
            });
        let join = tokio::spawn(serve);

        Ok(RealGrpcServerHandle {
            local_addr,
            shutdown_tx: Some(shutdown_tx),
            join: Some(join),
        })
    }
}

impl Default for RealGrpcServer {
    fn default() -> Self {
        Self::new()
    }
}

/// 真实 tonic 服务器的句柄，可查询监听地址并优雅停止后台 task。
pub struct RealGrpcServerHandle {
    local_addr: SocketAddr,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
    join: Option<JoinHandle<Result<(), tonic::transport::Error>>>,
}

impl RealGrpcServerHandle {
    /// 返回服务器实际监听的本地地址。
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// 优雅停止服务器：发送 shutdown 信号并等待后台 task 结束。
    /// 重复调用是幂等的。
    pub async fn stop(&mut self) -> Result<(), GrpcError> {
        if let Some(tx) = self.shutdown_tx.take() {
            // 接收方被 drop 时 send 返回 Err，这里忽略。
            let _ = tx.send(());
        }
        if let Some(join) = self.join.take() {
            // 等待 task 退出；abort 或正常完成都会落到这里。
            let _ = join.await;
        }
        Ok(())
    }
}

impl Drop for RealGrpcServerHandle {
    fn drop(&mut self) {
        // 兜底：测试中即使忘记调用 stop()，drop 时也尝试触发 shutdown，
        // 避免后台 task 泄漏占用端口。
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(join) = self.join.take() {
            join.abort();
        }
    }
}

/// 真实 tonic gRPC 客户端，通过 TCP 调用 [`RealGrpcServer`]。
pub struct RealGrpcClient {
    inner: UserServiceClient<tonic::transport::Channel>,
}

impl RealGrpcClient {
    /// 连接到 `addr`（例如 `127.0.0.1:50051`），返回可发起 RPC 的客户端。
    /// 地址字符串不应包含 scheme，函数内部会自动拼接 `http://` 前缀。
    pub async fn connect(addr: impl Into<String>) -> Result<Self, GrpcError> {
        let addr_str = addr.into();
        if addr_str.is_empty() {
            return Err(GrpcError::ConnectionFailed(
                "Address cannot be empty".to_string(),
            ));
        }
        let endpoint = format!("http://{}", addr_str);
        let client = UserServiceClient::connect(endpoint)
            .await
            .map_err(|e| GrpcError::ConnectionFailed(format!("connect {addr_str} failed: {e}")))?;
        Ok(Self { inner: client })
    }

    /// 调用 `GetUser` RPC，按 id 查询用户。未找到时返回
    /// [`GrpcError::MethodNotFound`]，其他 RPC 错误返回
    /// [`GrpcError::Transport`]。
    pub async fn get_user(&mut self, id: i64) -> Result<UserResponse, GrpcError> {
        let req = ProtoUserRequest {
            id,
            username: String::new(),
        };
        let resp = self
            .inner
            .get_user(tonic::Request::new(req))
            .await
            .map_err(|e| {
                if e.code() == tonic::Code::NotFound {
                    GrpcError::MethodNotFound(e.message().to_string())
                } else {
                    GrpcError::Transport(e.to_string())
                }
            })?;
        let u = resp.into_inner();
        Ok(UserResponse {
            id: u.id,
            username: u.username,
            email: u.email,
        })
    }

    /// 调用 `ListUsers` RPC，返回后端全量用户列表（按 id 升序）。
    pub async fn list_users(&mut self) -> Result<Vec<UserResponse>, GrpcError> {
        let resp = self
            .inner
            .list_users(tonic::Request::new(ProtoEmpty {}))
            .await
            .map_err(|e| GrpcError::Transport(e.to_string()))?;
        let users = resp.into_inner().users;
        Ok(users
            .into_iter()
            .map(|u| UserResponse {
                id: u.id,
                username: u.username,
                email: u.email,
            })
            .collect())
    }
}
