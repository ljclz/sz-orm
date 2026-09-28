//! T25: MSSQL 方言/类型/错误分类纯单元测试（12 tests）

use sz_orm_mssql::{MssqlDialect, MssqlErrorCategory, MssqlType};

// --- MssqlDialect (6 tests) ---

#[test]
fn test_mssql_dialect_new_default_equal() {
    let a = MssqlDialect::new();
    let b = MssqlDialect::default();
    assert_eq!(a.quote_identifier("x"), b.quote_identifier("x"));
}

#[test]
fn test_mssql_dialect_quote_identifier_plain_and_escape() {
    let d = MssqlDialect::new();
    assert_eq!(d.quote_identifier("col"), "[col]");
    assert_eq!(d.quote_identifier("a]b"), "[a]]b]");
}

#[test]
fn test_mssql_dialect_limit_clause_top() {
    let d = MssqlDialect::new();
    assert_eq!(d.limit_clause(Some(10), None), "TOP 10 ");
}

#[test]
fn test_mssql_dialect_limit_clause_offset_fetch() {
    let d = MssqlDialect::new();
    assert_eq!(
        d.limit_clause(Some(10), Some(20)),
        "OFFSET 20 ROWS FETCH NEXT 10 ROWS ONLY"
    );
    assert_eq!(d.limit_clause(None, Some(5)), "OFFSET 5 ROWS");
    assert_eq!(d.limit_clause(None, None), "");
}

#[test]
fn test_mssql_dialect_placeholder() {
    let d = MssqlDialect::new();
    assert_eq!(d.placeholder(1), "@P1");
    assert_eq!(d.placeholder(2), "@P2");
}

#[test]
fn test_mssql_dialect_is_reserved_keyword_true_and_false() {
    let d = MssqlDialect::new();
    assert!(d.is_reserved_keyword("SELECT"));
    assert!(d.is_reserved_keyword("select"));
    assert!(d.is_reserved_keyword("TOP"));
    assert!(d.is_reserved_keyword("CLUSTERED"));
    assert!(!d.is_reserved_keyword("mycol"));
    assert!(!d.is_reserved_keyword("status"));
}

// --- MssqlType (5 tests) ---

#[test]
fn test_mssql_type_as_sql_type_representative() {
    assert_eq!(MssqlType::Bigint.as_sql_type(), "BIGINT");
    assert_eq!(MssqlType::Nvarchar.as_sql_type(), "NVARCHAR");
    assert_eq!(MssqlType::Datetimeoffset.as_sql_type(), "DATETIMEOFFSET");
    assert_eq!(MssqlType::Variant.as_sql_type(), "SQL_VARIANT");
    assert_eq!(MssqlType::Null.as_sql_type(), "NULL");
}

#[test]
fn test_mssql_type_is_numeric() {
    assert!(MssqlType::Int.is_numeric());
    assert!(MssqlType::Bigint.is_numeric());
    assert!(MssqlType::Decimal.is_numeric());
    assert!(MssqlType::Bit.is_numeric());
    assert!(MssqlType::Tinyint.is_numeric());
    assert!(!MssqlType::Varchar.is_numeric());
    assert!(!MssqlType::Date.is_numeric());
}

#[test]
fn test_mssql_type_is_string() {
    assert!(MssqlType::Varchar.is_string());
    assert!(MssqlType::Nvarchar.is_string());
    assert!(MssqlType::Text.is_string());
    assert!(MssqlType::Xml.is_string());
    assert!(!MssqlType::Int.is_string());
    assert!(!MssqlType::Varbinary.is_string());
}

#[test]
fn test_mssql_type_is_binary_and_temporal() {
    assert!(MssqlType::Binary.is_binary());
    assert!(MssqlType::Varbinary.is_binary());
    assert!(MssqlType::Image.is_binary());
    assert!(!MssqlType::Int.is_binary());

    assert!(MssqlType::Date.is_temporal());
    assert!(MssqlType::Datetime.is_temporal());
    assert!(MssqlType::Datetime2.is_temporal());
    assert!(MssqlType::Time.is_temporal());
    assert!(!MssqlType::Int.is_temporal());
}

#[test]
fn test_mssql_type_parse_name_variants() {
    assert_eq!(MssqlType::parse_name("INT"), MssqlType::Int);
    assert_eq!(MssqlType::parse_name("integer"), MssqlType::Int);
    assert_eq!(MssqlType::parse_name("NVARCHAR"), MssqlType::Nvarchar);
    assert_eq!(MssqlType::parse_name("SQL_VARIANT"), MssqlType::Variant);
    assert_eq!(MssqlType::parse_name("NULL"), MssqlType::Null);
    assert_eq!(MssqlType::parse_name("UNKNOWN"), MssqlType::Variant);
}

// --- MssqlErrorCategory (1 test, comprehensive) ---

#[test]
fn test_mssql_error_category_from_code_description_retriable() {
    assert_eq!(MssqlErrorCategory::from_code(2627), MssqlErrorCategory::DuplicateKey);
    assert_eq!(MssqlErrorCategory::from_code(2601), MssqlErrorCategory::DuplicateKey);
    assert_eq!(MssqlErrorCategory::from_code(547), MssqlErrorCategory::ConstraintViolation);
    assert_eq!(MssqlErrorCategory::from_code(515), MssqlErrorCategory::NullViolation);
    assert_eq!(MssqlErrorCategory::from_code(208), MssqlErrorCategory::InvalidObject);
    assert_eq!(MssqlErrorCategory::from_code(1205), MssqlErrorCategory::Deadlock);
    assert_eq!(MssqlErrorCategory::from_code(-2), MssqlErrorCategory::Timeout);
    assert_eq!(MssqlErrorCategory::from_code(999), MssqlErrorCategory::Other);

    assert!(MssqlErrorCategory::Deadlock.is_retriable());
    assert!(MssqlErrorCategory::Timeout.is_retriable());
    assert!(!MssqlErrorCategory::DuplicateKey.is_retriable());
    assert!(!MssqlErrorCategory::Other.is_retriable());

    assert_eq!(MssqlErrorCategory::Deadlock.description(), "deadlock detected");
    assert_eq!(MssqlErrorCategory::Timeout.description(), "query timeout");
}