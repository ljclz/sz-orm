use sz_orm_core::schema_sync::{
    diff, ColumnDef, MssqlDdlGenerator, MySqlDdlGenerator, OracleDdlGenerator, PgDdlGenerator,
    SchemaDiff, SchemaSync, SqliteDdlGenerator, TableDef, DdlGenerator,
};

fn col(name: &str, sql_type: &str, nullable: bool, pk: bool) -> ColumnDef {
    ColumnDef::new(name, sql_type, nullable, pk, None)
}

fn table(name: &str, cols: Vec<ColumnDef>) -> TableDef {
    TableDef::new(name, cols)
}

#[test]
fn test_column_def_new() {
    let c = ColumnDef::new("id", "BIGINT", false, true, None);
    assert_eq!(c.name, "id");
    assert_eq!(c.sql_type, "BIGINT");
    assert!(!c.nullable);
    assert!(c.primary_key);
    assert!(c.default.is_none());
}

#[test]
fn test_column_def_with_default() {
    let c = ColumnDef::new("status", "INT", false, false, Some("0".to_string()));
    assert_eq!(c.default, Some("0".to_string()));
}

#[test]
fn test_table_def_new() {
    let t = table("users", vec![col("id", "BIGINT", false, true)]);
    assert_eq!(t.name, "users");
    assert_eq!(t.columns.len(), 1);
}

#[test]
fn test_diff_empty() {
    let d = diff(&[], &[]);
    assert!(d.added_tables.is_empty());
    assert!(d.dropped_tables.is_empty());
}

#[test]
fn test_diff_added_table() {
    let entity = vec![table("users", vec![col("id", "BIGINT", false, true)])];
    let d = diff(&entity, &[]);
    assert_eq!(d.added_tables.len(), 1);
    assert_eq!(d.added_tables[0].name, "users");
}

#[test]
fn test_diff_dropped_table() {
    let db = vec![table("old_table", vec![col("id", "BIGINT", false, true)])];
    let d = diff(&[], &db);
    assert_eq!(d.dropped_tables.len(), 1);
    assert_eq!(d.dropped_tables[0], "old_table");
}

#[test]
fn test_diff_added_column() {
    let entity = vec![table("users", vec![
        col("id", "BIGINT", false, true),
        col("email", "VARCHAR(255)", false, false),
    ])];
    let db = vec![table("users", vec![col("id", "BIGINT", false, true)])];
    let d = diff(&entity, &db);
    assert_eq!(d.added_columns.len(), 1);
    assert_eq!(d.added_columns[0].0, "users");
    assert_eq!(d.added_columns[0].1.name, "email");
}

#[test]
fn test_diff_dropped_column() {
    let entity = vec![table("users", vec![col("id", "BIGINT", false, true)])];
    let db = vec![table("users", vec![
        col("id", "BIGINT", false, true),
        col("old_col", "VARCHAR(100)", true, false),
    ])];
    let d = diff(&entity, &db);
    assert_eq!(d.dropped_columns.len(), 1);
    assert_eq!(d.dropped_columns[0].0, "users");
    assert_eq!(d.dropped_columns[0].1, "old_col");
}

#[test]
fn test_diff_type_changed() {
    let entity = vec![table("users", vec![col("id", "BIGINT", false, true)])];
    let db = vec![table("users", vec![col("id", "INT", false, true)])];
    let d = diff(&entity, &db);
    assert_eq!(d.type_changed_columns.len(), 1);
    assert_eq!(d.type_changed_columns[0].0, "users");
    assert_eq!(d.type_changed_columns[0].1.sql_type, "INT");
    assert_eq!(d.type_changed_columns[0].2.sql_type, "BIGINT");
}

#[test]
fn test_diff_nullable_changed() {
    let entity = vec![table("users", vec![col("email", "VARCHAR(255)", false, false)])];
    let db = vec![table("users", vec![col("email", "VARCHAR(255)", true, false)])];
    let d = diff(&entity, &db);
    assert_eq!(d.type_changed_columns.len(), 1);
}

#[test]
fn test_diff_renamed_column() {
    let entity = vec![table("users", vec![
        col("id", "BIGINT", false, true),
        col("email2", "VARCHAR(255)", false, false),
    ])];
    let db = vec![table("users", vec![
        col("id", "BIGINT", false, true),
        col("email", "VARCHAR(255)", false, false),
    ])];
    let d = diff(&entity, &db);
    assert_eq!(d.renamed_columns.len(), 1);
    assert_eq!(d.renamed_columns[0].0, "users");
    assert_eq!(d.renamed_columns[0].1, "email");
    assert_eq!(d.renamed_columns[0].2, "email2");
}

