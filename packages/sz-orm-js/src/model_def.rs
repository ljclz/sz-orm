//! 模型定义支持：字段类型、索引、关系定义。
//!
//! - [`ModelDefinition`] — 模型定义（表名、字段、索引、关系）
//! - [`FieldDefinition`] — 字段定义（名称、类型、约束）
//! - [`IndexDefinition`] — 索引定义
//! - [`RelationDefinition`] — 关系定义（一对多、多对一）

use napi_derive::napi;
use sz_orm_core::dialect::get_dialect;
use sz_orm_core::DbType;

type Result<T> = napi::bindgen_prelude::Result<T>;

fn parse_db_type(s: &str) -> Result<DbType> {
    DbType::from_str(s).ok_or_else(|| napi::Error::from_reason(format!("unknown DbType: {}", s)))
}

fn dialect_or_err(db_type: DbType) -> Result<Box<dyn sz_orm_core::dialect::Dialect>> {
    get_dialect(db_type).map_err(|e| napi::Error::from_reason(e.to_string()))
}

// ============================================================================
// 字段类型
// ============================================================================

/// 字段数据类型
#[napi]
#[derive(Debug, PartialEq, Eq)]
pub enum FieldType {
    /// 自增主键
    AutoIncrement,
    /// 整数
    Integer,
    /// 长整数
    BigInteger,
    /// 浮点数
    Float,
    /// 双精度浮点
    Double,
    /// 定长字符串
    String,
    /// 文本
    Text,
    /// 布尔
    Boolean,
    /// 日期
    Date,
    /// 日期时间
    DateTime,
    /// 时间
    Time,
    /// JSON
    Json,
    /// 二进制
    Bytes,
    /// UUID
    Uuid,
    /// 十进制
    Decimal,
}

impl FieldType {
    /// 转换为 SQL 类型字符串
    pub fn to_sql_type(&self, db_type: DbType) -> &'static str {
        match (self, db_type) {
            (FieldType::AutoIncrement, DbType::PostgreSQL) => "SERIAL",
            (FieldType::AutoIncrement, DbType::MySQL) => "INT AUTO_INCREMENT",
            (FieldType::AutoIncrement, _) => "INTEGER AUTOINCREMENT",
            (FieldType::Integer, DbType::PostgreSQL) => "INTEGER",
            (FieldType::Integer, DbType::MySQL) => "INT",
            (FieldType::Integer, _) => "INTEGER",
            (FieldType::BigInteger, DbType::PostgreSQL) => "BIGINT",
            (FieldType::BigInteger, DbType::MySQL) => "BIGINT",
            (FieldType::BigInteger, _) => "BIGINT",
            (FieldType::Float, _) => "FLOAT",
            (FieldType::Double, DbType::PostgreSQL) => "DOUBLE PRECISION",
            (FieldType::Double, _) => "DOUBLE",
            (FieldType::String, _) => "VARCHAR(255)",
            (FieldType::Text, DbType::PostgreSQL) => "TEXT",
            (FieldType::Text, DbType::MySQL) => "LONGTEXT",
            (FieldType::Text, _) => "TEXT",
            (FieldType::Boolean, DbType::PostgreSQL) => "BOOLEAN",
            (FieldType::Boolean, DbType::MySQL) => "TINYINT(1)",
            (FieldType::Boolean, _) => "BOOLEAN",
            (FieldType::Date, _) => "DATE",
            (FieldType::DateTime, DbType::PostgreSQL) => "TIMESTAMP",
            (FieldType::DateTime, DbType::MySQL) => "DATETIME",
            (FieldType::DateTime, _) => "DATETIME",
            (FieldType::Time, _) => "TIME",
            (FieldType::Json, DbType::PostgreSQL) => "JSONB",
            (FieldType::Json, DbType::MySQL) => "JSON",
            (FieldType::Json, _) => "TEXT",
            (FieldType::Bytes, DbType::PostgreSQL) => "BYTEA",
            (FieldType::Bytes, DbType::MySQL) => "BLOB",
            (FieldType::Bytes, _) => "BLOB",
            (FieldType::Uuid, DbType::PostgreSQL) => "UUID",
            (FieldType::Uuid, _) => "VARCHAR(36)",
            (FieldType::Decimal, _) => "DECIMAL(19,4)",
        }
    }

    /// 是否是数值类型
    pub fn is_numeric(&self) -> bool {
        matches!(
            self,
            FieldType::AutoIncrement
                | FieldType::Integer
                | FieldType::BigInteger
                | FieldType::Float
                | FieldType::Double
                | FieldType::Decimal
        )
    }

    /// 是否是字符串类型
    pub fn is_string_like(&self) -> bool {
        matches!(
            self,
            FieldType::String | FieldType::Text | FieldType::Json | FieldType::Uuid
        )
    }

    /// 是否是时间类型
    pub fn is_temporal(&self) -> bool {
        matches!(
            self,
            FieldType::Date | FieldType::DateTime | FieldType::Time
        )
    }
}

