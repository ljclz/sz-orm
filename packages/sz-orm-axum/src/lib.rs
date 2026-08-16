//! # SZ-ORM 的 axum 框架集成
//!
//! 提供：
//! - [`PoolState`] — 连接池的 axum State 包装
//! - [`JsonRows`] — 包装 `QueryRows` 实现 `IntoResponse`
//! - [`JsonResp<T>`] — 通用 JSON 响应包装
//! - [`transaction_layer`] — 事务中间件（请求成功提交，失败回滚）

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

/// 连接池的 axum State 包装
///
/// `Pool` 内部使用 `Arc` 共享，`PoolState` 提供轻量级 `Clone`，
/// 便于在 `Router::with_state` 中使用。
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

    /// 从 Arc<Pool> 创建（避免重复 Arc 包装）
    pub fn from_arc(pool: Arc<Pool>) -> Self {
        Self { pool }
    }

    /// 获取连接池引用
    pub fn pool(&self) -> &Pool {
        &self.pool
    }
}

/// 将 Pool 直接转为 `Arc<Pool>`（便于直接用作 axum State）
pub fn pool_into_state(pool: Pool) -> Arc<Pool> {
    Arc::new(pool)
}

// ============================================================================
// JsonRows — 查询结果的 JSON 响应
// ============================================================================

/// 包装 `QueryRows` 实现 `IntoResponse`
///
/// `QueryRows = Vec<HashMap<String, Value>>`，由于 `Value` 实现了 `Serialize`，
/// 直接序列化为 JSON 数组。
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

/// 通用 JSON 响应包装
///
/// 对任何实现了 `Serialize` 的类型提供 `IntoResponse` 实现。
pub struct JsonResp<T: Serialize>(pub T);

impl<T: Serialize> IntoResponse for JsonResp<T> {
    fn into_response(self) -> Response {
        Json(self.0).into_response()
    }
}

// ============================================================================
// TransactionLayer — 事务中间件
// ============================================================================

/// 事务中间件
///
/// 在请求处理前从连接池获取连接并开启事务；
/// 请求处理完成后：
/// - 响应状态为 2xx → 提交事务
/// - 响应状态非 2xx → 回滚事务
///
/// # 用法
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
