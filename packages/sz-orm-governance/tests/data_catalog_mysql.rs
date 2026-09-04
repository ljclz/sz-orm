//! sz-orm-governance 数据目录真实 MySQL 端到端验证
//!
//! 从真实 MySQL information_schema 获取表结构，用 DataCatalogBuilder 生成数据目录。
//! 需要 MySQL 运行在 127.0.0.1:3306，数据库 shop。

#![cfg(feature = "governance")]

use sz_orm_governance::data_catalog::DataCatalogBuilder;
use sqlx::Row;

#[tokio::test]
#[ignore = "需要真实 MySQL 127.0.0.1:3306 shop"]
async fn test_data_catalog_from_mysql_schema() {
    let pool = sqlx::MySqlPool::connect("mysql://root:test123@127.0.0.1:3306/shop")
        .await
        .expect("MySQL 连接失败");

    let rows = sqlx::query("SELECT COLUMN_NAME, DATA_TYPE FROM information_schema.columns WHERE TABLE_SCHEMA = 'shop' AND TABLE_NAME = 'sz_user' ORDER BY ORDINAL_POSITION")
        .fetch_all(&pool)
        .await
        .expect("查询 information_schema 失败");

    assert!(!rows.is_empty(), "sz_user 表应有列");

    let mut columns: Vec<(&str, &str)> = Vec::new();
    let mut name_bufs: Vec<String> = Vec::new();
    let mut type_bufs: Vec<String> = Vec::new();
    for row in &rows {
        let name: String = row.try_get(0).expect("获取列名失败");
        let dtype: String = row.try_get(1).expect("获取数据类型失败");
        name_bufs.push(name);
        type_bufs.push(dtype);
    }
    for (n, t) in name_bufs.iter().zip(type_bufs.iter()) {
        columns.push((n.as_str(), t.as_str()));
    }

    let catalog = DataCatalogBuilder::new().build("sz_user", &columns);

    assert_eq!(catalog.table, "sz_user");
    assert!(!catalog.columns.is_empty());
    assert!(catalog.quality_score > 0.0);
}

#[tokio::test]
#[ignore = "需要真实 MySQL 127.0.0.1:3306 shop"]
async fn test_data_catalog_multiple_tables() {
    let pool = sqlx::MySqlPool::connect("mysql://root:test123@127.0.0.1:3306/shop")
        .await
        .expect("MySQL 连接失败");

    let table_rows = sqlx::query("SELECT TABLE_NAME FROM information_schema.tables WHERE TABLE_SCHEMA = 'shop' LIMIT 5")
        .fetch_all(&pool)
        .await
        .expect("查询表列表失败");

    assert!(!table_rows.is_empty(), "shop 库应有表");

    for row in &table_rows {
        let table_name: String = row.try_get(0).expect("获取表名失败");
        let sql = format!(
            "SELECT COLUMN_NAME, DATA_TYPE FROM information_schema.columns WHERE TABLE_SCHEMA = 'shop' AND TABLE_NAME = '{table_name}' ORDER BY ORDINAL_POSITION LIMIT 10"
        );
        let col_rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .fetch_all(&pool)
            .await
            .expect("查询列信息失败");

        if col_rows.is_empty() {
            continue;
        }

        let mut columns: Vec<(&str, &str)> = Vec::new();
        let mut name_bufs: Vec<String> = Vec::new();
        let mut type_bufs: Vec<String> = Vec::new();
        for r in &col_rows {
            let name: String = r.try_get(0).expect("获取列名失败");
            let dtype: String = r.try_get(1).expect("获取数据类型失败");
            name_bufs.push(name);
            type_bufs.push(dtype);
        }
        for (n, t) in name_bufs.iter().zip(type_bufs.iter()) {
            columns.push((n.as_str(), t.as_str()));
        }

        let catalog = DataCatalogBuilder::new().build(&table_name, &columns);
        assert_eq!(catalog.table, table_name);
        assert!(!catalog.columns.is_empty());
    }
}