#[test]
fn test_diff_no_change() {
    let entity = vec![table("users", vec![col("id", "BIGINT", false, true)])];
    let db = vec![table("users", vec![col("id", "BIGINT", false, true)])];
    let d = diff(&entity, &db);
    assert!(d.added_tables.is_empty());
    assert!(d.dropped_tables.is_empty());
    assert!(d.added_columns.is_empty());
    assert!(d.dropped_columns.is_empty());
    assert!(d.type_changed_columns.is_empty());
    assert!(d.renamed_columns.is_empty());
}

#[test]
fn test_mysql_ddl_generate_added_table() {
    let diff = SchemaDiff {
        added_tables: vec![table("users", vec![
            col("id", "BIGINT", false, true),
            col("name", "VARCHAR(100)", false, false),
        ])],
        ..Default::default()
    };
    let gen = MySqlDdlGenerator;
    let ddl = gen.generate(&diff).unwrap();
    assert_eq!(ddl.len(), 1);
    assert!(ddl[0].contains("CREATE TABLE users"));
    assert!(ddl[0].contains("id BIGINT NOT NULL PRIMARY KEY"));
    assert!(ddl[0].contains("name VARCHAR(100) NOT NULL"));
}

#[test]
fn test_mysql_ddl_generate_added_column() {
    let diff = SchemaDiff {
        added_columns: vec![("users".to_string(), col("email", "VARCHAR(255)", false, false))],
        ..Default::default()
    };
    let gen = MySqlDdlGenerator;
    let ddl = gen.generate(&diff).unwrap();
    assert_eq!(ddl.len(), 1);
    assert_eq!(ddl[0], "ALTER TABLE users ADD COLUMN email VARCHAR(255) NOT NULL");
}

#[test]
fn test_mysql_ddl_generate_type_change() {
    let diff = SchemaDiff {
        type_changed_columns: vec![(
            "users".to_string(),
            col("id", "INT", false, true),
            col("id", "BIGINT", false, true),
        )],
        ..Default::default()
    };
    let gen = MySqlDdlGenerator;
    let ddl = gen.generate(&diff).unwrap();
    assert_eq!(ddl.len(), 1);
    assert_eq!(ddl[0], "ALTER TABLE users MODIFY COLUMN id BIGINT NOT NULL");
}

#[test]
fn test_mysql_ddl_generate_rename() {
    let diff = SchemaDiff {
        renamed_columns: vec![("users".to_string(), "old_name".to_string(), "new_name".to_string())],
        ..Default::default()
    };
    let gen = MySqlDdlGenerator;
    let ddl = gen.generate(&diff).unwrap();
    assert_eq!(ddl.len(), 1);
    assert_eq!(ddl[0], "ALTER TABLE users RENAME COLUMN old_name TO new_name");
}

#[test]
fn test_pg_ddl_generate_added_column() {
    let diff = SchemaDiff {
        added_columns: vec![("users".to_string(), col("age", "INT", true, false))],
        ..Default::default()
    };
    let gen = PgDdlGenerator;
    let ddl = gen.generate(&diff).unwrap();
    assert_eq!(ddl[0], "ALTER TABLE users ADD COLUMN age INT");
}

#[test]
fn test_pg_ddl_generate_type_change() {
    let diff = SchemaDiff {
        type_changed_columns: vec![(
            "users".to_string(),
            col("age", "INT", false, false),
            col("age", "BIGINT", false, false),
        )],
        ..Default::default()
    };
    let gen = PgDdlGenerator;
    let ddl = gen.generate(&diff).unwrap();
    assert_eq!(ddl[0], "ALTER TABLE users ALTER COLUMN age TYPE BIGINT");
}

#[test]
fn test_sqlite_ddl_generate_added_column_with_default() {
    let diff = SchemaDiff {
        added_columns: vec![("users".to_string(), ColumnDef::new("status", "INT", false, false, Some("0".to_string())))],
        ..Default::default()
    };
    let gen = SqliteDdlGenerator;
    let ddl = gen.generate(&diff).unwrap();
    assert_eq!(ddl[0], "ALTER TABLE users ADD COLUMN status INT DEFAULT 0");
}

