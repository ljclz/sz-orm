use serde::{Deserialize, Serialize};
pub use sz_orm_core::dialect_security::Dialect;
use sz_orm_core::schema_sync::{ColumnDef, DdlGenerator, SchemaDiff, TableDef};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Cardinality {
    OneToOne,
    OneToMany,
    ManyToOne,
    ManyToMany,
}

impl Cardinality {
    pub fn as_str(&self) -> &'static str {
        match self {
            Cardinality::OneToOne => "1:1",
            Cardinality::OneToMany => "1:N",
            Cardinality::ManyToOne => "N:1",
            Cardinality::ManyToMany => "N:N",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ColumnType {
    Int,
    BigInt,
    SmallInt,
    TinyInt,
    Varchar(Option<u32>),
    Text,
    Boolean,
    Decimal(u32, u32),
    Float,
    Double,
    Date,
    DateTime,
    Timestamp,
    Time,
    Json,
    Binary,
    Uuid,
    Custom(String),
}

impl ColumnType {
    pub fn to_sql_type(&self) -> String {
        match self {
            ColumnType::Int => "INT".to_string(),
            ColumnType::BigInt => "BIGINT".to_string(),
            ColumnType::SmallInt => "SMALLINT".to_string(),
            ColumnType::TinyInt => "TINYINT".to_string(),
            ColumnType::Varchar(Some(n)) => format!("VARCHAR({})", n),
            ColumnType::Varchar(None) => "VARCHAR(255)".to_string(),
            ColumnType::Text => "TEXT".to_string(),
            ColumnType::Boolean => "BOOLEAN".to_string(),
            ColumnType::Decimal(p, s) => format!("DECIMAL({},{})", p, s),
            ColumnType::Float => "FLOAT".to_string(),
            ColumnType::Double => "DOUBLE".to_string(),
            ColumnType::Date => "DATE".to_string(),
            ColumnType::DateTime => "DATETIME".to_string(),
            ColumnType::Timestamp => "TIMESTAMP".to_string(),
            ColumnType::Time => "TIME".to_string(),
            ColumnType::Json => "JSON".to_string(),
            ColumnType::Binary => "BINARY".to_string(),
            ColumnType::Uuid => "UUID".to_string(),
            ColumnType::Custom(s) => s.clone(),
        }
    }

    pub fn from_sql_type(sql_type: &str) -> Self {
        let upper = sql_type.to_uppercase();
        if upper.starts_with("VARCHAR")
            || upper.starts_with("CHAR")
            || upper.starts_with("NVARCHAR")
        {
            if let Some(start) = upper.find('(') {
                if let Some(end) = upper.find(')') {
                    if let Ok(n) = upper[start + 1..end].parse::<u32>() {
                        return ColumnType::Varchar(Some(n));
                    }
                }
            }
            ColumnType::Varchar(None)
        } else if upper.starts_with("TEXT") || upper.starts_with("CLOB") {
            ColumnType::Text
        } else if upper.starts_with("BIGINT")
            || upper.starts_with("SERIAL")
            || upper.starts_with("BIGSERIAL")
        {
            ColumnType::BigInt
        } else if upper.starts_with("INT") || upper.starts_with("INTEGER") {
            ColumnType::Int
        } else if upper.starts_with("SMALLINT") {
            ColumnType::SmallInt
        } else if upper.starts_with("TINYINT") {
            ColumnType::TinyInt
        } else if upper.starts_with("BOOLEAN") || upper.starts_with("BOOL") {
            ColumnType::Boolean
        } else if upper.starts_with("DECIMAL") || upper.starts_with("NUMERIC") {
            if let Some(start) = upper.find('(') {
                if let Some(end) = upper.find(')') {
                    let parts: Vec<&str> = upper[start + 1..end].split(',').collect();
                    if parts.len() == 2 {
                        if let (Ok(p), Ok(s)) = (
                            parts[0].trim().parse::<u32>(),
                            parts[1].trim().parse::<u32>(),
                        ) {
                            return ColumnType::Decimal(p, s);
                        }
                    }
                }
            }
            ColumnType::Decimal(10, 2)
        } else if upper.starts_with("FLOAT") || upper.starts_with("REAL") {
            ColumnType::Float
        } else if upper.starts_with("DOUBLE") {
            ColumnType::Double
        } else if upper.starts_with("DATETIME") {
            ColumnType::DateTime
        } else if upper.starts_with("TIMESTAMP") {
            ColumnType::Timestamp
        } else if upper.starts_with("DATE") {
            ColumnType::Date
        } else if upper.starts_with("TIME") {
            ColumnType::Time
        } else if upper.starts_with("JSON") {
            ColumnType::Json
        } else if upper.starts_with("BINARY")
            || upper.starts_with("BLOB")
            || upper.starts_with("BYTEA")
        {
            ColumnType::Binary
        } else if upper.starts_with("UUID") {
            ColumnType::Uuid
        } else {
            ColumnType::Custom(sql_type.to_string())
        }
    }

    pub fn to_rust_type(&self) -> &'static str {
        match self {
            ColumnType::Int => "i32",
            ColumnType::BigInt => "i64",
            ColumnType::SmallInt => "i16",
            ColumnType::TinyInt => "i8",
            ColumnType::Varchar(_) | ColumnType::Text => "String",
            ColumnType::Boolean => "bool",
            ColumnType::Decimal(_, _) => "f64",
            ColumnType::Float => "f32",
            ColumnType::Double => "f64",
            ColumnType::Date => "chrono::NaiveDate",
            ColumnType::DateTime | ColumnType::Timestamp => "chrono::DateTime<chrono::Utc>",
            ColumnType::Time => "chrono::NaiveTime",
            ColumnType::Json => "serde_json::Value",
            ColumnType::Binary => "Vec<u8>",
            ColumnType::Uuid => "uuid::Uuid",
            ColumnType::Custom(_) => "String",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DesignColumn {
    pub name: String,
    pub col_type: ColumnType,
    pub nullable: bool,
    pub default: Option<String>,
    pub comment: Option<String>,
    pub is_primary_key: bool,
    pub is_auto_increment: bool,
}

impl DesignColumn {
    pub fn new(name: &str, col_type: ColumnType) -> Self {
        Self {
            name: name.to_string(),
            col_type,
            nullable: false,
            default: None,
            comment: None,
            is_primary_key: false,
            is_auto_increment: false,
        }
    }

    pub fn primary_key(mut self) -> Self {
        self.is_primary_key = true;
        self.nullable = false;
        self
    }

    pub fn nullable(mut self) -> Self {
        self.nullable = true;
        self
    }

    pub fn auto_increment(mut self) -> Self {
        self.is_auto_increment = true;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DesignIndex {
    pub name: String,
    pub columns: Vec<String>,
    pub unique: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DesignTable {
    pub name: String,
    pub columns: Vec<DesignColumn>,
    pub indexes: Vec<DesignIndex>,
    pub comment: Option<String>,
}

impl DesignTable {
    pub fn new(name: &str, columns: Vec<DesignColumn>) -> Self {
        Self {
            name: name.to_string(),
            columns,
            indexes: vec![],
            comment: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DesignRelation {
    pub from_table: String,
    pub to_table: String,
    pub from_column: String,
    pub to_column: String,
    pub cardinality: Cardinality,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SchemaDesign {
    pub tables: Vec<DesignTable>,
    pub relations: Vec<DesignRelation>,
    pub dialect: Dialect,
}

impl SchemaDesign {
    pub fn new(dialect: Dialect) -> Self {
        Self {
            tables: vec![],
            relations: vec![],
            dialect,
        }
    }

    pub fn with_table(mut self, table: DesignTable) -> Self {
        self.tables.push(table);
        self
    }

    pub fn with_relation(mut self, relation: DesignRelation) -> Self {
        self.relations.push(relation);
        self
    }

    pub fn to_table_defs(&self) -> Vec<TableDef> {
        self.tables
            .iter()
            .map(|t| {
                let columns: Vec<ColumnDef> = t
                    .columns
                    .iter()
                    .map(|c| {
                        ColumnDef::new(
                            c.name.clone(),
                            c.col_type.to_sql_type(),
                            c.nullable,
                            c.is_primary_key,
                            c.default.clone(),
                        )
                    })
                    .collect();
                TableDef::new(t.name.clone(), columns)
            })
            .collect()
    }

    pub fn from_table_defs(table_defs: &[TableDef], dialect: Dialect) -> Self {
        let tables: Vec<DesignTable> = table_defs
            .iter()
            .map(|t| {
                let columns: Vec<DesignColumn> = t
                    .columns
                    .iter()
                    .map(|c| DesignColumn {
                        name: c.name.clone(),
                        col_type: ColumnType::from_sql_type(&c.sql_type),
                        nullable: c.nullable,
                        default: c.default.clone(),
                        comment: None,
                        is_primary_key: c.primary_key,
                        is_auto_increment: false,
                    })
                    .collect();
                DesignTable {
                    name: t.name.clone(),
                    columns,
                    indexes: vec![],
                    comment: None,
                }
            })
            .collect();
        SchemaDesign {
            tables,
            relations: vec![],
            dialect,
        }
    }

    pub fn to_schema_diff(&self) -> SchemaDiff {
        SchemaDiff {
            added_tables: self.to_table_defs(),
            ..Default::default()
        }
    }
}

pub fn select_ddl_generator(dialect: Dialect) -> Box<dyn DdlGenerator> {
    use sz_orm_core::schema_sync::*;
    match dialect {
        Dialect::MySql => Box::new(MySqlDdlGenerator),
        Dialect::PostgreSql => Box::new(PgDdlGenerator),
        Dialect::Sqlite => Box::new(SqliteDdlGenerator),
        Dialect::Oracle => Box::new(OracleDdlGenerator),
        Dialect::Mssql => Box::new(MssqlDdlGenerator),
    }
}

pub fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => {
                    let mut result = String::new();
                    result.extend(first.to_uppercase());
                    result.push_str(chars.as_str());
                    result
                }
                None => String::new(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_column_type_roundtrip() {
        let types = vec![
            ColumnType::BigInt,
            ColumnType::Int,
            ColumnType::Varchar(Some(255)),
            ColumnType::Text,
            ColumnType::Boolean,
            ColumnType::Decimal(10, 2),
            ColumnType::Float,
            ColumnType::Double,
            ColumnType::Date,
            ColumnType::DateTime,
            ColumnType::Timestamp,
            ColumnType::Json,
            ColumnType::Binary,
            ColumnType::Uuid,
        ];
        for ct in types {
            let sql = ct.to_sql_type();
            let recovered = ColumnType::from_sql_type(&sql);
            assert_eq!(ct, recovered, "roundtrip failed for sql_type: {}", sql);
        }
    }

    #[test]
    fn test_ir_roundtrip() {
        let design = SchemaDesign::new(Dialect::MySql).with_table(DesignTable::new(
            "users",
            vec![
                DesignColumn::new("id", ColumnType::BigInt)
                    .primary_key()
                    .auto_increment(),
                DesignColumn::new("name", ColumnType::Varchar(Some(255))),
                DesignColumn::new("email", ColumnType::Varchar(Some(255))).nullable(),
            ],
        ));

        let table_defs = design.to_table_defs();
        let design2 = SchemaDesign::from_table_defs(&table_defs, Dialect::MySql);

        assert_eq!(design.tables.len(), design2.tables.len());
        assert_eq!(design.tables[0].name, design2.tables[0].name);
        assert_eq!(
            design.tables[0].columns.len(),
            design2.tables[0].columns.len()
        );
        assert_eq!(
            design.tables[0].columns[0].name,
            design2.tables[0].columns[0].name
        );
        assert_eq!(
            design.tables[0].columns[0].is_primary_key,
            design2.tables[0].columns[0].is_primary_key
        );
        assert_eq!(
            design.tables[0].columns[2].nullable,
            design2.tables[0].columns[2].nullable
        );
    }

    #[test]
    fn test_relation_preserved_in_ir() {
        let design = SchemaDesign::new(Dialect::PostgreSql)
            .with_table(DesignTable::new(
                "users",
                vec![DesignColumn::new("id", ColumnType::BigInt).primary_key()],
            ))
            .with_table(DesignTable::new(
                "orders",
                vec![
                    DesignColumn::new("id", ColumnType::BigInt).primary_key(),
                    DesignColumn::new("user_id", ColumnType::BigInt),
                ],
            ))
            .with_relation(DesignRelation {
                from_table: "users".to_string(),
                to_table: "orders".to_string(),
                from_column: "id".to_string(),
                to_column: "user_id".to_string(),
                cardinality: Cardinality::OneToMany,
            });

        assert_eq!(design.tables.len(), 2);
        assert_eq!(design.relations.len(), 1);
        assert_eq!(design.relations[0].cardinality, Cardinality::OneToMany);
    }

    #[test]
    fn test_pascal_case() {
        assert_eq!(to_pascal_case("users"), "Users");
        assert_eq!(to_pascal_case("order_items"), "OrderItems");
        assert_eq!(to_pascal_case("User"), "User");
    }

    #[test]
    fn test_to_rust_type_all_variants() {
        assert_eq!(ColumnType::Int.to_rust_type(), "i32");
        assert_eq!(ColumnType::BigInt.to_rust_type(), "i64");
        assert_eq!(ColumnType::SmallInt.to_rust_type(), "i16");
        assert_eq!(ColumnType::TinyInt.to_rust_type(), "i8");
        assert_eq!(ColumnType::Varchar(None).to_rust_type(), "String");
        assert_eq!(ColumnType::Text.to_rust_type(), "String");
        assert_eq!(ColumnType::Boolean.to_rust_type(), "bool");
        assert_eq!(ColumnType::Decimal(10, 2).to_rust_type(), "f64");
        assert_eq!(ColumnType::Float.to_rust_type(), "f32");
        assert_eq!(ColumnType::Double.to_rust_type(), "f64");
        assert_eq!(ColumnType::Date.to_rust_type(), "chrono::NaiveDate");
        assert_eq!(
            ColumnType::DateTime.to_rust_type(),
            "chrono::DateTime<chrono::Utc>"
        );
        assert_eq!(
            ColumnType::Timestamp.to_rust_type(),
            "chrono::DateTime<chrono::Utc>"
        );
        assert_eq!(ColumnType::Time.to_rust_type(), "chrono::NaiveTime");
        assert_eq!(ColumnType::Json.to_rust_type(), "serde_json::Value");
        assert_eq!(ColumnType::Binary.to_rust_type(), "Vec<u8>");
        assert_eq!(ColumnType::Uuid.to_rust_type(), "uuid::Uuid");
        assert_eq!(ColumnType::Custom("X".into()).to_rust_type(), "String");
    }

    #[test]
    fn test_from_sql_type_aliases() {
        assert_eq!(
            ColumnType::from_sql_type("NVARCHAR(100)"),
            ColumnType::Varchar(Some(100))
        );
        assert_eq!(ColumnType::from_sql_type("CLOB"), ColumnType::Text);
        assert_eq!(ColumnType::from_sql_type("SERIAL"), ColumnType::BigInt);
        assert_eq!(ColumnType::from_sql_type("BIGSERIAL"), ColumnType::BigInt);
        assert_eq!(ColumnType::from_sql_type("INTEGER"), ColumnType::Int);
        assert_eq!(ColumnType::from_sql_type("BOOL"), ColumnType::Boolean);
        assert_eq!(
            ColumnType::from_sql_type("NUMERIC(8,2)"),
            ColumnType::Decimal(8, 2)
        );
        assert_eq!(ColumnType::from_sql_type("REAL"), ColumnType::Float);
        assert_eq!(ColumnType::from_sql_type("BLOB"), ColumnType::Binary);
        assert_eq!(ColumnType::from_sql_type("BYTEA"), ColumnType::Binary);
        assert_eq!(
            ColumnType::from_sql_type("UNKNOWN"),
            ColumnType::Custom("UNKNOWN".into())
        );
    }

    #[test]
    fn test_cardinality_as_str() {
        assert_eq!(Cardinality::OneToOne.as_str(), "1:1");
        assert_eq!(Cardinality::OneToMany.as_str(), "1:N");
        assert_eq!(Cardinality::ManyToOne.as_str(), "N:1");
        assert_eq!(Cardinality::ManyToMany.as_str(), "N:N");
    }

    #[test]
    fn test_pascal_case_edge_cases() {
        assert_eq!(to_pascal_case(""), "");
        assert_eq!(to_pascal_case("a"), "A");
        assert_eq!(to_pascal_case("user_id_2"), "UserId2");
        assert_eq!(to_pascal_case("__private"), "Private");
    }
}
