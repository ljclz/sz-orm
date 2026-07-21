//! # SZ-ORM WASM — WASM 查询接口
//!
//! 提供面向浏览器端的轻量查询能力，内置内存数据库与 SQL 子集解析，
//! 适合在不依赖后端的环境下做本地查询与演示。
//!
//! ## 主要类型
//!
//! - [`WasmQuery`] — 查询请求（SQL + 参数）
//! - 内存数据库 — 支持 SQL 子集的本地执行

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;

/// WASM 查询请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmQuery {
    pub sql: String,
    pub params: Vec<serde_json::Value>,
}

impl WasmQuery {
    pub fn new(sql: &str) -> Self {
        Self {
            sql: sql.to_string(),
            params: vec![],
        }
    }

    pub fn with_params(sql: &str, params: Vec<serde_json::Value>) -> Self {
        Self {
            sql: sql.to_string(),
            params,
        }
    }
}

/// 内存数据库，支持简单的 SQL 子集
///
/// 支持的 SQL：
/// - `SELECT * FROM <table>` / `SELECT * FROM <table> WHERE <col> = ?`
/// - `INSERT INTO <table> (<cols>) VALUES (?, ?, ...)` (支持多行)
/// - `UPDATE <table> SET <col> = ? WHERE <col> = ?`
/// - `DELETE FROM <table> WHERE <col> = ?`
/// - `CREATE TABLE <name> (...)`
pub struct WasmDatabase {
    tables: Mutex<HashMap<String, Vec<serde_json::Value>>>,
}

impl WasmDatabase {
    pub fn new() -> Self {
        Self {
            tables: Mutex::new(HashMap::new()),
        }
    }

    pub fn query(&self, q: WasmQuery) -> Result<Vec<serde_json::Value>, String> {
        let sql = q.sql.trim().trim_end_matches(';').trim();
        let upper = sql.to_uppercase();

        if !upper.starts_with("SELECT") {
            return Err(format!("query only supports SELECT, got: {}", sql));
        }

        let table = Self::parse_table_from_select(&upper, sql)?;
        let tables = self
            .tables
            .lock()
            .map_err(|e| format!("lock error: {}", e))?;
        let rows = tables.get(&table).cloned().unwrap_or_default();

        let where_idx = upper.find("WHERE");
        if let Some(wi) = where_idx {
            let where_clause = sql[wi + 5..].trim();
            let filtered = Self::apply_where_filter(&rows, where_clause, &q.params)?;
            Ok(filtered)
        } else {
            Ok(rows)
        }
    }

    pub fn execute(&self, q: WasmQuery) -> Result<usize, String> {
        let sql = q.sql.trim().trim_end_matches(';').trim();
        let upper = sql.to_uppercase();

        if upper.starts_with("INSERT") {
            self.execute_insert(sql, &q.params)
        } else if upper.starts_with("UPDATE") {
            self.execute_update(sql, &q.params)
        } else if upper.starts_with("DELETE") {
            self.execute_delete(sql, &q.params)
        } else if upper.starts_with("CREATE TABLE") {
            self.execute_create_table(sql)
        } else {
            Err(format!("unsupported execute: {}", sql))
        }
    }

    fn parse_table_from_select(upper: &str, sql: &str) -> Result<String, String> {
        let from_idx = upper
            .find("FROM")
            .ok_or_else(|| "missing FROM in SELECT".to_string())?;
        let after_from = sql[from_idx + 4..].trim();
        let table = after_from
            .split_whitespace()
            .next()
            .ok_or_else(|| "missing table name after FROM".to_string())?;
        Ok(table.to_string())
    }

    fn apply_where_filter(
        rows: &[serde_json::Value],
        where_clause: &str,
        params: &[serde_json::Value],
    ) -> Result<Vec<serde_json::Value>, String> {
        let parts: Vec<&str> = where_clause.splitn(2, '=').collect();
        if parts.len() != 2 {
            return Err(format!("unsupported WHERE clause: {}", where_clause));
        }
        let col = parts[0].trim();
        let value_part = parts[1].trim();

        let target_value: serde_json::Value = if value_part == "?" {
            params
                .first()
                .cloned()
                .ok_or_else(|| "missing param for WHERE ?".to_string())?
        } else {
            serde_json::from_str(value_part)
                .unwrap_or_else(|_| serde_json::Value::String(value_part.to_string()))
        };

        let filtered: Vec<serde_json::Value> = rows
            .iter()
            .filter(|row| {
                row.as_object()
                    .and_then(|obj| obj.get(col))
                    .map(|v| v == &target_value)
                    .unwrap_or(false)
            })
            .cloned()
            .collect();

        Ok(filtered)
    }

