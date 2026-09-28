//! T20: Oracle 连接串解析纯单元测试（4 tests）

use sz_orm_oracle::parse_connect_string;

#[test]
fn test_parse_connect_string_full_format() {
    let info = parse_connect_string("127.0.0.1:1521/freepdb1.FALSE").unwrap();
    assert_eq!(info.host(), "127.0.0.1");
    assert_eq!(info.port(), 1521);
    assert_eq!(info.service_name(), "freepdb1.FALSE");
    assert_eq!(info.username(), "");
    assert_eq!(info.password(), "");
}

#[test]
fn test_parse_connect_string_default_port() {
    let info = parse_connect_string("localhost/orcl").unwrap();
    assert_eq!(info.host(), "localhost");
    assert_eq!(info.port(), 1521);
    assert_eq!(info.service_name(), "orcl");
}

#[test]
fn test_parse_connect_string_missing_service_error() {
    let err = parse_connect_string("127.0.0.1:1521").unwrap_err();
    assert!(err.to_string().contains("service_name"));
}

#[test]
fn test_parse_connect_string_as_connect_string_roundtrip() {
    let info = parse_connect_string("dbhost:1522/svc").unwrap();
    assert_eq!(info.as_connect_string(), "dbhost:1522/svc");
}