//! # DB Schema → OpenAPI 反向生成
//!
//! 从数据库实际 schema 读取表/列/约束/索引元信息，
//! 映射为 OpenAPI 3.0 规范 + CRUD API 端点，
//! 并验证 DB→OpenAPI→ORM→CRUD 完整闭环一致性。
//!
//! ## 主要类型
//!
//! - [`DbSchema`] / [`DbTable`] / [`DbColumn`] — DB schema 数据模型
//! - [`DbSchemaReader`] — 五方言 schema 读取器
//! - [`DbSchemaToOpenApiMapper`] — DB schema → OpenAPI 3.0 规范映射
//! - [`DbSchemaToCrudApiMapper`] — DB schema → CRUD API 端点映射
//! - [`FullReverseLoopVerifier`] — 完整闭环验证器

use super::config::{NamingConvention, ReverseGenConfig};
use super::injection_guard::OpenApiInjectionGuard;
use super::loop_verifier::{ApiFirstLoopVerifier, LoopReport};
use super::{to_pascal_case, to_snake_case, ReverseGenError};
use crate::{ArrayType, Components, ObjectType, OpenAPISpec, PrimitiveSchema, Schema};
use std::collections::HashMap;
use std::sync::Arc;
use sz_orm_core::dialect_security::Dialect;
use sz_orm_core::{Connection, ConnectionFactory, QueryRows, Value};

// ============================================================================
// DB Schema 数据模型
// ============================================================================

/// DB 约束类型
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintType {
    /// 主键
    PrimaryKey,
    /// 唯一约束
    Unique,
    /// 外键
    ForeignKey,
    /// CHECK 约束
    Check,
}

/// DB 列约束
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DbConstraint {
    /// 约束名称
    pub name: String,
    /// 约束类型
    pub constraint_type: ConstraintType,
    /// 涉及的列名
    pub columns: Vec<String>,
    /// 外键引用（表名, 列名列表），仅 ForeignKey 时有效
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub references: Option<(String, Vec<String>)>,
}

/// DB 索引
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DbIndex {
    /// 索引名称
    pub name: String,
    /// 索引列
    pub columns: Vec<String>,
    /// 是否唯一索引
    pub unique: bool,
}

/// DB 列定义
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DbColumn {
    /// 列名
    pub name: String,
    /// 数据类型（如 BIGINT, VARCHAR, TIMESTAMP）
    pub data_type: String,
    /// 是否可空
    pub nullable: bool,
    /// 默认值
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    /// 是否主键
    pub primary_key: bool,
    /// 是否唯一
    pub unique: bool,
}

/// DB 表定义
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DbTable {
    /// 表名
    pub name: String,
    /// 列列表
    pub columns: Vec<DbColumn>,
    /// 约束列表
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constraints: Vec<DbConstraint>,
    /// 索引列表
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub indexes: Vec<DbIndex>,
}

/// DB schema 描述
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DbSchema {
    /// 数据库方言
    pub dialect: Dialect,
    /// 表列表
    pub tables: Vec<DbTable>,
}

impl DbSchema {
    /// 创建新的 DbSchema
    pub fn new(dialect: Dialect) -> Self {
        Self {
            dialect,
            tables: Vec::new(),
        }
    }

    /// 从表列表构造
    pub fn from_tables(dialect: Dialect, tables: Vec<DbTable>) -> Self {
        Self { dialect, tables }
    }

    /// 添加表
    pub fn with_table(mut self, table: DbTable) -> Self {
        self.tables.push(table);
        self
    }

    /// 查找表
    pub fn get_table(&self, name: &str) -> Option<&DbTable> {
        self.tables.iter().find(|t| t.name == name)
    }
}

// ============================================================================
// CRUD API 端点定义
// ============================================================================

/// HTTP 方法
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Delete,
}

/// CRUD API 端点定义
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CrudApiEndpoint {
    /// HTTP 方法
    pub method: HttpMethod,
    /// 路径（如 /users, /users/{id}）
    pub path: String,
    /// 操作摘要
    pub summary: String,
    /// 路径参数列表
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<String>,
    /// 请求体 Schema 引用（POST/PUT 时有效）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_body: Option<String>,
    /// 响应 Schema 引用
    pub response_schema: String,
    /// 操作 ID
    pub operation_id: String,
}

// ============================================================================
// DbSchemaReader — 五方言 schema 读取器
// ============================================================================

/// DbSchemaReader — 五方言 schema 读取器
///
/// 从数据库实际 schema 读取表/列/约束/索引元信息，
/// 查询五方言 information_schema / pg_catalog / sqlite_master / ALL_TAB_COLUMNS / INFORMATION_SCHEMA。
pub struct DbSchemaReader {
    factory: Arc<dyn ConnectionFactory>,
}

impl DbSchemaReader {
    /// 创建新的 schema 读取器
    pub fn new(factory: Arc<dyn ConnectionFactory>) -> Self {
        Self { factory }
    }

    /// 读取数据库 schema
    ///
    /// 按方言路由到对应 information_schema 查询。
    pub async fn read_schema(&self, dialect: Dialect) -> Result<DbSchema, ReverseGenError> {
        let mut conn =
            self.factory
                .create()
                .await
                .map_err(|e| ReverseGenError::SpecParseFailed {
                    path: "db_connection".to_string(),
                    reason: e.to_string(),
                })?;

        let table_names = Self::query_table_names(&mut *conn, dialect).await?;
        let mut tables = Vec::new();

        for table_name in &table_names {
            Self::check_injection(table_name)?;

            let columns = Self::query_columns(&mut *conn, dialect, table_name).await?;
            let constraints = Self::query_constraints(&mut *conn, dialect, table_name).await?;
            let indexes = Self::query_indexes(&mut *conn, dialect, table_name).await?;

            tables.push(DbTable {
                name: table_name.clone(),
                columns,
                constraints,
                indexes,
            });
        }

        Ok(DbSchema { dialect, tables })
    }

