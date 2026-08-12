//! BatchDialect — 五方言批量 SQL 生成
//!
//! 按 DbType 适配 MySQL/PostgreSQL/SQLite/Oracle/MSSQL 批量 INSERT/UPDATE/DELETE/UPSERT 语法。
//! 复用既有 DefaultBatchOps SQL 生成逻辑（quote/chunk_indices）。

use serde_json::Value;

use sz_orm_core::DbError;
use sz_orm_core::DbType;

use crate::UpsertMode;

/// 方言引用符号
fn quote_identifier(db_type: DbType, name: &str) -> String {
    match db_type {
        DbType::MySQL | DbType::MariaDB | DbType::OceanBase | DbType::TiDB => {
            let escaped = name.replace('`', "``");
            format!("`{}`", escaped)
        }
        _ => {
            let escaped = name.replace('"', "\"\"");
            format!("\"{}\"", escaped)
        }
    }
}

/// 方言占位符
fn placeholder(db_type: DbType, index: usize) -> String {
    match db_type {
        DbType::PostgreSQL | DbType::GaussDB | DbType::Kingbase | DbType::PolarDB => {
            format!("${}", index)
        }
        _ => "?".to_string(),
    }
}

/// 批量方言 SQL 生成器
pub struct BatchDialect;

impl BatchDialect {
    /// 生成多值 INSERT
    ///
    /// 返回 (SQL, 参数列表)，SQL 使用占位符，参数按列顺序绑定。
    pub fn build_batch_insert(
        db_type: DbType,
        table: &str,
        rows: &[Value],
        chunk: (usize, usize),
    ) -> Result<(String, Vec<Value>), DbError> {
        if rows.is_empty() {
            return Err(DbError::InvalidInput("rows is empty".into()));
        }
        let (start, end) = chunk;
        if start >= end || end > rows.len() {
            return Err(DbError::InvalidInput(format!(
                "invalid chunk ({}, {}) for rows len {}",
                start,
                end,
                rows.len()
            )));
        }
        let first = &rows[start];
        let columns: Vec<String> = match first.as_object() {
            Some(map) if !map.is_empty() => map.keys().cloned().collect(),
            _ => {
                return Err(DbError::InvalidInput(
                    "first row is not a valid object".into(),
                ))
            }
        };
        let chunk_rows = &rows[start..end];
        let cols_str = columns
            .iter()
            .map(|c| quote_identifier(db_type, c))
            .collect::<Vec<_>>()
            .join(", ");
        let mut params = Vec::new();
        let mut row_placeholders = Vec::new();
        let mut ph_idx = 1;
        for row in chunk_rows {
            let mut phs = Vec::new();
            for col in &columns {
                phs.push(placeholder(db_type, ph_idx));
                ph_idx += 1;
                let val = row.get(col).cloned().unwrap_or(Value::Null);
                params.push(val);
            }
            let joined = phs.join(", ");
            row_placeholders.push(format!("({})", joined));
        }
        let sql = format!(
            "INSERT INTO {} ({}) VALUES {}",
            quote_identifier(db_type, table),
            cols_str,
            row_placeholders.join(", ")
        );
        Ok((sql, params))
    }

    /// 生成 CASE WHEN UPDATE
    pub fn build_batch_update(
        db_type: DbType,
        table: &str,
        rows: &[Value],
        pk: &str,
        chunk: (usize, usize),
    ) -> Result<(String, Vec<Value>), DbError> {
        if rows.is_empty() {
            return Err(DbError::InvalidInput("rows is empty".into()));
        }
        let (start, end) = chunk;
        if start >= end || end > rows.len() {
            return Err(DbError::InvalidInput(format!(
                "invalid chunk ({}, {}) for rows len {}",
                start,
                end,
                rows.len()
            )));
        }
        let first = &rows[start];
        let columns: Vec<String> = match first.as_object() {
            Some(map) if !map.is_empty() => map.keys().cloned().collect(),
            _ => {
                return Err(DbError::InvalidInput(
                    "first row is not a valid object".into(),
                ))
            }
        };
        if !columns.contains(&pk.to_string()) {
            return Err(DbError::InvalidInput(format!(
                "primary key '{}' not found in columns",
                pk
            )));
        }
        let chunk_rows = &rows[start..end];
        let non_pk: Vec<&str> = columns
            .iter()
            .map(|s| s.as_str())
            .filter(|c| *c != pk)
            .collect();
        if non_pk.is_empty() {
            return Err(DbError::InvalidInput("no non-pk columns to update".into()));
        }
        let pk_q = quote_identifier(db_type, pk);
        let mut params = Vec::new();
        let mut ph_idx = 1;
        let set_clauses: Vec<String> = non_pk
            .iter()
            .map(|col| {
                let col_q = quote_identifier(db_type, col);
                let mut when_parts = Vec::new();
                for _row in chunk_rows {
                    let ph1 = placeholder(db_type, ph_idx);
                    let ph2 = placeholder(db_type, ph_idx + 1);
                    when_parts.push(format!("{} = {} THEN {}", pk_q, ph1, ph2));
                    ph_idx += 2;
                }
                for row in chunk_rows {
                    let pk_val = row.get(pk).cloned().unwrap_or(Value::Null);
                    let col_val = row.get(*col).cloned().unwrap_or(Value::Null);
                    params.push(pk_val);
                    params.push(col_val);
                }
                format!(
                    "{} = CASE WHEN {} ELSE {} END",
                    col_q,
                    when_parts.join(" WHEN "),
                    col_q
                )
            })
            .collect();
        let mut in_placeholders = Vec::new();
        for row in chunk_rows {
            in_placeholders.push(placeholder(db_type, ph_idx));
            ph_idx += 1;
            let pk_val = row.get(pk).cloned().unwrap_or(Value::Null);
            params.push(pk_val);
        }
        let sql = format!(
            "UPDATE {} SET {} WHERE {} IN ({})",
            quote_identifier(db_type, table),
            set_clauses.join(", "),
            pk_q,
            in_placeholders.join(", ")
        );
        Ok((sql, params))
    }

