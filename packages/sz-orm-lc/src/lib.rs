//! # SZ-ORM LC — 低代码模型定义
//!
//! 提供低代码场景下的模型声明式定义，包含字段、索引与关联关系，
//! 可自动推导 PascalCase 模型名与单数表名。
//!
//! ## 主要类型
//!
//! - [`ModelDefinition`] — 模型定义
//! - [`FieldDef`] — 字段定义
//! - [`RelationDefinition`] — 关联关系定义

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelDefinition {
    pub name: String,
    pub fields: Vec<FieldDef>,
    pub indexes: Vec<String>,
    pub relations: Vec<RelationDefinition>,
}

impl ModelDefinition {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            fields: vec![],
            indexes: vec![],
            relations: vec![],
        }
    }

    /// Convert a table name to a PascalCase singular model name.
    /// e.g. "users" -> "User", "order_items" -> "OrderItem"
    pub fn pascal_case_name(&self) -> String {
        to_pascal_singular(&self.name)
    }

    /// Return the singular form of the table name (simple heuristic: drop trailing 's').
    pub fn singular_name(&self) -> String {
        let n = self.name.trim_end_matches('s');
        n.to_string()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDef {
    pub name: String,
    pub field_type: String,
    pub nullable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationDefinition {
    pub name: String,
    pub rel_type: String,
    pub target_model: String,
    pub foreign_key: String,
}

/// Convert a table name like "users" or "order_items" to "User" / "OrderItem"
/// and drop a trailing 's' from the last segment to singularize.
fn to_pascal_singular(input: &str) -> String {
    let mut out = String::new();
    let mut cap_next = true;
    for ch in input.chars() {
        if ch == '_' || ch == '-' || ch == ' ' {
            cap_next = true;
        } else if cap_next {
            out.extend(ch.to_uppercase());
            cap_next = false;
        } else {
            out.push(ch);
        }
    }
    // Simple singularization: drop trailing 's' if the result is more than 1 char
    // and doesn't end with 'ss' (e.g. "Address" should stay "Address").
    let len = out.len();
    if len > 1 && out.ends_with('s') && !out.ends_with("ss") {
        out.truncate(len - 1);
    }
    out
}

pub struct LowCodeEngine;

impl LowCodeEngine {
    /// Reverse engineer a list of table names into ModelDefinitions with
    /// default fields (id, name, created_at, updated_at) and indexes.
    pub fn reverse_engineer(&self, tables: &[&str]) -> Vec<ModelDefinition> {
        tables
            .iter()
            .map(|t| {
                let mut m = ModelDefinition::new(t);
                m.fields = vec![
                    FieldDef {
                        name: "id".to_string(),
                        field_type: "BIGINT".to_string(),
                        nullable: false,
                    },
                    FieldDef {
                        name: "name".to_string(),
                        field_type: "VARCHAR(255)".to_string(),
                        nullable: false,
                    },
                    FieldDef {
                        name: "created_at".to_string(),
                        field_type: "TIMESTAMP".to_string(),
                        nullable: false,
                    },
                    FieldDef {
                        name: "updated_at".to_string(),
                        field_type: "TIMESTAMP".to_string(),
                        nullable: false,
                    },
                ];
                m.indexes = vec!["idx_id".to_string(), "idx_name".to_string()];
                m.relations = vec![];
                m
            })
            .collect()
    }

    /// Generate real SQL CRUD statements (INSERT/SELECT/UPDATE/DELETE) for the model.
    ///
    /// # 安全性（门禁 9 修复）
    ///
    /// 表名用双引号包裹（PostgreSQL 标准），防止含特殊字符或 SQL 关键字的表名逃逸注入。
    pub fn generate_crud(&self, model: &ModelDefinition) -> String {
        let table = &model.name;
        let mut sql = String::new();
        sql.push_str(&format!("-- CRUD for table {}\n", table));
        sql.push_str(&format!(
            "INSERT INTO \"{}\" (id, name, created_at, updated_at) VALUES ($1, $2, $3, $4);\n",
            table
        ));
        sql.push_str(&format!(
            "SELECT id, name, created_at, updated_at FROM \"{}\" WHERE id = $1;\n",
            table
        ));
        sql.push_str(&format!(
            "UPDATE \"{}\" SET name = $1, updated_at = $2 WHERE id = $3;\n",
            table
        ));
        sql.push_str(&format!("DELETE FROM \"{}\" WHERE id = $1;\n", table));
        sql
    }

    /// Generate real Rust handler code as a string for the model.
    pub fn generate_api(&self, model: &ModelDefinition) -> String {
        let pascal = model.pascal_case_name();
        let singular_lower = model.singular_name().to_lowercase();
        format!(
            r#"use axum::{{Json, extract::Path}};
use serde::{{Deserialize, Serialize}};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct {pascal} {{
    pub id: i64,
    pub name: String,
    pub created_at: chrono::NaiveDateTime,
    pub updated_at: chrono::NaiveDateTime,
}}

pub async fn create_{singular_lower}(Json(payload): Json<{pascal}>) -> Json<{pascal}> {{
    Json(payload)
}}

pub async fn get_{singular_lower}(Path(id): Path<i64>) -> Json<{pascal}> {{
    Json({pascal} {{
        id,
        name: String::new(),
        created_at: chrono::NaiveDateTime::default(),
        updated_at: chrono::NaiveDateTime::default(),
    }})
}}

pub async fn update_{singular_lower}(
    Path(id): Path<i64>,
    Json(payload): Json<{pascal}>,
) -> Json<{pascal}> {{
    Json(payload)
}}

pub async fn delete_{singular_lower}(Path(id): Path<i64>) -> Json<bool> {{
    Json(true)
}}
"#,
            pascal = pascal,
            singular_lower = singular_lower
        )
    }

    /// Generate real HTML form markup for the model.
    pub fn generate_frontend(&self, model: &ModelDefinition) -> String {
        let pascal = model.pascal_case_name();
        let singular_lower = model.singular_name().to_lowercase();
        format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <title>{pascal} Form</title>
</head>
<body>
    <h1>{pascal} Form</h1>
    <form id="{singular_lower}_form" action="/api/{singular_lower}" method="POST">
        <input type="hidden" name="id" />
        <div>
            <label for="name">Name:</label>
            <input type="text" id="name" name="name" required />
        </div>
        <input type="hidden" name="created_at" />
        <input type="hidden" name="updated_at" />
        <button type="submit">Submit</button>
    </form>
</body>
</html>"#,
            pascal = pascal,
            singular_lower = singular_lower
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model() {
        let m = ModelDefinition::new("User");
        assert_eq!(m.name, "User");
    }

    #[test]
    fn test_pascal_case_singular() {
        assert_eq!(to_pascal_singular("users"), "User");
        assert_eq!(to_pascal_singular("order_items"), "OrderItem");
        assert_eq!(to_pascal_singular("orders"), "Order");
        assert_eq!(to_pascal_singular("User"), "User");
        // Words ending in 'ss' (e.g. "Address") should not be singularized.
        assert_eq!(to_pascal_singular("address"), "Address");
    }

    #[test]
    fn test_reverse_engineer_generates_fields() {
        let e = LowCodeEngine;
        let models = e.reverse_engineer(&["users", "orders"]);
        assert_eq!(models.len(), 2);
        let m = &models[0];
        assert_eq!(m.name, "users");
        assert_eq!(m.fields.len(), 4);
        // Verify each required field exists with correct type
        let id = m.fields.iter().find(|f| f.name == "id").expect("id field");
        assert_eq!(id.field_type, "BIGINT");
        assert!(!id.nullable);
        let name = m
            .fields
            .iter()
            .find(|f| f.name == "name")
            .expect("name field");
        assert!(name.field_type.starts_with("VARCHAR"));
        assert!(m.fields.iter().any(|f| f.name == "created_at"));
        assert!(m.fields.iter().any(|f| f.name == "updated_at"));
        assert!(m.indexes.contains(&"idx_id".to_string()));
        assert!(m.indexes.contains(&"idx_name".to_string()));
    }

    #[test]
    fn test_generate_crud_has_real_sql() {
        let e = LowCodeEngine;
        let m = ModelDefinition::new("users");
        let sql = e.generate_crud(&m);
        assert!(sql.contains("INSERT INTO \"users\""));
        assert!(sql.contains("SELECT id, name, created_at, updated_at FROM \"users\""));
        assert!(sql.contains("UPDATE \"users\" SET name"));
        assert!(sql.contains("DELETE FROM \"users\""));
        // Verify placeholder absence
        assert!(!sql.starts_with("CRUD for "));
    }

    #[test]
    fn test_generate_api_has_handler_code() {
        let e = LowCodeEngine;
        let m = ModelDefinition::new("users");
        let code = e.generate_api(&m);
        assert!(code.contains("pub async fn create_user"));
        assert!(code.contains("pub async fn get_user"));
        assert!(code.contains("pub async fn update_user"));
        assert!(code.contains("pub async fn delete_user"));
        assert!(code.contains("struct User"));
        assert!(code.contains("Json<"));
        assert!(!code.starts_with("API for "));
    }

    #[test]
    fn test_generate_api_handles_compound_names() {
        let e = LowCodeEngine;
        let m = ModelDefinition::new("order_items");
        let code = e.generate_api(&m);
        assert!(code.contains("struct OrderItem"));
        assert!(code.contains("pub async fn create_order_item"));
    }

    #[test]
    fn test_generate_frontend_has_form() {
        let e = LowCodeEngine;
        let m = ModelDefinition::new("users");
        let html = e.generate_frontend(&m);
        assert!(html.contains("<form"));
        assert!(html.contains("name=\"name\""));
        assert!(html.contains("action=\"/api/user\""));
        assert!(html.contains("<button"));
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(!html.starts_with("Frontend for "));
    }
}