    fn execute_insert(&self, sql: &str, params: &[serde_json::Value]) -> Result<usize, String> {
        let upper = sql.to_uppercase();
        let into_idx = upper
            .find("INTO")
            .ok_or_else(|| "missing INTO in INSERT".to_string())?;
        let after_into = sql[into_idx + 4..].trim();

        let paren_pos = after_into
            .find('(')
            .ok_or_else(|| "missing columns ( in INSERT".to_string())?;
        let table = after_into[..paren_pos].trim();

        let close_paren = after_into
            .find(')')
            .ok_or_else(|| "missing ) in INSERT columns".to_string())?;
        let cols_str = &after_into[paren_pos + 1..close_paren];
        let columns: Vec<String> = cols_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if columns.is_empty() {
            return Err("empty column list in INSERT".to_string());
        }

        let rest = &after_into[close_paren + 1..];
        let rest_upper = rest.to_uppercase();
        let values_idx = rest_upper
            .find("VALUES")
            .ok_or_else(|| "missing VALUES in INSERT".to_string())?;
        let values_part = rest[values_idx + 6..].trim();

        // 用括号深度匹配解析多个值组：(?, ?), (?, ?), ...
        let group_strs: Vec<String> = Self::parse_value_groups(values_part);
        if group_strs.is_empty() {
            return Err("no value groups in INSERT".to_string());
        }

        let mut rows_to_insert: Vec<serde_json::Value> = Vec::with_capacity(group_strs.len());
        let mut param_idx = 0usize;

        for group_str in &group_strs {
            let mut row = serde_json::Map::new();
            let val_strs: Vec<&str> = group_str.split(',').collect();
            if val_strs.len() != columns.len() {
                return Err(format!(
                    "value count {} != column count {}",
                    val_strs.len(),
                    columns.len()
                ));
            }
            for (col, val_str) in columns.iter().zip(val_strs.iter()) {
                let v = val_str.trim();
                let value: serde_json::Value = if v == "?" {
                    params
                        .get(param_idx)
                        .cloned()
                        .ok_or_else(|| "missing param for INSERT ?".to_string())?
                } else {
                    serde_json::from_str(v)
                        .unwrap_or_else(|_| serde_json::Value::String(v.to_string()))
                };
                if v == "?" {
                    param_idx += 1;
                }
                row.insert(col.clone(), value);
            }
            rows_to_insert.push(serde_json::Value::Object(row));
        }

        let mut tables = self
            .tables
            .lock()
            .map_err(|e| format!("lock error: {}", e))?;
        let table_rows = tables.entry(table.to_string()).or_insert_with(Vec::new);
        let inserted = rows_to_insert.len();
        table_rows.extend(rows_to_insert);
        Ok(inserted)
    }

