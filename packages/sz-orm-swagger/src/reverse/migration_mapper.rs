//! OpenApiToMigrationMapper — OpenAPI Schema → 迁移文件（5 方言 DDL）

use super::{to_snake_case, ReverseGenError};
use crate::Schema;
use std::collections::HashMap;
use std::sync::Arc;
use sz_orm_core::dialect_security::Dialect;
use sz_orm_core::migration::Migration;
use sz_orm_core::schema_sync::{
    ColumnDef, MssqlDdlGenerator, MySqlDdlGenerator, OracleDdlGenerator, PgDdlGenerator,
    SchemaDiff, SqliteDdlGenerator, TableDef,
};

/// DDL 方言映射
fn map_dialect_to_sql_type(dialect: Dialect, rust_type: &str, max_length: Option<u32>) -> String {
    match rust_type {
        "String" => match max_length {
            Some(n) => match dialect {
                Dialect::MySql => format!("VARCHAR({})", n),
                Dialect::PostgreSql => format!("VARCHAR({})", n),
                Dialect::Sqlite => format!("VARCHAR({})", n),
                Dialect::Oracle => format!("VARCHAR2({})", n),
                Dialect::Mssql => format!("VARCHAR({})", n),
            },
            None => match dialect {
                Dialect::MySql => "TEXT".to_string(),
                Dialect::PostgreSql => "TEXT".to_string(),
                Dialect::Sqlite => "TEXT".to_string(),
                Dialect::Oracle => "CLOB".to_string(),
                Dialect::Mssql => "NVARCHAR(MAX)".to_string(),
            },
        },
        "i64" => match dialect {
            Dialect::MySql => "BIGINT".to_string(),
            Dialect::PostgreSql => "BIGINT".to_string(),
            Dialect::Sqlite => "INTEGER".to_string(),
            Dialect::Oracle => "NUMBER(19)".to_string(),
            Dialect::Mssql => "BIGINT".to_string(),
        },
        "i32" => match dialect {
            Dialect::MySql => "INT".to_string(),
            Dialect::PostgreSql => "INTEGER".to_string(),
            Dialect::Sqlite => "INTEGER".to_string(),
            Dialect::Oracle => "NUMBER(10)".to_string(),
            Dialect::Mssql => "INT".to_string(),
        },
        "f64" => match dialect {
            Dialect::MySql => "DOUBLE".to_string(),
            Dialect::PostgreSql => "DOUBLE PRECISION".to_string(),
            Dialect::Sqlite => "REAL".to_string(),
            Dialect::Oracle => "BINARY_DOUBLE".to_string(),
            Dialect::Mssql => "FLOAT".to_string(),
        },
        "bool" => match dialect {
            Dialect::MySql => "TINYINT(1)".to_string(),
            Dialect::PostgreSql => "BOOLEAN".to_string(),
            Dialect::Sqlite => "INTEGER".to_string(),
            Dialect::Oracle => "NUMBER(1)".to_string(),
            Dialect::Mssql => "BIT".to_string(),
        },
        "DateTime" => match dialect {
            Dialect::MySql => "DATETIME".to_string(),
            Dialect::PostgreSql => "TIMESTAMP".to_string(),
            Dialect::Sqlite => "TEXT".to_string(),
            Dialect::Oracle => "TIMESTAMP".to_string(),
            Dialect::Mssql => "DATETIME2".to_string(),
        },
        "Uuid" => match dialect {
            Dialect::MySql => "VARCHAR(36)".to_string(),
            Dialect::PostgreSql => "UUID".to_string(),
            Dialect::Sqlite => "TEXT".to_string(),
            Dialect::Oracle => "VARCHAR2(36)".to_string(),
            Dialect::Mssql => "UNIQUEIDENTIFIER".to_string(),
        },
        _ => "TEXT".to_string(),
    }
}

/// 从 Schema 提取 Rust 类型字符串
fn schema_to_rust_type_string(schema: &Schema) -> String {
    if let Schema::Primitive(p) = schema {
        match p.schema_type.as_str() {
            "string" => match p.format.as_deref() {
                Some("date-time") => "DateTime".to_string(),
                Some("uuid") => "Uuid".to_string(),
                _ => "String".to_string(),
            },
            "integer" => match p.format.as_deref() {
                Some("int32") => "i32".to_string(),
                _ => "i64".to_string(),
            },
            "number" => "f64".to_string(),
            "boolean" => "bool".to_string(),
            _ => "String".to_string(),
        }
    } else {
        "String".to_string()
    }
}