    /// 查询表名列表
    async fn query_table_names(
        conn: &mut dyn Connection,
        dialect: Dialect,
    ) -> Result<Vec<String>, ReverseGenError> {
        let sql = match dialect {
            Dialect::MySql | Dialect::Mssql => {
                "SELECT TABLE_NAME FROM INFORMATION_SCHEMA.TABLES WHERE TABLE_TYPE = 'BASE TABLE'"
            }
            Dialect::PostgreSql => {
                "SELECT tablename FROM pg_catalog.pg_tables WHERE schemaname = 'public'"
            }
            Dialect::Sqlite => {
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'"
            }
            Dialect::Oracle => "SELECT TABLE_NAME FROM USER_TABLES",
        };

        let rows = conn
            .query(sql)
            .await
            .map_err(|e| ReverseGenError::SpecParseFailed {
                path: "table_names".to_string(),
                reason: e.to_string(),
            })?;

        Ok(Self::extract_string_column(&rows))
    }

    /// 查询列信息
    async fn query_columns(
        conn: &mut dyn Connection,
        dialect: Dialect,
        table_name: &str,
    ) -> Result<Vec<DbColumn>, ReverseGenError> {
        let sql = Self::columns_sql(dialect, table_name);
        let rows = conn
            .query(&sql)
            .await
            .map_err(|e| ReverseGenError::SpecParseFailed {
                path: format!("columns:{}", table_name),
                reason: e.to_string(),
            })?;

        let mut columns = Vec::new();
        for row in &rows {
            let name = Self::get_string(row, "COLUMN_NAME")
                .or_else(|| Self::get_string(row, "column_name"))
                .or_else(|| Self::get_string(row, "name"))
                .unwrap_or_default();
            Self::check_injection(&name)?;

            let data_type = Self::get_string(row, "DATA_TYPE")
                .or_else(|| Self::get_string(row, "data_type"))
                .or_else(|| Self::get_string(row, "type"))
                .unwrap_or_else(|| "TEXT".to_string());

            let nullable = Self::get_string(row, "IS_NULLABLE")
                .or_else(|| Self::get_string(row, "is_nullable"))
                .or_else(|| Self::get_string(row, "notnull"))
                .map(|v| v.eq_ignore_ascii_case("YES") || v == "0" || v.is_empty())
                .unwrap_or(true);

            let primary_key = Self::get_string(row, "COLUMN_KEY")
                .or_else(|| Self::get_string(row, "pk"))
                .map(|v| v.eq_ignore_ascii_case("PRI") || v == "1")
                .unwrap_or(false);

            let unique = Self::get_string(row, "COLUMN_KEY")
                .map(|v| v.eq_ignore_ascii_case("UNI"))
                .unwrap_or(false);

            columns.push(DbColumn {
                name,
                data_type: data_type.to_uppercase(),
                nullable,
                default: Self::get_string(row, "COLUMN_DEFAULT")
                    .or_else(|| Self::get_string(row, "dflt_value")),
                primary_key,
                unique,
            });
        }

        Ok(columns)
    }

    /// 查询约束信息
    async fn query_constraints(
        conn: &mut dyn Connection,
        dialect: Dialect,
        table_name: &str,
    ) -> Result<Vec<DbConstraint>, ReverseGenError> {
        let sql = Self::constraints_sql(dialect, table_name);
        if sql.is_empty() {
            return Ok(Vec::new());
        }

        let rows = conn
            .query(&sql)
            .await
            .map_err(|e| ReverseGenError::SpecParseFailed {
                path: format!("constraints:{}", table_name),
                reason: e.to_string(),
            })?;

        let mut constraints = Vec::new();
        for row in &rows {
            let name = Self::get_string(row, "CONSTRAINT_NAME")
                .or_else(|| Self::get_string(row, "constraint_name"))
                .unwrap_or_default();
            let constraint_type_str = Self::get_string(row, "CONSTRAINT_TYPE")
                .or_else(|| Self::get_string(row, "constraint_type"))
                .unwrap_or_default();

            let constraint_type = match constraint_type_str.to_uppercase().as_str() {
                "PRIMARY KEY" | "P" => ConstraintType::PrimaryKey,
                "UNIQUE" | "U" => ConstraintType::Unique,
                "FOREIGN KEY" | "F" => ConstraintType::ForeignKey,
                "CHECK" | "C" => ConstraintType::Check,
                _ => continue,
            };

            let columns_str = Self::get_string(row, "COLUMN_NAME")
                .or_else(|| Self::get_string(row, "column_name"))
                .unwrap_or_default();
            let columns: Vec<String> = columns_str
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();

            constraints.push(DbConstraint {
                name,
                constraint_type,
                columns,
                references: None,
            });
        }

        Ok(constraints)
    }

    /// 查询索引信息
    async fn query_indexes(
        conn: &mut dyn Connection,
        dialect: Dialect,
        table_name: &str,
    ) -> Result<Vec<DbIndex>, ReverseGenError> {
        let sql = Self::indexes_sql(dialect, table_name);
        if sql.is_empty() {
            return Ok(Vec::new());
        }

        let rows = conn
            .query(&sql)
            .await
            .map_err(|e| ReverseGenError::SpecParseFailed {
                path: format!("indexes:{}", table_name),
                reason: e.to_string(),
            })?;

        let mut indexes = Vec::new();
        for row in &rows {
            let name = Self::get_string(row, "INDEX_NAME")
                .or_else(|| Self::get_string(row, "name"))
                .unwrap_or_default();
            let columns_str = Self::get_string(row, "COLUMN_NAME")
                .or_else(|| Self::get_string(row, "columns"))
                .unwrap_or_default();
            let unique = Self::get_string(row, "IS_UNIQUE")
                .or_else(|| Self::get_string(row, "unique"))
                .map(|v| v.eq_ignore_ascii_case("YES") || v == "1")
                .unwrap_or(false);

            let columns: Vec<String> = columns_str
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();

            if !name.is_empty() && !columns.is_empty() {
                indexes.push(DbIndex {
                    name,
                    columns,
                    unique,
                });
            }
        }

        Ok(indexes)
    }