    fn execute_update(&self, sql: &str, params: &[serde_json::Value]) -> Result<usize, String> {
        let upper = sql.to_uppercase();
        let set_idx = upper
            .find("SET")
            .ok_or_else(|| "missing SET in UPDATE".to_string())?;
        let after_update = sql[..set_idx].trim();
        let table = after_update
            .strip_prefix("UPDATE")
            .or_else(|| after_update.strip_prefix("update"))
            .ok_or_else(|| "missing UPDATE keyword".to_string())?
            .trim();

        let where_idx = upper.find("WHERE");
        let set_clause = if let Some(wi) = where_idx {
            sql[set_idx + 3..wi].trim()
        } else {
            sql[set_idx + 3..].trim()
        };

        // 只支持单列 SET col = ?
        let set_parts: Vec<&str> = set_clause.splitn(2, '=').collect();
        if set_parts.len() != 2 {
            return Err(format!("unsupported SET clause: {}", set_clause));
        }
        let set_col = set_parts[0].trim().to_string();
        let set_val_str = set_parts[1].trim();

        let mut param_idx = 0usize;
        let set_value: serde_json::Value = if set_val_str == "?" {
            params
                .get(param_idx)
                .cloned()
                .ok_or_else(|| "missing param for SET ?".to_string())?
        } else {
            serde_json::from_str(set_val_str)
                .unwrap_or_else(|_| serde_json::Value::String(set_val_str.to_string()))
        };
        if set_val_str == "?" {
            param_idx += 1;
        }

        let (where_col, where_val): (Option<String>, Option<serde_json::Value>) =
            if let Some(wi) = where_idx {
                let where_clause = sql[wi + 5..].trim();
                let where_parts: Vec<&str> = where_clause.splitn(2, '=').collect();
                if where_parts.len() != 2 {
                    return Err(format!("unsupported WHERE clause: {}", where_clause));
                }
                let wcol = where_parts[0].trim().to_string();
                let wval_str = where_parts[1].trim();
                let wval: serde_json::Value = if wval_str == "?" {
                    params
                        .get(param_idx)
                        .cloned()
                        .ok_or_else(|| "missing param for WHERE ?".to_string())?
                } else {
                    serde_json::from_str(wval_str)
                        .unwrap_or_else(|_| serde_json::Value::String(wval_str.to_string()))
                };
                (Some(wcol), Some(wval))
            } else {
                (None, None)
            };

        let mut tables = self
            .tables
            .lock()
            .map_err(|e| format!("lock error: {}", e))?;
        let table_rows = tables.entry(table.to_string()).or_insert_with(Vec::new);
        let mut updated = 0usize;
        for row in table_rows.iter_mut() {
            if let Some(obj) = row.as_object_mut() {
                let matches = match (&where_col, &where_val) {
                    (Some(wc), Some(wv)) => obj.get(wc).map(|v| v == wv).unwrap_or(false),
                    _ => true,
                };
                if matches {
                    obj.insert(set_col.clone(), set_value.clone());
                    updated += 1;
                }
            }
        }
        Ok(updated)
    }

    fn execute_delete(&self, sql: &str, params: &[serde_json::Value]) -> Result<usize, String> {
        let upper = sql.to_uppercase();
        let from_idx = upper
            .find("FROM")
            .ok_or_else(|| "missing FROM in DELETE".to_string())?;
        let where_idx = upper.find("WHERE");
        let table = if let Some(wi) = where_idx {
            sql[from_idx + 4..wi].trim()
        } else {
            sql[from_idx + 4..].trim()
        };

        let (where_col, where_val): (Option<String>, Option<serde_json::Value>) =
            if let Some(wi) = where_idx {
                let where_clause = sql[wi + 5..].trim();
                let where_parts: Vec<&str> = where_clause.splitn(2, '=').collect();
                if where_parts.len() != 2 {
                    return Err(format!("unsupported WHERE clause: {}", where_clause));
                }
                let wcol = where_parts[0].trim().to_string();
                let wval_str = where_parts[1].trim();
                let wval: serde_json::Value = if wval_str == "?" {
                    params
                        .first()
                        .cloned()
                        .ok_or_else(|| "missing param for DELETE WHERE ?".to_string())?
                } else {
                    serde_json::from_str(wval_str)
                        .unwrap_or_else(|_| serde_json::Value::String(wval_str.to_string()))
                };
                (Some(wcol), Some(wval))
            } else {
                (None, None)
            };

        let mut tables = self
            .tables
            .lock()
            .map_err(|e| format!("lock error: {}", e))?;
        let table_rows = tables.entry(table.to_string()).or_insert_with(Vec::new);

        if let (Some(wc), Some(wv)) = (where_col, where_val) {
            let before = table_rows.len();
            table_rows.retain(|row| {
                row.as_object()
                    .and_then(|obj| obj.get(&wc))
                    .map(|v| v != &wv)
                    .unwrap_or(true)
            });
            Ok(before - table_rows.len())
        } else {
            let count = table_rows.len();
            table_rows.clear();
            Ok(count)
        }
    }

