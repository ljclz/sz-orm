//! # SZ-ORM 的 actix-web 框架集成
//!
//! 提供：
//! - [`PoolState`] — 连接池的 actix-web 应用数据包装（实现 `FromRequest`）
//! - [`JsonRows`] — 包装 `QueryRows` 实现 `Responder`
//! - [`JsonResp<T>`] — 通用 JSON 响应包装
//! - [`TransactionMiddleware`] — 事务中间件骨架（请求成功提交，失败回滚）
//!
//! 由于 Rust 孤儿规则，无法直接为 `Arc<Pool>` 或 `QueryRows` 实现
//! actix-web 的 `FromRequest`/`Responder`，因此使用 `PoolState` / `JsonRows`
//! 包装类型（与 sz-orm-axum 风格保持一致）。

use actix_web::{
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    web, FromRequest, HttpRequest, HttpResponse, Responder,
};
use serde::Serialize;
use std::{
    future::{ready, Ready},
    sync::Arc,
};
use sz_orm_core::{Pool, QueryRows, Value};

// ============================================================================
// PoolState — 连接池的 actix-web 应用数据包装
// ============================================================================

/// 连接池的 actix-web 应用数据包装
///
/// `Pool` 内部使用 `Arc` 共享，`PoolState` 提供轻量级 `Clone`，
/// 便于在 `App::app_data` 中注册，并实现 `FromRequest` 以便在 handler 中提取。
#[derive(Clone)]
pub struct PoolState {
    pool: Arc<Pool>,
}

impl PoolState {
    /// 创建 PoolState
    pub fn new(pool: Pool) -> Self {
        Self {
            pool: Arc::new(pool),
        }
    }

    /// 从 `Arc<Pool>` 创建（避免重复 Arc 包装）
    pub fn from_arc(pool: Arc<Pool>) -> Self {
        Self { pool }
    }

    /// 获取连接池引用
    pub fn pool(&self) -> &Pool {
        &self.pool
    }
}

impl FromRequest for PoolState {
    type Error = actix_web::Error;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _: &mut actix_web::dev::Payload) -> Self::Future {
        // 优先从 app_data 中提取 PoolState
        // 注意：`web::Data<T>` deref 到 `Arc<T>`，因此 `**state` 是 `Arc<PoolState>`
        // 而非 `PoolState`。使用 `get_ref()` 直接获取 `&T` 后再 clone 更清晰。
        if let Some(state) = req.app_data::<web::Data<PoolState>>() {
            return ready(Ok(state.get_ref().clone()));
        }
        // 兼容直接注册 Arc<Pool> 的场景
        if let Some(pool) = req.app_data::<web::Data<Arc<Pool>>>() {
            return ready(Ok(PoolState::from_arc(pool.get_ref().clone())));
        }
        ready(Err(actix_web::error::ErrorInternalServerError(
            "PoolState not found in app data",
        )))
    }
}

// ============================================================================
// JsonRows — 查询结果的 JSON 响应
// ============================================================================

/// 包装 `QueryRows` 实现 `Responder`
///
/// `QueryRows = Vec<HashMap<String, Value>>`，逐字段转换为 JSON 对象数组。
pub struct JsonRows(pub QueryRows);

impl Responder for JsonRows {
    type Body = actix_web::body::BoxBody;

    fn respond_to(self, _: &HttpRequest) -> HttpResponse {
        let json: Vec<serde_json::Value> = self
            .0
            .iter()
            .map(|row| {
                let mut map = serde_json::Map::new();
                for (k, v) in row {
                    map.insert(k.clone(), value_to_json(v));
                }
                serde_json::Value::Object(map)
            })
            .collect();
        HttpResponse::Ok().json(json)
    }
}

/// `Value` 转换为 `serde_json::Value`
///
/// 手动映射以使 `Bytes` 输出十六进制字符串、`Decimal`/`Date` 等保留为字符串，
/// 避免默认序列化把 `Vec<u8>` 展开为数字数组。
fn value_to_json(v: &Value) -> serde_json::Value {
    match v {
        Value::Null => serde_json::Value::Null,
        Value::Bool(b) => serde_json::Value::Bool(*b),
        Value::I8(n) => (*n).into(),
        Value::I16(n) => (*n).into(),
        Value::I32(n) => (*n).into(),
        Value::I64(n) => (*n).into(),
        Value::U8(n) => (*n).into(),
        Value::U16(n) => (*n).into(),
        Value::U32(n) => (*n).into(),
        Value::U64(n) => (*n).into(),
        Value::F32(f) => serde_json::Value::from(*f),
        Value::F64(f) => serde_json::Value::from(*f),
        Value::String(s) => serde_json::Value::String(s.clone()),
        Value::Decimal(s) => serde_json::Value::String(s.clone()),
        // 字节序列以十六进制字符串表示，避免默认序列化为数字数组
        Value::Bytes(b) => serde_json::Value::String(
            b.iter().map(|byte| format!("{:02x}", byte)).collect(),
        ),
        Value::Date(s) | Value::DateTime(s) | Value::Time(s) => {
            serde_json::Value::String(s.clone())
        }
        Value::Json(s) => {
            serde_json::from_str(s).unwrap_or_else(|_| serde_json::Value::String(s.clone()))
        }
        Value::Uuid(s) => serde_json::Value::String(s.clone()),
        Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(value_to_json).collect())
        }
        Value::Object(map) => {
            let mut obj = serde_json::Map::new();
            for (k, v) in map {
                obj.insert(k.clone(), value_to_json(v));
            }
            serde_json::Value::Object(obj)
        }
        // Value 标记为 #[non_exhaustive]，预留兜底
        _ => serde_json::Value::Null,
    }
}