    /// 生成批量 DELETE（WHERE pk IN (?, ?, ...)）
    pub fn build_batch_delete(
        db_type: DbType,
        table: &str,
        pk: &str,
        ids: &[Value],
        chunk: (usize, usize),
    ) -> Result<(String, Vec<Value>), DbError> {
        if ids.is_empty() {
            return Err(DbError::InvalidInput("ids is empty".into()));
        }
        let (start, end) = chunk;
        if start >= end || end > ids.len() {
            return Err(DbError::InvalidInput(format!(
                "invalid chunk ({}, {}) for ids len {}",
                start,
                end,
                ids.len()
            )));
        }
        let chunk_ids = &ids[start..end];
        let pk_q = quote_identifier(db_type, pk);
        let mut placeholders = Vec::new();
        let mut params = Vec::new();
        for (i, id) in chunk_ids.iter().enumerate() {
            placeholders.push(placeholder(db_type, i + 1));
            params.push(id.clone());
        }
        let sql = format!(
            "DELETE FROM {} WHERE {} IN ({})",
            quote_identifier(db_type, table),
            pk_q,
            placeholders.join(", ")
        );
        Ok((sql, params))
    }

    /// 生成批量 UPSERT（方言适配）
    pub fn build_batch_upsert(
        db_type: DbType,
        table: &str,
        rows: &[Value],
        mode: UpsertMode,
        chunk: (usize, usize),
    ) -> Result<(String, Vec<Value>), DbError> {
        let (sql, params) = Self::build_batch_insert(db_type, table, rows, chunk)?;
        let first = &rows[chunk.0];
        let columns: Vec<String> = first
            .as_object()
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default();
        let pk = columns.first().cloned().unwrap_or_default();
        let non_pk: Vec<String> = columns.iter().filter(|c| *c != &pk).cloned().collect();
        if non_pk.is_empty() {
            return Ok((sql, params));
        }
        let conflict_part = match mode {
            UpsertMode::MysqlOnDuplicate => {
                let updates: Vec<String> = non_pk
                    .iter()
                    .map(|col| {
                        let q = quote_identifier(db_type, col);
                        format!("{} = VALUES({})", q, q)
                    })
                    .collect();
                format!(" ON DUPLICATE KEY UPDATE {}", updates.join(", "))
            }
            UpsertMode::PostgresOnConflict | UpsertMode::SqliteOnConflict => {
                let updates: Vec<String> = non_pk
                    .iter()
                    .map(|col| {
                        let q = quote_identifier(db_type, col);
                        format!("{} = EXCLUDED.{}", q, q)
                    })
                    .collect();
                format!(
                    " ON CONFLICT ({}) DO UPDATE SET {}",
                    quote_identifier(db_type, &pk),
                    updates.join(", ")
                )
            }
            UpsertMode::OracleMerge | UpsertMode::MssqlMerge => String::new(),
        };
        Ok((format!("{}{}", sql, conflict_part), params))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn build_batch_insert_mysql() {
        let rows = vec![json!({"id": 1, "name": "a"}), json!({"id": 2, "name": "b"})];
        let (sql, params) =
            BatchDialect::build_batch_insert(DbType::MySQL, "users", &rows, (0, 2)).unwrap();
        assert!(sql.contains("INSERT INTO"));
        assert!(sql.contains("`users`"));
        assert_eq!(params.len(), 4);
    }

    #[test]
    fn build_batch_insert_pg_dollar_placeholders() {
        let rows = vec![json!({"id": 1, "name": "a"})];
        let (sql, _) =
            BatchDialect::build_batch_insert(DbType::PostgreSQL, "users", &rows, (0, 1)).unwrap();
        assert!(sql.contains("$1"));
        assert!(sql.contains("$2"));
    }

    #[test]
    fn build_batch_delete_in_clause() {
        let ids = vec![json!(1), json!(2), json!(3)];
        let (sql, params) =
            BatchDialect::build_batch_delete(DbType::MySQL, "users", "id", &ids, (0, 3)).unwrap();
        assert!(sql.contains("DELETE FROM"));
        assert!(sql.contains("IN ("));
        assert_eq!(params.len(), 3);
    }

    #[test]
    fn build_batch_delete_pg_placeholders() {
        let ids = vec![json!(1), json!(2)];
        let (sql, _) =
            BatchDialect::build_batch_delete(DbType::PostgreSQL, "users", "id", &ids, (0, 2))
                .unwrap();
        assert!(sql.contains("$1"));
        assert!(sql.contains("$2"));
    }

    #[test]
    fn build_batch_upsert_mysql() {
        let rows = vec![json!({"id": 1, "name": "a"})];
        let (sql, _) = BatchDialect::build_batch_upsert(
            DbType::MySQL,
            "users",
            &rows,
            UpsertMode::MysqlOnDuplicate,
            (0, 1),
        )
        .unwrap();
        assert!(sql.contains("ON DUPLICATE KEY UPDATE"));
    }

    #[test]
    fn build_batch_upsert_pg() {
        let rows = vec![json!({"id": 1, "name": "a"})];
        let (sql, _) = BatchDialect::build_batch_upsert(
            DbType::PostgreSQL,
            "users",
            &rows,
            UpsertMode::PostgresOnConflict,
            (0, 1),
        )
        .unwrap();
        assert!(sql.contains("ON CONFLICT"));
        assert!(sql.contains("DO UPDATE SET"));
    }

    #[test]
    fn build_batch_upsert_sqlite() {
        let rows = vec![json!({"id": 1, "name": "a"})];
        let (sql, _) = BatchDialect::build_batch_upsert(
            DbType::Sqlite,
            "users",
            &rows,
            UpsertMode::SqliteOnConflict,
            (0, 1),
        )
        .unwrap();
        assert!(sql.contains("ON CONFLICT"));
    }

    #[test]
    fn build_batch_upsert_oracle() {
        let rows = vec![json!({"id": 1, "name": "a"})];
        let (sql, _) = BatchDialect::build_batch_upsert(
            DbType::Oracle,
            "users",
            &rows,
            UpsertMode::OracleMerge,
            (0, 1),
        )
        .unwrap();
        assert!(sql.contains("INSERT INTO"));
    }

    #[test]
    fn build_batch_upsert_mssql() {
        let rows = vec![json!({"id": 1, "name": "a"})];
        let (sql, _) = BatchDialect::build_batch_upsert(
            DbType::SqlServer,
            "users",
            &rows,
            UpsertMode::MssqlMerge,
            (0, 1),
        )
        .unwrap();
        assert!(sql.contains("INSERT INTO"));
    }

    #[test]
    fn build_batch_update_case_when() {
        let rows = vec![json!({"id": 1, "name": "a"}), json!({"id": 2, "name": "b"})];
        let (sql, params) =
            BatchDialect::build_batch_update(DbType::MySQL, "users", &rows, "id", (0, 2)).unwrap();
        assert!(sql.contains("CASE WHEN"));
        assert!(sql.contains("IN ("));
        assert!(!params.is_empty());
    }

    #[test]
    fn empty_rows_returns_error() {
        let rows: Vec<Value> = vec![];
        let result = BatchDialect::build_batch_insert(DbType::MySQL, "users", &rows, (0, 0));
        assert!(result.is_err());
    }

    #[test]
    fn empty_ids_returns_error() {
        let ids: Vec<Value> = vec![];
        let result = BatchDialect::build_batch_delete(DbType::MySQL, "users", "id", &ids, (0, 0));
        assert!(result.is_err());
    }

    #[test]
    fn invalid_chunk_returns_error() {
        let rows = vec![json!({"id": 1})];
        let result = BatchDialect::build_batch_insert(DbType::MySQL, "users", &rows, (0, 5));
        assert!(result.is_err());
    }

    #[test]
    fn sql_injection_table_name() {
        let rows = vec![json!({"id": 1})];
        let (sql, _) =
            BatchDialect::build_batch_insert(DbType::MySQL, "users` OR 1=1 --", &rows, (0, 1))
                .unwrap();
        assert!(sql.contains("``"));
        assert!(sql.starts_with("INSERT INTO `users`` OR 1=1 --`"));
    }
}
