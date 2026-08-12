//! PostgreSQL COPY 协议批量导入
//!
//! `COPY table FROM STDIN` 比 multi-value INSERT 性能更高（跳过 SQL 解析）。
//! 仅 PostgreSQL 方言支持，其他方言降级为多值 INSERT。

use serde_json::Value;

use sz_orm_core::DbError;
use sz_orm_core::DbType;

use crate::BatchResult;

/// COPY 协议执行器
pub struct CopyProtocolExecutor {
    db_type: DbType,
}

impl CopyProtocolExecutor {
    pub fn new(db_type: DbType) -> Self {
        Self { db_type }
    }

    pub fn db_type(&self) -> DbType {
        self.db_type
    }

    /// 是否支持 COPY 协议
    pub fn is_supported(&self) -> bool {
        matches!(
            self.db_type,
            DbType::PostgreSQL | DbType::GaussDB | DbType::Kingbase | DbType::PolarDB
        )
    }

    /// 生成 COPY SQL
    ///
    /// 仅 PostgreSQL 方言支持，其他方言返回错误。
    pub fn build_copy_sql(&self, table: &str, columns: &[String]) -> Result<String, DbError> {
        if !self.is_supported() {
            return Err(DbError::Unsupported(format!(
                "COPY protocol not supported for {:?}, fallback to multi-value INSERT",
                self.db_type
            )));
        }
        if columns.is_empty() {
            return Err(DbError::InvalidInput("columns is empty".into()));
        }
        let cols = columns
            .iter()
            .map(|c| format!("\"{}\"", c.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(", ");
        Ok(format!(
            "COPY {table} ({cols}) FROM STDIN WITH (FORMAT csv)"
        ))
    }

    /// 生成 CSV 数据行
    pub fn build_csv_rows(rows: &[Value], columns: &[String]) -> String {
        let mut csv = String::new();
        for row in rows {
            let values: Vec<String> = columns
                .iter()
                .map(|col| {
                    let val = row.get(col).unwrap_or(&Value::Null);
                    match val {
                        Value::Null => String::new(),
                        Value::String(s) => {
                            if s.contains(',') || s.contains('"') || s.contains('\n') {
                                format!("\"{}\"", s.replace('"', "\"\""))
                            } else {
                                s.clone()
                            }
                        }
                        other => other.to_string(),
                    }
                })
                .collect();
            csv.push_str(&values.join(","));
            csv.push('\n');
        }
        csv
    }

    /// 执行 COPY 导入（生成 SQL + CSV 数据）
    ///
    /// 返回 (COPY SQL, CSV 数据)。调用方负责通过 Connection 发送 COPY 数据。
    pub fn execute_copy(
        &self,
        table: &str,
        rows: &[Value],
    ) -> Result<(String, String, BatchResult), DbError> {
        if rows.is_empty() {
            return Ok((String::new(), String::new(), BatchResult::new()));
        }
        if !self.is_supported() {
            return Err(DbError::Unsupported(format!(
                "COPY not supported for {:?}, fallback to multi-value INSERT",
                self.db_type
            )));
        }
        let first = &rows[0];
        let columns: Vec<String> = first
            .as_object()
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default();
        if columns.is_empty() {
            return Err(DbError::InvalidInput("first row has no columns".into()));
        }
        let copy_sql = self.build_copy_sql(table, &columns)?;
        let csv_data = Self::build_csv_rows(rows, &columns);
        let result = BatchResult {
            inserted: rows.len(),
            updated: 0,
            failed: 0,
            generated_sqls: vec![copy_sql.clone()],
        };
        Ok((copy_sql, csv_data, result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn copy_supported_pg() {
        let exec = CopyProtocolExecutor::new(DbType::PostgreSQL);
        assert!(exec.is_supported());
    }

    #[test]
    fn copy_not_supported_mysql() {
        let exec = CopyProtocolExecutor::new(DbType::MySQL);
        assert!(!exec.is_supported());
    }

    #[test]
    fn copy_not_supported_sqlite() {
        let exec = CopyProtocolExecutor::new(DbType::Sqlite);
        assert!(!exec.is_supported());
    }

    #[test]
    fn build_copy_sql_pg() {
        let exec = CopyProtocolExecutor::new(DbType::PostgreSQL);
        let sql = exec
            .build_copy_sql("users", &["id".into(), "name".into()])
            .unwrap();
        assert!(sql.contains("COPY"));
        assert!(sql.contains("FROM STDIN"));
        assert!(sql.contains("FORMAT csv"));
    }

    #[test]
    fn build_copy_sql_mysql_fails() {
        let exec = CopyProtocolExecutor::new(DbType::MySQL);
        let result = exec.build_copy_sql("users", &["id".into()]);
        assert!(result.is_err());
    }

    #[test]
    fn build_csv_basic() {
        let rows = vec![
            json!({"id": 1, "name": "Alice"}),
            json!({"id": 2, "name": "Bob"}),
        ];
        let csv = CopyProtocolExecutor::build_csv_rows(&rows, &["id".into(), "name".into()]);
        assert!(csv.contains("1,Alice"));
        assert!(csv.contains("2,Bob"));
    }

    #[test]
    fn build_csv_with_comma() {
        let rows = vec![json!({"id": 1, "name": "Alice, Jr."})];
        let csv = CopyProtocolExecutor::build_csv_rows(&rows, &["id".into(), "name".into()]);
        assert!(csv.contains("\"Alice, Jr.\""));
    }

    #[test]
    fn build_csv_with_quote() {
        let rows = vec![json!({"id": 1, "name": "Alice \"Bob\""})];
        let csv = CopyProtocolExecutor::build_csv_rows(&rows, &["id".into(), "name".into()]);
        assert!(csv.contains("\"Alice \"\"Bob\"\"\""));
    }

    #[test]
    fn build_csv_null_value() {
        let rows = vec![json!({"id": 1, "name": Value::Null})];
        let csv = CopyProtocolExecutor::build_csv_rows(&rows, &["id".into(), "name".into()]);
        assert!(csv.contains("1,"));
    }

    #[test]
    fn execute_copy_pg() {
        let exec = CopyProtocolExecutor::new(DbType::PostgreSQL);
        let rows = vec![json!({"id": 1, "name": "a"}), json!({"id": 2, "name": "b"})];
        let (sql, csv, result) = exec.execute_copy("users", &rows).unwrap();
        assert!(sql.contains("COPY"));
        assert!(!csv.is_empty());
        assert_eq!(result.inserted, 2);
    }

    #[test]
    fn execute_copy_mysql_fallback() {
        let exec = CopyProtocolExecutor::new(DbType::MySQL);
        let rows = vec![json!({"id": 1, "name": "a"})];
        let result = exec.execute_copy("users", &rows);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("fallback"));
    }

    #[test]
    fn execute_copy_empty_rows() {
        let exec = CopyProtocolExecutor::new(DbType::PostgreSQL);
        let (sql, csv, result) = exec.execute_copy("users", &[]).unwrap();
        assert!(sql.is_empty());
        assert!(csv.is_empty());
        assert_eq!(result.inserted, 0);
    }

    #[test]
    fn copy_supported_gaussdb() {
        let exec = CopyProtocolExecutor::new(DbType::GaussDB);
        assert!(exec.is_supported());
    }
}
