//! sz-orm-multimodal ER 图真实 MySQL 端到端验证
//!
//! 从真实 MySQL information_schema 获取表结构，构建 ER 图，生成 DDL。
//! 需要 MySQL 运行在 127.0.0.1:3306，数据库 shop。

#![cfg(feature = "multimodal-er")]

use sqlx::Row;
use sz_orm_multimodal::er_diagram::{Entity, ErDiagram, ErDiagramInteractor, Field};

#[tokio::test]
#[ignore = "需要真实 MySQL 127.0.0.1:3306 shop"]
async fn test_er_diagram_from_mysql_schema() {
    let pool = sqlx::MySqlPool::connect("mysql://root:test123@127.0.0.1:3306/shop")
        .await
        .expect("MySQL 连接失败");

    let rows = sqlx::query("SELECT TABLE_NAME, COLUMN_NAME, DATA_TYPE, COLUMN_KEY FROM information_schema.columns WHERE TABLE_SCHEMA = 'shop' AND TABLE_NAME = 'sz_user' ORDER BY ORDINAL_POSITION")
        .fetch_all(&pool)
        .await
        .expect("查询 information_schema 失败");

    assert!(!rows.is_empty(), "sz_user 表应有列");

    let mut fields = Vec::new();
    for row in &rows {
        let name: String = row.try_get(1).expect("获取列名失败");
        let dtype: String = row.try_get(2).expect("获取数据类型失败");
        let column_key: String = row.try_get(3).unwrap_or_default();
        let is_pk = column_key == "PRI";
        fields.push(Field {
            name,
            data_type: dtype,
            is_primary_key: is_pk,
            is_foreign_key: false,
            references: None,
        });
    }

    let entity = Entity {
        name: "sz_user".to_string(),
        fields,
    };
    let diagram = ErDiagram {
        entities: vec![entity],
        relationships: vec![],
    };

    let interactor = ErDiagramInteractor::new();
    let ddl = interactor.to_ddl(&diagram);

    assert!(ddl.contains("CREATE TABLE sz_user"));
    assert!(ddl.contains("id"));
}

#[tokio::test]
#[ignore = "需要真实 MySQL 127.0.0.1:3306 shop"]
async fn test_er_diagram_multiple_tables_from_mysql() {
    let pool = sqlx::MySqlPool::connect("mysql://root:test123@127.0.0.1:3306/shop")
        .await
        .expect("MySQL 连接失败");

    let table_rows = sqlx::query("SELECT TABLE_NAME FROM information_schema.tables WHERE TABLE_SCHEMA = 'shop' LIMIT 3")
        .fetch_all(&pool)
        .await
        .expect("查询表列表失败");

    assert!(!table_rows.is_empty(), "shop 库应有表");

    let mut entities = Vec::new();
    for row in &table_rows {
        let table_name: String = row.try_get(0).expect("获取表名失败");
        let sql = format!(
            "SELECT COLUMN_NAME, DATA_TYPE, COLUMN_KEY FROM information_schema.columns WHERE TABLE_SCHEMA = 'shop' AND TABLE_NAME = '{table_name}' ORDER BY ORDINAL_POSITION LIMIT 20"
        );
        let col_rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .fetch_all(&pool)
            .await
            .expect("查询列信息失败");

        if col_rows.is_empty() {
            continue;
        }

        let mut fields = Vec::new();
        for r in &col_rows {
            let name: String = r.try_get(0).expect("获取列名失败");
            let dtype: String = r.try_get(1).expect("获取数据类型失败");
            let column_key: String = r.try_get(2).unwrap_or_default();
            let is_pk = column_key == "PRI";
            fields.push(Field {
                name,
                data_type: dtype,
                is_primary_key: is_pk,
                is_foreign_key: false,
                references: None,
            });
        }

        entities.push(Entity {
            name: table_name,
            fields,
        });
    }

    assert!(!entities.is_empty());

    let diagram = ErDiagram {
        entities,
        relationships: vec![],
    };
    let interactor = ErDiagramInteractor::new();
    let ddl = interactor.to_ddl(&diagram);

    assert!(ddl.contains("CREATE TABLE"));
}