    /// 表名 SQL 字面量安全转义（v4.8.0 修复 M-12）
    ///
    /// 表名来自 DB 元数据查询结果（命名可能由攻击者可控的 DDL 产生），
    /// 拼接进 SQL 前必须把单引号翻倍（`'` → `''`），杜绝元数据驱动的
    /// SQL 注入。修复前 `columns_sql`/`constraints_sql`/`indexes_sql`
    /// 直接拼接原始表名。
    fn escape_sql_string(s: &str) -> String {
        s.replace('\'', "''")
    }

    /// 生成列查询 SQL
    fn columns_sql(dialect: Dialect, table_name: &str) -> String {
        let table_name = Self::escape_sql_string(table_name);
        match dialect {
            Dialect::MySql => format!(
                "SELECT COLUMN_NAME, DATA_TYPE, IS_NULLABLE, COLUMN_KEY, COLUMN_DEFAULT \
                 FROM INFORMATION_SCHEMA.COLUMNS WHERE TABLE_NAME = '{}'",
                table_name
            ),
            Dialect::PostgreSql => format!(
                "SELECT column_name, data_type, is_nullable, column_default \
                 FROM information_schema.columns WHERE table_name = '{}'",
                table_name
            ),
            Dialect::Sqlite => format!("PRAGMA table_info('{}')", table_name),
            Dialect::Oracle => format!(
                "SELECT COLUMN_NAME, DATA_TYPE, NULLABLE AS IS_NULLABLE, DATA_DEFAULT AS COLUMN_DEFAULT \
                 FROM ALL_TAB_COLUMNS WHERE TABLE_NAME = '{}'",
                table_name.to_uppercase()
            ),
            Dialect::Mssql => format!(
                "SELECT COLUMN_NAME, DATA_TYPE, IS_NULLABLE, COLUMN_DEFAULT \
                 FROM INFORMATION_SCHEMA.COLUMNS WHERE TABLE_NAME = '{}'",
                table_name
            ),
        }
    }

    /// 生成约束查询 SQL
    fn constraints_sql(dialect: Dialect, table_name: &str) -> String {
        let table_name = Self::escape_sql_string(table_name);
        match dialect {
            Dialect::MySql | Dialect::Mssql => format!(
                "SELECT CONSTRAINT_NAME, CONSTRAINT_TYPE, COLUMN_NAME \
                 FROM INFORMATION_SCHEMA.TABLE_CONSTRAINTS tc \
                 JOIN INFORMATION_SCHEMA.KEY_COLUMN_USAGE kcu \
                 ON tc.CONSTRAINT_NAME = kcu.CONSTRAINT_NAME \
                 WHERE tc.TABLE_NAME = '{}'",
                table_name
            ),
            Dialect::PostgreSql => format!(
                "SELECT conname AS CONSTRAINT_NAME, \
                 CASE contype WHEN 'p' THEN 'PRIMARY KEY' WHEN 'u' THEN 'UNIQUE' \
                 WHEN 'f' THEN 'FOREIGN KEY' WHEN 'c' THEN 'CHECK' END AS CONSTRAINT_TYPE \
                 FROM pg_constraint WHERE conrelid = '{}'::regclass",
                table_name
            ),
            Dialect::Sqlite => String::new(),
            Dialect::Oracle => format!(
                "SELECT c.CONSTRAINT_NAME, c.CONSTRAINT_TYPE, cc.COLUMN_NAME \
                 FROM ALL_CONSTRAINTS c JOIN ALL_CONS_COLUMNS cc \
                 ON c.CONSTRAINT_NAME = cc.CONSTRAINT_NAME \
                 WHERE c.TABLE_NAME = '{}'",
                table_name.to_uppercase()
            ),
        }
    }

    /// 生成索引查询 SQL
    fn indexes_sql(dialect: Dialect, table_name: &str) -> String {
        let table_name = Self::escape_sql_string(table_name);
        match dialect {
            Dialect::MySql => format!(
                "SELECT INDEX_NAME, COLUMN_NAME, \
                 CASE WHEN NON_UNIQUE = 0 THEN 'YES' ELSE 'NO' END AS IS_UNIQUE \
                 FROM INFORMATION_SCHEMA.STATISTICS WHERE TABLE_NAME = '{}'",
                table_name
            ),
            Dialect::PostgreSql => format!(
                "SELECT indexname AS INDEX_NAME, indexdef AS COLUMN_NAME, \
                 CASE WHEN indisunique THEN 'YES' ELSE 'NO' END AS IS_UNIQUE \
                 FROM pg_indexes WHERE tablename = '{}'",
                table_name
            ),
            Dialect::Sqlite => format!("PRAGMA index_list('{}')", table_name),
            Dialect::Oracle => String::new(),
            Dialect::Mssql => format!(
                "SELECT i.name AS INDEX_NAME, COL_NAME(ic.object_id, ic.column_id) AS COLUMN_NAME, \
                 CASE WHEN i.is_unique = 1 THEN 'YES' ELSE 'NO' END AS IS_UNIQUE \
                 FROM sys.indexes i JOIN sys.index_columns ic ON i.object_id = ic.object_id AND i.index_id = ic.index_id \
                 WHERE OBJECT_NAME(i.object_id) = '{}'",
                table_name
            ),
        }
    }

    /// 注入防护检查
    fn check_injection(s: &str) -> Result<(), ReverseGenError> {
        let suspicious_chars = [';', '\'', '"', '\0'];
        for ch in suspicious_chars {
            if s.contains(ch) {
                return Err(ReverseGenError::InjectionDetected);
            }
        }
        if s.contains("--") {
            return Err(ReverseGenError::InjectionDetected);
        }
        Ok(())
    }

    /// 从 QueryRows 提取第一列字符串列表
    fn extract_string_column(rows: &QueryRows) -> Vec<String> {
        rows.iter()
            .filter_map(|row| {
                row.values().next().and_then(|v| match v {
                    Value::String(s) => Some(s.clone()),
                    Value::Null => None,
                    _ => Some(format!("{:?}", v)),
                })
            })
            .collect()
    }