// ============================================================================
// JsonResp<T> — 通用 JSON 响应包装
// ============================================================================

/// 通用 JSON 响应包装
///
/// 对任何实现了 `Serialize` 的类型提供 `Responder` 实现。
pub struct JsonResp<T: Serialize>(pub T);

impl<T: Serialize> Responder for JsonResp<T> {
    type Body = actix_web::body::BoxBody;

    fn respond_to(self, _: &HttpRequest) -> HttpResponse {
        match serde_json::to_value(&self.0) {
            Ok(v) => HttpResponse::Ok().json(v),
            Err(e) => HttpResponse::InternalServerError()
                .body(format!("JSON 序列化失败: {}", e)),
        }
    }
}

// ============================================================================
// TransactionMiddleware — 事务中间件骨架
// ============================================================================

/// 事务中间件骨架
///
/// 预留的事务包裹结构。当前实现仅透传请求；后续可通过 `ServiceRequest::app_data`
/// 获取 `PoolState`，在请求前 acquire 连接并 `begin_transaction`，请求后
/// 根据 `ServiceResponse` 状态码 2xx 提交 / 否则回滚。
///
/// # 用法
///
/// ```ignore
/// use actix_web::{web, App};
/// use sz_orm_actix::{PoolState, TransactionMiddleware};
///
/// let app = App::new()
///     .app_data(web::Data::new(PoolState::new(pool)))
///     .wrap(TransactionMiddleware);
/// ```
pub struct TransactionMiddleware;

impl<S, B> Transform<S, ServiceRequest> for TransactionMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = actix_web::Error;
    type Transform = TransactionMiddlewareService<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(TransactionMiddlewareService { service }))
    }
}

pub struct TransactionMiddlewareService<S> {
    service: S,
}

impl<S, B> Service<ServiceRequest> for TransactionMiddlewareService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = actix_web::Error;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>>>,
    >;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let fut = self.service.call(req);
        Box::pin(async move {
            // TODO: 在此包裹事务逻辑
            // 1. 从 app_data 获取 PoolState
            // 2. acquire 连接并 begin_transaction
            // 3. 将连接注入 request extensions 供 handler 复用
            // 4. 根据响应状态码 commit / rollback
            let res = fut.await?;
            Ok(res)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_pool_state_clone() {
        fn _assert_clone<T: Clone>() {}
        _assert_clone::<PoolState>();
    }

    #[test]
    fn test_value_to_json_variants() {
        assert_eq!(value_to_json(&Value::Null), serde_json::Value::Null);
        assert_eq!(
            value_to_json(&Value::Bool(true)),
            serde_json::Value::Bool(true)
        );
        assert_eq!(value_to_json(&Value::I64(42)), serde_json::json!(42));
        assert_eq!(
            value_to_json(&Value::String("hi".into())),
            serde_json::json!("hi")
        );
        assert_eq!(
            value_to_json(&Value::Bytes(vec![0x1a, 0x2b])),
            serde_json::json!("1a2b")
        );
        // JSON 字符串解析为对象/数组
        assert_eq!(
            value_to_json(&Value::Json("{\"k\":1}".into())),
            serde_json::json!({"k": 1})
        );
    }

    #[test]
    fn test_json_rows_responder() {
        let mut row = HashMap::new();
        row.insert("id".to_string(), Value::I64(1));
        row.insert("name".to_string(), Value::String("Alice".into()));
        let rows: QueryRows = vec![row];
        // actix-web 4 的 `HttpRequest` 不再实现 `Default`，
        // 通过 `TestRequest::default().to_http_request()` 构造测试请求。
        let req = actix_web::test::TestRequest::default().to_http_request();
        let resp = JsonRows(rows).respond_to(&req);
        assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
    }

    #[test]
    fn test_json_resp_responder() {
        #[derive(Serialize)]
        struct User {
            id: i64,
            name: String,
        }
        let user = User {
            id: 1,
            name: "Bob".into(),
        };
        let req = actix_web::test::TestRequest::default().to_http_request();
        let resp = JsonResp(user).respond_to(&req);
        assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
    }
}