// ============================================================================
// 字段定义
// ============================================================================

/// 字段定义
#[napi]
pub struct FieldDefinition {
    name: String,
    field_type: FieldType,
    nullable: bool,
    unique: bool,
    primary_key: bool,
    default_value: Option<String>,
    indexed: bool,
    comment: String,
}

#[napi]
impl FieldDefinition {
    /// 创建字段定义
    #[napi(constructor)]
    pub fn new(name: String, field_type: FieldType) -> Self {
        Self {
            name,
            field_type,
            nullable: true,
            unique: false,
            primary_key: false,
            default_value: None,
            indexed: false,
            comment: String::new(),
        }
    }

    /// 设置非空（链式）
    #[napi]
    pub fn not_null(&mut self) {
        self.nullable = false;
    }

    /// 设置唯一约束（链式）
    #[napi]
    pub fn set_unique(&mut self) {
        self.unique = true;
    }

    /// 设置主键（链式）
    #[napi]
    pub fn set_primary_key(&mut self) {
        self.primary_key = true;
        self.nullable = false;
    }

    /// 设置默认值（链式）
    #[napi]
    pub fn set_default(&mut self, value: String) {
        self.default_value = Some(value);
    }

    /// 设置索引（链式）
    #[napi]
    pub fn set_indexed(&mut self) {
        self.indexed = true;
    }

    /// 设置注释（链式）
    #[napi]
    pub fn set_comment(&mut self, comment: String) {
        self.comment = comment;
    }

    /// 字段名
    #[napi(getter)]
    pub fn name(&self) -> String {
        self.name.clone()
    }

    /// 是否可空
    #[napi(getter)]
    pub fn is_nullable(&self) -> bool {
        self.nullable
    }

    /// 是否唯一
    #[napi(getter)]
    pub fn is_unique(&self) -> bool {
        self.unique
    }

    /// 是否主键
    #[napi(getter)]
    pub fn is_primary_key(&self) -> bool {
        self.primary_key
    }

    /// 是否有索引
    #[napi(getter)]
    pub fn is_indexed(&self) -> bool {
        self.indexed
    }

    /// 默认值
    #[napi(getter)]
    pub fn default_value(&self) -> Option<String> {
        self.default_value.clone()
    }

    /// 生成列定义 SQL 片段
    pub fn to_column_sql(&self, db_type: DbType) -> String {
        let dialect = match dialect_or_err(db_type) {
            Ok(d) => d,
            Err(_) => return String::new(),
        };
        let mut sql = format!(
            "{} {}",
            dialect.quote(&self.name),
            self.field_type.to_sql_type(db_type)
        );
        if !self.nullable {
            sql.push_str(" NOT NULL");
        }
        if self.unique {
            sql.push_str(" UNIQUE");
        }
        if self.primary_key {
            sql.push_str(" PRIMARY KEY");
        }
        if let Some(ref default) = self.default_value {
            sql.push_str(&format!(" DEFAULT {}", default));
        }
        sql
    }
}

// ============================================================================
// 索引定义
// ============================================================================

/// 索引定义
#[napi]
pub struct IndexDefinition {
    name: String,
    columns: Vec<String>,
    unique: bool,
}

#[napi]
impl IndexDefinition {
    /// 创建索引定义
    #[napi(constructor)]
    pub fn new(name: String, columns: Vec<String>) -> Self {
        Self {
            name,
            columns,
            unique: false,
        }
    }

    /// 设置唯一索引（链式）
    #[napi]
    pub fn set_unique(&mut self) {
        self.unique = true;
    }

    /// 索引名
    #[napi(getter)]
    pub fn name(&self) -> String {
        self.name.clone()
    }

    /// 列列表
    #[napi(getter)]
    pub fn columns(&self) -> Vec<String> {
        self.columns.clone()
    }

    /// 是否唯一
    #[napi(getter)]
    pub fn is_unique(&self) -> bool {
        self.unique
    }

