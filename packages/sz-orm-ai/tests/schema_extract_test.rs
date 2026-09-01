//! Schema 提取器单元测试
//!
//! 使用 MockConnection 验证 5 种方言的 Schema 提取器均能正确构造 SchemaContext。
//! 集成测试（连接真实 DB）在 schema_extract_integration.rs 中，标记为 #[ignore]。

#![cfg(feature = "ai-schema-extract")]

use sz_orm_ai::schema_extractor::{
    create_extractor, MssqlSchemaExtractor, MySqlSchemaExtractor, OracleSchemaExtractor,
    PgSchemaExtractor, SchemaExtractor, SqliteSchemaExtractor,
};
use sz_orm_core::mock::MockConnection;
use sz_orm_core::{DbType, Value};

#[tokio::test]
async fn test_mysql_schema_extractor() {
    let mut mock = MockConnection::new();

    // 预设表列表查询
    mock.expect_query(
        "SELECT TABLE_NAME FROM information_schema.TABLES WHERE TABLE_SCHEMA = DATABASE() ORDER BY TABLE_NAME",
    )
    .with_rows(vec![
        vec![("TABLE_NAME", Value::from("users"))],
        vec![("TABLE_NAME", Value::from("orders"))],
    ]);

    // 预设 users 表列查询
    mock.expect_query(
        "SELECT COLUMN_NAME, DATA_TYPE, IS_NULLABLE, COLUMN_KEY FROM information_schema.COLUMNS WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = 'users' ORDER BY ORDINAL_POSITION",
    )
    .with_rows(vec![
        vec![
            ("COLUMN_NAME", Value::from("id")),
            ("DATA_TYPE", Value::from("int")),
            ("IS_NULLABLE", Value::from("NO")),
            ("COLUMN_KEY", Value::from("PRI")),
        ],
        vec![
            ("COLUMN_NAME", Value::from("name")),
            ("DATA_TYPE", Value::from("varchar")),
            ("IS_NULLABLE", Value::from("YES")),
            ("COLUMN_KEY", Value::from("")),
        ],
    ]);

    // 预设 orders 表列查询
    mock.expect_query(
        "SELECT COLUMN_NAME, DATA_TYPE, IS_NULLABLE, COLUMN_KEY FROM information_schema.COLUMNS WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = 'orders' ORDER BY ORDINAL_POSITION",
    )
    .with_rows(vec![
        vec![
            ("COLUMN_NAME", Value::from("id")),
            ("DATA_TYPE", Value::from("bigint")),
            ("IS_NULLABLE", Value::from("NO")),
            ("COLUMN_KEY", Value::from("PRI")),
        ],
        vec![
            ("COLUMN_NAME", Value::from("user_id")),
            ("DATA_TYPE", Value::from("int")),
            ("IS_NULLABLE", Value::from("NO")),
            ("COLUMN_KEY", Value::from("MUL")),
        ],
    ]);

    let extractor = MySqlSchemaExtractor;
    let schema = extractor.extract_schema(&mut mock).await.unwrap();

    assert_eq!(schema.tables.len(), 2);
    assert_eq!(schema.tables[0].name, "users");
    assert_eq!(schema.tables[0].columns.len(), 2);
    assert_eq!(schema.tables[0].columns[0].name, "id");
    assert!(schema.tables[0].columns[0].is_primary_key);
    assert!(!schema.tables[0].columns[0].nullable);
    assert_eq!(schema.tables[0].columns[1].name, "name");
    assert!(!schema.tables[0].columns[1].is_primary_key);
    assert!(schema.tables[0].columns[1].nullable);

    assert_eq!(schema.tables[1].name, "orders");
    assert_eq!(schema.tables[1].columns.len(), 2);
    assert_eq!(schema.tables[1].columns[1].name, "user_id");
    assert!(!schema.tables[1].columns[1].is_primary_key);
}

