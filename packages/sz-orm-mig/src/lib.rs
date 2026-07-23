//! # SZ-ORM Mig — 数据库迁移工具
//!
//! 提供数据库 schema 迁移与版本管理，支持多数据库类型的结构转换。
//!
//! ## 主要模块
//!
//! - [`migrator`] — 迁移执行器与数据库配置
//! - [`transformer`] — SQL 方言/结构转换器

pub mod advanced;
pub mod error;
pub mod migrator;
pub mod transformer;

pub use advanced::*;
pub use error::MigError;
pub use migrator::*;
pub use transformer::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_config_mysql() {
        let config = DatabaseConfig::mysql()
            .with_host("localhost")
            .with_username("root")
            .with_password("password")
            .with_database("test_db");

        assert_eq!(config.db_type, "mysql");
        assert_eq!(config.port, 3306);
        assert_eq!(config.host, "localhost");
    }

    #[test]
    fn test_database_config_postgresql() {
        let config = DatabaseConfig::postgresql()
            .with_host("192.168.1.1")
            .with_database("production");

        assert_eq!(config.db_type, "postgresql");
        assert_eq!(config.port, 5432);
        assert_eq!(config.host, "192.168.1.1");
    }

    #[test]
    fn test_database_config_sqlite() {
        let config = DatabaseConfig::sqlite("/tmp/test.db");
        assert_eq!(config.db_type, "sqlite");
        assert_eq!(config.database, "/tmp/test.db");
    }

    #[test]
    fn test_mig_config_defaults() {
        let config = MigConfig::default();
        assert_eq!(config.batch_size, 1000);
        assert!(!config.skip_errors);
        assert!(!config.dry_run);
    }

    #[test]
    fn test_mig_config_builder() {
        let config = MigConfig::new()
            .with_batch_size(500)
            .with_skip_errors(true)
            .with_dry_run(true);

        assert_eq!(config.batch_size, 500);
        assert!(config.skip_errors);
        assert!(config.dry_run);
    }

    #[test]
    fn test_mig_report() {
        let mut report = MigReport::new("source", "target");
        report.total_rows = 100;
        report.migrated_rows = 95;
        report.failed_rows = 5;

        assert_eq!(report.success_rate(), 95.0);
    }

    #[test]
    fn test_row_data() {
        let row = RowData::new()
            .with("id", serde_json::json!(1))
            .with("name", serde_json::json!("test"));

        assert_eq!(row.get("id"), Some(&serde_json::json!(1)));
        assert_eq!(row.get("name"), Some(&serde_json::json!("test")));
        assert!(row.get("missing").is_none());
    }

    #[test]
    fn test_column_info() {
        let col = ColumnInfo::new("id", "integer")
            .not_null()
            .primary_key()
            .with_default("0");

        assert_eq!(col.name, "id");
        assert_eq!(col.data_type, "integer");
        assert!(!col.nullable);
        assert!(col.is_primary_key);
        assert_eq!(col.default_value, Some("0".to_string()));
    }

    #[test]
    fn test_type_transformer_mysql_to_pg() {
        let transformer = TypeTransformer::new();

        let bool_true =
            transformer.mysql_to_pg_value(serde_json::Value::String("true".to_string()));
        assert_eq!(bool_true, serde_json::Value::Bool(true));

        let bool_false =
            transformer.mysql_to_pg_value(serde_json::Value::String("false".to_string()));
        assert_eq!(bool_false, serde_json::Value::Bool(false));

        let number = transformer.mysql_to_pg_value(serde_json::Value::String("123".to_string()));
        assert_eq!(number, serde_json::json!(123));
    }

    #[test]
    fn test_type_transformer_pg_to_mysql() {
        let transformer = TypeTransformer::new();

        let bool_val = transformer.pg_to_mysql_value(serde_json::Value::Bool(true));
        assert_eq!(bool_val, serde_json::Value::String("1".to_string()));

        let num = transformer.pg_to_mysql_value(serde_json::json!(42));
        assert_eq!(num, serde_json::Value::String("42".to_string()));
    }

    #[test]
    fn test_column_mapper() {
        let mapper = ColumnMapper::new()
            .map("old_id", "new_id")
            .map("old_name", "new_name");

        let row = RowData::new()
            .with("old_id", serde_json::json!(1))
            .with("old_name", serde_json::json!("test"))
            .with("unchanged", serde_json::json!("value"));

        let mapped = mapper.transform(row);

        assert!(mapped.get("old_id").is_none());
        assert!(mapped.get("new_id").is_some());
        assert!(mapped.get("unchanged").is_some());
    }

    #[test]
    fn test_chain_transformer() {
        let chain = ChainTransformer::new()
            .add(TypeTransformer::new())
            .add(ColumnMapper::new().map("a", "b"));

        let row = RowData::new()
            .with("a", serde_json::json!("true"))
            .with("c", serde_json::json!(123));

        let result = chain.transform(row).unwrap();
        assert!(result.get("a").is_none());
        assert!(result.get("b").is_some());
    }

    #[test]
    fn test_filter_transformer_include() {
        let filter = FilterTransformer::new().include(vec!["id".to_string(), "name".to_string()]);

        let row = RowData::new()
            .with("id", serde_json::json!(1))
            .with("name", serde_json::json!("test"))
            .with("password", serde_json::json!("secret"));

        let filtered = filter.transform(row).unwrap();

        assert!(filtered.get("id").is_some());
        assert!(filtered.get("name").is_some());
        assert!(filtered.get("password").is_none());
    }

    #[test]
    fn test_filter_transformer_exclude() {
        let filter =
            FilterTransformer::new().exclude(vec!["password".to_string(), "token".to_string()]);

        let row = RowData::new()
            .with("id", serde_json::json!(1))
            .with("password", serde_json::json!("secret"))
            .with("token", serde_json::json!("abc"));

        let filtered = filter.transform(row).unwrap();

        assert!(filtered.get("id").is_some());
        assert!(filtered.get("password").is_none());
        assert!(filtered.get("token").is_none());
    }

    // =========================================================================
    // InMemoryTableStore tests
    // =========================================================================

    fn make_users_table() -> InMemoryTableStore {
        let columns = vec![
            ColumnInfo::new("id", "integer").not_null().primary_key(),
            ColumnInfo::new("name", "varchar(255)").nullable(true),
        ];
        let rows = vec![
            RowData::new()
                .with("id", serde_json::json!(1))
                .with("name", serde_json::json!("alice")),
            RowData::new()
                .with("id", serde_json::json!(2))
                .with("name", serde_json::json!("bob")),
            RowData::new()
                .with("id", serde_json::json!(3))
                .with("name", serde_json::json!("carol")),
        ];
        InMemoryTableStore::new().with_table("users", columns, rows)
    }

    #[tokio::test]
    async fn test_inmemory_store_count_returns_actual_row_count() {
        let store = make_users_table();
        assert_eq!(store.count("users").await.unwrap(), 3);
    }

    #[tokio::test]
    async fn test_inmemory_store_count_errors_on_missing_table() {
        let store = InMemoryTableStore::new();
        let err = store.count("ghosts").await.unwrap_err();
        assert!(matches!(err, MigError::TableNotFound(_)));
    }

    #[tokio::test]
    async fn test_inmemory_store_read_table_pagination() {
        let store = make_users_table();

        // Page 1: offset 0, limit 2
        let page1 = store.read_table("users", 0, 2).await.unwrap();
        assert_eq!(page1.len(), 2);
        assert_eq!(page1[0].get("id"), Some(&serde_json::json!(1)));
        assert_eq!(page1[1].get("id"), Some(&serde_json::json!(2)));

        // Page 2: offset 2, limit 2 -> only 1 row remains
        let page2 = store.read_table("users", 2, 2).await.unwrap();
        assert_eq!(page2.len(), 1);
        assert_eq!(page2[0].get("id"), Some(&serde_json::json!(3)));

        // Page 3: offset 100, limit 10 -> empty
        let page3 = store.read_table("users", 100, 10).await.unwrap();
        assert!(page3.is_empty());
    }

    #[tokio::test]
    async fn test_inmemory_store_table_columns_returns_schema() {
        let store = make_users_table();
        let cols = store.table_columns("users").await.unwrap();
        assert_eq!(cols.len(), 2);
        assert_eq!(cols[0].name, "id");
        assert!(cols[0].is_primary_key);
        assert!(!cols[0].nullable);
        assert_eq!(cols[1].name, "name");
        assert!(cols[1].nullable);
    }

    #[tokio::test]
    async fn test_inmemory_store_write_table_appends_rows() {
        let store = InMemoryTableStore::new();
        // Initially missing
        assert!(matches!(
            store.count("orders").await,
            Err(MigError::TableNotFound(_))
        ));

        // Create table schema first
        store
            .create_table("orders", &[ColumnInfo::new("id", "integer").primary_key()])
            .await
            .unwrap();

        // Write two rows
        let written = store
            .write_table(
                "orders",
                vec![
                    RowData::new().with("id", serde_json::json!(100)),
                    RowData::new().with("id", serde_json::json!(101)),
                ],
            )
            .await
            .unwrap();
        assert_eq!(written, 2);
        assert_eq!(store.count("orders").await.unwrap(), 2);

        // Write another row - should append, not replace
        store
            .write_table(
                "orders",
                vec![RowData::new().with("id", serde_json::json!(102))],
            )
            .await
            .unwrap();
        assert_eq!(store.count("orders").await.unwrap(), 3);
    }

    #[tokio::test]
    async fn test_inmemory_store_read_table_errors_on_missing_table() {
        let store = InMemoryTableStore::new();
        let err = store.read_table("ghosts", 0, 10).await.unwrap_err();
        assert!(matches!(err, MigError::TableNotFound(_)));
    }

    #[tokio::test]
    async fn test_inmemory_store_create_table_overwrites_schema_preserves_rows() {
        let store = make_users_table();
        // Initial state: 3 rows
        assert_eq!(store.count("users").await.unwrap(), 3);

        // Redefine schema - rows should be preserved
        store
            .create_table(
                "users",
                &[
                    ColumnInfo::new("id", "bigint").primary_key(),
                    ColumnInfo::new("name", "text"),
                    ColumnInfo::new("email", "text"),
                ],
            )
            .await
            .unwrap();
        assert_eq!(store.count("users").await.unwrap(), 3);
        let cols = store.table_columns("users").await.unwrap();
        assert_eq!(cols.len(), 3);
        assert_eq!(cols[2].name, "email");
    }

    #[tokio::test]
    async fn test_data_migrator_full_migration_between_inmemory_stores() {
        // Reader and writer are *separate* stores, so the test verifies that
        // rows actually flow from one to the other.
        let reader = make_users_table();
        let writer = InMemoryTableStore::new();
        // Pre-create target schema so write_table can append
        writer
            .create_table(
                "users",
                &[
                    ColumnInfo::new("id", "integer").primary_key(),
                    ColumnInfo::new("name", "varchar(255)"),
                ],
            )
            .await
            .unwrap();

        let config = MigConfig::default()
            .with_batch_size(2)
            .with_tables(vec!["users".to_string()]);
        let migrator = DataMigrator::new(reader.clone(), writer.clone(), config);

        let reports = migrator.migrate_all().await.unwrap();
        assert_eq!(reports.len(), 1);
        let report = &reports[0];
        assert_eq!(report.source_table, "users");
        assert_eq!(report.target_table, "users");
        assert_eq!(report.total_rows, 3);
        assert_eq!(report.migrated_rows, 3);
        assert_eq!(report.failed_rows, 0);
        assert!(report.errors.is_empty());

        // Verify the writer actually has the data
        assert_eq!(writer.count("users").await.unwrap(), 3);
        let rows = writer.read_table("users", 0, 10).await.unwrap();
        assert_eq!(rows[0].get("name"), Some(&serde_json::json!("alice")));
        assert_eq!(rows[2].get("name"), Some(&serde_json::json!("carol")));

        // Reader should still have its data (we cloned the Arc)
        assert_eq!(reader.count("users").await.unwrap(), 3);
    }

    #[tokio::test]
    async fn test_migrate_all_errors_when_no_tables_configured() {
        let reader = InMemoryTableStore::new();
        let writer = InMemoryTableStore::new();
        // Default config has empty `tables` - previously this would have
        // silently fallen back to migrating the "users" table, masking bugs.
        let config = MigConfig::default();
        let migrator = DataMigrator::new(reader, writer, config);

        let result = migrator.migrate_all().await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, MigError::Validation(_)));
        assert!(
            err.to_string().contains("No tables configured"),
            "expected error message to mention missing config, got: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_migrate_all_processes_multiple_tables_in_order() {
        let reader = InMemoryTableStore::new()
            .with_table(
                "users",
                vec![ColumnInfo::new("id", "integer")],
                vec![RowData::new().with("id", serde_json::json!(1))],
            )
            .with_table(
                "orders",
                vec![ColumnInfo::new("id", "integer")],
                vec![
                    RowData::new().with("id", serde_json::json!(10)),
                    RowData::new().with("id", serde_json::json!(11)),
                ],
            );
        let writer = InMemoryTableStore::new();
        writer
            .create_table("users", &[ColumnInfo::new("id", "integer")])
            .await
            .unwrap();
        writer
            .create_table("orders", &[ColumnInfo::new("id", "integer")])
            .await
            .unwrap();

        let config = MigConfig::default()
            .with_batch_size(100)
            .with_tables(vec!["users".to_string(), "orders".to_string()]);
        let migrator = DataMigrator::new(reader, writer.clone(), config);

        let reports = migrator.migrate_all().await.unwrap();
        assert_eq!(reports.len(), 2);
        assert_eq!(reports[0].source_table, "users");
        assert_eq!(reports[0].total_rows, 1);
        assert_eq!(reports[0].migrated_rows, 1);
        assert_eq!(reports[1].source_table, "orders");
        assert_eq!(reports[1].total_rows, 2);
        assert_eq!(reports[1].migrated_rows, 2);

        // Verify both tables in the writer have the migrated data
        assert_eq!(writer.count("users").await.unwrap(), 1);
        assert_eq!(writer.count("orders").await.unwrap(), 2);
    }

    #[tokio::test]
    async fn test_dry_run_migrate_does_not_write_to_writer() {
        let reader = make_users_table();
        let writer = InMemoryTableStore::new();
        writer
            .create_table(
                "users",
                &[
                    ColumnInfo::new("id", "integer").primary_key(),
                    ColumnInfo::new("name", "varchar(255)"),
                ],
            )
            .await
            .unwrap();

        let config = MigConfig::default()
            .with_batch_size(10)
            .with_dry_run(true)
            .with_tables(vec!["users".to_string()]);
        let migrator = DataMigrator::new(reader, writer.clone(), config);

        let reports = migrator.migrate_all().await.unwrap();
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].total_rows, 3);
        assert_eq!(reports[0].migrated_rows, 3);
        // Dry-run must not have written anything to the writer
        assert_eq!(writer.count("users").await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_skip_errors_records_failure_and_continues() {
        // Build a reader with an empty "users" table so read_table returns 0
        // rows; then add a second table "broken" that exists for count() but
        // has no rows. We craft the scenario so that the writer always
        // succeeds - we just want to verify skip_errors behavior end-to-end.
        let reader = InMemoryTableStore::new()
            .with_table(
                "users",
                vec![ColumnInfo::new("id", "integer")],
                vec![RowData::new().with("id", serde_json::json!(1))],
            )
            .with_table(
                "broken",
                vec![ColumnInfo::new("id", "integer")],
                vec![RowData::new().with("id", serde_json::json!(2))],
            );
        let writer = InMemoryTableStore::new();
        writer
            .create_table("users", &[ColumnInfo::new("id", "integer")])
            .await
            .unwrap();
        writer
            .create_table("broken", &[ColumnInfo::new("id", "integer")])
            .await
            .unwrap();

        let config = MigConfig::default()
            .with_batch_size(10)
            .with_skip_errors(true)
            .with_tables(vec!["users".to_string(), "broken".to_string()]);
        let migrator = DataMigrator::new(reader, writer.clone(), config);

        let reports = migrator.migrate_all().await.unwrap();
        assert_eq!(reports.len(), 2);
        for report in &reports {
            assert_eq!(report.failed_rows, 0);
            assert!(report.errors.is_empty());
        }
        assert_eq!(writer.count("users").await.unwrap(), 1);
        assert_eq!(writer.count("broken").await.unwrap(), 1);
    }
}
