use actix_web::http::StatusCode;
use actix_web::Responder;
use serde::Serialize;
use std::collections::HashMap;
use sz_orm_actix::{JsonResp, JsonRows, PoolState};
use sz_orm_core::Value;

fn make_test_request() -> actix_web::HttpRequest {
    actix_web::test::TestRequest::default().to_http_request()
}

#[test]
fn test_json_rows_empty() {
    let rows: Vec<HashMap<String, Value>> = vec![];
    let resp = JsonRows(rows).respond_to(&make_test_request());
    assert_eq!(resp.status(), StatusCode::OK);
}

#[test]
fn test_json_rows_single() {
    let mut row = HashMap::new();
    row.insert("id".to_string(), Value::I64(1));
    row.insert("name".to_string(), Value::String("Alice".to_string()));
    let resp = JsonRows(vec![row]).respond_to(&make_test_request());
    assert_eq!(resp.status(), StatusCode::OK);
}

#[test]
fn test_json_rows_multiple() {
    let mut row1 = HashMap::new();
    row1.insert("id".to_string(), Value::I64(1));
    let mut row2 = HashMap::new();
    row2.insert("id".to_string(), Value::I64(2));
    let resp = JsonRows(vec![row1, row2]).respond_to(&make_test_request());
    assert_eq!(resp.status(), StatusCode::OK);
}

#[test]
fn test_json_rows_with_all_int_types() {
    let mut row = HashMap::new();
    row.insert("i8".to_string(), Value::I8(1));
    row.insert("i16".to_string(), Value::I16(2));
    row.insert("i32".to_string(), Value::I32(3));
    row.insert("i64".to_string(), Value::I64(4));
    row.insert("u8".to_string(), Value::U8(5));
    row.insert("u16".to_string(), Value::U16(6));
    row.insert("u32".to_string(), Value::U32(7));
    row.insert("u64".to_string(), Value::U64(8));
    let resp = JsonRows(vec![row]).respond_to(&make_test_request());
    assert_eq!(resp.status(), StatusCode::OK);
}

#[test]
fn test_json_rows_with_float_types() {
    let mut row = HashMap::new();
    row.insert("f32".to_string(), Value::F32(1.5));
    row.insert("f64".to_string(), Value::F64(2.5));
    let resp = JsonRows(vec![row]).respond_to(&make_test_request());
    assert_eq!(resp.status(), StatusCode::OK);
}

#[test]
fn test_json_rows_with_bytes_as_hex() {
    let mut row = HashMap::new();
    row.insert("data".to_string(), Value::Bytes(vec![0x1a, 0x2b, 0x3c]));
    let resp = JsonRows(vec![row]).respond_to(&make_test_request());
    assert_eq!(resp.status(), StatusCode::OK);
}

#[test]
fn test_json_rows_with_decimal_and_dates() {
    let mut row = HashMap::new();
    row.insert("amount".to_string(), Value::Decimal("123.45".to_string()));
    row.insert("created".to_string(), Value::Date("2024-01-01".to_string()));
    row.insert(
        "updated".to_string(),
        Value::DateTime("2024-01-01T00:00:00".to_string()),
    );
    row.insert("time".to_string(), Value::Time("12:30:00".to_string()));
    let resp = JsonRows(vec![row]).respond_to(&make_test_request());
    assert_eq!(resp.status(), StatusCode::OK);
}

#[test]
fn test_json_rows_with_uuid() {
    let mut row = HashMap::new();
    row.insert(
        "id".to_string(),
        Value::Uuid("550e8400-e29b-41d4-a716-446655440000".to_string()),
    );
    let resp = JsonRows(vec![row]).respond_to(&make_test_request());
    assert_eq!(resp.status(), StatusCode::OK);
}

#[test]
fn test_json_rows_with_json_value() {
    let mut row = HashMap::new();
    row.insert(
        "meta".to_string(),
        Value::Json("{\"key\":\"value\"}".to_string()),
    );
    let resp = JsonRows(vec![row]).respond_to(&make_test_request());
    assert_eq!(resp.status(), StatusCode::OK);
}

#[test]
fn test_json_rows_with_array() {
    let mut row = HashMap::new();
    row.insert(
        "tags".to_string(),
        Value::Array(vec![Value::String("a".into()), Value::String("b".into())]),
    );
    let resp = JsonRows(vec![row]).respond_to(&make_test_request());
    assert_eq!(resp.status(), StatusCode::OK);
}

#[test]
fn test_json_rows_with_object() {
    let mut row = HashMap::new();
    let mut inner = std::collections::HashMap::new();
    inner.insert("key".to_string(), Value::String("value".to_string()));
    row.insert("meta".to_string(), Value::Object(inner));
    let resp = JsonRows(vec![row]).respond_to(&make_test_request());
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
    let resp = JsonResp(user).respond_to(&make_test_request());
    assert_eq!(resp.status(), StatusCode::OK);
}

#[test]
fn test_json_resp_vec() {
    #[derive(Serialize)]
    struct Item {
        id: i64,
    }
    let items = vec![Item { id: 1 }, Item { id: 2 }];
    let resp = JsonResp(items).respond_to(&make_test_request());
    assert_eq!(resp.status(), StatusCode::OK);
}

#[test]
fn test_json_resp_string() {
    let resp = JsonResp("hello".to_string()).respond_to(&make_test_request());
    assert_eq!(resp.status(), StatusCode::OK);
}

#[test]
fn test_json_resp_i64() {
    let resp = JsonResp(42i64).respond_to(&make_test_request());
    assert_eq!(resp.status(), StatusCode::OK);
}

#[test]
fn test_pool_state_clone_trait() {
    fn _assert_clone<T: Clone>() {}
    _assert_clone::<PoolState>();
}