#[tokio::test]
async fn test_pg_schema_extractor() {
    let mut mock = MockConnection::new();

    mock.expect_query(
        "SELECT tablename FROM pg_tables WHERE schemaname = 'public' ORDER BY tablename",
    )
    .with_rows(vec![vec![("tablename", Value::from("users"))]]);

    mock.expect_query(
        "SELECT column_name, data_type, is_nullable, (SELECT COUNT(*) FROM information_schema.key_column_usage kcu JOIN information_schema.table_constraints tc ON kcu.constraint_name = tc.constraint_name WHERE kcu.table_name = 'users' AND tc.constraint_type = 'PRIMARY KEY' AND kcu.column_name = c.column_name) as is_pk FROM information_schema.columns c WHERE table_schema = 'public' AND table_name = 'users' ORDER BY ordinal_position",
    )
    .with_rows(vec![
        vec![
            ("column_name", Value::from("id")),
            ("data_type", Value::from("integer")),
            ("is_nullable", Value::from("NO")),
            ("is_pk", Value::from(1i64)),
        ],
        vec![
            ("column_name", Value::from("email")),
            ("data_type", Value::from("text")),
            ("is_nullable", Value::from("YES")),
            ("is_pk", Value::from(0i64)),
        ],
    ]);

    let extractor = PgSchemaExtractor;
    let schema = extractor.extract_schema(&mut mock).await.unwrap();

    assert_eq!(schema.tables.len(), 1);
    assert_eq!(schema.tables[0].name, "users");
    assert_eq!(schema.tables[0].columns.len(), 2);
    assert!(schema.tables[0].columns[0].is_primary_key);
    assert!(!schema.tables[0].columns[1].is_primary_key);
}

#[tokio::test]
async fn test_sqlite_schema_extractor() {
    let mut mock = MockConnection::new();

    mock.expect_query(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )
    .with_rows(vec![vec![("name", Value::from("products"))]]);

    mock.expect_query("PRAGMA table_info('products')")
        .with_rows(vec![
            vec![
                ("name", Value::from("id")),
                ("type", Value::from("INTEGER")),
                ("notnull", Value::from(1i64)),
                ("pk", Value::from(1i64)),
            ],
            vec![
                ("name", Value::from("price")),
                ("type", Value::from("REAL")),
                ("notnull", Value::from(0i64)),
                ("pk", Value::from(0i64)),
            ],
        ]);

    let extractor = SqliteSchemaExtractor;
    let schema = extractor.extract_schema(&mut mock).await.unwrap();

    assert_eq!(schema.tables.len(), 1);
    assert_eq!(schema.tables[0].name, "products");
    assert_eq!(schema.tables[0].columns.len(), 2);
    assert!(schema.tables[0].columns[0].is_primary_key);
    assert!(!schema.tables[0].columns[0].nullable);
    assert!(schema.tables[0].columns[1].nullable);
}

#[tokio::test]
async fn test_oracle_schema_extractor() {
    let mut mock = MockConnection::new();

    mock.expect_query("SELECT TABLE_NAME FROM USER_TABLES ORDER BY TABLE_NAME")
        .with_rows(vec![vec![("TABLE_NAME", Value::from("EMPLOYEES"))]]);

    mock.expect_query(
        "SELECT COLUMN_NAME, DATA_TYPE, NULLABLE, (SELECT COUNT(*) FROM USER_CONSTRAINTS uc JOIN USER_CONS_COLUMNS ucc ON uc.CONSTRAINT_NAME = ucc.CONSTRAINT_NAME WHERE uc.TABLE_NAME = 'EMPLOYEES' AND uc.CONSTRAINT_TYPE = 'P' AND ucc.COLUMN_NAME = c.COLUMN_NAME) as IS_PK FROM USER_TAB_COLUMNS c WHERE TABLE_NAME = 'EMPLOYEES' ORDER BY COLUMN_ID",
    )
    .with_rows(vec![
        vec![
            ("COLUMN_NAME", Value::from("EMP_ID")),
            ("DATA_TYPE", Value::from("NUMBER")),
            ("NULLABLE", Value::from("N")),
            ("IS_PK", Value::from(1i64)),
        ],
        vec![
            ("COLUMN_NAME", Value::from("SALARY")),
            ("DATA_TYPE", Value::from("NUMBER")),
            ("NULLABLE", Value::from("Y")),
            ("IS_PK", Value::from(0i64)),
        ],
    ]);

    let extractor = OracleSchemaExtractor;
    let schema = extractor.extract_schema(&mut mock).await.unwrap();

    assert_eq!(schema.tables.len(), 1);
    assert_eq!(schema.tables[0].name, "EMPLOYEES");
    assert_eq!(schema.tables[0].columns.len(), 2);
    assert!(schema.tables[0].columns[0].is_primary_key);
    assert!(schema.tables[0].columns[1].nullable);
}