    /// 生成 CREATE INDEX SQL
    pub fn to_sql(&self, table: &str, db_type: DbType) -> String {
        let dialect = match dialect_or_err(db_type) {
            Ok(d) => d,
            Err(_) => return String::new(),
        };
        let cols: Vec<String> = self.columns.iter().map(|c| dialect.quote(c)).collect();
        let unique_kw = if self.unique { "UNIQUE " } else { "" };
        format!(
            "CREATE {}INDEX {} ON {} ({})",
            unique_kw,
            dialect.quote(&self.name),
            dialect.quote(table),
            cols.join(", ")
        )
    }
}

// ============================================================================
// 关系定义
// ============================================================================

/// 关系类型
#[napi]
#[derive(Debug, PartialEq, Eq)]
pub enum RelationType {
    /// 一对多
    OneToMany,
    /// 多对一
    ManyToOne,
    /// 一对一
    OneToOne,
}

/// 关系定义
#[napi]
pub struct RelationDefinition {
    name: String,
    relation_type: RelationType,
    target_model: String,
    foreign_key: String,
    target_key: String,
}

#[napi]
impl RelationDefinition {
    /// 创建关系定义
    #[napi(constructor)]
    pub fn new(
        name: String,
        relation_type: RelationType,
        target_model: String,
        foreign_key: String,
    ) -> Self {
        Self {
            name,
            relation_type,
            target_model,
            foreign_key,
            target_key: "id".to_string(),
        }
    }

    /// 设置目标键（链式）
    #[napi]
    pub fn set_target_key(&mut self, key: String) {
        self.target_key = key;
    }

    /// 关系名
    #[napi(getter)]
    pub fn name(&self) -> String {
        self.name.clone()
    }

    /// 目标模型
    #[napi(getter)]
    pub fn target_model(&self) -> String {
        self.target_model.clone()
    }

    /// 外键
    #[napi(getter)]
    pub fn foreign_key(&self) -> String {
        self.foreign_key.clone()
    }

    /// 目标键
    #[napi(getter)]
    pub fn target_key(&self) -> String {
        self.target_key.clone()
    }
}

// ============================================================================
// 模型定义
// ============================================================================

/// 模型定义：表名、字段、索引、关系
#[napi]
pub struct ModelDefinition {
    db_type: DbType,
    table_name: String,
    fields: Vec<FieldDefinition>,
    indexes: Vec<IndexDefinition>,
    relations: Vec<RelationDefinition>,
}

#[napi]
impl ModelDefinition {
    /// 创建模型定义
    #[napi(constructor)]
    pub fn new(db_type: Option<String>, table_name: String) -> Result<Self> {
        let dt = db_type.unwrap_or_else(|| "mysql".to_string());
        Ok(Self {
            db_type: parse_db_type(&dt)?,
            table_name,
            fields: vec![],
            indexes: vec![],
            relations: vec![],
        })
    }

    /// 添加字段（链式）
    pub fn add_field(&mut self, field: FieldDefinition) {
        self.fields.push(field);
    }

    /// 添加索引（链式）
    pub fn add_index(&mut self, index: IndexDefinition) {
        self.indexes.push(index);
    }

    /// 添加关系（链式）
    pub fn add_relation(&mut self, relation: RelationDefinition) {
        self.relations.push(relation);
    }

    /// 表名
    #[napi(getter)]
    pub fn table_name(&self) -> String {
        self.table_name.clone()
    }

    /// 字段数
    #[napi(getter)]
    pub fn field_count(&self) -> u32 {
        self.fields.len() as u32
    }

    /// 索引数
    #[napi(getter)]
    pub fn index_count(&self) -> u32 {
        self.indexes.len() as u32
    }

    /// 关系数
    #[napi(getter)]
    pub fn relation_count(&self) -> u32 {
        self.relations.len() as u32
    }

    /// 生成 CREATE TABLE SQL
    #[napi]
    pub fn to_create_table_sql(&self) -> Result<String> {
        let dialect = dialect_or_err(self.db_type)?;
        if self.fields.is_empty() {
            return Err(napi::Error::from_reason("no fields defined"));
        }
        let columns: Vec<String> = self
            .fields
            .iter()
            .map(|f| f.to_column_sql(self.db_type))
            .collect();
        Ok(format!(
            "CREATE TABLE {} (\n  {}\n)",
            dialect.quote(&self.table_name),
            columns.join(",\n  ")
        ))
    }