#[test]
fn test_sqlite_ddl_generate_added_column_null_default() {
    let diff = SchemaDiff {
        added_columns: vec![("users".to_string(), col("email", "TEXT", true, false))],
        ..Default::default()
    };
    let gen = SqliteDdlGenerator;
    let ddl = gen.generate(&diff).unwrap();
    assert_eq!(ddl[0], "ALTER TABLE users ADD COLUMN email TEXT DEFAULT NULL");
}

#[test]
fn test_sqlite_ddl_type_change_error() {
    let diff = SchemaDiff {
        type_changed_columns: vec![(
            "users".to_string(),
            col("age", "INT", false, false),
            col("age", "BIGINT", false, false),
        )],
        ..Default::default()
    };
    let gen = SqliteDdlGenerator;
    let result = gen.generate(&diff);
    assert!(result.is_err());
}

#[test]
fn test_oracle_ddl_generate_added_column() {
    let diff = SchemaDiff {
        added_columns: vec![("users".to_string(), col("age", "INT", false, false))],
        ..Default::default()
    };
    let gen = OracleDdlGenerator;
    let ddl = gen.generate(&diff).unwrap();
    assert_eq!(ddl[0], "ALTER TABLE users ADD (age INT NOT NULL)");
}

#[test]
fn test_oracle_ddl_generate_type_change() {
    let diff = SchemaDiff {
        type_changed_columns: vec![(
            "users".to_string(),
            col("age", "INT", false, false),
            col("age", "BIGINT", false, false),
        )],
        ..Default::default()
    };
    let gen = OracleDdlGenerator;
    let ddl = gen.generate(&diff).unwrap();
    assert_eq!(ddl[0], "ALTER TABLE users MODIFY (age BIGINT NOT NULL)");
}

#[test]
fn test_mssql_ddl_generate_added_column() {
    let diff = SchemaDiff {
        added_columns: vec![("users".to_string(), col("age", "INT", false, false))],
        ..Default::default()
    };
    let gen = MssqlDdlGenerator;
    let ddl = gen.generate(&diff).unwrap();
    assert_eq!(ddl[0], "ALTER TABLE users ADD age INT NOT NULL");
}

#[test]
fn test_mssql_ddl_generate_rename() {
    let diff = SchemaDiff {
        renamed_columns: vec![("users".to_string(), "old_name".to_string(), "new_name".to_string())],
        ..Default::default()
    };
    let gen = MssqlDdlGenerator;
    let ddl = gen.generate(&diff).unwrap();
    assert_eq!(ddl[0], "EXEC sp_rename 'users.old_name', 'new_name', 'COLUMN'");
}

#[test]
fn test_schema_sync_new() {
    let sync = SchemaSync::new(vec![table("users", vec![col("id", "BIGINT", false, true)])]);
    let _ = sync;
}

#[test]
fn test_schema_sync_with_generator() {
    let sync = SchemaSync::with_generator(
        vec![table("users", vec![col("id", "BIGINT", false, true)])],
        Box::new(PgDdlGenerator),
    );
    let _ = sync;
}

#[test]
fn test_schema_sync_with_rename_threshold() {
    let sync = SchemaSync::new(vec![])
        .with_rename_threshold(3, 0.5);
    let _ = sync;
}

#[test]
fn test_ddl_generate_empty_diff() {
    let diff = SchemaDiff::default();
    let gen = MySqlDdlGenerator;
    let ddl = gen.generate(&diff).unwrap();
    assert!(ddl.is_empty());
}

#[test]
fn test_mysql_ddl_generate_with_default_value() {
    let diff = SchemaDiff {
        added_tables: vec![table("logs", vec![
            ColumnDef::new("level", "INT", false, false, Some("0".to_string())),
        ])],
        ..Default::default()
    };
    let gen = MySqlDdlGenerator;
    let ddl = gen.generate(&diff).unwrap();
    assert!(ddl[0].contains("DEFAULT 0"));
}

#[test]
fn test_diff_multiple_tables() {
    let entity = vec![
        table("users", vec![col("id", "BIGINT", false, true)]),
        table("posts", vec![col("id", "BIGINT", false, true)]),
    ];
    let db = vec![table("users", vec![col("id", "BIGINT", false, true)])];
    let d = diff(&entity, &db);
    assert_eq!(d.added_tables.len(), 1);
    assert_eq!(d.added_tables[0].name, "posts");
}