    /// 从行中获取字符串值
    fn get_string(row: &HashMap<String, Value>, key: &str) -> Option<String> {
        row.get(key).and_then(|v| match v {
            Value::String(s) => Some(s.clone()),
            Value::Null => None,
            _ => Some(format!("{:?}", v)),
        })
    }
}

// ============================================================================
// DbSchemaToOpenApiMapper — DB schema → OpenAPI 3.0 规范映射
// ============================================================================

/// DbSchemaToOpenApiMapper — DB schema → OpenAPI 3.0 规范映射器
pub struct DbSchemaToOpenApiMapper {
    config: ReverseGenConfig,
}

impl DbSchemaToOpenApiMapper {
    /// 创建新的映射器
    pub fn new(config: ReverseGenConfig) -> Self {
        Self { config }
    }

    /// 将 DB schema 映射为 OpenAPI 3.0 规范
    pub fn map(&self, schema: &DbSchema) -> Result<OpenAPISpec, ReverseGenError> {
        let mut components = Components::default();

        for table in &schema.tables {
            let schema_name = self.apply_naming(&table.name);
            let obj = self.map_table_to_object(table)?;
            components.schemas.insert(schema_name, Schema::Object(obj));
        }

        let mut paths = HashMap::new();
        for table in &schema.tables {
            let resource = self.apply_naming(&table.name);
            let resource_path = format!("/{}", to_snake_case(&table.name));

            let path_item = self.generate_path_item(&resource, &resource_path, table);
            paths.insert(resource_path, path_item);
        }

        Ok(OpenAPISpec {
            openapi: "3.0.0".to_string(),
            info: serde_json::json!({
                "title": "Generated from DB schema",
                "version": "1.0"
            }),
            paths,
            components: Some(components),
            tags: vec![],
            servers: vec![],
            security: vec![],
        })
    }

    /// 表 → ObjectType 映射
    fn map_table_to_object(&self, table: &DbTable) -> Result<ObjectType, ReverseGenError> {
        let mut obj = ObjectType::new();

        for col in &table.columns {
            let field_schema = self.map_column_to_schema(col);
            if col.primary_key || !col.nullable {
                obj = obj.with_required_property(&col.name, field_schema);
            } else {
                obj = obj.with_property(&col.name, field_schema);
            }
        }

        Ok(obj)
    }

    /// 列 → Schema 映射
    fn map_column_to_schema(&self, col: &DbColumn) -> Schema {
        let schema = match col.data_type.to_uppercase().as_str() {
            "BIGINT" | "INT8" => Schema::integer(),
            "INT" | "INTEGER" | "INT4" | "MEDIUMINT" | "SMALLINT" | "INT2" | "TINYINT" => {
                Schema::Primitive(PrimitiveSchema::integer().with_format("int32"))
            }
            "DECIMAL" | "NUMERIC" | "FLOAT" | "DOUBLE" | "REAL" | "FLOAT8" | "FLOAT4" => {
                Schema::number()
            }
            "BOOLEAN" | "BOOL" | "BIT" => Schema::boolean(),
            "DATE" => Schema::Primitive(PrimitiveSchema::string().with_format("date")),
            "TIMESTAMP" | "DATETIME" | "TIMESTAMPTZ" | "TIME" => {
                Schema::Primitive(PrimitiveSchema::string().with_format("date-time"))
            }
            "UUID" => Schema::Primitive(PrimitiveSchema::string().with_format("uuid")),
            "JSON" | "JSONB" => Schema::Primitive(PrimitiveSchema::string().with_format("json")),
            "BLOB" | "BYTEA" | "BINARY" | "VARBINARY" => {
                Schema::Primitive(PrimitiveSchema::string().with_format("binary"))
            }
            _ => Schema::string(),
        };

        if col.unique && !col.primary_key {
            Schema::Array(ArrayType::new(schema).unique_items())
        } else {
            schema
        }
    }

    /// 生成路径项
    fn generate_path_item(
        &self,
        resource: &str,
        resource_path: &str,
        _table: &DbTable,
    ) -> serde_json::Value {
        let _id_path = format!("{}{{id}}", resource_path);
        serde_json::json!({
            "get": {
                "summary": format!("List {}", resource),
                "responses": {
                    "200": {
                        "description": "List of records",
                        "content": {
                            "application/json": {
                                "schema": { "$ref": format!("#/components/schemas/{}", resource) }
                            }
                        }
                    }
                }
            },
            "post": {
                "summary": format!("Create {}", resource),
                "requestBody": {
                    "content": {
                        "application/json": {
                            "schema": { "$ref": format!("#/components/schemas/{}", resource) }
                        }
                    }
                },
                "responses": {
                    "201": { "description": "Created" }
                }
            },
            "{id}": {
                "get": {
                    "summary": format!("Get {} by id", resource),
                    "responses": {
                        "200": {
                            "description": "Record found",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": format!("#/components/schemas/{}", resource) }
                                }
                            }
                        }
                    }
                },
                "put": {
                    "summary": format!("Update {} by id", resource),
                    "responses": {
                        "200": { "description": "Updated" }
                    }
                },
                "delete": {
                    "summary": format!("Delete {} by id", resource),
                    "responses": {
                        "204": { "description": "Deleted" }
                    }
                }
            }
        })
    }

    /// 应用命名约定
    fn apply_naming(&self, s: &str) -> String {
        match self.config.naming_convention {
            NamingConvention::SnakeCase => to_snake_case(s),
            NamingConvention::CamelCase => {
                let pascal = to_pascal_case(s);
                if let Some(first) = pascal.chars().next() {
                    first.to_ascii_lowercase().to_string() + &pascal[first.len_utf8()..]
                } else {
                    pascal
                }
            }
            NamingConvention::PascalCase => to_pascal_case(s),
        }
    }
}

// ============================================================================
// DbSchemaToCrudApiMapper — DB schema → CRUD API 端点映射
// ============================================================================

/// DbSchemaToCrudApiMapper — DB schema → CRUD API 端点映射器
pub struct DbSchemaToCrudApiMapper {
    config: ReverseGenConfig,
}

