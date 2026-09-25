//! M3: oracle type_mapping 测试 — 覆盖 OracleTypeKind/OracleColumnMeta/TypeMapping

use sz_orm_oracle::{OracleColumnMeta, OracleTypeKind, TypeMapping, ValueKind};

// === OracleTypeKind::parse_name ===

#[test]
fn test_parse_number_variants() {
    assert_eq!(OracleTypeKind::parse_name("NUMBER"), OracleTypeKind::Number);
    assert_eq!(OracleTypeKind::parse_name("number"), OracleTypeKind::Number);
    assert_eq!(
        OracleTypeKind::parse_name("INTEGER"),
        OracleTypeKind::Number
    );
    assert_eq!(OracleTypeKind::parse_name("INT"), OracleTypeKind::Number);
    assert_eq!(OracleTypeKind::parse_name("FLOAT"), OracleTypeKind::Number);
    assert_eq!(
        OracleTypeKind::parse_name("DECIMAL"),
        OracleTypeKind::Number
    );
    assert_eq!(OracleTypeKind::parse_name("REAL"), OracleTypeKind::Number);
}

#[test]
fn test_parse_varchar_variants() {
    assert_eq!(
        OracleTypeKind::parse_name("VARCHAR2"),
        OracleTypeKind::Varchar2
    );
    assert_eq!(
        OracleTypeKind::parse_name("VARCHAR"),
        OracleTypeKind::Varchar2
    );
    assert_eq!(
        OracleTypeKind::parse_name("NVARCHAR2"),
        OracleTypeKind::Nvarchar2
    );
}

#[test]
fn test_parse_timestamp_variants() {
    assert_eq!(
        OracleTypeKind::parse_name("TIMESTAMP"),
        OracleTypeKind::Timestamp
    );
    assert_eq!(
        OracleTypeKind::parse_name("TIMESTAMP WITH TIME ZONE"),
        OracleTypeKind::TimestampTz
    );
    assert_eq!(
        OracleTypeKind::parse_name("TIMESTAMP WITH LOCAL TIME ZONE"),
        OracleTypeKind::TimestampLtz
    );
}

#[test]
fn test_parse_other_types() {
    assert_eq!(OracleTypeKind::parse_name("DATE"), OracleTypeKind::Date);
    assert_eq!(OracleTypeKind::parse_name("CLOB"), OracleTypeKind::Clob);
    assert_eq!(OracleTypeKind::parse_name("BLOB"), OracleTypeKind::Blob);
    assert_eq!(OracleTypeKind::parse_name("JSON"), OracleTypeKind::Json);
    assert_eq!(
        OracleTypeKind::parse_name("BOOLEAN"),
        OracleTypeKind::Boolean
    );
    assert_eq!(
        OracleTypeKind::parse_name("XMLTYPE"),
        OracleTypeKind::Xmltype
    );
}

#[test]
fn test_parse_unknown_defaults_to_varchar2() {
    assert_eq!(
        OracleTypeKind::parse_name("UNKNOWN_TYPE"),
        OracleTypeKind::Varchar2
    );
}

// === OracleTypeKind::to_ddl ===

#[test]
fn test_to_ddl_number() {
    assert_eq!(OracleTypeKind::Number.to_ddl(None, None, None), "NUMBER");
    assert_eq!(
        OracleTypeKind::Number.to_ddl(None, Some(10), None),
        "NUMBER(10)"
    );
    assert_eq!(
        OracleTypeKind::Number.to_ddl(None, Some(10), Some(2)),
        "NUMBER(10, 2)"
    );
}

#[test]
fn test_to_ddl_varchar2() {
    assert_eq!(
        OracleTypeKind::Varchar2.to_ddl(None, None, None),
        "VARCHAR2(4000)"
    );
    assert_eq!(
        OracleTypeKind::Varchar2.to_ddl(Some(100), None, None),
        "VARCHAR2(100)"
    );
}

#[test]
fn test_to_ddl_timestamp_variants() {
    assert_eq!(
        OracleTypeKind::Timestamp.to_ddl(None, None, None),
        "TIMESTAMP"
    );
    assert_eq!(
        OracleTypeKind::TimestampTz.to_ddl(None, None, None),
        "TIMESTAMP WITH TIME ZONE"
    );
    assert_eq!(
        OracleTypeKind::TimestampLtz.to_ddl(None, None, None),
        "TIMESTAMP WITH LOCAL TIME ZONE"
    );
}

