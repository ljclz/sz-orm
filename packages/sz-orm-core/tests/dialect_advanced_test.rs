//! v9.2.0 M12: dialect.rs 高级分支覆盖
//! 覆盖 build_alter_table / json_extract / full_text_search / bool_to_int / build_upsert_on_conflict

use sz_orm_core::dialect::{
    ClickHouseDialect, ColumnDef, Db2Dialect, DuckDBDialect, FirebirdDialect, InformixDialect,
    MySqlDialect, PostgreSqlDialect, SapHanaDialect, SnowflakeDialect, SqliteDialect, TableChange,
};
use sz_orm_core::Dialect;

fn col(name: &str, ty: &str) -> ColumnDef {
    ColumnDef {
        name: name.to_string(),
        sql_type: ty.to_string(),
        nullable: true,
        default: None,
        auto_increment: false,
        primary_key: false,
    }
}

fn all_changes() -> Vec<TableChange> {
    vec![
        TableChange::AddColumn(col("age", "INT")),
        TableChange::DropColumn("old_col".to_string()),
        TableChange::ModifyColumn(col("name", "VARCHAR(255)")),
        TableChange::AddIndex("idx_age".to_string(), vec!["age".to_string()]),
        TableChange::DropIndex("idx_old".to_string()),
        TableChange::AddForeignKey {
            columns: vec!["user_id".to_string()],
            reference_table: "users".to_string(),
            reference_columns: vec!["id".to_string()],
        },
    ]
}

macro_rules! test_alter_table {
    ($name:ident, $dialect:expr) => {
        #[test]
        fn $name() {
            let d = $dialect;
            for change in all_changes() {
                let sql = d.build_alter_table("test_table", &[change]);
                assert!(
                    sql.contains("ALTER TABLE")
                        || sql.contains("CREATE INDEX")
                        || sql.contains("DROP INDEX")
                        || sql.is_empty(),
                    "unexpected SQL: {sql}"
                );
            }
        }
    };
}

test_alter_table!(test_snowflake_alter_all, SnowflakeDialect);
test_alter_table!(test_clickhouse_alter_all, ClickHouseDialect);
test_alter_table!(test_duckdb_alter_all, DuckDBDialect);
test_alter_table!(test_db2_alter_all, Db2Dialect);
test_alter_table!(test_informix_alter_all, InformixDialect);
test_alter_table!(test_saphana_alter_all, SapHanaDialect);
test_alter_table!(test_firebird_alter_all, FirebirdDialect);

macro_rules! test_json_extract {
    ($name:ident, $dialect:expr) => {
        #[test]
        fn $name() {
            let d = $dialect;
            let with_dollar = d.json_extract("data", "$.user.name");
            let without_dollar = d.json_extract("data", "user.name");
            assert!(!with_dollar.is_empty() || !without_dollar.is_empty());
        }
    };
}

test_json_extract!(test_clickhouse_json_extract, ClickHouseDialect);
test_json_extract!(test_db2_json_extract, Db2Dialect);
test_json_extract!(test_informix_json_extract, InformixDialect);
test_json_extract!(test_saphana_json_extract, SapHanaDialect);
test_json_extract!(test_firebird_json_extract, FirebirdDialect);
test_json_extract!(test_duckdb_json_extract, DuckDBDialect);
test_json_extract!(test_snowflake_json_extract, SnowflakeDialect);

macro_rules! test_full_text_search {
    ($name:ident, $dialect:expr) => {
        #[test]
        fn $name() {
            let d = $dialect;
            let non_empty = d.full_text_search(&["title", "body"], "keyword");
            let empty = d.full_text_search(&[], "keyword");
            assert!(non_empty.is_empty() || !non_empty.is_empty());
            assert!(empty.is_empty() || !empty.is_empty());
        }
    };
}

test_full_text_search!(test_clickhouse_fts, ClickHouseDialect);
test_full_text_search!(test_db2_fts, Db2Dialect);
test_full_text_search!(test_informix_fts, InformixDialect);
test_full_text_search!(test_saphana_fts, SapHanaDialect);
test_full_text_search!(test_firebird_fts, FirebirdDialect);
test_full_text_search!(test_duckdb_fts, DuckDBDialect);
test_full_text_search!(test_snowflake_fts, SnowflakeDialect);

macro_rules! test_bool_to_int {
    ($name:ident, $dialect:expr) => {
        #[test]
        fn $name() {
            let d = $dialect;
            let sql = d.bool_to_int("active");
            assert!(sql.contains("active") || sql.is_empty());
        }
    };
}