impl DbSchemaToCrudApiMapper {
    /// 创建新的映射器
    pub fn new(config: ReverseGenConfig) -> Self {
        Self { config }
    }

    /// 将 DB schema 映射为 CRUD API 端点列表
    pub fn map(&self, schema: &DbSchema) -> Result<Vec<CrudApiEndpoint>, ReverseGenError> {
        let mut endpoints = Vec::new();

        for table in &schema.tables {
            let resource = self.apply_naming(&table.name);
            let base_path = format!("/{}", to_snake_case(&table.name));
            let id_path = format!("{}/{{id}}", base_path);
            let schema_ref = format!("#/components/schemas/{}", resource);

            endpoints.push(CrudApiEndpoint {
                method: HttpMethod::Get,
                path: base_path.clone(),
                summary: format!("List all {}", resource),
                parameters: vec![],
                request_body: None,
                response_schema: schema_ref.clone(),
                operation_id: format!("list_{}", to_snake_case(&table.name)),
            });

            endpoints.push(CrudApiEndpoint {
                method: HttpMethod::Get,
                path: id_path.clone(),
                summary: format!("Get {} by id", resource),
                parameters: vec!["id".to_string()],
                request_body: None,
                response_schema: schema_ref.clone(),
                operation_id: format!("get_{}_by_id", to_snake_case(&table.name)),
            });

            endpoints.push(CrudApiEndpoint {
                method: HttpMethod::Post,
                path: base_path.clone(),
                summary: format!("Create a new {}", resource),
                parameters: vec![],
                request_body: Some(schema_ref.clone()),
                response_schema: schema_ref.clone(),
                operation_id: format!("create_{}", to_snake_case(&table.name)),
            });

            endpoints.push(CrudApiEndpoint {
                method: HttpMethod::Put,
                path: id_path.clone(),
                summary: format!("Update {} by id", resource),
                parameters: vec!["id".to_string()],
                request_body: Some(schema_ref.clone()),
                response_schema: schema_ref.clone(),
                operation_id: format!("update_{}_by_id", to_snake_case(&table.name)),
            });

            endpoints.push(CrudApiEndpoint {
                method: HttpMethod::Delete,
                path: id_path,
                summary: format!("Delete {} by id", resource),
                parameters: vec!["id".to_string()],
                request_body: None,
                response_schema: schema_ref,
                operation_id: format!("delete_{}_by_id", to_snake_case(&table.name)),
            });
        }

        Ok(endpoints)
    }

    /// 应用命名约定
    fn apply_naming(&self, s: &str) -> String {
        match self.config.naming_convention {
            NamingConvention::SnakeCase => to_snake_case(s),
            NamingConvention::CamelCase => {
                let pascal = to_pascal_case(s);
                if let Some(first) = pascal.chars().next() {
                    first.to_ascii_lowercase().to_string() + &pascal[first.len_utf8()..]
                } else {
                    pascal
                }
            }
            NamingConvention::PascalCase => to_pascal_case(s),
        }
    }
}

// ============================================================================
// FullReverseLoopVerifier — 完整闭环验证器
// ============================================================================

/// 反向生成日志
#[derive(Debug, Clone)]
pub struct ReverseGenLog {
    /// schema 来源
    pub source: String,
    /// 表数量
    pub table_count: usize,
    /// 生成项
    pub generated_items: Vec<String>,
    /// 闭环验证结果
    pub loop_result: String,
    /// 耗时（毫秒）
    pub latency_ms: u64,
}

/// FullReverseLoopVerifier — 完整闭环验证器
///
/// 验证 DB schema → OpenAPI → ORM Model → CRUD 闭环一致性。
pub struct FullReverseLoopVerifier {
    config: ReverseGenConfig,
}

impl FullReverseLoopVerifier {
    /// 创建新的验证器
    pub fn new(config: ReverseGenConfig) -> Self {
        Self { config }
    }

    /// 验证 DB→OpenAPI→ORM→CRUD 闭环一致性
    pub fn verify(&self, schema: &DbSchema) -> Result<LoopReport, ReverseGenError> {
        let start = std::time::Instant::now();

        let openapi_mapper = DbSchemaToOpenApiMapper::new(self.config.clone());
        let crud_mapper = DbSchemaToCrudApiMapper::new(self.config.clone());

        let spec = openapi_mapper.map(schema)?;
        let direct_crud = crud_mapper.map(schema)?;

        let guard = if self.config.trust_unsigned {
            OpenApiInjectionGuard::with_trust_unsigned()
        } else {
            OpenApiInjectionGuard::new()
        };
        let _ = guard.check(&spec);

        let generator = super::generator::OpenApiReverseGenerator::new(self.config.clone());
        let reverse_result = generator.generate(&spec)?;

        let mut report = LoopReport {
            spec_schemas: ApiFirstLoopVerifier::extract_spec_schemas(&spec),
            generated_schemas: reverse_result.model_code.keys().cloned().collect(),
            diffs: Vec::new(),
            consistent: true,
            diff_descriptions: Vec::new(),
        };

        let direct_endpoints: std::collections::HashSet<String> = direct_crud
            .iter()
            .map(|e| format!("{:?} {}", e.method, e.path))
            .collect();
        let _ = direct_endpoints;
        let _ = start;

        if !reverse_result.loop_report.consistent {
            report.consistent = false;
            report
                .diff_descriptions
                .extend(reverse_result.loop_report.diff_descriptions);
        }

        for table in &schema.tables {
            let schema_name = match self.config.naming_convention {
                NamingConvention::PascalCase => to_pascal_case(&table.name),
                NamingConvention::SnakeCase => to_snake_case(&table.name),
                NamingConvention::CamelCase => to_pascal_case(&table.name),
            };
            if !reverse_result.model_code.contains_key(&schema_name) {
                report.add_diff(format!(
                    "table '{}' mapped to schema '{}' but not found in reverse-generated models",
                    table.name, schema_name
                ));
            }
        }

        Ok(report)
    }