    /// 生成所有 CREATE INDEX SQL
    #[napi]
    pub fn to_create_index_sqls(&self) -> Vec<String> {
        self.indexes
            .iter()
            .map(|idx| idx.to_sql(&self.table_name, self.db_type))
            .collect()
    }

    /// 生成 DROP TABLE SQL
    #[napi]
    pub fn to_drop_table_sql(&self) -> Result<String> {
        let dialect = dialect_or_err(self.db_type)?;
        Ok(format!(
            "DROP TABLE IF EXISTS {}",
            dialect.quote(&self.table_name)
        ))
    }

    /// 模型摘要
    #[napi]
    pub fn summary(&self) -> String {
        format!(
            "Model({}, fields={}, indexes={}, relations={})",
            self.table_name,
            self.fields.len(),
            self.indexes.len(),
            self.relations.len()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ----- FieldType -----

    #[test]
    fn field_type_to_sql_type_mysql() {
        assert_eq!(FieldType::Integer.to_sql_type(DbType::MySQL), "INT");
        assert_eq!(FieldType::BigInteger.to_sql_type(DbType::MySQL), "BIGINT");
        assert_eq!(FieldType::Boolean.to_sql_type(DbType::MySQL), "TINYINT(1)");
        assert_eq!(FieldType::Json.to_sql_type(DbType::MySQL), "JSON");
        assert_eq!(FieldType::Bytes.to_sql_type(DbType::MySQL), "BLOB");
        assert_eq!(FieldType::DateTime.to_sql_type(DbType::MySQL), "DATETIME");
    }

    #[test]
    fn field_type_to_sql_type_postgres() {
        assert_eq!(
            FieldType::AutoIncrement.to_sql_type(DbType::PostgreSQL),
            "SERIAL"
        );
        assert_eq!(
            FieldType::Boolean.to_sql_type(DbType::PostgreSQL),
            "BOOLEAN"
        );
        assert_eq!(FieldType::Json.to_sql_type(DbType::PostgreSQL), "JSONB");
        assert_eq!(FieldType::Bytes.to_sql_type(DbType::PostgreSQL), "BYTEA");
        assert_eq!(
            FieldType::DateTime.to_sql_type(DbType::PostgreSQL),
            "TIMESTAMP"
        );
        assert_eq!(
            FieldType::Double.to_sql_type(DbType::PostgreSQL),
            "DOUBLE PRECISION"
        );
        assert_eq!(FieldType::Uuid.to_sql_type(DbType::PostgreSQL), "UUID");
    }

    #[test]
    fn field_type_to_sql_type_sqlite() {
        assert_eq!(FieldType::Integer.to_sql_type(DbType::Sqlite), "INTEGER");
        assert_eq!(FieldType::Boolean.to_sql_type(DbType::Sqlite), "BOOLEAN");
    }

    #[test]
    fn field_type_is_numeric() {
        assert!(FieldType::Integer.is_numeric());
        assert!(FieldType::BigInteger.is_numeric());
        assert!(FieldType::Float.is_numeric());
        assert!(FieldType::Double.is_numeric());
        assert!(FieldType::Decimal.is_numeric());
        assert!(FieldType::AutoIncrement.is_numeric());
        assert!(!FieldType::String.is_numeric());
        assert!(!FieldType::Boolean.is_numeric());
    }

    #[test]
    fn field_type_is_string_like() {
        assert!(FieldType::String.is_string_like());
        assert!(FieldType::Text.is_string_like());
        assert!(FieldType::Json.is_string_like());
        assert!(FieldType::Uuid.is_string_like());
        assert!(!FieldType::Integer.is_string_like());
    }

    #[test]
    fn field_type_is_temporal() {
        assert!(FieldType::Date.is_temporal());
        assert!(FieldType::DateTime.is_temporal());
        assert!(FieldType::Time.is_temporal());
        assert!(!FieldType::Integer.is_temporal());
    }

    // ----- FieldDefinition -----

    #[test]
    fn field_definition_new() {
        let f = FieldDefinition::new("name".to_string(), FieldType::String);
        assert_eq!(f.name(), "name");
        assert!(f.is_nullable());
        assert!(!f.is_unique());
        assert!(!f.is_primary_key());
    }

    #[test]
    fn field_definition_not_null() {
        let mut f = FieldDefinition::new("name".to_string(), FieldType::String);
        f.not_null();
        assert!(!f.is_nullable());
    }

    #[test]
    fn field_definition_unique() {
        let mut f = FieldDefinition::new("email".to_string(), FieldType::String);
        f.set_unique();
        assert!(f.is_unique());
    }

    #[test]
    fn field_definition_primary_key() {
        let mut f = FieldDefinition::new("id".to_string(), FieldType::AutoIncrement);
        f.set_primary_key();
        assert!(f.is_primary_key());
        assert!(!f.is_nullable());
    }

    #[test]
    fn field_definition_default_value() {
        let mut f = FieldDefinition::new("active".to_string(), FieldType::Boolean);
        f.set_default("true".to_string());
        assert_eq!(f.default_value(), Some("true".to_string()));
    }

    #[test]
    fn field_definition_indexed() {
        let mut f = FieldDefinition::new("email".to_string(), FieldType::String);
        f.set_indexed();
        assert!(f.is_indexed());
    }

    #[test]
    fn field_definition_to_column_sql() {
        let mut f = FieldDefinition::new("name".to_string(), FieldType::String);
        f.not_null();
        let sql = f.to_column_sql(DbType::MySQL);
        assert!(sql.contains("name"));
        assert!(sql.contains("NOT NULL"));
    }

    #[test]
    fn field_definition_to_column_sql_with_default() {
        let mut f = FieldDefinition::new("active".to_string(), FieldType::Boolean);
        f.set_default("1".to_string());
        let sql = f.to_column_sql(DbType::MySQL);
        assert!(sql.contains("DEFAULT 1"));
    }

    #[test]
    fn field_definition_to_column_sql_primary_key() {
        let mut f = FieldDefinition::new("id".to_string(), FieldType::AutoIncrement);
        f.set_primary_key();
        let sql = f.to_column_sql(DbType::MySQL);
        assert!(sql.contains("PRIMARY KEY"));
    }

    // ----- IndexDefinition -----

    #[test]
    fn index_definition_new() {
        let idx = IndexDefinition::new("idx_email".to_string(), vec!["email".to_string()]);
        assert_eq!(idx.name(), "idx_email");
        assert_eq!(idx.columns().len(), 1);
        assert!(!idx.is_unique());
    }

    #[test]
    fn index_definition_unique() {
        let mut idx = IndexDefinition::new("idx_email".to_string(), vec!["email".to_string()]);
        idx.set_unique();
        assert!(idx.is_unique());
    }

    #[test]
    fn index_definition_to_sql() {
        let idx = IndexDefinition::new("idx_name".to_string(), vec!["name".to_string()]);
        let sql = idx.to_sql("users", DbType::MySQL);
        assert!(sql.contains("CREATE INDEX"));
        assert!(sql.contains("users"));
        assert!(sql.contains("name"));
    }

    #[test]
    fn index_definition_to_sql_unique() {
        let mut idx = IndexDefinition::new("idx_email".to_string(), vec!["email".to_string()]);
        idx.set_unique();
        let sql = idx.to_sql("users", DbType::MySQL);
        assert!(sql.contains("UNIQUE"));
    }

    #[test]
    fn index_definition_multi_column() {
        let idx = IndexDefinition::new(
            "idx_comp".to_string(),
            vec!["a".to_string(), "b".to_string()],
        );
        let sql = idx.to_sql("t", DbType::MySQL);
        assert!(sql.contains("a"));
        assert!(sql.contains("b"));
    }

    // ----- RelationDefinition -----

    #[test]
    fn relation_definition_new() {
        let r = RelationDefinition::new(
            "posts".to_string(),
            RelationType::OneToMany,
            "Post".to_string(),
            "user_id".to_string(),
        );
        assert_eq!(r.name(), "posts");
        assert_eq!(r.target_model(), "Post");
        assert_eq!(r.foreign_key(), "user_id");
        assert_eq!(r.target_key(), "id");
    }

    #[test]
    fn relation_definition_target_key() {
        let mut r = RelationDefinition::new(
            "r".to_string(),
            RelationType::ManyToOne,
            "User".to_string(),
            "uid".to_string(),
        );
        r.set_target_key("uuid".to_string());
        assert_eq!(r.target_key(), "uuid");
    }

    // ----- ModelDefinition -----

    #[test]
    fn model_definition_new() {
        let m = ModelDefinition::new(None, "users".to_string()).unwrap();
        assert_eq!(m.table_name(), "users");
        assert_eq!(m.field_count(), 0);
        assert_eq!(m.index_count(), 0);
        assert_eq!(m.relation_count(), 0);
    }

    #[test]
    fn model_definition_add_field() {
        let mut m = ModelDefinition::new(None, "users".to_string()).unwrap();
        m.add_field(FieldDefinition::new(
            "id".to_string(),
            FieldType::AutoIncrement,
        ));
        assert_eq!(m.field_count(), 1);
    }

    #[test]
    fn model_definition_add_index() {
        let mut m = ModelDefinition::new(None, "users".to_string()).unwrap();
        m.add_index(IndexDefinition::new(
            "idx".to_string(),
            vec!["name".to_string()],
        ));
        assert_eq!(m.index_count(), 1);
    }

    #[test]
    fn model_definition_add_relation() {
        let mut m = ModelDefinition::new(None, "users".to_string()).unwrap();
        m.add_relation(RelationDefinition::new(
            "posts".to_string(),
            RelationType::OneToMany,
            "Post".to_string(),
            "user_id".to_string(),
        ));
        assert_eq!(m.relation_count(), 1);
    }

    #[test]
    fn model_definition_to_create_table_sql() {
        let mut m = ModelDefinition::new(None, "users".to_string()).unwrap();
        m.add_field(FieldDefinition::new(
            "id".to_string(),
            FieldType::AutoIncrement,
        ));
        m.add_field(FieldDefinition::new("name".to_string(), FieldType::String));
        let sql = m.to_create_table_sql().unwrap();
        assert!(sql.contains("CREATE TABLE"));
        assert!(sql.contains("id"));
        assert!(sql.contains("name"));
    }

    #[test]
    fn model_definition_to_create_table_sql_empty_error() {
        let m = ModelDefinition::new(None, "users".to_string()).unwrap();
        assert!(m.to_create_table_sql().is_err());
    }

    #[test]
    fn model_definition_to_create_index_sqls() {
        let mut m = ModelDefinition::new(None, "users".to_string()).unwrap();
        m.add_index(IndexDefinition::new(
            "idx1".to_string(),
            vec!["a".to_string()],
        ));
        m.add_index(IndexDefinition::new(
            "idx2".to_string(),
            vec!["b".to_string()],
        ));
        let sqls = m.to_create_index_sqls();
        assert_eq!(sqls.len(), 2);
    }

    #[test]
    fn model_definition_to_drop_table_sql() {
        let m = ModelDefinition::new(None, "users".to_string()).unwrap();
        let sql = m.to_drop_table_sql().unwrap();
        assert!(sql.contains("DROP TABLE"));
        assert!(sql.contains("IF EXISTS"));
    }

    #[test]
    fn model_definition_summary() {
        let mut m = ModelDefinition::new(None, "users".to_string()).unwrap();
        m.add_field(FieldDefinition::new(
            "id".to_string(),
            FieldType::AutoIncrement,
        ));
        let s = m.summary();
        assert!(s.contains("users"));
        assert!(s.contains("fields=1"));
    }

    #[test]
    fn model_definition_postgres_quoting() {
        let mut m =
            ModelDefinition::new(Some("postgres".to_string()), "users".to_string()).unwrap();
        m.add_field(FieldDefinition::new(
            "id".to_string(),
            FieldType::AutoIncrement,
        ));
        let sql = m.to_create_table_sql().unwrap();
        assert!(sql.contains("\"users\""));
    }

    #[test]
    fn model_definition_full() {
        let mut m = ModelDefinition::new(None, "users".to_string()).unwrap();
        let mut id_field = FieldDefinition::new("id".to_string(), FieldType::AutoIncrement);
        id_field.set_primary_key();
        m.add_field(id_field);
        let mut name_field = FieldDefinition::new("name".to_string(), FieldType::String);
        name_field.not_null();
        m.add_field(name_field);
        let mut email_field = FieldDefinition::new("email".to_string(), FieldType::String);
        email_field.set_unique();
        m.add_field(email_field);
        m.add_index(IndexDefinition::new(
            "idx_name".to_string(),
            vec!["name".to_string()],
        ));
        m.add_relation(RelationDefinition::new(
            "posts".to_string(),
            RelationType::OneToMany,
            "Post".to_string(),
            "user_id".to_string(),
        ));
        let sql = m.to_create_table_sql().unwrap();
        assert!(sql.contains("PRIMARY KEY"));
        assert!(sql.contains("UNIQUE"));
        assert_eq!(m.field_count(), 3);
        assert_eq!(m.index_count(), 1);
        assert_eq!(m.relation_count(), 1);
    }
}
