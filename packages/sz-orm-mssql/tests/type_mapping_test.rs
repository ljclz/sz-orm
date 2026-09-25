//! M3: mssql type_mapping 测试 — 覆盖 MssqlTypeKind/MssqlColumnMeta/MssqlTypeMapping

use sz_orm_mssql::{MssqlColumnMeta, MssqlTypeKind, MssqlTypeMapping, ValueKind};

// === MssqlTypeKind::parse_name ===

#[test]
fn test_parse_int_variants() {
    assert_eq!(MssqlTypeKind::parse_name("INT"), MssqlTypeKind::Int);
    assert_eq!(MssqlTypeKind::parse_name("INTEGER"), MssqlTypeKind::Int);
    assert_eq!(MssqlTypeKind::parse_name("BIGINT"), MssqlTypeKind::BigInt);
    assert_eq!(
        MssqlTypeKind::parse_name("SMALLINT"),
        MssqlTypeKind::Smallint
    );
    assert_eq!(MssqlTypeKind::parse_name("TINYINT"), MssqlTypeKind::Tinyint);
}

#[test]
fn test_parse_decimal_variants() {
    assert_eq!(MssqlTypeKind::parse_name("DECIMAL"), MssqlTypeKind::Decimal);
    assert_eq!(MssqlTypeKind::parse_name("NUMERIC"), MssqlTypeKind::Numeric);
}

#[test]
fn test_parse_string_types() {
    assert_eq!(MssqlTypeKind::parse_name("VARCHAR"), MssqlTypeKind::Varchar);
    assert_eq!(
        MssqlTypeKind::parse_name("NVARCHAR"),
        MssqlTypeKind::Nvarchar
    );
    assert_eq!(MssqlTypeKind::parse_name("CHAR"), MssqlTypeKind::Char);
    assert_eq!(MssqlTypeKind::parse_name("CHARACTER"), MssqlTypeKind::Char);
    assert_eq!(MssqlTypeKind::parse_name("NCHAR"), MssqlTypeKind::Nchar);
    assert_eq!(MssqlTypeKind::parse_name("TEXT"), MssqlTypeKind::Text);
    assert_eq!(MssqlTypeKind::parse_name("NTEXT"), MssqlTypeKind::Ntext);
}

#[test]
fn test_parse_datetime_types() {
    assert_eq!(MssqlTypeKind::parse_name("DATE"), MssqlTypeKind::Date);
    assert_eq!(
        MssqlTypeKind::parse_name("DATETIME"),
        MssqlTypeKind::Datetime
    );
    assert_eq!(
        MssqlTypeKind::parse_name("DATETIME2"),
        MssqlTypeKind::Datetime2
    );
    assert_eq!(
        MssqlTypeKind::parse_name("DATETIMEOFFSET"),
        MssqlTypeKind::Datetimeoffset
    );
    assert_eq!(
        MssqlTypeKind::parse_name("SMALLDATETIME"),
        MssqlTypeKind::Smalldatetime
    );
    assert_eq!(MssqlTypeKind::parse_name("TIME"), MssqlTypeKind::Time);
}

#[test]
fn test_parse_special_types() {
    assert_eq!(MssqlTypeKind::parse_name("BIT"), MssqlTypeKind::Bit);
    assert_eq!(MssqlTypeKind::parse_name("JSON"), MssqlTypeKind::Json);
    assert_eq!(MssqlTypeKind::parse_name("XML"), MssqlTypeKind::Xml);
    assert_eq!(
        MssqlTypeKind::parse_name("GEOGRAPHY"),
        MssqlTypeKind::Geography
    );
    assert_eq!(
        MssqlTypeKind::parse_name("GEOMETRY"),
        MssqlTypeKind::Geometry
    );
    assert_eq!(
        MssqlTypeKind::parse_name("UNIQUEIDENTIFIER"),
        MssqlTypeKind::Uniqueidentifier
    );
    assert_eq!(
        MssqlTypeKind::parse_name("HIERARCHYID"),
        MssqlTypeKind::Hierarchyid
    );
}

#[test]
fn test_parse_unknown_defaults_to_varchar() {
    assert_eq!(
        MssqlTypeKind::parse_name("UNKNOWN_TYPE"),
        MssqlTypeKind::Varchar
    );
}

#[test]
fn test_parse_case_insensitive() {
    assert_eq!(MssqlTypeKind::parse_name("int"), MssqlTypeKind::Int);
    assert_eq!(MssqlTypeKind::parse_name("varchar"), MssqlTypeKind::Varchar);
}

