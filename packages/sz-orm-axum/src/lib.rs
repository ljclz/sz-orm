//! # SZ-ORM axum Framework Integration
//!
//! Provides:
//! - [`PoolState`] — axum State wrapper for connection pool
//! - [`JsonRows`] — Wraps `QueryRows` implementing `IntoResponse`
//! - [`JsonResp<T>`] — Generic JSON response wrapper
//! - [`transaction_layer`] — Transaction middleware (commit on success, rollback on failure)

mod auth;
mod cors;
mod pagination;
mod response;
mod validation;

pub use auth::{AuthConfig, AuthResult, RoleChecker, TokenValidator};
pub use cors::{CorsConfig, CorsOrigin};
pub use pagination::{
    FilterCondition, FilterOp, FilterParams, Pagination, PaginationExtractor, QueryParams,
    SortDirection, SortParams,
};
pub use response::{
    ApiError, ApiResponse, ApiStatus, ErrorResponse, HealthCheck, HealthStatus, PaginatedResponse,
    PaginationMeta, ResponseWrapper,
};
pub use validation::{
    FieldValidator, RequestValidator, RuleType, ValidationError, ValidationResult, ValidationRule,
};

use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use std::sync::Arc;

use sz_orm_core::{Pool, QueryRows};

// ============================================================================
// PoolState — 连接池的 axum State 包装
// ============================================================================

/// axum State wrapper for connection pool
///
/// `Pool` uses `Arc` internally for sharing, `PoolState` provides lightweight `Clone`,
/// for easy use in `Router::with_state`.
#[derive(Clone)]
pub struct PoolState {
    pool: Arc<Pool>,
}

impl PoolState {
    /// Create PoolState
    pub fn new(pool: Pool) -> Self {
        Self {
            pool: Arc::new(pool),
        }
    }

    /// Create from Arc<Pool> (avoids duplicate Arc wrapping)
    pub fn from_arc(pool: Arc<Pool>) -> Self {
        Self { pool }
    }

    /// Get connection pool reference
    pub fn pool(&self) -> &Pool {
        &self.pool
    }
}

/// Convert Pool directly to `Arc<Pool>` (for direct use as axum State)
pub fn pool_into_state(pool: Pool) -> Arc<Pool> {
    Arc::new(pool)
}

// ============================================================================
// JsonRows — 查询结果的 JSON 响应
// ============================================================================

/// Wrap `QueryRows` implementing `IntoResponse`
///
/// `QueryRows = Vec<HashMap<String, Value>>`, since `Value` implements `Serialize`,
/// directly serialized as JSON array.
pub struct JsonRows(pub QueryRows);

impl IntoResponse for JsonRows {
    fn into_response(self) -> Response {
        match serde_json::to_value(&self.0) {
            Ok(v) => Json(v).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("QueryRows 序列化失败: {}", e),
            )
                .into_response(),
        }
    }
}

// ============================================================================
// JsonResp<T> — 通用 JSON 响应包装
// ============================================================================

/// Generic JSON response wrapper
///
/// Provides `IntoResponse` implementation for any type implementing `Serialize`.
pub struct JsonResp<T: Serialize>(pub T);

impl<T: Serialize> IntoResponse for JsonResp<T> {
    fn into_response(self) -> Response {
        Json(self.0).into_response()
    }
}

// ============================================================================
// TransactionLayer — 事务中间件
// ============================================================================