    fn execute_create_table(&self, sql: &str) -> Result<usize, String> {
        let upper = sql.to_uppercase();
        let table_idx = upper
            .find("TABLE")
            .ok_or_else(|| "missing TABLE in CREATE TABLE".to_string())?;
        let after_table = sql[table_idx + 5..].trim();
        let paren_idx = after_table
            .find('(')
            .ok_or_else(|| "missing ( in CREATE TABLE".to_string())?;
        let table = after_table[..paren_idx].trim();
        let mut tables = self
            .tables
            .lock()
            .map_err(|e| format!("lock error: {}", e))?;
        tables.entry(table.to_string()).or_insert_with(Vec::new);
        Ok(0)
    }

    /// 用括号深度匹配解析值组："(?, ?), (?, ?)" -> ["?, ?", "?, ?"]
    fn parse_value_groups(values_part: &str) -> Vec<String> {
        let mut groups = Vec::new();
        let mut current = String::new();
        let mut depth = 0i32;
        for ch in values_part.chars() {
            match ch {
                '(' => {
                    depth += 1;
                    if depth == 1 {
                        current.clear();
                    } else {
                        current.push(ch);
                    }
                }
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        groups.push(current.clone());
                        current.clear();
                    } else {
                        current.push(ch);
                    }
                }
                _ if depth > 0 => {
                    current.push(ch);
                }
                _ => {}
            }
        }
        groups
    }
}

impl Default for WasmDatabase {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_new_database_is_empty() {
        let db = WasmDatabase::new();
        let rows = db.query(WasmQuery::new("SELECT * FROM users")).unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn test_create_table() {
        let db = WasmDatabase::new();
        let affected = db
            .execute(WasmQuery::new("CREATE TABLE users (id INTEGER, name TEXT)"))
            .unwrap();
        assert_eq!(affected, 0);
        // 创建后再查询应该返回空数组而不是错误
        let rows = db.query(WasmQuery::new("SELECT * FROM users")).unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn test_insert_and_select_single_row() {
        let db = WasmDatabase::new();
        db.execute(WasmQuery::new("CREATE TABLE users (id INTEGER, name TEXT)"))
            .unwrap();
        let inserted = db
            .execute(WasmQuery::with_params(
                "INSERT INTO users (id, name) VALUES (?, ?)",
                vec![json!(1), json!("Alice")],
            ))
            .unwrap();
        assert_eq!(inserted, 1);

        let rows = db.query(WasmQuery::new("SELECT * FROM users")).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["id"], json!(1));
        assert_eq!(rows[0]["name"], json!("Alice"));
    }

    #[test]
    fn test_insert_multiple_rows() {
        let db = WasmDatabase::new();
        db.execute(WasmQuery::new("CREATE TABLE users (id INTEGER, name TEXT)"))
            .unwrap();
        let inserted = db
            .execute(WasmQuery::with_params(
                "INSERT INTO users (id, name) VALUES (?, ?), (?, ?), (?, ?)",
                vec![
                    json!(1),
                    json!("A"),
                    json!(2),
                    json!("B"),
                    json!(3),
                    json!("C"),
                ],
            ))
            .unwrap();
        assert_eq!(inserted, 3);

        let rows = db.query(WasmQuery::new("SELECT * FROM users")).unwrap();
        assert_eq!(rows.len(), 3);
    }

    #[test]
    fn test_select_with_where_filter() {
        let db = WasmDatabase::new();
        db.execute(WasmQuery::new(
            "CREATE TABLE users (id INTEGER, name TEXT, age INTEGER)",
        ))
        .unwrap();
        db.execute(WasmQuery::with_params(
            "INSERT INTO users (id, name, age) VALUES (?, ?, ?), (?, ?, ?), (?, ?, ?)",
            vec![
                json!(1),
                json!("Alice"),
                json!(30),
                json!(2),
                json!("Bob"),
                json!(25),
                json!(3),
                json!("Charlie"),
                json!(30),
            ],
        ))
        .unwrap();

        // WHERE age = ?
        let rows = db
            .query(WasmQuery::with_params(
                "SELECT * FROM users WHERE age = ?",
                vec![json!(30)],
            ))
            .unwrap();
        assert_eq!(rows.len(), 2);
        for row in &rows {
            assert_eq!(row["age"], json!(30));
        }
    }