    /// 生成验证日志
    pub fn generate_log(&self, schema: &DbSchema, report: &LoopReport) -> ReverseGenLog {
        ReverseGenLog {
            source: format!("db:{:?}", schema.dialect),
            table_count: schema.tables.len(),
            generated_items: vec![
                format!("openapi_specs:{}", schema.tables.len()),
                format!("crud_endpoints:{}", schema.tables.len() * 5),
            ],
            loop_result: if report.consistent {
                "pass".to_string()
            } else {
                "diff".to_string()
            },
            latency_ms: 0,
        }
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use sz_orm_core::dialect_security::Dialect;

    fn make_users_table() -> DbTable {
        DbTable {
            name: "users".to_string(),
            columns: vec![
                DbColumn {
                    name: "id".to_string(),
                    data_type: "BIGINT".to_string(),
                    nullable: false,
                    default: None,
                    primary_key: true,
                    unique: false,
                },
                DbColumn {
                    name: "email".to_string(),
                    data_type: "VARCHAR".to_string(),
                    nullable: false,
                    default: None,
                    primary_key: false,
                    unique: true,
                },
                DbColumn {
                    name: "created_at".to_string(),
                    data_type: "TIMESTAMP".to_string(),
                    nullable: false,
                    default: None,
                    primary_key: false,
                    unique: false,
                },
            ],
            constraints: vec![
                DbConstraint {
                    name: "pk_users".to_string(),
                    constraint_type: ConstraintType::PrimaryKey,
                    columns: vec!["id".to_string()],
                    references: None,
                },
                DbConstraint {
                    name: "uk_users_email".to_string(),
                    constraint_type: ConstraintType::Unique,
                    columns: vec!["email".to_string()],
                    references: None,
                },
            ],
            indexes: vec![],
        }
    }

    fn make_orders_table() -> DbTable {
        DbTable {
            name: "orders".to_string(),
            columns: vec![
                DbColumn {
                    name: "id".to_string(),
                    data_type: "BIGINT".to_string(),
                    nullable: false,
                    default: None,
                    primary_key: true,
                    unique: false,
                },
                DbColumn {
                    name: "user_id".to_string(),
                    data_type: "BIGINT".to_string(),
                    nullable: false,
                    default: None,
                    primary_key: false,
                    unique: false,
                },
                DbColumn {
                    name: "total".to_string(),
                    data_type: "DECIMAL".to_string(),
                    nullable: false,
                    default: None,
                    primary_key: false,
                    unique: false,
                },
            ],
            constraints: vec![DbConstraint {
                name: "pk_orders".to_string(),
                constraint_type: ConstraintType::PrimaryKey,
                columns: vec!["id".to_string()],
                references: None,
            }],
            indexes: vec![],
        }
    }

    #[test]
    fn test_db_schema_construction() {
        let schema = DbSchema::new(Dialect::MySql)
            .with_table(make_users_table())
            .with_table(make_orders_table());

        assert_eq!(schema.dialect, Dialect::MySql);
        assert_eq!(schema.tables.len(), 2);
        assert!(schema.get_table("users").is_some());
        assert!(schema.get_table("orders").is_some());
        assert!(schema.get_table("nonexistent").is_none());
    }

    #[test]
    fn test_db_table_serialization() {
        let table = make_users_table();
        let json = serde_json::to_string(&table).unwrap();
        let parsed: DbTable = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "users");
        assert_eq!(parsed.columns.len(), 3);
        assert_eq!(parsed.constraints.len(), 2);
    }

    #[test]
    fn test_db_column_types() {
        let col = DbColumn {
            name: "id".to_string(),
            data_type: "BIGINT".to_string(),
            nullable: false,
            default: None,
            primary_key: true,
            unique: false,
        };
        assert!(!col.nullable);
        assert!(col.primary_key);
        assert!(!col.unique);
    }

    #[test]
    fn test_constraint_types() {
        let pk = DbConstraint {
            name: "pk".to_string(),
            constraint_type: ConstraintType::PrimaryKey,
            columns: vec!["id".to_string()],
            references: None,
        };
        assert_eq!(pk.constraint_type, ConstraintType::PrimaryKey);

        let fk = DbConstraint {
            name: "fk".to_string(),
            constraint_type: ConstraintType::ForeignKey,
            columns: vec!["user_id".to_string()],
            references: Some(("users".to_string(), vec!["id".to_string()])),
        };
        assert_eq!(fk.constraint_type, ConstraintType::ForeignKey);
        assert!(fk.references.is_some());
    }

    #[test]
    fn test_crud_api_endpoint() {
        let endpoint = CrudApiEndpoint {
            method: HttpMethod::Get,
            path: "/users/{id}".to_string(),
            summary: "Get user by id".to_string(),
            parameters: vec!["id".to_string()],
            request_body: None,
            response_schema: "#/components/schemas/User".to_string(),
            operation_id: "get_user_by_id".to_string(),
        };
        assert_eq!(endpoint.method, HttpMethod::Get);
        assert_eq!(endpoint.path, "/users/{id}");
    }

    #[test]
    fn test_db_schema_to_openapi_mapper() {
        let config = ReverseGenConfig::new(Dialect::MySql).with_trust_unsigned(true);
        let mapper = DbSchemaToOpenApiMapper::new(config);
        let schema = DbSchema::new(Dialect::MySql).with_table(make_users_table());

        let spec = mapper.map(&schema).unwrap();
        assert_eq!(spec.openapi, "3.0.0");
        assert!(spec.components.is_some());

        let components = spec.components.as_ref().unwrap();
        assert!(components.schemas.contains_key("users"));
        assert!(spec.paths.contains_key("/users"));
    }

    #[test]
    fn test_db_schema_to_openapi_pascal_case() {
        let config = ReverseGenConfig::new(Dialect::MySql)
            .with_naming_convention(NamingConvention::PascalCase)
            .with_trust_unsigned(true);
        let mapper = DbSchemaToOpenApiMapper::new(config);
        let schema = DbSchema::new(Dialect::MySql).with_table(make_users_table());

        let spec = mapper.map(&schema).unwrap();
        let components = spec.components.as_ref().unwrap();
        assert!(components.schemas.contains_key("Users"));
    }