// === MssqlTypeKind::as_sql_name ===

#[test]
fn test_as_sql_name() {
    assert_eq!(MssqlTypeKind::BigInt.as_sql_name(), "BIGINT");
    assert_eq!(MssqlTypeKind::Varchar.as_sql_name(), "VARCHAR");
    assert_eq!(MssqlTypeKind::Nvarchar.as_sql_name(), "NVARCHAR");
    assert_eq!(MssqlTypeKind::Decimal.as_sql_name(), "DECIMAL");
    assert_eq!(MssqlTypeKind::Geography.as_sql_name(), "GEOGRAPHY");
}

// === MssqlTypeKind::to_ddl ===

#[test]
fn test_to_ddl_decimal() {
    assert_eq!(MssqlTypeKind::Decimal.to_ddl(None, None, None), "DECIMAL");
    assert_eq!(
        MssqlTypeKind::Decimal.to_ddl(None, Some(10), None),
        "DECIMAL(10)"
    );
    assert_eq!(
        MssqlTypeKind::Decimal.to_ddl(None, Some(10), Some(2)),
        "DECIMAL(10, 2)"
    );
}

#[test]
fn test_to_ddl_varchar_with_length() {
    assert_eq!(
        MssqlTypeKind::Varchar.to_ddl(Some(50), None, None),
        "VARCHAR(50)"
    );
}

#[test]
fn test_to_ddl_varchar_max() {
    assert_eq!(
        MssqlTypeKind::Varchar.to_ddl(Some(-1), None, None),
        "VARCHAR(MAX)"
    );
}

#[test]
fn test_to_ddl_varchar_no_length() {
    assert_eq!(MssqlTypeKind::Varchar.to_ddl(None, None, None), "VARCHAR");
}

#[test]
fn test_to_ddl_float_with_length() {
    assert_eq!(
        MssqlTypeKind::Float.to_ddl(Some(53), None, None),
        "FLOAT(53)"
    );
    assert_eq!(MssqlTypeKind::Float.to_ddl(None, None, None), "FLOAT");
}

#[test]
fn test_to_ddl_datetime2_precision() {
    assert_eq!(
        MssqlTypeKind::Datetime2.to_ddl(None, Some(7), None),
        "DATETIME2(7)"
    );
    assert_eq!(
        MssqlTypeKind::Datetime2.to_ddl(None, None, None),
        "DATETIME2"
    );
}

#[test]
fn test_to_ddl_simple_types() {
    assert_eq!(MssqlTypeKind::Int.to_ddl(None, None, None), "INT");
    assert_eq!(MssqlTypeKind::Bit.to_ddl(None, None, None), "BIT");
    assert_eq!(MssqlTypeKind::Date.to_ddl(None, None, None), "DATE");
}

// === MssqlTypeKind classification ===

#[test]
fn test_is_numeric() {
    assert!(MssqlTypeKind::Int.is_numeric());
    assert!(MssqlTypeKind::BigInt.is_numeric());
    assert!(MssqlTypeKind::Decimal.is_numeric());
    assert!(MssqlTypeKind::Float.is_numeric());
    assert!(MssqlTypeKind::Bit.is_numeric());
    assert!(MssqlTypeKind::Tinyint.is_numeric());
    assert!(!MssqlTypeKind::Varchar.is_numeric());
    assert!(!MssqlTypeKind::Date.is_numeric());
}

#[test]
fn test_is_string() {
    assert!(MssqlTypeKind::Varchar.is_string());
    assert!(MssqlTypeKind::Nvarchar.is_string());
    assert!(MssqlTypeKind::Char.is_string());
    assert!(MssqlTypeKind::Text.is_string());
    assert!(!MssqlTypeKind::Int.is_string());
}

#[test]
fn test_is_binary() {
    assert!(MssqlTypeKind::Binary.is_binary());
    assert!(MssqlTypeKind::Varbinary.is_binary());
    assert!(MssqlTypeKind::Image.is_binary());
    assert!(!MssqlTypeKind::Varchar.is_binary());
}

#[test]
fn test_is_temporal() {
    assert!(MssqlTypeKind::Date.is_temporal());
    assert!(MssqlTypeKind::Datetime.is_temporal());
    assert!(MssqlTypeKind::Datetime2.is_temporal());
    assert!(MssqlTypeKind::Time.is_temporal());
    assert!(!MssqlTypeKind::Int.is_temporal());
}

// === MssqlColumnMeta builder ===

