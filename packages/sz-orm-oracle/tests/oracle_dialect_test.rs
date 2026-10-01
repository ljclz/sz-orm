//! T21: Oracle 方言/类型/错误分类/PL-SQL 调用纯单元测试（18 tests）

use sz_orm_oracle::{OracleDataType, OracleDialect, OracleErrorCategory, PlSqlCall};

// --- OracleDialect (9 tests) ---

#[test]
fn test_dialect_new_default_equal() {
    let a = OracleDialect::new();
    let b = OracleDialect;
    assert_eq!(a.quote_identifier("x"), b.quote_identifier("x"));
}

#[test]
fn test_dialect_quote_identifier_plain() {
    let d = OracleDialect::new();
    assert_eq!(d.quote_identifier("col"), "\"col\"");
}

#[test]
fn test_dialect_quote_identifier_escape_embedded_quote() {
    let d = OracleDialect::new();
    assert_eq!(d.quote_identifier("a\"b"), "\"a\"\"b\"");
}

#[test]
fn test_dialect_limit_clause_limit_only() {
    let d = OracleDialect::new();
    assert_eq!(d.limit_clause(Some(10), None), "FETCH NEXT 10 ROWS ONLY");
}

#[test]
fn test_dialect_limit_clause_limit_and_offset() {
    let d = OracleDialect::new();
    assert_eq!(
        d.limit_clause(Some(10), Some(20)),
        "OFFSET 20 ROWS FETCH NEXT 10 ROWS ONLY"
    );
}

#[test]
fn test_dialect_limit_clause_offset_only() {
    let d = OracleDialect::new();
    assert_eq!(d.limit_clause(None, Some(5)), "OFFSET 5 ROWS");
}

#[test]
fn test_dialect_limit_clause_none() {
    let d = OracleDialect::new();
    assert_eq!(d.limit_clause(None, None), "");
}

#[test]
fn test_dialect_placeholder_sequential() {
    let d = OracleDialect::new();
    assert_eq!(d.placeholder(1), ":1");
    assert_eq!(d.placeholder(2), ":2");
}

#[test]
fn test_dialect_is_reserved_keyword_true_and_false() {
    let d = OracleDialect::new();
    assert!(d.is_reserved_keyword("SELECT"));
    assert!(d.is_reserved_keyword("select"));
    assert!(d.is_reserved_keyword("SYSDATE"));
    assert!(!d.is_reserved_keyword("MYCOL"));
    assert!(!d.is_reserved_keyword("username"));
}

// --- OracleDataType (7 tests) ---

#[test]
fn test_data_type_as_sql_type_representative() {
    assert_eq!(OracleDataType::Number.as_sql_type(), "NUMBER");
    assert_eq!(OracleDataType::Varchar2.as_sql_type(), "VARCHAR2");
    assert_eq!(
        OracleDataType::TimestampTz.as_sql_type(),
        "TIMESTAMP WITH TIME ZONE"
    );
    assert_eq!(OracleDataType::LongRaw.as_sql_type(), "LONG RAW");
    assert_eq!(OracleDataType::Null.as_sql_type(), "NULL");
}

#[test]
fn test_data_type_is_numeric() {
    assert!(OracleDataType::Number.is_numeric());
    assert!(OracleDataType::BinaryFloat.is_numeric());
    assert!(OracleDataType::BinaryDouble.is_numeric());
    assert!(!OracleDataType::Varchar2.is_numeric());
    assert!(!OracleDataType::Date.is_numeric());
}

#[test]
fn test_data_type_is_string() {
    assert!(OracleDataType::Varchar2.is_string());
    assert!(OracleDataType::Nvarchar2.is_string());
    assert!(OracleDataType::Char.is_string());
    assert!(OracleDataType::Long.is_string());
    assert!(OracleDataType::Xmltype.is_string());
    assert!(!OracleDataType::Clob.is_string());
    assert!(!OracleDataType::Number.is_string());
}

#[test]
fn test_data_type_is_binary() {
    assert!(OracleDataType::Raw.is_binary());
    assert!(OracleDataType::LongRaw.is_binary());
    assert!(OracleDataType::Blob.is_binary());
    assert!(!OracleDataType::Clob.is_binary());
}