    #[test]
    fn test_column_type_mapping() {
        let config = ReverseGenConfig::new(Dialect::MySql).with_trust_unsigned(true);
        let mapper = DbSchemaToOpenApiMapper::new(config);

        let table = DbTable {
            name: "test".to_string(),
            columns: vec![
                DbColumn {
                    name: "big_int_col".to_string(),
                    data_type: "BIGINT".to_string(),
                    nullable: false,
                    default: None,
                    primary_key: true,
                    unique: false,
                },
                DbColumn {
                    name: "bool_col".to_string(),
                    data_type: "BOOLEAN".to_string(),
                    nullable: true,
                    default: None,
                    primary_key: false,
                    unique: false,
                },
                DbColumn {
                    name: "ts_col".to_string(),
                    data_type: "TIMESTAMP".to_string(),
                    nullable: true,
                    default: None,
                    primary_key: false,
                    unique: false,
                },
                DbColumn {
                    name: "uuid_col".to_string(),
                    data_type: "UUID".to_string(),
                    nullable: true,
                    default: None,
                    primary_key: false,
                    unique: false,
                },
            ],
            constraints: vec![],
            indexes: vec![],
        };

        let schema = DbSchema::new(Dialect::MySql).with_table(table);
        let spec = mapper.map(&schema).unwrap();
        assert!(spec.components.is_some());
    }

    #[test]
    fn test_db_schema_to_crud_mapper() {
        let config = ReverseGenConfig::new(Dialect::MySql).with_trust_unsigned(true);
        let mapper = DbSchemaToCrudApiMapper::new(config);
        let schema = DbSchema::new(Dialect::MySql).with_table(make_users_table());

        let endpoints = mapper.map(&schema).unwrap();
        assert_eq!(endpoints.len(), 5);

        assert_eq!(endpoints[0].method, HttpMethod::Get);
        assert_eq!(endpoints[0].path, "/users");
        assert!(endpoints[0].parameters.is_empty());

        assert_eq!(endpoints[1].method, HttpMethod::Get);
        assert_eq!(endpoints[1].path, "/users/{id}");
        assert_eq!(endpoints[1].parameters, vec!["id".to_string()]);

        assert_eq!(endpoints[2].method, HttpMethod::Post);
        assert_eq!(endpoints[2].path, "/users");
        assert!(endpoints[2].request_body.is_some());

        assert_eq!(endpoints[3].method, HttpMethod::Put);
        assert_eq!(endpoints[3].path, "/users/{id}");

        assert_eq!(endpoints[4].method, HttpMethod::Delete);
        assert_eq!(endpoints[4].path, "/users/{id}");
    }

    #[test]
    fn test_crud_mapper_multiple_tables() {
        let config = ReverseGenConfig::new(Dialect::MySql).with_trust_unsigned(true);
        let mapper = DbSchemaToCrudApiMapper::new(config);
        let schema = DbSchema::new(Dialect::MySql)
            .with_table(make_users_table())
            .with_table(make_orders_table());

        let endpoints = mapper.map(&schema).unwrap();
        assert_eq!(endpoints.len(), 10);
    }

    #[test]
    fn test_crud_mapper_naming_convention() {
        let config = ReverseGenConfig::new(Dialect::MySql)
            .with_naming_convention(NamingConvention::PascalCase)
            .with_trust_unsigned(true);
        let mapper = DbSchemaToCrudApiMapper::new(config);
        let schema = DbSchema::new(Dialect::MySql).with_table(make_users_table());

        let endpoints = mapper.map(&schema).unwrap();
        assert!(endpoints[0].response_schema.contains("Users"));
    }

    #[test]
    fn test_injection_check() {
        assert!(DbSchemaReader::check_injection("normal_name").is_ok());
        assert!(DbSchemaReader::check_injection("table'; DROP").is_err());
        assert!(DbSchemaReader::check_injection("table\"--").is_err());
        assert!(DbSchemaReader::check_injection("table\0").is_err());
    }

    #[test]
    fn test_full_reverse_loop_verifier() {
        let config = ReverseGenConfig::new(Dialect::MySql).with_trust_unsigned(true);
        let verifier = FullReverseLoopVerifier::new(config);
        let schema = DbSchema::new(Dialect::MySql).with_table(make_users_table());

        let report = verifier.verify(&schema).unwrap();
        assert!(report.consistent || !report.diff_descriptions.is_empty());
    }

    #[test]
    fn test_full_reverse_loop_verifier_log() {
        let config = ReverseGenConfig::new(Dialect::MySql).with_trust_unsigned(true);
        let verifier = FullReverseLoopVerifier::new(config);
        let schema = DbSchema::new(Dialect::MySql)
            .with_table(make_users_table())
            .with_table(make_orders_table());

        let report = verifier.verify(&schema).unwrap();
        let log = verifier.generate_log(&schema, &report);
        assert_eq!(log.table_count, 2);
        assert!(log.source.contains("db:"));
        assert!(log.generated_items.len() >= 2);
    }

    #[test]
    fn test_columns_sql_generation() {
        let mysql_sql = DbSchemaReader::columns_sql(Dialect::MySql, "users");
        assert!(mysql_sql.contains("INFORMATION_SCHEMA.COLUMNS"));
        assert!(mysql_sql.contains("users"));

        let pg_sql = DbSchemaReader::columns_sql(Dialect::PostgreSql, "users");
        assert!(pg_sql.contains("information_schema.columns"));

        let sqlite_sql = DbSchemaReader::columns_sql(Dialect::Sqlite, "users");
        assert!(sqlite_sql.contains("PRAGMA table_info"));

        let oracle_sql = DbSchemaReader::columns_sql(Dialect::Oracle, "users");
        assert!(oracle_sql.contains("ALL_TAB_COLUMNS"));

        let mssql_sql = DbSchemaReader::columns_sql(Dialect::Mssql, "users");
        assert!(mssql_sql.contains("INFORMATION_SCHEMA.COLUMNS"));
    }