#[test]
fn test_to_ddl_lob_types() {
    assert_eq!(OracleTypeKind::Clob.to_ddl(None, None, None), "CLOB");
    assert_eq!(OracleTypeKind::Blob.to_ddl(None, None, None), "BLOB");
    assert_eq!(OracleTypeKind::Nclob.to_ddl(None, None, None), "NCLOB");
}

// === OracleTypeKind classification ===

#[test]
fn test_is_numeric() {
    assert!(OracleTypeKind::Number.is_numeric());
    assert!(OracleTypeKind::BinaryFloat.is_numeric());
    assert!(OracleTypeKind::BinaryDouble.is_numeric());
    assert!(!OracleTypeKind::Varchar2.is_numeric());
    assert!(!OracleTypeKind::Date.is_numeric());
}

#[test]
fn test_is_string() {
    assert!(OracleTypeKind::Varchar2.is_string());
    assert!(OracleTypeKind::Nvarchar2.is_string());
    assert!(OracleTypeKind::Char.is_string());
    assert!(OracleTypeKind::Clob.is_string());
    assert!(OracleTypeKind::Long.is_string());
    assert!(!OracleTypeKind::Number.is_string());
}

#[test]
fn test_is_binary() {
    assert!(OracleTypeKind::Raw.is_binary());
    assert!(OracleTypeKind::LongRaw.is_binary());
    assert!(OracleTypeKind::Blob.is_binary());
    assert!(!OracleTypeKind::Varchar2.is_binary());
}

#[test]
fn test_is_temporal() {
    assert!(OracleTypeKind::Date.is_temporal());
    assert!(OracleTypeKind::Timestamp.is_temporal());
    assert!(OracleTypeKind::TimestampTz.is_temporal());
    assert!(OracleTypeKind::TimestampLtz.is_temporal());
    assert!(!OracleTypeKind::Number.is_temporal());
}

#[test]
fn test_is_lob() {
    assert!(OracleTypeKind::Clob.is_lob());
    assert!(OracleTypeKind::Nclob.is_lob());
    assert!(OracleTypeKind::Blob.is_lob());
    assert!(!OracleTypeKind::Varchar2.is_lob());
}

// === OracleTypeKind::value_kind ===

#[test]
fn test_value_kind_mapping() {
    assert_eq!(OracleTypeKind::Number.value_kind(), ValueKind::Number);
    assert_eq!(OracleTypeKind::Varchar2.value_kind(), ValueKind::String);
    assert_eq!(OracleTypeKind::Raw.value_kind(), ValueKind::Bytes);
    assert_eq!(OracleTypeKind::Date.value_kind(), ValueKind::DateTime);
    assert_eq!(OracleTypeKind::Boolean.value_kind(), ValueKind::Bool);
    assert_eq!(OracleTypeKind::Json.value_kind(), ValueKind::Json);
    assert_eq!(OracleTypeKind::Rowid.value_kind(), ValueKind::String);
    assert_eq!(OracleTypeKind::Urowid.value_kind(), ValueKind::String);
}

// === OracleColumnMeta builder ===

#[test]
fn test_column_meta_new_defaults() {
    let col = OracleColumnMeta::new("id", OracleTypeKind::Number);
    assert_eq!(col.name, "id");
    assert_eq!(col.data_type, OracleTypeKind::Number);
    assert!(col.nullable);
    assert!(!col.is_primary_key);
    assert!(!col.is_unique);
}

#[test]
fn test_column_meta_builder_chain() {
    let col = OracleColumnMeta::new("id", OracleTypeKind::Number)
        .with_precision(10, 0)
        .with_nullable(false)
        .primary_key();
    assert_eq!(col.precision, Some(10));
    assert_eq!(col.scale, Some(0));
    assert!(!col.nullable);
    assert!(col.is_primary_key);
}

#[test]
fn test_column_meta_with_length() {
    let col = OracleColumnMeta::new("name", OracleTypeKind::Varchar2).with_length(255);
    assert_eq!(col.length, Some(255));
}

#[test]
fn test_column_meta_with_default() {
    let col = OracleColumnMeta::new("active", OracleTypeKind::Boolean).with_default("1");
    assert_eq!(col.default_value.as_deref(), Some("1"));
}