/// Transaction middleware
///
/// Acquires connection from pool and starts transaction before request processing;
/// After request processing:
/// - Response status 2xx → commit transaction
/// - Response status non-2xx → rollback transaction
///
/// # Usage
///
/// ```ignore
/// use axum::middleware::from_fn_with_state;
/// use sz_orm_axum::{PoolState, transaction_layer};
///
/// let app = Router::new()
///     .route("/", get(handler))
///     .layer(from_fn_with_state(state.clone(), transaction_layer))
///     .with_state(state);
/// ```
pub async fn transaction_layer(
    State(state): State<PoolState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    // 1. 获取连接
    let mut conn = match state.pool().acquire().await {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("获取连接失败: {}", e),
            )
                .into_response()
        }
    };

    // 2. 开启事务
    if let Err(e) = conn.begin_transaction().await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("开启事务失败: {}", e),
        )
            .into_response();
    }

    // 3. 执行请求
    let resp = next.run(req).await;

    // 4. 根据响应状态提交或回滚
    let tx_result = if resp.status().is_success() {
        conn.commit().await
    } else {
        conn.rollback().await
    };

    if let Err(e) = tx_result {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("事务结束失败: {}", e),
        )
            .into_response();
    }

    resp
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use sz_orm_core::Value;

    #[test]
    fn test_pool_state_clone() {
        fn _assert_clone<T: Clone>() {}
        _assert_clone::<PoolState>();
    }

    #[test]
    fn test_json_rows_into_response() {
        let mut row = HashMap::new();
        row.insert("id".to_string(), Value::I64(1));
        row.insert("name".to_string(), Value::String("Alice".to_string()));
        let rows: QueryRows = vec![row];
        let json_rows = JsonRows(rows);
        let resp = json_rows.into_response();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[test]
    fn test_json_resp_into_response() {
        #[derive(Serialize)]
        struct User {
            id: i64,
            name: String,
        }
        let user = User {
            id: 1,
            name: "Bob".to_string(),
        };
        let resp = JsonResp(user).into_response();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[test]
    fn test_json_rows_empty() {
        let rows: QueryRows = vec![];
        let resp = JsonRows(rows).into_response();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[test]
    fn test_json_rows_multiple_rows() {
        let mut row1 = HashMap::new();
        row1.insert("id".to_string(), Value::I64(1));
        row1.insert("name".to_string(), Value::String("Alice".to_string()));
        let mut row2 = HashMap::new();
        row2.insert("id".to_string(), Value::I64(2));
        row2.insert("name".to_string(), Value::String("Bob".to_string()));
        let rows: QueryRows = vec![row1, row2];
        let resp = JsonRows(rows).into_response();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[test]
    fn test_json_resp_with_vec() {
        let data = vec![1i64, 2, 3];
        let resp = JsonResp(data).into_response();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[test]
    fn test_json_resp_with_string() {
        let resp = JsonResp("hello".to_string()).into_response();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[test]
    fn test_json_resp_with_option_none() {
        let data: Option<i64> = None;
        let resp = JsonResp(data).into_response();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[test]
    fn test_json_resp_with_option_some() {
        let data: Option<i64> = Some(42);
        let resp = JsonResp(data).into_response();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[test]
    fn test_json_rows_with_null_value() {
        let mut row = HashMap::new();
        row.insert("id".to_string(), Value::Null);
        row.insert("name".to_string(), Value::String("test".to_string()));
        let rows: QueryRows = vec![row];
        let resp = JsonRows(rows).into_response();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[test]
    fn test_json_rows_with_various_types() {
        let mut row = HashMap::new();
        row.insert("i64".to_string(), Value::I64(42));
        row.insert("f64".to_string(), Value::F64(3.15));
        row.insert("bool".to_string(), Value::Bool(true));
        row.insert("str".to_string(), Value::String("hello".to_string()));
        row.insert("null".to_string(), Value::Null);
        let rows: QueryRows = vec![row];
        let resp = JsonRows(rows).into_response();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[test]
    fn test_pool_into_state_returns_arc() {
        // 测试 pool_into_state 函数签名正确（不实际创建 Pool，因为需要异步运行时）
        fn _assert<T: Send + Sync>() {}
        _assert::<Arc<Pool>>();
    }

    #[test]
    fn test_json_resp_with_nested_struct() {
        #[derive(Serialize)]
        struct Inner {
            value: i64,
        }
        #[derive(Serialize)]
        struct Outer {
            inner: Inner,
            name: String,
        }
        let data = Outer {
            inner: Inner { value: 99 },
            name: "nested".to_string(),
        };
        let resp = JsonResp(data).into_response();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[test]
    fn test_json_resp_with_empty_vec() {
        let data: Vec<i64> = vec![];
        let resp = JsonResp(data).into_response();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[test]
    fn test_json_rows_with_empty_row() {
        let row: HashMap<String, Value> = HashMap::new();
        let rows: QueryRows = vec![row];
        let resp = JsonRows(rows).into_response();
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