/// 从 Schema 提取 max_length
fn schema_to_max_length(schema: &Schema) -> Option<u32> {
    if let Schema::Primitive(p) = schema {
        p.max_length
    } else {
        None
    }
}

/// 选择 DDL 生成器
fn select_ddl_generator(dialect: Dialect) -> Arc<dyn sz_orm_core::schema_sync::DdlGenerator> {
    match dialect {
        Dialect::MySql => Arc::new(MySqlDdlGenerator),
        Dialect::PostgreSql => Arc::new(PgDdlGenerator),
        Dialect::Sqlite => Arc::new(SqliteDdlGenerator),
        Dialect::Oracle => Arc::new(OracleDdlGenerator),
        Dialect::Mssql => Arc::new(MssqlDdlGenerator),
    }
}

/// OpenApiToMigrationMapper — OpenAPI Schema → 迁移文件
pub struct OpenApiToMigrationMapper {
    /// 目标方言
    pub dialect: Dialect,
}

impl OpenApiToMigrationMapper {
    /// 创建新的 mapper
    pub fn new(dialect: Dialect) -> Self {
        Self { dialect }
    }

    /// Schema → TableDef 转换
    pub fn schema_to_table_def(
        &self,
        schema_name: &str,
        schema: &Schema,
    ) -> Result<TableDef, ReverseGenError> {
        let table_name = to_snake_case(schema_name);

        let obj = match schema {
            Schema::Object(o) => o,
            _ => {
                return Err(ReverseGenError::UnsupportedSchemaConstruct {
                    construct: "non-object schema".to_string(),
                    schema: schema_name.to_string(),
                });
            }
        };

        let mut columns = Vec::new();
        for (name, prop_schema) in &obj.properties {
            let required = obj.required.contains(name);
            let rust_type = schema_to_rust_type_string(prop_schema);
            let max_length = schema_to_max_length(prop_schema);
            let sql_type = map_dialect_to_sql_type(self.dialect, &rust_type, max_length);

            let is_primary_key = name == "id";
            let nullable = !required && !is_primary_key;

            columns.push(ColumnDef::new(
                to_snake_case(name),
                sql_type,
                nullable,
                is_primary_key,
                None,
            ));
        }

        Ok(TableDef::new(table_name, columns))
    }

    /// 生成迁移文件
    pub fn generate_migration(
        &self,
        schema_name: &str,
        schema: &Schema,
    ) -> Result<Migration, ReverseGenError> {
        let table_def = self.schema_to_table_def(schema_name, schema)?;
        let table_name = table_def.name.clone();

        let mut diff = SchemaDiff::default();
        diff.added_tables.push(table_def);

        let generator = select_ddl_generator(self.dialect);
        let ddl = generator
            .generate(&diff)
            .map_err(|e| ReverseGenError::SpecParseFailed {
                path: schema_name.to_string(),
                reason: e.to_string(),
            })?;

        let sql_up = ddl.join(";\n") + ";";
        let sql_down = format!("DROP TABLE IF EXISTS {};", table_name);

        let version = chrono::Utc::now().format("%Y%m%d%H%M%S").to_string();
        let migration_name = format!("create_{}", table_name);

        Ok(Migration::new(
            &version,
            &migration_name,
            &sql_up,
            &sql_down,
        ))
    }

    /// 批量生成迁移
    pub fn generate_migrations(
        &self,
        schemas: &HashMap<String, Schema>,
    ) -> Result<Vec<Migration>, ReverseGenError> {
        let mut migrations = Vec::new();
        for (name, schema) in schemas {
            let migration = self.generate_migration(name, schema)?;
            migrations.push(migration);
        }
        Ok(migrations)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ObjectType, PrimitiveSchema};