    #[test]
    fn test_select_with_where_no_match() {
        let db = WasmDatabase::new();
        db.execute(WasmQuery::new("CREATE TABLE users (id INTEGER, name TEXT)"))
            .unwrap();
        db.execute(WasmQuery::with_params(
            "INSERT INTO users (id, name) VALUES (?, ?)",
            vec![json!(1), json!("Alice")],
        ))
        .unwrap();

        let rows = db
            .query(WasmQuery::with_params(
                "SELECT * FROM users WHERE id = ?",
                vec![json!(999)],
            ))
            .unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn test_update_with_where() {
        let db = WasmDatabase::new();
        db.execute(WasmQuery::new("CREATE TABLE users (id INTEGER, name TEXT)"))
            .unwrap();
        db.execute(WasmQuery::with_params(
            "INSERT INTO users (id, name) VALUES (?, ?), (?, ?)",
            vec![json!(1), json!("Alice"), json!(2), json!("Bob")],
        ))
        .unwrap();

        let updated = db
            .execute(WasmQuery::with_params(
                "UPDATE users SET name = ? WHERE id = ?",
                vec![json!("Alice2"), json!(1)],
            ))
            .unwrap();
        assert_eq!(updated, 1);

        let rows = db
            .query(WasmQuery::with_params(
                "SELECT * FROM users WHERE id = ?",
                vec![json!(1)],
            ))
            .unwrap();
        assert_eq!(rows[0]["name"], json!("Alice2"));

        // Bob 未被修改
        let rows = db
            .query(WasmQuery::with_params(
                "SELECT * FROM users WHERE id = ?",
                vec![json!(2)],
            ))
            .unwrap();
        assert_eq!(rows[0]["name"], json!("Bob"));
    }

    #[test]
    fn test_update_all_rows() {
        let db = WasmDatabase::new();
        db.execute(WasmQuery::new(
            "CREATE TABLE users (id INTEGER, status TEXT)",
        ))
        .unwrap();
        db.execute(WasmQuery::with_params(
            "INSERT INTO users (id, status) VALUES (?, ?), (?, ?)",
            vec![json!(1), json!("active"), json!(2), json!("active")],
        ))
        .unwrap();

        let updated = db
            .execute(WasmQuery::with_params(
                "UPDATE users SET status = ?",
                vec![json!("inactive")],
            ))
            .unwrap();
        assert_eq!(updated, 2);

        let rows = db.query(WasmQuery::new("SELECT * FROM users")).unwrap();
        for row in &rows {
            assert_eq!(row["status"], json!("inactive"));
        }
    }

    #[test]
    fn test_delete_with_where() {
        let db = WasmDatabase::new();
        db.execute(WasmQuery::new("CREATE TABLE users (id INTEGER, name TEXT)"))
            .unwrap();
        db.execute(WasmQuery::with_params(
            "INSERT INTO users (id, name) VALUES (?, ?), (?, ?), (?, ?)",
            vec![
                json!(1),
                json!("A"),
                json!(2),
                json!("B"),
                json!(3),
                json!("C"),
            ],
        ))
        .unwrap();

        let deleted = db
            .execute(WasmQuery::with_params(
                "DELETE FROM users WHERE id = ?",
                vec![json!(2)],
            ))
            .unwrap();
        assert_eq!(deleted, 1);

        let rows = db.query(WasmQuery::new("SELECT * FROM users")).unwrap();
        assert_eq!(rows.len(), 2);
        // B 已删除
        for row in &rows {
            assert_ne!(row["id"], json!(2));
        }
    }

    #[test]
    fn test_delete_all_rows() {
        let db = WasmDatabase::new();
        db.execute(WasmQuery::new("CREATE TABLE users (id INTEGER)"))
            .unwrap();
        db.execute(WasmQuery::with_params(
            "INSERT INTO users (id) VALUES (?), (?), (?)",
            vec![json!(1), json!(2), json!(3)],
        ))
        .unwrap();

        let deleted = db.execute(WasmQuery::new("DELETE FROM users")).unwrap();
        assert_eq!(deleted, 3);

        let rows = db.query(WasmQuery::new("SELECT * FROM users")).unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn test_full_crud_cycle() {
        let db = WasmDatabase::new();
        // CREATE
        db.execute(WasmQuery::new(
            "CREATE TABLE products (id INTEGER, name TEXT, price INTEGER)",
        ))
        .unwrap();
        // INSERT
        db.execute(WasmQuery::with_params(
            "INSERT INTO products (id, name, price) VALUES (?, ?, ?), (?, ?, ?)",
            vec![
                json!(1),
                json!("Apple"),
                json!(10),
                json!(2),
                json!("Banana"),
                json!(5),
            ],
        ))
        .unwrap();
        // SELECT
        let rows = db.query(WasmQuery::new("SELECT * FROM products")).unwrap();
        assert_eq!(rows.len(), 2);
        // UPDATE
        db.execute(WasmQuery::with_params(
            "UPDATE products SET price = ? WHERE id = ?",
            vec![json!(7), json!(2)],
        ))
        .unwrap();
        // SELECT 验证更新
        let rows = db
            .query(WasmQuery::with_params(
                "SELECT * FROM products WHERE id = ?",
                vec![json!(2)],
            ))
            .unwrap();
        assert_eq!(rows[0]["price"], json!(7));
        // DELETE
        db.execute(WasmQuery::with_params(
            "DELETE FROM products WHERE id = ?",
            vec![json!(1)],
        ))
        .unwrap();
        // SELECT 验证删除
        let rows = db.query(WasmQuery::new("SELECT * FROM products")).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["id"], json!(2));
    }

    #[test]
    fn test_query_non_select_returns_error() {
        let db = WasmDatabase::new();
        let result = db.query(WasmQuery::new("INSERT INTO foo VALUES (1)"));
        assert!(result.is_err());
    }

    #[test]
    fn test_execute_unsupported_returns_error() {
        let db = WasmDatabase::new();
        let result = db.execute(WasmQuery::new("DROP TABLE foo"));
        assert!(result.is_err());
    }

    #[test]
    fn test_select_missing_table_returns_empty() {
        let db = WasmDatabase::new();
        let rows = db
            .query(WasmQuery::new("SELECT * FROM nonexistent"))
            .unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn test_query_with_trailing_semicolon() {
        let db = WasmDatabase::new();
        db.execute(WasmQuery::new("CREATE TABLE users (id INTEGER);"))
            .unwrap();
        db.execute(WasmQuery::with_params(
            "INSERT INTO users (id) VALUES (?);",
            vec![json!(42)],
        ))
        .unwrap();

        let rows = db.query(WasmQuery::new("SELECT * FROM users;")).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["id"], json!(42));
    }

    #[test]
    fn test_default_implementation() {
        let db = WasmDatabase::default();
        let rows = db.query(WasmQuery::new("SELECT * FROM any_table")).unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn test_wasm_query_new() {
        let q = WasmQuery::new("SELECT 1");
        assert_eq!(q.sql, "SELECT 1");
        assert!(q.params.is_empty());
    }

    #[test]
    fn test_wasm_query_with_params() {
        let q = WasmQuery::with_params("SELECT * FROM t WHERE id = ?", vec![json!(5)]);
        assert_eq!(q.sql, "SELECT * FROM t WHERE id = ?");
        assert_eq!(q.params.len(), 1);
    }

    #[test]
    fn test_insert_value_count_mismatch_returns_error() {
        let db = WasmDatabase::new();
        db.execute(WasmQuery::new("CREATE TABLE t (a INTEGER, b INTEGER)"))
            .unwrap();
        // 列数 2 但只提供 1 个值
        let result = db.execute(WasmQuery::with_params(
            "INSERT INTO t (a, b) VALUES (?, ?), (?)",
            vec![json!(1), json!(2), json!(3)],
        ));
        assert!(result.is_err());
    }

    #[test]
    fn test_missing_param_returns_error() {
        let db = WasmDatabase::new();
        db.execute(WasmQuery::new("CREATE TABLE t (a INTEGER, b INTEGER)"))
            .unwrap();
        // 需要 2 个参数但只提供 1 个
        let result = db.execute(WasmQuery::with_params(
            "INSERT INTO t (a, b) VALUES (?, ?)",
            vec![json!(1)],
        ));
        assert!(result.is_err());
    }
}