#[test]
fn test_column_meta_with_charset() {
    let col = OracleColumnMeta::new("name", OracleTypeKind::Varchar2).with_charset("AL32UTF8");
    assert_eq!(col.charset.as_deref(), Some("AL32UTF8"));
}

#[test]
fn test_column_meta_unique() {
    let col = OracleColumnMeta::new("email", OracleTypeKind::Varchar2).unique();
    assert!(col.is_unique);
}

#[test]
fn test_column_meta_to_ddl_not_null() {
    let col = OracleColumnMeta::new("id", OracleTypeKind::Number)
        .with_precision(10, 0)
        .with_nullable(false);
    let ddl = col.to_ddl();
    assert!(ddl.contains("NUMBER(10, 0)"));
    assert!(ddl.contains("NOT NULL"));
}

#[test]
fn test_column_meta_to_ddl_with_default() {
    let col = OracleColumnMeta::new("active", OracleTypeKind::Boolean).with_default("1");
    let ddl = col.to_ddl();
    assert!(ddl.contains("DEFAULT 1"));
}

#[test]
fn test_column_meta_to_column_ddl() {
    let col = OracleColumnMeta::new("id", OracleTypeKind::Number).with_nullable(false);
    let ddl = col.to_column_ddl();
    assert!(ddl.starts_with("id "));
    assert!(ddl.contains("NUMBER"));
    assert!(ddl.contains("NOT NULL"));
}

#[test]
fn test_column_meta_display() {
    let col = OracleColumnMeta::new("name", OracleTypeKind::Varchar2).with_length(100);
    let s = format!("{}", col);
    assert!(s.starts_with("name "));
    assert!(s.contains("VARCHAR2(100)"));
}

// === TypeMapping ===

#[test]
fn test_type_mapping_new() {
    let tm = TypeMapping::new();
    assert_eq!(tm.custom_mapping_count(), 0);
}

#[test]
fn test_type_mapping_register_and_lookup() {
    let tm = TypeMapping::new().register("MY_CUSTOM_TYPE", ValueKind::String);
    assert_eq!(tm.to_value_kind("MY_CUSTOM_TYPE"), ValueKind::String);
    assert_eq!(tm.custom_mapping_count(), 1);
}

#[test]
fn test_type_mapping_register_case_insensitive() {
    let tm = TypeMapping::new().register("MyType", ValueKind::Number);
    assert_eq!(tm.to_value_kind("MYTYPE"), ValueKind::Number);
    assert_eq!(tm.to_value_kind("mytype"), ValueKind::Number);
}

#[test]
fn test_type_mapping_unknown_falls_back_to_parse() {
    let tm = TypeMapping::new();
    assert_eq!(tm.to_value_kind("NUMBER"), ValueKind::Number);
    assert_eq!(tm.to_value_kind("VARCHAR2"), ValueKind::String);
}

#[test]
fn test_type_mapping_from_value() {
    use sz_orm_core::Value;
    assert_eq!(
        TypeMapping::from_value(&Value::I64(42)),
        OracleTypeKind::Number
    );
    assert_eq!(
        TypeMapping::from_value(&Value::String("x".to_string())),
        OracleTypeKind::Varchar2
    );
    assert_eq!(
        TypeMapping::from_value(&Value::Bool(true)),
        OracleTypeKind::Boolean
    );
    assert_eq!(
        TypeMapping::from_value(&Value::F64(1.0)),
        OracleTypeKind::BinaryDouble
    );
}

#[test]
fn test_type_mapping_cast_sql() {
    let tm = TypeMapping::new();
    let sql = tm.cast_sql("col", OracleTypeKind::Number);
    assert!(sql.contains("CAST(col AS NUMBER)"));
}

#[test]
fn test_type_mapping_compatibility_check_sql() {
    let tm = TypeMapping::new();
    let sql = tm.compatibility_check_sql("col", OracleTypeKind::Number);
    assert!(sql.contains("CAST(col AS NUMBER)"));
    assert!(sql.contains("CASE WHEN"));
}

#[test]
fn test_type_mapping_display() {
    let tm = TypeMapping::new().register("X", ValueKind::String);
    let s = format!("{}", tm);
    assert!(s.contains("TypeMapping"));
    assert!(s.contains("custom=1"));
}