    fn make_user_schema() -> Schema {
        let mut obj = ObjectType::new();
        obj = obj.with_required_property("id", Schema::integer());
        obj = obj.with_required_property(
            "name",
            Schema::Primitive(PrimitiveSchema::string().with_length_range(0, 255)),
        );
        obj = obj.with_property("email", Schema::string());
        Schema::Object(obj)
    }

    #[test]
    fn test_schema_to_table_def() {
        let mapper = OpenApiToMigrationMapper::new(Dialect::MySql);
        let schema = make_user_schema();
        let table_def = mapper.schema_to_table_def("User", &schema).unwrap();

        assert_eq!(table_def.name, "user");
        assert_eq!(table_def.columns.len(), 3);

        let id_col = table_def.get_column("id").unwrap();
        assert!(id_col.primary_key);
        assert!(!id_col.nullable);
        assert_eq!(id_col.sql_type, "BIGINT");

        let name_col = table_def.get_column("name").unwrap();
        assert!(!name_col.nullable);
        assert_eq!(name_col.sql_type, "VARCHAR(255)");

        let email_col = table_def.get_column("email").unwrap();
        assert!(email_col.nullable);
    }

    #[test]
    fn test_generate_migration_mysql() {
        let mapper = OpenApiToMigrationMapper::new(Dialect::MySql);
        let schema = make_user_schema();
        let migration = mapper.generate_migration("User", &schema).unwrap();

        assert!(migration.sql_up.contains("CREATE TABLE user"));
        assert!(migration.sql_up.contains("id BIGINT NOT NULL PRIMARY KEY"));
        assert!(migration.sql_up.contains("name VARCHAR(255) NOT NULL"));
        assert!(migration.sql_down.contains("DROP TABLE IF EXISTS user"));
    }

    #[test]
    fn test_generate_migration_postgresql() {
        let mapper = OpenApiToMigrationMapper::new(Dialect::PostgreSql);
        let schema = make_user_schema();
        let migration = mapper.generate_migration("User", &schema).unwrap();

        assert!(migration.sql_up.contains("CREATE TABLE user"));
        assert!(migration.sql_up.contains("id BIGINT NOT NULL PRIMARY KEY"));
    }

    #[test]
    fn test_generate_migration_sqlite() {
        let mapper = OpenApiToMigrationMapper::new(Dialect::Sqlite);
        let schema = make_user_schema();
        let migration = mapper.generate_migration("User", &schema).unwrap();

        assert!(migration.sql_up.contains("CREATE TABLE user"));
        assert!(migration.sql_up.contains("id INTEGER NOT NULL PRIMARY KEY"));
    }

    #[test]
    fn test_generate_migration_oracle() {
        let mapper = OpenApiToMigrationMapper::new(Dialect::Oracle);
        let schema = make_user_schema();
        let migration = mapper.generate_migration("User", &schema).unwrap();

        assert!(migration.sql_up.contains("CREATE TABLE user"));
        assert!(migration
            .sql_up
            .contains("id NUMBER(19) NOT NULL PRIMARY KEY"));
    }

    #[test]
    fn test_generate_migration_mssql() {
        let mapper = OpenApiToMigrationMapper::new(Dialect::Mssql);
        let schema = make_user_schema();
        let migration = mapper.generate_migration("User", &schema).unwrap();

        assert!(migration.sql_up.contains("CREATE TABLE user"));
        assert!(migration.sql_up.contains("id BIGINT NOT NULL PRIMARY KEY"));
    }

    #[test]
    fn test_non_object_schema_error() {
        let mapper = OpenApiToMigrationMapper::new(Dialect::MySql);
        let schema = Schema::string();
        let result = mapper.schema_to_table_def("NotAnObject", &schema);
        assert!(result.is_err());
    }

    #[test]
    fn test_dialect_specific_types() {
        let mapper = OpenApiToMigrationMapper::new(Dialect::PostgreSql);
        let mut obj = ObjectType::new();
        obj = obj.with_required_property(
            "id",
            Schema::Primitive(PrimitiveSchema::string().with_format("uuid")),
        );
        let schema = Schema::Object(obj);
        let table_def = mapper.schema_to_table_def("Entity", &schema).unwrap();
        let id_col = table_def.get_column("id").unwrap();
        assert_eq!(id_col.sql_type, "UUID");
    }
}
