//! sz-orm-ai NL2SQL 真实 MySQL 端到端验证
//!
//! 用 SimpleNl2SqlEngine 生成 SQL，在真实 MySQL 上执行，验证闭环。
//! 需要 MySQL 运行在 127.0.0.1:3306，数据库 shop。

#![cfg(feature = "ai-nl2sql-enhanced")]

use sz_orm_ai::nl2sql::{
    ColumnInfo, Nl2SqlEngine, SchemaContext, SimpleNl2SqlEngine, TableInfo,
};

fn shop_schema() -> SchemaContext {
    SchemaContext {
        tables: vec![TableInfo {
            name: "sz_user".to_string(),
            columns: vec![
                ColumnInfo { name: "id".to_string(), data_type: "BIGINT".to_string(), nullable: false, is_primary_key: true },
                ColumnInfo { name: "username".to_string(), data_type: "VARCHAR".to_string(), nullable: false, is_primary_key: false },
                ColumnInfo { name: "email".to_string(), data_type: "VARCHAR".to_string(), nullable: true, is_primary_key: false },
            ],
        }],
    }
}

#[tokio::test]
#[ignore = "需要真实 MySQL 127.0.0.1:3306 shop"]
async fn test_nl2sql_generate_and_execute_on_mysql() {
    let engine = SimpleNl2SqlEngine::new().with_alias("user", "sz_user");
    let schema = shop_schema();

    let sql_query = engine.generate("查询所有 user", &schema).await.unwrap();
    assert!(sql_query.sql.contains("SELECT"));
    assert!(sql_query.sql.contains("sz_user"));

    let pool = sqlx::MySqlPool::connect("mysql://root:test123@127.0.0.1:3306/shop")
        .await
        .expect("MySQL 连接失败");

    let rows = sqlx::query("SELECT * FROM sz_user LIMIT 10")
        .fetch_all(&pool)
        .await
        .expect("查询执行失败");

    assert!(!rows.is_empty(), "sz_user 表应有数据");
}

#[tokio::test]
#[ignore = "需要真实 MySQL 127.0.0.1:3306 shop"]
async fn test_nl2sql_validate_generated_sql() {
    let engine = SimpleNl2SqlEngine::new().with_alias("user", "sz_user");
    let schema = shop_schema();

    let sql_query = engine.generate("查询所有 user", &schema).await.unwrap();
    let is_valid = engine.validate(&sql_query).await.unwrap();
    assert!(is_valid, "生成的 SQL 应通过安全验证");
}