#[test]
fn test_diff_column_with_default() {
    let entity = vec![table("users", vec![
        col("id", "BIGINT", false, true),
        ColumnDef::new("status", "INT", false, false, Some("1".to_string())),
    ])];
    let db = vec![table("users", vec![col("id", "BIGINT", false, true)])];
    let d = diff(&entity, &db);
    assert_eq!(d.added_columns.len(), 1);
    assert_eq!(d.added_columns[0].1.default, Some("1".to_string()));
}
#[test]
fn test_has_destructive_changes_empty() {
    let d = SchemaDiff::default();
    assert!(!d.has_destructive_changes());
}

#[test]
fn test_has_destructive_changes_dropped_table() {
    let d = diff(&[], &[table("old", vec![col("id", "BIGINT", false, true)])]);
    assert!(d.has_destructive_changes());
}

#[test]
fn test_has_destructive_changes_dropped_column() {
    let entity = vec![table("users", vec![col("id", "BIGINT", false, true)])];
    let db = vec![table("users", vec![col("id", "BIGINT", false, true), col("old_col", "TEXT", true, false)])];
    let d = diff(&entity, &db);
    assert!(d.has_destructive_changes());
}

#[test]
fn test_has_destructive_changes_added_only() {
    let entity = vec![table("users", vec![col("id", "BIGINT", false, true)])];
    let d = diff(&entity, &[]);
    assert!(!d.has_destructive_changes());
}

#[test]
fn test_diff_against_added_table() {
    let sync = SchemaSync::new(vec![table("users", vec![col("id", "BIGINT", false, true)])]);
    let d = sync.diff_against(&[]);
    assert_eq!(d.added_tables.len(), 1);
}

#[test]
fn test_diff_against_dropped_table() {
    let sync = SchemaSync::new(vec![]);
    let db = vec![table("old", vec![col("id", "BIGINT", false, true)])];
    let d = sync.diff_against(&db);
    assert_eq!(d.dropped_tables.len(), 1);
}

#[test]
fn test_diff_against_no_changes() {
    let t = table("users", vec![col("id", "BIGINT", false, true)]);
    let sync = SchemaSync::new(vec![t.clone()]);
    let d = sync.diff_against(&[t]);
    assert!(d.is_empty());
}

#[test]
fn test_diff_against_added_column() {
    let entity = vec![table("users", vec![col("id", "BIGINT", false, true), col("name", "TEXT", false, false)])];
    let db = vec![table("users", vec![col("id", "BIGINT", false, true)])];
    let sync = SchemaSync::new(entity);
    let d = sync.diff_against(&db);
    assert_eq!(d.added_columns.len(), 1);
}

#[test]
fn test_diff_against_dropped_column() {
    let entity = vec![table("users", vec![col("id", "BIGINT", false, true)])];
    let db = vec![table("users", vec![col("id", "BIGINT", false, true), col("old", "TEXT", true, false)])];
    let sync = SchemaSync::new(entity);
    let d = sync.diff_against(&db);
    assert_eq!(d.dropped_columns.len(), 1);
}

#[test]
fn test_diff_against_type_changed() {
    let entity = vec![table("users", vec![col("id", "BIGINT", false, true)])];
    let db = vec![table("users", vec![col("id", "INT", false, true)])];
    let sync = SchemaSync::new(entity);
    let d = sync.diff_against(&db);
    assert!(d.type_changed_columns.len() >= 1);
}

#[test]
fn test_diff_against_multiple_tables() {
    let entity = vec![
        table("users", vec![col("id", "BIGINT", false, true)]),
        table("posts", vec![col("id", "BIGINT", false, true)]),
    ];
    let db = vec![table("users", vec![col("id", "BIGINT", false, true)])];
    let sync = SchemaSync::new(entity);
    let d = sync.diff_against(&db);
    assert_eq!(d.added_tables.len(), 1);
    assert_eq!(d.added_tables[0].name, "posts");
}

#[test]

fn test_is_empty_true() {
    let d = SchemaDiff::default();
    assert!(d.is_empty());
}

#[test]
fn test_is_empty_false_with_added() {
    let entity = vec![table("users", vec![col("id", "BIGINT", false, true)])];
    let d = diff(&entity, &[]);
    assert!(!d.is_empty());
}

#[test]
fn test_get_column_found() {
    let t = table("users", vec![col("id", "BIGINT", false, true), col("name", "TEXT", false, false)]);
    assert!(t.get_column("id").is_some());
    assert!(t.get_column("name").is_some());
}

#[test]
fn test_get_column_not_found() {
    let t = table("users", vec![col("id", "BIGINT", false, true)]);
    assert!(t.get_column("nonexistent").is_none());
}