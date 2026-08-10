use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::Serialize;
use std::collections::HashMap;
use sz_orm_axum::{JsonResp, JsonRows};
use sz_orm_core::Value;

#[test]
fn test_json_rows_empty() {
    let rows: Vec<HashMap<String, Value>> = vec![];
    let resp = JsonRows(rows).into_response();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[test]
fn test_json_rows_single() {
    let mut row = HashMap::new();
    row.insert("id".to_string(), Value::I64(1));
    row.insert("name".to_string(), Value::String("Alice".to_string()));
    let resp = JsonRows(vec![row]).into_response();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[test]
fn test_json_rows_multiple() {
    let mut row1 = HashMap::new();
    row1.insert("id".to_string(), Value::I64(1));
    let mut row2 = HashMap::new();
    row2.insert("id".to_string(), Value::I64(2));
    let resp = JsonRows(vec![row1, row2]).into_response();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[test]
fn test_json_rows_with_various_types() {
    let mut row = HashMap::new();
    row.insert("id".to_string(), Value::I64(42));
    row.insert("active".to_string(), Value::Bool(true));
    row.insert("score".to_string(), Value::F64(2.5));
    row.insert("name".to_string(), Value::String("test".to_string()));
    row.insert("data".to_string(), Value::Null);
    let resp = JsonRows(vec![row]).into_response();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[test]
fn test_json_resp_simple_struct() {
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
fn test_json_resp_vec() {
    #[derive(Serialize)]
    struct Item {
        id: i64,
    }
    let items = vec![Item { id: 1 }, Item { id: 2 }, Item { id: 3 }];
    let resp = JsonResp(items).into_response();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[test]
fn test_json_resp_nested_struct() {
    #[derive(Serialize)]
    struct Address {
        city: String,
        zip: String,
    }
    #[derive(Serialize)]
    struct User {
        id: i64,
        address: Address,
    }
    let user = User {
        id: 1,
        address: Address {
            city: "Beijing".to_string(),
            zip: "100000".to_string(),
        },
    };
    let resp = JsonResp(user).into_response();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[test]
fn test_json_resp_string() {
    let resp = JsonResp("hello world".to_string()).into_response();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[test]
fn test_json_resp_i64() {
    let resp = JsonResp(42i64).into_response();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[test]
fn test_json_resp_option_none() {
    let val: Option<i64> = None;
    let resp = JsonResp(val).into_response();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[test]
fn test_json_resp_option_some() {
    let val: Option<i64> = Some(99);
    let resp = JsonResp(val).into_response();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[test]
fn test_pool_state_clone_trait() {
    fn _assert_clone<T: Clone>() {}
    _assert_clone::<sz_orm_axum::PoolState>();
}

#[test]
fn test_pool_into_state_returns_arc() {
    fn _assert_arc<T>() {}
    _assert_arc::<std::sync::Arc<sz_orm_core::Pool>>();
}