#[test]
fn test_column_meta_new_defaults() {
    let col = MssqlColumnMeta::new("id", MssqlTypeKind::BigInt);
    assert_eq!(col.name, "id");
    assert_eq!(col.data_type, MssqlTypeKind::BigInt);
    assert!(col.nullable);
    assert!(!col.is_primary_key);
    assert!(!col.is_identity);
    assert!(!col.is_computed);
    assert!(!col.is_sparse);
}

#[test]
fn test_column_meta_builder_chain() {
    let col = MssqlColumnMeta::new("id", MssqlTypeKind::BigInt)
        .with_nullable(false)
        .primary_key();
    assert!(!col.nullable);
    assert!(col.is_primary_key);
}

#[test]
fn test_column_meta_with_max_length() {
    let col = MssqlColumnMeta::new("name", MssqlTypeKind::Varchar).with_max_length(255);
    assert_eq!(col.max_length, Some(255));
}

#[test]
fn test_column_meta_with_precision() {
    let col = MssqlColumnMeta::new("price", MssqlTypeKind::Decimal).with_precision(10, 2);
    assert_eq!(col.precision, Some(10));
    assert_eq!(col.scale, Some(2));
}

#[test]
fn test_column_meta_identity() {
    let col = MssqlColumnMeta::new("id", MssqlTypeKind::BigInt).identity();
    assert!(col.is_identity);
    assert!(!col.nullable);
}

#[test]
fn test_column_meta_computed() {
    let col = MssqlColumnMeta::new("total", MssqlTypeKind::Decimal).computed("price * quantity");
    assert!(col.is_computed);
    assert_eq!(col.computed_expression.as_deref(), Some("price * quantity"));
}

#[test]
fn test_column_meta_sparse() {
    let col = MssqlColumnMeta::new("optional_data", MssqlTypeKind::Varchar).sparse();
    assert!(col.is_sparse);
    assert!(col.nullable);
}

#[test]
fn test_column_meta_unique() {
    let col = MssqlColumnMeta::new("email", MssqlTypeKind::Varchar).unique();
    assert!(col.is_unique);
}

#[test]
fn test_column_meta_to_ddl_identity() {
    let col = MssqlColumnMeta::new("id", MssqlTypeKind::BigInt).identity();
    let ddl = col.to_ddl();
    assert!(ddl.contains("IDENTITY(1,1)"));
}

#[test]
fn test_column_meta_to_ddl_computed() {
    let col = MssqlColumnMeta::new("total", MssqlTypeKind::Decimal).computed("price * qty");
    let ddl = col.to_ddl();
    assert!(ddl.contains("AS (price * qty)"));
}

#[test]
fn test_column_meta_to_ddl_sparse() {
    let col = MssqlColumnMeta::new("data", MssqlTypeKind::Varchar)
        .with_max_length(100)
        .sparse();
    let ddl = col.to_ddl();
    assert!(ddl.contains("SPARSE"));
}

#[test]
fn test_column_meta_to_ddl_null() {
    let col = MssqlColumnMeta::new("name", MssqlTypeKind::Varchar).with_max_length(100);
    let ddl = col.to_ddl();
    assert!(ddl.contains("NULL"));
}

#[test]
fn test_column_meta_to_ddl_not_null() {
    let col = MssqlColumnMeta::new("name", MssqlTypeKind::Varchar)
        .with_max_length(100)
        .with_nullable(false);
    let ddl = col.to_ddl();
    assert!(ddl.contains("NOT NULL"));
}

#[test]
fn test_column_meta_to_column_ddl() {
    let col = MssqlColumnMeta::new("id", MssqlTypeKind::BigInt).with_nullable(false);
    let ddl = col.to_column_ddl();
    assert!(ddl.starts_with("id "));
    assert!(ddl.contains("BIGINT"));
}

#[test]
fn test_column_meta_display() {
    let col = MssqlColumnMeta::new("name", MssqlTypeKind::Varchar).with_max_length(50);
    let s = format!("{}", col);
    assert!(s.starts_with("name "));
    assert!(s.contains("VARCHAR(50)"));
}

// === MssqlTypeMapping ===

#[test]
fn test_type_mapping_new() {
    let tm = MssqlTypeMapping::new();
    assert_eq!(tm.custom_mapping_count(), 0);
}

#[test]
fn test_type_mapping_register_and_lookup() {
    let tm = MssqlTypeMapping::new().register("MY_TYPE", ValueKind::String);
    assert_eq!(tm.to_value_kind("MY_TYPE"), ValueKind::String);
    assert_eq!(tm.custom_mapping_count(), 1);
}