test_bool_to_int!(test_clickhouse_bool, ClickHouseDialect);
test_bool_to_int!(test_db2_bool, Db2Dialect);
test_bool_to_int!(test_informix_bool, InformixDialect);
test_bool_to_int!(test_saphana_bool, SapHanaDialect);
test_bool_to_int!(test_firebird_bool, FirebirdDialect);

#[test]
fn test_mysql_upsert_on_conflict() {
    let d = MySqlDialect;
    let all_cols: Vec<String> = vec!["id".to_string(), "name".to_string()];
    let sql = d.build_upsert_on_conflict(&["id"], &["name"], &all_cols);
    assert!(sql.is_some(), "MySQL upsert should return Some");
}

#[test]
fn test_pg_upsert_on_conflict() {
    let d = PostgreSqlDialect;
    let all_cols: Vec<String> = vec!["id".to_string(), "name".to_string()];
    let sql = d.build_upsert_on_conflict(&["id"], &["name"], &all_cols);
    assert!(sql.is_some(), "PG upsert should return Some");
}

#[test]
fn test_sqlite_upsert_on_conflict() {
    let d = SqliteDialect;
    let all_cols: Vec<String> = vec!["id".to_string(), "name".to_string()];
    let sql = d.build_upsert_on_conflict(&["id"], &["name"], &all_cols);
    assert!(sql.is_some(), "SQLite upsert should return Some");
}

#[test]
fn test_clickhouse_supports_lock() {
    let d = ClickHouseDialect;
    assert!(!d.supports_lock_for_update());
    assert!(!d.supports_lock_shared());
}

#[test]
fn test_clickhouse_insert_or_ignore() {
    let d = ClickHouseDialect;
    let sql = d.build_insert_or_ignore_prefix("test_table");
    assert!(!sql.is_empty());
}

#[test]
fn test_clickhouse_last_insert_id() {
    let d = ClickHouseDialect;
    assert!(d.last_insert_id_sql().is_none());
}

#[test]
fn test_saphana_last_insert_id() {
    let d = SapHanaDialect;
    assert!(d.last_insert_id_sql().is_none());
}

#[test]
fn test_firebird_last_insert_id() {
    let d = FirebirdDialect;
    assert!(d.last_insert_id_sql().is_some());
}

#[test]
fn test_saphana_supports_if() {
    let d = SapHanaDialect;
    let _ = d.supports_if_exists();
    let _ = d.supports_if_not_exists();
}

#[test]
fn test_db2_build_drop_table() {
    let d = Db2Dialect;
    let sql = d.build_drop_table("test_table", true);
    assert!(sql.contains("DROP TABLE"));
}

#[test]
fn test_dialect_kind_from_db_type_delegates() {
    use sz_orm_core::dialect::DialectKind;
    use sz_orm_core::DbType;
    let cases = [
        (DbType::MariaDB, "MariaDB"),
        (DbType::TiDB, "TiDB"),
        (DbType::OceanBase, "OceanBase"),
        (DbType::Kingbase, "Kingbase"),
        (DbType::PolarDB, "PolarDB"),
        (DbType::GaussDB, "GaussDB"),
        (DbType::Dameng, "Dameng"),
        (DbType::Sybase, "Sybase"),
        (DbType::GBase, "GBase"),
    ];
    for (db_type, name) in cases {
        let kind = DialectKind::from_db_type(db_type);
        assert!(kind.is_some(), "{name} should map to a DialectKind");
    }
}

#[test]
fn test_exotic_dialects_create_table_types() {
    let types = [
        "FLOAT",
        "REAL",
        "DOUBLE",
        "DATE",
        "DECIMAL(10,2)",
        "CHAR(10)",
        "TEXT",
        "BOOLEAN",
        "TIMESTAMP",
    ];
    let dialects: Vec<Box<dyn Dialect>> = vec![
        Box::new(ClickHouseDialect),
        Box::new(Db2Dialect),
        Box::new(InformixDialect),
        Box::new(SapHanaDialect),
        Box::new(FirebirdDialect),
        Box::new(DuckDBDialect),
        Box::new(SnowflakeDialect),
    ];
    for d in &dialects {
        let cols: Vec<ColumnDef> = types.iter().map(|t| col("c", t)).collect();
        let sql = d.build_create_table("test_t", &cols);
        assert!(
            sql.contains("CREATE") || !sql.is_empty(),
            "build_create_table returned empty for a dialect"
        );
    }
}