#[test]
fn test_data_type_is_temporal() {
    assert!(OracleDataType::Date.is_temporal());
    assert!(OracleDataType::Timestamp.is_temporal());
    assert!(OracleDataType::TimestampTz.is_temporal());
    assert!(OracleDataType::TimestampLtz.is_temporal());
    assert!(!OracleDataType::Number.is_temporal());
}

#[test]
fn test_data_type_is_lob() {
    assert!(OracleDataType::Clob.is_lob());
    assert!(OracleDataType::Nclob.is_lob());
    assert!(OracleDataType::Blob.is_lob());
    assert!(!OracleDataType::Raw.is_lob());
}

#[test]
fn test_data_type_parse_name_variants() {
    assert_eq!(OracleDataType::parse_name("NUMBER"), OracleDataType::Number);
    assert_eq!(
        OracleDataType::parse_name("integer"),
        OracleDataType::Number
    );
    assert_eq!(
        OracleDataType::parse_name("VARCHAR2"),
        OracleDataType::Varchar2
    );
    assert_eq!(
        OracleDataType::parse_name("timestamp"),
        OracleDataType::Timestamp
    );
    assert_eq!(
        OracleDataType::parse_name("TIMESTAMP WITH TIME ZONE"),
        OracleDataType::TimestampTz
    );
    assert_eq!(
        OracleDataType::parse_name("TIMESTAMP WITH LOCAL TIME ZONE"),
        OracleDataType::TimestampLtz
    );
    assert_eq!(
        OracleDataType::parse_name("INTERVAL YEAR TO MONTH"),
        OracleDataType::IntervalYearToMonth
    );
    assert_eq!(
        OracleDataType::parse_name("INTERVAL DAY TO SECOND"),
        OracleDataType::IntervalDayToSecond
    );
    assert_eq!(
        OracleDataType::parse_name("UNKNOWN"),
        OracleDataType::Varchar2
    );
}

// --- OracleErrorCategory (1 test, comprehensive) ---

#[test]
fn test_error_category_from_code_description_retriable() {
    assert_eq!(
        OracleErrorCategory::from_code(1),
        OracleErrorCategory::DuplicateKey
    );
    assert_eq!(
        OracleErrorCategory::from_code(2291),
        OracleErrorCategory::ForeignKeyViolation
    );
    assert_eq!(
        OracleErrorCategory::from_code(2290),
        OracleErrorCategory::CheckConstraintViolation
    );
    assert_eq!(
        OracleErrorCategory::from_code(1401),
        OracleErrorCategory::ValueTooLarge
    );
    assert_eq!(
        OracleErrorCategory::from_code(900),
        OracleErrorCategory::InvalidSql
    );
    assert_eq!(
        OracleErrorCategory::from_code(942),
        OracleErrorCategory::ObjectNotFound
    );
    assert_eq!(
        OracleErrorCategory::from_code(60),
        OracleErrorCategory::Deadlock
    );
    assert_eq!(
        OracleErrorCategory::from_code(54),
        OracleErrorCategory::ResourceBusy
    );
    assert_eq!(
        OracleErrorCategory::from_code(99999),
        OracleErrorCategory::Other
    );

    assert!(!OracleErrorCategory::DuplicateKey.is_retriable());
    assert!(OracleErrorCategory::Deadlock.is_retriable());
    assert!(OracleErrorCategory::ResourceBusy.is_retriable());
    assert!(OracleErrorCategory::Timeout.is_retriable());
    assert!(!OracleErrorCategory::Other.is_retriable());

    assert!(!OracleErrorCategory::DuplicateKey.description().is_empty());
    assert_eq!(
        OracleErrorCategory::Deadlock.description(),
        "deadlock detected"
    );
}

// --- PlSqlCall (1 test, procedure + function) ---

#[test]
fn test_plsql_call_build_procedure_and_function() {
    let proc = PlSqlCall::procedure("my_proc")
        .param("p_id", "42")
        .param("p_name", "alice");
    let proc_sql = proc.build();
    assert!(proc_sql.contains("BEGIN"));
    assert!(proc_sql.contains("my_proc"));
    assert!(proc_sql.contains("p_id => p_id_val"));
    assert!(proc_sql.contains("p_name => p_name_val"));
    assert!(!proc_sql.contains("result :="));

    let func = PlSqlCall::function("my_func", "NUMBER").param("p_x", "1");
    let func_sql = func.build();
    assert!(func_sql.contains("result := my_func"));
    assert!(func_sql.contains("NUMBER"));
}