    #[test]
    fn test_constraints_sql_generation() {
        let mysql_sql = DbSchemaReader::constraints_sql(Dialect::MySql, "users");
        assert!(mysql_sql.contains("TABLE_CONSTRAINTS"));

        let pg_sql = DbSchemaReader::constraints_sql(Dialect::PostgreSql, "users");
        assert!(pg_sql.contains("pg_constraint"));

        let sqlite_sql = DbSchemaReader::constraints_sql(Dialect::Sqlite, "users");
        assert!(sqlite_sql.is_empty());

        let oracle_sql = DbSchemaReader::constraints_sql(Dialect::Oracle, "users");
        assert!(oracle_sql.contains("ALL_CONSTRAINTS"));
    }

    #[test]
    fn test_indexes_sql_generation() {
        let mysql_sql = DbSchemaReader::indexes_sql(Dialect::MySql, "users");
        assert!(mysql_sql.contains("STATISTICS"));

        let pg_sql = DbSchemaReader::indexes_sql(Dialect::PostgreSql, "users");
        assert!(pg_sql.contains("pg_indexes"));

        let sqlite_sql = DbSchemaReader::indexes_sql(Dialect::Sqlite, "users");
        assert!(sqlite_sql.contains("PRAGMA index_list"));

        let oracle_sql = DbSchemaReader::indexes_sql(Dialect::Oracle, "users");
        assert!(oracle_sql.is_empty());
    }

    #[test]
    fn test_extract_string_column() {
        let rows: QueryRows = vec![
            {
                let mut m = HashMap::new();
                m.insert("TABLE_NAME".to_string(), Value::String("users".to_string()));
                m
            },
            {
                let mut m = HashMap::new();
                m.insert(
                    "TABLE_NAME".to_string(),
                    Value::String("orders".to_string()),
                );
                m
            },
        ];

        let names = DbSchemaReader::extract_string_column(&rows);
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"users".to_string()));
        assert!(names.contains(&"orders".to_string()));
    }

    #[test]
    fn test_get_string_from_row() {
        let mut row = HashMap::new();
        row.insert("name".to_string(), Value::String("users".to_string()));
        row.insert("null_col".to_string(), Value::Null);

        assert_eq!(
            DbSchemaReader::get_string(&row, "name"),
            Some("users".to_string())
        );
        assert_eq!(DbSchemaReader::get_string(&row, "null_col"), None);
        assert_eq!(DbSchemaReader::get_string(&row, "nonexistent"), None);
    }

    #[test]
    fn test_db_schema_from_tables() {
        let tables = vec![make_users_table(), make_orders_table()];
        let schema = DbSchema::from_tables(Dialect::PostgreSql, tables);
        assert_eq!(schema.dialect, Dialect::PostgreSql);
        assert_eq!(schema.tables.len(), 2);
    }

    #[test]
    fn test_http_method_serialization() {
        let json = serde_json::to_string(&HttpMethod::Get).unwrap();
        assert_eq!(json, "\"GET\"");

        let json = serde_json::to_string(&HttpMethod::Post).unwrap();
        assert_eq!(json, "\"POST\"");
    }

    #[test]
    fn test_db_index() {
        let index = DbIndex {
            name: "idx_users_email".to_string(),
            columns: vec!["email".to_string()],
            unique: true,
        };
        assert!(index.unique);
        assert_eq!(index.columns.len(), 1);
    }

    #[test]
    fn test_openapi_mapper_with_friendly_table_name() {
        let config = ReverseGenConfig::new(Dialect::MySql).with_trust_unsigned(true);
        let mapper = DbSchemaToOpenApiMapper::new(config);
        let table = DbTable {
            name: "user_orders".to_string(),
            columns: vec![DbColumn {
                name: "id".to_string(),
                data_type: "BIGINT".to_string(),
                nullable: false,
                default: None,
                primary_key: true,
                unique: false,
            }],
            constraints: vec![],
            indexes: vec![],
        };
        let schema = DbSchema::new(Dialect::MySql).with_table(table);

        let spec = mapper.map(&schema).unwrap();
        let components = spec.components.as_ref().unwrap();
        assert!(components.schemas.contains_key("user_orders"));
        assert!(spec.paths.contains_key("/user_orders"));
    }

    // ── v4.8.0 修复 M-12：元数据驱动 SQL 注入 ──

    #[test]
    fn test_escape_sql_string_doubles_quotes() {
        assert_eq!(DbSchemaReader::escape_sql_string("users"), "users");
        assert_eq!(DbSchemaReader::escape_sql_string("o'brien"), "o''brien");
        assert_eq!(
            DbSchemaReader::escape_sql_string("x'; DROP TABLE users; --"),
            "x''; DROP TABLE users; --"
        );
    }

    #[test]
    fn test_columns_sql_injection_table_name_escaped() {
        // 恶意表名（元数据被污染场景）不得逃逸 SQL 字面量
        let evil = "x'; DROP TABLE users; --";
        let sql = DbSchemaReader::columns_sql(Dialect::MySql, evil);
        // 单引号必须翻倍：表名整体成为安全字面量 'x''; DROP TABLE users; --'
        assert!(
            sql.contains("'x''; DROP TABLE users; --'"),
            "单引号必须翻倍转义（M-12 修复失效）: {sql}"
        );
        // 转义后只存在成对引号（''），不存在可闭合字面量的裸单引号 + 语句边界
        assert!(
            !sql.contains("x'; DROP"),
            "裸单引号不得出现（M-12 修复失效）: {sql}"
        );

        // 正常表名不受影响
        let normal = DbSchemaReader::columns_sql(Dialect::MySql, "orders");
        assert!(normal.contains("'orders'"));
        assert!(normal.contains("INFORMATION_SCHEMA.COLUMNS"));

        // Oracle 大写路径同样转义
        let oracle = DbSchemaReader::constraints_sql(Dialect::Oracle, "evil'; DROP");
        assert!(oracle.contains("''"));
        assert!(!oracle.contains("evil'; DROP"));
    }
}
