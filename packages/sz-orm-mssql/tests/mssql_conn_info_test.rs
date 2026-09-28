//! T24: MSSQL 连接串解析纯单元测试（3 tests）

use sz_orm_mssql::parse_conn_str;

#[test]
fn test_parse_conn_str_full_format() {
    let info = parse_conn_str(
        "server=127.0.0.1;port=1433;database=testdb;user=sa;password=pwd123",
    )
    .unwrap();
    assert_eq!(info.server(), "127.0.0.1");
    assert_eq!(info.port(), 1433);
    assert_eq!(info.database(), "testdb");
    assert_eq!(info.user(), "sa");
    assert_eq!(info.password(), "pwd123");
    assert_eq!(
        info.as_dsn(),
        "server=127.0.0.1;port=1433;database=testdb;user=sa;password=pwd123"
    );
}

#[test]
fn test_parse_conn_str_aliases_and_default_port() {
    let info =
        parse_conn_str("server=dbhost;database=mydb;uid=alice;pwd=secret").unwrap();
    assert_eq!(info.server(), "dbhost");
    assert_eq!(info.port(), 1433);
    assert_eq!(info.database(), "mydb");
    assert_eq!(info.user(), "alice");
    assert_eq!(info.password(), "secret");
}

#[test]
fn test_parse_conn_str_missing_server_error() {
    let err = parse_conn_str("database=mydb;user=sa;password=pwd").unwrap_err();
    assert!(err.to_string().contains("server"));
}