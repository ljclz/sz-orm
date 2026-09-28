use sz_orm_core::ColType;

#[test]
fn test_col_type_from_type_name_basic() {
    assert_eq!(ColType::from_type_name("BIGINT"), ColType::I64);
    assert_eq!(ColType::from_type_name("VARCHAR"), ColType::String);
    assert_eq!(ColType::from_type_name("BOOLEAN"), ColType::Bool);
    assert_eq!(ColType::from_type_name("UNKNOWN_X"), ColType::Unknown);
    assert_eq!(ColType::from_type_name(""), ColType::Unknown);
}

#[test]
fn test_col_type_from_type_name_int_family() {
    assert_eq!(ColType::from_type_name("TINYINT"), ColType::I8);
    assert_eq!(ColType::from_type_name("SMALLINT"), ColType::I16);
    assert_eq!(ColType::from_type_name("INT"), ColType::I32);
    assert_eq!(ColType::from_type_name("INT8"), ColType::I64);
    assert_eq!(ColType::from_type_name("INT UNSIGNED"), ColType::U32);
}

#[test]
fn test_col_type_parse_sqlite() {
    assert_eq!(ColType::parse_sqlite("INTEGER"), ColType::I64);
    assert_eq!(ColType::parse_sqlite("REAL"), ColType::F64);
    assert_eq!(ColType::parse_sqlite("TEXT"), ColType::String);
    assert_eq!(ColType::parse_sqlite("BLOB"), ColType::Bytes);
    assert_eq!(ColType::parse_sqlite(""), ColType::Unknown);
}

#[test]
fn test_col_type_parse_sqlite_case_insensitive() {
    assert_eq!(ColType::parse_sqlite("integer"), ColType::I64);
    assert_eq!(ColType::parse_sqlite("text"), ColType::String);
    assert_eq!(ColType::parse_sqlite("boolean"), ColType::Bool);
}

#[test]
fn test_col_type_parse_mysql() {
    assert_eq!(ColType::parse_mysql("TINYINT"), ColType::I8);
    assert_eq!(ColType::parse_mysql("BIGINT"), ColType::I64);
    assert_eq!(ColType::parse_mysql("JSON"), ColType::Json);
    assert_eq!(ColType::parse_mysql("YEAR"), ColType::I16);
    assert_eq!(ColType::parse_mysql("DOUBLE"), ColType::F64);
}

#[test]
fn test_col_type_parse_mysql_text_blob_family() {
    assert_eq!(ColType::parse_mysql("LONGTEXT"), ColType::String);
    assert_eq!(ColType::parse_mysql("ENUM"), ColType::String);
    assert_eq!(ColType::parse_mysql("LONGBLOB"), ColType::Bytes);
    assert_eq!(ColType::parse_mysql("VARBINARY"), ColType::Bytes);
}

#[test]
fn test_col_type_parse_postgres() {
    assert_eq!(ColType::parse_postgres("INT8"), ColType::I64);
    assert_eq!(ColType::parse_postgres("FLOAT8"), ColType::F64);
    assert_eq!(ColType::parse_postgres("JSONB"), ColType::Json);
    assert_eq!(ColType::parse_postgres("OID"), ColType::I32);
    assert_eq!(ColType::parse_postgres("BOOL"), ColType::Bool);
}

#[test]
fn test_col_type_parse_postgres_text_bytea() {
    assert_eq!(ColType::parse_postgres("BPCHAR"), ColType::String);
    assert_eq!(ColType::parse_postgres("CITEXT"), ColType::String);
    assert_eq!(ColType::parse_postgres("BYTEA"), ColType::Bytes);
    assert_eq!(ColType::parse_postgres("TIMESTAMPTZ"), ColType::DateTime);
}

#[test]
fn test_col_type_parse_postgres_case_insensitive() {
    assert_eq!(ColType::parse_postgres("int8"), ColType::I64);
    assert_eq!(ColType::parse_postgres("float8"), ColType::F64);
    assert_eq!(ColType::parse_postgres("uuid"), ColType::Uuid);
}