#[tokio::test]
async fn test_mssql_schema_extractor() {
    let mut mock = MockConnection::new();

    mock.expect_query(
        "SELECT TABLE_NAME FROM INFORMATION_SCHEMA.TABLES WHERE TABLE_TYPE = 'BASE TABLE' ORDER BY TABLE_NAME",
    )
    .with_rows(vec![vec![("TABLE_NAME", Value::from("Customers"))]]);

    mock.expect_query(
        "SELECT COLUMN_NAME, DATA_TYPE, IS_NULLABLE, (SELECT COUNT(*) FROM INFORMATION_SCHEMA.KEY_COLUMN_USAGE kcu JOIN INFORMATION_SCHEMA.TABLE_CONSTRAINTS tc ON kcu.CONSTRAINT_NAME = tc.CONSTRAINT_NAME WHERE kcu.TABLE_NAME = 'Customers' AND tc.CONSTRAINT_TYPE = 'PRIMARY KEY' AND kcu.COLUMN_NAME = c.COLUMN_NAME) as IS_PK FROM INFORMATION_SCHEMA.COLUMNS c WHERE TABLE_NAME = 'Customers' ORDER BY ORDINAL_POSITION",
    )
    .with_rows(vec![
        vec![
            ("COLUMN_NAME", Value::from("CustomerID")),
            ("DATA_TYPE", Value::from("int")),
            ("IS_NULLABLE", Value::from("NO")),
            ("IS_PK", Value::from(1i64)),
        ],
        vec![
            ("COLUMN_NAME", Value::from("CompanyName")),
            ("DATA_TYPE", Value::from("nvarchar")),
            ("IS_NULLABLE", Value::from("YES")),
            ("IS_PK", Value::from(0i64)),
        ],
    ]);

    let extractor = MssqlSchemaExtractor;
    let schema = extractor.extract_schema(&mut mock).await.unwrap();

    assert_eq!(schema.tables.len(), 1);
    assert_eq!(schema.tables[0].name, "Customers");
    assert_eq!(schema.tables[0].columns.len(), 2);
    assert!(schema.tables[0].columns[0].is_primary_key);
}

#[tokio::test]
async fn test_create_extractor_by_dialect() {
    let mysql_extractor = create_extractor(DbType::MySQL);
    let pg_extractor = create_extractor(DbType::PostgreSQL);
    let sqlite_extractor = create_extractor(DbType::Sqlite);
    let oracle_extractor = create_extractor(DbType::Oracle);
    let mssql_extractor = create_extractor(DbType::SqlServer);

    let mut mock = MockConnection::new();
    let mysql_schema = mysql_extractor.extract_schema(&mut mock).await.unwrap();
    let pg_schema = pg_extractor.extract_schema(&mut mock).await.unwrap();
    let sqlite_schema = sqlite_extractor.extract_schema(&mut mock).await.unwrap();
    let oracle_schema = oracle_extractor.extract_schema(&mut mock).await.unwrap();
    let mssql_schema = mssql_extractor.extract_schema(&mut mock).await.unwrap();

    assert!(mysql_schema.tables.is_empty());
    assert!(pg_schema.tables.is_empty());
    assert!(sqlite_schema.tables.is_empty());
    assert!(oracle_schema.tables.is_empty());
    assert!(mssql_schema.tables.is_empty());
}

#[tokio::test]
async fn test_empty_database() {
    let mut mock = MockConnection::new();
    mock.expect_query(
        "SELECT TABLE_NAME FROM information_schema.TABLES WHERE TABLE_SCHEMA = DATABASE() ORDER BY TABLE_NAME",
    )
    .with_rows(vec![]);

    let extractor = MySqlSchemaExtractor;
    let schema = extractor.extract_schema(&mut mock).await.unwrap();
    assert!(schema.tables.is_empty());
}
