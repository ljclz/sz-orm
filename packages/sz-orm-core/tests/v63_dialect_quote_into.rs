//! v6.3 Dialect::quote_into 等价性测试
//!
//! 验证 quote_into 写入 buf 的字节序列等于 quote 返回 String 内容。
//! 覆盖 4 dialect × 6 样本 = 24 断言。

use sz_orm_core::{MySqlDialect, OracleDialect, PostgreSqlDialect, SqliteDialect};

fn assert_quote_into_eq(dialect: &dyn sz_orm_core::Dialect, identifier: &str) {
    let expected = dialect.quote(identifier);
    let mut buf = String::new();
    dialect.quote_into(identifier, &mut buf);
    assert_eq!(
        buf, expected,
        "quote_into mismatch for identifier: {identifier:?}"
    );
}

#[test]
fn mysql_quote_into_equivalence() {
    let dialect = MySqlDialect;
    assert_quote_into_eq(&dialect, "");
    assert_quote_into_eq(&dialect, "users");
    assert_quote_into_eq(&dialect, "ta`ble");
    assert_quote_into_eq(&dialect, "ta\"ble");
    assert_quote_into_eq(&dialect, "ta`\"ble");
    assert_quote_into_eq(&dialect, &"a".repeat(256));
}

#[test]
fn postgresql_quote_into_equivalence() {
    let dialect = PostgreSqlDialect;
    assert_quote_into_eq(&dialect, "");
    assert_quote_into_eq(&dialect, "users");
    assert_quote_into_eq(&dialect, "ta`ble");
    assert_quote_into_eq(&dialect, "ta\"ble");
    assert_quote_into_eq(&dialect, "ta`\"ble");
    assert_quote_into_eq(&dialect, &"a".repeat(256));
}

#[test]
fn sqlite_quote_into_equivalence() {
    let dialect = SqliteDialect;
    assert_quote_into_eq(&dialect, "");
    assert_quote_into_eq(&dialect, "users");
    assert_quote_into_eq(&dialect, "ta`ble");
    assert_quote_into_eq(&dialect, "ta\"ble");
    assert_quote_into_eq(&dialect, "ta`\"ble");
    assert_quote_into_eq(&dialect, &"a".repeat(256));
}

#[test]
fn oracle_quote_into_equivalence() {
    let dialect = OracleDialect;
    assert_quote_into_eq(&dialect, "");
    assert_quote_into_eq(&dialect, "users");
    assert_quote_into_eq(&dialect, "ta`ble");
    assert_quote_into_eq(&dialect, "ta\"ble");
    assert_quote_into_eq(&dialect, "ta`\"ble");
    assert_quote_into_eq(&dialect, &"a".repeat(256));
}