#[test]
fn test_type_mapping_case_insensitive() {
    let tm = MssqlTypeMapping::new().register("CustomType", ValueKind::Number);
    assert_eq!(tm.to_value_kind("CUSTOMTYPE"), ValueKind::Number);
}

#[test]
fn test_type_mapping_unknown_falls_back_to_parse() {
    let tm = MssqlTypeMapping::new();
    assert_eq!(tm.to_value_kind("INT"), ValueKind::Number);
    assert_eq!(tm.to_value_kind("VARCHAR"), ValueKind::String);
}

#[test]
fn test_type_mapping_from_value() {
    use sz_orm_core::Value;
    assert_eq!(
        MssqlTypeMapping::from_value(&Value::I64(42)),
        MssqlTypeKind::BigInt
    );
    assert_eq!(
        MssqlTypeMapping::from_value(&Value::String("x".to_string())),
        MssqlTypeKind::Nvarchar
    );
    assert_eq!(
        MssqlTypeMapping::from_value(&Value::Bool(true)),
        MssqlTypeKind::Bit
    );
    assert_eq!(
        MssqlTypeMapping::from_value(&Value::F64(1.0)),
        MssqlTypeKind::Float
    );
}

#[test]
fn test_type_mapping_cast_sql() {
    let tm = MssqlTypeMapping::new();
    let sql = tm.cast_sql("col", MssqlTypeKind::Int);
    assert!(sql.contains("CAST(col AS INT)"));
}

#[test]
fn test_type_mapping_try_cast_sql() {
    let tm = MssqlTypeMapping::new();
    let sql = tm.try_cast_sql("col", MssqlTypeKind::Int);
    assert!(sql.contains("TRY_CAST(col AS INT)"));
}

#[test]
fn test_type_mapping_convert_sql_no_style() {
    let tm = MssqlTypeMapping::new();
    let sql = tm.convert_sql(MssqlTypeKind::Int, "col", None);
    assert!(sql.contains("CONVERT(INT, col)"));
}

#[test]
fn test_type_mapping_convert_sql_with_style() {
    let tm = MssqlTypeMapping::new();
    let sql = tm.convert_sql(MssqlTypeKind::Int, "col", Some(120));
    assert!(sql.contains("CONVERT(INT, col, 120)"));
}

#[test]
fn test_type_mapping_display() {
    let tm = MssqlTypeMapping::new().register("X", ValueKind::String);
    let s = format!("{}", tm);
    assert!(s.contains("MssqlTypeMapping"));
    assert!(s.contains("custom=1"));
}

// === MssqlTypeKind additional methods ===

#[test]
fn test_is_lob() {
    assert!(MssqlTypeKind::Text.is_lob());
    assert!(MssqlTypeKind::Ntext.is_lob());
    assert!(MssqlTypeKind::Image.is_lob());
    assert!(MssqlTypeKind::Xml.is_lob());
    assert!(!MssqlTypeKind::Int.is_lob());
}

#[test]
fn test_requires_length() {
    assert!(MssqlTypeKind::Varchar.requires_length());
    assert!(MssqlTypeKind::Nvarchar.requires_length());
    assert!(MssqlTypeKind::Char.requires_length());
    assert!(MssqlTypeKind::Binary.requires_length());
    assert!(MssqlTypeKind::Varbinary.requires_length());
    assert!(!MssqlTypeKind::Int.requires_length());
}

#[test]
fn test_requires_precision() {
    assert!(MssqlTypeKind::Decimal.requires_precision());
    assert!(MssqlTypeKind::Numeric.requires_precision());
    assert!(MssqlTypeKind::Datetime2.requires_precision());
    assert!(MssqlTypeKind::Time.requires_precision());
    assert!(!MssqlTypeKind::Int.requires_precision());
}

#[test]
fn test_value_kind_mapping() {
    assert_eq!(MssqlTypeKind::Int.value_kind(), ValueKind::Number);
    assert_eq!(MssqlTypeKind::Varchar.value_kind(), ValueKind::String);
    assert_eq!(MssqlTypeKind::Binary.value_kind(), ValueKind::Bytes);
    assert_eq!(MssqlTypeKind::Date.value_kind(), ValueKind::DateTime);
    assert_eq!(MssqlTypeKind::Bit.value_kind(), ValueKind::Bool);
    assert_eq!(
        MssqlTypeKind::Uniqueidentifier.value_kind(),
        ValueKind::Uuid
    );
    assert_eq!(MssqlTypeKind::Json.value_kind(), ValueKind::Json);
}
